use std::path::PathBuf;

use sysinfo::{Gpus, System};

use crate::error::BenchError;
use crate::model::MetricSample;
use crate::probe::MetricProbe;

pub struct HostProbe {
    system: System,
    gpus: Option<Gpus>,
    warmed: bool,
    notes: Vec<String>,
}

impl HostProbe {
    pub fn open() -> Result<Self, BenchError> {
        let system = System::new()
            .map_err(|error| BenchError::Message(format!("sysinfo failed to open: {error}")))?;
        let gpus = Gpus::new_with_refreshed_list().ok();
        let mut notes = vec!["cpu and ram: sysinfo".to_string()];
        if gpus.is_some() {
            notes.push(
                "gpu and vram: sysinfo, which reads DXGI memory and PDH GPU engine counters"
                    .to_string(),
            );
        } else {
            notes.push(
                "gpu and vram: unavailable because the GPU list could not be opened".to_string(),
            );
        }
        notes.push(power_note());
        notes.push(presentmon_status().describe());
        Ok(Self {
            system,
            gpus,
            warmed: false,
            notes,
        })
    }
}

impl MetricProbe for HostProbe {
    fn sample(&mut self) -> Result<MetricSample, BenchError> {
        self.system.refresh_cpu_usage();
        if let Some(gpus) = self.gpus.as_mut() {
            gpus.refresh(false);
        }
        if !self.warmed {
            std::thread::sleep(sysinfo::MINIMUM_CPU_UPDATE_INTERVAL);
            self.system.refresh_cpu_usage();
            if let Some(gpus) = self.gpus.as_mut() {
                gpus.refresh(false);
            }
            self.warmed = true;
        }
        self.system.refresh_memory();

        let cpu_percent = average_cpu(&self.system);
        let gpu_percent = self.gpus.as_ref().and_then(max_usage).map(f64::from);
        let vram_bytes = self.gpus.as_ref().and_then(sum_used_memory);

        Ok(MetricSample {
            offset_ms: 0,
            cpu_percent,
            ram_bytes: Some(self.system.used_memory()),
            gpu_percent,
            vram_bytes,
            power_mw: power_milliwatts(),
            frame_time_ms: None,
        })
    }

    fn collector_notes(&self) -> Vec<String> {
        self.notes.clone()
    }
}

pub fn process_is_running(names: &[&str]) -> Result<bool, BenchError> {
    let system = System::new_all().map_err(|error| {
        BenchError::Message(format!("sysinfo failed to list processes: {error}"))
    })?;
    Ok(system.processes().values().any(|process| {
        let name = process.name().to_string_lossy();
        names
            .iter()
            .any(|expected| name.eq_ignore_ascii_case(expected))
    }))
}

pub enum PresentMonStatus {
    Found(PathBuf),
    Missing,
}

impl PresentMonStatus {
    pub fn describe(&self) -> String {
        match self {
            Self::Found(path) => format!("frame pacing: PresentMon at {}", path.display()),
            Self::Missing => {
                "frame pacing: PresentMon is not installed; pass a CSV or a timestamp log"
                    .to_string()
            }
        }
    }
}

pub fn presentmon_status() -> PresentMonStatus {
    let path_var = std::env::var_os("PATH").unwrap_or_default();
    for directory in std::env::split_paths(&path_var) {
        let candidate = directory.join("PresentMon.exe");
        if candidate.is_file() {
            return PresentMonStatus::Found(candidate);
        }
    }
    PresentMonStatus::Missing
}

fn average_cpu(system: &System) -> Option<f64> {
    let mut count = 0u64;
    let mut sum = 0.0;
    for cpu in system.cpus() {
        let usage = f64::from(cpu.usage());
        if usage.is_finite() {
            sum += usage;
            count += 1;
        }
    }
    if count == 0 {
        None
    } else {
        Some(sum / count as f64)
    }
}

fn max_usage(gpus: &Gpus) -> Option<f32> {
    let mut best: Option<f32> = None;
    for gpu in gpus {
        if let Some(usage) = gpu.usage() {
            if usage.is_finite() {
                best = Some(best.map_or(usage, |current| current.max(usage)));
            }
        }
    }
    best
}

fn sum_used_memory(gpus: &Gpus) -> Option<u64> {
    let mut any = false;
    let mut total = 0u64;
    for gpu in gpus {
        if let Some(used) = gpu.used_memory() {
            any = true;
            total = total.saturating_add(used);
        }
    }
    if any {
        Some(total)
    } else {
        None
    }
}

fn power_milliwatts() -> Option<f64> {
    #[cfg(windows)]
    {
        super::win32::power_milliwatts()
    }
    #[cfg(not(windows))]
    {
        None
    }
}

fn power_note() -> String {
    #[cfg(windows)]
    {
        super::win32::power_note()
    }
    #[cfg(not(windows))]
    {
        "power: no Windows energy estimate on this target".to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::{process_is_running, HostProbe};
    use crate::probe::MetricProbe;

    #[test]
    fn unknown_process_is_not_running() {
        let running = process_is_running(&["background-customizer-missing-process.exe"])
            .expect("process list");
        assert!(!running);
    }

    #[test]
    fn host_probe_samples_cpu_and_ram() {
        let mut probe = HostProbe::open().expect("probe");
        let sample = probe.sample().expect("sample");
        assert!(sample.cpu_percent.is_some());
        assert!(sample.ram_bytes.is_some());
        assert!(!probe.collector_notes().is_empty());
    }
}
