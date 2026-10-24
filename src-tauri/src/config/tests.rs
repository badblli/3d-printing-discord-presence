use super::legacy::{parse_legacy, LegacyError};
use super::validate::*;
use super::*;
use std::fs;
use uuid::Uuid;

fn bambu() -> PrinterConnection {
    PrinterConnection::Bambu {
        host: "192.168.1.50".into(),
        serial: "00M00A000000000".into(),
        model: None,
    }
}

fn config() -> AppConfig {
    AppConfig {
        printers: vec![PrinterConfig {
            id: Uuid::from_u128(1),
            name: "Test printer".into(),
            enabled: true,
            connection: bambu(),
        }],
        ..AppConfig::default()
    }
}

#[test]
fn defaults_are_valid_and_have_no_printers() {
    let value = AppConfig::default();
    assert!(value.printers.is_empty());
    assert!(value.discord.enabled);
    assert!(value.discord.app_id_override.is_none());
    assert!(!value.autostart);
    assert_eq!(validate_config(&value), Ok(()));
}

#[test]
fn round_trip_all_connection_kinds() {
    let dir = tempfile::tempdir().unwrap();
    let store = ConfigStore::new(dir.path().join("nested"));
    let mut value = config();
    for connection in [
        PrinterConnection::Moonraker {
            url: "http://printer.local:7125".into(),
        },
        PrinterConnection::OctoPrint {
            url: "https://192.168.1.50/octoprint".into(),
        },
        PrinterConnection::PrusaLink {
            url: "http://prusa.local".into(),
        },
    ] {
        value.printers.push(PrinterConfig {
            id: Uuid::new_v4(),
            name: "Test printer".into(),
            enabled: false,
            connection,
        });
    }
    store.save(&value).unwrap();
    let loaded = store.load().unwrap();
    assert_eq!(loaded.config, value);
    assert!(!loaded.recovered_corrupt);
    let json: serde_json::Value =
        serde_json::from_slice(&fs::read(dir.path().join("nested/config.json")).unwrap()).unwrap();
    assert_eq!(json["printers"][0]["connection"]["kind"], "bambu");
    assert_eq!(json["printers"][2]["connection"]["kind"], "octo_print");
}

#[test]
fn saved_json_never_contains_access_codes_or_api_keys() {
    let dir = tempfile::tempdir().unwrap();
    let store = ConfigStore::new(dir.path());
    let secrets = MemoryStore::default();
    let mut value = config();
    secrets.set(value.printers[0].id, "12345678").unwrap();
    let api_id = Uuid::from_u128(2);
    value.printers.push(PrinterConfig {
        id: api_id,
        name: "Test API".into(),
        enabled: true,
        connection: PrinterConnection::OctoPrint {
            url: "http://octoprint.local".into(),
        },
    });
    secrets.set(api_id, "FAKE_API_KEY_ONLY_FOR_TESTS").unwrap();
    store.save(&value).unwrap();
    let json = fs::read_to_string(dir.path().join("config.json")).unwrap();
    for forbidden in [
        "12345678",
        "FAKE_API_KEY_ONLY_FOR_TESTS",
        "access_code",
        "api_key",
        "password",
    ] {
        assert!(!json.contains(forbidden));
    }
    assert_eq!(
        secrets.get(api_id).unwrap().as_deref(),
        Some("FAKE_API_KEY_ONLY_FOR_TESTS")
    );
}

#[test]
fn missing_file_returns_defaults_without_recovery() {
    let dir = tempfile::tempdir().unwrap();
    let loaded = ConfigStore::new(dir.path()).load().unwrap();
    assert_eq!(loaded.config, AppConfig::default());
    assert!(!loaded.recovered_corrupt);
}

#[test]
fn corrupt_file_is_backed_up_exactly() {
    let dir = tempfile::tempdir().unwrap();
    let broken = b"{ broken test JSON";
    fs::write(dir.path().join("config.json"), broken).unwrap();
    let store = ConfigStore::new(dir.path());
    let loaded = store.load().unwrap();
    assert!(loaded.recovered_corrupt);
    assert_eq!(loaded.config, AppConfig::default());
    assert_eq!(
        fs::read(dir.path().join("config.json.bak")).unwrap(),
        broken
    );
    store.save(&loaded.config).unwrap();
    assert!(!store.load().unwrap().recovered_corrupt);
}

