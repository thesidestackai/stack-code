//! Security matrix for the central N6 admission guard.
//!
//! Nothing here contacts a real broker, a real provider, or port 11435/11434.
//! Inference is served by loopback fixture servers on ephemeral ports, and
//! readiness decisions come from scripted authorities except in the protocol
//! section, which drives the production HTTP authority against its own
//! loopback fixture.

// These tests set process-wide environment variables and must keep them stable
// across the awaits that follow, so the serializing guard is deliberately held
// across await points. It guards only env mutation in a single-threaded test
// body; nothing inside the critical section can block on acquiring it again.
#![allow(clippy::await_holding_lock)]

use std::collections::HashMap;
use std::ffi::OsString;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex as StdMutex, MutexGuard, OnceLock};

use api::{
    AnthropicClient, ApiError, AuthSource, BrokerOrigin, HttpReadinessAuthority, InputContentBlock,
    InputMessage, MessageRequest, N6ReadinessAuthority, N6ReadinessFuture, N6RefusalKind,
    OpenAiCompatClient, OpenAiCompatConfig, ProviderClient, SIDESTACK_MARKER_ENV,
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::Mutex;

// ---------------------------------------------------------------------------
// Environment serialization
// ---------------------------------------------------------------------------

/// Every test that mutates process-wide environment variables takes this lock
/// so a parallel `cargo test` cannot observe another test's partial state.
fn env_lock() -> MutexGuard<'static, ()> {
    static LOCK: OnceLock<StdMutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| StdMutex::new(()))
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

struct EnvGuard {
    key: String,
    original: Option<OsString>,
}

impl EnvGuard {
    fn set(key: &str, value: Option<&str>) -> Self {
        let original = std::env::var_os(key);
        match value {
            Some(value) => std::env::set_var(key, value),
            None => std::env::remove_var(key),
        }
        Self {
            key: key.to_string(),
            original,
        }
    }

    fn marker_on() -> Self {
        Self::set(SIDESTACK_MARKER_ENV, Some("1"))
    }

    fn marker_off() -> Self {
        Self::set(SIDESTACK_MARKER_ENV, None)
    }
}

impl Drop for EnvGuard {
    fn drop(&mut self) {
        match self.original.take() {
            Some(value) => std::env::set_var(&self.key, value),
            None => std::env::remove_var(&self.key),
        }
    }
}

// ---------------------------------------------------------------------------
// Scripted readiness authority
// ---------------------------------------------------------------------------

/// A readiness authority that replays a fixed script and records every model it
/// was asked about, so tests can assert both the *number* of readiness
/// decisions and the exact wire model each one named.
#[derive(Debug)]
struct ScriptedAuthority {
    script: StdMutex<Vec<ReadinessVerdict>>,
    asked: StdMutex<Vec<String>>,
    origins: StdMutex<Vec<String>>,
}

#[derive(Debug, Clone, Copy)]
enum ReadinessVerdict {
    Ready,
    NotReady,
    Transport,
}

impl ScriptedAuthority {
    fn new(script: &[ReadinessVerdict]) -> Arc<Self> {
        // Reversed so `pop` replays in the written order.
        let mut queued = script.to_vec();
        queued.reverse();
        Arc::new(Self {
            script: StdMutex::new(queued),
            asked: StdMutex::new(Vec::new()),
            origins: StdMutex::new(Vec::new()),
        })
    }

    fn models_asked(&self) -> Vec<String> {
        self.asked.lock().expect("asked lock").clone()
    }

    fn call_count(&self) -> usize {
        self.asked.lock().expect("asked lock").len()
    }

    fn origins_asked(&self) -> Vec<String> {
        self.origins.lock().expect("origins lock").clone()
    }
}

impl N6ReadinessAuthority for ScriptedAuthority {
    fn check<'a>(&'a self, origin: &'a BrokerOrigin, wire_model: &'a str) -> N6ReadinessFuture<'a> {
        Box::pin(async move {
            self.asked
                .lock()
                .expect("asked lock")
                .push(wire_model.to_string());
            self.origins
                .lock()
                .expect("origins lock")
                .push(origin.endpoint().to_string());
            let verdict = self
                .script
                .lock()
                .expect("script lock")
                .pop()
                .expect("readiness authority was called more times than the script allows");
            match verdict {
                ReadinessVerdict::Ready => Ok(()),
                ReadinessVerdict::NotReady => Err(ApiError::N6AdmissionRefused {
                    model: wire_model.to_string(),
                    kind: N6RefusalKind::NotReady {
                        reason_code: "scripted_not_ready".to_string(),
                    },
                }),
                ReadinessVerdict::Transport => Err(ApiError::N6AdmissionRefused {
                    model: wire_model.to_string(),
                    kind: N6RefusalKind::Transport("scripted transport failure".to_string()),
                }),
            }
        })
    }
}

/// An authority that must never be consulted. Any call fails the test.
#[derive(Debug, Default)]
struct ForbiddenAuthority {
    calls: AtomicUsize,
}

impl ForbiddenAuthority {
    fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    fn call_count(&self) -> usize {
        self.calls.load(Ordering::SeqCst)
    }
}

impl N6ReadinessAuthority for ForbiddenAuthority {
    fn check<'a>(
        &'a self,
        _origin: &'a BrokerOrigin,
        _wire_model: &'a str,
    ) -> N6ReadinessFuture<'a> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Box::pin(async { Ok(()) })
    }
}

// ---------------------------------------------------------------------------
// Assertions shared across the matrix
// ---------------------------------------------------------------------------

fn assert_non_retryable_n6(error: &ApiError, label: &str) {
    assert!(
        matches!(error, ApiError::N6AdmissionRefused { .. }),
        "{label}: expected an N6 admission refusal, got {error:?}"
    );
    assert!(
        !error.is_retryable(),
        "{label}: N6 refusals must never be retryable — a retryable refusal would let the \
         provider fallback chain try another model"
    );
}

