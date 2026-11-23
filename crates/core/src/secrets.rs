use std::collections::HashMap;
use std::sync::Mutex;

use crate::{CoreError, ProviderId};

/// Keys stay behind this trait. The Windows shell stores them in Credential
/// Manager. Tests use the in-memory store. Nothing here writes a file.
pub trait SecretStore: Send + Sync {
    fn set_key(&self, provider: &str, secret: &str) -> Result<(), CoreError>;
    fn get_key(&self, provider: &str) -> Result<Option<String>, CoreError>;
    fn delete_key(&self, provider: &str) -> Result<(), CoreError>;

    fn has_key(&self, provider: &str) -> Result<bool, CoreError> {
        Ok(self.get_key(provider)?.is_some())
    }
}

pub fn require_hosted(slug: &str) -> Result<ProviderId, CoreError> {
    match ProviderId::from_slug(slug) {
        Some(id) if id.needs_key() => Ok(id),
        Some(_) => Err(CoreError::Message(
            "Local models do not use an API key.".into(),
        )),
        None => Err(CoreError::Message("Unknown provider.".into())),
    }
}

#[derive(Default)]
pub struct MemorySecretStore {
    keys: Mutex<HashMap<String, String>>,
}

impl SecretStore for MemorySecretStore {
    fn set_key(&self, provider: &str, secret: &str) -> Result<(), CoreError> {
        let provider = require_hosted(provider)?.slug().to_string();
        if secret.trim().is_empty() {
            return Err(CoreError::Message("Enter a key before saving.".into()));
        }
        self.keys
            .lock()
            .expect("secret store lock")
            .insert(provider, secret.to_string());
        Ok(())
    }

    fn get_key(&self, provider: &str) -> Result<Option<String>, CoreError> {
        let provider = require_hosted(provider)?.slug();
        Ok(self
            .keys
            .lock()
            .expect("secret store lock")
            .get(provider)
            .cloned())
    }

    fn delete_key(&self, provider: &str) -> Result<(), CoreError> {
        let provider = require_hosted(provider)?.slug();
        self.keys
            .lock()
            .expect("secret store lock")
            .remove(provider);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn memory_store_roundtrip_does_not_touch_disk() {
        let store = MemorySecretStore::default();
        let secret = "sk-test-DO_NOT_STORE_9f3a";
        store.set_key("openai", secret).unwrap();
        assert_eq!(store.get_key("openai").unwrap().as_deref(), Some(secret));
        store.delete_key("openai").unwrap();
        assert_eq!(store.get_key("openai").unwrap(), None);
    }

    #[test]
    fn rejects_unknown_provider() {
        let store = MemorySecretStore::default();
        assert!(store.set_key("local", "nope").is_err());
        assert!(store.set_key("not-a-provider", "nope").is_err());
    }
}
