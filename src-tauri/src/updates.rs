//! The webview can request an update, but never supplies an endpoint or installer.
use std::{collections::HashSet, path::Path, sync::Mutex, time::{Duration, SystemTime, UNIX_EPOCH}};

use serde::Serialize;
use tauri::{menu::{Menu, MenuItem}, AppHandle, Emitter, Manager, State};
use tauri_plugin_notification::NotificationExt;
use tauri_plugin_updater::{Update, UpdaterExt};

use crate::config::Lang;

const INITIAL_DELAY: Duration = Duration::from_secs(20);
const CHECK_INTERVAL: Duration = Duration::from_secs(6 * 60 * 60);

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum UpdateState {
    Idle,
    Checking,
    Available { version: String, notes: Option<String> },
    Downloading { percent: u8 },
    Ready,
    Error { code: &'static str },
}

#[derive(Clone, Debug, Serialize)]
pub struct UpdateSnapshot {
    pub current_version: String,
    pub last_checked_ms: Option<u64>,
    pub status: UpdateState,
}

struct Machine {
    snapshot: UpdateSnapshot,
    notified: HashSet<String>,
}

impl Machine {
    fn new(version: String) -> Self {
        Self {
            snapshot: UpdateSnapshot { current_version: version, last_checked_ms: None, status: UpdateState::Idle },
            notified: HashSet::new(),
        }
    }

    fn begin_check(&mut self) {
        self.snapshot.status = UpdateState::Checking;
    }

    fn checked(&mut self, available: Option<(String, Option<String>)>, now: u64) {
        self.snapshot.last_checked_ms = Some(now);
        self.snapshot.status = match available {
            Some((version, notes)) => UpdateState::Available { version, notes },
            None => UpdateState::Idle,
        };
    }

    fn fail(&mut self, code: &'static str) {
        self.snapshot.status = UpdateState::Error { code };
    }

    fn progress(&mut self, received: u64, total: Option<u64>) -> bool {
        // Keep 100% for verified bytes; missing Content-Length remains indeterminate at 0%.
        let percent = total.filter(|total| *total > 0)
            .map(|total| ((received as u128 * 100 / total as u128).min(99)) as u8)
            .unwrap_or(0);
        let next = UpdateState::Downloading { percent };
        let changed = self.snapshot.status != next;
        self.snapshot.status = next;
        changed
    }

    fn ready(&mut self) {
        self.snapshot.status = UpdateState::Ready;
    }

    fn should_notify(&mut self, version: &str, hidden: bool) -> bool {
        hidden && self.notified.insert(version.to_owned())
    }
}

pub struct Updates {
    machine: Mutex<Machine>,
    // Serialize manual checks, scheduled checks and installs. Reject concurrent work.
    operation: tokio::sync::Mutex<()>,
    pending: Mutex<Option<Update>>,
    menu: Menu<tauri::Wry>,
    item: Mutex<Option<MenuItem<tauri::Wry>>>,
}

impl Updates {
    fn publish(&self, app: &AppHandle) -> UpdateSnapshot {
        let snapshot = self.machine.lock().unwrap().snapshot.clone();
        let _ = app.emit_to("main", "update", &snapshot);
        snapshot
    }

    fn fail(&self, app: &AppHandle, code: &'static str) -> String {
        self.machine.lock().unwrap().fail(code);
        self.publish(app);
        code.to_owned()
    }

    pub(crate) fn set_language(&self, lang: Lang) {
        if let Some(update) = self.pending.lock().unwrap().as_ref() {
            if let Some(item) = self.item.lock().unwrap().as_ref() {
                let _ = item.set_text(update_label(lang, &update.version));
            }
        }
    }

    fn sync_menu(&self, app: &AppHandle, lang: Lang) {
        let pending = self.pending.lock().unwrap();
        let mut item = self.item.lock().unwrap();
        if let Some(update) = pending.as_ref() {
            if let Some(item) = item.as_ref() {
                let _ = item.set_text(update_label(lang, &update.version));
            } else if let Ok(new_item) = MenuItem::with_id(app, "update", update_label(lang, &update.version), true, None::<&str>) {
                if self.menu.append(&new_item).is_ok() { *item = Some(new_item); }
            }
        } else if let Some(old_item) = item.take() {
            let _ = self.menu.remove(&old_item);
        }
    }
}

fn update_label(lang: Lang, version: &str) -> String {
    match lang {
        Lang::Tr => format!("Güncelleme var (v{version})"),
        Lang::En => format!("Update available (v{version})"),
    }
}

pub(crate) fn start(app: &AppHandle, menu: Menu<tauri::Wry>) {
    let mut machine = Machine::new(app.package_info().version.to_string());
    if let Ok(directory) = app.path().app_config_dir() {
        machine.notified = read_notified(&directory.join("update-notifications.json"));
    }
    app.manage(Updates {
        machine: Mutex::new(machine),
        operation: tokio::sync::Mutex::new(()),
        pending: Mutex::new(None), menu, item: Mutex::new(None),
    });
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(INITIAL_DELAY).await;
        loop {
            let _ = check_for_updates(app.clone(), app.state::<Updates>()).await;
            tokio::time::sleep(CHECK_INTERVAL).await;
        }
    });
}

