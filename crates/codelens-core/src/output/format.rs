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
    pub worst_directory: Option<HealthRiskItem>,
    pub worst_files: Vec<HealthRiskItem>,
}

#[derive(Debug, Clone)]
pub(crate) struct HealthRiskItem {
    pub path: String,
    pub score: String,
    pub grade: Grade,
    pub detail: String,
}

impl HealthPresentation {
    pub(crate) fn from_report(report: &HealthReport) -> Self {
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
            worst_directory: report.by_directory.first().map(|directory| HealthRiskItem {
                path: directory.path.display().to_string(),
                score: format_score(directory.score),
                grade: directory.grade,
                detail: format!("{} files", directory.file_count),
            }),
            worst_files: report
                .worst_files
                .iter()
                .map(|file| HealthRiskItem {
                    path: file.path.display().to_string(),
                    score: format_score(file.score),
                    grade: file.grade,
                    detail: file.top_issue.to_string(),
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
