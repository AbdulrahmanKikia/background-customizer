//! Benchmark harness for the six specification scenarios.
//!
//! Wallpaper Engine and Lively numbers are recorded only when those programs
//! are running. A missing target stays null instead of being filled with a guess.

mod error;
mod latency;
mod model;
mod platform;
mod presentmon;
mod probe;
mod regression;
mod scenario;
mod startup;

pub use error::BenchError;
pub use latency::{latency_ms, LatencyMarks, TimestampLog};
pub use model::{
    missing_baselines, read_document, write_document, DeviceEvent, DeviceKind, DocumentKind,
    MeasureStatus, MetricSample, MetricSummary, ResultsDocument, ScenarioResult, TargetId,
    TargetResults, SCHEMA_VERSION,
};
pub use presentmon::{frame_times_from_offsets, parse_presentmon_csv, FramePacing};
pub use probe::{record_scenario, DeviceSnapshot, DeviceWatch, MetricProbe, RecordRequest};
pub use regression::{compare_idle_cpu, idle_cpu_regressed, IdleComparison};
pub use scenario::{scenario, scenarios, Scenario, ScenarioId, ScenarioKind};
pub use startup::time_command;

use std::path::Path;
use std::time::Duration;

use platform::{process_is_running, HostProbe};

pub fn check_paths(
    baseline: &Path,
    candidate: Option<&Path>,
) -> Result<Vec<IdleComparison>, BenchError> {
    let baseline = read_document(baseline)?;
    let Some(candidate) = candidate else {
        return Ok(Vec::new());
    };
    if !candidate.exists() {
        return Ok(Vec::new());
    }
    let candidate = read_document(candidate)?;
    compare_idle_cpu(&baseline, &candidate)
}

pub fn record_to_file(
    target: TargetId,
    scenario_id: ScenarioId,
    duration: Option<Duration>,
    timestamps: Option<&TimestampLog>,
    presentmon_csv: Option<&str>,
    output: &Path,
) -> Result<ResultsDocument, BenchError> {
    let running = process_is_running(target.process_names())?;
    let definition = scenario(scenario_id);
    let missing_reason = format!(
        "{} is not running ({})",
        target.as_str(),
        target.process_names().join(", ")
    );
    let mut probe = HostProbe::open()?;
    let mut device_watch = LiveDevices::current();
    let watch = if definition.kind == ScenarioKind::HotUnplug {
        Some(&mut device_watch as &mut dyn DeviceWatch)
    } else {
        None
    };
    let result = record_scenario(
        &mut probe,
        watch,
        &RecordRequest {
            scenario: scenario_id,
            duration: duration.unwrap_or(definition.duration),
            sample_every: Duration::from_secs(1),
            target_running: running,
            missing_reason,
            timestamps,
            presentmon_csv,
            cold_start: None,
        },
    )?;
    let status = result.status;
    let reason = result.reason.clone();
    let document = ResultsDocument {
        schema_version: SCHEMA_VERSION,
        kind: DocumentKind::Run,
        recorded_at: Some(model::utc_now_rfc3339()?),
        note: if running {
            "Recorded from a running target. Null metrics were not measured.".to_string()
        } else {
            "Target was not running, so this run has no measurements.".to_string()
        },
        targets: vec![TargetResults {
            id: target,
            status,
            reason,
            scenarios: vec![result],
        }],
    };
    write_document(output, &document)?;
    Ok(document)
}

struct LiveDevices {
    latest: DeviceSnapshot,
}

impl LiveDevices {
    fn current() -> Self {
        Self {
            latest: platform::device_snapshot(),
        }
    }
}

impl DeviceWatch for LiveDevices {
    fn snapshot(&mut self) -> Result<DeviceSnapshot, BenchError> {
        self.latest = platform::device_snapshot();
        Ok(self.latest)
    }
}
