use std::collections::HashMap;
use std::sync::Mutex;

use uuid::Uuid;

pub const SERVICE_NAME: &str = "3d-printing-presence";

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("secret_store_unavailable")]
pub struct SecretError;

/// Missing entries are None; deletion is idempotent. Backend errors are redacted.
pub trait SecretStore: Send + Sync {
    fn get(&self, printer: Uuid) -> Result<Option<String>, SecretError>;
    fn set(&self, printer: Uuid, secret: &str) -> Result<(), SecretError>;
    fn delete(&self, printer: Uuid) -> Result<(), SecretError>;
}

#[derive(Default)]
pub struct KeyringStore;

impl KeyringStore {
    fn entry(printer: Uuid) -> Result<keyring::Entry, SecretError> {
        keyring::Entry::new(SERVICE_NAME, &format!("printer:{printer}")).map_err(|_| SecretError)
    }
}

impl SecretStore for KeyringStore {
    fn get(&self, printer: Uuid) -> Result<Option<String>, SecretError> {
        match Self::entry(printer)?.get_password() {
            Ok(value) => Ok(Some(value)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(_) => Err(SecretError),
        }
    }

    fn set(&self, printer: Uuid, secret: &str) -> Result<(), SecretError> {
        Self::entry(printer)?
            .set_password(secret)
            .map_err(|_| SecretError)
    }

    fn delete(&self, printer: Uuid) -> Result<(), SecretError> {
        match Self::entry(printer)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(_) => Err(SecretError),
        }
    }
}

/// Isolated, nonpersistent backend for tests. Intentionally does not implement Debug.
#[derive(Default)]
pub struct MemoryStore {
    values: Mutex<HashMap<Uuid, String>>,
}

impl SecretStore for MemoryStore {
    fn get(&self, printer: Uuid) -> Result<Option<String>, SecretError> {
        Ok(self
            .values
            .lock()
            .map_err(|_| SecretError)?
            .get(&printer)
            .cloned())
    }

    fn set(&self, printer: Uuid, secret: &str) -> Result<(), SecretError> {
        self.values
            .lock()
            .map_err(|_| SecretError)?
            .insert(printer, secret.to_owned());
        Ok(())
    }

    fn delete(&self, printer: Uuid) -> Result<(), SecretError> {
        self.values
            .lock()
            .map_err(|_| SecretError)?
            .remove(&printer);
        Ok(())
    }
}