#[test]
fn save_replaces_existing_file_and_cleans_temporary_files() {
    let dir = tempfile::tempdir().unwrap();
    let store = ConfigStore::new(dir.path());
    store.save(&AppConfig::default()).unwrap();
    store.save(&config()).unwrap();
    assert_eq!(store.load().unwrap().config, config());
    assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
}

#[test]
fn memory_store_isolated_update_and_idempotent_delete() {
    let store = MemoryStore::default();
    let one = Uuid::from_u128(1);
    let two = Uuid::from_u128(2);
    assert_eq!(store.get(one).unwrap(), None);
    store.set(one, "12345678").unwrap();
    store.set(two, "FAKE_KEY").unwrap();
    store.set(one, "ABCDEFGH").unwrap();
    assert_eq!(store.get(one).unwrap().as_deref(), Some("ABCDEFGH"));
    store.delete(one).unwrap();
    store.delete(one).unwrap();
    assert_eq!(store.get(one).unwrap(), None);
    assert_eq!(store.get(two).unwrap().as_deref(), Some("FAKE_KEY"));
}

#[test]
fn removing_printer_removes_secret_and_persists() {
    let dir = tempfile::tempdir().unwrap();
    let store = ConfigStore::new(dir.path());
    let secrets = MemoryStore::default();
    let mut value = config();
    let id = value.printers[0].id;
    secrets.set(id, "12345678").unwrap();
    store.remove_printer(&mut value, id, &secrets).unwrap();
    assert!(value.printers.is_empty());
    assert_eq!(store.load().unwrap().config, value);
    assert_eq!(secrets.get(id).unwrap(), None);
}

#[test]
fn bambu_accepts_lan_addresses() {
    for host in [
        "192.168.1.50",
        "10.0.0.50",
        "172.16.0.50",
        "172.31.255.254",
        "169.254.1.50",
    ] {
        let mut connection = bambu();
        if let PrinterConnection::Bambu {
            host: ref mut field,
            ..
        } = connection
        {
            *field = host.into();
        }
        assert_eq!(validate_connection(&connection), Ok(()));
    }
}

#[test]
fn bambu_rejects_non_lan_and_invalid_addresses() {
    for host in [
        "127.0.0.1",
        "0.0.0.0",
        "224.0.0.1",
        "8.8.8.8",
        "172.32.0.1",
        "printer.local",
        "::1",
        "192.168.1.999",
        "192.168.001.50",
    ] {
        let mut connection = bambu();
        if let PrinterConnection::Bambu {
            host: ref mut field,
            ..
        } = connection
        {
            *field = host.into();
        }
        assert!(validate_connection(&connection).is_err());
    }
}

#[test]
fn serial_requires_eight_to_thirty_two_ascii_alphanumerics() {
    for (serial, valid) in [
        ("A".repeat(8), true),
        ("Z".repeat(32), true),
        ("A".repeat(7), false),
        ("A".repeat(33), false),
        ("ABCD-123".into(), false),
        ("üBCDEFGH".into(), false),
    ] {
        let mut connection = bambu();
        if let PrinterConnection::Bambu {
            serial: ref mut field,
            ..
        } = connection
        {
            *field = serial;
        }
        assert_eq!(validate_connection(&connection).is_ok(), valid);
    }
}

#[test]
fn access_code_requires_eight_ascii_alphanumerics() {
    for code in ["12345678", "AbCd1234"] {
        assert_eq!(validate_access_code(code), Ok(()));
    }
    for code in [
        "",
        "1234567",
        "123456789",
        "1234-678",
        "ü2345678",
        "1234567 ",
    ] {
        assert_eq!(
            validate_access_code(code),
            Err(ValidationError::InvalidAccessCode)
        );
    }
}

