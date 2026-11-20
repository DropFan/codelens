//! Output format trait definition.

use std::io::Write;

use crate::analyzer::stats::AnalysisResult;
use crate::config::SortBy;
use crate::error::Result;
use crate::insight::coupling::CouplingReport;
use crate::insight::diff::DiffReport;
use crate::insight::estimation::{EstimationComparison, EstimationReport};
use crate::insight::health::{HealthAssessment, HealthReport, HealthScope};
use crate::insight::hotspot::HotspotReport;
use crate::insight::trend::TrendReport;
use crate::insight::Grade;

/// Unified report type for all output formatters.
#[derive(Debug, Clone)]
pub enum Report {
    Analysis(AnalysisResult),
    Health(HealthReport),
    Hotspot(HotspotReport),
    Coupling(CouplingReport),
    Trend(TrendReport),
    Diff(Box<DiffReport>),
    Estimation(EstimationReport),
    EstimationComparison(EstimationComparison),
    /// Default-command bundle: stats + health + estimation as ONE document,
    /// so machine formats (JSON/HTML/CSV) stay parseable.
    Combined(Box<CombinedReport>),
}

/// The default command's combined output.
#[derive(Debug, Clone, serde::Serialize)]
pub struct CombinedReport {
    pub analysis: AnalysisResult,
    pub health: HealthReport,
    pub estimation: EstimationComparison,
}

/// Formatter-neutral statistics used by human-readable analysis reports.
///
/// This view owns field selection and derived values while console, Markdown,
/// and HTML remain responsible for their own layout and number decoration.
#[derive(Debug, Clone)]
pub(crate) struct AnalysisPresentation {
    pub total_files: usize,
    pub code_lines: usize,
    pub comment_lines: usize,
    pub blank_lines: usize,
    pub total_lines: usize,
    pub total_size: u64,
    pub language_count: usize,
    pub functions: usize,
    pub complexity: Option<AnalysisComplexityPresentation>,
    pub tests: Option<AnalysisTestPresentation>,
    pub duplication: Option<AnalysisDuplicationPresentation>,
    pub tokens_est: u64,
    pub languages: Vec<AnalysisLanguagePresentation>,
    pub elapsed_secs: f64,
    pub scanned_files: usize,
    pub skipped_files: usize,
    pub error_files: usize,
}

#[derive(Debug, Clone)]
pub(crate) struct AnalysisComplexityPresentation {
    pub cyclomatic: usize,
    pub cognitive: usize,
}

#[derive(Debug, Clone)]
pub(crate) struct AnalysisTestPresentation {
    pub files: usize,
    pub code_lines: usize,
    pub code_ratio: Option<f64>,
}

#[derive(Debug, Clone)]
pub(crate) struct AnalysisDuplicationPresentation {
    pub unique_lines: usize,
    pub dryness_percent: f64,
}

#[derive(Debug, Clone)]
pub(crate) struct AnalysisLanguagePresentation {
    pub name: String,
    pub files: usize,
    pub code_lines: usize,
    pub comment_lines: usize,
    pub blank_lines: usize,
    pub total_lines: usize,
    pub tokens_est: u64,
}

impl AnalysisPresentation {
    pub(crate) fn from_result(result: &AnalysisResult, top_n: Option<usize>) -> Self {
        let summary = &result.summary;
        let production_code = summary.lines.code.saturating_sub(summary.test_lines.code);
        let tests = (summary.test_files > 0).then(|| AnalysisTestPresentation {
            files: summary.test_files,
            code_lines: summary.test_lines.code,
            code_ratio: (production_code > 0)
                .then_some(summary.test_lines.code as f64 / production_code as f64),
        });
        let duplication = (summary.uloc > 0).then(|| {
            let non_blank = summary.lines.total.saturating_sub(summary.lines.blank);
            AnalysisDuplicationPresentation {
                unique_lines: summary.uloc,
                dryness_percent: if non_blank > 0 {
                    summary.uloc as f64 / non_blank as f64 * 100.0
                } else {
                    100.0
                },
            }
        });
        let mut languages: Vec<_> = summary
            .by_language
            .iter()
            .map(|(name, stats)| AnalysisLanguagePresentation {
                name: name.clone(),
                files: stats.files,
                code_lines: stats.lines.code,
                comment_lines: stats.lines.comment,
                blank_lines: stats.lines.blank,
                total_lines: stats.lines.total,
                tokens_est: stats.tokens_est,
            })
            .collect();
        if let Some(limit) = top_n {
            languages.truncate(limit);
        }

        Self {
            total_files: summary.total_files,
            code_lines: summary.lines.code,
            comment_lines: summary.lines.comment,
            blank_lines: summary.lines.blank,
            total_lines: summary.lines.total,
            total_size: summary.total_size,
            language_count: summary.by_language.len(),
            functions: summary.complexity.functions,
            complexity: (summary.complexity.cyclomatic > 0).then_some({
                AnalysisComplexityPresentation {
                    cyclomatic: summary.complexity.cyclomatic,
                    cognitive: summary.complexity.cognitive,
                }
            }),
            tests,
            duplication,
            tokens_est: summary.tokens_est,
            languages,
            elapsed_secs: result.elapsed.as_secs_f64(),
            scanned_files: result.scanned_files,
            skipped_files: result.skipped_files,
            error_files: result.error_files,
        }
    }
}

