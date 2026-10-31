//! Tray, settings window, single instance, and login autostart.
//!
//! Closing settings hides the window. Quit is the tray command that exits.
//! There is no wallpaper surface and no WorkerW attach.

mod lifecycle;
mod platform;
mod tray;

use tauri::WindowEvent;

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
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                if hides_on_close() {
                    api.prevent_close();
                    tray::hide_settings(window);
                }
            }
        })
        .setup(|app| {
            tray::install(app.handle())?;
            platform::enable_current_user_autostart()?;
            Ok(())
        })
        .build(tauri::generate_context!())
        .map(|app| {
            app.run(|_app, event| {
                if let tauri::RunEvent::ExitRequested { api, code, .. } = event {
                    if code != Some(tauri::RESTART_EXIT_CODE) && !lifecycle::quit_requested() {
                        api.prevent_exit();
                    }
                }
            });
        });
    if let Err(error) = result {
        eprintln!("failed to start: {error}");
        std::process::exit(1);
    }
}
