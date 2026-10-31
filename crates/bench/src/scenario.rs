use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::error::BenchError;

/// The six scenarios from the specification. Durations are the soak or the
/// maximum observation window, not a claim that a run has been recorded.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ScenarioId {
    #[serde(rename = "idle-static")]
    IdleStatic,
    #[serde(rename = "h264-1080p30")]
    H2641080p30,
    #[serde(rename = "hevc-av1-4k60")]
    HevcAv14k60,
    #[serde(rename = "two-monitors")]
    TwoMonitors,
    #[serde(rename = "fullscreen-occlusion")]
    FullscreenOcclusion,
    #[serde(rename = "hot-unplug")]
    HotUnplug,
}

impl ScenarioId {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::IdleStatic => "idle-static",
            Self::H2641080p30 => "h264-1080p30",
            Self::HevcAv14k60 => "hevc-av1-4k60",
            Self::TwoMonitors => "two-monitors",
            Self::FullscreenOcclusion => "fullscreen-occlusion",
            Self::HotUnplug => "hot-unplug",
        }
    }

    pub fn parse(value: &str) -> Result<Self, BenchError> {
        scenarios()
            .iter()
            .find(|scenario| scenario.id.as_str() == value)
            .map(|scenario| scenario.id)
            .ok_or_else(|| BenchError::Message(format!("unknown scenario: {value}")))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScenarioKind {
    Soak,
    HotUnplug,
}

#[derive(Debug, Clone, Copy)]
pub struct Scenario {
    pub id: ScenarioId,
    pub summary: &'static str,
    pub duration: Duration,
    pub kind: ScenarioKind,
}

pub fn scenarios() -> &'static [Scenario] {
    const TEN_MINUTES: Duration = Duration::from_secs(600);
    &[
        Scenario {
            id: ScenarioId::IdleStatic,
            summary: "Idle 10 minutes with a static image.",
            duration: TEN_MINUTES,
            kind: ScenarioKind::Soak,
        },
        Scenario {
            id: ScenarioId::H2641080p30,
            summary: "1080p30 H.264 loop, 10 minutes.",
            duration: TEN_MINUTES,
            kind: ScenarioKind::Soak,
        },
        Scenario {
            id: ScenarioId::HevcAv14k60,
            summary: "4K60 HEVC or AV1 loop.",
            duration: TEN_MINUTES,
            kind: ScenarioKind::Soak,
        },
        Scenario {
            id: ScenarioId::TwoMonitors,
            summary: "Two monitors with different content.",
            duration: TEN_MINUTES,
            kind: ScenarioKind::Soak,
        },
        Scenario {
            id: ScenarioId::FullscreenOcclusion,
            summary: "Fullscreen on monitor 1 while monitor 2 stays visible.",
            duration: TEN_MINUTES,
            kind: ScenarioKind::Soak,
        },
        Scenario {
            id: ScenarioId::HotUnplug,
            summary: "Hot-unplug a monitor and an audio device. The duration is the maximum time the harness waits for both removal events.",
            duration: TEN_MINUTES,
            kind: ScenarioKind::HotUnplug,
        },
    ]
}

pub fn scenario(id: ScenarioId) -> &'static Scenario {
    let index = match id {
        ScenarioId::IdleStatic => 0,
        ScenarioId::H2641080p30 => 1,
        ScenarioId::HevcAv14k60 => 2,
        ScenarioId::TwoMonitors => 3,
        ScenarioId::FullscreenOcclusion => 4,
        ScenarioId::HotUnplug => 5,
    };
    &scenarios()[index]
}

#[cfg(test)]
mod tests {
    use super::{scenarios, ScenarioId, ScenarioKind};

    #[test]
    fn six_scenarios_are_unique() {
        let list = scenarios();
        assert_eq!(list.len(), 6);
        let mut ids: Vec<_> = list.iter().map(|item| item.id.as_str()).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), 6);
        assert_eq!(list[0].duration.as_secs(), 600);
        assert_eq!(list[1].duration.as_secs(), 600);
        assert!(matches!(list[5].kind, ScenarioKind::HotUnplug));
        assert_eq!(
            ScenarioId::parse("idle-static").expect("parse").as_str(),
            "idle-static"
        );
        assert!(ScenarioId::parse("nope").is_err());
    }
}
