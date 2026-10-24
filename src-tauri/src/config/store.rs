use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use uuid::Uuid;

use super::model::AppConfig;
use super::secrets::{SecretError, SecretStore};
use super::validate::{validate_config, ValidationError};

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("config_io_failed")]
    Io,
    #[error("config_serialization_failed")]
    Serialization,
    #[error(transparent)]
    Validation(#[from] ValidationError),
    #[error(transparent)]
    Secret(#[from] SecretError),
}

impl From<io::Error> for StoreError {
    fn from(_: io::Error) -> Self {
        Self::Io
    }
}

#[derive(Debug)]
pub struct LoadResult {
    pub config: AppConfig,
    /// The invalid original was moved to config.json.bak; notify the user.
    pub recovered_corrupt: bool,
}

pub struct ConfigStore {
    dir: PathBuf,
}

impl ConfigStore {
    pub fn new(dir: impl AsRef<Path>) -> Self {
        Self {
            dir: dir.as_ref().to_owned(),
        }
    }

    pub fn load(&self) -> Result<LoadResult, StoreError> {
        let path = self.dir.join("config.json");
        let bytes = match fs::read(&path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                return Ok(LoadResult {
                    config: AppConfig::default(),
                    recovered_corrupt: false,
                });
            }
            Err(error) => return Err(error.into()),
        };
        // Inspect the version before the schema: future versions may add fields
        // that this version deliberately rejects during normal deserialization.
        if let Ok(value) = serde_json::from_slice::<serde_json::Value>(&bytes) {
            if let Some(version) = value.get("version").and_then(|v| v.as_u64()) {
                if version != u64::from(super::model::CONFIG_VERSION) {
                    return Err(ValidationError::UnsupportedVersion.into());
                }
            }
        }
        if let Ok(config) = serde_json::from_slice::<AppConfig>(&bytes) {
            match validate_config(&config) {
                Ok(()) => {
                    return Ok(LoadResult {
                        config,
                        recovered_corrupt: false,
                    })
                }
                // Never replace a future version with defaults on an app downgrade.
                Err(ValidationError::UnsupportedVersion) => {
                    return Err(ValidationError::UnsupportedVersion.into());
                }
                Err(_) => {}
            }
        }
        fs::rename(path, self.dir.join("config.json.bak"))?;
        Ok(LoadResult {
            config: AppConfig::default(),
            recovered_corrupt: true,
        })
    }

    pub fn save(&self, config: &AppConfig) -> Result<(), StoreError> {
        validate_config(config)?;
        let bytes = serde_json::to_vec_pretty(config).map_err(|_| StoreError::Serialization)?;
        fs::create_dir_all(&self.dir)?;
        // A unique sibling avoids collisions and ensures rename stays on the same volume.
        let temp = TemporaryFile(self.dir.join(format!(".config-{}.tmp", Uuid::new_v4())));
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp.0)?;
        file.write_all(&bytes)?;
        file.sync_all()?;
        drop(file);
        fs::rename(&temp.0, self.dir.join("config.json"))?;
        Ok(())
    }

    /// Persist removal before deleting the credential. On a keyring failure, retry with
    /// the same id (even if already absent). Disk and caller state remain consistent.
    pub fn remove_printer(
        &self,
        config: &mut AppConfig,
        id: Uuid,
        secrets: &dyn SecretStore,
    ) -> Result<(), StoreError> {
        let mut updated = config.clone();
        updated.printers.retain(|printer| printer.id != id);
        self.save(&updated)?;
        *config = updated;
        secrets.delete(id)?;
        Ok(())
    }
}

struct TemporaryFile(PathBuf);

impl Drop for TemporaryFile {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}
