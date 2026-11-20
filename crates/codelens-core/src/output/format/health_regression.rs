//! Formatter-neutral presentation data for health regressions.

use crate::insight::health::{
    ConfidenceLevel, HealthAssessment, HealthReport, HealthScope, RegressionReport,
};
use crate::insight::Grade;

use super::{
    HealthDimensionPresentation, HealthDirectoryPresentation, HealthFilePresentation,
    HealthLanguagePresentation, HealthPresentation,
};

#[derive(Debug, Clone)]
pub(crate) struct HealthRegressionPresentation {
    pub baseline: String,
    pub model: String,
    pub transition: String,
    pub score_delta: String,
    pub score_nonnegative: bool,
    pub scope_change: Option<(String, String)>,
    pub insufficient_data: bool,
    pub improved_files: usize,
    pub regressed_files: Vec<HealthRegressedFilePresentation>,
    pub languages: Vec<HealthLanguageChangePresentation>,
    pub verdict: HealthRegressionVerdict,
}

#[derive(Debug, Clone)]
pub(crate) struct HealthRegressedFilePresentation {
    pub path: String,
    pub before: String,
    pub after: String,
    pub is_new: bool,
    pub from_grade: Grade,
    pub to_grade: Grade,
}

#[derive(Debug, Clone)]
pub(crate) struct HealthLanguageChangePresentation {
    pub language: String,
    pub status: String,
    pub before: String,
    pub after: String,
    pub delta: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum HealthRegressionVerdict {
    InsufficientData,
    Regressed,
    NoRegression,
}

impl HealthPresentation {
    pub(crate) fn from_report(report: &HealthReport) -> Self {
        let directories: Vec<_> = report
            .by_directory
            .iter()
            .map(|directory| HealthDirectoryPresentation {
                path: directory.path.display().to_string(),
                score_display: format_score(directory.score),
                grade: directory.grade,
                file_count: directory.file_count,
            })
            .collect();
        Self {
            score: format_score(report.score),
            model: health_model_display_name(&report.model),
            scope: health_scope_display_name(report.scope).to_string(),
            confidence: format_confidence(report.confidence.level, report.confidence.coverage),
            tail_score: format_score(report.tail_risk.tail_score),
            failing_files: report.tail_risk.failing_files,
            failing_ratio: format!("{:.1}%", report.tail_risk.failing_ratio * 100.0),
            test_health: report.test_health.as_ref().map(|test| {
                let score = format_score(test.score);
                let confidence = format_confidence(test.confidence.level, test.confidence.coverage);
                if test.assessment == HealthAssessment::InsufficientData {
                    format!(
                        "Insufficient data (estimated score {score}); {} files; {confidence}",
                        test.file_count
                    )
                } else {
                    format!(
                        "{} ({score}); {} files; {confidence}",
                        test.grade, test.file_count
                    )
                }
            }),
            dimensions: report
                .dimensions
                .iter()
                .map(|dimension| HealthDimensionPresentation {
                    dimension: dimension.dimension,
                    score_display: format_score(dimension.score),
                    grade: dimension.grade,
                })
                .collect(),
            languages: report
                .by_language
                .iter()
                .map(|language| {
                    let estimated = language.assessment == HealthAssessment::InsufficientData;
                    HealthLanguagePresentation {
                        language: language.language.clone(),
                        score_display: format_score(language.score),
                        grade: language.grade,
                        grade_display: if estimated {
                            "Insufficient data".to_string()
                        } else {
                            language.grade.to_string()
                        },
                        estimated,
                        file_count: language.file_count,
                        code_lines: language.code_lines,
                        confidence: format!(
                            "{} ({:.0}%)",
                            language.confidence.level,
                            language.confidence.coverage * 100.0
                        ),
                        tail_score: format_score(language.tail_risk.tail_score),
                    }
                })
                .collect(),
            worst_directory: directories.first().cloned(),
            directories,
            worst_files: report
                .worst_files
                .iter()
                .map(|file| HealthFilePresentation {
                    path: file.path.display().to_string(),
                    score_display: format_score(file.score),
                    grade: file.grade,
                    top_issue: file.top_issue,
                })
                .collect(),
            regression: report.regression.as_ref().map(health_regression),
        }
    }
}

pub(super) fn health_regression(regression: &RegressionReport) -> HealthRegressionPresentation {
    HealthRegressionPresentation {
        baseline: regression.baseline.clone(),
        model: regression.model.clone(),
        transition: format!(
            "{} ({:.1}) → {} ({:.1})",
            regression.baseline_grade,
            regression.baseline_score,
            regression.current_grade,
            regression.current_score
        ),
        score_delta: format_optional_delta(Some(regression.score_delta)),
        score_nonnegative: regression.score_delta >= 0.0,
        scope_change: regression.scope_changed.then(|| {
            (
                regression.baseline_scope.to_string(),
                regression.current_scope.to_string(),
            )
        }),
        insufficient_data: regression.assessment == HealthAssessment::InsufficientData,
        improved_files: regression.improved_files,
        regressed_files: regression
            .regressed_files
            .iter()
            .map(|file| HealthRegressedFilePresentation {
                path: file.path.display().to_string(),
                before: if file.is_new {
                    "NEW".to_string()
                } else {
                    format!("{} ({:.1})", file.from_grade, file.from_score)
                },
                after: format!("{} ({:.1})", file.to_grade, file.to_score),
                is_new: file.is_new,
                from_grade: file.from_grade,
                to_grade: file.to_grade,
            })
            .collect(),
        languages: regression
            .by_language
            .iter()
            .map(|language| HealthLanguageChangePresentation {
                language: language.language.clone(),
                status: language.status.to_string(),
                before: format_optional_health(language.from_grade, language.from_score),
                after: format_optional_health(language.to_grade, language.to_score),
                delta: format_optional_delta(language.score_delta),
            })
            .collect(),
        verdict: if regression.assessment == HealthAssessment::InsufficientData {
            HealthRegressionVerdict::InsufficientData
        } else if regression.failed {
            HealthRegressionVerdict::Regressed
        } else {
            HealthRegressionVerdict::NoRegression
        },
    }
}

pub(crate) fn format_optional_health(grade: Option<Grade>, score: Option<f64>) -> String {
    match (grade, score) {
        (Some(grade), Some(score)) => format!("{grade} ({score:.1})"),
        _ => "-".to_string(),
    }
}

pub(crate) fn format_optional_delta(delta: Option<f64>) -> String {
    match delta {
        Some(delta) if delta >= 0.0 => format!("+{delta:.1}"),
        Some(delta) => format!("{delta:.1}"),
        None => "-".to_string(),
    }
}

pub(super) fn format_score(score: f64) -> String {
    format!("{score:.1}")
}

pub(super) fn format_confidence(level: ConfidenceLevel, coverage: f64) -> String {
    format!("{level} ({:.1}% measured)", coverage * 100.0)
}

pub(crate) fn health_model_display_name(model: &str) -> String {
    match model {
        "default" => "v1".to_string(),
        "default-no-dup" => "v1 (no duplication scan)".to_string(),
        "default-v2" => "v2".to_string(),
        "default-v2-no-dup" => "v2 (no duplication scan)".to_string(),
        other => other.to_string(),
    }
}

pub(super) fn health_scope_display_name(scope: HealthScope) -> &'static str {
    match scope {
        HealthScope::Production => "Production code",
        HealthScope::TestsOnly => "Test code only",
        HealthScope::LegacyAll => "All analyzed files (legacy v1 scope)",
        HealthScope::Empty => "No health-supported files",
    }
}
