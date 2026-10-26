use std::{collections::HashSet, path::Path, sync::Arc};

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::config::{
    validate::{validate_access_code, validate_config},
    AppConfig, ConfigStore, DiscordConfig, Lang, PrinterConfig, PrinterConnection, SecretStore,
};

use super::RuntimeConfig;

/// Input only: deliberately no Serialize or Debug implementation.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SecretUpdate {
    pub id: Uuid,
    pub secret: String,
}

#[derive(Serialize)]
pub struct PrinterConfigView {
    #[serde(flatten)]
    pub printer: PrinterConfig,
    pub has_secret: bool,
}

#[derive(Serialize)]
pub struct ConfigView {
    pub version: u32,
    pub printers: Vec<PrinterConfigView>,
    pub discord: DiscordConfig,
    pub autostart: bool,
    pub lang: Lang,
}

pub struct Settings {
    store: ConfigStore,
    secrets: Arc<dyn SecretStore>,
    pub config: AppConfig,
    legacy_available: bool,
    recovered_corrupt: bool,
    revision: u64,
    // Retain failed deletions so a later successful save can retry cleanup.
    pending_deletions: HashSet<Uuid>,
}

impl Settings {
    pub fn load(dir: &Path, secrets: Arc<dyn SecretStore>) -> Result<Self, String> {
        Self::load_for_environment(dir, secrets, &os_locale(), legacy_exists())
    }

    fn load_for_environment(
        dir: &Path,
        secrets: Arc<dyn SecretStore>,
        locale: &str,
        legacy_exists: bool,
    ) -> Result<Self, String> {
        let first_run = !dir
            .join("config.json")
            .try_exists()
            .map_err(|_| "config_io_failed")?;
        let store = ConfigStore::new(dir);
        let loaded = store.load().map_err(|error| error.to_string())?;
        let mut config = loaded.config;
        if first_run {
            config.lang = language_for_locale(locale);
        }
        Ok(Self {
            store,
            secrets,
            config,
            legacy_available: first_run && legacy_exists,
            recovered_corrupt: loaded.recovered_corrupt,
            revision: 0,
            pending_deletions: HashSet::new(),
        })
    }

    pub fn revision(&self) -> u64 {
        self.revision
    }

    pub fn runtime_config(&self) -> RuntimeConfig {
        RuntimeConfig {
            config: self.config.clone(),
            legacy_available: self.legacy_available,
            recovered_corrupt: self.recovered_corrupt,
        }
    }

    pub fn view(&self) -> Result<ConfigView, String> {
        Ok(ConfigView {
            version: self.config.version,
            printers: self
                .config
                .printers
                .iter()
                .map(|printer| {
                    Ok(PrinterConfigView {
                        printer: printer.clone(),
                        has_secret: self
                            .secrets
                            .get(printer.id)
                            .map_err(|error| error.to_string())?
                            .is_some(),
                    })
                })
                .collect::<Result<_, String>>()?,
            discord: self.config.discord.clone(),
            autostart: self.config.autostart,
            lang: self.config.lang,
        })
    }

    pub fn save(&mut self, config: AppConfig, updates: Vec<SecretUpdate>) -> Result<(), String> {
        self.save_with_studio(config, updates, Vec::new())
    }

    pub fn save_with_studio(
        &mut self,
        config: AppConfig,
        updates: Vec<SecretUpdate>,
        studio_secrets: Vec<Uuid>,
    ) -> Result<(), String> {
        self.save_with_studio_at(
            config,
            updates,
            studio_secrets,
            dirs::config_dir().as_deref(),
        )
    }

