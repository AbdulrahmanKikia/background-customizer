//! Taskbar removal, current-user login autostart, and the desktop wallpaper host.
//! Unsafe Win32 stays in `win32`.

use std::fs::OpenOptions;
use std::io::Write;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use auto_launch::{AutoLaunch, AutoLaunchBuilder, WindowsEnableMode};
use decoder::decode_bytes;
use renderer::{PresentCounter, StaticImageSurface};

use crate::desktop::{DesktopHost, DesktopSurface};
use crate::lifecycle::{ensure_login_autostart, LoginAutostart, TaskbarPresence};
use crate::ShellError;

#[cfg(windows)]
mod win32;

/// Bundled Milestone 3 verification wallpaper. See `assets/README.md`.
const SAMPLE_WALLPAPER_PNG: &[u8] = include_bytes!("../../../../assets/sample-wallpaper.png");

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

/// The wallpaper surface lives until the process exits. Dropping it here would
/// destroy the wallpaper as soon as setup returned.
#[cfg(windows)]
static DESKTOP: Mutex<Option<WindowsDesktop>> = Mutex::new(None);

#[cfg(windows)]
struct WindowsDesktop {
    layer: isize,
    width: u32,
    height: u32,
    surface: Option<StaticImageSurface>,
}

#[cfg(windows)]
impl DesktopHost for WindowsDesktop {
    fn attach_surface(&mut self) -> Result<DesktopSurface, String> {
        if self.layer != 0 {
            return Ok(DesktopSurface {
                hwnd: self.layer,
                width: self.width,
                height: self.height,
            });
        }
        let attached = win32::attach_surface_layer()?;
        self.layer = attached.hwnd;
        self.width = attached.width;
        self.height = attached.height;
        Ok(attached)
    }

    fn detach(&mut self) -> Result<(), String> {
        self.surface.take();
        if self.layer == 0 {
            return Ok(());
        }
        win32::destroy_layer(self.layer)?;
        self.layer = 0;
        self.width = 0;
        self.height = 0;
        Ok(())
    }
}

#[cfg(windows)]
impl Drop for WindowsDesktop {
    fn drop(&mut self) {
        let _ = self.detach();
    }
}

pub fn attach_desktop() -> Result<(), ShellError> {
    #[cfg(windows)]
    {
        let mut host = DESKTOP
            .lock()
            .map_err(|_| ShellError("desktop host lock poisoned".to_string()))?;
        if host.is_some() {
            return Ok(());
        }
        let presents = Arc::new(PresentCounter::new());
        let mut desktop = WindowsDesktop {
            layer: 0,
            width: 0,
            height: 0,
            surface: None,
        };
        let attached = desktop.attach_surface().map_err(ShellError)?;
        let image = decode_bytes(SAMPLE_WALLPAPER_PNG).map_err(ShellError)?;
        let mut surface = StaticImageSurface::attach(
            attached.hwnd,
            attached.width,
            attached.height,
            Arc::clone(&presents),
        )
        .map_err(ShellError)?;
        surface.present_static(&image).map_err(ShellError)?;
        write_present_stats(&presents, "after_first");
        schedule_idle_present_check(Arc::clone(&presents));
        desktop.surface = Some(surface);
        *host = Some(desktop);
        Ok(())
    }
    #[cfg(not(windows))]
    {
        Err(ShellError(
            "desktop attach is only implemented on Windows".to_string(),
        ))
    }
}

pub fn detach_desktop() {
    #[cfg(windows)]
    {
        let mut host = match DESKTOP.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        host.take();
    }
}

fn schedule_idle_present_check(presents: Arc<PresentCounter>) {
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_secs(3));
        write_present_stats(&presents, "after_idle");
    });
}

fn write_present_stats(presents: &PresentCounter, label: &str) {
    let path = std::env::temp_dir().join("background-customizer-presents.txt");
    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) {
        let _ = writeln!(file, "{label} frames={}", presents.frames_submitted());
    }
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
