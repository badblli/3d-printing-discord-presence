use super::*;
use crate::{config::MemoryStore, model::PrinterKind};
use async_trait::async_trait;
use std::sync::atomic::{AtomicUsize, Ordering};
use tokio::time::timeout;

fn config() -> AppConfig {
    AppConfig {
        printers: vec![PrinterConfig {
            id: Uuid::from_u128(1),
            name: "Test printer".into(),
            enabled: true,
            connection: PrinterConnection::Bambu {
                host: "192.168.1.50".into(),
                serial: "00M00A000000000".into(),
                model: Some("A1 Combo".into()),
            },
        }],
        discord: crate::config::DiscordConfig {
            enabled: false,
            ..Default::default()
        },
        ..Default::default()
    }
}

fn runtime_config(config: AppConfig) -> RuntimeConfig {
    RuntimeConfig {
        config,
        legacy_available: false,
        recovered_corrupt: false,
    }
}

fn status(state: PrintState, remaining_secs: Option<u32>) -> PrintStatus {
    PrintStatus {
        kind: PrinterKind::Bambu,
        state,
        progress: Some(42),
        remaining_secs,
        file_name: Some("fake-test.gcode.3mf".into()),
        printer_model: Some("A1 Combo".into()),
        layer: Some(4),
        total_layers: Some(10),
        nozzle_temp: Some(210.0),
        bed_temp: Some(60.0),
        observed_at_ms: 1_000_000,
    }
}

fn row(id: u128, state: PrintState, remaining: Option<u32>) -> PrinterSnapshot {
    PrinterSnapshot {
        id: Uuid::from_u128(id),
        enabled: true,
        has_secret: true,
        unsupported: false,
        connection: ConnectionState::Connected,
        status: Some(status(state, remaining)),
    }
}

#[test]
fn selection_prioritizes_printing_then_preparing_then_paused() {
    let mut rows = vec![
        row(1, PrintState::Paused, Some(1)),
        row(2, PrintState::Preparing, Some(2)),
        row(3, PrintState::Printing, Some(300)),
        row(4, PrintState::Finished, Some(0)),
    ];
    assert_eq!(select_printer(&rows).unwrap().id, Uuid::from_u128(3));
    rows[2].connection = ConnectionState::Error(ConnectionError::Stale);
    assert_eq!(select_printer(&rows).unwrap().id, Uuid::from_u128(2));
    rows[1].enabled = false;
    assert_eq!(select_printer(&rows).unwrap().id, Uuid::from_u128(1));
    rows[0].unsupported = true;
    assert!(select_printer(&rows).is_none());
    for state in [PrintState::Idle, PrintState::Finished, PrintState::Failed] {
        assert!(select_printer(&[row(1, state, Some(1))]).is_none());
    }
}

#[test]
fn selection_uses_absolute_finish_time_and_stable_ties() {
    let mut rows = vec![
        row(1, PrintState::Printing, None),
        row(2, PrintState::Printing, Some(30)),
        row(3, PrintState::Printing, Some(90)),
    ];
    // The longer remaining duration was observed earlier and finishes sooner.
    rows[2].status.as_mut().unwrap().observed_at_ms -= 100_000;
    assert_eq!(select_printer(&rows).unwrap().id, Uuid::from_u128(3));
    rows[2].status.as_mut().unwrap().observed_at_ms += 100_000;
    assert_eq!(select_printer(&rows).unwrap().id, Uuid::from_u128(2));
    rows[2].status.as_mut().unwrap().remaining_secs = Some(30);
    assert_eq!(select_printer(&rows).unwrap().id, Uuid::from_u128(2));
}

struct FakeSource {
    live: Arc<AtomicUsize>,
    starts: Arc<AtomicUsize>,
    stops: Arc<AtomicUsize>,
    overlap: Arc<AtomicUsize>,
}

