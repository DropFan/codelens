//! Formatter-neutral presentation data for diff reports.

use crate::insight::diff::DiffReport;

use super::health_regression::{health_regression, HealthRegressionPresentation};

#[derive(Debug, Clone)]
pub(crate) struct DiffPresentation {
    pub from: String,
    pub to: String,
    pub health: HealthRegressionPresentation,
    pub failed: bool,
    pub metrics: Vec<DiffMetricPresentation>,
    pub languages: Vec<DiffLanguagePresentation>,
}

#[derive(Debug, Clone)]
pub(crate) struct DiffMetricPresentation {
    pub label: &'static str,
    pub before: usize,
    pub after: usize,
    pub delta: i64,
    pub delta_text: String,
    pub percent_text: String,
}

#[derive(Debug, Clone)]
pub(crate) struct DiffLanguagePresentation {
    pub language: String,
    pub status: String,
    pub code_before: usize,
    pub code_after: usize,
    pub code_delta: i64,
    pub code_delta_text: String,
}

impl DiffPresentation {
    pub(crate) fn from_report(report: &DiffReport) -> Self {
        let delta = &report.delta;
        let metrics = [
            ("Files", &delta.files),
            ("Code", &delta.code),
            ("Comments", &delta.comment),
            ("Complexity", &delta.complexity),
            ("Functions", &delta.functions),
        ]
        .into_iter()
        .map(|(label, value)| {
            let delta = value.signed_delta();
            DiffMetricPresentation {
                label,
                before: value.from,
                after: value.to,
                delta,
                delta_text: signed_label(delta),
                percent_text: format!("{:+.1}%", value.percent),
            }
        })
        .collect();
        let languages = report
            .by_language
            .iter()
            .map(|language| DiffLanguagePresentation {
                language: language.language.clone(),
                status: language.status.to_string(),
                code_before: language.code.from,
                code_after: language.code.to,
                code_delta: language.code.signed_delta(),
                code_delta_text: signed_label(language.code.signed_delta()),
            })
            .collect();

        Self {
            from: report.from.clone(),
            to: report.to.clone(),
            health: health_regression(&report.health),
            failed: report.health.failed,
            metrics,
            languages,
        }
    }
}

fn signed_label(delta: i64) -> String {
    if delta >= 0 {
        format!("+{delta}")
    } else {
        delta.to_string()
    }
}
