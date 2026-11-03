//! Taskbar removal, current-user login autostart, and the desktop wallpaper host.
//! Unsafe Win32 stays in `win32`.

use std::collections::HashMap;
use std::fs::OpenOptions;
use std::io::Write;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use auto_launch::{AutoLaunch, AutoLaunchBuilder, WindowsEnableMode};
use core_lib::{diff_monitors, Monitor, MonitorChange, MonitorId, MonitorSource};
use decoder::{decode_bytes, RgbaImage};
use renderer::{PresentCounter, StaticImageSurface};

use crate::desktop::{sample_index_for_monitor, DesktopHost, DesktopSurface};
use crate::lifecycle::{ensure_login_autostart, LoginAutostart, TaskbarPresence};
use crate::ShellError;

#[cfg(windows)]
mod win32;

/// Bundled verification wallpapers. See `assets/README.md`.
const SAMPLE_WALLPAPER_A_PNG: &[u8] = include_bytes!("../../../../assets/sample-wallpaper.png");
const SAMPLE_WALLPAPER_B_PNG: &[u8] = include_bytes!("../../../../assets/sample-wallpaper-b.png");

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

/// Wallpaper surfaces live until the process exits.
#[cfg(windows)]
static DESKTOP: Mutex<Option<WindowsDesktop>> = Mutex::new(None);

#[cfg(windows)]
struct MonitorSlot {
    layer: DesktopSurface,
    gpu: StaticImageSurface,
    sample_index: usize,
}

#[cfg(windows)]
struct WindowsDesktop {
    monitors: Vec<Monitor>,
    slots: HashMap<String, MonitorSlot>,
    presents: Arc<PresentCounter>,
    samples: [RgbaImage; 2],
}

#[cfg(windows)]
impl DesktopHost for WindowsDesktop {
    fn attach_monitors(&mut self, monitors: &[Monitor]) -> Result<Vec<DesktopSurface>, String> {
        self.apply_layout(monitors)?;
        Ok(self.slots.values().map(|slot| slot.layer.clone()).collect())
    }

    fn detach_all(&mut self) -> Result<(), String> {
        let ids: Vec<String> = self.slots.keys().cloned().collect();
        for id in ids {
            self.remove_slot(&id)?;
        }
        self.monitors.clear();
        Ok(())
    }
}

#[cfg(windows)]
impl WindowsDesktop {
    fn apply_layout(&mut self, monitors: &[Monitor]) -> Result<(), String> {
        let changes = diff_monitors(&self.monitors, monitors);
        for change in changes {
            match change {
                MonitorChange::Removed(id) => self.remove_slot(id.as_str())?,
                MonitorChange::Added(monitor) => self.add_slot(&monitor, monitors)?,
                MonitorChange::Changed(monitor) => {
                    self.remove_slot(monitor.id().as_str())?;
                    self.add_slot(&monitor, monitors)?;
                }
            }
        }
        self.monitors = monitors.to_vec();
        write_monitor_stats(&self.monitors, &self.slots);
        Ok(())
    }

    fn add_slot(&mut self, monitor: &Monitor, layout: &[Monitor]) -> Result<(), String> {
        let native = win32::NativeMonitor {
            id: monitor.id().as_str().to_string(),
            name: monitor.name().to_string(),
            left: monitor.left_px(),
            top: monitor.top_px(),
            width: monitor.width_px(),
            height: monitor.height_px(),
            primary: monitor.primary(),
        };
        let layer = win32::attach_monitor_layer(&native)?;
        let sample_index = sample_index_for_monitor(layout, monitor.id());
        let mut gpu = StaticImageSurface::attach(
            layer.hwnd,
            layer.width,
            layer.height,
            Arc::clone(&self.presents),
        )?;
        gpu.present_static(&self.samples[sample_index])?;
        self.slots.insert(
            monitor.id().as_str().to_string(),
            MonitorSlot {
                layer,
                gpu,
                sample_index,
            },
        );
        Ok(())
    }

    fn remove_slot(&mut self, id: &str) -> Result<(), String> {
        if let Some(slot) = self.slots.remove(id) {
            drop(slot.gpu);
            win32::destroy_layer(slot.layer.hwnd)?;
        }
        Ok(())
    }
}

#[cfg(windows)]
impl Drop for WindowsDesktop {
    fn drop(&mut self) {
        let _ = self.detach_all();
    }
}

#[cfg(windows)]
struct WindowsMonitorSource;