#[test]
fn urls_allow_only_local_http_endpoints() {
    for url in [
        "http://printer.local",
        "https://printer.local:443/api",
        "http://192.168.1.50:7125",
        "http://[fd00::50]",
        "http://[fe80::50]",
        "http://PRINTER.LOCAL",
    ] {
        assert_eq!(validate_url(url), Ok(()));
    }
    for url in [
        "ftp://printer.local",
        "http://example.com",
        "http://8.8.8.8",
        "http://127.0.0.1",
        "http://[::1]",
        "http://[2001:4860:4860::8888]",
        "http://local",
        "http://.local",
        "http://-bad.local",
        "http://printer.local.evil.test",
        "printer.local",
        "http://printer.local:99999",
        " http://printer.local",
        "http:\\printer.local",
    ] {
        assert!(validate_url(url).is_err());
    }
}

#[test]
fn url_credentials_queries_and_fragments_are_forbidden() {
    for url in [
        "http://user:FAKE_PASSWORD@printer.local",
        "http://user@printer.local",
        "http://@printer.local",
        "http://printer.local?apikey=FAKE_KEY",
        "http://printer.local#FAKE_KEY",
    ] {
        assert!(validate_url(url).is_err());
    }
}

#[test]
fn invalid_url_cannot_be_saved_and_does_not_replace_existing_config() {
    let dir = tempfile::tempdir().unwrap();
    let store = ConfigStore::new(dir.path());
    store.save(&config()).unwrap();
    let before = fs::read(dir.path().join("config.json")).unwrap();
    let mut invalid = config();
    invalid.printers[0].connection = PrinterConnection::OctoPrint {
        url: "http://user:FAKE_PASSWORD@printer.local".into(),
    };
    assert!(store.save(&invalid).is_err());
    assert_eq!(fs::read(dir.path().join("config.json")).unwrap(), before);
}

#[test]
fn duplicate_ids_and_bad_discord_ids_are_rejected() {
    let mut value = config();
    value.printers.push(value.printers[0].clone());
    assert_eq!(
        validate_config(&value),
        Err(ValidationError::DuplicatePrinterId)
    );
    value.printers.pop();
    for id in ["123", "1234567890123456a", "12345678901234567890123"] {
        value.discord.app_id_override = Some(id.into());
        assert_eq!(validate_config(&value), Err(ValidationError::InvalidAppId));
    }
    value.discord.app_id_override = Some("11111111111111111".into());
    assert_eq!(validate_config(&value), Ok(()));
}

#[test]
fn future_config_version_is_preserved() {
    let dir = tempfile::tempdir().unwrap();
    let mut value = config();
    value.version = 999;
    let bytes = serde_json::to_vec(&value).unwrap();
    fs::write(dir.path().join("config.json"), &bytes).unwrap();
    assert!(matches!(
        ConfigStore::new(dir.path()).load(),
        Err(StoreError::Validation(ValidationError::UnsupportedVersion))
    ));
    assert_eq!(fs::read(dir.path().join("config.json")).unwrap(), bytes);
}

const LEGACY: &[u8] = br#"{"host":"192.168.1.50","serial":"00M00A000000000","access_code":"FAKE_ENCRYPTED_BLOB","app_id":"11111111111111111","show_name":false}"#;

#[test]
fn legacy_format_converts_and_separates_secret() {
    let imported = parse_legacy(LEGACY, |encrypted| {
        assert_eq!(encrypted, "FAKE_ENCRYPTED_BLOB");
        Ok("12345678".into())
    })
    .unwrap();
    assert_eq!(imported.config.printers.len(), 1);
    assert_eq!(imported.config.printers[0].connection, bambu());
    assert!(!imported.config.discord.show_file_name);
    assert_eq!(
        imported.config.discord.app_id_override.as_deref(),
        Some("11111111111111111")
    );
    assert_eq!(imported.secret, "12345678");
    let json = serde_json::to_string(&imported.config).unwrap();
    assert!(!json.contains("12345678"));
    assert!(!json.contains("FAKE_ENCRYPTED_BLOB"));
}

#[test]
fn legacy_missing_show_name_defaults_true() {
    let mut json: serde_json::Value = serde_json::from_slice(LEGACY).unwrap();
    json.as_object_mut().unwrap().remove("show_name");
    let imported = parse_legacy(&serde_json::to_vec(&json).unwrap(), |_| {
        Ok("12345678".into())
    })
    .unwrap();
    assert!(imported.config.discord.show_file_name);
}

