//! The only UI entry points. Secrets are accepted as input-only DTOs.
use std::sync::{
    atomic::{AtomicU8, Ordering},
    Arc,
};

use tauri::{AppHandle, State};
use tauri_plugin_autostart::ManagerExt;
use tokio::sync::Mutex;
use uuid::Uuid;

use crate::{
    config::AppConfig,
    runtime::{ConfigView, SecretUpdate, Settings, Snapshot, Supervisor},
};

pub struct Runtime {
    pub settings: Arc<Mutex<Settings>>,
    pub supervisor: Supervisor,
    /// 0: running, 1: draining sources, 2: ready to exit.
    pub exit_state: AtomicU8,
}

#[tauri::command]
pub async fn get_config(state: State<'_, Runtime>) -> Result<ConfigView, String> {
    let settings = state.settings.clone().lock_owned().await;
    tauri::async_runtime::spawn_blocking(move || settings.view())
        .await
        .map_err(|_| "runtime_task_failed".to_owned())?
}

#[tauri::command]
pub fn get_snapshot(state: State<'_, Runtime>) -> Snapshot {
    state.supervisor.snapshot()
}

// Hold the settings guard through persistence and supervisor acknowledgement so
// concurrent UI requests cannot apply stale configurations out of order. Blocking
// keyring/filesystem operations run off the async executor and the window thread.
async fn mutate(
    app: AppHandle,
    state: &Runtime,
    action: impl FnOnce(&mut Settings, &AppHandle) -> Result<(), String> + Send + 'static,
) -> Result<ConfigView, String> {
    let mut settings = state.settings.clone().lock_owned().await;
    if state.exit_state.load(Ordering::SeqCst) != 0 {
        return Err("runtime_stopped".into());
    }
    let previous_revision = settings.revision();
    let previous_lang = settings.config.lang;
    let native_app = app.clone();
    let (settings, result) = tauri::async_runtime::spawn_blocking(move || {
        let result = action(&mut settings, &native_app).and_then(|()| settings.view());
        (settings, result)
    })
    .await
    .map_err(|_| "runtime_task_failed".to_owned())?;
    // A delete can persist removal even when keyring cleanup fails. Always bring
    // running sources in line with the actual persisted settings before replying.
    if settings.revision() != previous_revision {
        state
            .supervisor
            .reconfigure(settings.runtime_config())
            .await?;
    }
    if settings.config.lang != previous_lang {
        crate::update_tray_language(&app, settings.config.lang)
            .map_err(|_| "tray_update_failed")?;
    }
    result
}

fn save_with_autostart(
    settings: &mut Settings,
    app: &AppHandle,
    config: AppConfig,
    secrets: Vec<SecretUpdate>,
    studio_secrets: Vec<Uuid>,
) -> Result<(), String> {
    crate::config::validate::validate_config(&config).map_err(|error| error.to_string())?;
    let changed = settings.config.autostart != config.autostart;
    let requested = config.autostart;
    if changed {
        apply_autostart(app, requested)?;
    }
    let result = settings.save_with_studio(config, secrets, studio_secrets);
    if result.is_err() && changed && settings.config.autostart != requested {
        apply_autostart(app, settings.config.autostart).map_err(|_| "autostart_rollback_failed")?;
    }
    result
}

pub(crate) fn apply_autostart(app: &AppHandle, enabled: bool) -> Result<(), String> {
    let manager = app.autolaunch();
    let result = if enabled {
        manager.enable()
    } else {
        manager.disable()
    };
    result.map_err(|_| "autostart_update_failed".into())
}

#[tauri::command]
pub async fn save_config(
    app: AppHandle,
    state: State<'_, Runtime>,
    config: AppConfig,
    secrets: Vec<SecretUpdate>,
    // Printer ids whose access code is read from Bambu Studio on the backend; optional for older callers.
    studio_secrets: Option<Vec<Uuid>>,
) -> Result<ConfigView, String> {
    let studio_secrets = studio_secrets.unwrap_or_default();
    mutate(app, &state, move |settings, app| {
        save_with_autostart(settings, app, config, secrets, studio_secrets)
    })
    .await
}

#[tauri::command]
pub async fn delete_printer(
    app: AppHandle,
    state: State<'_, Runtime>,
    id: Uuid,
) -> Result<ConfigView, String> {
    mutate(app, &state, move |settings, _| settings.delete_printer(id)).await
}

#[tauri::command]
pub async fn import_legacy(
    app: AppHandle,
    state: State<'_, Runtime>,
) -> Result<ConfigView, String> {
    mutate(app, &state, |settings, _| settings.import_legacy()).await
}

#[tauri::command]
pub async fn set_autostart(
    app: AppHandle,
    state: State<'_, Runtime>,
    enabled: bool,
) -> Result<ConfigView, String> {
    mutate(app, &state, move |settings, app| {
        let mut config = settings.config.clone();
        config.autostart = enabled;
        save_with_autostart(settings, app, config, Vec::new(), Vec::new())
    })
    .await
}

#[tauri::command]
pub async fn discover_printers(seconds: Option<u8>) -> Vec<crate::discovery::Discovered> {
    tauri::async_runtime::spawn_blocking(move || crate::discovery::discover(seconds))
        .await
        .unwrap_or_default()
}
