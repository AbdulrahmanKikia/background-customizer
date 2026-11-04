//! Typed Tauri commands and events for config and monitor state.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use core_lib::{BundledSample, Config, FitMode, Monitor, MonitorId, WallpaperSource};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, Runtime, State};

use crate::library::{self, LibraryItem};
use crate::platform;
use crate::ShellError;

pub const EVENT_CONFIG_CHANGED: &str = "config-changed";
pub const EVENT_MONITORS_CHANGED: &str = "monitors-changed";

/// Shared persisted settings used by IPC commands and the desktop host.
pub struct AppStore {
    pub path: PathBuf,
    pub config: Arc<Mutex<Config>>,
}

impl AppStore {
    pub fn snapshot(&self) -> Result<Config, String> {
        let config = self
            .config
            .lock()
            .map_err(|_| "config lock poisoned".to_string())?;
        Ok(config.clone())
    }

    pub fn save(&self) -> Result<Config, String> {
        let config = self
            .config
            .lock()
            .map_err(|_| "config lock poisoned".to_string())?;
        config
            .save_json_sync(&self.path)
            .map_err(|error| error.to_string())?;
        Ok(config.clone())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppStateDto {
    pub config: Config,
    pub config_path: String,
    pub monitors: Vec<Monitor>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum WallpaperInput {
    Bundled { sample: BundledSample },
    Path { path: String },
}

impl WallpaperInput {
    fn into_source(self) -> Result<WallpaperSource, String> {
        match self {
            Self::Bundled { sample } => Ok(WallpaperSource::bundled(sample)),
            Self::Path { path } => WallpaperSource::path(path).map_err(|error| error.to_string()),
        }
    }
}

pub fn resolve_config_path<R: Runtime>(app: &AppHandle<R>) -> Result<PathBuf, ShellError> {
    let dir = app
        .path()
        .app_config_dir()
        .map_err(|error| ShellError(format!("config directory: {error}")))?;
    Ok(dir.join("config.json"))
}

pub fn load_store(path: PathBuf) -> Result<AppStore, ShellError> {
    let config = Config::load_or_default(&path).map_err(|error| ShellError(error.to_string()))?;
    Ok(AppStore {
        path,
        config: Arc::new(Mutex::new(config)),
    })
}

fn emit_config_changed<R: Runtime>(app: &AppHandle<R>, config: &Config) {
    if let Err(error) = app.emit(EVENT_CONFIG_CHANGED, config) {
        eprintln!("failed to emit {EVENT_CONFIG_CHANGED}: {error}");
    }
}

pub fn emit_monitors_changed<R: Runtime>(app: &AppHandle<R>, monitors: &[Monitor]) {
    if let Err(error) = app.emit(EVENT_MONITORS_CHANGED, monitors) {
        eprintln!("failed to emit {EVENT_MONITORS_CHANGED}: {error}");
    }
}

fn write_config_stats(path: &std::path::Path, config: &Config) {
    use std::fs::OpenOptions;
    use std::io::Write;

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

pub fn persist_and_publish<R: Runtime>(
    app: &AppHandle<R>,
    store: &AppStore,
) -> Result<Config, String> {
    let config = store.save()?;
    write_config_stats(&store.path, &config);
    emit_config_changed(app, &config);
    Ok(config)
}

/// If `BACKGROUND_CUSTOMIZER_SET_SAMPLE` is `a` or `b`, assign that bundled
/// sample to every connected monitor and persist. Used for restart verification.
pub fn apply_bootstrap_sample<R: Runtime>(app: &AppHandle<R>) -> Result<(), String> {
    let Ok(raw) = std::env::var("BACKGROUND_CUSTOMIZER_SET_SAMPLE") else {
        return Ok(());
    };
    let sample = match raw.trim().to_ascii_lowercase().as_str() {
        "a" => BundledSample::A,
        "b" => BundledSample::B,
        other => {
            return Err(format!(
                "BACKGROUND_CUSTOMIZER_SET_SAMPLE must be a or b, got {other}"
            ));
        }
    };
    let store = app
        .try_state::<AppStore>()
        .ok_or_else(|| "app store is missing".to_string())?;
    let monitors = platform::current_monitors()?;
    let source = WallpaperSource::bundled(sample);
    {
        let mut config = store
            .config
            .lock()
            .map_err(|_| "config lock poisoned".to_string())?;
        for monitor in &monitors {
            config
                .set_monitor_wallpaper(monitor.id(), source.clone())
                .map_err(|error| error.to_string())?;
        }
    }
    for monitor in &monitors {
        platform::apply_monitor_wallpaper_on_main(
            app,
            monitor.id().as_str().to_string(),
            source.clone(),
        )?;
    }
    let config = persist_and_publish(app, store.inner())?;
    let marker = std::env::temp_dir().join("background-customizer-bootstrap.txt");
    let _ = std::fs::write(
        marker,
        format!(
            "sample={}\nmonitors={}\n",
            sample.as_str(),
            config.profiles().len()
        ),
    );
    Ok(())
}

#[tauri::command]
pub fn get_config(store: State<'_, AppStore>) -> Result<Config, String> {
    store.snapshot()
}

#[tauri::command]
pub fn list_monitors() -> Result<Vec<Monitor>, String> {
    platform::current_monitors()
}

#[tauri::command]
pub fn get_state(store: State<'_, AppStore>) -> Result<AppStateDto, String> {
    Ok(AppStateDto {
        config: store.snapshot()?,
        config_path: store.path.display().to_string(),
        monitors: platform::current_monitors()?,
    })
}

#[tauri::command]
pub fn set_monitor_wallpaper<R: Runtime>(
    app: AppHandle<R>,
    store: State<'_, AppStore>,
    monitor_id: String,
    wallpaper: WallpaperInput,
) -> Result<Config, String> {
    let id = MonitorId::new(monitor_id).map_err(|error| error.to_string())?;
    let source = wallpaper.into_source()?;
    {
        let mut config = store
            .config
            .lock()
            .map_err(|_| "config lock poisoned".to_string())?;
        config
            .set_monitor_wallpaper(&id, source.clone())
            .map_err(|error| error.to_string())?;
    }
    platform::apply_monitor_wallpaper_on_main(&app, id.as_str().to_string(), source)?;
    persist_and_publish(&app, &store)
}

#[tauri::command]
pub fn set_monitor_fit<R: Runtime>(
    app: AppHandle<R>,
    store: State<'_, AppStore>,
    monitor_id: String,
    fit: FitMode,
) -> Result<Config, String> {
    let id = MonitorId::new(monitor_id).map_err(|error| error.to_string())?;
    {
        let mut config = store
            .config
            .lock()
            .map_err(|_| "config lock poisoned".to_string())?;
        config
            .set_monitor_fit(&id, fit)
            .map_err(|error| error.to_string())?;
    }
    persist_and_publish(&app, &store)
}

#[tauri::command]
pub fn list_library(store: State<'_, AppStore>) -> Result<Vec<LibraryItem>, String> {
    library::list_library(&store.path)
}

#[tauri::command]
pub fn import_wallpaper(store: State<'_, AppStore>, path: String) -> Result<LibraryItem, String> {
    library::import_image(&store.path, Path::new(&path))
}

#[tauri::command]
pub fn apply_library_wallpaper<R: Runtime>(
    app: AppHandle<R>,
    store: State<'_, AppStore>,
    monitor_id: String,
    library_id: String,
) -> Result<Config, String> {
    let item = library::find_library_item(&store.path, &library_id)?;
    let input = match item.wallpaper {
        WallpaperSource::Bundled { sample } => WallpaperInput::Bundled { sample },
        WallpaperSource::Path { path } => WallpaperInput::Path { path },
    };
    set_monitor_wallpaper(app, store, monitor_id, input)
}

#[cfg(test)]
mod tests {
    use super::WallpaperInput;
    use core_lib::{BundledSample, WallpaperSource};

    #[test]
    fn wallpaper_input_maps_to_core_source() {
        let bundled = WallpaperInput::Bundled {
            sample: BundledSample::B,
        }
        .into_source()
        .expect("bundled");
        assert_eq!(bundled, WallpaperSource::bundled(BundledSample::B));

        let path = WallpaperInput::Path {
            path: "C:/a.png".to_string(),
        }
        .into_source()
        .expect("path");
        assert_eq!(path, WallpaperSource::path("C:/a.png").expect("path"));
    }
}
