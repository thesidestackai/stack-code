use std::collections::HashMap;
use std::ffi::OsString;
use std::sync::Arc;
use std::sync::{Mutex as StdMutex, OnceLock};

use api::{
    ApiError, ContentBlockDelta, ContentBlockDeltaEvent, ContentBlockStartEvent,
    ContentBlockStopEvent, InputContentBlock, InputMessage, MessageDeltaEvent, MessageRequest,
    OpenAiCompatClient, OpenAiCompatConfig, OutputContentBlock, ProviderClient, StreamEvent,
    ToolChoice, ToolDefinition,
};
use serde_json::json;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::Mutex;

#[tokio::test]
async fn send_message_uses_openai_compatible_endpoint_and_auth() {
    let state = Arc::new(Mutex::new(Vec::<CapturedRequest>::new()));
    let body = concat!(
        "{",
        "\"id\":\"chatcmpl_test\",",
        "\"model\":\"grok-3\",",
        "\"choices\":[{",
        "\"message\":{\"role\":\"assistant\",\"content\":\"Hello from Grok\",\"tool_calls\":[]},",
        "\"finish_reason\":\"stop\"",
        "}],",
        "\"usage\":{\"prompt_tokens\":11,\"completion_tokens\":5}",
        "}"
    );
    let server = spawn_server(
        state.clone(),
        vec![http_response("200 OK", "application/json", body)],
    )
    .await;

    let client = OpenAiCompatClient::new("xai-test-key", OpenAiCompatConfig::xai())
        .with_base_url(server.base_url());
    let response = client
        .send_message(&sample_request(false))
        .await
        .expect("request should succeed");

    assert_eq!(response.model, "grok-3");
    assert_eq!(response.total_tokens(), 16);
    assert_eq!(
        response.content,
        vec![OutputContentBlock::Text {
            text: "Hello from Grok".to_string(),
        }]
    );

    let captured = state.lock().await;
    let request = captured.first().expect("server should capture request");
    assert_eq!(request.path, "/chat/completions");
    assert_eq!(
        request.headers.get("authorization").map(String::as_str),
        Some("Bearer xai-test-key")
    );
    let body: serde_json::Value = serde_json::from_str(&request.body).expect("json body");
    assert_eq!(body["model"], json!("grok-3"));
    assert_eq!(body["messages"][0]["role"], json!("system"));
    assert_eq!(body["tools"][0]["type"], json!("function"));
}

#[tokio::test]
async fn send_message_blocks_oversized_xai_requests_before_the_http_call() {
    let state = Arc::new(Mutex::new(Vec::<CapturedRequest>::new()));
    let server = spawn_server(
        state.clone(),
        vec![http_response("200 OK", "application/json", "{}")],
    )
    .await;

    let client = OpenAiCompatClient::new("xai-test-key", OpenAiCompatConfig::xai())
        .with_base_url(server.base_url());
    let error = client
        .send_message(&MessageRequest {
            model: "grok-3".to_string(),
            max_tokens: 64_000,
            messages: vec![InputMessage {
                role: "user".to_string(),
                content: vec![InputContentBlock::Text {
                    text: "x".repeat(300_000),
                }],
            }],
            system: Some("Keep the answer short.".to_string()),
            tools: None,
            tool_choice: None,
            stream: false,
            ..Default::default()
        })
        .await
        .expect_err("oversized request should fail local context-window preflight");

    assert!(matches!(error, ApiError::ContextWindowExceeded { .. }));
    assert!(
        state.lock().await.is_empty(),
        "preflight failure should avoid any upstream HTTP request"
    );
}

#[tokio::test]
async fn send_message_accepts_full_chat_completions_endpoint_override() {
    let state = Arc::new(Mutex::new(Vec::<CapturedRequest>::new()));
    let body = concat!(
        "{",
        "\"id\":\"chatcmpl_full_endpoint\",",
        "\"model\":\"grok-3\",",
        "\"choices\":[{",
        "\"message\":{\"role\":\"assistant\",\"content\":\"Endpoint override works\",\"tool_calls\":[]},",
        "\"finish_reason\":\"stop\"",
        "}],",
        "\"usage\":{\"prompt_tokens\":7,\"completion_tokens\":3}",
        "}"
    );
    let server = spawn_server(
        state.clone(),
        vec![http_response("200 OK", "application/json", body)],
    )
    .await;

    let endpoint_url = format!("{}/chat/completions", server.base_url());
    let client = OpenAiCompatClient::new("xai-test-key", OpenAiCompatConfig::xai())
        .with_base_url(endpoint_url);
    let response = client
        .send_message(&sample_request(false))
        .await
        .expect("request should succeed");

    assert_eq!(response.total_tokens(), 10);

    let captured = state.lock().await;
    let request = captured.first().expect("server should capture request");
    assert_eq!(request.path, "/chat/completions");
}

