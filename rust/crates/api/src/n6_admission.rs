//! N6 broker admission guard.
//!
//! This module implements the in-process half of the `SideStack` local-coding
//! trust contract. The shell wrapper (`scripts/claw-sidestack-local`) can only
//! gate the model it resolves *before* `exec`; once the process is running, the
//! REPL `/model` command, Agent primary/fallback selection, and the provider
//! retry loop can all reach the broker with a model the wrapper never saw.
//!
//! Two independent layers close that gap:
//!
//! * **Layer A (LAW-1 routing containment)** — when the dedicated `SideStack`
//!   process marker is active, only an OpenAI-compatible provider pointed at a
//!   validated loopback broker origin may perform inference at all. Anthropic,
//!   cloud OpenAI-compatible endpoints, and raw `:11434` are refused *before*
//!   any network call. See [`marker_active`] and [`BrokerOrigin::from_base_url`].
//!
//! * **Layer B (per-attempt readiness)** — every outbound inference HTTP
//!   attempt aimed at a validated broker origin is preceded by a fresh
//!   readiness query for the exact wire model the payload will carry. This runs
//!   inside the retry loop, so retry number four is gated exactly as tightly as
//!   the first attempt.
//!
//! # No admission cache
//!
//! The readiness response is a point-in-time snapshot, not a lease: the broker
//! issues no reservation, epoch, or residency token that a client could hold.
//! Caching an "admitted" verdict would therefore convert a momentary yes into a
//! standing permission. Every attempt re-asks. There is deliberately no
//! process-lifetime, per-model, or time-based cache anywhere in this module.
//!
//! # Residual race
//!
//! A readiness `GET` that returns ready and the inference `POST` that follows
//! are two separate requests. The broker's state can change in between. This
//! guard narrows that window to a single round trip; it does not close it, and
//! nothing here should be read as an atomic admission.

use std::collections::BTreeSet;
use std::fmt;
use std::future::Future;
use std::pin::Pin;
use std::time::Duration;

use reqwest::Url;
use serde::de::{Deserializer, Error as DeError, IgnoredAny, MapAccess, Visitor};

use crate::error::{ApiError, N6RefusalKind};

/// Process marker set by `scripts/claw-sidestack-local` immediately before it
/// `exec`s the canonical claw binary.
///
/// This is deliberately *not* `RUSTY_CLAUDE_LLM_CALLER`: that variable carries
/// broker telemetry/header semantics, is populated from a user-editable env
/// file rather than by the wrapper itself, and would fail *open* if it drifted.
/// Enforcement state needs its own name that only the protected wrapper sets.
pub const SIDESTACK_MARKER_ENV: &str = "CLAW_SIDESTACK_N6_ENFORCE";

/// Stable, versioned identifier for the in-process enforcement contract this
/// module implements: [`marker_active`] routing containment (Layer A) plus a
/// per-broker-attempt readiness admission (Layer B).
///
/// The wrapper exports [`SIDESTACK_MARKER_ENV`] immediately before `exec`, but
/// setting an environment variable proves nothing about the binary that
/// receives it: a build that predates this contract simply ignores the marker
/// and runs an ungated API. So the binary advertises this token in its
/// `--version` banner, and `scripts/claw-canonical-status` reports whether the
/// canonical executable carries it. A binary that cannot enforce in process is
/// then refused *before* any readiness query or `exec`.
///
/// Freshness is deliberately not the proof. `claw-canonical-status` calls an
/// install CURRENT when its Git SHA equals the locally known `origin/main`,
/// which is useful provenance but says nothing about a wrapper or worktree that
/// is ahead of `origin/main`, and nothing at all under `STALE` overrides or
/// `UNKNOWN_BASE`. Capability is reported orthogonally to freshness for exactly
/// that reason.
///
/// # Versioning
///
/// The trailing `-v1` is load-bearing. The protocol being asserted is
/// "recognises the marker, contains routing, and admits per attempt". If any of
/// that changes shape, a new binary advertises `-v2` and an old wrapper stops
/// matching, rather than a boolean "N6 supported" silently spanning two
/// incompatible contracts. Never widen the match to a prefix or substring:
/// `sidestack-n6-enforce-v10` is a different contract, not this one.
pub const SIDESTACK_N6_ENFORCE_CAPABILITY: &str = "sidestack-n6-enforce-v1";

