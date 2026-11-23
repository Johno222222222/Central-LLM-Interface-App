use std::path::Path;

use rusqlite::{params, Connection};
use uuid::Uuid;

use crate::{
    AppSettings, Attachment, Chat, ChatMessage, ChatSettings, ChatSummary, CoreError, ProviderId,
};

pub fn now_stamp() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

pub struct Store {
    conn: Connection,
}

impl Store {
    pub fn open(path: &Path, defaults: &AppSettings) -> Result<Self, CoreError> {
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent)
                    .map_err(|error| CoreError::Message(error.to_string()))?;
            }
        }
        let conn = Connection::open(path)?;
        let store = Self { conn };
        store.migrate(defaults)?;
        Ok(store)
    }

    pub fn open_memory(defaults: &AppSettings) -> Result<Self, CoreError> {
        let conn = Connection::open_in_memory()?;
        let store = Self { conn };
        store.migrate(defaults)?;
        Ok(store)
    }

    fn migrate(&self, defaults: &AppSettings) -> Result<(), CoreError> {
        self.conn.execute_batch(
            "
            PRAGMA foreign_keys = ON;
            CREATE TABLE IF NOT EXISTS settings (
                id INTEGER PRIMARY KEY CHECK (id = 1),
                llama_server_url TEXT NOT NULL,
                comfy_host TEXT NOT NULL,
                comfy_port INTEGER NOT NULL,
                comfy_path TEXT NOT NULL,
                models_dir TEXT NOT NULL,
                output_dir TEXT NOT NULL,
                appearance TEXT NOT NULL,
                context_length INTEGER NOT NULL,
                gpu_layers INTEGER NOT NULL
            );
            CREATE TABLE IF NOT EXISTS chats (
                id TEXT PRIMARY KEY,
                title TEXT NOT NULL,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                model_id TEXT NOT NULL,
                provider TEXT NOT NULL,
                settings_json TEXT NOT NULL,
                context_used INTEGER NOT NULL,
                context_limit INTEGER NOT NULL
            );
            CREATE TABLE IF NOT EXISTS messages (
                id TEXT PRIMARY KEY,
                chat_id TEXT NOT NULL REFERENCES chats(id) ON DELETE CASCADE,
                role TEXT NOT NULL,
                content TEXT NOT NULL,
                thinking TEXT,
                attachments_json TEXT NOT NULL,
                created_at TEXT NOT NULL,
                provider TEXT,
                model_id TEXT
            );
            ",
        )?;
        let count: i64 = self
            .conn
            .query_row("SELECT COUNT(*) FROM settings", [], |row| row.get(0))?;
        if count == 0 {
            self.save_settings(defaults)?;
        }
        Ok(())
    }

    pub fn get_settings(&self) -> Result<AppSettings, CoreError> {
        self.conn.query_row(
            "SELECT llama_server_url, comfy_host, comfy_port, comfy_path, models_dir, output_dir, appearance, context_length, gpu_layers FROM settings WHERE id = 1",
            [],
            |row| {
                let appearance: String = row.get(6)?;
                Ok(AppSettings {
                    llama_server_url: row.get(0)?,
                    comfy_host: row.get(1)?,
                    comfy_port: row.get(2)?,
                    comfy_path: row.get(3)?,
                    models_dir: row.get(4)?,
                    output_dir: row.get(5)?,
                    appearance: parse_appearance(&appearance),
                    context_length: row.get(7)?,
                    gpu_layers: row.get(8)?,
                })
            },
        ).map_err(CoreError::from)
    }

    pub fn save_settings(&self, settings: &AppSettings) -> Result<(), CoreError> {
        self.conn.execute(
            "INSERT INTO settings (id, llama_server_url, comfy_host, comfy_port, comfy_path, models_dir, output_dir, appearance, context_length, gpu_layers)
             VALUES (1, ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
             ON CONFLICT(id) DO UPDATE SET
                llama_server_url = excluded.llama_server_url,
                comfy_host = excluded.comfy_host,
                comfy_port = excluded.comfy_port,
                comfy_path = excluded.comfy_path,
                models_dir = excluded.models_dir,
                output_dir = excluded.output_dir,
                appearance = excluded.appearance,
                context_length = excluded.context_length,
                gpu_layers = excluded.gpu_layers",
            params![
                settings.llama_server_url,
                settings.comfy_host,
                settings.comfy_port,
                settings.comfy_path,
                settings.models_dir,
                settings.output_dir,
                appearance_name(settings.appearance),
                settings.context_length,
                settings.gpu_layers,
            ],
        )?;
        Ok(())
    }

    pub fn list_chats(&self) -> Result<Vec<ChatSummary>, CoreError> {
        let mut stmt = self.conn.prepare(
            "SELECT id, title, created_at, updated_at, model_id, provider, context_used, context_limit
             FROM chats ORDER BY updated_at DESC",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(ChatSummary {
                id: row.get(0)?,
                title: row.get(1)?,
                created_at: row.get(2)?,
                updated_at: row.get(3)?,
                model_id: row.get(4)?,
                provider: parse_provider(&row.get::<_, String>(5)?),
                context_used: row.get(6)?,
                context_limit: row.get(7)?,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(CoreError::from)
    }

    pub fn get_chat(&self, id: &str) -> Result<Chat, CoreError> {
        let summary = self.chat_summary(id)?;
        let settings = self.chat_settings(id)?;
        let mut stmt = self.conn.prepare(
            "SELECT id, role, content, thinking, attachments_json, created_at, provider, model_id
             FROM messages WHERE chat_id = ?1 ORDER BY created_at ASC, rowid ASC",
        )?;
        let messages = stmt
            .query_map(params![id], |row| {
                let attachments: String = row.get(4)?;
                let provider: Option<String> = row.get(6)?;
                Ok(ChatMessage {
                    id: row.get(0)?,
                    role: row.get(1)?,
                    content: row.get(2)?,
                    thinking: row.get(3)?,
                    attachments: serde_json::from_str(&attachments).unwrap_or_default(),
                    created_at: row.get(5)?,
                    provider: provider.as_deref().and_then(ProviderId::from_slug),
                    model_id: row.get(7)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Chat {
            id: summary.id,
            title: summary.title,
            created_at: summary.created_at,
            updated_at: summary.updated_at,
            model_id: summary.model_id,
            provider: summary.provider,
            messages,
            settings,
            context_used: summary.context_used,
            context_limit: summary.context_limit,
        })
    }

    pub fn create_chat(
        &self,
        provider: ProviderId,
        model_id: &str,
        context_limit: u32,
    ) -> Result<Chat, CoreError> {
        let id = Uuid::new_v4().to_string();
        let stamp = now_stamp();
        let settings = ChatSettings::default();
        let settings_json = serde_json::to_string(&settings)?;
        self.conn.execute(
            "INSERT INTO chats (id, title, created_at, updated_at, model_id, provider, settings_json, context_used, context_limit)
             VALUES (?1, 'New chat', ?2, ?2, ?3, ?4, ?5, 0, ?6)",
            params![id, stamp, model_id, provider.slug(), settings_json, context_limit],
        )?;
        self.get_chat(&id)
    }

    pub fn delete_chat(&self, id: &str) -> Result<(), CoreError> {
        self.conn
            .execute("DELETE FROM chats WHERE id = ?1", params![id])?;
        Ok(())
    }

    pub fn update_chat_model(
        &self,
        id: &str,
        provider: ProviderId,
        model_id: &str,
        context_limit: u32,
    ) -> Result<(), CoreError> {
        self.conn.execute(
            "UPDATE chats SET provider = ?1, model_id = ?2, context_limit = ?3, updated_at = ?4 WHERE id = ?5",
            params![provider.slug(), model_id, context_limit, now_stamp(), id],
        )?;
        Ok(())
    }

    pub fn update_chat_settings(&self, id: &str, settings: &ChatSettings) -> Result<(), CoreError> {
        let json = serde_json::to_string(settings)?;
        self.conn.execute(
            "UPDATE chats SET settings_json = ?1, updated_at = ?2 WHERE id = ?3",
            params![json, now_stamp(), id],
        )?;
        Ok(())
    }

    pub fn insert_message(&self, chat_id: &str, message: &ChatMessage) -> Result<(), CoreError> {
        let attachments = serde_json::to_string(&message.attachments)?;
        self.conn.execute(
            "INSERT INTO messages (id, chat_id, role, content, thinking, attachments_json, created_at, provider, model_id)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                message.id,
                chat_id,
                message.role,
                message.content,
                message.thinking,
                attachments,
                message.created_at,
                message.provider.map(|provider| provider.slug().to_string()),
                message.model_id,
            ],
        )?;
        Ok(())
    }

    pub fn update_message(
        &self,
        id: &str,
        content: &str,
        thinking: Option<&str>,
    ) -> Result<(), CoreError> {
        self.conn.execute(
            "UPDATE messages SET content = ?1, thinking = ?2 WHERE id = ?3",
            params![content, thinking, id],
        )?;
        Ok(())
    }

    pub fn touch_chat(
        &self,
        id: &str,
        title: Option<&str>,
        context_used: u32,
        context_limit: u32,
    ) -> Result<(), CoreError> {
        if let Some(title) = title {
            self.conn.execute(
                "UPDATE chats SET title = ?1, updated_at = ?2, context_used = ?3, context_limit = ?4 WHERE id = ?5",
                params![title, now_stamp(), context_used, context_limit, id],
            )?;
        } else {
            self.conn.execute(
                "UPDATE chats SET updated_at = ?1, context_used = ?2, context_limit = ?3 WHERE id = ?4",
                params![now_stamp(), context_used, context_limit, id],
            )?;
        }
        Ok(())
    }

    fn chat_summary(&self, id: &str) -> Result<ChatSummary, CoreError> {
        self.conn
            .query_row(
                "SELECT id, title, created_at, updated_at, model_id, provider, context_used, context_limit FROM chats WHERE id = ?1",
                params![id],
                |row| {
                    Ok(ChatSummary {
                        id: row.get(0)?,
                        title: row.get(1)?,
                        created_at: row.get(2)?,
                        updated_at: row.get(3)?,
                        model_id: row.get(4)?,
                        provider: parse_provider(&row.get::<_, String>(5)?),
                        context_used: row.get(6)?,
                        context_limit: row.get(7)?,
                    })
                },
            )
            .map_err(|_| CoreError::Message("That chat is not on this computer.".into()))
    }

    fn chat_settings(&self, id: &str) -> Result<ChatSettings, CoreError> {
        let json: String = self.conn.query_row(
            "SELECT settings_json FROM chats WHERE id = ?1",
            params![id],
            |row| row.get(0),
        )?;
        Ok(serde_json::from_str(&json).unwrap_or_default())
    }
}

pub fn new_message(
    role: &str,
    content: &str,
    attachments: Vec<Attachment>,
    provider: Option<ProviderId>,
    model_id: Option<String>,
) -> ChatMessage {
    ChatMessage {
        id: Uuid::new_v4().to_string(),
        role: role.to_string(),
        content: content.to_string(),
        thinking: None,
        attachments,
        created_at: now_stamp(),
        provider,
        model_id,
    }
}

fn appearance_name(appearance: crate::Appearance) -> &'static str {
    match appearance {
        crate::Appearance::System => "system",
        crate::Appearance::Light => "light",
        crate::Appearance::Dark => "dark",
    }
}

fn parse_appearance(value: &str) -> crate::Appearance {
    match value {
        "system" => crate::Appearance::System,
        "light" => crate::Appearance::Light,
        _ => crate::Appearance::Dark,
    }
}

fn parse_provider(value: &str) -> ProviderId {
    ProviderId::from_slug(value).unwrap_or(ProviderId::Local)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::MemorySecretStore;
    use crate::SecretStore;

    #[test]
    fn a_thread_survives_reopening_the_database() {
        let dir = std::env::temp_dir().join(format!("cllm-db-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("chats.db");
        let models = dir.join("models");
        let outputs = dir.join("outputs");
        let defaults =
            AppSettings::fresh(models.display().to_string(), outputs.display().to_string());
        let chat_id;
        {
            let store = Store::open(&path, &defaults).unwrap();
            let chat = store
                .create_chat(ProviderId::Local, "Qwen2.5-7B-Q4_K_M.gguf", 8192)
                .unwrap();
            chat_id = chat.id.clone();
            let message = new_message("user", "Remember this", Vec::new(), None, None);
            store.insert_message(&chat.id, &message).unwrap();
            store
                .touch_chat(&chat.id, Some("Remember this"), 4, 8192)
                .unwrap();
        }
        let store = Store::open(&path, &defaults).unwrap();
        let chat = store.get_chat(&chat_id).unwrap();
        assert_eq!(chat.messages.len(), 1);
        assert_eq!(chat.messages[0].content, "Remember this");
        assert_eq!(chat.title, "Remember this");
        assert_eq!(chat.model_id, "Qwen2.5-7B-Q4_K_M.gguf");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn api_keys_are_not_written_into_the_database() {
        let secret = "sk-test-DO_NOT_STORE_9f3a";
        let secrets = MemorySecretStore::default();
        secrets.set_key("openai", secret).unwrap();
        let dir = std::env::temp_dir().join(format!("cllm-secret-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("chats.db");
        let defaults = AppSettings::fresh(
            dir.join("models").display().to_string(),
            dir.join("out").display().to_string(),
        );
        let store = Store::open(&path, &defaults).unwrap();
        let saved = serde_json::to_value(store.get_settings().unwrap()).unwrap();
        for key in saved.as_object().unwrap().keys() {
            let lower = key.to_lowercase();
            assert!(!lower.contains("secret"), "{key}");
            assert!(!lower.contains("password"), "{key}");
            assert!(
                !lower.contains("apikey") && !lower.contains("api_key"),
                "{key}"
            );
        }
        store
            .create_chat(ProviderId::Openai, "gpt-4.1-mini", 128_000)
            .unwrap();
        let bytes = std::fs::read(&path).unwrap();
        let text = String::from_utf8_lossy(&bytes);
        assert!(!text.contains(secret));
        assert_eq!(secrets.get_key("openai").unwrap().as_deref(), Some(secret));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