#[test]
fn legacy_malformed_and_decryption_errors_are_redacted() {
    assert!(matches!(
        parse_legacy(b"{}", |_| panic!("must not decrypt")),
        Err(LegacyError::Format)
    ));
    assert!(matches!(
        parse_legacy(LEGACY, |_| Err(LegacyError::Decryption)),
        Err(LegacyError::Decryption)
    ));
    assert!(matches!(
        parse_legacy(LEGACY, |_| Ok("bad".into())),
        Err(LegacyError::Validation(ValidationError::InvalidAccessCode))
    ));
    assert_eq!(LegacyError::Format.to_string(), "legacy_format_invalid");
}

#[test]
fn invalid_stored_connection_recovers_with_backup() {
    let dir = tempfile::tempdir().unwrap();
    let mut value = config();
    value.printers[0].connection = PrinterConnection::Moonraker {
        url: "https://example.com".into(),
    };
    fs::write(
        dir.path().join("config.json"),
        serde_json::to_vec(&value).unwrap(),
    )
    .unwrap();
    let loaded = ConfigStore::new(dir.path()).load().unwrap();
    assert!(loaded.recovered_corrupt);
    assert_eq!(loaded.config, AppConfig::default());
    assert!(dir.path().join("config.json.bak").exists());
}

#[test]
fn future_schema_with_unknown_fields_is_not_quarantined() {
    let dir = tempfile::tempdir().unwrap();
    let bytes = br#"{"version":999,"new_schema_field":true}"#;
    fs::write(dir.path().join("config.json"), bytes).unwrap();
    assert!(matches!(
        ConfigStore::new(dir.path()).load(),
        Err(StoreError::Validation(ValidationError::UnsupportedVersion))
    ));
    assert_eq!(fs::read(dir.path().join("config.json")).unwrap(), bytes);
    assert!(!dir.path().join("config.json.bak").exists());
}

#[test]
fn failed_backup_reports_error_and_keeps_original() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("config.json"), b"broken").unwrap();
    fs::create_dir(dir.path().join("config.json.bak")).unwrap();
    assert!(matches!(
        ConfigStore::new(dir.path()).load(),
        Err(StoreError::Io)
    ));
    assert_eq!(fs::read(dir.path().join("config.json")).unwrap(), b"broken");
}

#[test]
fn failed_save_does_not_remove_printer_or_secret() {
    let dir = tempfile::tempdir().unwrap();
    let blocked = dir.path().join("file");
    fs::write(&blocked, b"not a directory").unwrap();
    let store = ConfigStore::new(blocked);
    let secrets = MemoryStore::default();
    let mut value = config();
    let id = value.printers[0].id;
    secrets.set(id, "12345678").unwrap();
    assert!(store.remove_printer(&mut value, id, &secrets).is_err());
    assert_eq!(value, config());
    assert_eq!(secrets.get(id).unwrap().as_deref(), Some("12345678"));
}

#[test]
fn keyring_delete_failure_is_reported_and_can_be_retried() {
    struct FailingStore;
    impl SecretStore for FailingStore {
        fn get(&self, _: Uuid) -> Result<Option<String>, SecretError> {
            Err(SecretError)
        }
        fn set(&self, _: Uuid, _: &str) -> Result<(), SecretError> {
            Err(SecretError)
        }
        fn delete(&self, _: Uuid) -> Result<(), SecretError> {
            Err(SecretError)
        }
    }
    let dir = tempfile::tempdir().unwrap();
    let store = ConfigStore::new(dir.path());
    let mut value = config();
    let id = value.printers[0].id;
    assert!(matches!(
        store.remove_printer(&mut value, id, &FailingStore),
        Err(StoreError::Secret(_))
    ));
    assert!(value.printers.is_empty());
    assert_eq!(store.load().unwrap().config, value);
    let secrets = MemoryStore::default();
    secrets.set(id, "12345678").unwrap();
    store.remove_printer(&mut value, id, &secrets).unwrap();
    assert_eq!(secrets.get(id).unwrap(), None);
}

#[test]
fn serializable_schema_rejects_secret_fields() {
    let mut json = serde_json::to_value(config()).unwrap();
    json["printers"][0]["connection"]["access_code"] = "12345678".into();
    assert!(serde_json::from_value::<AppConfig>(json).is_err());
}
