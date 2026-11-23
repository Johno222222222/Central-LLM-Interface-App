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
mod types;

pub use db::{new_message, now_stamp, Store};
pub use gguf::read_gguf_meta;
pub use llama::{bundled_origin, llama_command_args, LlamaLaunch, DEFAULT_PORT, VRAM_MARGIN_MIB};
pub use models::{
    catalog_from_ids, default_context_limit, family_for_model_id, family_from_filename,
    hosted_catalog, loaded_ids, parse_model_list, parse_router_models, scan_gguf_dir,
    select_model_ids,
};
pub use providers::{
    build_body, build_headers, completion_url, default_origin, friendly_http_error,
    parse_model_ids, redact, stream_completion, test_provider, test_url, CompletionRequest,
    ImagePart, OutgoingMessage, StreamOutcome,
};
pub use secrets::{require_hosted, MemorySecretStore, SecretStore};
pub use sse::{parse_sse_block, SseBuffer, StreamPiece};
pub use tokens::{estimate_thread_tokens, estimate_tokens, format_context, title_from_message};
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