/// Readiness endpoint path on the broker. Mirrors the shell wrapper contract.
pub const READINESS_PATH: &str = "/status/n6_planner_ready";

/// The one port a `SideStack` broker origin may use. `:11434` (raw Ollama) is
/// never a valid application inference target.
pub const BROKER_PORT: u16 = 11435;

/// Hard upper bound on a readiness response body.
pub const MAX_READINESS_BODY_BYTES: usize = 65_536;

const READINESS_CONNECT_TIMEOUT: Duration = Duration::from_secs(2);
const READINESS_TOTAL_TIMEOUT: Duration = Duration::from_secs(5);

const REQUESTED_MODEL_PARAM: &str = "requested_model";

/// Boxed future returned by [`N6ReadinessAuthority::check`].
///
/// Mirrors the crate's existing `ProviderFuture` idiom so no `async-trait`
/// dependency is required.
pub type N6ReadinessFuture<'a> = Pin<Box<dyn Future<Output = Result<(), ApiError>> + Send + 'a>>;

/// Returns `true` when the dedicated `SideStack` process marker is active.
///
/// The value is interpreted strictly: `trim() == "1"`. Anything else — unset,
/// empty, `"0"`, `"true"`, `"1 1"` — means the marker is absent.
///
/// This is read from the live process environment at each inference request
/// rather than captured once at client construction. That is deliberate:
/// `ProviderClient` variants are also built directly by the CLI (the Anthropic
/// runtime path constructs `ProviderClient::Anthropic(..)` itself rather than
/// going through `from_model*`), so a construction-captured policy would carry
/// no marker on exactly the cloud path Layer A exists to refuse — it would fail
/// open. An environment read once per inference request is negligible beside
/// the network call it guards.
#[must_use]
pub fn marker_active() -> bool {
    std::env::var(SIDESTACK_MARKER_ENV).is_ok_and(|value| value.trim() == "1")
}

/// A validated `SideStack` broker origin.
///
/// The only production constructor is [`BrokerOrigin::from_base_url`]. The
/// readiness endpoint it exposes is rebuilt from the *parsed* host and the
/// compile-time port constant, never spliced from the caller's string, so a
/// crafted base URL cannot smuggle a different destination into the readiness
/// request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BrokerOrigin {
    endpoint: Url,
}

impl BrokerOrigin {
    /// Validate an OpenAI-compatible base URL as a `SideStack` broker origin.
    ///
    /// Returns `None` — meaning "not a broker origin, this guard does not
    /// apply" — unless every one of these holds:
    ///
    /// * scheme is exactly `http` (no `https`, no other scheme);
    /// * host is exactly `127.0.0.1` or `localhost` (not `localhost.evil`);
    /// * port is explicitly `11435` (an implicit default port is rejected);
    /// * no username, no password, no query, no fragment;
    /// * the URL can be a base;
    /// * the path, after trimming a trailing slash, is one of the shapes the
    ///   OpenAI-compat endpoint builder actually accepts: empty, `/v1`, or
    ///   `/v1/chat/completions`.
    ///
    /// Matching `chat_completions_endpoint`, which trims a trailing slash and
    /// appends `/chat/completions` unless the base already ends with it, those
    /// three shapes are exactly the canonical `SideStack` configurations.
    #[must_use]
    pub fn from_base_url(base_url: &str) -> Option<Self> {
        let parsed = Url::parse(base_url.trim()).ok()?;
        if parsed.cannot_be_a_base() {
            return None;
        }
        if parsed.scheme() != "http" {
            return None;
        }
        if !parsed.username().is_empty() || parsed.password().is_some() {
            return None;
        }
        if parsed.query().is_some() || parsed.fragment().is_some() {
            return None;
        }
        // `Url::port` returns `None` when the port is the scheme default, so
        // this also rejects `http://localhost` (implicit :80).
        if parsed.port() != Some(BROKER_PORT) {
            return None;
        }
        let host = parsed.host_str()?;
        if host != "127.0.0.1" && host != "localhost" {
            return None;
        }
        if !matches!(
            parsed.path().trim_end_matches('/'),
            "" | "/v1" | "/v1/chat/completions"
        ) {
            return None;
        }
        // Re-derive from validated components rather than reusing the parsed
        // URL, so no path, query, or userinfo from the input can survive.
        Some(Self {
            endpoint: readiness_endpoint_for(host, BROKER_PORT)?,
        })
    }

