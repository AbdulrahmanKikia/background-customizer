use serde::{Deserialize, Serialize};

use crate::error::BenchError;

/// Scripted window-event timestamps. Latency is settled minus event.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LatencyMarks {
    pub event_ms: u64,
    pub settled_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TimestampLog {
    pub pause: Option<LatencyMarks>,
    pub resume: Option<LatencyMarks>,
    #[serde(default)]
    pub frame_offset_ms: Vec<u64>,
    pub unplug_monitor_ms: Option<u64>,
    pub unplug_audio_ms: Option<u64>,
}

pub fn latency_ms(marks: &LatencyMarks) -> Result<u64, BenchError> {
    marks.settled_ms.checked_sub(marks.event_ms).ok_or_else(|| {
        BenchError::Message(format!(
            "settled timestamp {} is earlier than event {}",
            marks.settled_ms, marks.event_ms
        ))
    })
}

#[cfg(test)]
mod tests {
    use super::{latency_ms, LatencyMarks};

    #[test]
    fn pause_latency_is_the_timestamp_delta() {
        let marks = LatencyMarks {
            event_ms: 1_000,
            settled_ms: 1_080,
        };
        assert_eq!(latency_ms(&marks).expect("latency"), 80);
        let backwards = LatencyMarks {
            event_ms: 50,
            settled_ms: 40,
        };
        assert!(latency_ms(&backwards).is_err());
    }
}
