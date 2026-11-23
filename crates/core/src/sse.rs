use serde_json::Value;

use crate::ProviderId;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StreamPiece {
    Text(String),
    Thinking(String),
    Usage {
        prompt: u32,
        completion: u32,
    },
    Error(String),
    /// A fragment of a tool call. `id` and `name` arrive once; `arguments` may arrive in pieces.
    ToolDelta {
        index: u32,
        id: String,
        name: String,
        arguments: String,
    },
}

#[derive(Debug, Default)]
pub struct SseBuffer {
    pending: String,
}

impl SseBuffer {
    pub fn push(&mut self, chunk: &str, provider: ProviderId) -> Vec<StreamPiece> {
        self.pending.push_str(&chunk.replace("\r\n", "\n"));
        let mut pieces = Vec::new();
        while let Some(index) = self.pending.find("\n\n") {
            let block = self.pending[..index].to_string();
            self.pending.drain(..index + 2);
            pieces.extend(parse_sse_block(provider, &block));
        }
        pieces
    }

    pub fn finish(&mut self, provider: ProviderId) -> Vec<StreamPiece> {
        let rest = std::mem::take(&mut self.pending);
        let rest = rest.trim_end_matches(['\n', '\r']);
        if rest.trim().is_empty() {
            Vec::new()
        } else {
            parse_sse_block(provider, rest)
        }
    }
}

pub fn parse_sse_block(provider: ProviderId, block: &str) -> Vec<StreamPiece> {
    let mut data = String::new();
    for line in block.lines() {
        let line = line.trim_end_matches('\r');
        if line.is_empty() || line.starts_with(':') {
            continue;
        }
        if let Some(rest) = line.strip_prefix("data:") {
            if !data.is_empty() {
                data.push('\n');
            }
            data.push_str(rest.trim_start());
        }
    }
    let data = data.trim();
    if data.is_empty() || data == "[DONE]" {
        return Vec::new();
    }
    let Ok(value) = serde_json::from_str::<Value>(data) else {
        return Vec::new();
    };
    match provider {
        ProviderId::Anthropic => pieces_from_anthropic(&value),
        ProviderId::Gemini => pieces_from_gemini(&value),
        ProviderId::Local | ProviderId::Openai | ProviderId::Deepseek | ProviderId::Openrouter => {
            pieces_from_openai(&value)
        }
    }
}

fn pieces_from_openai(value: &Value) -> Vec<StreamPiece> {
    let mut out = Vec::new();
    if let Some(message) = value
        .pointer("/error/message")
        .and_then(|item| item.as_str())
    {
        out.push(StreamPiece::Error(message.to_string()));
    }
    if let Some(delta) = value.pointer("/choices/0/delta") {
        if let Some(text) = delta.get("content").and_then(|item| item.as_str()) {
            if !text.is_empty() {
                out.push(StreamPiece::Text(text.to_string()));
            }
        }
        for key in ["reasoning_content", "reasoning"] {
            if let Some(text) = delta.get(key).and_then(|item| item.as_str()) {
                if !text.is_empty() {
                    out.push(StreamPiece::Thinking(text.to_string()));
                }
            }
        }
        if let Some(calls) = delta.get("tool_calls").and_then(|item| item.as_array()) {
            for call in calls {
                let index = call
                    .get("index")
                    .and_then(|item| item.as_u64())
                    .unwrap_or(0) as u32;
                let id = call
                    .get("id")
                    .and_then(|item| item.as_str())
                    .unwrap_or("")
                    .to_string();
                let name = call
                    .pointer("/function/name")
                    .and_then(|item| item.as_str())
                    .unwrap_or("")
                    .to_string();
                let arguments = call
                    .pointer("/function/arguments")
                    .and_then(|item| item.as_str())
                    .unwrap_or("")
                    .to_string();
                if !id.is_empty() || !name.is_empty() || !arguments.is_empty() {
                    out.push(StreamPiece::ToolDelta {
                        index,
                        id,
                        name,
                        arguments,
                    });
                }
            }
        }
    }
    push_usage(
        &mut out,
        value.pointer("/usage/prompt_tokens"),
        value.pointer("/usage/completion_tokens"),
    );
    out
}