/// Formatter-neutral health summary used by human-readable outputs.
///
/// Machine-readable formats serialize `HealthReport` directly and never use
/// this view, which keeps their schema stable while console, Markdown, and
/// HTML share the same wording and precision.
#[derive(Debug, Clone)]
pub(crate) struct HealthPresentation {
    pub score: String,
    pub model: String,
    pub scope: String,
    pub confidence: String,
    pub tail_score: String,
    pub failing_files: usize,
    pub failing_ratio: String,
    pub test_health: Option<String>,
    pub dimensions: Vec<HealthDimensionPresentation>,
    pub languages: Vec<HealthLanguagePresentation>,
    pub directories: Vec<HealthDirectoryPresentation>,
    pub worst_directory: Option<HealthDirectoryPresentation>,
    pub worst_files: Vec<HealthFilePresentation>,
}

#[derive(Debug, Clone)]
pub(crate) struct HealthDimensionPresentation {
    pub dimension: crate::insight::scoring::HealthDimension,
    pub score_display: String,
    pub grade: Grade,
}

#[derive(Debug, Clone)]
pub(crate) struct HealthLanguagePresentation {
    pub language: String,
    pub score_display: String,
    pub grade: Grade,
    pub grade_display: String,
    pub estimated: bool,
    pub file_count: usize,
    pub code_lines: usize,
    pub confidence: String,
    pub tail_score: String,
}

#[derive(Debug, Clone)]
pub(crate) struct HealthDirectoryPresentation {
    pub path: String,
    pub score_display: String,
    pub grade: Grade,
    pub file_count: usize,
}

#[derive(Debug, Clone)]
pub(crate) struct HealthFilePresentation {
    pub path: String,
    pub score_display: String,
    pub grade: Grade,
    pub top_issue: crate::insight::scoring::HealthDimension,
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
        }
    }
}

pub(crate) fn format_score(score: f64) -> String {
    format!("{score:.1}")
}

pub(crate) fn format_confidence(
    level: crate::insight::health::ConfidenceLevel,
    coverage: f64,
) -> String {
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

fn health_scope_display_name(scope: HealthScope) -> &'static str {
    match scope {
        HealthScope::Production => "Production code",
        HealthScope::TestsOnly => "Test code only",
        HealthScope::LegacyAll => "All analyzed files (legacy v1 scope)",
        HealthScope::Empty => "No health-supported files",
    }
}

/// Sort file stats by the given key (descending for numeric keys,
/// ascending for names) and truncate to `top_n`. Shared by the
/// formatters that render per-file tables (`--by-file`).
pub fn sorted_files(
    files: &[crate::analyzer::stats::FileStats],
    sort_by: SortBy,
    top_n: Option<usize>,
) -> Vec<&crate::analyzer::stats::FileStats> {
    use std::cmp::Reverse;

    let mut sorted: Vec<_> = files.iter().collect();
    match sort_by {
        // `Files` has no per-file meaning; fall back to total lines
        SortBy::Lines | SortBy::Files => sorted.sort_by_key(|f| Reverse(f.lines.total)),
        SortBy::Code => sorted.sort_by_key(|f| Reverse(f.lines.code)),
        SortBy::Name => sorted.sort_by(|a, b| a.path.cmp(&b.path)),
        SortBy::Size => sorted.sort_by_key(|f| Reverse(f.size)),
    }
    if let Some(n) = top_n {
        sorted.truncate(n);
    }
    sorted
}

/// Trait for output formatters.
pub trait OutputFormat: Send + Sync {
    /// Get the format name.
    fn name(&self) -> &'static str;

    /// Get the file extension.
    fn extension(&self) -> &'static str;

    /// Write the report to the writer.
    fn write(&self, report: &Report, options: &OutputOptions, writer: &mut dyn Write)
        -> Result<()>;
}

/// Output options.
#[derive(Debug, Clone)]
pub struct OutputOptions {
    /// Show only summary.
    pub summary_only: bool,
    /// Show per-file statistics.
    pub by_file: bool,
    /// Show per-directory statistics (tree view).
    pub by_dir: bool,
    /// Directory depth for --by-dir.
    pub dir_depth: usize,
    /// Show estimated LLM token counts.
    pub show_tokens: bool,
    /// Sort order.
    pub sort_by: SortBy,
    /// Limit to top N results.
    pub top_n: Option<usize>,
    /// Colorize output (for console).
    pub colorize: bool,
    /// Show git information.
    pub show_git_info: bool,
}

impl Default for OutputOptions {
    fn default() -> Self {
        Self {
            summary_only: false,
            by_file: false,
            by_dir: false,
            dir_depth: 3,
            show_tokens: false,
            sort_by: SortBy::Lines,
            top_n: None,
            colorize: true,
            show_git_info: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_output_options_default() {
        let options = OutputOptions::default();
        assert!(!options.summary_only);
        assert!(matches!(options.sort_by, SortBy::Lines));
        assert!(options.top_n.is_none());
        assert!(options.colorize);
        assert!(!options.show_git_info);
    }

    #[test]
    fn test_output_options_custom() {
        let options = OutputOptions {
            summary_only: true,
            by_file: false,
            by_dir: false,
            dir_depth: 3,
            show_tokens: false,
            sort_by: SortBy::Code,
            top_n: Some(10),
            colorize: false,
            show_git_info: true,
        };
        assert!(options.summary_only);
        assert!(matches!(options.sort_by, SortBy::Code));
        assert_eq!(options.top_n, Some(10));
        assert!(!options.colorize);
        assert!(options.show_git_info);
    }
}
