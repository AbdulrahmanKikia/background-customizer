use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::error::CoreError;
use crate::monitor::{FitMode, Monitor, MonitorId};
use crate::profile::{MonitorAssignment, Profile};
use crate::wallpaper::{BundledSample, WallpaperSource};

pub const DEFAULT_PROFILE_NAME: &str = "default";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Config {
    profiles: Vec<Profile>,
    active_profile: Option<String>,
}

impl Config {
    pub fn empty() -> Self {
        Self {
            profiles: Vec::new(),
            active_profile: None,
        }
    }

    /// Empty config with a named active profile and no assignments yet.
    pub fn with_default_profile() -> Result<Self, CoreError> {
        let mut config = Self::empty();
        config.insert_profile(Profile::new(DEFAULT_PROFILE_NAME)?)?;
        config.set_active_profile(DEFAULT_PROFILE_NAME)?;
        Ok(config)
    }

    pub fn profiles(&self) -> &[Profile] {
        &self.profiles
    }

    pub fn active_profile(&self) -> Option<&str> {
        self.active_profile.as_deref()
    }

    pub fn active_profile_data(&self) -> Option<&Profile> {
        let name = self.active_profile.as_deref()?;
        self.profiles.iter().find(|profile| profile.name() == name)
    }

    pub fn active_profile_data_mut(&mut self) -> Option<&mut Profile> {
        let name = self.active_profile.clone()?;
        self.profiles
            .iter_mut()
            .find(|profile| profile.name() == name)
    }

    pub fn insert_profile(&mut self, profile: Profile) -> Result<(), CoreError> {
        if self
            .profiles
            .iter()
            .any(|existing| existing.name() == profile.name())
        {
            return Err(CoreError::Invalid(format!(
                "profile already exists: {}",
                profile.name()
            )));
        }
        self.profiles.push(profile);
        Ok(())
    }

    pub fn set_active_profile(&mut self, name: &str) -> Result<(), CoreError> {
        if !self.profiles.iter().any(|profile| profile.name() == name) {
            return Err(CoreError::Invalid(format!("unknown profile: {name}")));
        }
        self.active_profile = Some(name.to_string());
        Ok(())
    }

    /// Ensure every connected monitor has an assignment. New monitors get
    /// alternating bundled samples A/B. Returns true when the config changed.
    pub fn ensure_monitor_assignments(&mut self, monitors: &[Monitor]) -> Result<bool, CoreError> {
        if self.active_profile_data().is_none() {
            if self.profiles.is_empty() {
                self.insert_profile(Profile::new(DEFAULT_PROFILE_NAME)?)?;
            }
            let name = self.profiles[0].name().to_string();
            self.set_active_profile(&name)?;
        }
        let profile = self
            .active_profile_data_mut()
            .ok_or_else(|| CoreError::Invalid("active profile missing".to_string()))?;
        let mut changed = false;
        for (index, monitor) in monitors.iter().enumerate() {
            if profile.assignment(monitor.id()).is_some() {
                continue;
            }
            profile.assign(MonitorAssignment::new(
                monitor.id().clone(),
                FitMode::Fill,
                WallpaperSource::bundled(BundledSample::from_index(index)),
            ));
            changed = true;
        }
        Ok(changed)
    }

    pub fn wallpaper_for(&self, monitor_id: &MonitorId) -> Option<&WallpaperSource> {
        self.active_profile_data()?
            .assignment(monitor_id)
            .map(MonitorAssignment::wallpaper)
    }

    pub fn set_monitor_wallpaper(
        &mut self,
        monitor_id: &MonitorId,
        wallpaper: WallpaperSource,
    ) -> Result<(), CoreError> {
        let profile = self
            .active_profile_data_mut()
            .ok_or_else(|| CoreError::Invalid("active profile missing".to_string()))?;
        if let Some(assignment) = profile.assignment_mut(monitor_id) {
            assignment.set_wallpaper(wallpaper);
            return Ok(());
        }
        profile.assign(MonitorAssignment::new(
            monitor_id.clone(),
            FitMode::Fill,
            wallpaper,
        ));
        Ok(())
    }

    pub fn set_monitor_fit(
        &mut self,
        monitor_id: &MonitorId,
        fit: FitMode,
    ) -> Result<(), CoreError> {
        let profile = self
            .active_profile_data_mut()
            .ok_or_else(|| CoreError::Invalid("active profile missing".to_string()))?;
        if let Some(assignment) = profile.assignment_mut(monitor_id) {
            assignment.set_fit(fit);
            return Ok(());
        }
        profile.assign(MonitorAssignment::new(
            monitor_id.clone(),
            fit,
            WallpaperSource::bundled(BundledSample::A),
        ));
        Ok(())
    }

    pub async fn load_json(path: &Path) -> Result<Self, CoreError> {
        let raw = tokio::fs::read_to_string(path)
            .await
            .map_err(|error| CoreError::Io(error.to_string()))?;
        serde_json::from_str(&raw).map_err(|error| CoreError::Parse(error.to_string()))
    }