#[async_trait]
impl PrinterSource for FakeSource {
    async fn run(self: Box<Self>, events: mpsc::Sender<SourceEvent>, shutdown: CancellationToken) {
        if self.live.fetch_add(1, Ordering::SeqCst) != 0 {
            self.overlap.fetch_add(1, Ordering::SeqCst);
        }
        self.starts.fetch_add(1, Ordering::SeqCst);
        events
            .send(SourceEvent::Connection {
                state: ConnectionState::Connected,
            })
            .await
            .unwrap();
        events
            .send(SourceEvent::Status {
                status: Some(status(PrintState::Printing, Some(30))),
            })
            .await
            .unwrap();
        shutdown.cancelled().await;
        // Fill the adapter channel during shutdown: restart must not deadlock or
        // let this old status override the new generation's clean snapshot.
        for _ in 0..64 {
            let _ = events.try_send(SourceEvent::Status {
                status: Some(status(PrintState::Paused, None)),
            });
        }
        self.live.fetch_sub(1, Ordering::SeqCst);
        self.stops.fetch_add(1, Ordering::SeqCst);
    }
}

async fn wait_for(supervisor: &Supervisor, predicate: impl Fn(&Snapshot) -> bool) -> Snapshot {
    let mut snapshots = supervisor.subscribe();
    timeout(Duration::from_secs(3), async {
        loop {
            let snapshot = snapshots.borrow_and_update().clone();
            if predicate(&snapshot) {
                return snapshot;
            }
            snapshots.changed().await.unwrap();
        }
    })
    .await
    .expect("supervisor did not reach expected state")
}

#[tokio::test]
async fn restart_waits_for_old_source_and_shutdown_cleans_up() {
    let live = Arc::new(AtomicUsize::new(0));
    let starts = Arc::new(AtomicUsize::new(0));
    let stops = Arc::new(AtomicUsize::new(0));
    let overlap = Arc::new(AtomicUsize::new(0));
    let factory: Arc<SourceFactory> = {
        let (live, starts, stops, overlap) =
            (live.clone(), starts.clone(), stops.clone(), overlap.clone());
        Arc::new(move |_, _| {
            Ok(Some(Box::new(FakeSource {
                live: live.clone(),
                starts: starts.clone(),
                stops: stops.clone(),
                overlap: overlap.clone(),
            })))
        })
    };
    let secrets = Arc::new(MemoryStore::default());
    secrets.set(Uuid::from_u128(1), "12345678").unwrap();
    let mut config = config();
    let supervisor = Supervisor::with_factory(runtime_config(config.clone()), secrets, factory);
    let snapshot = wait_for(&supervisor, |snapshot| snapshot.activity.is_some()).await;
    assert_eq!(snapshot.selected_printer, Some(Uuid::from_u128(1)));
    assert_eq!(snapshot.discord, DiscordState::Disabled);
    assert!(snapshot.activity.unwrap().state.contains("Printing"));
    config.lang = Lang::Tr;
    config.discord.show_file_name = false;
    supervisor
        .reconfigure(runtime_config(config.clone()))
        .await
        .unwrap();
    let snapshot = wait_for(&supervisor, |snapshot| snapshot.activity.is_some()).await;
    assert_eq!(
        snapshot.activity.as_ref().unwrap().details,
        "3D baskı alıyorum"
    );
    assert!(snapshot.activity.unwrap().state.contains("Basılıyor"));
    assert_eq!(starts.load(Ordering::SeqCst), 2);
    assert_eq!(stops.load(Ordering::SeqCst), 1);
    assert_eq!(overlap.load(Ordering::SeqCst), 0);
    config.printers[0].enabled = false;
    supervisor
        .reconfigure(runtime_config(config.clone()))
        .await
        .unwrap();
    let snapshot = supervisor.snapshot();
    assert!(snapshot.activity.is_none());
    assert_eq!(snapshot.printers[0].connection, ConnectionState::Stopped);
    assert_eq!(starts.load(Ordering::SeqCst), 2);
    config.printers[0].enabled = true;
    supervisor
        .reconfigure(runtime_config(config))
        .await
        .unwrap();
    wait_for(&supervisor, |snapshot| snapshot.activity.is_some()).await;
    timeout(Duration::from_secs(3), supervisor.shutdown())
        .await
        .unwrap();
    assert_eq!(live.load(Ordering::SeqCst), 0);
    assert_eq!(starts.load(Ordering::SeqCst), 3);
    assert_eq!(stops.load(Ordering::SeqCst), 3);
    assert!(supervisor.snapshot().activity.is_none());
    assert!(supervisor
        .reconfigure(runtime_config(AppConfig::default()))
        .await
        .is_err());
}

