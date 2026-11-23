use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use futures_util::StreamExt;
use serde_json::{json, Value};

use crate::sse::{SseBuffer, StreamPiece};
use crate::{ChatSettings, CoreError, ProviderId};

#[derive(Debug, Clone)]
pub struct ImagePart {
    pub mime: String,
    pub data_base64: String,
}

#[derive(Debug, Clone)]
pub struct OutgoingMessage {
    pub role: String,
    pub content: String,
    pub images: Vec<ImagePart>,
}

#[derive(Debug, Clone)]
pub struct CompletionRequest {
    pub provider: ProviderId,
    pub model: String,
    pub messages: Vec<OutgoingMessage>,
    pub settings: ChatSettings,
    /// Scheme and host, no path. Example: `https://api.openai.com`.
    pub origin: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StreamOutcome {
    pub text: String,
    pub thinking: String,
    pub prompt_tokens: Option<u32>,
    pub completion_tokens: Option<u32>,
    pub cancelled: bool,
    pub error: Option<String>,
}

pub fn default_origin(provider: ProviderId) -> &'static str {
    match provider {
        ProviderId::Local => "http://127.0.0.1:17860",
        ProviderId::Openai => "https://api.openai.com",
        ProviderId::Anthropic => "https://api.anthropic.com",
        ProviderId::Gemini => "https://generativelanguage.googleapis.com",
        ProviderId::Deepseek => "https://api.deepseek.com",
        ProviderId::Openrouter => "https://openrouter.ai",
    }
}

pub fn completion_url(provider: ProviderId, origin: &str, model: &str) -> String {
    let origin = origin.trim_end_matches('/');
    match provider {
        ProviderId::Local | ProviderId::Openai | ProviderId::Deepseek => {
            format!("{origin}/v1/chat/completions")
        }
        ProviderId::Openrouter => format!("{origin}/api/v1/chat/completions"),
        ProviderId::Anthropic => format!("{origin}/v1/messages"),
        ProviderId::Gemini => {
            format!(
                "{origin}/v1beta/models/{}:streamGenerateContent?alt=sse",
                path_encode(model)
            )
        }
    }
}

pub fn test_url(provider: ProviderId, origin: &str) -> String {
    let origin = origin.trim_end_matches('/');
    match provider {
        ProviderId::Local => format!("{origin}/health"),
        ProviderId::Openai | ProviderId::Anthropic => format!("{origin}/v1/models"),
        ProviderId::Gemini => format!("{origin}/v1beta/models"),
        ProviderId::Deepseek => format!("{origin}/models"),
        ProviderId::Openrouter => format!("{origin}/api/v1/models"),
    }
}

pub fn build_headers(provider: ProviderId, api_key: &str) -> Vec<(String, String)> {
    match provider {
        ProviderId::Local => Vec::new(),
        ProviderId::Anthropic => vec![
            ("x-api-key".into(), api_key.to_string()),
            ("anthropic-version".into(), "2023-06-01".into()),
        ],
        ProviderId::Gemini => vec![("x-goog-api-key".into(), api_key.to_string())],
        ProviderId::Openrouter => vec![
            ("authorization".into(), format!("Bearer {api_key}")),
            ("x-title".into(), "Central LLM Interface App".into()),
        ],
        ProviderId::Openai | ProviderId::Deepseek => {
            vec![("authorization".into(), format!("Bearer {api_key}"))]
        }
    }
}

/// Sampling goes on the provider's own request shape.
/// OpenAI-compatible calls get temperature, top P, max tokens, and custom
/// params at the top level. Think budget is a real field for Anthropic and Gemini.
pub fn build_body(request: &CompletionRequest) -> Value {
    match request.provider {
        ProviderId::Anthropic => anthropic_body(request),
        ProviderId::Gemini => gemini_body(request),
        ProviderId::Local | ProviderId::Openai | ProviderId::Deepseek | ProviderId::Openrouter => {
            openai_body(request)
        }
    }
}

