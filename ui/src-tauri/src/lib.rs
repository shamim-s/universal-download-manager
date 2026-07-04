// UDM desktop shell (Phase 8/10): bundles the daemon as a sidecar, system tray,
// and close-to-tray behavior.
//
// On startup we spawn the `udm-daemon` sidecar so the app is self-contained —
// no separate process to manage. Spawning is best-effort: if the port is
// already bound (e.g. a standalone daemon is running) the sidecar exits and the
// UI simply connects to the existing one. The child is killed when the app
// actually exits (tray → Quit), not when the window is hidden to the tray.

use std::collections::HashMap;
use std::sync::Mutex;

use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    Emitter, Manager, RunEvent,
};
use tauri_plugin_shell::process::{CommandChild, CommandEvent};
use tauri_plugin_shell::ShellExt;

/// Holds the running daemon child process so we can kill it on exit.
struct DaemonProc(Mutex<Option<CommandChild>>);

/// Short-lived store handing a "New Download" intent from the main window to the
/// popup window it spawns. Keyed by a caller-generated id so a payload (which can
/// carry a large cookie header) never has to travel through a URL.
#[derive(Default)]
struct IntentStash(Mutex<HashMap<String, serde_json::Value>>);

/// Stash a download intent for a soon-to-open New Download window to claim.
#[tauri::command]
fn stash_intent(stash: tauri::State<IntentStash>, id: String, intent: serde_json::Value) {
    stash.0.lock().unwrap().insert(id, intent);
}

/// Claim (and remove) a previously stashed download intent by id.
#[tauri::command]
fn take_intent(stash: tauri::State<IntentStash>, id: String) -> Option<serde_json::Value> {
    stash.0.lock().unwrap().remove(&id)
}

fn show_main(app: &tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

/// Launch the bundled daemon sidecar and keep its handle for shutdown.
fn spawn_daemon(app: &tauri::AppHandle) {
    let command = match app.shell().sidecar("udm-daemon") {
        Ok(cmd) => cmd,
        Err(e) => {
            // No bundled binary (e.g. `tauri dev` without prepare:sidecar) — the
            // user can still run the daemon manually.
            tracing_eprintln(format!("daemon sidecar unavailable: {e}"));
            return;
        }
    };
    match command.spawn() {
        Ok((mut rx, child)) => {
            app.state::<DaemonProc>().0.lock().unwrap().replace(child);
            // Drain the event stream so the pipe never fills; surface daemon logs.
            tauri::async_runtime::spawn(async move {
                while let Some(event) = rx.recv().await {
                    match event {
                        CommandEvent::Stdout(line) | CommandEvent::Stderr(line) => {
                            tracing_eprintln(format!("[daemon] {}", String::from_utf8_lossy(&line).trim_end()));
                        }
                        CommandEvent::Error(err) => tracing_eprintln(format!("[daemon] error: {err}")),
                        CommandEvent::Terminated(payload) => {
                            tracing_eprintln(format!("[daemon] exited: {:?}", payload.code));
                            break;
                        }
                        _ => {}
                    }
                }
            });
        }
        Err(e) => tracing_eprintln(format!("failed to start daemon sidecar: {e}")),
    }
}

/// Minimal stderr logger (the UI shell has no tracing subscriber configured).
fn tracing_eprintln(msg: String) {
    eprintln!("{msg}");
}

// --- Autostart (start with Windows/login) -------------------------------------

/// Enable or disable launching UDM at login.
#[tauri::command]
fn set_autostart(app: tauri::AppHandle, enable: bool) -> Result<(), String> {
    use tauri_plugin_autostart::ManagerExt;
    let manager = app.autolaunch();
    if enable {
        manager.enable()
    } else {
        manager.disable()
    }
    .map_err(|e| e.to_string())
}

/// Whether UDM is currently set to launch at login.
#[tauri::command]
fn get_autostart(app: tauri::AppHandle) -> Result<bool, String> {
    use tauri_plugin_autostart::ManagerExt;
    app.autolaunch().is_enabled().map_err(|e| e.to_string())
}

// --- Auto-update --------------------------------------------------------------

/// Check for an update and, if one is available, download + install it. Returns
/// a human-readable status. Inert until `plugins.updater` (endpoints + pubkey)
/// is configured — see docs/RELEASE_CHECKLIST.md §5.
#[tauri::command]
async fn check_for_updates(app: tauri::AppHandle) -> Result<String, String> {
    use tauri_plugin_updater::UpdaterExt;
    let updater = app
        .updater()
        .map_err(|e| format!("Updater not configured yet: {e}"))?;
    match updater.check().await {
        Ok(Some(update)) => {
            let version = update.version.clone();
            update
                .download_and_install(|_downloaded, _total| {}, || {})
                .await
                .map_err(|e| format!("Update download failed: {e}"))?;
            Ok(format!("Updated to {version}. Restart UDM to finish."))
        }
        Ok(None) => Ok("You're on the latest version.".to_string()),
        Err(e) => Err(format!("Update check failed: {e}")),
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .invoke_handler(tauri::generate_handler![
            set_autostart,
            get_autostart,
            check_for_updates,
            stash_intent,
            take_intent
        ])
        .manage(DaemonProc(Mutex::new(None)))
        .manage(IntentStash::default())
        .setup(|app| {
            spawn_daemon(app.handle());

            let show = MenuItem::with_id(app, "show", "Show UDM", true, None::<&str>)?;
            let pause = MenuItem::with_id(app, "pause_all", "Pause all downloads", true, None::<&str>)?;
            let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&show, &pause, &quit])?;

            TrayIconBuilder::with_id("udm-tray")
                .icon(app.default_window_icon().unwrap().clone())
                .tooltip("UDM — Universal Download Manager")
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "show" => show_main(app),
                    "pause_all" => {
                        let _ = app.emit("tray-pause-all", ());
                    }
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
            Ok(())
        })
        .on_window_event(|window, event| {
            // Close-to-tray: hide instead of exiting so downloads keep running.
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                let _ = window.hide();
                api.prevent_close();
            }
        })
        .build(tauri::generate_context!())
        .expect("error while building UDM");

    app.run(|app_handle, event| {
        // When the app truly exits (tray → Quit), stop the bundled daemon too.
        if let RunEvent::Exit = event {
            if let Some(child) = app_handle.state::<DaemonProc>().0.lock().unwrap().take() {
                let _ = child.kill();
            }
        }
    });
}
