//! Pure health-gate decisions, kept separate from report rendering.

use anyhow::Result;
use codelens_core::insight::health::{HealthAssessment, HealthReport};

pub(super) fn failure_reason(
    report: &HealthReport,
    threshold: Option<f64>,
    threshold_label: Option<&str>,
    fail_on_regression: bool,
    baseline_label: Option<&str>,
) -> Option<String> {
    if threshold.is_some() && report.assessment == HealthAssessment::InsufficientData {
        return Some(format!(
            "v2 measured only {:.0}% of the configured health model; score {:.1} is an estimate and cannot satisfy --fail-under",
            report.confidence.coverage * 100.0,
            report.score,
        ));
    }
    if let Some(threshold) = threshold {
        if report.score < threshold {
            return Some(format!(
                "health score {:.1} (grade {}) is below --fail-under threshold {}",
                report.score,
                report.grade,
                threshold_label.unwrap_or_default(),
            ));
        }
    }
    if fail_on_regression {
        if let Some(regression) = &report.regression {
            if regression.gate_failed() {
                let baseline = baseline_label.unwrap_or_default();
                return Some(
                    if regression.assessment == HealthAssessment::InsufficientData {
                        format!(
                        "comparison against '{baseline}' has insufficient measurement coverage; its score delta is an estimate and cannot satisfy --fail-on-regression"
                    )
                    } else {
                        format!("health regressed against baseline '{baseline}'")
                    },
                );
            }
        }
    }
    None
}

/// Parse a `--fail-under` threshold: a grade letter (A/B/C/D, using the
/// scoring model's grade boundaries) or a numeric score (0-100).
pub(super) fn parse_fail_under(input: &str) -> Result<f64> {
    match input.trim() {
        "A" | "a" => Ok(90.0),
        "B" | "b" => Ok(80.0),
        "C" | "c" => Ok(70.0),
        "D" | "d" => Ok(60.0),
        other => {
            if let Ok(score) = other.parse::<f64>() {
                if (0.0..=100.0).contains(&score) {
                    return Ok(score);
                }
            }
            anyhow::bail!(
                "invalid --fail-under value '{other}': expected a grade (A/B/C/D) or a score between 0 and 100"
            )
        }
    }
}