    fn save_with_studio_at(
        &mut self,
        config: AppConfig,
        mut updates: Vec<SecretUpdate>,
        studio_secrets: Vec<Uuid>,
        studio_root: Option<&Path>,
    ) -> Result<(), String> {
        validate_config(&config).map_err(|error| error.to_string())?;
        let mut requested: HashSet<_> = updates.iter().map(|update| update.id).collect();
        // Detect collisions before reading Studio or writing any credentials.
        for id in &studio_secrets {
            if !requested.insert(*id) {
                return Err("duplicate_secret_printer".into());
            }
        }
        for id in studio_secrets {
            let printer = config
                .printers
                .iter()
                .find(|printer| printer.id == id)
                .ok_or("unknown_secret_printer")?;
            let PrinterConnection::Bambu { serial, .. } = &printer.connection else {
                return Err("studio_code_missing".into());
            };
            let secret = studio_root
                .and_then(|root| crate::discovery::studio::access_code(root, serial))
                .ok_or("studio_code_missing")?;
            updates.push(SecretUpdate { id, secret });
        }
        let mut ids = HashSet::new();
        // Validate the entire request before any keychain or disk writes.
        for update in &updates {
            let printer = config
                .printers
                .iter()
                .find(|printer| printer.id == update.id)
                .ok_or("unknown_secret_printer")?;
            if !ids.insert(update.id) {
                return Err("duplicate_secret_printer".into());
            }
            if matches!(printer.connection, PrinterConnection::Bambu { .. }) {
                validate_access_code(&update.secret).map_err(|error| error.to_string())?;
            } else if update.secret.is_empty() {
                return Err("invalid_secret".into());
            }
        }
        let originals = updates
            .iter()
            .map(|update| {
                self.secrets
                    .get(update.id)
                    .map(|secret| (update.id, secret))
                    .map_err(|error| error.to_string())
            })
            .collect::<Result<Vec<_>, _>>()?;
        for (index, update) in updates.iter().enumerate() {
            if let Err(error) = self.secrets.set(update.id, &update.secret) {
                self.rollback(&originals[..=index])?;
                return Err(error.to_string());
            }
        }
        if let Err(error) = self.store.save(&config) {
            self.rollback(&originals)?;
            return Err(error.to_string());
        }
        for printer in &self.config.printers {
            if !config
                .printers
                .iter()
                .any(|current| current.id == printer.id)
            {
                self.pending_deletions.insert(printer.id);
            }
        }
        self.config = config;
        self.revision += 1;
        self.legacy_available = false;
        self.recovered_corrupt = false;
        self.pending_deletions
            .retain(|id| !self.config.printers.iter().any(|printer| printer.id == *id));
        for id in self.pending_deletions.clone() {
            self.secrets.delete(id).map_err(|error| error.to_string())?;
            self.pending_deletions.remove(&id);
        }
        Ok(())
    }

    fn rollback(&mut self, originals: &[(Uuid, Option<String>)]) -> Result<(), String> {
        let mut failed = false;
        for (id, value) in originals {
            let result = match value {
                Some(value) => self.secrets.set(*id, value),
                None => self.secrets.delete(*id),
            };
            failed |= result.is_err();
        }
        if failed {
            self.revision += 1;
            Err("secret_rollback_failed".into())
        } else {
            Ok(())
        }
    }

    pub fn delete_printer(&mut self, id: Uuid) -> Result<(), String> {
        let previous = self.config.clone();
        let result = self
            .store
            .remove_printer(&mut self.config, id, self.secrets.as_ref())
            .map_err(|error| error.to_string());
        if previous != self.config || result.is_ok() {
            self.revision += 1;
        }
        if result.is_err() && !self.config.printers.iter().any(|printer| printer.id == id) {
            self.pending_deletions.insert(id);
        } else if result.is_ok() {
            self.pending_deletions.remove(&id);
            self.legacy_available = false;
            self.recovered_corrupt = false;
        }
        result
    }

    pub fn import_legacy(&mut self) -> Result<(), String> {
        // Import is a first-run action. Never silently replace an existing setup.
        if !self.legacy_available || !self.config.printers.is_empty() {
            return Err("legacy_unavailable".into());
        }
        let imported = crate::config::legacy::import_legacy()
            .map_err(|error| error.to_string())?
            .ok_or("legacy_unavailable")?;
        let id = imported
            .config
            .printers
            .first()
            .ok_or("legacy_format_invalid")?
            .id;
        self.save(
            imported.config,
            vec![SecretUpdate {
                id,
                secret: imported.secret,
            }],
        )
    }
}

