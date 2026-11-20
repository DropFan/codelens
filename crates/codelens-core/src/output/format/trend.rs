//! Formatter-neutral presentation data for trend reports.

use crate::insight::{trend::TrendReport, DeltaValue};

#[derive(Debug, Clone)]
pub(crate) struct TrendPresentation {
    pub from_date: String,
    pub from_label: String,
    pub to_date: String,
    pub to_label: String,
    pub metric_rows: Vec<TrendMetricPresentation>,
    pub language_rows: Vec<TrendLanguagePresentation>,
}

#[derive(Debug, Clone)]
pub(crate) struct TrendMetricPresentation {
    pub name: &'static str,
    pub start: usize,
    pub end: usize,
    pub signed_change: i64,
    pub signed_change_text: String,
    pub percent_change_text: String,
}

#[derive(Debug, Clone)]
pub(crate) struct TrendLanguagePresentation {
    pub name: String,
    pub marker: String,
    pub code_start: usize,
    pub code_end: usize,
    pub signed_code_change: i64,
    pub signed_code_change_text: String,
}

impl TrendPresentation {
    pub(crate) fn from_report(report: &TrendReport) -> Self {
        let delta = &report.delta;
        let metric_row = |name: &'static str, value: &DeltaValue<usize>| {
            let delta = value.signed_delta();
            TrendMetricPresentation {
                name,
                start: value.from,
                end: value.to,
                signed_change: delta,
                signed_change_text: signed_label(delta),
                percent_change_text: format!("{:+.1}%", value.percent),
            }
        };
        let metric_rows = vec![
            metric_row("Files", &delta.files),
            metric_row("Lines", &delta.lines),
            metric_row("Code", &delta.code),
            metric_row("Comments", &delta.comment),
            metric_row("Blank", &delta.blank),
            metric_row("Complexity", &delta.complexity),
            metric_row("Functions", &delta.functions),
        ];
        let mut language_rows = Vec::with_capacity(report.by_language.len());
        for language in &report.by_language {
            let signed_code_change = language.code.signed_delta();
            language_rows.push(TrendLanguagePresentation {
                name: language.language.clone(),
                marker: language.status.to_string(),
                code_start: language.code.from,
                code_end: language.code.to,
                signed_code_change,
                signed_code_change_text: signed_label(signed_code_change),
            });
        }

        Self {
            from_date: report.from.timestamp.format("%Y-%m-%d").to_string(),
            from_label: report.from.label.clone().unwrap_or_default(),
            to_date: report.to.timestamp.format("%Y-%m-%d").to_string(),
            to_label: report.to.label.clone().unwrap_or_default(),
            metric_rows,
            language_rows,
        }
    }
}

fn signed_label(delta: i64) -> String {
    if delta > 0 {
        format!("+{delta}")
    } else {
        delta.to_string()
    }
}
