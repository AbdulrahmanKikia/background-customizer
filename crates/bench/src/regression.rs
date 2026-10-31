use crate::error::BenchError;
use crate::model::{MeasureStatus, ResultsDocument, TargetId};
use crate::scenario::ScenarioId;

const IDLE_CPU_EPSILON: f64 = 1e-6;

#[derive(Debug, Clone, PartialEq)]
pub enum IdleComparison {
    Skipped {
        target: TargetId,
        reason: String,
    },
    WithinBaseline {
        target: TargetId,
        baseline: f64,
        candidate: f64,
    },
    Increased {
        target: TargetId,
        baseline: f64,
        candidate: f64,
    },
}

pub fn compare_idle_cpu(
    baseline: &ResultsDocument,
    candidate: &ResultsDocument,
) -> Result<Vec<IdleComparison>, BenchError> {
    baseline.validate()?;
    candidate.validate()?;
    let mut comparisons = Vec::new();
    for base_target in &baseline.targets {
        let Some(next_target) = candidate
            .targets
            .iter()
            .find(|target| target.id == base_target.id)
        else {
            comparisons.push(IdleComparison::Skipped {
                target: base_target.id,
                reason: "candidate has no matching target".to_string(),
            });
            continue;
        };
        comparisons.push(compare_target(
            base_target.id,
            idle_cpu(&base_target.scenarios),
            idle_cpu(&next_target.scenarios),
        )?);
    }
    Ok(comparisons)
}

pub fn idle_cpu_regressed(comparisons: &[IdleComparison]) -> bool {
    comparisons
        .iter()
        .any(|comparison| matches!(comparison, IdleComparison::Increased { .. }))
}

fn compare_target(
    target: TargetId,
    baseline: IdleValue,
    candidate: IdleValue,
) -> Result<IdleComparison, BenchError> {
    match (baseline, candidate) {
        (IdleValue::Number(baseline), IdleValue::Number(candidate)) => {
            if !baseline.is_finite() || !candidate.is_finite() {
                return Err(BenchError::Message(format!(
                    "{} idle CPU is not a finite number",
                    target.as_str()
                )));
            }
            if candidate > baseline + IDLE_CPU_EPSILON {
                Ok(IdleComparison::Increased {
                    target,
                    baseline,
                    candidate,
                })
            } else {
                Ok(IdleComparison::WithinBaseline {
                    target,
                    baseline,
                    candidate,
                })
            }
        }
        (IdleValue::Missing(reason), _) => Ok(IdleComparison::Skipped { target, reason }),
        (_, IdleValue::Missing(reason)) => Ok(IdleComparison::Skipped { target, reason }),
    }
}

enum IdleValue {
    Number(f64),
    Missing(String),
}

fn idle_cpu(scenarios: &[crate::model::ScenarioResult]) -> IdleValue {
    let Some(scenario) = scenarios
        .iter()
        .find(|scenario| scenario.id == ScenarioId::IdleStatic)
    else {
        return IdleValue::Missing("idle-static scenario is absent".to_string());
    };
    if scenario.status != MeasureStatus::Recorded {
        return IdleValue::Missing(
            scenario
                .reason
                .clone()
                .unwrap_or_else(|| "idle-static was not recorded".to_string()),
        );
    }
    match scenario.summary.cpu_percent_mean {
        Some(value) => IdleValue::Number(value),
        None => IdleValue::Missing("idle-static has no CPU mean".to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::{compare_idle_cpu, idle_cpu_regressed, IdleComparison};
    use crate::model::{
        missing_baselines, MeasureStatus, MetricSummary, ResultsDocument, ScenarioResult, TargetId,
        TargetResults, SCHEMA_VERSION,
    };
    use crate::scenario::ScenarioId;

    fn document_with_idle(cpu: Option<f64>, status: MeasureStatus) -> ResultsDocument {
        let mut summary = MetricSummary::empty();
        summary.cpu_percent_mean = cpu;
        ResultsDocument {
            schema_version: SCHEMA_VERSION,
            kind: crate::model::DocumentKind::Run,
            recorded_at: None,
            note: "fixture".to_string(),
            targets: vec![TargetResults {
                id: TargetId::WallpaperEngine,
                status,
                reason: None,
                scenarios: vec![ScenarioResult {
                    id: ScenarioId::IdleStatic,
                    status,
                    reason: None,
                    summary,
                    samples: Vec::new(),
                    device_events: Vec::new(),
                    notes: Vec::new(),
                }],
            }],
        }
    }

    #[test]
    fn higher_idle_cpu_is_a_regression() {
        let baseline = document_with_idle(Some(1.0), MeasureStatus::Recorded);
        let candidate = document_with_idle(Some(1.5), MeasureStatus::Recorded);
        let comparisons = compare_idle_cpu(&baseline, &candidate).expect("compare");
        assert!(idle_cpu_regressed(&comparisons));
        assert!(matches!(
            comparisons[0],
            IdleComparison::Increased { baseline: 1.0, .. }
        ));
    }

    #[test]
    fn equal_or_lower_idle_cpu_passes() {
        let baseline = document_with_idle(Some(2.0), MeasureStatus::Recorded);
        let candidate = document_with_idle(Some(2.0), MeasureStatus::Recorded);
        let comparisons = compare_idle_cpu(&baseline, &candidate).expect("compare");
        assert!(!idle_cpu_regressed(&comparisons));
    }

    #[test]
    fn missing_baseline_numbers_are_not_a_regression() {
        let baseline = missing_baselines();
        let candidate = document_with_idle(Some(50.0), MeasureStatus::Recorded);
        let comparisons = compare_idle_cpu(&baseline, &candidate).expect("compare");
        assert!(!idle_cpu_regressed(&comparisons));
        assert!(matches!(comparisons[0], IdleComparison::Skipped { .. }));
    }
}
