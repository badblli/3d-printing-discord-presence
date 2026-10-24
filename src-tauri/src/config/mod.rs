//! Settings on disk and secrets in the OS keychain. No printer I/O is performed here.
pub mod legacy;
pub mod model;
pub mod secrets;
pub mod store;
pub mod validate;

pub use model::{AppConfig, DiscordConfig, Lang, PrinterConfig, PrinterConnection};
pub use secrets::{KeyringStore, MemoryStore, SecretError, SecretStore};
pub use store::{ConfigStore, LoadResult, StoreError};

#[cfg(test)]
mod tests;
