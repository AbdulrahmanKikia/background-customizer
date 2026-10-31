use std::time::Duration;

use crate::error::BenchError;
use crate::latency::{latency_ms, TimestampLog};
use crate::model::{
    missing_scenario, summarize, DeviceEvent, DeviceKind, MeasureStatus, MetricSample,
    MetricSummary, ScenarioResult,
};
use crate::presentmon::{frame_times_from_offsets, parse_presentmon_csv};
use crate::scenario::{scenario, ScenarioId, ScenarioKind};

pub trait MetricProbe {
    fn sample(&mut self) -> Result<MetricSample, BenchError>;
    fn collector_notes(&self) -> Vec<String>;
}

pub trait DeviceWatch {
    fn snapshot(&mut self) -> Result<DeviceSnapshot, BenchError>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DeviceSnapshot {
    pub monitors: Option<u32>,
    pub audio_outputs: Option<u32>,
}

pub struct RecordRequest<'a> {
    pub scenario: ScenarioId,
    pub duration: Duration,
    pub sample_every: Duration,
    pub target_running: bool,
    pub missing_reason: String,
    pub timestamps: Option<&'a TimestampLog>,
    pub presentmon_csv: Option<&'a str>,
    pub cold_start: Option<Duration>,
}

pub fn record_scenario(
    probe: &mut dyn MetricProbe,
    mut devices: Option<&mut dyn DeviceWatch>,
    request: &RecordRequest<'_>,
) -> Result<ScenarioResult, BenchError> {
    if !request.target_running {
        return Ok(missing_scenario(request.scenario, &request.missing_reason));
    }

    let definition = scenario(request.scenario);
    let started = std::time::Instant::now();
    let mut samples = Vec::new();
    let mut device_events = Vec::new();
    let mut previous_devices: Option<DeviceSnapshot> = None;
    let mut saw_monitor = false;
    let mut saw_audio = false;

    loop {
        let offset_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
        let mut sample = probe.sample()?;
        sample.offset_ms = offset_ms;
        samples.push(sample);

        if let Some(watch) = devices.as_mut() {
            let snapshot = (*watch).snapshot()?;
            if let Some(previous) = previous_devices {
                if decreased(previous.monitors, snapshot.monitors) && !saw_monitor {
                    device_events.push(DeviceEvent {
                        kind: DeviceKind::Monitor,
                        offset_ms,
                    });
                    saw_monitor = true;
                }
                if decreased(previous.audio_outputs, snapshot.audio_outputs) && !saw_audio {
                    device_events.push(DeviceEvent {
                        kind: DeviceKind::Audio,
                        offset_ms,
                    });
                    saw_audio = true;
                }
            }
            previous_devices = Some(snapshot);
        }

        let finished = started.elapsed() >= request.duration;
        let unplug_done = definition.kind == ScenarioKind::HotUnplug && saw_monitor && saw_audio;
        if finished || unplug_done || request.sample_every.is_zero() {
            break;
        }
        let remaining = request.duration.saturating_sub(started.elapsed());
        std::thread::sleep(request.sample_every.min(remaining));
        if started.elapsed() >= request.duration {
            break;
        }
    }

    let mut summary = summarize(&samples);
    let mut notes = probe.collector_notes();
    apply_timestamps(&mut summary, &mut device_events, &mut notes, request)?;
    if summary.cpu_percent_mean.is_none() {
        notes.push("cpu percent was not reported by the probe".to_string());
    }
    if summary.power_mw_mean.is_none() {
        notes.push(
            "power estimate was not available (no discharging battery rate and no external meter)"
                .to_string(),
        );
    }

    Ok(ScenarioResult {
        id: request.scenario,
        status: MeasureStatus::Recorded,
        reason: None,
        summary,
        samples,
        device_events,
        notes,
    })
}

fn apply_timestamps(
    summary: &mut MetricSummary,
    device_events: &mut Vec<DeviceEvent>,
    notes: &mut Vec<String>,
    request: &RecordRequest<'_>,
) -> Result<(), BenchError> {
    if let Some(cold_start) = request.cold_start {
        summary.cold_start_ms = Some(u64::try_from(cold_start.as_millis()).unwrap_or(u64::MAX));
    }
    if let Some(csv) = request.presentmon_csv {
        let pacing = parse_presentmon_csv(csv)?;
        summary.frame_time_ms_mean = Some(pacing.frame_time_ms_mean);
        summary.dropped_frames = Some(pacing.dropped_frames);
        notes.push(format!(
            "frame pacing from PresentMon CSV ({} frames)",
            pacing.frame_count
        ));
    } else if let Some(log) = request.timestamps {
        summary.frame_time_ms_mean = frame_times_from_offsets(&log.frame_offset_ms);
        if summary.frame_time_ms_mean.is_some() {
            notes.push(
                "frame pacing from scripted timestamps; PresentMon CSV was not used".to_string(),
            );
        }
    } else {
        notes.push("frame pacing was not captured (PresentMon is not installed and no timestamp log was provided)".to_string());
    }

    if let Some(log) = request.timestamps {
        if let Some(pause) = &log.pause {
            summary.pause_latency_ms = Some(latency_ms(pause)?);
        }
        if let Some(resume) = &log.resume {
            summary.resume_latency_ms = Some(latency_ms(resume)?);
        }
        if let Some(offset_ms) = log.unplug_monitor_ms {
            device_events.push(DeviceEvent {
                kind: DeviceKind::Monitor,
                offset_ms,
            });
        }
        if let Some(offset_ms) = log.unplug_audio_ms {
            device_events.push(DeviceEvent {
                kind: DeviceKind::Audio,
                offset_ms,
            });
        }
    }
    Ok(())
}

