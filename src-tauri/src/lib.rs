pub mod commands;
pub mod config;
pub mod model;
pub mod presence;
pub mod printers;
pub mod runtime;
pub mod updates;

use std::sync::{atomic::Ordering, Arc};
use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, RunEvent, WindowEvent};

/// Passed by the OS autostart entry: start in the tray without showing the window.
const HIDDEN_ARG: &str = "--hidden";

struct TrayLabels {
    show: MenuItem<tauri::Wry>,
    quit: MenuItem<tauri::Wry>,
}

fn tray_labels(lang: config::Lang) -> (&'static str, &'static str) {
    match lang {
        config::Lang::Tr => ("Göster", "Tamamen çık"),
        config::Lang::En => ("Show", "Quit"),
    }
}

fn update_tray_language(app: &AppHandle, lang: config::Lang) -> tauri::Result<()> {
    let labels = app.state::<TrayLabels>();
    let (show, quit) = tray_labels(lang);
    labels.show.set_text(show)?;
    labels.quit.set_text(quit)?;
    app.state::<updates::Updates>().set_language(lang);
    Ok(())
}

fn show_main(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

/// The frontend calls once its first usable frame (including recovery UI) is ready.
#[tauri::command]
fn ui_ready(window: tauri::WebviewWindow) {
    if !std::env::args().any(|arg| arg == HIDDEN_ARG) {
        let _ = window.show();
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        // Must be registered first so a second launch only focuses the running instance.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            show_main(app)
        }))
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            Some(vec![HIDDEN_ARG]),
        ))
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_notification::init())
        .invoke_handler(tauri::generate_handler![
            ui_ready,
            commands::get_config,
            commands::save_config,
            commands::delete_printer,
            commands::get_snapshot,
            commands::import_legacy,
            commands::set_autostart,
            updates::get_update_state,
            updates::check_for_updates,
            updates::install_update,
        ])
        .setup(|app| {
            let secrets: Arc<dyn config::SecretStore> = Arc::new(config::KeyringStore);
            let settings = runtime::Settings::load(&app.path().app_config_dir()?, secrets.clone())
                .map_err(std::io::Error::other)?;
            commands::apply_autostart(app.handle(), settings.config.autostart)
                .map_err(std::io::Error::other)?;
            let (show_text, quit_text) = tray_labels(settings.config.lang);
            let show = MenuItem::with_id(app, "show", show_text, true, None::<&str>)?;
            let quit = MenuItem::with_id(app, "quit", quit_text, true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&show, &quit])?;
            let icon = app
                .default_window_icon()
                .cloned()
                .ok_or("bundled window icon missing")?;
            TrayIconBuilder::with_id("main")
                .icon(icon)
                .tooltip("3D Printing")
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "show" => show_main(app),
                    "update" => show_main(app),
                    "quit" => app.exit(0),
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        show_main(tray.app_handle());
                    }
                })
                .build(app)?;
            app.manage(TrayLabels { show, quit });
            updates::start(app.handle(), menu);
            let supervisor = runtime::Supervisor::spawn(settings.runtime_config(), secrets);
            let snapshots = supervisor.subscribe();
            let emitter = app.handle().clone();
            tauri::async_runtime::spawn(runtime::emit_snapshots(snapshots, move |snapshot| {
                // Emit only to the local main webview; no printer credentials are
                // part of this payload, and no remote webview is granted access.
                let _ = emitter.emit_to("main", "snapshot", snapshot);
            }));
            app.manage(commands::Runtime {
                settings: Arc::new(tokio::sync::Mutex::new(settings)),
                supervisor,
                exit_state: std::sync::atomic::AtomicU8::new(0),
            });
            // Safety net: if the frontend never reports ready (e.g. a load error), still
            // show the window so the app is not invisible and unreachable.
            if !std::env::args().any(|arg| arg == HIDDEN_ARG) {
                let handle = app.handle().clone();
                tauri::async_runtime::spawn(async move {
                    tokio::time::sleep(std::time::Duration::from_secs(3)).await;
                    if let Some(window) = handle.get_webview_window("main") {
                        if !window.is_visible().unwrap_or(true) {
                            let _ = window.show();
                        }
                    }
                });
            }
            Ok(())
        })
        // Closing the window keeps the service running in the tray; "Tamamen çık" exits.
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app, event| {
            if let RunEvent::ExitRequested { api, .. } = event {
                let state = app.state::<commands::Runtime>();
                if state.exit_state.load(Ordering::SeqCst) != 2 {
                    api.prevent_exit();
                    if state
                        .exit_state
                        .compare_exchange(0, 1, Ordering::SeqCst, Ordering::SeqCst)
                        .is_ok()
                    {
                        let app = app.clone();
                        tauri::async_runtime::spawn(async move {
                            let state = app.state::<commands::Runtime>();
                            // Wait for any accepted save to finish before stopping
                            // sources. Repeated Quit requests still await cleanup.
                            let _settings = state.settings.lock().await;
                            state.supervisor.shutdown().await;
                            state.exit_state.store(2, Ordering::SeqCst);
                            app.exit(0);
                        });
                    }
                }
            }
        });
}

#[cfg(test)]
mod tests {
    #[test]
    fn tray_menu_uses_configured_language() {
        assert_eq!(
            super::tray_labels(crate::config::Lang::Tr),
            ("Göster", "Tamamen çık")
        );
        assert_eq!(
            super::tray_labels(crate::config::Lang::En),
            ("Show", "Quit")
        );
    }
}