fn language_for_locale(locale: &str) -> Lang {
    if locale.to_ascii_lowercase().starts_with("tr") {
        Lang::Tr
    } else {
        Lang::En
    }
}

fn legacy_exists() -> bool {
    #[cfg(windows)]
    {
        std::env::var_os("LOCALAPPDATA").is_some_and(|root| {
            Path::new(&root)
                .join("BambuDiscord/settings.json")
                .is_file()
        })
    }
    #[cfg(not(windows))]
    {
        false
    }
}

#[cfg(windows)]
fn os_locale() -> String {
    // Available since Vista; no profile data or subprocess is needed. The existing
    // windows crate does not enable Win32_Globalization, so use this narrow ABI.
    #[link(name = "kernel32")]
    extern "system" {
        fn GetUserDefaultUILanguage() -> u16;
        fn LCIDToLocaleName(locale: u32, name: *mut u16, count: i32, flags: u32) -> i32;
    }
    let mut buffer = [0u16; 85];
    // SAFETY: The first call takes no pointers. With the default sort, a LANGID
    // is also an LCID. Windows receives the buffer's exact writable capacity.
    let count = unsafe {
        LCIDToLocaleName(
            u32::from(GetUserDefaultUILanguage()),
            buffer.as_mut_ptr(),
            buffer.len() as i32,
            0,
        )
    };
    if count > 1 {
        String::from_utf16_lossy(&buffer[..count as usize - 1])
    } else {
        String::new()
    }
}