fn decreased(previous: Option<u32>, current: Option<u32>) -> bool {
    match (previous, current) {
        (Some(previous), Some(current)) => current < previous,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::{record_scenario, DeviceSnapshot, DeviceWatch, MetricProbe, RecordRequest};
    use crate::error::BenchError;
    use crate::latency::{LatencyMarks, TimestampLog};
    use crate::model::{DeviceKind, MeasureStatus, MetricSample};
    use crate::scenario::ScenarioId;
    use std::time::Duration;

    struct FixedProbe;

    impl MetricProbe for FixedProbe {
        fn sample(&mut self) -> Result<MetricSample, BenchError> {
            Ok(MetricSample {
                offset_ms: 0,
                cpu_percent: Some(4.0),
                ram_bytes: Some(1_000),
                gpu_percent: Some(1.0),
                vram_bytes: Some(2_000),
                power_mw: None,
                frame_time_ms: None,
            })
        }

        fn collector_notes(&self) -> Vec<String> {
            vec!["test probe".to_string()]
        }
    }

    struct ScriptedDevices {
        counts: Vec<DeviceSnapshot>,
        index: usize,
    }

    impl DeviceWatch for ScriptedDevices {
        fn snapshot(&mut self) -> Result<DeviceSnapshot, BenchError> {
            let snapshot = self
                .counts
                .get(self.index)
                .copied()
                .unwrap_or(DeviceSnapshot {
                    monitors: Some(1),
                    audio_outputs: Some(1),
                });
            self.index += 1;
            Ok(snapshot)
        }
    }

    #[test]
    fn missing_target_does_not_invent_metrics() {
        let mut probe = FixedProbe;
        let result = record_scenario(
            &mut probe,
            None,
            &RecordRequest {
                scenario: ScenarioId::IdleStatic,
                duration: Duration::ZERO,
                sample_every: Duration::ZERO,
                target_running: false,
                missing_reason: "not installed".to_string(),
                timestamps: None,
                presentmon_csv: None,
                cold_start: None,
            },
        )
        .expect("record");
        assert_eq!(result.status, MeasureStatus::Missing);
        assert!(result.summary.cpu_percent_mean.is_none());
        assert!(result.samples.is_empty());
    }

    #[test]
    fn running_target_records_samples_and_scripted_latency() {
        let mut probe = FixedProbe;
        let log = TimestampLog {
            pause: Some(LatencyMarks {
                event_ms: 10,
                settled_ms: 40,
            }),
            resume: Some(LatencyMarks {
                event_ms: 80,
                settled_ms: 100,
            }),
            frame_offset_ms: vec![0, 16, 33],
            unplug_monitor_ms: None,
            unplug_audio_ms: None,
        };
        let result = record_scenario(
            &mut probe,
            None,
            &RecordRequest {
                scenario: ScenarioId::FullscreenOcclusion,
                duration: Duration::ZERO,
                sample_every: Duration::ZERO,
                target_running: true,
                missing_reason: String::new(),
                timestamps: Some(&log),
                presentmon_csv: None,
                cold_start: Some(Duration::from_millis(25)),
            },
        )
        .expect("record");
        assert_eq!(result.status, MeasureStatus::Recorded);
        assert_eq!(result.summary.cpu_percent_mean, Some(4.0));
        assert_eq!(result.summary.pause_latency_ms, Some(30));
        assert_eq!(result.summary.resume_latency_ms, Some(20));
        assert_eq!(result.summary.cold_start_ms, Some(25));
        assert_eq!(result.summary.frame_time_ms_mean, Some(16.5));
    }

    #[test]
    fn hot_unplug_records_a_drop_in_device_counts() {
        let mut probe = FixedProbe;
        let mut devices = ScriptedDevices {
            counts: vec![
                DeviceSnapshot {
                    monitors: Some(2),
                    audio_outputs: Some(2),
                },
                DeviceSnapshot {
                    monitors: Some(1),
                    audio_outputs: Some(1),
                },
            ],
            index: 0,
        };
        let result = record_scenario(
            &mut probe,
            Some(&mut devices),
            &RecordRequest {
                scenario: ScenarioId::HotUnplug,
                duration: Duration::from_secs(600),
                sample_every: Duration::from_millis(1),
                target_running: true,
                missing_reason: String::new(),
                timestamps: None,
                presentmon_csv: None,
                cold_start: None,
            },
        )
        .expect("record");
        assert!(result
            .device_events
            .iter()
            .any(|event| event.kind == DeviceKind::Monitor));
        assert!(result
            .device_events
            .iter()
            .any(|event| event.kind == DeviceKind::Audio));
    }
}