    /// Build a readiness endpoint on a loopback port.
    ///
    /// No production path calls this — production origins come only from
    /// [`BrokerOrigin::from_base_url`], which pins the port to
    /// [`BROKER_PORT`]. It exists so the readiness HTTP contract (redirects,
    /// proxies, body bounds, response protocol) can be exercised against a
    /// loopback fixture server without contacting a real broker.
    #[must_use]
    pub fn loopback(port: u16) -> Option<Self> {
        Some(Self {
            endpoint: readiness_endpoint_for("127.0.0.1", port)?,
        })
    }

    /// The readiness endpoint URL, without the per-request query string.
    #[must_use]
    pub const fn endpoint(&self) -> &Url {
        &self.endpoint
    }

    /// Build the readiness URL for one wire model.
    ///
    /// The model is appended with `query_pairs_mut` so percent-encoding is
    /// handled by the URL crate rather than by hand.
    #[must_use]
    pub fn readiness_url(&self, wire_model: &str) -> Url {
        let mut url = self.endpoint.clone();
        url.query_pairs_mut()
            .append_pair(REQUESTED_MODEL_PARAM, wire_model);
        url
    }
}

fn readiness_endpoint_for(host: &str, port: u16) -> Option<Url> {
    let mut url = Url::parse(&format!("http://{host}:{port}/")).ok()?;
    url.set_path(READINESS_PATH);
    Some(url)
}

/// Readiness authority: answers "may this exact wire model be sent to this
/// broker origin, right now?".
///
/// Implementors must be usable from many concurrent requests. The production
/// implementation is [`HttpReadinessAuthority`]; tests supply scripted
/// implementations so no live broker is ever contacted.
pub trait N6ReadinessAuthority: fmt::Debug + Send + Sync {
    /// Perform one readiness decision. `Ok(())` admits exactly one subsequent
    /// inference HTTP attempt; any `Err` must be treated as fail-closed.
    fn check<'a>(&'a self, origin: &'a BrokerOrigin, wire_model: &'a str) -> N6ReadinessFuture<'a>;
}

/// Production readiness authority: one bounded HTTP `GET` per call.
///
/// The client is built with proxies disabled and redirects refused, so a
/// `HTTP_PROXY` in the environment or a `302` from the endpoint cannot move the
/// readiness question somewhere else.
#[derive(Debug, Clone)]
pub struct HttpReadinessAuthority {
    /// `None` when the bounded client could not be constructed. Every check
    /// then refuses, rather than silently falling back to a client with
    /// default proxy and redirect behaviour.
    http: Option<reqwest::Client>,
}

impl Default for HttpReadinessAuthority {
    fn default() -> Self {
        Self::new()
    }
}

impl HttpReadinessAuthority {
    /// Build the production readiness client.
    #[must_use]
    pub fn new() -> Self {
        Self {
            http: reqwest::Client::builder()
                .no_proxy()
                .redirect(reqwest::redirect::Policy::none())
                .connect_timeout(READINESS_CONNECT_TIMEOUT)
                .timeout(READINESS_TOTAL_TIMEOUT)
                .build()
                .ok(),
        }
    }