fn openai_body(request: &CompletionRequest) -> Value {
    let mut body = json!({
        "model": request.model,
        "messages": openai_messages(&request.messages),
        "stream": true,
        "stream_options": {"include_usage": true},
        "temperature": request.settings.temperature,
        "top_p": request.settings.top_p,
    });
    if request.provider == ProviderId::Openai {
        body["max_completion_tokens"] = json!(request.settings.max_tokens);
    } else {
        body["max_tokens"] = json!(request.settings.max_tokens);
    }
    if request.provider == ProviderId::Local {
        let enabled = request.settings.think;
        body["chat_template_kwargs"] = json!({ "enable_thinking": enabled });
        body["reasoning_effort"] = if enabled {
            json!(request.settings.effort.api_value())
        } else {
            json!("none")
        };
    } else if request.settings.think {
        body["reasoning_effort"] = json!(request.settings.effort.api_value());
    }
    merge_custom(
        &mut body,
        &request.settings,
        &["model", "messages", "stream"],
    );
    body
}

fn anthropic_body(request: &CompletionRequest) -> Value {
    let mut system = Vec::new();
    let mut messages: Vec<Value> = Vec::new();
    for message in &request.messages {
        if message.role == "system" {
            if !message.content.is_empty() {
                system.push(message.content.clone());
            }
            continue;
        }
        let role = if message.role == "assistant" {
            "assistant"
        } else {
            "user"
        };
        let mut content = Vec::new();
        for image in &message.images {
            content.push(json!({
                "type": "image",
                "source": {
                    "type": "base64",
                    "media_type": image.mime,
                    "data": image.data_base64,
                }
            }));
        }
        if !message.content.is_empty() || content.is_empty() {
            content.push(json!({"type": "text", "text": message.content}));
        }
        if let Some(last) = messages.last_mut() {
            if last.get("role").and_then(|item| item.as_str()) == Some(role) {
                if let Some(existing) = last.get_mut("content").and_then(|item| item.as_array_mut())
                {
                    existing.extend(content);
                    continue;
                }
            }
        }
        messages.push(json!({"role": role, "content": content}));
    }
    let budget = think_budget(&request.settings);
    let mut max_tokens = request.settings.max_tokens;
    if let Some(budget) = budget {
        if max_tokens <= budget {
            max_tokens = budget.saturating_add(1_024);
        }
    }
    let mut body = json!({
        "model": request.model,
        "max_tokens": max_tokens,
        "temperature": request.settings.temperature,
        "top_p": request.settings.top_p,
        "stream": true,
        "messages": messages,
    });
    if !system.is_empty() {
        body["system"] = json!(system.join("\n\n"));
    }
    if request.settings.think {
        body["thinking"] = json!({
            "type": "enabled",
            "budget_tokens": budget.unwrap_or_else(|| request.settings.effort.anthropic_budget()),
        });
    }
    merge_custom(
        &mut body,
        &request.settings,
        &["model", "messages", "stream", "system"],
    );
    body
}

fn gemini_body(request: &CompletionRequest) -> Value {
    let mut contents = Vec::new();
    let mut system = Vec::new();
    for message in &request.messages {
        if message.role == "system" {
            if !message.content.is_empty() {
                system.push(message.content.clone());
            }
            continue;
        }
        let role = if message.role == "assistant" {
            "model"
        } else {
            "user"
        };
        let mut parts = Vec::new();
        if !message.content.is_empty() {
            parts.push(json!({"text": message.content}));
        }
        for image in &message.images {
            parts.push(json!({
                "inlineData": {
                    "mimeType": image.mime,
                    "data": image.data_base64,
                }
            }));
        }
        if parts.is_empty() {
            parts.push(json!({"text": ""}));
        }
        contents.push(json!({"role": role, "parts": parts}));
    }
    let mut generation = json!({
        "temperature": request.settings.temperature,
        "topP": request.settings.top_p,
        "maxOutputTokens": request.settings.max_tokens,
    });
    if request.settings.think {
        let budget = think_budget(&request.settings)
            .unwrap_or_else(|| request.settings.effort.anthropic_budget());
        generation["thinkingConfig"] = json!({ "thinkingBudget": budget });
    } else {
        generation["thinkingConfig"] = json!({ "thinkingBudget": 0 });
    }
    if let Some(object) = generation.as_object_mut() {
        for (key, value) in &request.settings.custom_params {
            if key.is_empty() || key == "thinkingConfig" {
                continue;
            }
            object.insert(key.clone(), value.clone());
        }
    }
    let mut body = json!({
        "contents": contents,
        "generationConfig": generation,
    });
    if !system.is_empty() {
        body["systemInstruction"] = json!({
            "parts": [{"text": system.join("\n\n")}]
        });
    }
    body
}