fn read_notified(path: &Path) -> HashSet<String> {
    std::fs::read_to_string(path).ok()
        .and_then(|data| serde_json::from_str::<HashSet<String>>(&data).ok())
        .unwrap_or_default()
}

fn write_notified(path: &Path, versions: &HashSet<String>) {
    let mut ordered: Vec<_> = versions.iter().collect();
    ordered.sort();
    if let (Some(parent), Ok(data)) = (path.parent(), serde_json::to_vec(&ordered)) {
        if std::fs::create_dir_all(parent).is_ok() {
            let _ = std::fs::write(path, data);
        }
    }
}

#[tauri::command]
pub fn get_update_state(state: State<'_, Updates>) -> UpdateSnapshot {
    state.machine.lock().unwrap().snapshot.clone()
}

#[tauri::command]
pub async fn check_for_updates(app: AppHandle, state: State<'_, Updates>) -> Result<UpdateSnapshot, String> {
    let _operation = state.operation.try_lock().map_err(|_| "update_busy")?;
    state.machine.lock().unwrap().begin_check();
    state.publish(&app);
    let result = async {
        app.updater_builder().timeout(Duration::from_secs(30)).build()?.check().await
    }.await;
    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis() as u64;
    let update = match result {
        Ok(update) => update,
        Err(_) => {
            state.machine.lock().unwrap().snapshot.last_checked_ms = Some(now);
            return Err(state.fail(&app, "check_failed"));
        }
    };
    let metadata = update.as_ref().map(|update| (update.version.clone(), update.body.clone()));
    state.machine.lock().unwrap().checked(metadata.clone(), now);
    *state.pending.lock().unwrap() = update;
    let lang = app.state::<crate::commands::Runtime>().settings.lock().await.config.lang;
    state.sync_menu(&app, lang);
    state.publish(&app);
    if let Some((version, _)) = metadata {
        let hidden = app.get_webview_window("main")
            .is_some_and(|window| !window.is_visible().unwrap_or(true) || window.is_minimized().unwrap_or(false));
        if state.machine.lock().unwrap().should_notify(&version, hidden) {
            if let Ok(directory) = app.path().app_config_dir() {
                write_notified(&directory.join("update-notifications.json"), &state.machine.lock().unwrap().notified);
            }
            let body = match lang {
                Lang::Tr => format!("Yeni sürüm v{version} hazır. Güncellemek için uygulamayı açın."),
                Lang::En => format!("Version v{version} is available. Open the app to update."),
            };
            let _ = app.notification().builder().title("Plycast").body(body).show();
        }
    }
    Ok(state.machine.lock().unwrap().snapshot.clone())
}