    /// Query readiness for one wire model at one origin.
    ///
    /// There is no wait loop and no retry: a single failure refuses.
    pub async fn probe(&self, origin: &BrokerOrigin, wire_model: &str) -> Result<(), ApiError> {
        let refuse = |kind| ApiError::N6AdmissionRefused {
            model: wire_model.to_string(),
            kind,
        };

        let Some(http) = self.http.as_ref() else {
            return Err(refuse(N6RefusalKind::Transport(
                "readiness http client could not be built with proxies disabled and redirects refused".to_string(),
            )));
        };

        let response = http
            .get(origin.readiness_url(wire_model))
            .send()
            .await
            .map_err(|error| refuse(N6RefusalKind::Transport(format!("{error}"))))?;

        let status = response.status();
        if !status.is_success() {
            return Err(refuse(N6RefusalKind::Protocol(format!(
                "readiness endpoint returned HTTP {status}"
            ))));
        }

        let body = read_bounded_body(response)
            .await
            .map_err(|detail| refuse(N6RefusalKind::Protocol(detail)))?;

        match evaluate_readiness_body(wire_model, &body) {
            Ok(()) => Ok(()),
            Err(kind) => Err(refuse(kind)),
        }
    }
}

impl N6ReadinessAuthority for HttpReadinessAuthority {
    fn check<'a>(&'a self, origin: &'a BrokerOrigin, wire_model: &'a str) -> N6ReadinessFuture<'a> {
        Box::pin(self.probe(origin, wire_model))
    }
}

/// Read a response body, refusing anything over [`MAX_READINESS_BODY_BYTES`].
///
/// An advertised `content-length` over the bound is rejected before a single
/// body byte is read; otherwise chunks are accumulated and the read is aborted
/// the moment the running total would exceed the bound. `response.bytes()` is
/// deliberately not used — it is unbounded.
async fn read_bounded_body(mut response: reqwest::Response) -> Result<String, String> {
    let bound = MAX_READINESS_BODY_BYTES;
    if let Some(advertised) = response.content_length() {
        if advertised > bound as u64 {
            return Err(format!(
                "readiness body advertises {advertised} bytes, over the {bound}-byte bound"
            ));
        }
    }

    let mut buffer: Vec<u8> = Vec::new();
    loop {
        match response.chunk().await {
            Ok(Some(chunk)) => {
                if buffer.len().saturating_add(chunk.len()) > bound {
                    return Err(format!(
                        "readiness body exceeded the {bound}-byte bound before it completed"
                    ));
                }
                buffer.extend_from_slice(&chunk);
            }
            Ok(None) => break,
            Err(error) => return Err(format!("readiness body could not be read: {error}")),
        }
    }

    String::from_utf8(buffer).map_err(|_| "readiness body was not valid UTF-8".to_string())
}

/// The three fields the readiness contract requires, parsed under a
/// duplicate-key-rejecting deserializer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReadinessReport {
    /// Real JSON boolean. A string `"true"` or a number `1` is a protocol error.
    pub ready: bool,
    pub reason_code: String,
    pub requested_model: String,
}

impl<'de> serde::Deserialize<'de> for ReadinessReport {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_map(ReadinessReportVisitor)
    }
}

struct ReadinessReportVisitor;

impl<'de> Visitor<'de> for ReadinessReportVisitor {
    type Value = ReadinessReport;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a JSON object carrying ready, reason_code, and requested_model")
    }

    fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
    where
        A: MapAccess<'de>,
    {
        // `serde_json`'s derived map handling silently keeps the last value for
        // a repeated key, which would let a response smuggle a second `ready`
        // past the contract. Track every key we see — known and unknown alike —
        // and refuse the whole document on any repeat, matching the shell
        // parser's `no_duplicate_keys` rule.
        let mut seen: BTreeSet<String> = BTreeSet::new();
        let mut ready: Option<bool> = None;
        let mut reason_code: Option<String> = None;
        let mut requested_model: Option<String> = None;

        while let Some(key) = map.next_key::<String>()? {
            if !seen.insert(key.clone()) {
                return Err(DeError::custom(format!("duplicate readiness key `{key}`")));
            }
            match key.as_str() {
                "ready" => ready = Some(map.next_value::<bool>()?),
                "reason_code" => reason_code = Some(map.next_value::<String>()?),
                "requested_model" => requested_model = Some(map.next_value::<String>()?),
                // Unknown fields are allowed and ignored, but they still count
                // towards the duplicate-key rule above.
                _ => {
                    map.next_value::<IgnoredAny>()?;
                }
            }
        }

        Ok(ReadinessReport {
            ready: ready.ok_or_else(|| DeError::missing_field("ready"))?,
            reason_code: reason_code.ok_or_else(|| DeError::missing_field("reason_code"))?,
            requested_model: requested_model
                .ok_or_else(|| DeError::missing_field("requested_model"))?,
        })
    }
}