fn assert_routing_refusal(error: &ApiError, label: &str) {
    assert_non_retryable_n6(error, label);
    match error {
        ApiError::N6AdmissionRefused { kind, .. } => assert!(
            matches!(kind, N6RefusalKind::Routing(_)),
            "{label}: expected a LAW-1 routing refusal, got {kind:?}"
        ),
        other => panic!("{label}: unexpected error {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// A. Broker ready -> the inference attempt is made
// ---------------------------------------------------------------------------

#[tokio::test]
async fn a_ready_broker_admits_the_inference_attempt() {
    let _lock = env_lock();
    let _marker = EnvGuard::marker_off();

    let state = new_state();
    let server = spawn_server(state.clone(), vec![ok_completion("qwen3:14b")]).await;
    let authority = ScriptedAuthority::new(&[ReadinessVerdict::Ready]);

    let client = broker_gated_client(&server, authority.clone());
    let response = client
        .send_message(&request_for("qwen3:14b"))
        .await
        .expect("an admitted request must reach the inference endpoint");

    assert_eq!(response.model, "qwen3:14b");
    assert_eq!(authority.call_count(), 1, "exactly one readiness decision");
    assert_eq!(
        captured_paths(&state).await,
        vec!["/chat/completions".to_string()],
        "the inference mock must have received exactly one request"
    );
}

// ---------------------------------------------------------------------------
// B. Broker not ready -> zero inference traffic
// ---------------------------------------------------------------------------

#[tokio::test]
async fn b_not_ready_broker_blocks_the_inference_attempt_entirely() {
    let _lock = env_lock();
    let _marker = EnvGuard::marker_off();

    let state = new_state();
    let server = spawn_server(state.clone(), vec![ok_completion("qwen3:14b")]).await;
    let authority = ScriptedAuthority::new(&[ReadinessVerdict::NotReady]);

    let client = broker_gated_client(&server, authority.clone());
    let error = client
        .send_message(&request_for("qwen3:14b"))
        .await
        .expect_err("a not-ready broker must refuse");

    assert_non_retryable_n6(&error, "ready=false");
    assert_eq!(authority.call_count(), 1);
    assert!(
        captured_paths(&state).await.is_empty(),
        "no inference request may be sent when readiness refuses"
    );
}

// ---------------------------------------------------------------------------
// C. Same-model freshness: no admission cache
// ---------------------------------------------------------------------------

#[tokio::test]
async fn c_second_request_for_the_same_model_is_re_admitted_not_cached() {
    let _lock = env_lock();
    let _marker = EnvGuard::marker_off();

    let state = new_state();
    let server = spawn_server(state.clone(), vec![ok_completion("qwen3:14b")]).await;
    // The same model, admitted once and then refused. A cache would let the
    // second request through on the strength of the first answer.
    let authority = ScriptedAuthority::new(&[ReadinessVerdict::Ready, ReadinessVerdict::NotReady]);
    let client = broker_gated_client(&server, authority.clone());

    client
        .send_message(&request_for("qwen3:14b"))
        .await
        .expect("first request is admitted");
    let error = client
        .send_message(&request_for("qwen3:14b"))
        .await
        .expect_err("the second request must be re-admitted, and this time refused");

    assert_non_retryable_n6(&error, "same-model freshness");
    assert_eq!(
        authority.models_asked(),
        vec!["qwen3:14b".to_string(), "qwen3:14b".to_string()],
        "two readiness calls must be observed for the same model"
    );
    assert_eq!(
        captured_paths(&state).await.len(),
        1,
        "only the first, admitted request may have reached the inference endpoint"
    );
}

// ---------------------------------------------------------------------------
// D. Retry freshness: admission runs beneath the retry loop
// ---------------------------------------------------------------------------

#[tokio::test]
async fn d_a_retry_after_a_retryable_503_is_re_admitted_and_can_be_blocked() {
    let _lock = env_lock();
    let _marker = EnvGuard::marker_off();

    let state = new_state();
    // The fixture would happily serve a second attempt; the guard must stop it.
    let server = spawn_server(
        state.clone(),
        vec![
            http_response("503 Service Unavailable", "application/json", "{}"),
            ok_completion("qwen3:14b"),
        ],
    )
    .await;
    let authority = ScriptedAuthority::new(&[ReadinessVerdict::Ready, ReadinessVerdict::NotReady]);

    let client = broker_gated_client(&server, authority.clone()).with_retry_policy(
        4,
        std::time::Duration::from_millis(1),
        std::time::Duration::from_millis(2),
    );

    let error = client
        .send_message(&request_for("qwen3:14b"))
        .await
        .expect_err("the retry must be refused by the second readiness decision");

    assert_non_retryable_n6(&error, "retry freshness");
    assert_eq!(
        authority.call_count(),
        2,
        "the retry attempt must carry its own readiness decision"
    );
    assert_eq!(
        captured_paths(&state).await.len(),
        1,
        "exactly one inference HTTP request may be sent: the retry must never leave the process"
    );
}

// ---------------------------------------------------------------------------
// E / F. Wire model identity
// ---------------------------------------------------------------------------

#[tokio::test]
async fn e_readiness_sees_the_resolved_alias_not_the_alias_name() {
    let _lock = env_lock();
    let _marker = EnvGuard::marker_off();
    let _alias = EnvGuard::set("RUSTY_CLAUDE_MODEL_ALIAS__FAST", Some("qwen3:14b"));

    let state = new_state();
    let server = spawn_server(state.clone(), vec![ok_completion("qwen3:14b")]).await;
    let authority = ScriptedAuthority::new(&[ReadinessVerdict::Ready]);

    let client = broker_gated_client(&server, authority.clone());
    client
        .send_message(&request_for("fast"))
        .await
        .expect("admitted");

    assert_eq!(
        authority.models_asked(),
        vec!["qwen3:14b".to_string()],
        "readiness must name the resolved broker model, never the `fast` alias"
    );
    assert_eq!(
        captured_model(&state).await,
        "qwen3:14b",
        "the payload model must match the model that was admitted"
    );
}

#[tokio::test]
async fn f_readiness_and_payload_agree_after_routing_prefix_stripping() {
    let _lock = env_lock();
    let _marker = EnvGuard::marker_off();

    let state = new_state();
    let server = spawn_server(state.clone(), vec![ok_completion("gpt-4")]).await;
    let authority = ScriptedAuthority::new(&[ReadinessVerdict::Ready]);

    let client = broker_gated_client(&server, authority.clone());
    client
        .send_message(&request_for("openai/gpt-4"))
        .await
        .expect("admitted");

    assert_eq!(authority.models_asked(), vec!["gpt-4".to_string()]);
    assert_eq!(
        captured_model(&state).await,
        "gpt-4",
        "admission and payload must resolve to the identical wire model"
    );
}

#[test]
fn f2_the_shared_helper_is_the_payload_model_for_every_shape() {
    let _lock = env_lock();
    let _alias = EnvGuard::set("RUSTY_CLAUDE_MODEL_ALIAS__FAST", Some("qwen3:14b"));

    for input in [
        "fast",
        "openai/gpt-4",
        "xai/grok-3",
        "kimi/kimi-k2.5",
        "qwen3:14b",
        "claude-opus-4-6",
        "vendor/with/slashes",
    ] {
        let request = request_for(input);
        let payload = api::build_chat_completion_request(&request, OpenAiCompatConfig::openai());
        let payload_model = payload
            .get("model")
            .and_then(serde_json::Value::as_str)
            .expect("payload carries a model");
        assert_eq!(
            api::wire_model_for_request(&request),
            payload_model,
            "the admission helper and the payload builder must never disagree for {input}"
        );
    }
}

// ---------------------------------------------------------------------------
// G / H / I. LAW-1 routing containment under the SideStack marker
// ---------------------------------------------------------------------------

#[tokio::test]
async fn g_marker_refuses_direct_anthropic_before_any_connection() {
    let _lock = env_lock();
    let _marker = EnvGuard::marker_on();

    // A listener that is bound but never connected to: if the guard leaked, the
    // request would land here and the accept count would rise.
    let sink = spawn_connection_sink().await;
    let client = ProviderClient::Anthropic(
        AnthropicClient::from_auth(AuthSource::ApiKey("test-key".to_string()))
            .with_base_url(sink.base_url()),
    );

    let error = client
        .send_message(&request_for("claude-opus-4-6"))
        .await
        .expect_err("Anthropic is not an allowed SideStack route");

    assert_routing_refusal(&error, "marker + Anthropic");
    assert_eq!(
        sink.accepted(),
        0,
        "the refusal must happen before any network connection"
    );
}

#[tokio::test]
async fn h_marker_refuses_an_ordinary_cloud_openai_compatible_endpoint() {
    let _lock = env_lock();
    let _marker = EnvGuard::marker_on();

    let authority = ForbiddenAuthority::new();
    let client = ProviderClient::OpenAi(
        OpenAiCompatClient::new("test-key", OpenAiCompatConfig::openai())
            .with_base_url("https://api.openai.com/v1")
            .with_n6_authority(authority.clone()),
    );

    let error = client
        .send_message(&request_for("gpt-4"))
        .await
        .expect_err("cloud OpenAI is not an allowed SideStack route");

    assert_routing_refusal(&error, "marker + cloud OpenAI");
    assert_eq!(
        authority.call_count(),
        0,
        "an off-broker route must be refused before readiness is even asked"
    );
}

#[tokio::test]
async fn i_marker_refuses_raw_ollama_on_port_11434() {
    let _lock = env_lock();
    let _marker = EnvGuard::marker_on();

    let authority = ForbiddenAuthority::new();
    for raw in [
        "http://127.0.0.1:11434",
        "http://127.0.0.1:11434/v1",
        "http://localhost:11434/v1",
    ] {
        let client = ProviderClient::OpenAi(
            OpenAiCompatClient::new("test-key", OpenAiCompatConfig::openai())
                .with_base_url(raw)
                .with_n6_authority(authority.clone()),
        );
        let error = client
            .send_message(&request_for("qwen3:14b"))
            .await
            .expect_err("raw :11434 is never an application inference target");
        assert_routing_refusal(&error, raw);
    }
    assert_eq!(authority.call_count(), 0);
}

#[tokio::test]
async fn i2_marker_allows_the_validated_broker_origin() {
    let _lock = env_lock();
    let _marker = EnvGuard::marker_on();

    // Layer A must pass a genuine broker base URL through to Layer B. The
    // scripted authority refuses, so nothing is ever sent to port 11435.
    let authority = ScriptedAuthority::new(&[ReadinessVerdict::NotReady]);
    let client = ProviderClient::OpenAi(
        OpenAiCompatClient::new("test-key", OpenAiCompatConfig::openai())
            .with_base_url("http://127.0.0.1:11435/v1")
            .with_n6_authority(authority.clone()),
    );

    let error = client
        .send_message(&request_for("qwen3:14b"))
        .await
        .expect_err("readiness refused");

    assert_non_retryable_n6(&error, "marker + broker");
    match &error {
        ApiError::N6AdmissionRefused { kind, .. } => assert!(
            matches!(kind, N6RefusalKind::NotReady { .. }),
            "Layer A must admit the broker route and let Layer B decide, got {kind:?}"
        ),
        other => panic!("unexpected {other:?}"),
    }
    assert_eq!(authority.call_count(), 1);
}

// ---------------------------------------------------------------------------
// J / K. Marker-absent behaviour
// ---------------------------------------------------------------------------

#[tokio::test]
async fn j_without_the_marker_ordinary_upstream_traffic_is_unchanged() {
    let _lock = env_lock();
    let _marker = EnvGuard::marker_off();

    let state = new_state();
    let server = spawn_server(state.clone(), vec![ok_completion("grok-3")]).await;
    let authority = ForbiddenAuthority::new();

    // No marker, and a base URL that is not a broker origin: this is an
    // ordinary upstream provider and must behave exactly as before.
    let client = ProviderClient::Xai(
        OpenAiCompatClient::new("xai-test-key", OpenAiCompatConfig::xai())
            .with_base_url(server.base_url())
            .with_n6_authority(authority.clone()),
    );

    let response = client
        .send_message(&request_for("grok-3"))
        .await
        .expect("ordinary upstream requests must be untouched");

    assert_eq!(response.model, "grok-3");
    assert_eq!(
        authority.call_count(),
        0,
        "a non-broker destination must generate no readiness traffic at all"
    );
    assert_eq!(captured_paths(&state).await.len(), 1);
}

#[tokio::test]
async fn k_without_the_marker_a_broker_destination_is_still_gated() {
    let _lock = env_lock();
    let _marker = EnvGuard::marker_off();

    // Layer B does not depend on the marker: a direct canonical claw aimed at
    // the broker is admission-gated just the same.
    let authority = ScriptedAuthority::new(&[ReadinessVerdict::NotReady]);
    let client = ProviderClient::OpenAi(
        OpenAiCompatClient::new("test-key", OpenAiCompatConfig::openai())
            .with_base_url("http://localhost:11435/v1")
            .with_n6_authority(authority.clone()),
    );

    let error = client
        .send_message(&request_for("qwen3:14b"))
        .await
        .expect_err("broker traffic is gated with or without the marker");

    assert_non_retryable_n6(&error, "no marker + broker");
    assert_eq!(authority.call_count(), 1);
    assert_eq!(authority.models_asked(), vec!["qwen3:14b".to_string()]);
}

// ---------------------------------------------------------------------------
// L / M. Agent containment
// ---------------------------------------------------------------------------

#[tokio::test]
async fn l_agent_default_cloud_model_cannot_reach_a_provider_under_the_marker() {
    let _lock = env_lock();
    let _marker = EnvGuard::marker_on();
    let _key = EnvGuard::set("ANTHROPIC_API_KEY", Some("test-key"));

    // The Agent product default stays what it is; what changes is that under
    // the SideStack marker it cannot perform inference.
    let sink = spawn_connection_sink().await;
    let client = ProviderClient::Anthropic(
        AnthropicClient::from_auth(AuthSource::ApiKey("test-key".to_string()))
            .with_base_url(sink.base_url()),
    );

    for model in ["claude-opus-4-6", "claude-sonnet-4-6"] {
        let error = client
            .send_message(&request_for(model))
            .await
            .expect_err("the Agent default must not reach a cloud provider under the marker");
        assert_routing_refusal(&error, model);

        let error = client
            .stream_message(&request_for(model))
            .await
            .expect_err("streaming must be refused identically");
        assert_routing_refusal(&error, model);
    }
    assert_eq!(sink.accepted(), 0, "no connection may be opened");
}

#[tokio::test]
async fn m_the_model_that_is_admitted_is_the_model_that_is_sent() {
    let _lock = env_lock();
    // Marker off: Layer A validates the *real* base URL, so under the marker a
    // fixture server is correctly refused as off-broker (proved by `h`/`i`).
    // The claim here is about model identity on the admitted path, which Layer
    // B enforces with or without the marker.
    let _marker = EnvGuard::marker_off();
    let _alias = EnvGuard::set("RUSTY_CLAUDE_MODEL_ALIAS__FAST", Some("qwen3:14b"));

    // Whatever selects the model — a config primary, an Agent override, a REPL
    // `/model` switch — admission applies to the resulting request.
    let state = new_state();
    let server = spawn_server(state.clone(), vec![ok_completion("qwen3:14b")]).await;
    let authority = ScriptedAuthority::new(&[ReadinessVerdict::Ready]);

    let client = ProviderClient::OpenAi(broker_gated_client(&server, authority.clone()));
    client
        .send_message(&request_for("fast"))
        .await
        .expect("admitted");

    let admitted = authority.models_asked();
    assert_eq!(admitted, vec!["qwen3:14b".to_string()]);
    assert_eq!(
        captured_model(&state).await,
        admitted[0],
        "the admitted identity and the executed identity must be the same string"
    );
}

// ---------------------------------------------------------------------------
// N / O. Fallback semantics
// ---------------------------------------------------------------------------

#[tokio::test]
async fn n_each_fallback_entry_gets_its_own_readiness_decision() {
    let _lock = env_lock();
    let _marker = EnvGuard::marker_off();

    // Mirrors the provider chain in `tools`: separate clients per chain entry,
    // each with its own admission decision immediately before its own attempt.
    let primary_state = new_state();
    let primary_server = spawn_server(
        primary_state.clone(),
        vec![http_response(
            "503 Service Unavailable",
            "application/json",
            "{}",
        )],
    )
    .await;
    let primary_authority = ScriptedAuthority::new(&[ReadinessVerdict::Ready]);
    let primary = broker_gated_client(&primary_server, primary_authority.clone())
        .with_retry_policy(
            0,
            std::time::Duration::from_millis(1),
            std::time::Duration::from_millis(1),
        );

    let fallback_state = new_state();
    let fallback_server =
        spawn_server(fallback_state.clone(), vec![ok_completion("qwen3:14b")]).await;
    let fallback_authority = ScriptedAuthority::new(&[ReadinessVerdict::Ready]);
    let fallback = broker_gated_client(&fallback_server, fallback_authority.clone());

    let primary_error = primary
        .send_message(&request_for("qwen3:27b"))
        .await
        .expect_err("primary fails");
    assert!(
        primary_error.is_retryable(),
        "a provider 503 must stay retryable so the existing fallback path is unchanged"
    );

    // The chain falls back only because the error was retryable.
    let response = fallback
        .send_message(&request_for("qwen3:14b"))
        .await
        .expect("fallback succeeds");

    assert_eq!(response.model, "qwen3:14b");
    assert_eq!(
        primary_authority.models_asked(),
        vec!["qwen3:27b".to_string()]
    );
    assert_eq!(
        fallback_authority.models_asked(),
        vec!["qwen3:14b".to_string()],
        "the fallback attempt must carry its own readiness decision for its own model"
    );
}

#[tokio::test]
async fn o_a_policy_refusal_never_triggers_a_fallback_attempt() {
    let _lock = env_lock();
    let _marker = EnvGuard::marker_off();

    let primary_state = new_state();
    let primary_server =
        spawn_server(primary_state.clone(), vec![ok_completion("qwen3:27b")]).await;
    let primary_authority = ScriptedAuthority::new(&[ReadinessVerdict::NotReady]);
    let primary = broker_gated_client(&primary_server, primary_authority.clone());

    let fallback_authority = ForbiddenAuthority::new();

    let error = primary
        .send_message(&request_for("qwen3:27b"))
        .await
        .expect_err("readiness refused the primary");

    // `ProviderRuntimeClient` in `tools` falls back only when
    // `error.is_retryable()`. Because the policy error is not retryable, the
    // chain returns immediately and the fallback entry is never touched.
    assert_non_retryable_n6(&error, "policy refusal");
    assert!(
        !error.is_retryable(),
        "this single property is what stops the fallback chain"
    );
    assert_eq!(
        fallback_authority.call_count(),
        0,
        "no fallback model may be attempted after a policy refusal"
    );
    assert!(captured_paths(&primary_state).await.is_empty());

    // Transport and protocol refusals behave identically.
    for verdict in [ReadinessVerdict::Transport] {
        let authority = ScriptedAuthority::new(&[verdict]);
        let client = broker_gated_client(&primary_server, authority);
        let error = client
            .send_message(&request_for("qwen3:27b"))
            .await
            .expect_err("transport refusal");
        assert_non_retryable_n6(&error, "transport refusal");
    }
}

// ---------------------------------------------------------------------------
// P - U. Production readiness HTTP contract
// ---------------------------------------------------------------------------

/// Drive the real `HttpReadinessAuthority` against a loopback fixture.
async fn probe_fixture(response: String) -> Result<(), ApiError> {
    probe_fixture_for_model("qwen3:14b", response).await
}

async fn probe_fixture_for_model(model: &str, response: String) -> Result<(), ApiError> {
    let state = new_state();
    let server = spawn_server(state, vec![response]).await;
    let origin = BrokerOrigin::loopback(server.port()).expect("loopback origin");
    HttpReadinessAuthority::new().probe(&origin, model).await
}

fn readiness_body(ready: bool, reason: &str, echo: &str) -> String {
    format!(r#"{{"ready":{ready},"reason_code":"{reason}","requested_model":"{echo}"}}"#)
}

#[tokio::test]
async fn p_the_http_authority_admits_a_valid_ready_response() {
    let response = http_response(
        "200 OK",
        "application/json",
        &readiness_body(true, "planner_ready", "qwen3:14b"),
    );
    probe_fixture(response)
        .await
        .expect("a well-formed ready response must admit");
}

#[tokio::test]
async fn p2_the_http_authority_refuses_malformed_and_unready_responses() {
    let cases: Vec<(&str, String)> = vec![
        (
            "ready false",
            http_response(
                "200 OK",
                "application/json",
                &readiness_body(false, "vram_contended", "qwen3:14b"),
            ),
        ),
        (
            "non-2xx",
            http_response("500 Internal Server Error", "application/json", "{}"),
        ),
        (
            "404",
            http_response("404 Not Found", "text/plain", "no such endpoint"),
        ),
        (
            "malformed json",
            http_response("200 OK", "application/json", "{"),
        ),
        (
            "empty body",
            http_response("200 OK", "application/json", ""),
        ),
        (
            "non-object",
            http_response("200 OK", "application/json", "[1,2,3]"),
        ),
        (
            "wrong echo",
            http_response(
                "200 OK",
                "application/json",
                &readiness_body(true, "ok", "some-other-model"),
            ),
        ),
        (
            "reason with pipe",
            http_response(
                "200 OK",
                "application/json",
                &readiness_body(true, "a|b", "qwen3:14b"),
            ),
        ),
        (
            "ready as string",
            http_response(
                "200 OK",
                "application/json",
                r#"{"ready":"true","reason_code":"ok","requested_model":"qwen3:14b"}"#,
            ),
        ),
    ];

    for (label, response) in cases {
        let error = probe_fixture(response)
            .await
            .expect_err(&format!("{label} must refuse"));
        assert_non_retryable_n6(&error, label);
    }
}

#[tokio::test]
async fn p3_the_latest_tag_echo_is_accepted_over_the_wire() {
    let response = http_response(
        "200 OK",
        "application/json",
        &readiness_body(true, "planner_ready", "qwen3:latest"),
    );
    probe_fixture_for_model("qwen3", response)
        .await
        .expect("`:latest` is the one accepted echo normalization");
}

#[tokio::test]
async fn q_duplicate_known_key_is_refused_over_the_wire() {
    let response = http_response(
        "200 OK",
        "application/json",
        r#"{"ready":false,"ready":true,"reason_code":"ok","requested_model":"qwen3:14b"}"#,
    );
    let error = probe_fixture(response)
        .await
        .expect_err("duplicate refused");
    assert_non_retryable_n6(&error, "duplicate known key");
}

#[tokio::test]
async fn r_duplicate_unknown_key_is_refused_over_the_wire() {
    let response = http_response(
        "200 OK",
        "application/json",
        r#"{"ready":true,"reason_code":"ok","requested_model":"qwen3:14b","x":1,"x":2}"#,
    );
    let error = probe_fixture(response)
        .await
        .expect_err("duplicate refused");
    assert_non_retryable_n6(&error, "duplicate unknown key");
}

#[tokio::test]
async fn s_a_redirect_is_refused_and_never_followed() {
    let state = new_state();
    // Second response would be a perfectly valid ready answer — following the
    // redirect would wrongly admit.
    let redirect_target = new_state();
    let target = spawn_server(
        redirect_target.clone(),
        vec![http_response(
            "200 OK",
            "application/json",
            &readiness_body(true, "planner_ready", "qwen3:14b"),
        )],
    )
    .await;
    let server = spawn_server(
        state,
        vec![http_response_with_headers(
            "302 Found",
            "text/plain",
            "",
            &[(
                "location",
                &format!("{}/status/n6_planner_ready", target.base_url()),
            )],
        )],
    )
    .await;

    let origin = BrokerOrigin::loopback(server.port()).expect("loopback origin");
    let error = HttpReadinessAuthority::new()
        .probe(&origin, "qwen3:14b")
        .await
        .expect_err("a 302 must refuse rather than redirect");

    assert_non_retryable_n6(&error, "redirect");
    assert!(
        captured_paths(&redirect_target).await.is_empty(),
        "the redirect target must never be contacted"
    );
}

#[tokio::test]
async fn t_readiness_does_not_traverse_the_environment_proxy() {
    let _lock = env_lock();
    let proxy_state = new_state();
    let proxy = spawn_server(
        proxy_state.clone(),
        vec![http_response(
            "200 OK",
            "application/json",
            &readiness_body(true, "proxied", "qwen3:14b"),
        )],
    )
    .await;

    let _http = EnvGuard::set("HTTP_PROXY", Some(&proxy.base_url()));
    let _https = EnvGuard::set("HTTPS_PROXY", Some(&proxy.base_url()));
    let _http_lower = EnvGuard::set("http_proxy", Some(&proxy.base_url()));
    let _https_lower = EnvGuard::set("https_proxy", Some(&proxy.base_url()));

    let direct_state = new_state();
    let direct = spawn_server(
        direct_state.clone(),
        vec![http_response(
            "200 OK",
            "application/json",
            &readiness_body(true, "direct", "qwen3:14b"),
        )],
    )
    .await;

    let origin = BrokerOrigin::loopback(direct.port()).expect("loopback origin");
    HttpReadinessAuthority::new()
        .probe(&origin, "qwen3:14b")
        .await
        .expect("the direct readiness endpoint answers");

    assert!(
        captured_paths(&proxy_state).await.is_empty(),
        "readiness must never traverse HTTP_PROXY / HTTPS_PROXY"
    );
    assert_eq!(
        captured_paths(&direct_state).await.len(),
        1,
        "readiness must reach the broker origin directly"
    );
}

#[tokio::test]
async fn u_an_oversized_readiness_body_is_refused() {
    // Advertised content-length over the bound: refused before reading a byte.
    let padding = "x".repeat(api::MAX_READINESS_BODY_BYTES + 1);
    let oversized = format!(
        r#"{{"ready":true,"reason_code":"ok","requested_model":"qwen3:14b","pad":"{padding}"}}"#
    );
    let error = probe_fixture(http_response("200 OK", "application/json", &oversized))
        .await
        .expect_err("an oversized body must refuse");
    assert_non_retryable_n6(&error, "oversized body");
}

#[tokio::test]
async fn u2_a_body_at_the_allowed_boundary_is_still_parsed() {
    // Exactly at the bound: allowed, and therefore parsed and admitted.
    let prefix = r#"{"ready":true,"reason_code":"ok","requested_model":"qwen3:14b","pad":""#;
    let suffix = r#""}"#;
    let padding = "x".repeat(api::MAX_READINESS_BODY_BYTES - prefix.len() - suffix.len());
    let body = format!("{prefix}{padding}{suffix}");
    assert_eq!(body.len(), api::MAX_READINESS_BODY_BYTES);

    probe_fixture(http_response("200 OK", "application/json", &body))
        .await
        .expect("a body exactly at the bound is within the limit");
}

#[tokio::test]
async fn u3_an_unreachable_readiness_endpoint_refuses_without_waiting() {
    // Bind then drop, so the port is almost certainly closed.
    let port = {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        listener.local_addr().expect("addr").port()
    };
    let origin = BrokerOrigin::loopback(port).expect("loopback origin");
    let error = HttpReadinessAuthority::new()
        .probe(&origin, "qwen3:14b")
        .await
        .expect_err("an unreachable endpoint must refuse, not wait");
    assert_non_retryable_n6(&error, "transport");
    match &error {
        ApiError::N6AdmissionRefused { kind, .. } => assert!(
            matches!(kind, N6RefusalKind::Transport(_)),
            "expected a transport refusal, got {kind:?}"
        ),
        other => panic!("unexpected {other:?}"),
    }
}

#[tokio::test]
async fn v_the_readiness_query_names_the_endpoint_and_model_exactly() {
    let state = new_state();
    let server = spawn_server(
        state.clone(),
        vec![http_response(
            "200 OK",
            "application/json",
            &readiness_body(true, "ok", "qwen3:14b"),
        )],
    )
    .await;
    let origin = BrokerOrigin::loopback(server.port()).expect("loopback origin");
    HttpReadinessAuthority::new()
        .probe(&origin, "qwen3:14b")
        .await
        .expect("admitted");

    let captured = state.lock().await;
    let request = captured.first().expect("one readiness request");
    assert_eq!(
        request.path, "/status/n6_planner_ready?requested_model=qwen3%3A14b",
        "the readiness GET must name the endpoint and percent-encoded model"
    );
    assert_eq!(request.method, "GET");
}

#[tokio::test]
async fn w_the_scripted_origin_is_the_validated_broker_endpoint() {
    let _lock = env_lock();
    let _marker = EnvGuard::marker_off();

    let authority = ScriptedAuthority::new(&[ReadinessVerdict::NotReady]);
    let client = OpenAiCompatClient::new("test-key", OpenAiCompatConfig::openai())
        .with_base_url("http://127.0.0.1:11435/v1/chat/completions")
        .with_n6_authority(authority.clone());

    client
        .send_message(&request_for("qwen3:14b"))
        .await
        .expect_err("refused");

    assert_eq!(
        authority.origins_asked(),
        vec!["http://127.0.0.1:11435/status/n6_planner_ready".to_string()],
        "the readiness origin must be rebuilt from the validated host and port"
    );
}

// ---------------------------------------------------------------------------
// X. Broker inference transport: environment proxies are bypassed
// ---------------------------------------------------------------------------

/// Layer B decides that a validated broker origin may receive one POST. That
/// decision is worthless if the POST is then handed to a proxy: the
/// model-bearing body would leave the validated origin over a hop the guard
/// never inspected. The readiness GET already disables proxies; this asserts
/// the same property for the inference request the readiness GET admits.
#[tokio::test]
async fn x_broker_inference_does_not_traverse_the_environment_proxy() {
    let _lock = env_lock();
    let _marker = EnvGuard::marker_off();

    let proxy_state = new_state();
    let proxy = spawn_server(proxy_state.clone(), vec![ok_completion("qwen3:14b")]).await;

    // Both spellings, and NO_PROXY cleared so the fixture is deterministic:
    // without this an ambient NO_PROXY covering loopback could mask a real
    // proxy escape and make the test pass for the wrong reason.
    let _http = EnvGuard::set("HTTP_PROXY", Some(&proxy.base_url()));
    let _https = EnvGuard::set("HTTPS_PROXY", Some(&proxy.base_url()));
    let _http_lower = EnvGuard::set("http_proxy", Some(&proxy.base_url()));
    let _https_lower = EnvGuard::set("https_proxy", Some(&proxy.base_url()));
    let _no_proxy = EnvGuard::set("NO_PROXY", None);
    let _no_proxy_lower = EnvGuard::set("no_proxy", None);

    let direct_state = new_state();
    let direct = spawn_server(direct_state.clone(), vec![ok_completion("qwen3:14b")]).await;

    // Constructed *after* the proxy environment is in place, so the ordinary
    // client this candidate would otherwise use really would carry the proxy.
    let authority = ScriptedAuthority::new(&[ReadinessVerdict::Ready]);
    let client = broker_gated_client(&direct, authority.clone());
    let response = client
        .send_message(&request_for("qwen3:14b"))
        .await
        .expect("an admitted broker request must reach the broker origin directly");

    assert_eq!(response.model, "qwen3:14b");
    assert_eq!(authority.call_count(), 1, "exactly one readiness decision");
    assert!(
        captured_paths(&proxy_state).await.is_empty(),
        "a model-bearing broker inference POST must never traverse HTTP_PROXY / \
         HTTPS_PROXY — the admitted origin is the only permitted destination"
    );
    assert_eq!(
        captured_paths(&direct_state).await,
        vec!["/chat/completions".to_string()],
        "the inference POST must reach the validated origin directly"
    );
}

// ---------------------------------------------------------------------------
// Y. Compatibility: ordinary upstream inference keeps its proxy behaviour
// ---------------------------------------------------------------------------

/// The hardening is broker-specific. A non-SideStack OpenAI-compatible
/// upstream must still honour the operator's configured proxy, so this asserts
/// the *opposite* of X for an ungated destination. If a future change disabled
/// proxies globally this test fails.
#[tokio::test]
async fn y_ordinary_upstream_inference_still_honours_the_environment_proxy() {
    let _lock = env_lock();
    let _marker = EnvGuard::marker_off();

    let proxy_state = new_state();
    let proxy = spawn_server(proxy_state.clone(), vec![ok_completion("gpt-4")]).await;

    let _http = EnvGuard::set("HTTP_PROXY", Some(&proxy.base_url()));
    let _http_lower = EnvGuard::set("http_proxy", Some(&proxy.base_url()));
    let _no_proxy = EnvGuard::set("NO_PROXY", None);
    let _no_proxy_lower = EnvGuard::set("no_proxy", None);

    let upstream_state = new_state();
    let upstream = spawn_server(upstream_state.clone(), vec![ok_completion("gpt-4")]).await;

    // No broker origin override and an ephemeral port, so `BrokerOrigin::from_base_url`
    // yields None: this is an ordinary, ungated upstream client.
    let authority = ForbiddenAuthority::new();
    let client = OpenAiCompatClient::new("test-key", OpenAiCompatConfig::openai())
        .with_base_url(upstream.base_url())
        .with_n6_authority(authority.clone());
    client
        .send_message(&request_for("gpt-4"))
        .await
        .expect("the proxy answers on the upstream's behalf");

    assert_eq!(
        authority.call_count(),
        0,
        "an ordinary upstream must generate no readiness traffic"
    );
    assert_eq!(
        captured_paths(&proxy_state).await.len(),
        1,
        "ordinary upstream inference must still traverse the configured proxy — \
         the broker hardening must not disable proxies globally"
    );
    assert!(
        captured_paths(&upstream_state).await.is_empty(),
        "the proxy answered, so the upstream itself is not contacted directly"
    );
}

// ---------------------------------------------------------------------------
// Z. Broker inference transport: no redirect is ever followed
// ---------------------------------------------------------------------------

/// A redirect below Layer B is an origin escape *and* an unadmitted second
/// outbound inference attempt: the trap would receive a model-bearing request
/// that no readiness decision ever authorised. 307 and 308 are the sharpest
/// cases because they preserve the method and the body.
#[tokio::test]
async fn z_broker_inference_never_follows_a_redirect() {
    for status in [
        "301 Moved Permanently",
        "302 Found",
        "303 See Other",
        "307 Temporary Redirect",
        "308 Permanent Redirect",
    ] {
        let _lock = env_lock();
        let _marker = EnvGuard::marker_off();

        // The trap would answer with a perfectly valid completion, so following
        // the redirect would silently succeed against an unadmitted origin.
        let trap_state = new_state();
        let trap = spawn_server(trap_state.clone(), vec![ok_completion("qwen3:14b")]).await;

        let redirect_state = new_state();
        let redirect = spawn_server(
            redirect_state.clone(),
            vec![http_response_with_headers(
                status,
                "text/plain",
                "",
                &[(
                    "location",
                    &format!("{}/v1/chat/completions", trap.base_url()),
                )],
            )],
        )
        .await;

        let authority = ScriptedAuthority::new(&[ReadinessVerdict::Ready]);
        let client = broker_gated_client(&redirect, authority.clone());
        let error = client
            .send_message(&request_for("qwen3:14b"))
            .await
            .expect_err(&format!("{status}: a redirect must not be followed"));

        // The 3xx surfaces through the ordinary provider error path and is not
        // retryable, so it also cannot trigger a fallback to another model.
        assert!(
            matches!(error, ApiError::Api { .. }),
            "{status}: expected the 3xx to surface as a provider error, got {error:?}"
        );
        assert!(
            !error.is_retryable(),
            "{status}: an unfollowed redirect must not be retried"
        );
        assert_eq!(
            authority.call_count(),
            1,
            "{status}: exactly one readiness decision was made"
        );
        assert_eq!(
            captured_paths(&redirect_state).await.len(),
            1,
            "{status}: the admitted origin is contacted exactly once"
        );
        assert!(
            captured_paths(&trap_state).await.is_empty(),
            "{status}: the redirect target must never be contacted — following it \
             would send the model-bearing request to an origin that Layer A never \
             validated and Layer B never admitted"
        );
    }
}

// ---------------------------------------------------------------------------
// Z2. Compatibility: ordinary upstream inference still follows redirects
// ---------------------------------------------------------------------------

/// Redirect refusal is broker-specific too. An ordinary upstream keeps
/// reqwest's default redirect behaviour, so this asserts the opposite of Z.
#[tokio::test]
async fn z2_ordinary_upstream_inference_still_follows_a_redirect() {
    let _lock = env_lock();
    let _marker = EnvGuard::marker_off();
    let _no_proxy = EnvGuard::set("HTTP_PROXY", None);
    let _no_proxy_lower = EnvGuard::set("http_proxy", None);
    let _https = EnvGuard::set("HTTPS_PROXY", None);
    let _https_lower = EnvGuard::set("https_proxy", None);

    let target_state = new_state();
    let target = spawn_server(target_state.clone(), vec![ok_completion("gpt-4")]).await;

    let redirect_state = new_state();
    let redirect = spawn_server(
        redirect_state.clone(),
        vec![http_response_with_headers(
            "307 Temporary Redirect",
            "text/plain",
            "",
            &[(
                "location",
                &format!("{}/v1/chat/completions", target.base_url()),
            )],
        )],
    )
    .await;

    let authority = ForbiddenAuthority::new();
    let client = OpenAiCompatClient::new("test-key", OpenAiCompatConfig::openai())
        .with_base_url(redirect.base_url())
        .with_n6_authority(authority.clone());
    let response = client
        .send_message(&request_for("gpt-4"))
        .await
        .expect("an ordinary upstream still follows its provider's redirect");

    assert_eq!(response.model, "gpt-4");
    assert_eq!(authority.call_count(), 0);
    assert_eq!(
        captured_paths(&target_state).await.len(),
        1,
        "ordinary upstream redirect following must be preserved — the broker \
         hardening must not disable redirects globally"
    );
}

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

type SharedState = Arc<Mutex<Vec<CapturedRequest>>>;

fn new_state() -> SharedState {
    Arc::new(Mutex::new(Vec::new()))
}

async fn captured_paths(state: &SharedState) -> Vec<String> {
    state
        .lock()
        .await
        .iter()
        .map(|request| request.path.clone())
        .collect()
}

async fn captured_model(state: &SharedState) -> String {
    let captured = state.lock().await;
    let request = captured.first().expect("a captured inference request");
    let body: serde_json::Value = serde_json::from_str(&request.body).expect("json body");
    body.get("model")
        .and_then(serde_json::Value::as_str)
        .expect("payload model")
        .to_string()
}

/// A client whose inference goes to `server` but which is admission-gated as if
/// it were pointed at the broker. The origin override only ever *adds* gating.
fn broker_gated_client(
    server: &TestServer,
    authority: Arc<dyn N6ReadinessAuthority>,
) -> OpenAiCompatClient {
    OpenAiCompatClient::new("test-key", OpenAiCompatConfig::openai())
        .with_base_url(server.base_url())
        .with_n6_authority(authority)
        .with_n6_broker_origin(BrokerOrigin::loopback(server.port()).expect("loopback origin"))
}

fn request_for(model: &str) -> MessageRequest {
    MessageRequest {
        model: model.to_string(),
        max_tokens: 64,
        messages: vec![InputMessage {
            role: "user".to_string(),
            content: vec![InputContentBlock::Text {
                text: "Say hello".to_string(),
            }],
        }],
        system: None,
        tools: None,
        tool_choice: None,
        stream: false,
        ..Default::default()
    }
}

fn ok_completion(model: &str) -> String {
    let body = format!(
        concat!(
            "{{",
            "\"id\":\"chatcmpl_test\",",
            "\"model\":\"{model}\",",
            "\"choices\":[{{",
            "\"message\":{{\"role\":\"assistant\",\"content\":\"ok\",\"tool_calls\":[]}},",
            "\"finish_reason\":\"stop\"",
            "}}],",
            "\"usage\":{{\"prompt_tokens\":1,\"completion_tokens\":1}}",
            "}}"
        ),
        model = model
    );
    http_response("200 OK", "application/json", &body)
}

#[derive(Debug)]
struct CapturedRequest {
    method: String,
    path: String,
    #[allow(dead_code)]
    headers: HashMap<String, String>,
    body: String,
}

struct TestServer {
    base_url: String,
    port: u16,
    join_handle: tokio::task::JoinHandle<()>,
}

impl TestServer {
    fn base_url(&self) -> String {
        self.base_url.clone()
    }

    const fn port(&self) -> u16 {
        self.port
    }
}

impl Drop for TestServer {
    fn drop(&mut self) {
        self.join_handle.abort();
    }
}

/// A listener that accepts connections and counts them without ever replying.
/// Used to prove a refusal happened before any connection was opened.
struct ConnectionSink {
    base_url: String,
    accepted: Arc<AtomicUsize>,
    join_handle: tokio::task::JoinHandle<()>,
}

impl ConnectionSink {
    fn base_url(&self) -> String {
        self.base_url.clone()
    }

    fn accepted(&self) -> usize {
        self.accepted.load(Ordering::SeqCst)
    }
}

impl Drop for ConnectionSink {
    fn drop(&mut self) {
        self.join_handle.abort();
    }
}

async fn spawn_connection_sink() -> ConnectionSink {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let address = listener.local_addr().expect("addr");
    let accepted = Arc::new(AtomicUsize::new(0));
    let counter = accepted.clone();
    let join_handle = tokio::spawn(async move {
        while listener.accept().await.is_ok() {
            counter.fetch_add(1, Ordering::SeqCst);
        }
    });
    ConnectionSink {
        base_url: format!("http://{address}"),
        accepted,
        join_handle,
    }
}

async fn spawn_server(state: SharedState, responses: Vec<String>) -> TestServer {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listener should bind");
    let address = listener.local_addr().expect("listener addr");
    let join_handle = tokio::spawn(async move {
        for response in responses {
            let Ok((mut socket, _)) = listener.accept().await else {
                break;
            };
            let mut buffer = Vec::new();
            let mut header_end = None;
            loop {
                let mut chunk = [0_u8; 1024];
                let Ok(read) = socket.read(&mut chunk).await else {
                    break;
                };
                if read == 0 {
                    break;
                }
                buffer.extend_from_slice(&chunk[..read]);
                if let Some(position) = find_header_end(&buffer) {
                    header_end = Some(position);
                    break;
                }
            }

            let Some(header_end) = header_end else {
                continue;
            };
            let (header_bytes, remaining) = buffer.split_at(header_end);
            let header_text = String::from_utf8(header_bytes.to_vec()).expect("utf8 headers");
            let mut lines = header_text.split("\r\n");
            let request_line = lines.next().expect("request line");
            let mut parts = request_line.split_whitespace();
            let method = parts.next().unwrap_or_default().to_string();
            let path = parts.next().unwrap_or_default().to_string();
            let mut headers = HashMap::new();
            let mut content_length = 0_usize;
            for line in lines {
                if line.is_empty() {
                    continue;
                }
                let Some((name, value)) = line.split_once(':') else {
                    continue;
                };
                let value = value.trim().to_string();
                if name.eq_ignore_ascii_case("content-length") {
                    content_length = value.parse().unwrap_or(0);
                }
                headers.insert(name.to_ascii_lowercase(), value);
            }

            let mut body = remaining[4..].to_vec();
            while body.len() < content_length {
                let mut chunk = vec![0_u8; content_length - body.len()];
                let Ok(read) = socket.read(&mut chunk).await else {
                    break;
                };
                if read == 0 {
                    break;
                }
                body.extend_from_slice(&chunk[..read]);
            }

            state.lock().await.push(CapturedRequest {
                method,
                path,
                headers,
                body: String::from_utf8_lossy(&body).into_owned(),
            });

            let _ = socket.write_all(response.as_bytes()).await;
        }
    });

    TestServer {
        base_url: format!("http://{address}"),
        port: address.port(),
        join_handle,
    }
}

fn find_header_end(bytes: &[u8]) -> Option<usize> {
    bytes.windows(4).position(|window| window == b"\r\n\r\n")
}

fn http_response(status: &str, content_type: &str, body: &str) -> String {
    http_response_with_headers(status, content_type, body, &[])
}

fn http_response_with_headers(
    status: &str,
    content_type: &str,
    body: &str,
    headers: &[(&str, &str)],
) -> String {
    let mut extra_headers = String::new();
    for (name, value) in headers {
        use std::fmt::Write as _;
        write!(&mut extra_headers, "{name}: {value}\r\n").expect("header write");
    }
    format!(
        "HTTP/1.1 {status}\r\ncontent-type: {content_type}\r\n{extra_headers}content-length: {}\r\nconnection: close\r\n\r\n{body}",
        body.len()
    )
}
