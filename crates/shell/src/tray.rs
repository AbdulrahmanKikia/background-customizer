//! Tray icon and the settings window it controls.

use std::sync::Mutex;

use tauri::menu::{Menu, MenuItem};
use tauri::tray::{TrayIcon, TrayIconBuilder};
use tauri::{AppHandle, Manager, Runtime, WebviewWindow};

use crate::lifecycle::{
    pause_label, ResidentState, TaskbarPresence, TrayEffect, MENU_OPEN, MENU_PAUSE, MENU_QUIT,
    SETTINGS_LABEL,
};
use crate::platform::NativeTaskbar;
use crate::ShellError;

struct TrayState<R: Runtime> {
    resident: Mutex<ResidentState>,
    pause: MenuItem<R>,
    _icon: TrayIcon<R>,
}

pub fn install<R: Runtime>(app: &AppHandle<R>) -> Result<(), ShellError> {
    let pause = MenuItem::with_id(app, MENU_PAUSE, pause_label(false), true, None::<&str>)
        .map_err(|error| ShellError(format!("failed to create the pause menu: {error}")))?;
    let open = MenuItem::with_id(app, MENU_OPEN, "Open Settings", true, None::<&str>)
        .map_err(|error| ShellError(format!("failed to create the settings menu: {error}")))?;
    let quit = MenuItem::with_id(app, MENU_QUIT, "Quit", true, None::<&str>)
        .map_err(|error| ShellError(format!("failed to create the quit menu: {error}")))?;
    let menu = Menu::with_items(app, &[&pause, &open, &quit])
        .map_err(|error| ShellError(format!("failed to create the tray menu: {error}")))?;

    let mut builder = TrayIconBuilder::with_id("resident")
        .menu(&menu)
        .show_menu_on_left_click(true)
        .tooltip("Background Customizer")
        .on_menu_event(|app, event| on_menu(app, event.id.as_ref()));
    builder = builder.icon(tray_icon(app)?);
    let icon = builder
        .build(app)
        .map_err(|error| ShellError(format!("failed to create the tray icon: {error}")))?;

    app.manage(TrayState {
        resident: Mutex::new(ResidentState::new()),
        pause,
        _icon: icon,
    });

    if let Some(window) = app.get_webview_window(SETTINGS_LABEL) {
        keep_off_taskbar(&window);
    }
    let handle = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(400));
        if let Some(window) = handle.get_webview_window(SETTINGS_LABEL) {
            keep_off_taskbar(&window);
        }
    });
    Ok(())
}

fn tray_icon<R: Runtime>(_app: &AppHandle<R>) -> Result<tauri::image::Image<'static>, ShellError> {
    #[cfg(windows)]
    {
        tauri::image::Image::from_app_icon_resource(32)
            .map_err(|error| ShellError(format!("failed to load the tray icon: {error}")))
    }
    #[cfg(not(windows))]
    {
        Err(ShellError(
            "the tray icon is loaded from the Windows application icon".to_string(),
        ))
    }
}

pub fn hide_settings<R: Runtime>(window: &tauri::Window<R>) {
    let window = window.clone();
    // Hide after the close handler returns. Doing it inside WM_CLOSE lets the
    // window close, and the hide itself puts the taskbar style back.
    std::thread::spawn(move || {
        if let Err(error) = window.hide() {
            eprintln!("failed to hide settings: {error}");
        }
        let queued = window.clone();
        if let Err(error) = window.run_on_main_thread(move || keep_off_taskbar_window(&queued)) {
            eprintln!("failed to queue the taskbar update: {error}");
        }
    });
}

pub fn present_settings<R: Runtime>(app: &AppHandle<R>) {
    let Some(window) = app.get_webview_window(SETTINGS_LABEL) else {
        eprintln!("settings window is missing");
        return;
    };
    if let Err(error) = window.show() {
        eprintln!("failed to show settings: {error}");
    }
    if let Err(error) = window.unminimize() {
        eprintln!("failed to restore settings: {error}");
    }
    keep_off_taskbar(&window);
    if let Err(error) = window.set_focus() {
        eprintln!("failed to focus settings: {error}");
    }
}

fn on_menu<R: Runtime>(app: &AppHandle<R>, id: &str) {
    let Some(command) = ResidentState::command_from_id(id) else {
        return;
    };
    let Some(tray) = app.try_state::<TrayState<R>>() else {
        eprintln!("tray state is missing");
        return;
    };
    let effect = {
        let mut resident = match tray.resident.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        let (next, effect) = resident.on_tray(command);
        *resident = next;
        effect
    };
    match effect {
        TrayEffect::PauseStub { paused } => set_pause_label(&tray.pause, paused),
        TrayEffect::ShowSettings => present_settings(app),
        TrayEffect::Quit => {
            crate::lifecycle::request_quit();
            app.exit(0);
        }
    }
}

fn set_pause_label<R: Runtime>(item: &MenuItem<R>, paused: bool) {
    let item = item.clone();
    let label = pause_label(paused).to_string();
    // Menu updates wait on the UI thread. Running that wait from the click
    // handler deadlocks the event loop, so the update happens on a worker.
    std::thread::spawn(move || {
        if let Err(error) = item.set_text(&label) {
            eprintln!("failed to update the pause menu: {error}");
        }
    });
}

fn keep_off_taskbar<R: Runtime>(window: &WebviewWindow<R>) {
    if let Err(error) = window.set_skip_taskbar(true) {
        eprintln!("failed to skip the taskbar: {error}");
    }
    #[cfg(windows)]
    omit_hwnd(window.hwnd());
}

fn keep_off_taskbar_window<R: Runtime>(window: &tauri::Window<R>) {
    if let Err(error) = window.set_skip_taskbar(true) {
        eprintln!("failed to skip the taskbar: {error}");
    }
    #[cfg(windows)]
    omit_hwnd(window.hwnd());
}

#[cfg(windows)]
fn omit_hwnd(hwnd: tauri::Result<windows::Win32::Foundation::HWND>) {
    match hwnd {
        Ok(hwnd) => {
            let mut taskbar = NativeTaskbar::new(hwnd.0 as isize);
            if let Err(error) = taskbar.omit_from_taskbar() {
                eprintln!("failed to remove the taskbar button: {error}");
            }
        }
        Err(error) => eprintln!("failed to read the settings window handle: {error}"),
    }
}
