//! Taskbar removal and current-user login autostart.
//! The Win32 taskbar call lives in `win32`.

use auto_launch::{AutoLaunch, AutoLaunchBuilder, WindowsEnableMode};

use crate::lifecycle::{ensure_login_autostart, LoginAutostart, TaskbarPresence};
use crate::ShellError;

#[cfg(windows)]
mod win32;

pub struct NativeTaskbar {
    hwnd: isize,
}

impl NativeTaskbar {
    pub fn new(hwnd: isize) -> Self {
        Self { hwnd }
    }
}

impl TaskbarPresence for NativeTaskbar {
    fn omit_from_taskbar(&mut self) -> Result<(), String> {
        omit_from_taskbar(self.hwnd)
    }
}

pub fn enable_current_user_autostart() -> Result<(), ShellError> {
    ensure_login_autostart(&CurrentUserAutostart).map_err(ShellError)
}

struct CurrentUserAutostart;

impl LoginAutostart for CurrentUserAutostart {
    fn enable(&self) -> Result<(), String> {
        current_user_launch()?
            .enable()
            .map_err(|error| error.to_string())
    }

    fn is_enabled(&self) -> Result<bool, String> {
        current_user_launch()?
            .is_enabled()
            .map_err(|error| error.to_string())
    }
}

fn current_user_launch() -> Result<AutoLaunch, String> {
    let executable = std::env::current_exe().map_err(|error| error.to_string())?;
    let mut builder = AutoLaunchBuilder::new();
    builder.set_app_name("Background Customizer");
    builder.set_app_path(&executable.display().to_string());
    builder.set_windows_enable_mode(WindowsEnableMode::CurrentUser);
    builder.build().map_err(|error| error.to_string())
}

fn omit_from_taskbar(hwnd: isize) -> Result<(), String> {
    #[cfg(windows)]
    {
        win32::omit_from_taskbar(hwnd)
    }
    #[cfg(not(windows))]
    {
        let _ = hwnd;
        Ok(())
    }
}
