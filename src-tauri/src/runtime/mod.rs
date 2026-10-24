//! One owner for printer tasks and local Discord IPC. Only redacted DTOs leave here.
mod settings;
#[cfg(test)]
mod tests;

pub use settings::{ConfigView, SecretUpdate, Settings};

use std::{sync::Arc, time::Duration};

use serde::Serialize;
use tokio::sync::{mpsc, oneshot, watch, Mutex};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::{
    config::{AppConfig, Lang, PrinterConfig, PrinterConnection, SecretStore},
    model::{now_ms, ConnectionError, ConnectionState, PrintState, PrintStatus, SourceEvent},
    presence::{self, Activity, PresenceClient, PresenceOptions, PresenceState},
    printers::{bambu::BambuSource, PrinterSource},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DiscordState {
    Disabled,
    NoAppId,
    Disconnected,
    Connected,
    Error,
}

#[derive(Clone, PartialEq, Serialize)]
pub struct PrinterSnapshot {
    pub id: Uuid,
    pub enabled: bool,
    pub has_secret: bool,
    pub unsupported: bool,
    pub connection: ConnectionState,
    pub status: Option<PrintStatus>,
}

#[derive(Clone, PartialEq, Serialize)]
pub struct Snapshot {
    pub printers: Vec<PrinterSnapshot>,
    pub discord: DiscordState,
    pub selected_printer: Option<Uuid>,
    pub activity: Option<Activity>,
    pub legacy_available: bool,
    pub recovered_corrupt: bool,
}

#[derive(Clone)]
pub struct RuntimeConfig {
    pub config: AppConfig,
    pub legacy_available: bool,
    pub recovered_corrupt: bool,
}

impl RuntimeConfig {
    fn snapshot(&self) -> Snapshot {
        Snapshot {
            printers: Vec::new(),
            discord: discord_mode(&self.config),
            selected_printer: None,
            activity: None,
            legacy_available: self.legacy_available,
            recovered_corrupt: self.recovered_corrupt,
        }
    }
}

fn discord_mode(config: &AppConfig) -> DiscordState {
    if !config.discord.enabled {
        DiscordState::Disabled
    } else if presence::app_id(config.discord.app_id_override.as_deref()).is_none() {
        DiscordState::NoAppId
    } else {
        DiscordState::Disconnected
    }
}

/// Ignore disconnected/stale sources. Unknown deadlines sort after known ones;
/// ties retain config order so the card cannot oscillate between equal printers.
fn select_printer(printers: &[PrinterSnapshot]) -> Option<&PrinterSnapshot> {
    printers
        .iter()
        .filter(|printer| {
            printer.enabled
                && !printer.unsupported
                && printer.connection == ConnectionState::Connected
        })
        .filter_map(|printer| {
            let status = printer.status.as_ref()?;
            let rank = match status.state {
                PrintState::Printing => 0,
                PrintState::Preparing => 1,
                PrintState::Paused => 2,
                _ => return None,
            };
            let deadline = if status.state == PrintState::Printing {
                status.remaining_secs.map_or(u64::MAX, |secs| {
                    status.observed_at_ms.saturating_add(u64::from(secs) * 1000)
                })
            } else {
                u64::MAX
            };
            Some(((rank, deadline), printer))
        })
        .min_by_key(|(key, _)| *key)
        .map(|(_, printer)| printer)
}

type SourceResult = Result<Option<Box<dyn PrinterSource>>, ConnectionError>;
type SourceFactory = dyn Fn(&PrinterConfig, Option<String>) -> SourceResult + Send + Sync;

fn source_for(printer: &PrinterConfig, secret: Option<String>) -> SourceResult {
    match &printer.connection {
        PrinterConnection::Bambu {
            host,
            serial,
            model,
        } => {
            let access_code = secret.ok_or(ConnectionError::AuthRejected)?;
            Ok(Some(Box::new(BambuSource {
                host: host.clone(),
                serial: serial.clone(),
                access_code,
                model: model.clone(),
            })))
        }
        _ => Ok(None),
    }
}

struct Change {
    config: RuntimeConfig,
    applied: oneshot::Sender<()>,
}

struct TaggedEvent {
    generation: u64,
    id: Uuid,
    // None means an adapter returned unexpectedly: discard its last telemetry.
    event: Option<SourceEvent>,
}

struct Worker {
    shutdown: CancellationToken,
    task: tokio::task::JoinHandle<()>,
}

impl Worker {
    fn spawn(
        source: Box<dyn PrinterSource>,
        id: Uuid,
        generation: u64,
        events: mpsc::Sender<TaggedEvent>,
    ) -> Self {
        let shutdown = CancellationToken::new();
        let token = shutdown.clone();
        let task = tokio::spawn(async move {
            let (tx, mut rx) = mpsc::channel(32);
            let source_task = tokio::spawn(source.run(tx, token.clone()));
            loop {
                tokio::select! {
                    biased;
                    _ = token.cancelled() => break,
                    event = rx.recv() => {
                        let ended = event.is_none();
                        tokio::select! {
                            biased;
                            _ = token.cancelled() => break,
                            result = events.send(TaggedEvent { generation, id, event }) => {
                                if result.is_err() || ended { break; }
                            }
                        }
                    }
                }
            }
            token.cancel();
            drop(rx);
            let _ = source_task.await;
        });
        Self { shutdown, task }
    }
}

async fn stop_workers(workers: &mut Vec<Worker>) {
    for worker in workers.iter() {
        worker.shutdown.cancel();
    }
    for worker in workers.drain(..) {
        let _ = worker.task.await;
    }
}

/// The handle never contains credentials or exposes source/client internals.
pub struct Supervisor {
    snapshots: watch::Receiver<Snapshot>,
    changes: mpsc::Sender<Change>,
    shutdown: CancellationToken,
    task: Mutex<Option<tauri::async_runtime::JoinHandle<()>>>,
}

impl Supervisor {
    pub fn spawn(config: RuntimeConfig, secrets: Arc<dyn SecretStore>) -> Self {
        Self::with_factory(config, secrets, Arc::new(source_for))
    }

    fn with_factory(
        config: RuntimeConfig,
        secrets: Arc<dyn SecretStore>,
        factory: Arc<SourceFactory>,
    ) -> Self {
        let (snapshots_tx, snapshots) = watch::channel(config.snapshot());
        let (changes, changes_rx) = mpsc::channel(8);
        let shutdown = CancellationToken::new();
        let task = tauri::async_runtime::spawn(run(
            config,
            secrets,
            factory,
            changes_rx,
            snapshots_tx,
            shutdown.clone(),
        ));
        Self {
            snapshots,
            changes,
            shutdown,
            task: Mutex::new(Some(task)),
        }
    }

    pub fn subscribe(&self) -> watch::Receiver<Snapshot> {
        self.snapshots.clone()
    }

    pub fn snapshot(&self) -> Snapshot {
        self.snapshots.borrow().clone()
    }

    pub async fn reconfigure(&self, config: RuntimeConfig) -> Result<(), String> {
        let (applied, done) = oneshot::channel();
        self.changes
            .send(Change { config, applied })
            .await
            .map_err(|_| "runtime_stopped".to_owned())?;
        done.await.map_err(|_| "runtime_stopped".to_owned())
    }

    pub async fn shutdown(&self) {
        self.shutdown.cancel();
        if let Some(task) = self.task.lock().await.take() {
            let _ = task.await;
        }
    }
}

impl Drop for Supervisor {
    fn drop(&mut self) {
        self.shutdown.cancel();
    }
}

struct Session {
    snapshot: Snapshot,
    workers: Vec<Worker>,
    client: Option<PresenceClient>,
    presence_states: Option<watch::Receiver<PresenceState>>,
}

impl Session {
    fn start(
        config: &RuntimeConfig,
        secrets: &dyn SecretStore,
        factory: &SourceFactory,
        generation: u64,
        events: &mpsc::Sender<TaggedEvent>,
    ) -> Self {
        let mut session = Self {
            snapshot: config.snapshot(),
            workers: Vec::new(),
            client: None,
            presence_states: None,
        };
        for printer in &config.config.printers {
            let secret = secrets.get(printer.id);
            let mut row = PrinterSnapshot {
                id: printer.id,
                enabled: printer.enabled,
                has_secret: secret.as_ref().is_ok_and(|value| value.is_some()),
                unsupported: !matches!(printer.connection, PrinterConnection::Bambu { .. }),
                connection: ConnectionState::Stopped,
                status: None,
            };
            if printer.enabled && !row.unsupported {
                let source = secret
                    .map_err(|_| ConnectionError::Protocol)
                    .and_then(|secret| factory(printer, secret));
                match source {
                    Ok(Some(source)) => {
                        row.connection = ConnectionState::Connecting;
                        session.workers.push(Worker::spawn(
                            source,
                            printer.id,
                            generation,
                            events.clone(),
                        ));
                    }
                    Ok(None) => row.unsupported = true,
                    Err(error) => row.connection = ConnectionState::Error(error),
                }
            }
            session.snapshot.printers.push(row);
        }
        if session.snapshot.discord == DiscordState::Disconnected {
            if let Some(id) = presence::app_id(config.config.discord.app_id_override.as_deref()) {
                match PresenceClient::new(&id) {
                    Ok(client) => {
                        session.presence_states = Some(client.subscribe());
                        session.client = Some(client);
                    }
                    Err(_) => session.snapshot.discord = DiscordState::Error,
                }
            }
        }
        session
    }

    fn update_activity(&mut self, config: &AppConfig) {
        let selected = select_printer(&self.snapshot.printers);
        self.snapshot.selected_printer = selected.map(|printer| printer.id);
        self.snapshot.activity = selected
            .and_then(|printer| printer.status.as_ref())
            .and_then(|status| {
                presence::activity_for(
                    status,
                    &PresenceOptions {
                        show_file_name: config.discord.show_file_name,
                        lang: match config.lang {
                            Lang::Tr => "tr",
                            Lang::En => "en",
                        }
                        .into(),
                    },
                    now_ms() / 1000,
                )
            });
        if let Some(client) = &self.client {
            client.set(self.snapshot.activity.clone());
        }
    }

    fn apply(&mut self, id: Uuid, event: Option<SourceEvent>) {
        if let Some(printer) = self.snapshot.printers.iter_mut().find(|row| row.id == id) {
            match event {
                Some(SourceEvent::Connection { state }) => {
                    printer.connection = state;
                    if state != ConnectionState::Connected {
                        printer.status = None;
                    }
                }
                Some(SourceEvent::Status { status }) => printer.status = status,
                None => {
                    printer.connection = ConnectionState::Stopped;
                    printer.status = None;
                }
            }
        }
    }

    async fn stop(&mut self) {
        stop_workers(&mut self.workers).await;
        self.client = None;
        self.presence_states = None;
    }
}

async fn presence_changed(states: &mut Option<watch::Receiver<PresenceState>>) -> PresenceState {
    match states {
        Some(states) => {
            if states.changed().await.is_err() {
                std::future::pending::<()>().await;
            }
            *states.borrow_and_update()
        }
        None => std::future::pending().await,
    }
}

async fn run(
    mut config: RuntimeConfig,
    secrets: Arc<dyn SecretStore>,
    factory: Arc<SourceFactory>,
    mut changes: mpsc::Receiver<Change>,
    snapshots: watch::Sender<Snapshot>,
    shutdown: CancellationToken,
) {
    let (events_tx, mut events) = mpsc::channel::<TaggedEvent>(128);
    let mut generation = 0;
    let mut session = Session::start(
        &config,
        secrets.as_ref(),
        factory.as_ref(),
        generation,
        &events_tx,
    );
    snapshots.send_replace(session.snapshot.clone());
    loop {
        tokio::select! {
            biased;
            _ = shutdown.cancelled() => break,
            change = changes.recv() => {
                let Some(change) = change else { break; };
                session.stop().await;
                generation += 1;
                config = change.config;
                session = Session::start(&config, secrets.as_ref(), factory.as_ref(), generation, &events_tx);
                snapshots.send_replace(session.snapshot.clone());
                let _ = change.applied.send(());
            }
            Some(event) = events.recv() => {
                if event.generation == generation {
                    session.apply(event.id, event.event);
                    session.update_activity(&config.config);
                    publish(&snapshots, &session.snapshot);
                }
            }
            state = presence_changed(&mut session.presence_states) => {
                session.snapshot.discord = match state {
                    PresenceState::Connected => DiscordState::Connected,
                    PresenceState::Disconnected => DiscordState::Disconnected,
                    PresenceState::Error(_) => DiscordState::Error,
                };
                publish(&snapshots, &session.snapshot);
            }
        }
    }
    session.stop().await;
    for printer in &mut session.snapshot.printers {
        printer.connection = ConnectionState::Stopped;
        printer.status = None;
    }
    session.update_activity(&config.config);
    if matches!(
        session.snapshot.discord,
        DiscordState::Connected | DiscordState::Error
    ) {
        session.snapshot.discord = DiscordState::Disconnected;
    }
    publish(&snapshots, &session.snapshot);
}

fn publish(sender: &watch::Sender<Snapshot>, snapshot: &Snapshot) {
    sender.send_if_modified(|current| {
        if current == snapshot {
            false
        } else {
            *current = snapshot.clone();
            true
        }
    });
}

/// A trailing latest-value emission keeps fast telemetry bounded without losing
/// the final update when a printer goes quiet. Command reads stay immediate.
pub async fn emit_snapshots(mut snapshots: watch::Receiver<Snapshot>, emit: impl Fn(&Snapshot)) {
    let mut previous = snapshots.borrow_and_update().clone();
    emit(&previous);
    let mut next = tokio::time::Instant::now() + Duration::from_secs(1);
    while snapshots.changed().await.is_ok() {
        tokio::time::sleep_until(next).await;
        let snapshot = snapshots.borrow_and_update().clone();
        if snapshot != previous {
            emit(&snapshot);
            previous = snapshot;
            next = tokio::time::Instant::now() + Duration::from_secs(1);
        }
    }
}
