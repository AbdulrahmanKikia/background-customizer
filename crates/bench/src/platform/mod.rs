mod host;

#[cfg(windows)]
mod win32;

pub use host::{process_is_running, HostProbe};

#[cfg(windows)]
pub use win32::device_snapshot;

#[cfg(not(windows))]
pub fn device_snapshot() -> crate::probe::DeviceSnapshot {
    crate::probe::DeviceSnapshot {
        monitors: None,
        audio_outputs: None,
    }
}
