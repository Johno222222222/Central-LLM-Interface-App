//! Shared logic for Central LLM Interface App.
//!
//! The Tauri shell calls into this crate. API keys never pass through the
//! database types defined here; callers keep them in the OS keychain.

mod db;
mod gguf;
mod llama;
mod models;
mod providers;
mod secrets;
mod sse;
mod tokens;
mod tools;
mod types;

pub use db::{new_message, now_stamp, Store};
pub use gguf::{read_gguf_meta, GgufMeta};
pub use llama::{bundled_origin, llama_command_args, LlamaLaunch, DEFAULT_PORT, VRAM_MARGIN_MIB};
pub use models::{
    catalog_from_ids, clamp_context_step, collect_projectors, context_ceiling, default_context_limit,
    detect_lu_model_folder, family_for_model_id, family_from_filename, fit_local_models,
    hosted_catalog, loaded_ids, looks_like_vision, pair_mmproj, parse_model_list, parse_router_models,
    scan_all_model_dirs, scan_gguf_dir, select_model_ids, suggest_context, write_models_preset,
    CONTEXT_STEPS, DEFAULT_VRAM_BYTES,
};
pub use providers::{
    build_body, build_headers, completion_url, default_origin, friendly_http_error,
    parse_model_ids, redact, stream_completion, test_provider, test_url, CompletionRequest,
    ImagePart, OutgoingMessage, StreamOutcome,
};
pub use secrets::{require_account, require_hosted, MemorySecretStore, SecretStore};
pub use sse::{parse_sse_block, SseBuffer, StreamPiece};
pub use tokens::{estimate_thread_tokens, estimate_tokens, format_context, title_from_message};
pub use tools::{
    anthropic_tools, brave_search_url, builtin_tools, enabled_builtin, encode_mcp_message,
    gemini_tools, html_to_text, mcp_initialize, mcp_initialized_notification, mcp_tool_name,
    mcp_tools_call, mcp_tools_list, needs_command_approval, openai_tools, parse_mcp_tool_list,
    path_is_inside, paths_contain, public_http_url, tool_enabled, tool_guidance, McpBuffer,
    ToolSpec, BRAVE_SEARCH_ACCOUNT,
    MAX_TOOL_ROUNDS,
};
pub use types::*;

#[derive(Debug, thiserror::Error)]
pub enum CoreError {
    #[error("{0}")]
    Message(String),
    #[error("database error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
}

impl From<String> for CoreError {
    fn from(value: String) -> Self {
        Self::Message(value)
    }
}
