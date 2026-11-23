use std::fs;
use std::path::Path;

use serde_json::Value;

use crate::gguf::read_gguf_meta;
use crate::{CatalogModel, LoadedModel, ProviderId};

pub fn family_from_filename(name: &str) -> String {
    let n = name.to_lowercase();
    const FAMILIES: &[(&str, &str)] = &[
        ("llama", "Llama"),
        ("qwen", "Qwen"),
        ("mistral", "Mistral"),
        ("mixtral", "Mistral"),
        ("gemma", "Gemma"),
        ("phi-", "Phi"),
        ("phi3", "Phi"),
        ("phi4", "Phi"),
        ("deepseek", "DeepSeek"),
        ("command-r", "Command"),
        ("glm", "GLM"),
        ("yi-", "Yi"),
        ("internlm", "InternLM"),
        ("olmo", "Olmo"),
        ("granite", "Granite"),
        ("smollm", "SmolLM"),
        ("stablelm", "StableLM"),
        ("kimi", "Kimi"),
    ];
    for (needle, label) in FAMILIES {
        if n.contains(needle) {
            return (*label).to_string();
        }
    }
    "Other".to_string()
}

pub fn family_for_model_id(provider: ProviderId, model_id: &str) -> String {
    let id = model_id.to_lowercase();
    let bare = id.rsplit('/').next().unwrap_or(&id);
    match provider {
        ProviderId::Local => family_from_filename(model_id),
        ProviderId::Openrouter => match id.split('/').next() {
            Some("openai") => "OpenAI".into(),
            Some("anthropic") => "Anthropic".into(),
            Some("google") => "Google".into(),
            Some("deepseek") => "DeepSeek".into(),
            Some("meta-llama") => "Llama".into(),
            Some("qwen") => "Qwen".into(),
            Some(other) => title_word(other),
            None => "Other".into(),
        },
        ProviderId::Openai => {
            if bare.starts_with("gpt-5") {
                "GPT-5".into()
            } else if bare.starts_with("gpt-4.1") {
                "GPT-4.1".into()
            } else if bare.starts_with("gpt-4o") {
                "GPT-4o".into()
            } else if bare.starts_with("gpt-4") {
                "GPT-4".into()
            } else if bare.starts_with('o') {
                "Reasoning".into()
            } else {
                "OpenAI".into()
            }
        }
        ProviderId::Anthropic => "Claude".into(),
        ProviderId::Gemini => {
            if bare.contains("flash") {
                "Flash".into()
            } else {
                "Gemini".into()
            }
        }
        ProviderId::Deepseek => {
            if bare.contains("reason") {
                "Reasoner".into()
            } else {
                "Chat".into()
            }
        }
    }
}

fn title_word(value: &str) -> String {
    let mut chars = value.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => "Other".into(),
    }
}

pub fn default_context_limit(provider: ProviderId, model_id: &str) -> u32 {
    let id = model_id.to_lowercase();
    match provider {
        ProviderId::Local => 8_192,
        ProviderId::Anthropic => 200_000,
        ProviderId::Gemini => {
            if id.contains("flash") {
                1_000_000
            } else {
                1_000_000
            }
        }
        ProviderId::Openai => 128_000,
        ProviderId::Deepseek => 128_000,
        ProviderId::Openrouter => 128_000,
    }
}

pub fn hosted_catalog() -> Vec<CatalogModel> {
    const SEEDS: &[(ProviderId, &str, &str, u32)] = &[
        (ProviderId::Openai, "GPT-4.1", "gpt-4.1", 128_000),
        (ProviderId::Openai, "GPT-4.1", "gpt-4.1-mini", 128_000),
        (ProviderId::Openai, "GPT-4o", "gpt-4o", 128_000),
        (ProviderId::Openai, "GPT-4o", "gpt-4o-mini", 128_000),
        (ProviderId::Openai, "Reasoning", "o4-mini", 200_000),
        (ProviderId::Openai, "Reasoning", "o3", 200_000),
        (
            ProviderId::Anthropic,
            "Claude",
            "claude-sonnet-4-5",
            200_000,
        ),
        (ProviderId::Anthropic, "Claude", "claude-opus-4-5", 200_000),
        (ProviderId::Anthropic, "Claude", "claude-haiku-4-5", 200_000),
        (ProviderId::Gemini, "Gemini", "gemini-2.5-pro", 1_000_000),
        (ProviderId::Gemini, "Flash", "gemini-2.5-flash", 1_000_000),
        (ProviderId::Deepseek, "Chat", "deepseek-chat", 128_000),
        (
            ProviderId::Deepseek,
            "Reasoner",
            "deepseek-reasoner",
            128_000,
        ),
        (ProviderId::Openrouter, "OpenAI", "openai/gpt-4.1", 128_000),
        (
            ProviderId::Openrouter,
            "Anthropic",
            "anthropic/claude-sonnet-4.5",
            200_000,
        ),
        (
            ProviderId::Openrouter,
            "Google",
            "google/gemini-2.5-flash",
            1_000_000,
        ),
        (
            ProviderId::Openrouter,
            "DeepSeek",
            "deepseek/deepseek-chat",
            128_000,
        ),
    ];
    SEEDS
        .iter()
        .map(|(provider, family, id, context)| CatalogModel {
            id: (*id).to_string(),
            name: (*id).to_string(),
            provider: *provider,
            family: (*family).to_string(),
            context_limit: *context,
            installed: true,
            path: None,
            loaded: false,
            key_saved: false,
        })
        .collect()
}