fn pieces_from_anthropic(value: &Value) -> Vec<StreamPiece> {
    let mut out = Vec::new();
    match value.get("type").and_then(|item| item.as_str()) {
        Some("content_block_delta") => {
            match value.pointer("/delta/type").and_then(|item| item.as_str()) {
                Some("text_delta") => {
                    if let Some(text) = value.pointer("/delta/text").and_then(|item| item.as_str())
                    {
                        if !text.is_empty() {
                            out.push(StreamPiece::Text(text.to_string()));
                        }
                    }
                }
                Some("input_json_delta") => {
                    let index = value
                        .get("index")
                        .and_then(|item| item.as_u64())
                        .unwrap_or(0) as u32;
                    let arguments = value
                        .pointer("/delta/partial_json")
                        .and_then(|item| item.as_str())
                        .unwrap_or("")
                        .to_string();
                    if !arguments.is_empty() {
                        out.push(StreamPiece::ToolDelta {
                            index,
                            id: String::new(),
                            name: String::new(),
                            arguments,
                        });
                    }
                }
                Some("thinking_delta") => {
                    if let Some(text) = value
                        .pointer("/delta/thinking")
                        .and_then(|item| item.as_str())
                    {
                        if !text.is_empty() {
                            out.push(StreamPiece::Thinking(text.to_string()));
                        }
                    }
                }
                _ => {}
            }
        }
        Some("content_block_start") => {
            if value
                .pointer("/content_block/type")
                .and_then(|item| item.as_str())
                == Some("tool_use")
            {
                let index = value
                    .get("index")
                    .and_then(|item| item.as_u64())
                    .unwrap_or(0) as u32;
                out.push(StreamPiece::ToolDelta {
                    index,
                    id: value
                        .pointer("/content_block/id")
                        .and_then(|item| item.as_str())
                        .unwrap_or("")
                        .to_string(),
                    name: value
                        .pointer("/content_block/name")
                        .and_then(|item| item.as_str())
                        .unwrap_or("")
                        .to_string(),
                    arguments: String::new(),
                });
            }
        }
        Some("error") => {
            let message = value
                .pointer("/error/message")
                .and_then(|item| item.as_str())
                .unwrap_or("Anthropic returned an error.");
            out.push(StreamPiece::Error(message.to_string()));
        }
        Some("message_delta") => {
            push_usage(
                &mut out,
                value.pointer("/usage/input_tokens"),
                value.pointer("/usage/output_tokens"),
            );
        }
        _ => {}
    }
    out
}

fn pieces_from_gemini(value: &Value) -> Vec<StreamPiece> {
    let mut out = Vec::new();
    if let Some(message) = value
        .pointer("/error/message")
        .and_then(|item| item.as_str())
    {
        out.push(StreamPiece::Error(message.to_string()));
    }
    if let Some(parts) = value
        .pointer("/candidates/0/content/parts")
        .and_then(|item| item.as_array())
    {
        for part in parts {
            if let Some(call) = part.get("functionCall") {
                out.push(StreamPiece::ToolDelta {
                    index: out.len() as u32,
                    id: String::new(),
                    name: call
                        .get("name")
                        .and_then(|item| item.as_str())
                        .unwrap_or("")
                        .to_string(),
                    arguments: call
                        .get("args")
                        .map(|item| item.to_string())
                        .unwrap_or_default(),
                });
                continue;
            }
            let Some(text) = part.get("text").and_then(|item| item.as_str()) else {
                continue;
            };
            if text.is_empty() {
                continue;
            }
            if part.get("thought").and_then(|item| item.as_bool()) == Some(true) {
                out.push(StreamPiece::Thinking(text.to_string()));
            } else {
                out.push(StreamPiece::Text(text.to_string()));
            }
        }
    }
    push_usage(
        &mut out,
        value.pointer("/usageMetadata/promptTokenCount"),
        value.pointer("/usageMetadata/candidatesTokenCount"),
    );
    out
}

fn push_usage(out: &mut Vec<StreamPiece>, prompt: Option<&Value>, completion: Option<&Value>) {
    let prompt = json_u32(prompt);
    let completion = json_u32(completion);
    if prompt > 0 || completion > 0 {
        out.push(StreamPiece::Usage { prompt, completion });
    }
}

fn json_u32(value: Option<&Value>) -> u32 {
    value
        .and_then(|item| item.as_u64().or_else(|| item.as_f64().map(|n| n as u64)))
        .unwrap_or(0) as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn openai_stream_splits_text_and_reasoning() {
        let mut buffer = SseBuffer::default();
        let chunk = concat!(
            "data: {\"choices\":[{\"delta\":{\"reasoning_content\":\"plan\"}}]}\n\n",
            "data: {\"choices\":[{\"delta\":{\"content\":\"Hi\"}}]}\n\n",
            "data: {\"choices\":[{\"delta\":{}}],\"usage\":{\"prompt_tokens\":12,\"completion_tokens\":3}}\n\n",
            "data: [DONE]\n\n"
        );
        let pieces = buffer.push(chunk, ProviderId::Openai);
        assert_eq!(
            pieces,
            vec![
                StreamPiece::Thinking("plan".into()),
                StreamPiece::Text("Hi".into()),
                StreamPiece::Usage {
                    prompt: 12,
                    completion: 3
                },
            ]
        );
    }

    #[test]
    fn parser_keeps_a_split_chunk() {
        let mut buffer = SseBuffer::default();
        let first = buffer.push(
            "data: {\"choices\":[{\"delta\":{\"content\":\"Hel",
            ProviderId::Local,
        );
        assert!(first.is_empty());
        let second = buffer.push("lo\"}}]}\n\n", ProviderId::Local);
        assert_eq!(second, vec![StreamPiece::Text("Hello".into())]);
    }

    #[test]
    fn anthropic_thinking_and_text() {
        let block = concat!(
            "event: content_block_delta\n",
            "data: {\"type\":\"content_block_delta\",\"delta\":{\"type\":\"thinking_delta\",\"thinking\":\"why\"}}\n"
        );
        assert_eq!(
            parse_sse_block(ProviderId::Anthropic, block),
            vec![StreamPiece::Thinking("why".into())]
        );
    }
}