#[cfg(windows)]
impl MonitorSource for WindowsMonitorSource {
    fn monitors(&self) -> Result<Vec<Monitor>, core_lib::CoreError> {
        let native = win32::list_monitors().map_err(core_lib::CoreError::Invalid)?;
        native
            .into_iter()
            .map(|monitor| {
                let name = if monitor.name.is_empty() {
                    monitor.id.clone()
                } else {
                    monitor.name.clone()
                };
                Monitor::placed(
                    MonitorId::new(monitor.id)?,
                    name,
                    monitor.width,
                    monitor.height,
                    monitor.left,
                    monitor.top,
                    monitor.primary,
                )
            })
            .collect()
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
        win32::start_display_change_watcher().map_err(ShellError)?;
        let presents = Arc::new(PresentCounter::new());
        let samples = [
            decode_bytes(SAMPLE_WALLPAPER_A_PNG).map_err(ShellError)?,
            decode_bytes(SAMPLE_WALLPAPER_B_PNG).map_err(ShellError)?,
        ];
        let mut desktop = WindowsDesktop {
            monitors: Vec::new(),
            slots: HashMap::new(),
            presents: Arc::clone(&presents),
            samples,
        };
        let monitors = WindowsMonitorSource
            .monitors()
            .map_err(|error| ShellError(error.to_string()))?;
        desktop.attach_monitors(&monitors).map_err(ShellError)?;
        write_present_stats(&presents, "after_first");
        schedule_idle_present_check(Arc::clone(&presents));
        schedule_hotplug_resync();
        // Exercise the hotplug path once so a single-monitor machine still
        // proves resync does not crash.
        desktop.apply_layout(&monitors).map_err(ShellError)?;
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

fn schedule_hotplug_resync() {
    #[cfg(windows)]
    {
        std::thread::spawn(|| loop {
            std::thread::sleep(Duration::from_millis(750));
            if !win32::take_display_changed() {
                continue;
            }
            let mut host = match DESKTOP.lock() {
                Ok(guard) => guard,
                Err(poisoned) => poisoned.into_inner(),
            };
            let Some(desktop) = host.as_mut() else {
                break;
            };
            match WindowsMonitorSource.monitors() {
                Ok(monitors) => {
                    if let Err(error) = desktop.apply_layout(&monitors) {
                        eprintln!("monitor resync failed: {error}");
                    }
                }
                Err(error) => eprintln!("monitor enumeration failed: {error}"),
            }
        });
    }
}

fn write_present_stats(presents: &PresentCounter, label: &str) {
    let path = std::env::temp_dir().join("background-customizer-presents.txt");
    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) {
        let _ = writeln!(file, "{label} frames={}", presents.frames_submitted());
    }
}

#[cfg(windows)]
fn write_monitor_stats(monitors: &[Monitor], slots: &HashMap<String, MonitorSlot>) {
    let path = std::env::temp_dir().join("background-customizer-monitors.txt");
    if let Ok(mut file) = OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(path)
    {
        let _ = writeln!(file, "count={}", monitors.len());
        for monitor in monitors {
            let sample = slots
                .get(monitor.id().as_str())
                .map(|slot| slot.sample_index)
                .unwrap_or(0);
            let _ = writeln!(
                file,
                "id={} primary={} {}x{} @{},{} sample={}",
                monitor.id().as_str(),
                monitor.primary(),
                monitor.width_px(),
                monitor.height_px(),
                monitor.left_px(),
                monitor.top_px(),
                if sample == 0 { "A" } else { "B" }
            );
        }
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

#[cfg(test)]
mod tests {
    use core_lib::{diff_monitors, Monitor, MonitorChange, MonitorId};

    #[test]
    fn hotplug_diff_is_available_to_the_shell() {
        let before =
            vec![
                Monitor::placed(MonitorId::new("a").expect("id"), "a", 800, 600, 0, 0, true)
                    .expect("monitor"),
            ];
        let after = vec![
            Monitor::placed(MonitorId::new("a").expect("id"), "a", 1024, 768, 0, 0, true)
                .expect("monitor"),
            Monitor::placed(
                MonitorId::new("b").expect("id"),
                "b",
                800,
                600,
                1024,
                0,
                false,
            )
            .expect("monitor"),
        ];
        let changes = diff_monitors(&before, &after);
        assert!(changes
            .iter()
            .any(|change| matches!(change, MonitorChange::Changed(_))));
        assert!(changes
            .iter()
            .any(|change| matches!(change, MonitorChange::Added(_))));
    }
}
