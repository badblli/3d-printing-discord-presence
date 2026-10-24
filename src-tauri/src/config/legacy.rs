//! Explicit, read-only import. Nothing here runs during normal config loading.
use serde::Deserialize;

use super::model::{AppConfig, DiscordConfig, Lang, PrinterConfig, PrinterConnection};
use super::validate::{validate_access_code, validate_config, ValidationError};

#[derive(Debug, thiserror::Error)]
pub enum LegacyError {
    #[error("legacy_read_failed")]
    Read,
    #[error("legacy_format_invalid")]
    Format,
    #[error("legacy_decryption_failed")]
    Decryption,
    #[error(transparent)]
    Validation(#[from] ValidationError),
}

/// The secret is deliberately neither serializable nor included in Debug output.
pub struct LegacyImport {
    pub config: AppConfig,
    pub secret: String,
}

#[derive(Deserialize)]
struct LegacySettings {
    host: String,
    serial: String,
    access_code: String,
    app_id: String,
    #[serde(default = "default_show_name")]
    show_name: bool,
}

fn default_show_name() -> bool {
    true
}

/// Pure format conversion with an injected decoder so tests never need DPAPI or
/// the user's profile. The callback receives the base64-encoded encrypted blob.
pub fn parse_legacy(
    json: &[u8],
    decrypt: impl FnOnce(&str) -> Result<String, LegacyError>,
) -> Result<LegacyImport, LegacyError> {
    let old: LegacySettings = serde_json::from_slice(json).map_err(|_| LegacyError::Format)?;
    let config = AppConfig {
        printers: vec![PrinterConfig {
            id: uuid::Uuid::new_v4(),
            name: "Bambu Lab".into(),
            enabled: true,
            connection: PrinterConnection::Bambu {
                host: old.host,
                serial: old.serial,
                model: None,
            },
        }],
        discord: DiscordConfig {
            show_file_name: old.show_name,
            app_id_override: Some(old.app_id),
            ..DiscordConfig::default()
        },
        lang: Lang::Tr,
        ..AppConfig::default()
    };
    validate_config(&config)?;
    let secret = decrypt(&old.access_code)?;
    validate_access_code(&secret)?;
    Ok(LegacyImport { config, secret })
}

/// Caller stores the returned secret using the imported printer's UUID and saves
/// the config only after keyring success. The old file is never modified.
#[cfg(windows)]
pub fn import_legacy() -> Result<Option<LegacyImport>, LegacyError> {
    let Some(root) = std::env::var_os("LOCALAPPDATA") else {
        return Ok(None);
    };
    let path = std::path::PathBuf::from(root)
        .join("BambuDiscord")
        .join("settings.json");
    import_from_path(&path)
}

#[cfg(windows)]
fn import_from_path(path: &std::path::Path) -> Result<Option<LegacyImport>, LegacyError> {
    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err(LegacyError::Read),
    };
    parse_legacy(&bytes, decrypt_dpapi).map(Some)
}

#[cfg(not(windows))]
pub fn import_legacy() -> Result<Option<LegacyImport>, LegacyError> {
    Ok(None)
}

#[cfg(windows)]
fn decrypt_dpapi(encoded: &str) -> Result<String, LegacyError> {
    use windows::Win32::Foundation::{LocalFree, HLOCAL};
    use windows::Win32::Security::Cryptography::{
        CryptStringToBinaryA, CryptUnprotectData, CRYPTPROTECT_UI_FORBIDDEN, CRYPT_INTEGER_BLOB,
        CRYPT_STRING_BASE64,
    };

    // windows-rs converts the slice length to a DWORD internally.
    if encoded.is_empty() || encoded.len() > u32::MAX as usize {
        return Err(LegacyError::Decryption);
    }
    let mut size = 0;
    // SAFETY: First call queries size; the second receives a buffer of that size.
    unsafe {
        CryptStringToBinaryA(
            encoded.as_bytes(),
            CRYPT_STRING_BASE64,
            None,
            &mut size,
            None,
            None,
        )
        .map_err(|_| LegacyError::Decryption)?;
    }
    if size == 0 {
        return Err(LegacyError::Decryption);
    }
    let mut encrypted = vec![0u8; size as usize];
    unsafe {
        CryptStringToBinaryA(
            encoded.as_bytes(),
            CRYPT_STRING_BASE64,
            Some(encrypted.as_mut_ptr()),
            &mut size,
            None,
            None,
        )
        .map_err(|_| LegacyError::Decryption)?;
    }
    let input = CRYPT_INTEGER_BLOB {
        cbData: size,
        pbData: encrypted.as_mut_ptr(),
    };
    let mut output = CRYPT_INTEGER_BLOB::default();
    // SAFETY: Input remains alive throughout the call. Successful DPAPI output is
    // copied before freeing it with LocalFree, including on UTF-8 decoding errors.
    unsafe {
        CryptUnprotectData(
            &input,
            None,
            None,
            None,
            None,
            CRYPTPROTECT_UI_FORBIDDEN,
            &mut output,
        )
        .map_err(|_| LegacyError::Decryption)?;
        if output.pbData.is_null() {
            return Err(LegacyError::Decryption);
        }
        let secret = std::slice::from_raw_parts(output.pbData, output.cbData as usize).to_vec();
        let _ = LocalFree(Some(HLOCAL(output.pbData.cast())));
        String::from_utf8(secret).map_err(|_| LegacyError::Decryption)
    }
}

#[cfg(all(test, windows))]
mod windows_tests {
    #[test]
    fn absent_legacy_file_returns_none() {
        let dir = tempfile::tempdir().unwrap();
        assert!(super::import_from_path(&dir.path().join("missing.json"))
            .unwrap()
            .is_none());
    }

    #[test]
    fn invalid_dpapi_input_is_redacted() {
        let error = super::decrypt_dpapi("not a valid encrypted blob!")
            .err()
            .unwrap();
        assert_eq!(error.to_string(), "legacy_decryption_failed");
    }
}