    pub async fn save_json(&self, path: &Path) -> Result<(), CoreError> {
        let raw = serde_json::to_string_pretty(self)
            .map_err(|error| CoreError::Parse(error.to_string()))?;
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                tokio::fs::create_dir_all(parent)
                    .await
                    .map_err(|error| CoreError::Io(error.to_string()))?;
            }
        }
        tokio::fs::write(path, raw)
            .await
            .map_err(|error| CoreError::Io(error.to_string()))
    }

    /// Synchronous load for shell startup. Missing files yield `Ok(None)`.
    pub fn load_json_sync(path: &Path) -> Result<Option<Self>, CoreError> {
        match std::fs::read_to_string(path) {
            Ok(raw) => serde_json::from_str(&raw)
                .map(Some)
                .map_err(|error| CoreError::Parse(error.to_string())),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(CoreError::Io(error.to_string())),
        }
    }

    pub fn save_json_sync(&self, path: &Path) -> Result<(), CoreError> {
        let raw = serde_json::to_string_pretty(self)
            .map_err(|error| CoreError::Parse(error.to_string()))?;
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent)
                    .map_err(|error| CoreError::Io(error.to_string()))?;
            }
        }
        std::fs::write(path, raw).map_err(|error| CoreError::Io(error.to_string()))
    }

    pub fn load_or_default(path: &Path) -> Result<Self, CoreError> {
        match Self::load_json_sync(path)? {
            Some(config) => Ok(config),
            None => Self::with_default_profile(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Config;
    use crate::monitor::{FitMode, Monitor, MonitorId, MonitorSource};
    use crate::profile::{MonitorAssignment, Profile};
    use crate::wallpaper::{BundledSample, WallpaperSource};
    use crate::CoreError;

    struct FixedMonitors(Vec<Monitor>);

    impl MonitorSource for FixedMonitors {
        fn monitors(&self) -> Result<Vec<Monitor>, CoreError> {
            Ok(self.0.clone())
        }
    }

    fn sample_config() -> Config {
        let monitor_id = MonitorId::new("m1").expect("monitor id");
        let mut profile = Profile::new("default").expect("profile");
        profile.assign(MonitorAssignment::new(
            monitor_id,
            FitMode::Fill,
            WallpaperSource::bundled(BundledSample::A),
        ));
        let mut config = Config::empty();
        config.insert_profile(profile).expect("insert");
        config.set_active_profile("default").expect("activate");
        config
    }

    #[test]
    fn rejects_empty_ids_and_unknown_profiles() {
        assert!(MonitorId::new("").is_err());
        assert!(Monitor::new(MonitorId::new("m1").expect("id"), "", 1920, 1080).is_err());
        assert!(Monitor::new(MonitorId::new("m1").expect("id"), "Primary", 0, 1080).is_err());
        assert!(Profile::new("").is_err());

        let mut config = Config::empty();
        assert!(config.set_active_profile("missing").is_err());
    }

    #[test]
    fn monitor_source_is_the_os_boundary() {
        let monitor = Monitor::new(MonitorId::new("m1").expect("id"), "Primary", 1920, 1080)
            .expect("monitor");
        let source = FixedMonitors(vec![monitor.clone()]);
        let listed = source.monitors().expect("list");
        assert_eq!(listed, vec![monitor]);
    }

    #[tokio::test]
    async fn config_round_trips_through_json() {
        let config = sample_config();
        let path = std::env::temp_dir().join(format!(
            "background-customizer-core-{}-{}.json",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        ));
        config.save_json(&path).await.expect("save");
        let loaded = Config::load_json(&path).await.expect("load");
        assert_eq!(loaded, config);
        assert_eq!(loaded.active_profile(), Some("default"));
        assert_eq!(loaded.profiles()[0].monitors()[0].fit(), FitMode::Fill);
        assert_eq!(
            loaded.profiles()[0].monitors()[0].wallpaper(),
            &WallpaperSource::bundled(BundledSample::A)
        );
        std::fs::remove_file(&path).expect("cleanup");
    }

    #[tokio::test]
    async fn missing_config_file_is_an_io_error() {
        let path = std::env::temp_dir().join(format!(
            "background-customizer-missing-{}-{}.json",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        ));
        let error = Config::load_json(&path).await.expect_err("missing");
        assert!(matches!(error, CoreError::Io(_)));
    }

    #[test]
    fn sync_load_or_default_and_wallpaper_updates_persist() {
        let path = std::env::temp_dir().join(format!(
            "background-customizer-sync-{}-{}.json",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        ));
        let _ = std::fs::remove_file(&path);

        let mut config = Config::load_or_default(&path).expect("default");
        let monitors = vec![
            Monitor::placed(MonitorId::new("a").expect("id"), "a", 800, 600, 0, 0, true)
                .expect("monitor"),
            Monitor::placed(
                MonitorId::new("b").expect("id"),
                "b",
                800,
                600,
                800,
                0,
                false,
            )
            .expect("monitor"),
        ];
        assert!(config
            .ensure_monitor_assignments(&monitors)
            .expect("ensure"));
        assert_eq!(
            config.wallpaper_for(monitors[0].id()),
            Some(&WallpaperSource::bundled(BundledSample::A))
        );
        assert_eq!(
            config.wallpaper_for(monitors[1].id()),
            Some(&WallpaperSource::bundled(BundledSample::B))
        );

        config
            .set_monitor_wallpaper(monitors[0].id(), WallpaperSource::bundled(BundledSample::B))
            .expect("set");
        config.save_json_sync(&path).expect("save");

        let reloaded = Config::load_or_default(&path).expect("reload");
        assert_eq!(
            reloaded.wallpaper_for(monitors[0].id()),
            Some(&WallpaperSource::bundled(BundledSample::B))
        );
        std::fs::remove_file(&path).expect("cleanup");
    }

    #[test]
    fn old_assignments_without_wallpaper_default_to_sample_a() {
        let raw = r#"{
            "profiles": [{
                "name": "default",
                "monitors": [{ "monitor_id": "m1", "fit": "fill" }]
            }],
            "active_profile": "default"
        }"#;
        let config: Config = serde_json::from_str(raw).expect("parse");
        assert_eq!(
            config.profiles()[0].monitors()[0].wallpaper(),
            &WallpaperSource::bundled(BundledSample::A)
        );
    }
}