fn think_budget(settings: &ChatSettings) -> Option<u32> {
    settings.think_budget.filter(|value| *value > 0)
}

fn merge_custom(body: &mut Value, settings: &ChatSettings, locked: &[&str]) {
    let Some(object) = body.as_object_mut() else {
        return;
    };
    for (key, value) in &settings.custom_params {
        if key.is_empty() || locked.contains(&key.as_str()) {
            continue;
        }
        object.insert(key.clone(), value.clone());
    }
}

fn openai_messages(messages: &[OutgoingMessage]) -> Value {
    let items = messages
        .iter()
        .map(|message| {
            if message.images.is_empty() {
                json!({
                    "role": message.role,
                    "content": message.content,
                })
            } else {
                let mut parts = Vec::new();
                if !message.content.is_empty() {
                    parts.push(json!({"type": "text", "text": message.content}));
                }
                for image in &message.images {
                    let url = format!("data:{};base64,{}", image.mime, image.data_base64);
                    parts.push(json!({
                        "type": "image_url",
                        "image_url": {"url": url}
                    }));
                }
                json!({"role": message.role, "content": parts})
            }
        })
        .collect::<Vec<_>>();
    Value::Array(items)
}

pub fn redact(secret: &str, text: &str) -> String {
    if secret.trim().len() < 4 {
        return text.to_string();
    }
    text.replace(secret, "•••")
}

pub fn friendly_http_error(status: u16, body: &str) -> String {
    let detail = extract_error_message(body);
    let detail = if detail.is_empty() {
        String::new()
    } else {
        format!(" {detail}")
    };
    match status {
        401 | 403 => format!("That key was rejected.{detail}"),
        404 => format!("The model or endpoint was not found.{detail}"),
        413 => "That request is too large for the provider.".into(),
        429 => {
            format!("The provider is rate limiting requests. Wait a moment and try again.{detail}")
        }
        500..=599 => format!("The provider had an internal error ({status}).{detail}"),
        _ => format!("The provider returned HTTP {status}.{detail}"),
    }
}

pub fn friendly_transport(provider: ProviderId, detail: &str) -> String {
    let lower = detail.to_lowercase();
    if provider == ProviderId::Local
        && (lower.contains("connection refused") || lower.contains("connect"))
    {
        return "The local llama-server is not running. Check Settings, or point at a server that is already up.".into();
    }
    format!("Could not reach {}. {}", provider.label(), collapse(detail))
}

fn extract_error_message(body: &str) -> String {
    if let Ok(value) = serde_json::from_str::<Value>(body) {
        let message = value
            .pointer("/error/message")
            .or_else(|| value.pointer("/error/error/message"))
            .or_else(|| value.get("message"))
            .and_then(|item| item.as_str());
        if let Some(message) = message {
            return collapse(message);
        }
        if let Some(text) = value.get("error").and_then(|item| item.as_str()) {
            return collapse(text);
        }
    }
    collapse(body)
}

fn collapse(text: &str) -> String {
    let one = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if one.chars().count() > 400 {
        let clipped: String = one.chars().take(400).collect();
        format!("{clipped}…")
    } else {
        one
    }
}

fn path_encode(value: &str) -> String {
    let mut out = String::new();
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char);
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

