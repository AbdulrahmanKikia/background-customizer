use serde::{Deserialize, Serialize};

use crate::error::BenchError;
use crate::scenario::ScenarioId;

pub const SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TargetId {
    #[serde(rename = "wallpaper-engine")]
    WallpaperEngine,
    #[serde(rename = "lively")]
    Lively,
    #[serde(rename = "background-customizer")]
    BackgroundCustomizer,
}

impl TargetId {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::WallpaperEngine => "wallpaper-engine",
            Self::Lively => "lively",
            Self::BackgroundCustomizer => "background-customizer",
        }
    }

    pub fn parse(value: &str) -> Result<Self, BenchError> {
        match value {
            "wallpaper-engine" => Ok(Self::WallpaperEngine),
            "lively" => Ok(Self::Lively),
            "background-customizer" => Ok(Self::BackgroundCustomizer),
            _ => Err(BenchError::Message(format!("unknown target: {value}"))),
        }
    }

    pub fn process_names(self) -> &'static [&'static str] {
        match self {
            Self::WallpaperEngine => &["wallpaper64.exe", "wallpaper32.exe"],
            Self::Lively => &["Lively.exe"],
            Self::BackgroundCustomizer => &["background-customizer.exe"],
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum DocumentKind {
    Baseline,
    Run,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum MeasureStatus {
    /// No measurement was taken. Null metrics are missing data, not zeros.
    Missing,
    Recorded,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MetricSample {
    pub offset_ms: u64,
    pub cpu_percent: Option<f64>,
    pub ram_bytes: Option<u64>,
    pub gpu_percent: Option<f64>,
    pub vram_bytes: Option<u64>,
    pub power_mw: Option<f64>,
    pub frame_time_ms: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MetricSummary {
    pub cpu_percent_mean: Option<f64>,
    pub ram_bytes_mean: Option<u64>,
    pub gpu_percent_mean: Option<f64>,
    pub vram_bytes_mean: Option<u64>,
    pub power_mw_mean: Option<f64>,
    pub frame_time_ms_mean: Option<f64>,
    pub dropped_frames: Option<u64>,
    pub cold_start_ms: Option<u64>,
    pub pause_latency_ms: Option<u64>,
    pub resume_latency_ms: Option<u64>,
}

impl MetricSummary {
    pub fn empty() -> Self {
        Self {
            cpu_percent_mean: None,
            ram_bytes_mean: None,
            gpu_percent_mean: None,
            vram_bytes_mean: None,
            power_mw_mean: None,
            frame_time_ms_mean: None,
            dropped_frames: None,
            cold_start_ms: None,
            pause_latency_ms: None,
            resume_latency_ms: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum DeviceKind {
    Monitor,
    Audio,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeviceEvent {
    pub kind: DeviceKind,
    pub offset_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScenarioResult {
    pub id: ScenarioId,
    pub status: MeasureStatus,
    pub reason: Option<String>,
    pub summary: MetricSummary,
    pub samples: Vec<MetricSample>,
    pub device_events: Vec<DeviceEvent>,
    /// Why a collector did not produce a number. Empty when nothing was skipped.
    #[serde(default)]
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TargetResults {
    pub id: TargetId,
    pub status: MeasureStatus,
    pub reason: Option<String>,
    pub scenarios: Vec<ScenarioResult>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResultsDocument {
    pub schema_version: u32,
    pub kind: DocumentKind,
    pub recorded_at: Option<String>,
    pub note: String,
    pub targets: Vec<TargetResults>,
}

impl ResultsDocument {
    pub fn validate(&self) -> Result<(), BenchError> {
        if self.schema_version != SCHEMA_VERSION {
            return Err(BenchError::Parse(format!(
                "unsupported schema version {}",
                self.schema_version
            )));
        }
        Ok(())
    }
}

pub fn write_document(
    path: &std::path::Path,
    document: &ResultsDocument,
) -> Result<(), BenchError> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }
    let mut text = serde_json::to_string_pretty(document)?;
    text.push('\n');
    std::fs::write(path, text)?;
    Ok(())
}

pub fn read_document(path: &std::path::Path) -> Result<ResultsDocument, BenchError> {
    let text = std::fs::read_to_string(path)?;
    let document: ResultsDocument = serde_json::from_str(&text)?;
    document.validate()?;
    Ok(document)
}

pub fn summarize(samples: &[MetricSample]) -> MetricSummary {
    MetricSummary {
        cpu_percent_mean: mean_f64(samples.iter().filter_map(|sample| sample.cpu_percent)),
        ram_bytes_mean: mean_u64(samples.iter().filter_map(|sample| sample.ram_bytes)),
        gpu_percent_mean: mean_f64(samples.iter().filter_map(|sample| sample.gpu_percent)),
        vram_bytes_mean: mean_u64(samples.iter().filter_map(|sample| sample.vram_bytes)),
        power_mw_mean: mean_f64(samples.iter().filter_map(|sample| sample.power_mw)),
        frame_time_ms_mean: mean_f64(samples.iter().filter_map(|sample| sample.frame_time_ms)),
        dropped_frames: None,
        cold_start_ms: None,
        pause_latency_ms: None,
        resume_latency_ms: None,
    }
}

fn mean_f64(values: impl Iterator<Item = f64>) -> Option<f64> {
    let mut count = 0u64;
    let mut sum = 0.0;
    for value in values {
        if !value.is_finite() {
            continue;
        }
        sum += value;
        count += 1;
    }
    if count == 0 {
        None
    } else {
        Some(sum / count as f64)
    }
}

fn mean_u64(values: impl Iterator<Item = u64>) -> Option<u64> {
    let mut count = 0u64;
    let mut sum = 0u128;
    for value in values {
        sum += u128::from(value);
        count += 1;
    }
    if count == 0 {
        None
    } else {
        Some((sum / u128::from(count)) as u64)
    }
}

pub fn missing_scenario(id: ScenarioId, reason: impl Into<String>) -> ScenarioResult {
    ScenarioResult {
        id,
        status: MeasureStatus::Missing,
        reason: Some(reason.into()),
        summary: MetricSummary::empty(),
        samples: Vec::new(),
        device_events: Vec::new(),
        notes: Vec::new(),
    }
}

pub fn missing_baselines() -> ResultsDocument {
    let wallpaper = "Wallpaper Engine is not installed on this machine (wallpaper64.exe was not found), so no baseline was recorded.";
    let lively = "Lively Wallpaper is not installed on this machine (Lively.exe was not found), so no baseline was recorded.";
    ResultsDocument {
        schema_version: SCHEMA_VERSION,
        kind: DocumentKind::Baseline,
        recorded_at: None,
        note: "Placeholder baseline from Milestone 0.5. Null metrics mean the measurement was not taken. They are not zeros. PresentMon was not installed. CI compares idle-static cpu_percent_mean with results/latest.json when that candidate file exists, and fails only when the candidate is higher than a numeric baseline.".to_string(),
        targets: vec![
            missing_target(TargetId::WallpaperEngine, wallpaper),
            missing_target(TargetId::Lively, lively),
        ],
    }
}

fn missing_target(id: TargetId, reason: &str) -> TargetResults {
    let scenarios = crate::scenario::scenarios()
        .iter()
        .map(|scenario| missing_scenario(scenario.id, reason))
        .collect();
    TargetResults {
        id,
        status: MeasureStatus::Missing,
        reason: Some(reason.to_string()),
        scenarios,
    }
}

pub fn utc_now_rfc3339() -> Result<String, BenchError> {
    time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .map_err(|error| BenchError::Message(error.to_string()))
}

#[cfg(test)]
mod tests {
    use super::{missing_baselines, summarize, MeasureStatus, MetricSample, SCHEMA_VERSION};
    use crate::scenario::scenarios;

    #[test]
    fn placeholder_has_six_empty_scenarios_per_target() {
        let document = missing_baselines();
        assert_eq!(document.schema_version, SCHEMA_VERSION);
        assert!(document.recorded_at.is_none());
        assert_eq!(document.targets.len(), 2);
        for target in &document.targets {
            assert_eq!(target.status, MeasureStatus::Missing);
            assert_eq!(target.scenarios.len(), scenarios().len());
            for scenario in &target.scenarios {
                assert_eq!(scenario.status, MeasureStatus::Missing);
                assert!(scenario.summary.cpu_percent_mean.is_none());
                assert!(scenario.summary.ram_bytes_mean.is_none());
                assert!(scenario.summary.gpu_percent_mean.is_none());
                assert!(scenario.summary.vram_bytes_mean.is_none());
                assert!(scenario.summary.power_mw_mean.is_none());
                assert!(scenario.summary.frame_time_ms_mean.is_none());
                assert!(scenario.samples.is_empty());
            }
        }
    }

    #[test]
    fn committed_placeholder_matches_the_harness() {
        let path =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../results/baselines.json");
        let document = super::read_document(&path).expect("baseline file");
        assert_eq!(document, super::missing_baselines());
    }

    #[test]
    fn summary_ignores_missing_samples() {
        let samples = vec![
            MetricSample {
                offset_ms: 0,
                cpu_percent: Some(10.0),
                ram_bytes: None,
                gpu_percent: None,
                vram_bytes: Some(100),
                power_mw: None,
                frame_time_ms: Some(f64::NAN),
            },
            MetricSample {
                offset_ms: 1000,
                cpu_percent: Some(30.0),
                ram_bytes: Some(50),
                gpu_percent: None,
                vram_bytes: Some(300),
                power_mw: None,
                frame_time_ms: None,
            },
        ];
        let summary = summarize(&samples);
        assert_eq!(summary.cpu_percent_mean, Some(20.0));
        assert_eq!(summary.ram_bytes_mean, Some(50));
        assert_eq!(summary.vram_bytes_mean, Some(200));
        assert!(summary.gpu_percent_mean.is_none());
        assert!(summary.frame_time_ms_mean.is_none());
    }
}