#[tauri::command]
pub async fn install_update(app: AppHandle, state: State<'_, Updates>) -> Result<(), String> {
    let _operation = state.operation.try_lock().map_err(|_| "update_busy")?;
    let mut update = state.pending.lock().unwrap().clone().ok_or("update_unavailable")?;
    update.timeout = Some(Duration::from_secs(15 * 60));
    state.machine.lock().unwrap().progress(0, None);
    state.publish(&app);
    let mut received = 0u64;
    let bytes = update.download(|chunk, total| {
        received = received.saturating_add(chunk as u64);
        if state.machine.lock().unwrap().progress(received, total) { state.publish(&app); }
    }, || {}).await.map_err(|_| state.fail(&app, "download_failed"))?;
    // download() has verified the signature. Never mark unverified bytes ready.
    state.machine.lock().unwrap().ready();
    state.publish(&app);
    tauri::async_runtime::spawn_blocking(move || update.install(bytes))
        .await.map_err(|_| state.fail(&app, "install_failed"))?
        .map_err(|_| state.fail(&app, "install_failed"))?;
    // On Windows the installer exits this process and relaunches the new version.
    // Other platforms return from install and use the normal shutdown path.
    app.request_restart();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn check_download_ready_and_error_states() {
        let mut machine = Machine::new("0.2.0".into());
        assert_eq!(machine.snapshot.status, UpdateState::Idle);
        machine.begin_check();
        assert_eq!(machine.snapshot.status, UpdateState::Checking);
        machine.checked(Some(("0.3.0".into(), Some("Release notes".into()))), 42);
        assert_eq!(machine.snapshot.last_checked_ms, Some(42));
        assert_eq!(machine.snapshot.status, UpdateState::Available { version: "0.3.0".into(), notes: Some("Release notes".into()) });
        assert!(machine.progress(50, Some(100)));
        assert_eq!(machine.snapshot.status, UpdateState::Downloading { percent: 50 });
        assert!(!machine.progress(50, Some(100)));
        machine.progress(u64::MAX, Some(1));
        assert_eq!(machine.snapshot.status, UpdateState::Downloading { percent: 99 });
        machine.ready();
        assert_eq!(machine.snapshot.status, UpdateState::Ready);
        for code in ["check_failed", "download_failed", "install_failed"] {
            machine.fail(code);
            assert_eq!(machine.snapshot.status, UpdateState::Error { code });
        }
        machine.begin_check();
        machine.checked(None, 43);
        assert_eq!(machine.snapshot.status, UpdateState::Idle);
        assert_eq!(machine.snapshot.last_checked_ms, Some(43));
    }

    #[test]
    fn unknown_download_length_does_not_report_completion() {
        let mut machine = Machine::new("0.2.0".into());
        for total in [None, Some(0)] {
            machine.progress(100, total);
            assert_eq!(machine.snapshot.status, UpdateState::Downloading { percent: 0 });
        }
    }

    #[test]
    fn notifies_once_per_version_only_when_hidden() {
        let mut machine = Machine::new("0.2.0".into());
        assert!(!machine.should_notify("0.3.0", false));
        assert!(machine.should_notify("0.3.0", true));
        machine.begin_check();
        machine.fail("check_failed");
        assert!(!machine.should_notify("0.3.0", true));
        assert!(machine.should_notify("0.4.0", true));
        assert!(!machine.should_notify("0.3.0", true));
    }

    #[test]
    fn notification_history_survives_restart() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("update-notifications.json");
        let mut machine = Machine::new("0.2.0".into());
        assert!(machine.should_notify("0.3.0", true));
        write_notified(&path, &machine.notified);
        let mut restarted = Machine::new("0.2.0".into());
        restarted.notified = read_notified(&path);
        assert!(!restarted.should_notify("0.3.0", true));
        assert!(restarted.should_notify("0.4.0", true));
    }

    #[test]
    fn serialized_state_is_the_webview_contract() {
        assert_eq!(serde_json::to_value(UpdateState::Downloading { percent: 42 }).unwrap(), serde_json::json!({"state": "downloading", "percent": 42}));
        assert_eq!(update_label(Lang::Tr, "0.3.0"), "Güncelleme var (v0.3.0)");
        assert_eq!(update_label(Lang::En, "0.3.0"), "Update available (v0.3.0)");
    }
}
