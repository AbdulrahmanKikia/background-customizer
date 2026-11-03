use serde::{Deserialize, Serialize};

use crate::error::CoreError;

/// Stable id for one display. Platform code maps this to a native monitor handle.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct MonitorId(String);

impl MonitorId {
    pub fn new(value: impl Into<String>) -> Result<Self, CoreError> {
        let value = value.into();
        if value.is_empty() {
            return Err(CoreError::Invalid("monitor id is empty".to_string()));
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// How a wallpaper is placed on a monitor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FitMode {
    Fill,
    Fit,
    Stretch,
    Tile,
    Center,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Monitor {
    id: MonitorId,
    name: String,
    width_px: u32,
    height_px: u32,
    #[serde(default)]
    left_px: i32,
    #[serde(default)]
    top_px: i32,
    #[serde(default)]
    primary: bool,
}

impl Monitor {
    pub fn new(
        id: MonitorId,
        name: impl Into<String>,
        width_px: u32,
        height_px: u32,
    ) -> Result<Self, CoreError> {
        Self::placed(id, name, width_px, height_px, 0, 0, false)
    }

    pub fn placed(
        id: MonitorId,
        name: impl Into<String>,
        width_px: u32,
        height_px: u32,
        left_px: i32,
        top_px: i32,
        primary: bool,
    ) -> Result<Self, CoreError> {
        let name = name.into();
        if name.is_empty() {
            return Err(CoreError::Invalid("monitor name is empty".to_string()));
        }
        if width_px == 0 || height_px == 0 {
            return Err(CoreError::Invalid(
                "monitor size must be non-zero".to_string(),
            ));
        }
        Ok(Self {
            id,
            name,
            width_px,
            height_px,
            left_px,
            top_px,
            primary,
        })
    }

    pub fn id(&self) -> &MonitorId {
        &self.id
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn width_px(&self) -> u32 {
        self.width_px
    }

    pub fn height_px(&self) -> u32 {
        self.height_px
    }

    pub fn left_px(&self) -> i32 {
        self.left_px
    }

    pub fn top_px(&self) -> i32 {
        self.top_px
    }

    pub fn primary(&self) -> bool {
        self.primary
    }
}

/// Operating-system monitor enumeration. Implementations live outside this crate.
pub trait MonitorSource {
    fn monitors(&self) -> Result<Vec<Monitor>, CoreError>;
}

/// Diff between two monitor lists for hotplug / DPI / resolution handling.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MonitorChange {
    Added(Monitor),
    Removed(MonitorId),
    Changed(Monitor),
}

/// Compare previous and current monitor layouts.
pub fn diff_monitors(previous: &[Monitor], current: &[Monitor]) -> Vec<MonitorChange> {
    let mut changes = Vec::new();
    for monitor in current {
        match previous.iter().find(|item| item.id() == monitor.id()) {
            None => changes.push(MonitorChange::Added(monitor.clone())),
            Some(old) if old != monitor => changes.push(MonitorChange::Changed(monitor.clone())),
            Some(_) => {}
        }
    }
    for monitor in previous {
        if current.iter().all(|item| item.id() != monitor.id()) {
            changes.push(MonitorChange::Removed(monitor.id().clone()));
        }
    }
    changes
}

#[cfg(test)]
mod tests {
    use super::{diff_monitors, Monitor, MonitorChange, MonitorId};

    fn monitor(id: &str, width: u32, height: u32, left: i32, primary: bool) -> Monitor {
        Monitor::placed(
            MonitorId::new(id).expect("id"),
            id,
            width,
            height,
            left,
            0,
            primary,
        )
        .expect("monitor")
    }

    #[test]
    fn diff_reports_add_remove_and_resize() {
        let previous = vec![
            monitor("a", 1920, 1080, 0, true),
            monitor("b", 1280, 720, 1920, false),
        ];
        let current = vec![
            monitor("a", 2560, 1440, 0, true),
            monitor("c", 800, 600, 2560, false),
        ];
        let changes = diff_monitors(&previous, &current);
        assert!(changes.contains(&MonitorChange::Changed(monitor("a", 2560, 1440, 0, true))));
        assert!(changes.contains(&MonitorChange::Removed(MonitorId::new("b").expect("id"))));
        assert!(changes.contains(&MonitorChange::Added(monitor("c", 800, 600, 2560, false))));
    }
}
