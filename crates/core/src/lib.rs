//! Config, profiles, and the monitor model.
//!
//! Operating-system calls stay behind [`MonitorSource`]. This crate does not
//! talk to a windowing system.

mod config;
mod error;
mod monitor;
mod profile;

pub use config::Config;
pub use error::CoreError;
pub use monitor::{FitMode, Monitor, MonitorId, MonitorSource};
pub use profile::{MonitorAssignment, Profile};
