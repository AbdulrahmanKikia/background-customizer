//! Taskbar removal, current-user login autostart, and the desktop wallpaper host.
//! Unsafe Win32 stays in `win32`.

use std::collections::HashMap;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use auto_launch::{AutoLaunch, AutoLaunchBuilder, WindowsEnableMode};
use core_lib::{
    diff_monitors, BundledSample, Config, Monitor, MonitorChange, MonitorId, MonitorSource,
    WallpaperSource,
};
use decoder::{decode_bytes, decode_path, RgbaImage};
use renderer::{PresentCounter, StaticImageSurface};
use tauri::{AppHandle, Manager, Runtime};

use crate::desktop::{DesktopHost, DesktopSurface};
use crate::ipc::{emit_monitors_changed, AppStore};
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
    wallpaper: WallpaperSource,
}

#[cfg(windows)]
struct WindowsDesktop {
    monitors: Vec<Monitor>,
    slots: HashMap<String, MonitorSlot>,
    presents: Arc<PresentCounter>,
    samples: [RgbaImage; 2],
    config: Arc<Mutex<Config>>,
    config_path: std::path::PathBuf,
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
        self.sync_assignments(monitors)?;
        // Heal layers destroyed when Windows reshuffles the desktop shell.
        let dead: Vec<String> = self
            .slots
            .iter()
            .filter(|(_, slot)| !win32::layer_is_alive(slot.layer.hwnd))
            .map(|(id, _)| id.clone())
            .collect();
        for id in dead {
            self.remove_slot(&id)?;
        }
        let changes = diff_monitors(&self.monitors, monitors);
        for change in changes {
            match change {
                MonitorChange::Removed(id) => self.remove_slot(id.as_str())?,
                MonitorChange::Added(monitor) => self.add_slot(&monitor)?,
                MonitorChange::Changed(monitor) => {
                    self.remove_slot(monitor.id().as_str())?;
                    self.add_slot(&monitor)?;
                }
            }
        }
        // Re-attach any monitor that lost its layer but did not change geometry.
        for monitor in monitors {
            if !self.slots.contains_key(monitor.id().as_str()) {
                self.add_slot(monitor)?;
            }
        }
        self.monitors = monitors.to_vec();
        write_monitor_stats(&self.monitors, &self.slots);
        Ok(())
    }

    fn sync_assignments(&mut self, monitors: &[Monitor]) -> Result<(), String> {
        let mut config = self
            .config
            .lock()
            .map_err(|_| "config lock poisoned".to_string())?;
        let changed = config
            .ensure_monitor_assignments(monitors)
            .map_err(|error| error.to_string())?;
        if changed {
            config
                .save_json_sync(&self.config_path)
                .map_err(|error| error.to_string())?;
            write_config_applied(&self.config_path, &config);
        }
        Ok(())
    }

    fn wallpaper_for(&self, monitor_id: &MonitorId) -> Result<WallpaperSource, String> {
        let config = self
            .config
            .lock()
            .map_err(|_| "config lock poisoned".to_string())?;
        config
            .wallpaper_for(monitor_id)
            .cloned()
            .ok_or_else(|| format!("no wallpaper assignment for {}", monitor_id.as_str()))
    }

    fn add_slot(&mut self, monitor: &Monitor) -> Result<(), String> {
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
        let wallpaper = self.wallpaper_for(monitor.id())?;
        let image = resolve_image(&wallpaper, &self.samples)?;
        let mut gpu = StaticImageSurface::attach(
            layer.hwnd,
            layer.width,
            layer.height,
            Arc::clone(&self.presents),
        )?;
        gpu.present_static(&image)?;
        self.slots.insert(
            monitor.id().as_str().to_string(),
            MonitorSlot {
                layer,
                gpu,
                wallpaper,
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

    fn apply_wallpaper(
        &mut self,
        monitor_id: &str,
        wallpaper: &WallpaperSource,
    ) -> Result<(), String> {
        if self
            .slots
            .get(monitor_id)
            .is_some_and(|slot| !win32::layer_is_alive(slot.layer.hwnd))
        {
            self.recreate_slot(monitor_id)?;
        }
        let image = resolve_image(wallpaper, &self.samples)?;
        let present_result = {
            let Some(slot) = self.slots.get_mut(monitor_id) else {
                return Ok(());
            };
            slot.gpu.present_static(&image)
        };
        if let Err(error) = present_result {
            // Surface lost after another app took the GPU / DWM reshuffled.
            self.recreate_slot(monitor_id)?;
            let slot = self
                .slots
                .get_mut(monitor_id)
                .ok_or_else(|| format!("slot missing after recreate: {error}"))?;
            let image = resolve_image(wallpaper, &self.samples)?;
            slot.gpu.present_static(&image)?;
        }
        if let Some(slot) = self.slots.get_mut(monitor_id) {
            slot.wallpaper = wallpaper.clone();
        }
        write_monitor_stats(&self.monitors, &self.slots);
        Ok(())
    }

    fn recreate_slot(&mut self, monitor_id: &str) -> Result<(), String> {
        let monitor = self
            .monitors
            .iter()
            .find(|monitor| monitor.id().as_str() == monitor_id)
            .cloned();
        self.remove_slot(monitor_id)?;
        if let Some(monitor) = monitor {
            self.add_slot(&monitor)?;
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

fn resolve_image(
    wallpaper: &WallpaperSource,
    samples: &[RgbaImage; 2],
) -> Result<RgbaImage, String> {
    match wallpaper {
        WallpaperSource::Bundled {
            sample: BundledSample::A,
        } => Ok(samples[0].clone()),
        WallpaperSource::Bundled {
            sample: BundledSample::B,
        } => Ok(samples[1].clone()),
        WallpaperSource::Path { path } => decode_path(Path::new(path)),
    }
}

pub fn current_monitors() -> Result<Vec<Monitor>, String> {
    #[cfg(windows)]
    {
        WindowsMonitorSource
            .monitors()
            .map_err(|error| error.to_string())
    }
    #[cfg(not(windows))]
    {
        Err("monitor enumeration is only implemented on Windows".to_string())
    }
}

pub fn apply_monitor_wallpaper(
    monitor_id: &str,
    wallpaper: &WallpaperSource,
) -> Result<(), String> {
    #[cfg(windows)]
    {
        let mut host = DESKTOP
            .lock()
            .map_err(|_| "desktop host lock poisoned".to_string())?;
        let Some(desktop) = host.as_mut() else {
            return Ok(());
        };
        desktop.apply_wallpaper(monitor_id, wallpaper)
    }
    #[cfg(not(windows))]
    {
        let _ = (monitor_id, wallpaper);
        Ok(())
    }
}

/// Apply on the UI thread. Tauri commands and hotplug workers should use this
/// so wgpu/HWND work does not race the event loop.
pub fn apply_monitor_wallpaper_on_main<R: Runtime>(
    app: &AppHandle<R>,
    monitor_id: String,
    wallpaper: WallpaperSource,
) -> Result<(), String> {
    let (tx, rx) = std::sync::mpsc::channel();
    app.run_on_main_thread(move || {
        let result = apply_monitor_wallpaper(&monitor_id, &wallpaper);
        let _ = tx.send(result);
    })
    .map_err(|error| format!("queue wallpaper apply: {error}"))?;
    rx.recv()
        .map_err(|error| format!("wallpaper apply worker: {error}"))?
}

pub fn attach_desktop<R: Runtime>(app: &AppHandle<R>) -> Result<(), ShellError> {
    #[cfg(windows)]
    {
        let store = app
            .try_state::<AppStore>()
            .ok_or_else(|| ShellError("app store is missing".to_string()))?;
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
            config: Arc::clone(&store.config),
            config_path: store.path.clone(),
        };
        let monitors = WindowsMonitorSource
            .monitors()
            .map_err(|error| ShellError(error.to_string()))?;
        desktop.attach_monitors(&monitors).map_err(ShellError)?;
        {
            let config = store
                .config
                .lock()
                .map_err(|_| ShellError("config lock poisoned".to_string()))?;
            write_config_applied(&store.path, &config);
        }
        write_present_stats(&presents, "after_first");
        schedule_idle_present_check(Arc::clone(&presents));
        schedule_hotplug_resync(app.clone());
        // Exercise the hotplug path once so a single-monitor machine still
        // proves resync does not crash.
        desktop.apply_layout(&monitors).map_err(ShellError)?;
        *host = Some(desktop);
        Ok(())
    }
    #[cfg(not(windows))]
    {
        let _ = app;
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

fn schedule_hotplug_resync<R: Runtime>(app: AppHandle<R>) {
    #[cfg(windows)]
    {
        std::thread::spawn(move || loop {
            std::thread::sleep(Duration::from_millis(750));
            if !win32::take_display_changed() {
                continue;
            }
            let path = std::env::temp_dir().join("background-customizer-hotplug.txt");
            let _ = std::fs::write(&path, "resync_begin\n");
            // Rebuild wallpaper surfaces on the UI thread — creating/destroying
            // wgpu HWND surfaces from this worker crashes when Windows reshuffles
            // the desktop (fullscreen apps, show-desktop, shell refresh).
            let app_for_job = app.clone();
            let (tx, rx) = std::sync::mpsc::channel();
            if let Err(error) = app_for_job.run_on_main_thread(move || {
                let result = resync_desktop_layout();
                let _ = tx.send(result);
            }) {
                let _ = std::fs::write(&path, format!("queue_failed:{error}\n"));
                continue;
            }
            match rx.recv() {
                Ok(Ok(monitors)) => {
                    let _ = std::fs::write(&path, format!("resync_ok:{}\n", monitors.len()));
                    emit_monitors_changed(&app, &monitors);
                }
                Ok(Err(error)) => {
                    let _ = std::fs::write(&path, format!("resync_err:{error}\n"));
                    eprintln!("monitor resync failed: {error}");
                }
                Err(error) => {
                    let _ = std::fs::write(&path, format!("resync_recv:{error}\n"));
                }
            }
        });
    }
    #[cfg(not(windows))]
    {
        let _ = app;
    }
}

#[cfg(windows)]
fn resync_desktop_layout() -> Result<Vec<Monitor>, String> {
    let mut host = DESKTOP
        .lock()
        .map_err(|_| "desktop host lock poisoned".to_string())?;
    let Some(desktop) = host.as_mut() else {
        return Err("desktop host is not attached".to_string());
    };
    let monitors = WindowsMonitorSource
        .monitors()
        .map_err(|error| error.to_string())?;
    desktop.apply_layout(&monitors)?;
    Ok(monitors)
}

fn write_present_stats(presents: &PresentCounter, label: &str) {
    let path = std::env::temp_dir().join("background-customizer-presents.txt");
    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) {
        let _ = writeln!(file, "{label} frames={}", presents.frames_submitted());
    }
}

fn write_config_applied(path: &Path, config: &Config) {
    let stats = std::env::temp_dir().join("background-customizer-config.txt");
    if let Ok(mut file) = OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(stats)
    {
        let _ = writeln!(file, "path={}", path.display());
        let _ = writeln!(
            file,
            "active_profile={}",
            config.active_profile().unwrap_or("")
        );
        if let Some(profile) = config.active_profile_data() {
            for assignment in profile.monitors() {
                let wallpaper = match assignment.wallpaper() {
                    WallpaperSource::Bundled { sample } => {
                        format!("bundled:{}", sample.as_str())
                    }
                    WallpaperSource::Path { path } => format!("path:{path}"),
                };
                let _ = writeln!(
                    file,
                    "monitor={} fit={:?} wallpaper={}",
                    assignment.monitor_id().as_str(),
                    assignment.fit(),
                    wallpaper
                );
            }
        }
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
                .map(|slot| match &slot.wallpaper {
                    WallpaperSource::Bundled { sample } => sample.as_str().to_uppercase(),
                    WallpaperSource::Path { .. } => "PATH".to_string(),
                })
                .unwrap_or_else(|| "?".to_string());
            let _ = writeln!(
                file,
                "id={} primary={} {}x{} @{},{} sample={}",
                monitor.id().as_str(),
                monitor.primary(),
                monitor.width_px(),
                monitor.height_px(),
                monitor.left_px(),
                monitor.top_px(),
                sample
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
