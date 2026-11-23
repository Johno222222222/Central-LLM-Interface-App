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

/// Windows Credential Manager, persisted for this user on this PC.
/// `CRED_PERSIST_LOCAL_MACHINE` is used on purpose. The keyring crate writes
/// `CRED_PERSIST_ENTERPRISE`, and on a machine that is not domain-joined
/// (including GitHub's Windows runners) that write can succeed while the
/// following read comes back empty.
#[cfg(windows)]
fn write_secret(account: &str, secret: &str) -> Result<(), CoreError> {
    use windows_sys::Win32::Security::Credentials::{
        CredWriteW, CREDENTIALW, CRED_PERSIST_LOCAL_MACHINE, CRED_TYPE_GENERIC,
    };

    let target = wide(&target_name(account));
    let user = wide(account);
    let mut blob = utf16_le_bytes(secret);
    let credential = CREDENTIALW {
        Flags: 0,
        Type: CRED_TYPE_GENERIC,
        TargetName: target.as_ptr() as *mut u16,
        Comment: std::ptr::null_mut(),
        LastWritten: zero_filetime(),
        CredentialBlobSize: blob.len() as u32,
        CredentialBlob: blob.as_mut_ptr(),
        Persist: CRED_PERSIST_LOCAL_MACHINE,
        AttributeCount: 0,
        Attributes: std::ptr::null_mut(),
        TargetAlias: std::ptr::null_mut(),
        UserName: user.as_ptr() as *mut u16,
    };
    let ok = unsafe { CredWriteW(&credential, 0) };
    blob.fill(0);
    if ok == 0 {
        return Err(credential_error("save"));
    }
    Ok(())
}

#[cfg(windows)]
fn read_secret(account: &str) -> Result<Option<String>, CoreError> {
    use windows_sys::Win32::Foundation::ERROR_NOT_FOUND;
    use windows_sys::Win32::Security::Credentials::{CredFree, CredReadW, CRED_TYPE_GENERIC};

    let target = wide(&target_name(account));
    let mut credential = std::ptr::null_mut();
    let ok = unsafe { CredReadW(target.as_ptr(), CRED_TYPE_GENERIC, 0, &mut credential) };
    if ok == 0 {
        let code = last_error();
        if code == ERROR_NOT_FOUND {
            return Ok(None);
        }
        return Err(CoreError::Message(format!(
            "Could not read the key from Windows Credential Manager (error {code})."
        )));
    }
    let result = unsafe { decode_blob(&*credential) };
    unsafe { CredFree(credential as *const std::ffi::c_void) };
    result.map(Some)
}

#[cfg(windows)]
fn delete_secret(account: &str) -> Result<(), CoreError> {
    use windows_sys::Win32::Foundation::ERROR_NOT_FOUND;
    use windows_sys::Win32::Security::Credentials::{CredDeleteW, CRED_TYPE_GENERIC};

    let target = wide(&target_name(account));
    let ok = unsafe { CredDeleteW(target.as_ptr(), CRED_TYPE_GENERIC, 0) };
    if ok == 0 {
        let code = last_error();
        if code == ERROR_NOT_FOUND {
            return Ok(());
        }
        return Err(CoreError::Message(format!(
            "Could not remove the key from Windows Credential Manager (error {code})."
        )));
    }
    Ok(())
}

#[cfg(windows)]
fn target_name(account: &str) -> String {
    format!("{account}.{SERVICE}")
}

#[cfg(windows)]
fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}

#[cfg(windows)]
fn utf16_le_bytes(value: &str) -> Vec<u8> {
    value
        .encode_utf16()
        .flat_map(|unit| unit.to_le_bytes())
        .collect()
}

#[cfg(windows)]
fn zero_filetime() -> windows_sys::Win32::Foundation::FILETIME {
    windows_sys::Win32::Foundation::FILETIME {
        dwLowDateTime: 0,
        dwHighDateTime: 0,
    }
}

#[cfg(windows)]
fn last_error() -> u32 {
    unsafe { windows_sys::Win32::Foundation::GetLastError() }
}

#[cfg(windows)]
fn credential_error(action: &str) -> CoreError {
    CoreError::Message(format!(
        "Could not {action} the key in Windows Credential Manager (error {}).",
        last_error()
    ))
}

#[cfg(windows)]
unsafe fn decode_blob(
    credential: &windows_sys::Win32::Security::Credentials::CREDENTIALW,
) -> Result<String, CoreError> {
    let size = credential.CredentialBlobSize as usize;
    if size == 0 || credential.CredentialBlob.is_null() {
        return Ok(String::new());
    }
    if size % 2 != 0 {
        return Err(CoreError::Message(
            "The saved key in Windows Credential Manager was not valid text.".into(),
        ));
    }
    let bytes = std::slice::from_raw_parts(credential.CredentialBlob, size);
    let units = bytes
        .chunks_exact(2)
        .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
        .collect::<Vec<_>>();
    String::from_utf16(&units).map_err(|_| {
        CoreError::Message("The saved key in Windows Credential Manager was not valid text.".into())
    })
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
