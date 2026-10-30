use serde::{Deserialize, Serialize};

use crate::error::CoreError;
use crate::monitor::{FitMode, MonitorId};

/// Wallpaper placement for one monitor inside a profile.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MonitorAssignment {
    monitor_id: MonitorId,
    fit: FitMode,
}

impl MonitorAssignment {
    pub fn new(monitor_id: MonitorId, fit: FitMode) -> Self {
        Self { monitor_id, fit }
    }

    pub fn monitor_id(&self) -> &MonitorId {
        &self.monitor_id
    }

    pub fn fit(&self) -> FitMode {
        self.fit
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Profile {
    name: String,
    monitors: Vec<MonitorAssignment>,
}

impl Profile {
    pub fn new(name: impl Into<String>) -> Result<Self, CoreError> {
        let name = name.into();
        if name.is_empty() {
            return Err(CoreError::Invalid("profile name is empty".to_string()));
        }
        Ok(Self {
            name,
            monitors: Vec::new(),
        })
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn monitors(&self) -> &[MonitorAssignment] {
        &self.monitors
    }

    pub fn assign(&mut self, assignment: MonitorAssignment) {
        let monitor_id = assignment.monitor_id().clone();
        if let Some(existing) = self
            .monitors
            .iter_mut()
            .find(|item| item.monitor_id() == &monitor_id)
        {
            *existing = assignment;
            return;
        }
        self.monitors.push(assignment);
    }
}
