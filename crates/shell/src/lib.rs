//! Tray, settings window, single instance, autostart, desktop attach, and IPC.
//!
//! Closing settings hides the window. Quit is the tray command that exits.
//! Wallpaper and monitor assignments persist across restarts.

mod desktop;
mod ipc;
mod lifecycle;
mod platform;
mod tray;

use tauri::{Manager, WindowEvent};

use ipc::{load_store, resolve_config_path};
use lifecycle::hides_on_close;

#[derive(Debug)]
struct ShellError(String);

impl std::fmt::Display for ShellError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for ShellError {}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let result = tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if lifecycle::on_second_instance() == lifecycle::TrayEffect::ShowSettings {
                tray::present_settings(app);
            }
        }))
        .invoke_handler(tauri::generate_handler![
            ipc::get_config,
            ipc::list_monitors,
            ipc::get_state,
            ipc::set_monitor_wallpaper,
            ipc::set_monitor_fit,
        ])
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                if hides_on_close() {
                    api.prevent_close();
                    tray::hide_settings(window);
                }
            }
        })
        .setup(|app| {
            let path = resolve_config_path(app.handle())?;
            let store = load_store(path)?;
            app.manage(store);
            tray::install(app.handle())?;
            platform::enable_current_user_autostart()?;
            platform::attach_desktop(app.handle())?;
            // Verification / scripting hook: apply a bundled sample through the
            // same persist path as `set_monitor_wallpaper`, then continue.
            if let Err(error) = ipc::apply_bootstrap_sample(app.handle()) {
                eprintln!("bootstrap wallpaper failed: {error}");
            }
            Ok(())
        })
        .build(tauri::generate_context!())
        .map(|app| {
            app.run(|_app, event| match event {
                tauri::RunEvent::ExitRequested { api, code, .. } => {
                    if code != Some(tauri::RESTART_EXIT_CODE) && !lifecycle::quit_requested() {
                        api.prevent_exit();
                    }
                }
                tauri::RunEvent::Exit => platform::detach_desktop(),
                _ => {}
            });
        });
    if let Err(error) = result {
        eprintln!("failed to start: {error}");
        std::process::exit(1);
    }
}