pub async fn stream_completion<F>(
    client: &reqwest::Client,
    request: &CompletionRequest,
    api_key: &str,
    cancel: &AtomicBool,
    mut on_piece: F,
) -> Result<StreamOutcome, CoreError>
where
    F: FnMut(&StreamPiece),
{
    if request.provider.needs_key() && api_key.trim().is_empty() {
        return Err(CoreError::Message(format!(
            "Add a {} key in Settings first.",
            request.provider.label()
        )));
    }
    if request.model.trim().is_empty() {
        return Err(CoreError::Message("Pick a model first.".into()));
    }
    let url = completion_url(request.provider, &request.origin, &request.model);
    let body = build_body(request);
    let mut builder = client.post(&url).json(&body);
    for (key, value) in build_headers(request.provider, api_key) {
        builder = builder.header(key, value);
    }
    let response = builder.send().await.map_err(|error| {
        CoreError::Message(friendly_transport(request.provider, &error.to_string()))
    })?;
    let status = response.status();
    if !status.is_success() {
        let text = response.text().await.unwrap_or_default();
        return Err(CoreError::Message(friendly_http_error(
            status.as_u16(),
            &redact(api_key, &text),
        )));
    }
    let mut stream = response.bytes_stream();
    let mut parser = SseBuffer::default();
    let mut outcome = StreamOutcome::default();
    while let Some(item) = stream.next().await {
        if cancel.load(Ordering::Relaxed) {
            outcome.cancelled = true;
            break;
        }
        let bytes = item.map_err(|error| CoreError::Message(error.to_string()))?;
        let text = String::from_utf8_lossy(&bytes);
        for piece in parser.push(&text, request.provider) {
            on_piece(&piece);
            apply_piece(&mut outcome, piece);
        }
    }
    for piece in parser.finish(request.provider) {
        on_piece(&piece);
        apply_piece(&mut outcome, piece);
    }
    if outcome.text.is_empty() && outcome.thinking.is_empty() {
        if let Some(error) = outcome.error.clone() {
            return Err(CoreError::Message(error));
        }
    }
    Ok(outcome)
}

fn apply_piece(outcome: &mut StreamOutcome, piece: StreamPiece) {
    match piece {
        StreamPiece::Text(text) => outcome.text.push_str(&text),
        StreamPiece::Thinking(text) => outcome.thinking.push_str(&text),
        StreamPiece::Usage { prompt, completion } => {
            outcome.prompt_tokens = Some(prompt);
            outcome.completion_tokens = Some(completion);
        }
        StreamPiece::Error(message) => outcome.error = Some(message),
    }
}

pub async fn test_provider(
    client: &reqwest::Client,
    provider: ProviderId,
    origin: &str,
    api_key: &str,
) -> Result<String, CoreError> {
    if provider.needs_key() && api_key.trim().is_empty() {
        return Err(CoreError::Message(format!(
            "Add a {} key in Settings first.",
            provider.label()
        )));
    }
    let url = test_url(provider, origin);
    let mut builder = client.get(&url).timeout(Duration::from_secs(20));
    for (key, value) in build_headers(provider, api_key) {
        builder = builder.header(key, value);
    }
    let response = builder
        .send()
        .await
        .map_err(|error| CoreError::Message(friendly_transport(provider, &error.to_string())))?;
    let status = response.status();
    if status.is_success() {
        return Ok(format!("Connected to {}.", provider.label()));
    }
    let text = response.text().await.unwrap_or_default();
    Err(CoreError::Message(friendly_http_error(
        status.as_u16(),
        &redact(api_key, &text),
    )))
}