pub fn scan_gguf_dir(dir: &Path) -> Vec<CatalogModel> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut models = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let Some(name) = path.file_name().and_then(|item| item.to_str()) else {
            continue;
        };
        if !name.to_lowercase().ends_with(".gguf") {
            continue;
        }
        let lower = name.to_lowercase();
        if lower.contains("mmproj") {
            continue;
        }
        let meta = read_gguf_meta(&path);
        let context_limit = meta.and_then(|item| item.context_length).unwrap_or(8_192);
        models.push(CatalogModel {
            id: name.to_string(),
            name: name.to_string(),
            provider: ProviderId::Local,
            family: family_from_filename(name),
            context_limit,
            installed: true,
            path: Some(path.display().to_string()),
            loaded: false,
            key_saved: true,
        });
    }
    models.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    models
}

pub fn select_model_ids(provider: ProviderId, mut ids: Vec<String>) -> Vec<String> {
    ids.sort();
    ids.dedup();
    if provider != ProviderId::Openrouter {
        ids.truncate(60);
        return ids;
    }
    let preferred = [
        "anthropic/",
        "openai/",
        "google/",
        "deepseek/",
        "qwen/",
        "meta-llama/",
    ];
    let mut picked = Vec::new();
    for prefix in preferred {
        for id in &ids {
            if id.starts_with(prefix) {
                picked.push(id.clone());
            }
            if picked.len() == 40 {
                return picked;
            }
        }
    }
    if picked.is_empty() {
        ids.truncate(40);
        return ids;
    }
    picked
}

pub fn catalog_from_ids(provider: ProviderId, ids: &[String]) -> Vec<CatalogModel> {
    ids.iter()
        .map(|id| CatalogModel {
            name: id.clone(),
            family: family_for_model_id(provider, id),
            context_limit: default_context_limit(provider, id),
            provider,
            installed: true,
            path: None,
            loaded: false,
            key_saved: true,
            id: id.clone(),
        })
        .collect()
}

pub fn parse_model_list(provider: ProviderId, body: &Value) -> Vec<String> {
    let raw = if provider == ProviderId::Gemini {
        body.get("models")
            .and_then(|item| item.as_array())
            .map(|items| {
                items
                    .iter()
                    .filter_map(|item| item.get("name").and_then(|name| name.as_str()))
                    .map(|name| name.trim_start_matches("models/").to_string())
                    .filter(|name| name.contains("gemini"))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default()
    } else {
        body.get("data")
            .and_then(|item| item.as_array())
            .map(|items| {
                items
                    .iter()
                    .filter_map(|item| item.get("id").and_then(|id| id.as_str()))
                    .map(str::to_string)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default()
    };
    select_model_ids(provider, raw)
}

/// Models the router currently has in memory. `GET /models` on llama-server.
pub fn parse_router_models(body: &Value) -> Vec<LoadedModel> {
    let Some(data) = body.get("data").and_then(|item| item.as_array()) else {
        return Vec::new();
    };
    data.iter()
        .filter_map(|item| {
            let id = item.get("id").and_then(|id| id.as_str())?.to_string();
            let status = item
                .pointer("/status/value")
                .and_then(|value| value.as_str())
                .unwrap_or("unloaded")
                .to_string();
            let context_limit = item
                .pointer("/memory/effective/n_ctx")
                .or_else(|| item.pointer("/memory/max/n_ctx"))
                .and_then(|value| value.as_u64())
                .map(|value| value as u32);
            Some(LoadedModel {
                id,
                status,
                context_limit,
            })
        })
        .collect()
}

pub fn loaded_ids(models: &[LoadedModel]) -> Vec<String> {
    models
        .iter()
        .filter(|model| matches!(model.status.as_str(), "loaded" | "sleeping" | "loading"))
        .map(|model| model.id.clone())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn groups_local_files_by_family_and_skips_mmproj() {
        let dir = std::env::temp_dir().join(format!("cllm-models-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("Qwen2.5-7B-Instruct-Q5_K_M.gguf"), b"gguf").unwrap();
        fs::write(dir.join("mmproj-qwen.gguf"), b"gguf").unwrap();
        fs::write(dir.join("notes.txt"), b"nope").unwrap();
        let models = scan_gguf_dir(&dir);
        let _ = fs::remove_dir_all(&dir);
        assert_eq!(models.len(), 1);
        assert_eq!(models[0].family, "Qwen");
        assert_eq!(models[0].provider, ProviderId::Local);
    }

    #[test]
    fn router_unload_list_ignores_unloaded_models() {
        let body = serde_json::json!({
            "data": [
                {"id": "a.gguf", "status": {"value": "loaded"}, "memory": {"effective": {"n_ctx": 8192}}},
                {"id": "b.gguf", "status": {"value": "unloaded"}},
                {"id": "c.gguf", "status": {"value": "sleeping"}}
            ]
        });
        let parsed = parse_router_models(&body);
        assert_eq!(
            loaded_ids(&parsed),
            vec!["a.gguf".to_string(), "c.gguf".to_string()]
        );
        assert_eq!(parsed[0].context_limit, Some(8192));
    }

    #[test]
    fn openrouter_refresh_prefers_known_families() {
        let ids = vec![
            "some-lab/tiny".into(),
            "openai/gpt-4.1".into(),
            "anthropic/claude-sonnet-4.5".into(),
        ];
        let picked = select_model_ids(ProviderId::Openrouter, ids);
        assert_eq!(picked[0], "anthropic/claude-sonnet-4.5");
        assert!(picked.iter().any(|id| id.starts_with("openai/")));
    }
}
