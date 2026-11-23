use central_llm_core::{require_hosted, CoreError, SecretStore};

#[cfg_attr(not(windows), allow(dead_code))]
const SERVICE: &str = "Central LLM Interface App";

pub struct KeychainStore;

impl SecretStore for KeychainStore {
    fn set_key(&self, provider: &str, secret: &str) -> Result<(), CoreError> {
        let provider = require_hosted(provider)?;
        if secret.trim().is_empty() {
            return Err(CoreError::Message("Enter a key before saving.".into()));
        }
        write_secret(provider.slug(), secret)
    }

    fn get_key(&self, provider: &str) -> Result<Option<String>, CoreError> {
        let provider = require_hosted(provider)?;
        read_secret(provider.slug())
    }

    fn delete_key(&self, provider: &str) -> Result<(), CoreError> {
        let provider = require_hosted(provider)?;
        delete_secret(provider.slug())
    }
}

#[cfg(windows)]
fn write_secret(account: &str, secret: &str) -> Result<(), CoreError> {
    let entry = keyring::Entry::new(SERVICE, account)
        .map_err(|error| CoreError::Message(error.to_string()))?;
    entry.set_password(secret).map_err(|error| {
        CoreError::Message(format!(
            "Could not save the key in Windows Credential Manager. {error}"
        ))
    })
}

#[cfg(windows)]
fn read_secret(account: &str) -> Result<Option<String>, CoreError> {
    let entry = keyring::Entry::new(SERVICE, account)
        .map_err(|error| CoreError::Message(error.to_string()))?;
    match entry.get_password() {
        Ok(value) => Ok(Some(value)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(error) => Err(CoreError::Message(format!(
            "Could not read the key from Windows Credential Manager. {error}"
        ))),
    }
}

#[cfg(windows)]
fn delete_secret(account: &str) -> Result<(), CoreError> {
    let entry = keyring::Entry::new(SERVICE, account)
        .map_err(|error| CoreError::Message(error.to_string()))?;
    match entry.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(error) => Err(CoreError::Message(format!(
            "Could not remove the key from Windows Credential Manager. {error}"
        ))),
    }
}

#[cfg(not(windows))]
fn write_secret(_account: &str, _secret: &str) -> Result<(), CoreError> {
    Err(windows_only())
}

#[cfg(not(windows))]
fn read_secret(_account: &str) -> Result<Option<String>, CoreError> {
    Err(windows_only())
}

#[cfg(not(windows))]
fn delete_secret(_account: &str) -> Result<(), CoreError> {
    Err(windows_only())
}

#[cfg(not(windows))]
fn windows_only() -> CoreError {
    CoreError::Message(
        "API keys are stored in Windows Credential Manager. Save them on Windows.".into(),
    )
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    use central_llm_core::{AppSettings, ProviderId, Store};
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn keychain_roundtrip_does_not_land_in_the_database() {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let secret = format!("sk-test-not-a-path-{nonce}");
        let keys = KeychainStore;
        keys.set_key("openai", &secret).unwrap();
        assert_eq!(
            keys.get_key("openai").unwrap().as_deref(),
            Some(secret.as_str())
        );

        let dir = std::env::temp_dir().join(format!("cllm-keychain-{nonce}"));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("studio.db");
        let defaults = AppSettings::fresh(
            dir.join("models").display().to_string(),
            dir.join("outputs").display().to_string(),
        );
        let store = Store::open(&path, &defaults).unwrap();
        store
            .create_chat(ProviderId::Openai, "gpt-4.1-mini", 128_000)
            .unwrap();
        let bytes = std::fs::read(&path).unwrap();
        assert!(!String::from_utf8_lossy(&bytes).contains(&secret));

        keys.delete_key("openai").unwrap();
        assert_eq!(keys.get_key("openai").unwrap(), None);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
