//! Settings window for Background Customizer.
//!
//! Milestone 0 opens that window and nothing else. The tray is a placeholder:
//! there is no Pause/Resume menu, no WorkerW attach, and no wallpaper surface.

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    if let Err(error) = tauri::Builder::default().run(tauri::generate_context!()) {
        eprintln!("failed to open settings window: {error}");
        std::process::exit(1);
    }
}