#[tokio::test]
async fn unsupported_missing_secret_and_discord_modes_are_explicit() {
    let mut config = config();
    config.discord.enabled = true;
    // Invalid override is useful here to exercise no_app_id independently of the
    // build-time shared ID; persisted configs reject this value at validation.
    config.discord.app_id_override = Some("invalid".into());
    for (id, connection) in [
        (
            2,
            PrinterConnection::Moonraker {
                url: "http://printer.local".into(),
            },
        ),
        (
            3,
            PrinterConnection::OctoPrint {
                url: "http://printer.local".into(),
            },
        ),
        (
            4,
            PrinterConnection::PrusaLink {
                url: "http://printer.local".into(),
            },
        ),
    ] {
        config.printers.push(PrinterConfig {
            id: Uuid::from_u128(id),
            name: "Test printer".into(),
            enabled: true,
            connection,
        });
    }
    let supervisor = Supervisor::spawn(
        runtime_config(config.clone()),
        Arc::new(MemoryStore::default()),
    );
    let snapshot = wait_for(&supervisor, |snapshot| snapshot.printers.len() == 4).await;
    assert_eq!(snapshot.discord, DiscordState::NoAppId);
    assert_eq!(
        snapshot.printers[0].connection,
        ConnectionState::Error(ConnectionError::AuthRejected)
    );
    for printer in &snapshot.printers[1..] {
        assert!(printer.unsupported);
        assert_eq!(printer.connection, ConnectionState::Stopped);
    }
    // Disabled never constructs a PresenceClient, even with a valid ID.
    config.discord.enabled = false;
    config.discord.app_id_override = Some("123456789012345678".into());
    supervisor
        .reconfigure(runtime_config(config))
        .await
        .unwrap();
    assert_eq!(supervisor.snapshot().discord, DiscordState::Disabled);
    supervisor.shutdown().await;
}

#[tokio::test]
async fn config_responses_and_snapshot_events_never_serialize_secrets() {
    let dir = tempfile::tempdir().unwrap();
    let secrets = Arc::new(MemoryStore::default());
    let mut settings = Settings::load(dir.path(), secrets.clone()).unwrap();
    let mut config = config();
    config.printers.push(PrinterConfig {
        id: Uuid::from_u128(2),
        name: "Test API printer".into(),
        enabled: true,
        connection: PrinterConnection::OctoPrint {
            url: "http://printer.local".into(),
        },
    });
    settings
        .save(
            config,
            vec![
                SecretUpdate {
                    id: Uuid::from_u128(1),
                    secret: "12345678".into(),
                },
                SecretUpdate {
                    id: Uuid::from_u128(2),
                    secret: "fake-api-key-private".into(),
                },
            ],
        )
        .unwrap();
    let supervisor = Supervisor::with_factory(
        settings.runtime_config(),
        secrets.clone(),
        Arc::new(|_, _| {
            Ok(Some(Box::new(FakeSource {
                live: Arc::new(AtomicUsize::new(0)),
                starts: Arc::new(AtomicUsize::new(0)),
                stops: Arc::new(AtomicUsize::new(0)),
                overlap: Arc::new(AtomicUsize::new(0)),
            })))
        }),
    );
    let snapshot = wait_for(&supervisor, |snapshot| {
        snapshot.printers.len() == 2 && snapshot.activity.is_some()
    })
    .await;
    let response = serde_json::to_value(settings.view().unwrap()).unwrap();
    assert_eq!(response["printers"][0]["has_secret"], true);
    assert_eq!(response["printers"][1]["has_secret"], true);
    assert_eq!(response["printers"][1]["connection"]["kind"], "octo_print");
    let event = serde_json::to_value(&snapshot).unwrap();
    assert_eq!(event["discord"], "disabled");
    assert_eq!(
        event["printers"][0]["connection"],
        serde_json::json!({"state":"connected"})
    );
    let disk = std::fs::read_to_string(dir.path().join("config.json")).unwrap();
    for serialized in [response.to_string(), event.to_string(), disk] {
        for private in [
            "12345678",
            "fake-api-key-private",
            "\"secret\"",
            "access_code",
            "api_key",
        ] {
            assert!(
                !serialized.contains(private),
                "secret leaked from DTO or config"
            );
        }
    }
    let error = settings
        .save(
            settings.config.clone(),
            vec![SecretUpdate {
                id: Uuid::from_u128(1),
                secret: "fake-invalid-private-access-code".into(),
            }],
        )
        .unwrap_err();
    assert_eq!(error, "invalid_access_code");
    assert_eq!(
        secrets.get(Uuid::from_u128(1)).unwrap().as_deref(),
        Some("12345678")
    );
    settings.delete_printer(Uuid::from_u128(1)).unwrap();
    assert!(secrets.get(Uuid::from_u128(1)).unwrap().is_none());
    settings.delete_printer(Uuid::from_u128(1)).unwrap();
    let mut empty = settings.config.clone();
    empty.printers.clear();
    settings.save(empty, Vec::new()).unwrap();
    assert!(secrets.get(Uuid::from_u128(2)).unwrap().is_none());
    supervisor.shutdown().await;
}

