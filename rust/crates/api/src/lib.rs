mod client;
mod error;
mod http_client;
mod n6_admission;
mod prompt_cache;
mod providers;
mod sse;
mod types;

pub use client::{
    oauth_token_is_expired, read_base_url, read_xai_base_url, resolve_saved_oauth_token,
    resolve_startup_auth_source, MessageStream, OAuthTokenSet, ProviderClient,
};
pub use error::{ApiError, N6RefusalKind};
pub use http_client::{
    build_http_client, build_http_client_or_default, build_http_client_with, ProxyConfig,
};
pub use n6_admission::{
    evaluate_readiness_body, marker_active, BrokerOrigin, HttpReadinessAuthority,
    N6ReadinessAuthority, N6ReadinessFuture, ReadinessReport, BROKER_PORT,
    MAX_READINESS_BODY_BYTES, READINESS_PATH, SIDESTACK_MARKER_ENV,
    SIDESTACK_N6_ENFORCE_CAPABILITY,
};
pub use prompt_cache::{
    CacheBreakEvent, PromptCache, PromptCacheConfig, PromptCachePaths, PromptCacheRecord,
    PromptCacheStats,
};
pub use providers::anthropic::{AnthropicClient, AnthropicClient as ApiClient, AuthSource};
pub use providers::openai_compat::{
    build_chat_completion_request, flatten_tool_result_content, is_reasoning_model,
    model_rejects_is_error_field, translate_message, wire_model_for_request, OpenAiCompatClient,
    OpenAiCompatConfig,
};
pub use providers::{
    detect_provider_kind, max_tokens_for_model, max_tokens_for_model_with_override,
    resolve_model_alias, ProviderKind,
};
pub use sse::{parse_frame, SseParser};
pub use types::{
    ContentBlockDelta, ContentBlockDeltaEvent, ContentBlockStartEvent, ContentBlockStopEvent,
    InputContentBlock, InputMessage, MessageDelta, MessageDeltaEvent, MessageRequest,
    MessageResponse, MessageStartEvent, MessageStopEvent, OutputContentBlock, StreamEvent,
    ToolChoice, ToolDefinition, ToolResultContentBlock, Usage,
};

pub use telemetry::{
    AnalyticsEvent, AnthropicRequestProfile, ClientIdentity, JsonlTelemetrySink,
    MemoryTelemetrySink, SessionTraceRecord, SessionTracer, TelemetryEvent, TelemetrySink,
    DEFAULT_ANTHROPIC_VERSION,
};
