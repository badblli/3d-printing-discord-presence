//! Read Studio's local config only on explicit discovery/save requests.
use crate::config::validate::validate_access_code;
use serde_json::Value;
use std::{fs::File, io::Read, path::Path};

const MAX_FILE: u64 = 4 * 1024 * 1024;

/// `root` is the platform config directory (injectable for tests).
/// Deliberately returns no filesystem/JSON error that could expose local data.
pub fn access_code(root: &Path, serial: &str) -> Option<String> {
    let file = File::open(root.join("BambuStudio/BambuStudio.conf")).ok()?;
    if file.metadata().ok()?.len() > MAX_FILE {
        return None;
    }
    let mut text = String::new();
    file.take(MAX_FILE + 1).read_to_string(&mut text).ok()?;
    if text.len() as u64 > MAX_FILE {
        return None;
    }
    // Studio may append a non-JSON checksum line; consume only the first value.
    let value = serde_json::Deserializer::from_str(&text)
        .into_iter::<Value>()
        .next()?
        .ok()?;
    ["access_code", "user_access_code"]
        .into_iter()
        .find_map(|key| {
            let code = value.get(key)?.get(serial)?.as_str()?;
            validate_access_code(code).ok()?;
            Some(code.to_owned())
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checksum_and_fallback_are_supported_without_exposing_codes() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join("BambuStudio")).unwrap();
        let path = dir.path().join("BambuStudio/BambuStudio.conf");
        for content in [
            "{\"access_code\":{\"00M00A000000000\":\"12345678\"}}\n# MD5 checksum fake",
            "{\"user_access_code\":{\"00M00A000000000\":\"12345678\"}}",
            "{\"access_code\":{},\"user_access_code\":{\"00M00A000000000\":\"12345678\"}}",
        ] {
            std::fs::write(&path, content).unwrap();
            assert!(access_code(dir.path(), "00M00A000000000").as_deref() == Some("12345678"));
            assert!(access_code(dir.path(), "00M00A000000001").is_none());
        }
    }

    #[test]
    fn missing_corrupt_invalid_and_oversized_files_are_absent() {
        let dir = tempfile::tempdir().unwrap();
        assert!(access_code(dir.path(), "00M00A000000000").is_none());
        std::fs::create_dir(dir.path().join("BambuStudio")).unwrap();
        let path = dir.path().join("BambuStudio/BambuStudio.conf");
        for content in [
            "{",
            "null",
            "{\"access_code\":{\"00M00A000000000\":\"bad!\"}}",
        ] {
            std::fs::write(&path, content).unwrap();
            assert!(access_code(dir.path(), "00M00A000000000").is_none());
        }
        File::create(path).unwrap().set_len(MAX_FILE + 1).unwrap();
        assert!(access_code(dir.path(), "00M00A000000000").is_none());
    }
}
