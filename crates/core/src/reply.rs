/// How an assistant reply ended, so the chat can show Done, Stopped, Cut off, or Error.

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReplyEnd {
    /// `done`, `stopped`, `cut_off`, or `error`.
    pub status: &'static str,
    /// Empty for Done and Stopped. `context limit`, `max tokens`, or the error text.
    pub reason: String,
}

pub fn classify_reply(
    cancelled: bool,
    error: Option<&str>,
    finish_reason: Option<&str>,
    prompt_tokens: Option<u32>,
    completion_tokens: Option<u32>,
    max_tokens: u32,
    context_limit: u32,
) -> ReplyEnd {
    if cancelled {
        return ReplyEnd {
            status: "stopped",
            reason: String::new(),
        };
    }
    if let Some(message) = error.map(str::trim).filter(|text| !text.is_empty()) {
        if context_error(message) {
            return ReplyEnd {
                status: "cut_off",
                reason: "context limit".into(),
            };
        }
        let mut reason = message.to_string();
        if reason.len() > 180 {
            reason.truncate(180);
        }
        return ReplyEnd {
            status: "error",
            reason,
        };
    }
    let finish = finish_reason.unwrap_or("").trim().to_ascii_lowercase();
    if is_length_finish(&finish) {
        return ReplyEnd {
            status: "cut_off",
            reason: if hit_context(prompt_tokens, completion_tokens, max_tokens, context_limit) {
                "context limit".into()
            } else {
                "max tokens".into()
            },
        };
    }
    ReplyEnd {
        status: "done",
        reason: String::new(),
    }
}

fn is_length_finish(reason: &str) -> bool {
    matches!(
        reason,
        "length" | "max_tokens" | "max_tokens_exceeded" | "max_output_tokens"
    )
}

fn context_error(message: &str) -> bool {
    let text = message.to_lowercase();
    text.contains("n_ctx")
        || text.contains("maximum context")
        || text.contains("context window")
        || text.contains("context length")
        || (text.contains("context")
            && (text.contains("limit") || text.contains("exceed") || text.contains("full")))
}

fn hit_context(
    prompt_tokens: Option<u32>,
    completion_tokens: Option<u32>,
    max_tokens: u32,
    context_limit: u32,
) -> bool {
    if context_limit == 0 {
        return false;
    }
    let prompt = prompt_tokens.unwrap_or(0);
    let completion = completion_tokens.unwrap_or(0);
    let total = prompt.saturating_add(completion);
    let near_context = total > 0 && total.saturating_add(context_limit / 20) >= context_limit
        || prompt.saturating_add(context_limit / 10) >= context_limit && prompt > 0;
    let near_max = completion_tokens
        .map(|count| max_tokens > 0 && count.saturating_add(max_tokens / 10) >= max_tokens)
        .unwrap_or(false);
    if near_context && near_max {
        return prompt.saturating_add(context_limit / 10) >= context_limit;
    }
    near_context
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stop_cancel_and_clean_finish() {
        let stopped = classify_reply(true, None, Some("length"), Some(100), Some(50), 256, 8192);
        assert_eq!(stopped.status, "stopped");
        let done = classify_reply(false, None, Some("stop"), Some(100), Some(20), 256, 8192);
        assert_eq!(done.status, "done");
        assert!(done.reason.is_empty());
    }

    #[test]
    fn length_is_max_tokens_or_the_context_window() {
        let maxed = classify_reply(false, None, Some("length"), Some(400), Some(250), 256, 16384);
        assert_eq!(maxed.status, "cut_off");
        assert_eq!(maxed.reason, "max tokens");
        let window = classify_reply(false, None, Some("length"), Some(15000), Some(800), 4096, 16384);
        assert_eq!(window.status, "cut_off");
        assert_eq!(window.reason, "context limit");
    }

    #[test]
    fn context_errors_are_a_cut_off_not_a_raw_failure() {
        let end = classify_reply(
            false,
            Some("request exceeds the available context size (n_ctx)"),
            None,
            None,
            None,
            1024,
            16384,
        );
        assert_eq!(end.status, "cut_off");
        assert_eq!(end.reason, "context limit");
        let other = classify_reply(false, Some("llama-server connection reset"), None, None, None, 1024, 16384);
        assert_eq!(other.status, "error");
        assert!(other.reason.contains("connection reset"));
    }
}