/// Validate a readiness body against the contract and decide admission.
///
/// `Ok(())` means this one inference attempt is admitted. Every other outcome —
/// malformed JSON, a non-object, a missing or wrongly typed field, a
/// `reason_code` carrying a field separator or newline, an echo that does not
/// match the model we asked about, or `ready:false` — refuses. There is no
/// fallback and no wait.
pub fn evaluate_readiness_body(sent_model: &str, body: &str) -> Result<(), N6RefusalKind> {
    let report: ReadinessReport = serde_json::from_str(body)
        .map_err(|error| N6RefusalKind::Protocol(format!("readiness body rejected: {error}")))?;

    let reason_code = report.reason_code.trim();
    if reason_code.is_empty() {
        return Err(N6RefusalKind::Protocol(
            "readiness reason_code was empty".to_string(),
        ));
    }
    if reason_code.contains('|') || reason_code.contains('\n') || reason_code.contains('\r') {
        return Err(N6RefusalKind::Protocol(
            "readiness reason_code contained a field separator or line break".to_string(),
        ));
    }

    // The echo must name the model we actually asked about. `:latest` is the
    // one accepted normalization, matching the broker's tag defaulting.
    let echoed = report.requested_model.trim();
    if echoed != sent_model && echoed != format!("{sent_model}:latest") {
        return Err(N6RefusalKind::Protocol(format!(
            "readiness echoed model `{echoed}` but `{sent_model}` was requested"
        )));
    }

    if report.ready {
        Ok(())
    } else {
        Err(N6RefusalKind::NotReady {
            reason_code: reason_code.to_string(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{
        evaluate_readiness_body, marker_active, BrokerOrigin, N6RefusalKind, READINESS_PATH,
        SIDESTACK_MARKER_ENV,
    };
    use std::sync::{Mutex, MutexGuard, OnceLock};

    fn env_lock() -> MutexGuard<'static, ()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    struct MarkerGuard {
        original: Option<std::ffi::OsString>,
    }

    impl MarkerGuard {
        fn set(value: Option<&str>) -> Self {
            let original = std::env::var_os(SIDESTACK_MARKER_ENV);
            match value {
                Some(value) => std::env::set_var(SIDESTACK_MARKER_ENV, value),
                None => std::env::remove_var(SIDESTACK_MARKER_ENV),
            }
            Self { original }
        }
    }

    impl Drop for MarkerGuard {
        fn drop(&mut self) {
            match self.original.take() {
                Some(value) => std::env::set_var(SIDESTACK_MARKER_ENV, value),
                None => std::env::remove_var(SIDESTACK_MARKER_ENV),
            }
        }
    }

    #[test]
    fn marker_is_active_only_for_exactly_one_after_trimming() {
        let _lock = env_lock();

        let _guard = MarkerGuard::set(Some("1"));
        assert!(marker_active(), "bare 1 must activate the marker");

        let _guard = MarkerGuard::set(Some("  1  "));
        assert!(marker_active(), "surrounding whitespace must be trimmed");

        for inactive in ["", "0", "true", "yes", "11", "1 1", "01"] {
            let _guard = MarkerGuard::set(Some(inactive));
            assert!(
                !marker_active(),
                "value {inactive:?} must not activate the marker"
            );
        }

        let _guard = MarkerGuard::set(None);
        assert!(!marker_active(), "unset must not activate the marker");
    }

    #[test]
    fn accepts_the_canonical_sidestack_base_url_shapes() {
        for accepted in [
            "http://127.0.0.1:11435",
            "http://127.0.0.1:11435/",
            "http://localhost:11435",
            "http://127.0.0.1:11435/v1",
            "http://127.0.0.1:11435/v1/",
            "http://localhost:11435/v1",
            "http://127.0.0.1:11435/v1/chat/completions",
            // `Url::parse` resolves dot segments, so this normalizes to `/v1`.
            // It is accepted for the same reason `/v1` is: the host and port
            // are still the validated broker, and the readiness endpoint is
            // rebuilt from those components rather than from this path.
            "http://127.0.0.1:11435/../v1",
        ] {
            assert!(
                BrokerOrigin::from_base_url(accepted).is_some(),
                "{accepted} is a canonical SideStack base URL and must validate"
            );
        }
    }

    #[test]
    fn rejects_every_non_broker_url_shape() {
        for rejected in [
            "https://localhost:11435",
            "https://127.0.0.1:11435/v1",
            "http://localhost",
            "http://127.0.0.1",
            "http://localhost:11434",
            "http://127.0.0.1:11434/v1",
            "http://localhost:8080",
            "http://localhost.evil:11435",
            "http://127.0.0.1.evil:11435",
            "http://user@localhost:11435",
            "http://user:pass@127.0.0.1:11435",
            "http://127.0.0.1:11435/v1?x=1",
            "http://127.0.0.1:11435/v1#frag",
            "http://127.0.0.1:11435/admin",
            "http://127.0.0.1:11435/v1/embeddings",
            "http://127.0.0.1:11435/v1/../admin",
            "not a url",
            "",
            "mailto:someone@example.com",
            "http://[::1]:11435",
            "http://api.openai.com:11435/v1",
        ] {
            assert!(
                BrokerOrigin::from_base_url(rejected).is_none(),
                "{rejected} must not validate as a SideStack broker origin"
            );
        }
    }

    #[test]
    fn readiness_url_is_rebuilt_from_validated_components() {
        let origin = BrokerOrigin::from_base_url("http://localhost:11435/v1")
            .expect("canonical base URL validates");
        let url = origin.readiness_url("qwen3:14b");

        assert_eq!(url.scheme(), "http");
        assert_eq!(url.host_str(), Some("localhost"));
        assert_eq!(url.port(), Some(11435));
        // The `/v1` from the inference base URL must not leak into the
        // readiness path.
        assert_eq!(url.path(), READINESS_PATH);
        assert_eq!(url.query(), Some("requested_model=qwen3%3A14b"));
    }

    #[test]
    fn readiness_url_percent_encodes_hostile_model_names() {
        let origin =
            BrokerOrigin::from_base_url("http://127.0.0.1:11435").expect("base URL validates");
        let url = origin.readiness_url("evil&ready=true#x");

        assert_eq!(url.path(), READINESS_PATH);
        assert_eq!(
            url.query(),
            Some("requested_model=evil%26ready%3Dtrue%23x"),
            "a crafted model name must not be able to inject query parameters"
        );
    }

    fn refusal(sent: &str, body: &str) -> N6RefusalKind {
        evaluate_readiness_body(sent, body).expect_err("this readiness body must be refused")
    }

    #[test]
    fn admits_a_well_formed_ready_response() {
        assert!(evaluate_readiness_body(
            "qwen3:14b",
            r#"{"ready":true,"reason_code":"planner_ready","requested_model":"qwen3:14b"}"#,
        )
        .is_ok());
    }

    #[test]
    fn admits_the_latest_tag_normalization_of_the_echo() {
        assert!(evaluate_readiness_body(
            "qwen3",
            r#"{"ready":true,"reason_code":"planner_ready","requested_model":"qwen3:latest"}"#,
        )
        .is_ok());
    }

    #[test]
    fn allows_unknown_non_duplicate_fields() {
        assert!(evaluate_readiness_body(
            "qwen3:14b",
            r#"{"ready":true,"reason_code":"ok","requested_model":"qwen3:14b","queue_depth":3}"#,
        )
        .is_ok());
    }

    #[test]
    fn refuses_ready_false_as_a_policy_decision_not_a_protocol_error() {
        let kind = refusal(
            "qwen3:14b",
            r#"{"ready":false,"reason_code":"vram_contended","requested_model":"qwen3:14b"}"#,
        );
        match kind {
            N6RefusalKind::NotReady { reason_code } => assert_eq!(reason_code, "vram_contended"),
            other => panic!("expected NotReady, got {other:?}"),
        }
    }

    #[test]
    fn refuses_every_malformed_protocol_shape() {
        let sent = "qwen3:14b";
        let cases: &[(&str, &str)] = &[
            ("non-object array", r#"[{"ready":true}]"#),
            ("non-object string", r#""ready""#),
            ("non-object bool", "true"),
            ("non-object null", "null"),
            ("malformed json", r#"{"ready":true,"#),
            ("empty body", ""),
            (
                "missing ready",
                r#"{"reason_code":"ok","requested_model":"qwen3:14b"}"#,
            ),
            (
                "ready as string",
                r#"{"ready":"true","reason_code":"ok","requested_model":"qwen3:14b"}"#,
            ),
            (
                "ready as number",
                r#"{"ready":1,"reason_code":"ok","requested_model":"qwen3:14b"}"#,
            ),
            (
                "ready as null",
                r#"{"ready":null,"reason_code":"ok","requested_model":"qwen3:14b"}"#,
            ),
            (
                "missing reason_code",
                r#"{"ready":true,"requested_model":"qwen3:14b"}"#,
            ),
            (
                "empty reason_code",
                r#"{"ready":true,"reason_code":"","requested_model":"qwen3:14b"}"#,
            ),
            (
                "whitespace reason_code",
                r#"{"ready":true,"reason_code":"   ","requested_model":"qwen3:14b"}"#,
            ),
            (
                "reason_code with pipe",
                r#"{"ready":true,"reason_code":"a|b","requested_model":"qwen3:14b"}"#,
            ),
            (
                "reason_code with newline",
                r#"{"ready":true,"reason_code":"a\nb","requested_model":"qwen3:14b"}"#,
            ),
            (
                "reason_code with carriage return",
                r#"{"ready":true,"reason_code":"a\rb","requested_model":"qwen3:14b"}"#,
            ),
            (
                "reason_code not a string",
                r#"{"ready":true,"reason_code":7,"requested_model":"qwen3:14b"}"#,
            ),
            ("missing echo", r#"{"ready":true,"reason_code":"ok"}"#),
            (
                "empty echo",
                r#"{"ready":true,"reason_code":"ok","requested_model":""}"#,
            ),
            (
                "wrong echo",
                r#"{"ready":true,"reason_code":"ok","requested_model":"llama3:8b"}"#,
            ),
            (
                "echo is a prefix only",
                r#"{"ready":true,"reason_code":"ok","requested_model":"qwen3"}"#,
            ),
            (
                "echo not a string",
                r#"{"ready":true,"reason_code":"ok","requested_model":true}"#,
            ),
        ];

        for (label, body) in cases {
            let kind = refusal(sent, body);
            assert!(
                matches!(kind, N6RefusalKind::Protocol(_)),
                "{label} must be a protocol refusal, got {kind:?}"
            );
        }
    }

    #[test]
    fn refuses_a_duplicate_known_key() {
        let kind = refusal(
            "qwen3:14b",
            r#"{"ready":false,"ready":true,"reason_code":"ok","requested_model":"qwen3:14b"}"#,
        );
        assert!(
            matches!(kind, N6RefusalKind::Protocol(ref detail) if detail.contains("duplicate")),
            "a repeated `ready` must refuse the document, got {kind:?}"
        );
    }

    #[test]
    fn refuses_a_duplicate_unknown_key() {
        let kind = refusal(
            "qwen3:14b",
            r#"{"ready":true,"reason_code":"ok","requested_model":"qwen3:14b","note":1,"note":2}"#,
        );
        assert!(
            matches!(kind, N6RefusalKind::Protocol(ref detail) if detail.contains("duplicate")),
            "a repeated unknown key must refuse the document, got {kind:?}"
        );
    }

    #[test]
    fn refuses_a_duplicate_echo_even_when_both_values_agree() {
        let kind = refusal(
            "qwen3:14b",
            r#"{"ready":true,"reason_code":"ok","requested_model":"qwen3:14b","requested_model":"qwen3:14b"}"#,
        );
        assert!(
            matches!(kind, N6RefusalKind::Protocol(ref detail) if detail.contains("duplicate")),
            "duplicate keys refuse regardless of value agreement, got {kind:?}"
        );
    }
}
