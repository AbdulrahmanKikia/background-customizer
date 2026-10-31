use serde::{Deserialize, Serialize};

use crate::error::CoreError;

/// Stable id for one display. Platform code maps this to a native monitor handle.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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
}

impl Monitor {
    pub fn new(
        id: MonitorId,
        name: impl Into<String>,
        width_px: u32,
        height_px: u32,
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
}

/// Operating-system monitor enumeration. Implementations live outside this crate.
pub trait MonitorSource {
    fn monitors(&self) -> Result<Vec<Monitor>, CoreError>;
}