#[tokio::test]
async fn stream_message_normalizes_text_and_multiple_tool_calls() {
    let state = Arc::new(Mutex::new(Vec::<CapturedRequest>::new()));
    let sse = concat!(
        "data: {\"id\":\"chatcmpl_stream\",\"model\":\"grok-3\",\"choices\":[{\"delta\":{\"content\":\"Hello\"}}]}\n\n",
        "data: {\"id\":\"chatcmpl_stream\",\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"call_1\",\"function\":{\"name\":\"weather\",\"arguments\":\"{\\\"city\\\":\\\"Paris\\\"}\"}},{\"index\":1,\"id\":\"call_2\",\"function\":{\"name\":\"clock\",\"arguments\":\"{\\\"zone\\\":\\\"UTC\\\"}\"}}]}}]}\n\n",
        "data: {\"id\":\"chatcmpl_stream\",\"choices\":[{\"delta\":{},\"finish_reason\":\"tool_calls\"}]}\n\n",
        "data: [DONE]\n\n"
    );
    let server = spawn_server(
        state.clone(),
        vec![http_response_with_headers(
            "200 OK",
            "text/event-stream",
            sse,
            &[("x-request-id", "req_grok_stream")],
        )],
    )
    .await;

    let client = OpenAiCompatClient::new("xai-test-key", OpenAiCompatConfig::xai())
        .with_base_url(server.base_url());
    let mut stream = client
        .stream_message(&sample_request(false))
        .await
        .expect("stream should start");

    assert_eq!(stream.request_id(), Some("req_grok_stream"));

    let mut events = Vec::new();
    while let Some(event) = stream.next_event().await.expect("event should parse") {
        events.push(event);
    }

    assert!(matches!(events[0], StreamEvent::MessageStart(_)));
    assert!(matches!(
        events[1],
        StreamEvent::ContentBlockStart(ContentBlockStartEvent {
            content_block: OutputContentBlock::Text { .. },
            ..
        })
    ));
    assert!(matches!(
        events[2],
        StreamEvent::ContentBlockDelta(ContentBlockDeltaEvent {
            delta: ContentBlockDelta::TextDelta { .. },
            ..
        })
    ));
    assert!(matches!(
        events[3],
        StreamEvent::ContentBlockStart(ContentBlockStartEvent {
            index: 1,
            content_block: OutputContentBlock::ToolUse { .. },
        })
    ));
    assert!(matches!(
        events[4],
        StreamEvent::ContentBlockDelta(ContentBlockDeltaEvent {
            index: 1,
            delta: ContentBlockDelta::InputJsonDelta { .. },
        })
    ));
    assert!(matches!(
        events[5],
        StreamEvent::ContentBlockStart(ContentBlockStartEvent {
            index: 2,
            content_block: OutputContentBlock::ToolUse { .. },
        })
    ));
    assert!(matches!(
        events[6],
        StreamEvent::ContentBlockDelta(ContentBlockDeltaEvent {
            index: 2,
            delta: ContentBlockDelta::InputJsonDelta { .. },
        })
    ));
    assert!(matches!(
        events[7],
        StreamEvent::ContentBlockStop(ContentBlockStopEvent { index: 1 })
    ));
    assert!(matches!(
        events[8],
        StreamEvent::ContentBlockStop(ContentBlockStopEvent { index: 2 })
    ));
    assert!(matches!(
        events[9],
        StreamEvent::ContentBlockStop(ContentBlockStopEvent { index: 0 })
    ));
    assert!(matches!(events[10], StreamEvent::MessageDelta(_)));
    assert!(matches!(events[11], StreamEvent::MessageStop(_)));

    let captured = state.lock().await;
    let request = captured.first().expect("captured request");
    assert_eq!(request.path, "/chat/completions");
    assert!(request.body.contains("\"stream\":true"));
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn openai_streaming_requests_opt_into_usage_chunks() {
    let state = Arc::new(Mutex::new(Vec::<CapturedRequest>::new()));
    let sse = concat!(
        "data: {\"id\":\"chatcmpl_openai_stream\",\"model\":\"gpt-5\",\"choices\":[{\"delta\":{\"content\":\"Hi\"}}]}\n\n",
        "data: {\"id\":\"chatcmpl_openai_stream\",\"choices\":[{\"delta\":{},\"finish_reason\":\"stop\"}]}\n\n",
        "data: {\"id\":\"chatcmpl_openai_stream\",\"choices\":[],\"usage\":{\"prompt_tokens\":9,\"completion_tokens\":4}}\n\n",
        "data: [DONE]\n\n"
    );
    let server = spawn_server(
        state.clone(),
        vec![http_response_with_headers(
            "200 OK",
            "text/event-stream",
            sse,
            &[("x-request-id", "req_openai_stream")],
        )],
    )
    .await;

    let client = OpenAiCompatClient::new("openai-test-key", OpenAiCompatConfig::openai())
        .with_base_url(server.base_url());
    let mut stream = client
        .stream_message(&sample_request(false))
        .await
        .expect("stream should start");

    assert_eq!(stream.request_id(), Some("req_openai_stream"));

    let mut events = Vec::new();
    while let Some(event) = stream.next_event().await.expect("event should parse") {
        events.push(event);
    }

    assert!(matches!(events[0], StreamEvent::MessageStart(_)));
    assert!(matches!(
        events[1],
        StreamEvent::ContentBlockStart(ContentBlockStartEvent {
            content_block: OutputContentBlock::Text { .. },
            ..
        })
    ));
    assert!(matches!(
        events[2],
        StreamEvent::ContentBlockDelta(ContentBlockDeltaEvent {
            delta: ContentBlockDelta::TextDelta { .. },
            ..
        })
    ));
    assert!(matches!(
        events[3],
        StreamEvent::ContentBlockStop(ContentBlockStopEvent { index: 0 })
    ));
    assert!(matches!(
        events[4],
        StreamEvent::MessageDelta(MessageDeltaEvent { .. })
    ));
    assert!(matches!(events[5], StreamEvent::MessageStop(_)));

    match &events[4] {
        StreamEvent::MessageDelta(MessageDeltaEvent { usage, .. }) => {
            assert_eq!(usage.input_tokens, 9);
            assert_eq!(usage.output_tokens, 4);
        }
        other => panic!("expected message delta, got {other:?}"),
    }

    let captured = state.lock().await;
    let request = captured.first().expect("captured request");
    assert_eq!(request.path, "/chat/completions");
    let body: serde_json::Value = serde_json::from_str(&request.body).expect("json body");
    assert_eq!(body["stream"], json!(true));
    assert_eq!(body["stream_options"], json!({"include_usage": true}));
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn provider_client_dispatches_xai_requests_from_env() {
    let _lock = env_lock();
    let _api_key = ScopedEnvVar::set("XAI_API_KEY", "xai-test-key");

    let state = Arc::new(Mutex::new(Vec::<CapturedRequest>::new()));
    let server = spawn_server(
        state.clone(),
        vec![http_response(
            "200 OK",
            "application/json",
            "{\"id\":\"chatcmpl_provider\",\"model\":\"grok-3\",\"choices\":[{\"message\":{\"role\":\"assistant\",\"content\":\"Through provider client\",\"tool_calls\":[]},\"finish_reason\":\"stop\"}],\"usage\":{\"prompt_tokens\":9,\"completion_tokens\":4}}",
        )],
    )
    .await;
    let _base_url = ScopedEnvVar::set("XAI_BASE_URL", server.base_url());

    let client =
        ProviderClient::from_model("grok").expect("xAI provider client should be constructed");
    assert!(matches!(client, ProviderClient::Xai(_)));

    let response = client
        .send_message(&sample_request(false))
        .await
        .expect("provider-dispatched request should succeed");

    assert_eq!(response.total_tokens(), 13);

    let captured = state.lock().await;
    let request = captured.first().expect("captured request");
    assert_eq!(request.path, "/chat/completions");
    assert_eq!(
        request.headers.get("authorization").map(String::as_str),
        Some("Bearer xai-test-key")
    );
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn openai_compat_sends_optional_broker_caller_headers() {
    let _lock = env_lock();
    let _caller = ScopedEnvVar::set("RUSTY_CLAUDE_LLM_CALLER", "stack-code");
    let _task = ScopedEnvVar::set("RUSTY_CLAUDE_TASK_TYPE", "code");

    let state = Arc::new(Mutex::new(Vec::<CapturedRequest>::new()));
    let body = concat!(
        "{",
        "\"id\":\"chatcmpl_broker_headers\",",
        "\"model\":\"grok-3\",",
        "\"choices\":[{",
        "\"message\":{\"role\":\"assistant\",\"content\":\"ok\",\"tool_calls\":[]},",
        "\"finish_reason\":\"stop\"",
        "}],",
        "\"usage\":{\"prompt_tokens\":1,\"completion_tokens\":1}",
        "}"
    );
    let server = spawn_server(
        state.clone(),
        vec![http_response("200 OK", "application/json", body)],
    )
    .await;

    let client = OpenAiCompatClient::new("xai-test-key", OpenAiCompatConfig::xai())
        .with_base_url(server.base_url());
    client
        .send_message(&sample_request(false))
        .await
        .expect("request should succeed");

    let captured = state.lock().await;
    let request = captured.first().expect("captured request");
    assert_eq!(
        request.headers.get("x-llm-caller").map(String::as_str),
        Some("stack-code"),
        "X-LLM-Caller header should be forwarded when env var is set"
    );
    assert_eq!(
        request.headers.get("x-task-type").map(String::as_str),
        Some("code"),
        "X-Task-Type header should be forwarded when env var is set"
    );
    assert_eq!(
        request.headers.get("authorization").map(String::as_str),
        Some("Bearer xai-test-key"),
        "existing bearer auth must remain"
    );
    assert_eq!(
        request.headers.get("content-type").map(String::as_str),
        Some("application/json"),
        "existing content-type must remain"
    );
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn openai_compat_omits_blank_broker_caller_headers() {
    let _lock = env_lock();
    let _caller = ScopedEnvVar::set("RUSTY_CLAUDE_LLM_CALLER", "");
    let _task = ScopedEnvVar::set("RUSTY_CLAUDE_TASK_TYPE", "   ");

    let state = Arc::new(Mutex::new(Vec::<CapturedRequest>::new()));
    let body = concat!(
        "{",
        "\"id\":\"chatcmpl_broker_blank\",",
        "\"model\":\"grok-3\",",
        "\"choices\":[{",
        "\"message\":{\"role\":\"assistant\",\"content\":\"ok\",\"tool_calls\":[]},",
        "\"finish_reason\":\"stop\"",
        "}],",
        "\"usage\":{\"prompt_tokens\":1,\"completion_tokens\":1}",
        "}"
    );
    let server = spawn_server(
        state.clone(),
        vec![http_response("200 OK", "application/json", body)],
    )
    .await;

    let client = OpenAiCompatClient::new("xai-test-key", OpenAiCompatConfig::xai())
        .with_base_url(server.base_url());
    client
        .send_message(&sample_request(false))
        .await
        .expect("request should succeed");

    let captured = state.lock().await;
    let request = captured.first().expect("captured request");
    assert!(
        !request.headers.contains_key("x-llm-caller"),
        "blank RUSTY_CLAUDE_LLM_CALLER must not produce a header"
    );
    assert!(
        !request.headers.contains_key("x-task-type"),
        "whitespace-only RUSTY_CLAUDE_TASK_TYPE must not produce a header"
    );
    assert_eq!(
        request.headers.get("authorization").map(String::as_str),
        Some("Bearer xai-test-key"),
        "existing bearer auth must remain even with broker env vars blank"
    );
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn openai_compat_resolves_model_alias_from_env() {
    let _lock = env_lock();
    let _alias = ScopedEnvVar::set("RUSTY_CLAUDE_MODEL_ALIAS__FAST", "qwen3:14b");

    let state = Arc::new(Mutex::new(Vec::<CapturedRequest>::new()));
    let body = concat!(
        "{",
        "\"id\":\"chatcmpl_alias_resolved\",",
        "\"model\":\"qwen3:14b\",",
        "\"choices\":[{",
        "\"message\":{\"role\":\"assistant\",\"content\":\"ok\",\"tool_calls\":[]},",
        "\"finish_reason\":\"stop\"",
        "}],",
        "\"usage\":{\"prompt_tokens\":1,\"completion_tokens\":1}",
        "}"
    );
    let server = spawn_server(
        state.clone(),
        vec![http_response("200 OK", "application/json", body)],
    )
    .await;

    let client = OpenAiCompatClient::new("xai-test-key", OpenAiCompatConfig::xai())
        .with_base_url(server.base_url());
    client
        .send_message(&aliased_request("fast"))
        .await
        .expect("request should succeed");

    let captured = state.lock().await;
    let request = captured.first().expect("captured request");
    let body: serde_json::Value = serde_json::from_str(&request.body).expect("json body");
    assert_eq!(
        body["model"],
        json!("qwen3:14b"),
        "alias `fast` should be rewritten to `qwen3:14b` on the wire"
    );
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn openai_compat_leaves_model_unchanged_without_alias() {
    let _lock = env_lock();
    let _alias = ScopedEnvVar::remove("RUSTY_CLAUDE_MODEL_ALIAS__FAST");

    let state = Arc::new(Mutex::new(Vec::<CapturedRequest>::new()));
    let body = concat!(
        "{",
        "\"id\":\"chatcmpl_alias_absent\",",
        "\"model\":\"fast\",",
        "\"choices\":[{",
        "\"message\":{\"role\":\"assistant\",\"content\":\"ok\",\"tool_calls\":[]},",
        "\"finish_reason\":\"stop\"",
        "}],",
        "\"usage\":{\"prompt_tokens\":1,\"completion_tokens\":1}",
        "}"
    );
    let server = spawn_server(
        state.clone(),
        vec![http_response("200 OK", "application/json", body)],
    )
    .await;

    let client = OpenAiCompatClient::new("xai-test-key", OpenAiCompatConfig::xai())
        .with_base_url(server.base_url());
    client
        .send_message(&aliased_request("fast"))
        .await
        .expect("request should succeed");

    let captured = state.lock().await;
    let request = captured.first().expect("captured request");
    let body: serde_json::Value = serde_json::from_str(&request.body).expect("json body");
    assert_eq!(
        body["model"],
        json!("fast"),
        "model string must be preserved when no alias env var is set"
    );
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn openai_compat_ignores_blank_alias_value() {
    let _lock = env_lock();
    let _alias = ScopedEnvVar::set("RUSTY_CLAUDE_MODEL_ALIAS__FAST", "  ");

    let state = Arc::new(Mutex::new(Vec::<CapturedRequest>::new()));
    let body = concat!(
        "{",
        "\"id\":\"chatcmpl_alias_blank\",",
        "\"model\":\"fast\",",
        "\"choices\":[{",
        "\"message\":{\"role\":\"assistant\",\"content\":\"ok\",\"tool_calls\":[]},",
        "\"finish_reason\":\"stop\"",
        "}],",
        "\"usage\":{\"prompt_tokens\":1,\"completion_tokens\":1}",
        "}"
    );
    let server = spawn_server(
        state.clone(),
        vec![http_response("200 OK", "application/json", body)],
    )
    .await;

    let client = OpenAiCompatClient::new("xai-test-key", OpenAiCompatConfig::xai())
        .with_base_url(server.base_url());
    client
        .send_message(&aliased_request("fast"))
        .await
        .expect("request should succeed");

    let captured = state.lock().await;
    let request = captured.first().expect("captured request");
    let body: serde_json::Value = serde_json::from_str(&request.body).expect("json body");
    assert_eq!(
        body["model"],
        json!("fast"),
        "whitespace-only alias value must be treated as unset"
    );
}

// ---------------------------------------------------------------------------
// Unframed (non-SSE) answers to a streaming request
//
// A gateway can accept `stream: true` and answer with the complete
// `chat.completion` object as a plain JSON body instead of an SSE sequence.
// The SideStackAI broker does exactly this on its Ollama-backed
// `/v1/chat/completions` path: it hardcodes `stream: false` toward Ollama and
// returns a single `JSONResponse`. That body carries no `\n\n` frame
// separator, so it never becomes an SSE frame.
//
// The stream must still deliver the answer that the POST already produced. An
// empty stream here is not merely lossy: it makes callers re-issue an
// inference that has already executed.
// ---------------------------------------------------------------------------

/// The exact envelope `broker.py` returns for an Ollama-backed completion:
/// compact JSON, `object: chat.completion`, no frame separator anywhere.
const BROKER_UNFRAMED_COMPLETION: &str = concat!(
    "{\"id\":\"chatcmpl-ollama-1757000000\",\"object\":\"chat.completion\",",
    "\"created\":1757000000,\"model\":\"qwen3:14b\",",
    "\"choices\":[{\"index\":0,",
    "\"message\":{\"role\":\"assistant\",\"content\":\"PR180_SMOKE_OK\"},",
    "\"finish_reason\":\"stop\"}],",
    "\"usage\":{\"prompt_tokens\":11,\"completion_tokens\":5,\"total_tokens\":16}}"
);

#[tokio::test]
async fn stream_message_recovers_an_unframed_completion_body() {
    let state = Arc::new(Mutex::new(Vec::<CapturedRequest>::new()));
    // Two responses are queued so a second POST would be served rather than
    // hang. The assertion that only one was captured is therefore a real
    // statement about the client, not an artifact of the fixture.
    let server = spawn_server(
        state.clone(),
        vec![
            http_response("200 OK", "application/json", BROKER_UNFRAMED_COMPLETION),
            http_response("200 OK", "application/json", BROKER_UNFRAMED_COMPLETION),
        ],
    )
    .await;

    let client = OpenAiCompatClient::new("test-key", OpenAiCompatConfig::openai())
        .with_base_url(server.base_url());
    let mut stream = client
        .stream_message(&sample_request(true))
        .await
        .expect("stream should start");

    let mut events = Vec::new();
    while let Some(event) = stream.next_event().await.expect("event should parse") {
        events.push(event);
    }

    assert!(
        matches!(events.first(), Some(StreamEvent::MessageStart(_))),
        "recovered stream must open with message_start, got {events:?}"
    );
    assert!(
        events.iter().any(|event| matches!(
            event,
            StreamEvent::ContentBlockDelta(ContentBlockDeltaEvent {
                delta: ContentBlockDelta::TextDelta { text },
                ..
            }) if text == "PR180_SMOKE_OK"
        )),
        "recovered stream must carry the assistant text, got {events:?}"
    );
    assert!(
        matches!(events.last(), Some(StreamEvent::MessageStop(_))),
        "recovered stream must terminate with message_stop, got {events:?}"
    );

    let stop_reason = events.iter().find_map(|event| match event {
        StreamEvent::MessageDelta(MessageDeltaEvent { delta, .. }) => delta.stop_reason.clone(),
        _ => None,
    });
    assert_eq!(stop_reason.as_deref(), Some("end_turn"));

    assert_eq!(
        state.lock().await.len(),
        1,
        "recovering the body must not cost a second inference POST"
    );
}

#[tokio::test]
async fn stream_message_recovers_tool_calls_from_an_unframed_completion_body() {
    let state = Arc::new(Mutex::new(Vec::<CapturedRequest>::new()));
    let body = concat!(
        "{\"id\":\"chatcmpl-ollama-1757000001\",\"object\":\"chat.completion\",",
        "\"model\":\"qwen3:14b\",\"choices\":[{\"index\":0,\"message\":{",
        "\"role\":\"assistant\",\"content\":\"\",\"tool_calls\":[{\"id\":\"call_1\",",
        "\"type\":\"function\",\"function\":{\"name\":\"weather\",",
        "\"arguments\":\"{\\\"city\\\":\\\"Paris\\\"}\"}}]},",
        "\"finish_reason\":\"tool_calls\"}],",
        "\"usage\":{\"prompt_tokens\":9,\"completion_tokens\":4}}"
    );
    let server = spawn_server(
        state.clone(),
        vec![
            http_response("200 OK", "application/json", body),
            http_response("200 OK", "application/json", body),
        ],
    )
    .await;

    let client = OpenAiCompatClient::new("test-key", OpenAiCompatConfig::openai())
        .with_base_url(server.base_url());
    let mut stream = client
        .stream_message(&sample_request(true))
        .await
        .expect("stream should start");

    let mut events = Vec::new();
    while let Some(event) = stream.next_event().await.expect("event should parse") {
        events.push(event);
    }

    assert!(
        events.iter().any(|event| matches!(
            event,
            StreamEvent::ContentBlockStart(ContentBlockStartEvent {
                content_block: OutputContentBlock::ToolUse { name, .. },
                ..
            }) if name == "weather"
        )),
        "recovered stream must open the tool block, got {events:?}"
    );
    assert!(
        events.iter().any(|event| matches!(
            event,
            StreamEvent::ContentBlockDelta(ContentBlockDeltaEvent {
                delta: ContentBlockDelta::InputJsonDelta { partial_json },
                ..
            }) if partial_json == "{\"city\":\"Paris\"}"
        )),
        "recovered stream must carry the tool arguments, got {events:?}"
    );
    assert!(
        events
            .iter()
            .any(|event| matches!(event, StreamEvent::ContentBlockStop(_))),
        "recovered tool block must be closed, got {events:?}"
    );
    assert!(
        matches!(events.last(), Some(StreamEvent::MessageStop(_))),
        "recovered stream must terminate with message_stop, got {events:?}"
    );

    let stop_reason = events.iter().find_map(|event| match event {
        StreamEvent::MessageDelta(MessageDeltaEvent { delta, .. }) => delta.stop_reason.clone(),
        _ => None,
    });
    assert_eq!(stop_reason.as_deref(), Some("tool_use"));
    assert_eq!(state.lock().await.len(), 1);
}

/// Recovery is gated on the stream having produced nothing. A well-formed SSE
/// sequence is decoded exactly as before and reaches EOF with a drained
/// buffer, so the recovery path is never consulted.
#[tokio::test]
async fn stream_message_keeps_framed_sse_decoding_unchanged() {
    let state = Arc::new(Mutex::new(Vec::<CapturedRequest>::new()));
    let sse = concat!(
        "data: {\"id\":\"chatcmpl_stream\",\"model\":\"qwen3:14b\",\"choices\":[{\"delta\":{\"content\":\"Hello\"}}]}\n\n",
        "data: {\"id\":\"chatcmpl_stream\",\"choices\":[{\"delta\":{},\"finish_reason\":\"stop\"}]}\n\n",
        "data: [DONE]\n\n"
    );
    let server = spawn_server(
        state.clone(),
        vec![
            http_response("200 OK", "text/event-stream", sse),
            http_response("200 OK", "text/event-stream", sse),
        ],
    )
    .await;

    let client = OpenAiCompatClient::new("test-key", OpenAiCompatConfig::openai())
        .with_base_url(server.base_url());
    let mut stream = client
        .stream_message(&sample_request(true))
        .await
        .expect("stream should start");

    let mut events = Vec::new();
    while let Some(event) = stream.next_event().await.expect("event should parse") {
        events.push(event);
    }

    let text: String = events
        .iter()
        .filter_map(|event| match event {
            StreamEvent::ContentBlockDelta(ContentBlockDeltaEvent {
                delta: ContentBlockDelta::TextDelta { text },
                ..
            }) => Some(text.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(text, "Hello");
    assert_eq!(
        events
            .iter()
            .filter(|event| matches!(event, StreamEvent::MessageStart(_)))
            .count(),
        1,
        "a framed stream must not gain a duplicate message_start, got {events:?}"
    );
    assert!(matches!(events.last(), Some(StreamEvent::MessageStop(_))));
    assert_eq!(state.lock().await.len(), 1);
}

// ---------------------------------------------------------------------------
// Stream completion proof
//
// EOF is not proof that a partially consumed streaming response completed. A
// stream that delivered a valid frame and then ended mid-frame is missing the
// bytes that would have carried the rest of the answer, its `finish_reason`
// and its `[DONE]` sentinel. Synthesising a terminal event there reports a
// truncated turn as a successful one, which is worse than failing: the caller
// records a short answer as the model's answer.
//
// These cases drive the real client against a local server. Two identical
// responses are queued for every case, so a second POST would be served rather
// than hang: the recorded request count is a statement about the client, not
// an artefact of a starved fixture.
// ---------------------------------------------------------------------------

/// What one streaming turn produced: the events delivered before the stream
/// ended, how it ended, and how many physical POSTs it cost.
struct StreamOutcome {
    events: Vec<StreamEvent>,
    error: Option<ApiError>,
    requests: usize,
}

impl StreamOutcome {
    /// The assistant text delivered to the caller, in order.
    fn text(&self) -> String {
        self.events
            .iter()
            .filter_map(|event| match event {
                StreamEvent::ContentBlockDelta(ContentBlockDeltaEvent {
                    delta: ContentBlockDelta::TextDelta { text },
                    ..
                }) => Some(text.as_str()),
                _ => None,
            })
            .collect()
    }

    fn has_message_stop(&self) -> bool {
        self.events
            .iter()
            .any(|event| matches!(event, StreamEvent::MessageStop(_)))
    }

    /// Whether any tool-use block was opened for the caller.
    fn has_tool_use(&self) -> bool {
        self.events.iter().any(|event| {
            matches!(
                event,
                StreamEvent::ContentBlockStart(ContentBlockStartEvent {
                    content_block: OutputContentBlock::ToolUse { .. },
                    ..
                })
            )
        })
    }

    /// How many terminal `message_stop` events reached the caller. A stream
    /// may end exactly once.
    fn message_stop_count(&self) -> usize {
        self.events
            .iter()
            .filter(|event| matches!(event, StreamEvent::MessageStop(_)))
            .count()
    }

    fn has_message_delta(&self) -> bool {
        self.events
            .iter()
            .any(|event| matches!(event, StreamEvent::MessageDelta(_)))
    }

    fn stop_reason(&self) -> Option<String> {
        self.events.iter().find_map(|event| match event {
            StreamEvent::MessageDelta(MessageDeltaEvent { delta, .. }) => delta.stop_reason.clone(),
            _ => None,
        })
    }

    /// Asserts the turn failed as an incomplete stream and cost exactly one
    /// inference POST, and returns the failure detail for further assertions.
    fn expect_incomplete(&self) -> &'static str {
        assert_eq!(
            self.requests, 1,
            "an incomplete stream must not cost a second inference POST"
        );
        assert!(
            !self.has_message_stop(),
            "an incomplete stream must not synthesise message_stop, got {:?}",
            self.events
        );
        assert!(
            !self.has_message_delta(),
            "an incomplete stream must not synthesise a terminal usage delta, got {:?}",
            self.events
        );
        match self.error.as_ref() {
            Some(ApiError::InvalidSseFrame(detail)) => detail,
            other => panic!(
                "an incomplete stream must fail with a typed stream-protocol error, got {other:?} \
                 after events {:?}",
                self.events
            ),
        }
    }

    /// Asserts the turn completed normally at the cost of one inference POST.
    fn expect_complete(&self) -> &[StreamEvent] {
        assert!(
            self.error.is_none(),
            "a complete stream must not fail, got {:?}",
            self.error
        );
        assert_eq!(
            self.requests, 1,
            "one logical turn must cost exactly one physical inference POST"
        );
        assert!(
            matches!(self.events.first(), Some(StreamEvent::MessageStart(_))),
            "a complete stream must open with message_start, got {:?}",
            self.events
        );
        assert!(
            matches!(self.events.last(), Some(StreamEvent::MessageStop(_))),
            "a complete stream must terminate with message_stop, got {:?}",
            self.events
        );
        &self.events
    }
}

/// Runs one streaming turn against a local server that answers with `body`.
async fn run_stream_case(body: &str, content_type: &str) -> StreamOutcome {
    let state = Arc::new(Mutex::new(Vec::<CapturedRequest>::new()));
    let server = spawn_server(
        state.clone(),
        vec![
            http_response("200 OK", content_type, body),
            http_response("200 OK", content_type, body),
        ],
    )
    .await;

    let client = OpenAiCompatClient::new("test-key", OpenAiCompatConfig::openai())
        .with_base_url(server.base_url());
    let mut stream = client
        .stream_message(&sample_request(true))
        .await
        .expect("stream should start");

    let mut events = Vec::new();
    let error = loop {
        match stream.next_event().await {
            Ok(Some(event)) => events.push(event),
            Ok(None) => break None,
            Err(error) => break Some(error),
        }
    };

    let requests = state.lock().await.len();
    StreamOutcome {
        events,
        error,
        requests,
    }
}

/// The reviewer's blocker: a complete first frame followed by a `data:` line
/// that stops mid-JSON with no frame terminator — what a connection dropped
/// mid-stream leaves behind.
const TRUNCATED_TRAILING_FRAME: &str = concat!(
    "data: {\"id\":\"chatcmpl_stream\",\"model\":\"qwen3:14b\",\"choices\":[{\"delta\":{\"content\":\"partial\"}}]}\n\n",
    "data: {\"id\":\"chatcmpl_stream\",\"choices\":[{\"delta\":{\"content\":\"lost"
);

#[tokio::test]
async fn stream_message_rejects_a_truncated_trailing_frame() {
    let outcome = run_stream_case(TRUNCATED_TRAILING_FRAME, "text/event-stream").await;
    let detail = outcome.expect_incomplete();
    assert!(
        detail.contains("truncated"),
        "the failure must name the truncation, got {detail:?}"
    );
}

// ---------------------------------------------------------------------------
// Adversarial framing matrix
//
// Twenty wire shapes a 2xx streaming response can take, each pinned to the one
// question that matters at EOF: did this response prove it completed? Every
// shape that cannot prove it must fail with a typed error, deliver no
// synthesised terminal event, and cost exactly one physical inference POST.
// ---------------------------------------------------------------------------

/// A complete `chat.completion` object, exactly as a gateway that ignores
/// `stream: true` returns it: compact, with no frame separator anywhere.
const COMPACT_TEXT_COMPLETION: &str = concat!(
    "{\"id\":\"chatcmpl-compact\",\"object\":\"chat.completion\",\"model\":\"qwen3:14b\",",
    "\"choices\":[{\"index\":0,\"message\":{\"role\":\"assistant\",\"content\":\"Hello\"},",
    "\"finish_reason\":\"stop\"}],",
    "\"usage\":{\"prompt_tokens\":11,\"completion_tokens\":5}}"
);

/// One well-formed SSE text frame, used as the valid prefix every truncation
/// case is built on so the truncation is the only variable.
const VALID_TEXT_FRAME: &str = concat!(
    "data: {\"id\":\"chatcmpl_stream\",\"model\":\"qwen3:14b\",",
    "\"choices\":[{\"delta\":{\"content\":\"partial\"}}]}\n\n"
);

// --- Compact non-SSE -------------------------------------------------------

/// Case 1. The historical duplicate-inference shape. The answer the POST
/// already produced must be delivered, not re-requested.
#[tokio::test]
async fn case01_compact_text_completion_completes() {
    let outcome = run_stream_case(COMPACT_TEXT_COMPLETION, "application/json").await;
    outcome.expect_complete();
    assert_eq!(outcome.text(), "Hello");
    assert_eq!(outcome.stop_reason().as_deref(), Some("end_turn"));
}

/// Case 2. The same recovery must carry tool calls, not just text.
#[tokio::test]
async fn case02_compact_tool_call_completion_completes() {
    let body = concat!(
        "{\"id\":\"chatcmpl-compact-tool\",\"object\":\"chat.completion\",\"model\":\"qwen3:14b\",",
        "\"choices\":[{\"index\":0,\"message\":{\"role\":\"assistant\",\"content\":\"\",",
        "\"tool_calls\":[{\"id\":\"call_1\",\"type\":\"function\",\"function\":{",
        "\"name\":\"weather\",\"arguments\":\"{\\\"city\\\":\\\"Paris\\\"}\"}}]},",
        "\"finish_reason\":\"tool_calls\"}]}"
    );
    let outcome = run_stream_case(body, "application/json").await;
    outcome.expect_complete();
    assert!(
        outcome.events.iter().any(|event| matches!(
            event,
            StreamEvent::ContentBlockStart(ContentBlockStartEvent {
                content_block: OutputContentBlock::ToolUse { name, .. },
                ..
            }) if name == "weather"
        )),
        "the recovered tool call must reach the caller, got {:?}",
        outcome.events
    );
    assert_eq!(outcome.stop_reason().as_deref(), Some("tool_use"));
}

/// Case 3. Recovery is offered the whole body and trims it, so padding around
/// the object — including blank lines, which the frame scanner drains as empty
/// frames carrying no `data:` payload — does not hide the completion.
#[tokio::test]
async fn case03_compact_completion_with_surrounding_whitespace_completes() {
    let body = format!("\n\n  {COMPACT_TEXT_COMPLETION}  \n\n");
    let outcome = run_stream_case(&body, "application/json").await;
    outcome.expect_complete();
    assert_eq!(outcome.text(), "Hello");
}

/// Case 4. A body that is neither framed nor a decodable completion proves
/// nothing, so the turn fails instead of ending quietly.
#[tokio::test]
async fn case04_compact_malformed_json_fails() {
    let outcome =
        run_stream_case("{\"id\":\"chatcmpl-cut\",\"choices\":[", "application/json").await;
    let detail = outcome.expect_incomplete();
    assert!(
        detail.contains("not a complete chat.completion"),
        "the failure must name the undecodable body, got {detail:?}"
    );
}

/// Case 5. Recovery decodes the entire body, so an object followed by trailing
/// bytes is not a completion: something else was on the wire.
#[tokio::test]
async fn case05_compact_json_with_trailing_garbage_fails() {
    let body = format!("{COMPACT_TEXT_COMPLETION} unexpected-trailer");
    let outcome = run_stream_case(&body, "application/json").await;
    assert!(outcome
        .expect_incomplete()
        .contains("not a complete chat.completion"));
}

// --- SSE -------------------------------------------------------------------

/// Case 6. The ordinary single-frame stream.
#[tokio::test]
async fn case06_one_framed_text_event_completes() {
    let body = concat!(
        "data: {\"id\":\"s\",\"model\":\"qwen3:14b\",\"choices\":[{\"delta\":{\"content\":\"Hello\"}}]}\n\n",
        "data: {\"id\":\"s\",\"choices\":[{\"delta\":{},\"finish_reason\":\"stop\"}]}\n\n",
        "data: [DONE]\n\n"
    );
    let outcome = run_stream_case(body, "text/event-stream").await;
    outcome.expect_complete();
    assert_eq!(outcome.text(), "Hello");
    assert_eq!(outcome.stop_reason().as_deref(), Some("end_turn"));
}

/// Case 7. Several content frames arrive in order and concatenate.
#[tokio::test]
async fn case07_multiple_framed_text_events_complete() {
    let body = concat!(
        "data: {\"id\":\"s\",\"model\":\"qwen3:14b\",\"choices\":[{\"delta\":{\"content\":\"He\"}}]}\n\n",
        "data: {\"id\":\"s\",\"choices\":[{\"delta\":{\"content\":\"llo w\"}}]}\n\n",
        "data: {\"id\":\"s\",\"choices\":[{\"delta\":{\"content\":\"orld\"}}]}\n\n",
        "data: {\"id\":\"s\",\"choices\":[{\"delta\":{},\"finish_reason\":\"stop\"}]}\n\n",
        "data: [DONE]\n\n"
    );
    let outcome = run_stream_case(body, "text/event-stream").await;
    outcome.expect_complete();
    assert_eq!(outcome.text(), "Hello world");
}

/// Case 8. `\r\n\r\n` separates frames as well as `\n\n`, and the terminator
/// is stripped rather than left on the frame text.
#[tokio::test]
async fn case08_crlf_frame_separators_complete() {
    let body = concat!(
        "data: {\"id\":\"s\",\"model\":\"qwen3:14b\",\"choices\":[{\"delta\":{\"content\":\"Hello\"}}]}\r\n\r\n",
        "data: {\"id\":\"s\",\"choices\":[{\"delta\":{},\"finish_reason\":\"stop\"}]}\r\n\r\n",
        "data: [DONE]\r\n\r\n"
    );
    let outcome = run_stream_case(body, "text/event-stream").await;
    outcome.expect_complete();
    assert_eq!(outcome.text(), "Hello");
}

/// Case 9. A framed tool-use sequence terminates on `finish_reason:
/// tool_calls` and yields the call intact.
#[tokio::test]
async fn case09_framed_tool_use_sequence_completes() {
    let body = concat!(
        "data: {\"id\":\"s\",\"model\":\"qwen3:14b\",\"choices\":[{\"delta\":{\"tool_calls\":[{",
        "\"index\":0,\"id\":\"call_1\",\"function\":{\"name\":\"weather\",\"arguments\":\"{\\\"city\\\":\"}}]}}]}\n\n",
        "data: {\"id\":\"s\",\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,",
        "\"function\":{\"arguments\":\"\\\"Paris\\\"}\"}}]}}]}\n\n",
        "data: {\"id\":\"s\",\"choices\":[{\"delta\":{},\"finish_reason\":\"tool_calls\"}]}\n\n",
        "data: [DONE]\n\n"
    );
    let outcome = run_stream_case(body, "text/event-stream").await;
    outcome.expect_complete();
    let arguments: String = outcome
        .events
        .iter()
        .filter_map(|event| match event {
            StreamEvent::ContentBlockDelta(ContentBlockDeltaEvent {
                delta: ContentBlockDelta::InputJsonDelta { partial_json },
                ..
            }) => Some(partial_json.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(arguments, "{\"city\":\"Paris\"}");
    assert_eq!(outcome.stop_reason().as_deref(), Some("tool_use"));
}

/// Case 10. `[DONE]` alone proves completion. A backend that omits
/// `finish_reason` still terminated the stream deliberately, and the stop
/// reason falls back to the ordinary end of a turn.
#[tokio::test]
async fn case10_done_sentinel_without_finish_reason_completes() {
    let body = concat!(
        "data: {\"id\":\"s\",\"model\":\"qwen3:14b\",\"choices\":[{\"delta\":{\"content\":\"Hello\"}}]}\n\n",
        "data: [DONE]\n\n"
    );
    let outcome = run_stream_case(body, "text/event-stream").await;
    outcome.expect_complete();
    assert_eq!(outcome.text(), "Hello");
    assert_eq!(outcome.stop_reason().as_deref(), Some("end_turn"));
}

/// Case 11. A usage-only frame after `finish_reason` is legal — it is what
/// `stream_options.include_usage` adds — and must not be read as a truncation.
#[tokio::test]
async fn case11_final_usage_only_event_completes() {
    let body = concat!(
        "data: {\"id\":\"s\",\"model\":\"qwen3:14b\",\"choices\":[{\"delta\":{\"content\":\"Hello\"}}]}\n\n",
        "data: {\"id\":\"s\",\"choices\":[{\"delta\":{},\"finish_reason\":\"stop\"}]}\n\n",
        "data: {\"id\":\"s\",\"choices\":[],\"usage\":{\"prompt_tokens\":9,\"completion_tokens\":4}}\n\n",
        "data: [DONE]\n\n"
    );
    let outcome = run_stream_case(body, "text/event-stream").await;
    let events = outcome.expect_complete();
    let usage = events.iter().find_map(|event| match event {
        StreamEvent::MessageDelta(MessageDeltaEvent { usage, .. }) => Some(usage.clone()),
        _ => None,
    });
    assert_eq!(usage.map(|usage| usage.output_tokens), Some(4));
}

// --- Truncation / ambiguity ------------------------------------------------

/// Case 12. The next frame began and stopped at its `data:` prefix.
#[tokio::test]
async fn case12_truncated_next_data_line_fails() {
    let body = format!("{VALID_TEXT_FRAME}data: ");
    let outcome = run_stream_case(&body, "text/event-stream").await;
    assert!(outcome.expect_incomplete().contains("truncated"));
    assert_eq!(
        outcome.text(),
        "partial",
        "text delivered before the cut stays delivered; only the turn fails"
    );
}

/// Case 13. The next frame's JSON stops mid-object.
#[tokio::test]
async fn case13_partial_json_frame_fails() {
    let body = format!("{VALID_TEXT_FRAME}data: {{\"id\":\"s\",\"choices\":[{{\"delta\"");
    let outcome = run_stream_case(&body, "text/event-stream").await;
    assert!(outcome.expect_incomplete().contains("truncated"));
}

/// Case 14. A final frame that is complete and well-formed JSON but never
/// terminated is still unread: without its separator there is no frame, and
/// its content must not reach the caller.
#[tokio::test]
async fn case14_missing_frame_terminator_fails() {
    let body = format!(
        "{VALID_TEXT_FRAME}data: {{\"id\":\"s\",\"choices\":[{{\"delta\":{{\"content\":\"lost\"}},\"finish_reason\":\"stop\"}}]}}"
    );
    let outcome = run_stream_case(&body, "text/event-stream").await;
    assert!(outcome.expect_incomplete().contains("truncated"));
    assert_eq!(
        outcome.text(),
        "partial",
        "an unterminated frame must not be decoded, got {:?}",
        outcome.events
    );
}

/// Case 15. Whitespace between the last complete frame and the cut does not
/// make the leftover bytes a clean ending.
#[tokio::test]
async fn case15_whitespace_then_truncated_bytes_fails() {
    let body = format!("{VALID_TEXT_FRAME}  \n data: {{\"id\"");
    let outcome = run_stream_case(&body, "text/event-stream").await;
    assert!(outcome.expect_incomplete().contains("truncated"));
}

/// Case 16. A body that is only the start of a frame framed nothing, so it is
/// judged as an unframed body — and it is not a completion either.
#[tokio::test]
async fn case16_partial_initial_frame_only_fails() {
    let outcome = run_stream_case("data: {\"id\":\"s\",\"cho", "text/event-stream").await;
    assert!(outcome
        .expect_incomplete()
        .contains("not a complete chat.completion"));
}

/// Case 17. Once a `data:` frame has been consumed, the leftover bytes belong
/// to a frame that never arrived. They must not be re-read as a standalone
/// compact completion, however well-formed they look on their own.
#[tokio::test]
async fn case17_sse_prefix_then_compact_json_fails() {
    let body = format!("{VALID_TEXT_FRAME}{COMPACT_TEXT_COMPLETION}");
    let outcome = run_stream_case(&body, "text/event-stream").await;
    assert!(outcome.expect_incomplete().contains("truncated"));
    assert_eq!(
        outcome.text(),
        "partial",
        "the trailing object must not be decoded as a completion, got {:?}",
        outcome.events
    );
}

/// Case 18. Trailing non-whitespace after `[DONE]` still fails, and now says
/// why precisely: the body contradicted its own terminal marker. Reading it as
/// junk to ignore would mean accepting, unverified, that nothing further was
/// owed.
#[tokio::test]
async fn case18_complete_stream_then_garbage_fails() {
    let body = concat!(
        "data: {\"id\":\"s\",\"model\":\"qwen3:14b\",\"choices\":[{\"delta\":{\"content\":\"Hello\"}}]}\n\n",
        "data: {\"id\":\"s\",\"choices\":[{\"delta\":{},\"finish_reason\":\"stop\"}]}\n\n",
        "data: [DONE]\n\n",
        "unexpected-trailer"
    );
    let outcome = run_stream_case(body, "text/event-stream").await;
    assert!(outcome.expect_incomplete().contains("[DONE]"));
}

// --- Empty -----------------------------------------------------------------

/// Case 19. A zero-length 2xx body. The inference already ran, so this is
/// reported as the incomplete stream it is rather than re-sent.
#[tokio::test]
async fn case19_zero_length_body_fails() {
    let outcome = run_stream_case("", "application/json").await;
    assert!(outcome.expect_incomplete().contains("empty body"));
    assert!(
        outcome.events.is_empty(),
        "an empty body must not synthesise events, got {:?}",
        outcome.events
    );
}

/// Case 20. Whitespace, including bare frame separators, carries no content
/// and no proof of completion.
#[tokio::test]
async fn case20_whitespace_only_body_fails() {
    let outcome = run_stream_case("\n\n  \r\n\r\n \n", "application/json").await;
    assert!(outcome.expect_incomplete().contains("empty body"));
    assert!(outcome.events.is_empty());
}

/// Polling a failed stream again keeps failing the same way. A caller that
/// loops on `next_event` must never be handed a terminal event after the
/// failure, which is what falling through to the finaliser would produce.
#[tokio::test]
async fn an_incomplete_stream_stays_failed_when_polled_again() {
    let state = Arc::new(Mutex::new(Vec::<CapturedRequest>::new()));
    let truncated = format!("{VALID_TEXT_FRAME}data: {{\"id\"");
    let server = spawn_server(
        state.clone(),
        vec![http_response("200 OK", "text/event-stream", &truncated)],
    )
    .await;

    let client = OpenAiCompatClient::new("test-key", OpenAiCompatConfig::openai())
        .with_base_url(server.base_url());
    let mut stream = client
        .stream_message(&sample_request(true))
        .await
        .expect("stream should start");

    while let Ok(Some(_)) = stream.next_event().await {}

    for _ in 0..3 {
        let error = stream
            .next_event()
            .await
            .expect_err("a failed stream must keep failing");
        assert!(matches!(error, ApiError::InvalidSseFrame(_)), "{error:?}");
    }
    assert_eq!(state.lock().await.len(), 1);
}

// ---------------------------------------------------------------------------
// Terminal-sentinel latch
//
// `data: [DONE]` is the end of the stream. The OpenAI-compatible iteration
// contract stops reading there, and `finalize_at_eof` already treats the
// sentinel as proof that the response completed. Both readings only hold if
// the sentinel is final: if bytes after it can still reach the caller, the
// stream that "completed" keeps talking, and whatever it says arrives after
// the answer was already declared whole.
//
// The latch is therefore load-bearing, not cosmetic. Once the sentinel is
// recognised no later frame may emit content, reopen the stream, or turn a
// completed turn into a truncation failure.
// ---------------------------------------------------------------------------

/// The reviewer's first blocker: a valid text frame, the terminal sentinel,
/// and then another perfectly well-formed text frame behind it.
const DONE_THEN_LATER_TEXT: &str = concat!(
    "data: {\"id\":\"chatcmpl_done\",\"model\":\"qwen3:14b\",",
    "\"choices\":[{\"delta\":{\"content\":\"before\"},\"finish_reason\":\"stop\"}]}\n\n",
    "data: [DONE]\n\n",
    "data: {\"id\":\"chatcmpl_done\",\"model\":\"qwen3:14b\",",
    "\"choices\":[{\"delta\":{\"content\":\"AFTER\"}}]}\n\n"
);

#[tokio::test]
async fn post_done_text_never_reaches_the_caller() {
    let outcome = run_stream_case(DONE_THEN_LATER_TEXT, "text/event-stream").await;
    let detail = outcome.expect_incomplete();
    assert!(
        detail.contains("[DONE]"),
        "the failure must name the contradicted terminal marker, got {detail:?}"
    );
    assert_eq!(
        outcome.text(),
        "before",
        "no application data may be emitted after the terminal sentinel, got {:?}",
        outcome.events
    );
}

/// A tool call behind the sentinel is the same violation as text behind it,
/// and the more dangerous one: a tool block the caller acts on would be an
/// action taken on output the stream had already disowned.
#[tokio::test]
async fn post_done_tool_call_never_reaches_the_caller() {
    let body = concat!(
        "data: {\"id\":\"chatcmpl_done\",\"model\":\"qwen3:14b\",",
        "\"choices\":[{\"delta\":{\"content\":\"before\"},\"finish_reason\":\"stop\"}]}\n\n",
        "data: [DONE]\n\n",
        "data: {\"id\":\"chatcmpl_done\",\"choices\":[{\"delta\":{\"tool_calls\":[{",
        "\"index\":0,\"id\":\"call_after\",\"function\":{\"name\":\"weather\",",
        "\"arguments\":\"{}\"}}]}}]}\n\n"
    );
    let outcome = run_stream_case(body, "text/event-stream").await;
    outcome.expect_incomplete();
    assert!(
        !outcome.has_tool_use(),
        "no tool block may be opened from a frame behind the sentinel, got {:?}",
        outcome.events
    );
    assert_eq!(outcome.text(), "before");
}

/// Malformed bytes behind the sentinel must not even be parsed. Reaching the
/// decoder is the bug; whether the decoder then rejects them is beside the
/// point, and the stream fails for the violation, not for the malformation.
#[tokio::test]
async fn post_done_malformed_frame_is_never_decoded() {
    let body = concat!(
        "data: {\"id\":\"s\",\"model\":\"qwen3:14b\",",
        "\"choices\":[{\"delta\":{\"content\":\"before\"},\"finish_reason\":\"stop\"}]}\n\n",
        "data: [DONE]\n\n",
        "data: {not json at all}\n\n"
    );
    let outcome = run_stream_case(body, "text/event-stream").await;
    let detail = outcome.expect_incomplete();
    assert!(
        detail.contains("[DONE]"),
        "the violation is the continuation, not the malformation, got {detail:?}"
    );
    assert_eq!(outcome.text(), "before");
}

/// A frame cut off behind the sentinel is the same violation: the stream said
/// it was finished and then started saying something else.
#[tokio::test]
async fn post_done_truncated_frame_fails() {
    let body = concat!(
        "data: {\"id\":\"s\",\"model\":\"qwen3:14b\",",
        "\"choices\":[{\"delta\":{\"content\":\"before\"},\"finish_reason\":\"stop\"}]}\n\n",
        "data: [DONE]\n\n",
        "data: {\"id\":\"s\",\"choices\":[{\"delta\":{\"content\":\"lost"
    );
    let outcome = run_stream_case(body, "text/event-stream").await;
    outcome.expect_incomplete();
    assert_eq!(outcome.text(), "before");
}

/// A second sentinel is bytes past the end of the stream like any other. The
/// rule is uniform and byte-level on purpose: classifying what follows the
/// sentinel would mean decoding it, which is the thing being prevented.
#[tokio::test]
async fn duplicate_done_sentinel_fails_closed() {
    let body = concat!(
        "data: {\"id\":\"s\",\"model\":\"qwen3:14b\",",
        "\"choices\":[{\"delta\":{\"content\":\"before\"},\"finish_reason\":\"stop\"}]}\n\n",
        "data: [DONE]\n\n",
        "data: [DONE]\n\n"
    );
    let outcome = run_stream_case(body, "text/event-stream").await;
    outcome.expect_incomplete();
    assert_eq!(outcome.text(), "before");
}

/// Trailing whitespace is not a continuation. A body that ends on the sentinel
/// and a newline has said everything it owed.
#[tokio::test]
async fn done_followed_by_whitespace_completes() {
    let body = concat!(
        "data: {\"id\":\"s\",\"model\":\"qwen3:14b\",",
        "\"choices\":[{\"delta\":{\"content\":\"Hello\"},\"finish_reason\":\"stop\"}]}\n\n",
        "data: [DONE]\n\n   \n\n"
    );
    let outcome = run_stream_case(body, "text/event-stream").await;
    outcome.expect_complete();
    assert_eq!(outcome.text(), "Hello");
    assert_eq!(outcome.message_stop_count(), 1);
}

/// Text, then the sentinel, and nothing else: the ordinary complete stream the
/// latch must leave exactly as it was.
#[tokio::test]
async fn text_then_done_completes_once() {
    let body = concat!(
        "data: {\"id\":\"s\",\"model\":\"qwen3:14b\",",
        "\"choices\":[{\"delta\":{\"content\":\"Hello\"},\"finish_reason\":\"stop\"}]}\n\n",
        "data: [DONE]\n\n"
    );
    let outcome = run_stream_case(body, "text/event-stream").await;
    outcome.expect_complete();
    assert_eq!(outcome.text(), "Hello");
    assert_eq!(outcome.stop_reason().as_deref(), Some("end_turn"));
    assert_eq!(outcome.message_stop_count(), 1);
}

/// A tool-use stream that ends on the sentinel completes and delivers its tool
/// block exactly once.
#[tokio::test]
async fn tool_use_then_done_completes_once() {
    let body = concat!(
        "data: {\"id\":\"s\",\"model\":\"qwen3:14b\",\"choices\":[{\"delta\":{",
        "\"tool_calls\":[{\"index\":0,\"id\":\"call_1\",\"function\":{",
        "\"name\":\"weather\",\"arguments\":\"{}\"}}]}}]}\n\n",
        "data: {\"id\":\"s\",\"choices\":[{\"delta\":{},\"finish_reason\":\"tool_calls\"}]}\n\n",
        "data: [DONE]\n\n"
    );
    let outcome = run_stream_case(body, "text/event-stream").await;
    outcome.expect_complete();
    assert!(outcome.has_tool_use());
    assert_eq!(outcome.stop_reason().as_deref(), Some("tool_use"));
    assert_eq!(outcome.message_stop_count(), 1);
}

/// A sentinel alone, with no content frame before it. Nothing was delivered,
/// but the stream did say it finished, so it is complete rather than failed.
#[tokio::test]
async fn standalone_done_completes() {
    let outcome = run_stream_case("data: [DONE]\n\n", "text/event-stream").await;
    assert!(
        outcome.error.is_none(),
        "a lone sentinel is a complete, empty stream, got {:?}",
        outcome.error
    );
    assert_eq!(outcome.requests, 1);
    assert_eq!(outcome.text(), "");
}

/// The sentinel split across chunk boundaries never completes as a frame, so
/// it never latches. The body then ends mid-frame and fails closed, which is
/// the correct reading: nothing ever said the stream was finished.
#[tokio::test]
async fn partial_done_sentinel_fails_closed() {
    let body = concat!(
        "data: {\"id\":\"s\",\"model\":\"qwen3:14b\",",
        "\"choices\":[{\"delta\":{\"content\":\"Hello\"}}]}\n\n",
        "data: [DON"
    );
    let outcome = run_stream_case(body, "text/event-stream").await;
    let detail = outcome.expect_incomplete();
    assert!(
        detail.contains("truncated"),
        "an unfinished sentinel is a truncation, not a continuation, got {detail:?}"
    );
}

// ---------------------------------------------------------------------------
// Compact completion validation
//
// Compact recovery is a compatibility path for a gateway that ignored
// `stream: true`, not a generic "this JSON deserialized, ship it" fallback.
// Serde's leniency is the hazard: `ChatCompletionResponse` has no required
// discriminator and an optional `finish_reason`, so bodies that never claimed
// to be a finished chat completion still deserialize. The recovered object has
// to prove, on its own, that it is a completed OpenAI chat completion.
// ---------------------------------------------------------------------------

/// The reviewer's second blocker: structurally valid, but the body says it is
/// something other than a chat completion.
const COMPACT_WRONG_OBJECT: &str = concat!(
    "{\"id\":\"chatcmpl-compact\",\"object\":\"chat.completion.chunk\",\"model\":\"qwen3:14b\",",
    "\"choices\":[{\"index\":0,\"message\":{\"role\":\"assistant\",\"content\":\"Hello\"},",
    "\"finish_reason\":\"stop\"}]}"
);

#[tokio::test]
async fn compact_recovery_rejects_a_wrong_object_discriminator() {
    let outcome = run_stream_case(COMPACT_WRONG_OBJECT, "application/json").await;
    outcome.expect_incomplete();
}

/// The reviewer's third blocker: it calls itself a chat completion, but the
/// choice the client consumes never said it finished.
const COMPACT_NULL_FINISH_REASON: &str = concat!(
    "{\"id\":\"chatcmpl-compact\",\"object\":\"chat.completion\",\"model\":\"qwen3:14b\",",
    "\"choices\":[{\"index\":0,\"message\":{\"role\":\"assistant\",\"content\":\"Hello\"},",
    "\"finish_reason\":null}]}"
);

#[tokio::test]
async fn compact_recovery_rejects_a_null_finish_reason() {
    let outcome = run_stream_case(COMPACT_NULL_FINISH_REASON, "application/json").await;
    outcome.expect_incomplete();
}

/// No discriminator at all is not a weaker version of the wrong one. A body
/// that never said what it is has not proved it is a finished completion.
#[tokio::test]
async fn compact_recovery_rejects_a_missing_object_discriminator() {
    let body = concat!(
        "{\"id\":\"chatcmpl-compact\",\"model\":\"qwen3:14b\",",
        "\"choices\":[{\"index\":0,\"message\":{\"role\":\"assistant\",\"content\":\"Hello\"},",
        "\"finish_reason\":\"stop\"}]}"
    );
    let outcome = run_stream_case(body, "application/json").await;
    outcome.expect_incomplete();
}

/// An absent `finish_reason` is rejected for the same reason an explicit
/// `null` is: serde defaults both to `None`, and neither says the turn ended.
#[tokio::test]
async fn compact_recovery_rejects_a_missing_finish_reason() {
    let body = concat!(
        "{\"id\":\"chatcmpl-compact\",\"object\":\"chat.completion\",\"model\":\"qwen3:14b\",",
        "\"choices\":[{\"index\":0,\"message\":{\"role\":\"assistant\",\"content\":\"Hello\"}}]}"
    );
    let outcome = run_stream_case(body, "application/json").await;
    outcome.expect_incomplete();
}

/// A blank string is not a terminal reason either. It deserializes to
/// `Some("")`, which would otherwise pass an `is_some()` test while saying
/// nothing about how the turn ended.
#[tokio::test]
async fn compact_recovery_rejects_a_blank_finish_reason() {
    let body = concat!(
        "{\"id\":\"chatcmpl-compact\",\"object\":\"chat.completion\",\"model\":\"qwen3:14b\",",
        "\"choices\":[{\"index\":0,\"message\":{\"role\":\"assistant\",\"content\":\"Hello\"},",
        "\"finish_reason\":\"  \"}]}"
    );
    let outcome = run_stream_case(body, "application/json").await;
    outcome.expect_incomplete();
}

/// A well-formed envelope with nothing in it. There is no choice to consume,
/// so there is no answer here to stand in for the one the POST paid for.
#[tokio::test]
async fn compact_recovery_rejects_empty_choices() {
    let body = concat!(
        "{\"id\":\"chatcmpl-compact\",\"object\":\"chat.completion\",",
        "\"model\":\"qwen3:14b\",\"choices\":[]}"
    );
    let outcome = run_stream_case(body, "application/json").await;
    outcome.expect_incomplete();
}

/// Partial JSON never deserializes, so it never reaches the completion check.
#[tokio::test]
async fn compact_recovery_rejects_partial_json() {
    let body = concat!(
        "{\"id\":\"chatcmpl-compact\",\"object\":\"chat.completion\",\"model\":\"qwen3:14b\",",
        "\"choices\":[{\"index\":0,\"message\":{\"role\":\"assistant\",\"content\":\"Hel"
    );
    let outcome = run_stream_case(body, "application/json").await;
    outcome.expect_incomplete();
}

/// SSE framing that carries no `data:` payload — a comment — still leaves the
/// body un-decodable as one whole object, because the comment bytes are part
/// of what recovery is offered. Recovery is for a body that is the completion
/// and nothing else.
#[tokio::test]
async fn compact_recovery_rejects_comment_framing_before_the_object() {
    let body = format!(": keep-alive\n\n{COMPACT_TEXT_COMPLETION}");
    let outcome = run_stream_case(&body, "text/event-stream").await;
    outcome.expect_incomplete();
    assert_eq!(outcome.text(), "");
}

// --- Choice selection ------------------------------------------------------
//
// `build_chat_completion_request` never sends `n`, so a response carries one
// choice and `normalize_response` reads `choices[0]`. Validation follows that
// selection rather than quantifying over choices in either direction: it must
// not demand a choice the client never reads be terminal, and it must not let
// some other choice's terminal state vouch for the one actually consumed.

/// The selected choice never finished, so the response is rejected even though
/// a later choice did. Termination is not transferable between choices.
#[tokio::test]
async fn compact_recovery_rejects_an_unterminated_selected_choice() {
    let body = concat!(
        "{\"id\":\"chatcmpl-compact\",\"object\":\"chat.completion\",\"model\":\"qwen3:14b\",",
        "\"choices\":[",
        "{\"index\":0,\"message\":{\"role\":\"assistant\",\"content\":\"first\"}},",
        "{\"index\":1,\"message\":{\"role\":\"assistant\",\"content\":\"second\"},",
        "\"finish_reason\":\"stop\"}]}"
    );
    let outcome = run_stream_case(body, "application/json").await;
    outcome.expect_incomplete();
    assert_eq!(
        outcome.text(),
        "",
        "a rejected body must deliver no content, got {:?}",
        outcome.events
    );
}

/// The selected choice finished, so the response is accepted. An unterminated
/// choice the client never selects does not veto it.
#[tokio::test]
async fn compact_recovery_accepts_a_terminated_selected_choice() {
    let body = concat!(
        "{\"id\":\"chatcmpl-compact\",\"object\":\"chat.completion\",\"model\":\"qwen3:14b\",",
        "\"choices\":[",
        "{\"index\":0,\"message\":{\"role\":\"assistant\",\"content\":\"first\"},",
        "\"finish_reason\":\"stop\"},",
        "{\"index\":1,\"message\":{\"role\":\"assistant\",\"content\":\"second\"}}]}"
    );
    let outcome = run_stream_case(body, "application/json").await;
    outcome.expect_complete();
    assert_eq!(outcome.stop_reason().as_deref(), Some("end_turn"));
}

// ---------------------------------------------------------------------------
// Mixed frame separators
//
// `next_sse_frame` looks for `\n\n` first and falls back to `\r\n\r\n`, so a
// body that mixes the two can be split on the wrong boundary. That is a known,
// separately tracked diagnostic defect and is not repaired here. What this
// lane owes is the guarantee that it stays a *diagnostic* defect: a
// mis-separated body must not become a false success, a duplicate inference,
// or corrupt output. It fails closed, and these tests hold it there so the
// terminal latch and the compact validation cannot quietly turn it into
// something worse.
// ---------------------------------------------------------------------------

/// A CRLF-terminated frame followed by an LF-terminated one. The split lands
/// in the wrong place; the turn must still cost one POST and either complete
/// with only the content it actually decoded or fail with a typed error —
/// never deliver a terminal success built on mis-split bytes.
#[tokio::test]
async fn mixed_frame_separators_stay_fail_closed() {
    let body = concat!(
        "data: {\"id\":\"s\",\"model\":\"qwen3:14b\",\"choices\":[{\"delta\":{\"content\":\"one\"}}]}\r\n\r\n",
        "data: {\"id\":\"s\",\"choices\":[{\"delta\":{\"content\":\"two\"},\"finish_reason\":\"stop\"}]}\n\n"
    );
    let outcome = run_stream_case(body, "text/event-stream").await;
    assert_eq!(
        outcome.requests, 1,
        "a mis-separated body must never cost a second inference POST"
    );
    if outcome.error.is_none() {
        assert!(
            !outcome.text().contains("onetwoone"),
            "content must not be duplicated by a mis-split frame, got {:?}",
            outcome.text()
        );
    } else {
        // This is the tracked diagnostic defect, pinned rather than repaired.
        // The wrong split hands two concatenated objects to the JSON decoder,
        // so the failure surfaces as `Json { .. "trailing characters" }`
        // rather than as the framing error it really is. Confusing to read,
        // but safe: it is typed, it is latched, and it synthesises nothing.
        assert!(
            matches!(
                outcome.error,
                Some(ApiError::InvalidSseFrame(_) | ApiError::Json { .. })
            ),
            "a mis-separated body must fail with a typed error, got {:?}",
            outcome.error
        );
        assert!(
            !outcome.has_message_stop() && !outcome.has_message_delta(),
            "a mis-separated body must synthesise no terminal event, got {:?}",
            outcome.events
        );
    }
}

/// The same mixing with the terminal sentinel behind it. Whatever the split
/// does, no second POST and no duplicated terminal event may come of it.
#[tokio::test]
async fn mixed_frame_separators_with_done_stay_fail_closed() {
    let body = concat!(
        "data: {\"id\":\"s\",\"model\":\"qwen3:14b\",\"choices\":[{\"delta\":{\"content\":\"one\"}}]}\r\n\r\n",
        "data: [DONE]\n\n"
    );
    let outcome = run_stream_case(body, "text/event-stream").await;
    assert_eq!(outcome.requests, 1);
    assert!(
        outcome.message_stop_count() <= 1,
        "a mis-separated body must never produce a duplicate terminal event, got {:?}",
        outcome.events
    );
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CapturedRequest {
    path: String,
    headers: HashMap<String, String>,
    body: String,
}

struct TestServer {
    base_url: String,
    join_handle: tokio::task::JoinHandle<()>,
}

impl TestServer {
    fn base_url(&self) -> String {
        self.base_url.clone()
    }
}

impl Drop for TestServer {
    fn drop(&mut self) {
        self.join_handle.abort();
    }
}

async fn spawn_server(
    state: Arc<Mutex<Vec<CapturedRequest>>>,
    responses: Vec<String>,
) -> TestServer {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listener should bind");
    let address = listener.local_addr().expect("listener addr");
    let join_handle = tokio::spawn(async move {
        for response in responses {
            let (mut socket, _) = listener.accept().await.expect("accept");
            let mut buffer = Vec::new();
            let mut header_end = None;
            loop {
                let mut chunk = [0_u8; 1024];
                let read = socket.read(&mut chunk).await.expect("read request");
                if read == 0 {
                    break;
                }
                buffer.extend_from_slice(&chunk[..read]);
                if let Some(position) = find_header_end(&buffer) {
                    header_end = Some(position);
                    break;
                }
            }

            let header_end = header_end.expect("headers should exist");
            let (header_bytes, remaining) = buffer.split_at(header_end);
            let header_text = String::from_utf8(header_bytes.to_vec()).expect("utf8 headers");
            let mut lines = header_text.split("\r\n");
            let request_line = lines.next().expect("request line");
            let path = request_line
                .split_whitespace()
                .nth(1)
                .expect("path")
                .to_string();
            let mut headers = HashMap::new();
            let mut content_length = 0_usize;
            for line in lines {
                if line.is_empty() {
                    continue;
                }
                let (name, value) = line.split_once(':').expect("header");
                let value = value.trim().to_string();
                if name.eq_ignore_ascii_case("content-length") {
                    content_length = value.parse().expect("content length");
                }
                headers.insert(name.to_ascii_lowercase(), value);
            }

            let mut body = remaining[4..].to_vec();
            while body.len() < content_length {
                let mut chunk = vec![0_u8; content_length - body.len()];
                let read = socket.read(&mut chunk).await.expect("read body");
                if read == 0 {
                    break;
                }
                body.extend_from_slice(&chunk[..read]);
            }

            state.lock().await.push(CapturedRequest {
                path,
                headers,
                body: String::from_utf8(body).expect("utf8 body"),
            });

            socket
                .write_all(response.as_bytes())
                .await
                .expect("write response");
        }
    });

    TestServer {
        base_url: format!("http://{address}"),
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

fn aliased_request(model: &str) -> MessageRequest {
    MessageRequest {
        model: model.to_string(),
        max_tokens: 64,
        messages: vec![InputMessage {
            role: "user".to_string(),
            content: vec![InputContentBlock::Text {
                text: "Say hello".to_string(),
            }],
        }],
        system: Some("Use tools when needed".to_string()),
        tools: None,
        tool_choice: None,
        stream: false,
        ..Default::default()
    }
}

fn sample_request(stream: bool) -> MessageRequest {
    MessageRequest {
        model: "grok-3".to_string(),
        max_tokens: 64,
        messages: vec![InputMessage {
            role: "user".to_string(),
            content: vec![InputContentBlock::Text {
                text: "Say hello".to_string(),
            }],
        }],
        system: Some("Use tools when needed".to_string()),
        tools: Some(vec![ToolDefinition {
            name: "weather".to_string(),
            description: Some("Fetches weather".to_string()),
            input_schema: json!({
                "type": "object",
                "properties": {"city": {"type": "string"}},
                "required": ["city"]
            }),
        }]),
        tool_choice: Some(ToolChoice::Auto),
        stream,
        ..Default::default()
    }
}

fn env_lock() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: OnceLock<StdMutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| StdMutex::new(()))
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

struct ScopedEnvVar {
    key: &'static str,
    previous: Option<OsString>,
}

impl ScopedEnvVar {
    fn set(key: &'static str, value: impl AsRef<std::ffi::OsStr>) -> Self {
        let previous = std::env::var_os(key);
        std::env::set_var(key, value);
        Self { key, previous }
    }

    fn remove(key: &'static str) -> Self {
        let previous = std::env::var_os(key);
        std::env::remove_var(key);
        Self { key, previous }
    }
}

impl Drop for ScopedEnvVar {
    fn drop(&mut self) {
        match &self.previous {
            Some(value) => std::env::set_var(self.key, value),
            None => std::env::remove_var(self.key),
        }
    }
}
