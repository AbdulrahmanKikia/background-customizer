use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::error::CoreError;
use crate::profile::Profile;

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

    pub fn profiles(&self) -> &[Profile] {
        &self.profiles
    }

    pub fn active_profile(&self) -> Option<&str> {
        self.active_profile.as_deref()
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
}

#[cfg(test)]
mod tests {
    use super::Config;
    use crate::monitor::{FitMode, Monitor, MonitorId, MonitorSource};
    use crate::profile::{MonitorAssignment, Profile};
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
        profile.assign(MonitorAssignment::new(monitor_id, FitMode::Fill));
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
}
