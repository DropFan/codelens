//! Formatter-neutral presentation data for cost estimation reports.

use crate::insight::estimation::{EstimationComparison, EstimationReport, LanguageEstimation};

use super::OutputOptions;

pub(crate) const TOTAL_SLOC_LABEL: &str = "Total SLOC";
pub(crate) const EFFORT_LABEL: &str = "Effort (Person-Months)";
pub(crate) const SCHEDULE_LABEL: &str = "Schedule (Months)";
pub(crate) const PEOPLE_LABEL: &str = "People Required";
pub(crate) const COST_LABEL: &str = "Estimated Cost (USD)";

#[derive(Debug, Clone)]
pub(crate) struct EstimationPresentation {
    pub model: String,
    pub total_sloc: usize,
    pub total_sloc_display: String,
    pub effort_months: String,
    pub schedule_months: String,
    pub people_required: String,
    pub estimated_cost: String,
    pub estimated_cost_raw: f64,
    pub languages: Vec<EstimationLanguagePresentation>,
    pub params: Vec<EstimationParameterPresentation>,
}

#[derive(Debug, Clone)]
pub(crate) struct EstimationLanguagePresentation {
    pub language: String,
    pub code_lines: usize,
    pub code_lines_display: String,
    pub effort_months: String,
    pub effort_months_raw: f64,
    pub cost: String,
    pub cost_raw: f64,
}

#[derive(Debug, Clone)]
pub(crate) struct EstimationParameterPresentation {
    pub key: String,
    pub value: String,
}

#[derive(Debug, Clone)]
pub(crate) struct EstimationComparisonPresentation {
    pub total_sloc: usize,
    pub total_sloc_display: String,
    pub reports: Vec<EstimationPresentation>,
}

impl EstimationPresentation {
    pub(crate) fn from_report(report: &EstimationReport, options: &OutputOptions) -> Self {
        let mut languages = if options.summary_only {
            vec![]
        } else {
            report
                .by_language
                .iter()
                .map(EstimationLanguagePresentation::from)
                .collect::<Vec<_>>()
        };
        if let Some(top_n) = options.top_n {
            languages.truncate(top_n);
        }

        Self {
            model: report.model.clone(),
            total_sloc: report.total_sloc,
            total_sloc_display: format_number(report.total_sloc),
            effort_months: decimal(report.effort_months),
            schedule_months: decimal(report.schedule_months),
            people_required: decimal(report.people_required),
            estimated_cost: decimal(report.estimated_cost),
            estimated_cost_raw: report.estimated_cost,
            languages,
            params: report
                .params
                .iter()
                .map(|(key, value)| EstimationParameterPresentation {
                    key: key.clone(),
                    value: value.clone(),
                })
                .collect(),
        }
    }
}

impl From<&LanguageEstimation> for EstimationLanguagePresentation {
    fn from(language: &LanguageEstimation) -> Self {
        Self {
            language: language.language.clone(),
            code_lines: language.code_lines,
            code_lines_display: format_number(language.code_lines),
            effort_months: decimal(language.effort_months),
            effort_months_raw: language.effort_months,
            cost: decimal(language.cost),
            cost_raw: language.cost,
        }
    }
}

impl EstimationComparisonPresentation {
    pub(crate) fn from_report(report: &EstimationComparison) -> Self {
        let options = OutputOptions::default();
        Self {
            total_sloc: report.total_sloc,
            total_sloc_display: format_number(report.total_sloc),
            reports: report
                .reports
                .iter()
                .map(|report| EstimationPresentation::from_report(report, &options))
                .collect(),
        }
    }
}

fn decimal(value: f64) -> String {
    format!("{value:.2}")
}

fn format_number(value: usize) -> String {
    let text = value.to_string();
    let mut formatted = String::with_capacity(text.len() + text.len() / 3);
    for (index, character) in text.chars().rev().enumerate() {
        if index > 0 && index % 3 == 0 {
            formatted.push(',');
        }
        formatted.push(character);
    }
    formatted.chars().rev().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn presentation_owns_estimation_precision_and_labels() {
        let report = EstimationReport {
            model: "test".into(),
            total_sloc: 1_234,
            effort_months: 1.234,
            schedule_months: 2.345,
            people_required: 3.456,
            estimated_cost: 4_567.891,
            by_language: vec![],
            params: vec![],
        };
        let view = EstimationPresentation::from_report(&report, &OutputOptions::default());

        assert_eq!(view.total_sloc_display, "1,234");
        assert_eq!(view.effort_months, "1.23");
        assert_eq!(view.schedule_months, "2.35");
        assert_eq!(view.people_required, "3.46");
        assert_eq!(view.estimated_cost, "4567.89");
        assert_eq!(EFFORT_LABEL, "Effort (Person-Months)");
        assert_eq!(SCHEDULE_LABEL, "Schedule (Months)");
        assert_eq!(PEOPLE_LABEL, "People Required");
        assert_eq!(COST_LABEL, "Estimated Cost (USD)");
    }
}