pub fn parse_model_ids(provider: ProviderId, body: &str) -> Result<Vec<String>, CoreError> {
    let value: Value = serde_json::from_str(body)?;
    Ok(crate::parse_model_list(provider, &value))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Effort;
    use serde_json::json;

    fn text_request(provider: ProviderId) -> CompletionRequest {
        let mut settings = ChatSettings::default();
        settings.temperature = 0.4;
        settings.top_p = 0.8;
        settings.max_tokens = 256;
        settings.custom_params.insert("min_p".into(), json!(0.05));
        CompletionRequest {
            provider,
            model: "demo-model".into(),
            messages: vec![OutgoingMessage {
                role: "user".into(),
                content: "Hello".into(),
                images: Vec::new(),
            }],
            settings,
            origin: "http://127.0.0.1:9".into(),
        }
    }

    #[test]
    fn openai_sampling_is_top_level() {
        let body = build_body(&text_request(ProviderId::Openai));
        assert_eq!(body["temperature"], 0.4);
        assert_eq!(body["top_p"], 0.8);
        assert_eq!(body["max_completion_tokens"], 256);
        assert!(body.get("max_tokens").is_none());
        let deepseek = build_body(&text_request(ProviderId::Deepseek));
        assert_eq!(deepseek["max_tokens"], 256);
        assert_eq!(body["min_p"], 0.05);
        assert!(body.get("options").is_none());
        assert!(body.get("reasoning_effort").is_none());
        let headers = build_headers(ProviderId::Openai, "sk-secret");
        assert!(headers
            .iter()
            .any(|(key, value)| key == "authorization" && value == "Bearer sk-secret"));
        assert!(!body.to_string().contains("sk-secret"));
    }

    #[test]
    fn local_think_toggle_sets_reasoning_effort() {
        let mut request = text_request(ProviderId::Local);
        request.settings.think = false;
        let off = build_body(&request);
        assert_eq!(off["reasoning_effort"], "none");
        assert_eq!(off["chat_template_kwargs"]["enable_thinking"], false);

        request.settings.think = true;
        request.settings.effort = Effort::High;
        let on = build_body(&request);
        assert_eq!(on["reasoning_effort"], "high");
        assert_eq!(on["chat_template_kwargs"]["enable_thinking"], true);
    }

    #[test]
    fn anthropic_sends_think_budget() {
        let mut request = text_request(ProviderId::Anthropic);
        request.settings.think = true;
        request.settings.think_budget = Some(2_048);
        request.settings.max_tokens = 256;
        let body = build_body(&request);
        assert_eq!(body["thinking"]["budget_tokens"], 2_048);
        assert!(body["max_tokens"].as_u64().unwrap() > 2_048);
        assert_eq!(body["temperature"], 0.4);
        assert_eq!(body["top_p"], 0.8);
    }

    #[test]
    fn gemini_puts_sampling_in_generation_config() {
        let mut request = text_request(ProviderId::Gemini);
        request.model = "gemini-2.5-flash".into();
        request.settings.think = false;
        let body = build_body(&request);
        assert_eq!(body["generationConfig"]["temperature"], 0.4);
        assert_eq!(body["generationConfig"]["topP"], 0.8);
        assert_eq!(body["generationConfig"]["maxOutputTokens"], 256);
        assert_eq!(
            body["generationConfig"]["thinkingConfig"]["thinkingBudget"],
            0
        );
        assert_eq!(body["generationConfig"]["min_p"], 0.05);
        let url = completion_url(
            ProviderId::Gemini,
            "https://example.test",
            "gemini-2.5-flash",
        );
        assert!(url.contains("/models/gemini-2.5-flash:streamGenerateContent"));
    }

    #[test]
    fn bad_key_error_is_readable_and_redacts_the_secret() {
        let message = friendly_http_error(
            401,
            r#"{"error":{"message":"invalid api key sk-secret-value"}}"#,
        );
        let message = redact("sk-secret-value", &message);
        assert!(message.contains("That key was rejected."));
        assert!(!message.contains("sk-secret-value"));
    }

    #[tokio::test]
    async fn hosted_reply_streams_from_an_openai_compatible_server() {
        let body = concat!(
            "data: {\"choices\":[{\"delta\":{\"content\":\"Hel\"}}]}\n\n",
            "data: {\"choices\":[{\"delta\":{\"content\":\"lo\"}}]}\n\n",
            "data: [DONE]\n\n"
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut buf = vec![0u8; 4096];
            let _ = tokio::io::AsyncReadExt::read(&mut socket, &mut buf).await;
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            let _ = tokio::io::AsyncWriteExt::write_all(&mut socket, response.as_bytes()).await;
        });
        let client = reqwest::Client::new();
        let mut request = text_request(ProviderId::Openai);
        request.origin = format!("http://{addr}");
        let outcome = stream_completion(
            &client,
            &request,
            "sk-test",
            &AtomicBool::new(false),
            |_| {},
        )
        .await
        .unwrap();
        assert_eq!(outcome.text, "Hello");
    }
}