#[test]
fn settings_roll_back_credentials_if_config_write_fails() {
    let dir = tempfile::tempdir().unwrap();
    let secrets = Arc::new(MemoryStore::default());
    let mut settings = Settings::load(dir.path(), secrets.clone()).unwrap();
    settings
        .save(
            config(),
            vec![SecretUpdate {
                id: Uuid::from_u128(1),
                secret: "12345678".into(),
            }],
        )
        .unwrap();
    // A directory at the destination makes the atomic replacement fail.
    std::fs::remove_file(dir.path().join("config.json")).unwrap();
    std::fs::create_dir(dir.path().join("config.json")).unwrap();
    let result = settings.save(
        config(),
        vec![SecretUpdate {
            id: Uuid::from_u128(1),
            secret: "ABCDEFGH".into(),
        }],
    );
    assert_eq!(result.unwrap_err(), "config_io_failed");
    assert_eq!(
        secrets.get(Uuid::from_u128(1)).unwrap().as_deref(),
        Some("12345678")
    );
}

#[test]
fn saving_shorter_list_deletes_keys_and_existing_language_is_preserved() {
    let dir = tempfile::tempdir().unwrap();
    let secrets = Arc::new(MemoryStore::default());
    let mut settings = Settings::load(dir.path(), secrets.clone()).unwrap();
    let mut config = config();
    config.lang = Lang::Tr;
    settings
        .save(
            config.clone(),
            vec![SecretUpdate {
                id: Uuid::from_u128(1),
                secret: "12345678".into(),
            }],
        )
        .unwrap();
    let loaded = Settings::load(dir.path(), secrets.clone()).unwrap();
    assert_eq!(loaded.config.lang, Lang::Tr);
    assert!(!loaded.runtime_config().legacy_available);
    config.printers.clear();
    settings.save(config, Vec::new()).unwrap();
    assert!(secrets.get(Uuid::from_u128(1)).unwrap().is_none());
}

#[tokio::test]
async fn snapshot_emitter_coalesces_changes_and_sends_last_value() {
    let initial = runtime_config(AppConfig::default()).snapshot();
    let (tx, rx) = watch::channel(initial.clone());
    let (emitted, mut received) = mpsc::unbounded_channel();
    let task = tokio::spawn(emit_snapshots(rx, move |snapshot| {
        let _ = emitted.send((tokio::time::Instant::now(), snapshot.clone()));
    }));
    let first = timeout(Duration::from_secs(3), received.recv())
        .await
        .unwrap()
        .unwrap();
    let mut updated = initial;
    updated.legacy_available = true;
    tx.send_replace(updated.clone());
    updated.recovered_corrupt = true;
    tx.send_replace(updated.clone());
    let second = timeout(Duration::from_secs(3), received.recv())
        .await
        .unwrap()
        .unwrap();
    assert!(second.0.duration_since(first.0) >= Duration::from_secs(1));
    assert!(second.1.legacy_available && second.1.recovered_corrupt);
    tx.send_replace(updated);
    assert!(timeout(Duration::from_millis(30), received.recv())
        .await
        .is_err());
    drop(tx);
    timeout(Duration::from_secs(3), task)
        .await
        .unwrap()
        .unwrap();
}
