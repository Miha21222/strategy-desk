//! The TradingView sessionid, in the OS credential store (Windows Credential
//! Manager through `keyring`: service `strategy-desk`, key `tv_session`).
//!
//! Only `has_tv_session` ever leaves this module towards the UI. The value
//! goes to exactly two places: the macro env of a chat / scheduled run, and
//! the `session_check` task — both through the environment, never argv, and
//! never a log. Tests use [`MemoryStore`]; nothing here writes a real secret
//! in a test.

use std::collections::HashMap;
use std::fmt;
use std::sync::Mutex;

pub const SERVICE: &str = "strategy-desk";
pub const TV_SESSION: &str = "tv_session";

#[derive(Debug, thiserror::Error)]
#[error("credential store: {0}")]
pub struct SecretError(pub String);

/// A place secrets live. `get` = `Ok(None)` when nothing is stored.
pub trait SecretStore: Send + Sync + fmt::Debug {
    fn get(&self, key: &str) -> Result<Option<String>, SecretError>;
    fn set(&self, key: &str, value: &str) -> Result<(), SecretError>;
    fn delete(&self, key: &str) -> Result<(), SecretError>;
}

/// The OS store. On Windows the Credential Manager; elsewhere keyring's own
/// default (no platform backend is compiled in off Windows).
pub struct KeyringStore {
    service: String,
}

impl KeyringStore {
    pub fn new() -> Self {
        Self {
            service: SERVICE.into(),
        }
    }

    fn entry(&self, key: &str) -> Result<keyring::Entry, SecretError> {
        keyring::Entry::new(&self.service, key).map_err(|e| SecretError(e.to_string()))
    }
}

impl Default for KeyringStore {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Debug for KeyringStore {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("KeyringStore")
            .field("service", &self.service)
            .finish()
    }
}

impl SecretStore for KeyringStore {
    fn get(&self, key: &str) -> Result<Option<String>, SecretError> {
        match self.entry(key)?.get_password() {
            Ok(v) => Ok(Some(v)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(SecretError(e.to_string())),
        }
    }

    fn set(&self, key: &str, value: &str) -> Result<(), SecretError> {
        self.entry(key)?
            .set_password(value)
            .map_err(|e| SecretError(e.to_string()))
    }

    fn delete(&self, key: &str) -> Result<(), SecretError> {
        match self.entry(key)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(SecretError(e.to_string())),
        }
    }
}

/// In-memory store for tests (and a desk without a credential store).
#[derive(Default)]
pub struct MemoryStore {
    map: Mutex<HashMap<String, String>>,
}

impl fmt::Debug for MemoryStore {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("MemoryStore(..)")
    }
}

impl SecretStore for MemoryStore {
    fn get(&self, key: &str) -> Result<Option<String>, SecretError> {
        Ok(self
            .map
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(key)
            .cloned())
    }

    fn set(&self, key: &str, value: &str) -> Result<(), SecretError> {
        self.map
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(key.into(), value.into());
        Ok(())
    }

    fn delete(&self, key: &str) -> Result<(), SecretError> {
        self.map
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(key);
        Ok(())
    }
}

/// The sessionid, trimmed; `None` when absent or unreadable.
pub fn tv_session(store: &dyn SecretStore) -> Option<String> {
    store
        .get(TV_SESSION)
        .ok()
        .flatten()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

pub fn has_tv_session(store: &dyn SecretStore) -> bool {
    tv_session(store).is_some()
}

/// Store a pasted sessionid after the shape check of `macro_env`.
pub fn set_tv_session(store: &dyn SecretStore, value: &str) -> Result<(), SecretError> {
    let v = value.trim();
    if !crate::macro_env::looks_like_session_id(v) {
        return Err(SecretError("not a sessionid".into()));
    }
    store.set(TV_SESSION, v)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_session_round_trip_on_a_memory_store() {
        let s = MemoryStore::default();
        assert!(!has_tv_session(&s));
        assert!(set_tv_session(&s, "short").is_err());
        set_tv_session(&s, "  a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4 ").unwrap();
        assert_eq!(
            tv_session(&s).as_deref(),
            Some("a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4")
        );
        assert!(has_tv_session(&s));
        s.delete(TV_SESSION).unwrap();
        s.delete(TV_SESSION).unwrap();
        assert!(!has_tv_session(&s));
        assert_eq!(
            format!("{s:?}"),
            "MemoryStore(..)",
            "Debug never shows values"
        );
    }
}