#[cfg(not(windows))]
fn os_locale() -> String {
    #[cfg(target_os = "macos")]
    if let Ok(output) = std::process::Command::new("/usr/bin/defaults")
        .args(["read", "-g", "AppleLanguages"])
        .output()
    {
        if output.status.success() {
            if let Some(language) =
                String::from_utf8_lossy(&output.stdout)
                    .lines()
                    .find_map(|line| {
                        let language = line.trim().trim_end_matches(',').trim_matches('"');
                        (!language.is_empty() && language != "(" && language != ")")
                            .then_some(language.to_owned())
                    })
            {
                return language;
            }
        }
    }
    for key in ["LC_ALL", "LC_MESSAGES", "LANG"] {
        if let Ok(locale) = std::env::var(key) {
            if !locale.is_empty() {
                return locale;
            }
        }
    }
    String::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn studio_fixture(root: &Path) -> AppConfig {
        std::fs::create_dir(root.join("BambuStudio")).unwrap();
        std::fs::write(
            root.join("BambuStudio/BambuStudio.conf"),
            r#"{"access_code":{"00M00A000000000":"12345678"}}"#,
        )
        .unwrap();
        AppConfig {
            printers: vec![PrinterConfig {
                id: Uuid::from_u128(1),
                name: "Fake printer".into(),
                enabled: true,
                connection: PrinterConnection::Bambu {
                    host: "192.168.1.50".into(),
                    serial: "00M00A000000000".into(),
                    model: None,
                },
            }],
            ..AppConfig::default()
        }
    }

    #[test]
    fn studio_save_keeps_code_in_keyring_only() {
        let dir = tempfile::tempdir().unwrap();
        let studio = tempfile::tempdir().unwrap();
        let config = studio_fixture(studio.path());
        let id = config.printers[0].id;
        let secrets = Arc::new(crate::config::MemoryStore::default());
        let mut settings =
            Settings::load_for_environment(dir.path(), secrets.clone(), "en-US", false).unwrap();
        settings
            .save_with_studio_at(config, vec![], vec![id], Some(studio.path()))
            .unwrap();
        assert!(secrets.get(id).unwrap().as_deref() == Some("12345678"));
        assert!(settings.view().unwrap().printers[0].has_secret);
        for serialized in [
            serde_json::to_string(&settings.view().unwrap()).unwrap(),
            std::fs::read_to_string(dir.path().join("config.json")).unwrap(),
        ] {
            assert!(!serialized.contains("12345678"));
            assert!(!serialized.contains("access_code"));
        }
    }

    #[test]
    fn studio_missing_and_duplicate_requests_do_not_write() {
        let dir = tempfile::tempdir().unwrap();
        let studio = tempfile::tempdir().unwrap();
        let config = studio_fixture(studio.path());
        let id = config.printers[0].id;
        let secrets = Arc::new(crate::config::MemoryStore::default());
        let mut settings =
            Settings::load_for_environment(dir.path(), secrets.clone(), "en-US", false).unwrap();
        assert_eq!(
            settings
                .save_with_studio_at(config.clone(), vec![], vec![id], None)
                .unwrap_err(),
            "studio_code_missing"
        );
        assert_eq!(
            settings
                .save_with_studio_at(
                    config.clone(),
                    vec![SecretUpdate {
                        id,
                        secret: "87654321".into()
                    }],
                    vec![id],
                    Some(studio.path())
                )
                .unwrap_err(),
            "duplicate_secret_printer"
        );
        assert_eq!(
            settings
                .save_with_studio_at(config, vec![], vec![id, id], Some(studio.path()))
                .unwrap_err(),
            "duplicate_secret_printer"
        );
        assert!(secrets.get(id).unwrap().is_none());
        assert_eq!(settings.revision(), 0);
        assert!(!dir.path().join("config.json").exists());
    }

    #[test]
    fn studio_save_rolls_back_when_config_write_fails() {
        let dir = tempfile::tempdir().unwrap();
        let studio = tempfile::tempdir().unwrap();
        let config = studio_fixture(studio.path());
        let id = config.printers[0].id;
        let secrets = Arc::new(crate::config::MemoryStore::default());
        let mut settings =
            Settings::load_for_environment(dir.path(), secrets.clone(), "en-US", false).unwrap();
        secrets.set(id, "87654321").unwrap();
        std::fs::create_dir(dir.path().join("config.json")).unwrap();
        assert_eq!(
            settings
                .save_with_studio_at(config, vec![], vec![id], Some(studio.path()))
                .unwrap_err(),
            "config_io_failed"
        );
        assert!(secrets.get(id).unwrap().as_deref() == Some("87654321"));
        assert!(settings.config.printers.is_empty());
        assert_eq!(settings.revision(), 0);
    }

    #[test]
    fn locale_selects_turkish_only_for_tr_prefix() {
        for locale in ["tr", "tr-TR", "TR_tr.UTF-8"] {
            assert_eq!(language_for_locale(locale), Lang::Tr);
        }
        for locale in ["", "en-US", "de_DE", "C.UTF-8"] {
            assert_eq!(language_for_locale(locale), Lang::En);
        }
    }

    #[test]
    fn first_run_detects_legacy_without_import_and_keeps_saved_language() {
        let dir = tempfile::tempdir().unwrap();
        let secrets = Arc::new(crate::config::MemoryStore::default());
        let mut settings =
            Settings::load_for_environment(dir.path(), secrets.clone(), "tr-TR", true).unwrap();
        assert_eq!(settings.config.lang, Lang::Tr);
        assert!(settings.runtime_config().legacy_available);
        assert!(settings.config.printers.is_empty());
        assert!(!dir.path().join("config.json").exists());
        settings.save(settings.config.clone(), Vec::new()).unwrap();
        assert!(!settings.runtime_config().legacy_available);
        let loaded = Settings::load_for_environment(dir.path(), secrets, "en-US", true).unwrap();
        assert_eq!(loaded.config.lang, Lang::Tr);
        assert!(!loaded.runtime_config().legacy_available);
    }

    #[test]
    fn rejected_secret_request_does_not_change_revision_or_credentials() {
        let dir = tempfile::tempdir().unwrap();
        let secrets = Arc::new(crate::config::MemoryStore::default());
        let mut settings =
            Settings::load_for_environment(dir.path(), secrets, "en-US", false).unwrap();
        let result = settings.save(
            settings.config.clone(),
            vec![SecretUpdate {
                id: Uuid::from_u128(1),
                secret: "fake-private-api-key".into(),
            }],
        );
        assert_eq!(result.unwrap_err(), "unknown_secret_printer");
        assert_eq!(settings.revision(), 0);
        assert!(!dir.path().join("config.json").exists());
    }
}
