//! Secrets live in the OS credential store (Windows Credential Manager),
//! behind a trait so tests never touch the real one.

use std::collections::HashMap;
use std::sync::Mutex;

use crate::{ConfigError, Result};

pub const SERVICE: &str = "magpie";
pub const API_TOKEN: &str = "api-token";

pub trait SecretStore {
    /// `Ok(None)` when the secret does not exist.
    fn get(&self, key: &str) -> Result<Option<String>>;
    fn set(&self, key: &str, value: &str) -> Result<()>;
}

/// The real store: one credential per key under service `magpie`.
#[derive(Debug, Default, Clone, Copy)]
pub struct OsKeyring;

impl SecretStore for OsKeyring {
    fn get(&self, key: &str) -> Result<Option<String>> {
        match entry(key)?.get_password() {
            Ok(v) => Ok(Some(v)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(ConfigError::Secret(e.to_string())),
        }
    }

    fn set(&self, key: &str, value: &str) -> Result<()> {
        entry(key)?
            .set_password(value)
            .map_err(|e| ConfigError::Secret(e.to_string()))
    }
}

fn entry(key: &str) -> Result<keyring::Entry> {
    keyring::Entry::new(SERVICE, key).map_err(|e| ConfigError::Secret(e.to_string()))
}

/// In-memory store for tests.
#[derive(Debug, Default)]
pub struct MemorySecrets(Mutex<HashMap<String, String>>);

impl SecretStore for MemorySecrets {
    fn get(&self, key: &str) -> Result<Option<String>> {
        Ok(self.0.lock().expect("not poisoned").get(key).cloned())
    }

    fn set(&self, key: &str, value: &str) -> Result<()> {
        self.0
            .lock()
            .expect("not poisoned")
            .insert(key.to_owned(), value.to_owned());
        Ok(())
    }
}

/// For the app: returns the API token, generating one on first run.
pub fn api_token_or_create(store: &dyn SecretStore) -> Result<String> {
    if let Some(t) = store.get(API_TOKEN)?.filter(|t| !t.is_empty()) {
        return Ok(t);
    }
    let token = generate_token();
    store.set(API_TOKEN, &token)?;
    Ok(token)
}

/// 64 hex chars from two v4 UUIDs: 244 random bits from the OS RNG.
fn generate_token() -> String {
    format!(
        "{}{}",
        uuid::Uuid::new_v4().simple(),
        uuid::Uuid::new_v4().simple()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_created_once_then_reused() {
        let store = MemorySecrets::default();
        let first = api_token_or_create(&store).unwrap();
        assert_eq!(first.len(), 64);
        assert!(first.chars().all(|c| c.is_ascii_hexdigit()));
        assert_eq!(api_token_or_create(&store).unwrap(), first);
    }

    /// Touches the real OS credential store; run with `cargo test -- --ignored`.
    #[test]
    #[ignore = "writes to the real credential store"]
    fn os_keyring_roundtrip() {
        let key = format!("selftest-{}", uuid::Uuid::new_v4());
        assert_eq!(OsKeyring.get(&key).unwrap(), None);
        OsKeyring.set(&key, "v1").unwrap();
        OsKeyring.set(&key, "v2").unwrap();
        assert_eq!(OsKeyring.get(&key).unwrap().as_deref(), Some("v2"));
        entry(&key).unwrap().delete_credential().unwrap();
        assert_eq!(OsKeyring.get(&key).unwrap(), None);
    }

    #[test]
    fn empty_stored_token_is_replaced() {
        let store = MemorySecrets::default();
        store.set(API_TOKEN, "").unwrap();
        assert_eq!(api_token_or_create(&store).unwrap().len(), 64);
    }
}
