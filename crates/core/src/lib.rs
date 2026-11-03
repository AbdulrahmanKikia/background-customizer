//! Config, profiles, and the monitor model.
//!
//! Operating-system calls stay behind [`MonitorSource`]. This crate does not
//! talk to a windowing system.

mod config;
mod error;
mod monitor;
mod profile;
mod wallpaper;

pub use config::{Config, DEFAULT_PROFILE_NAME};
pub use error::CoreError;
pub use monitor::{diff_monitors, FitMode, Monitor, MonitorChange, MonitorId, MonitorSource};
pub use profile::{MonitorAssignment, Profile};
pub use wallpaper::{BundledSample, WallpaperSource};
