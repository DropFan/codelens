//! Output format trait definition.

use std::io::Write;

use crate::analyzer::stats::AnalysisResult;
use crate::config::{OutputFormatType, SortBy};
use crate::error::Result;
use crate::insight::coupling::CouplingReport;
use crate::insight::diff::DiffReport;
use crate::insight::estimation::{EstimationComparison, EstimationReport};
use crate::insight::health::HealthReport;
use crate::insight::hotspot::HotspotReport;
use crate::insight::trend::TrendReport;
use crate::insight::Grade;

mod analysis;
mod coupling;
mod diff;
mod estimation;
mod health_regression;
mod hotspot;
mod trend;

pub(crate) use analysis::AnalysisPresentation;
pub(crate) use coupling::{
    CouplingFilterPresentation, CouplingPairPresentation, CouplingPresentation,
};
pub(crate) use diff::{DiffLanguagePresentation, DiffMetricPresentation, DiffPresentation};
pub(crate) use estimation::{
    EstimationComparisonPresentation, EstimationLanguagePresentation, EstimationPresentation,
    COST_LABEL, EFFORT_LABEL, PEOPLE_LABEL, SCHEDULE_LABEL, TOTAL_SLOC_LABEL,
};
pub(crate) use health_regression::{
    health_model_display_name, HealthLanguageChangePresentation, HealthRegressedFilePresentation,
    HealthRegressionPresentation, HealthRegressionVerdict,
};
pub(crate) use hotspot::{
    HotspotFilePresentation, HotspotFunctionFilePresentation, HotspotKnowledgeIslandPresentation,
    HotspotPresentation,
};
pub(crate) use trend::{TrendLanguagePresentation, TrendMetricPresentation, TrendPresentation};

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ReportKind {
    Analysis,
    Health,
    Hotspot,
    Coupling,
    Trend,
    Diff,
    Estimation,
    EstimationComparison,
    Combined,
}

impl ReportKind {
    fn name(self) -> &'static str {
        match self {
            Self::Analysis => "analysis",
            Self::Health => "health",
            Self::Hotspot => "hotspot",
            Self::Coupling => "coupling",
            Self::Trend => "trend",
            Self::Diff => "diff",
            Self::Estimation => "estimation",
            Self::EstimationComparison => "estimation comparison",
            Self::Combined => "combined",
        }
    }
}

impl Report {
    pub(crate) fn kind(&self) -> ReportKind {
        match self {
            Self::Analysis(_) => ReportKind::Analysis,
            Self::Health(_) => ReportKind::Health,
            Self::Hotspot(_) => ReportKind::Hotspot,
            Self::Coupling(_) => ReportKind::Coupling,
            Self::Trend(_) => ReportKind::Trend,
            Self::Diff(_) => ReportKind::Diff,
            Self::Estimation(_) => ReportKind::Estimation,
            Self::EstimationComparison(_) => ReportKind::EstimationComparison,
            Self::Combined(_) => ReportKind::Combined,
        }
    }
}

/// Semantic fidelity promised by a report/format pair.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FormatCapability {
    FullReport,
    AggregateProjection(&'static str),
    Unsupported(&'static str),
}

pub(crate) fn report_format_capability(
    format: OutputFormatType,
    report: ReportKind,
) -> FormatCapability {
    use FormatCapability::{AggregateProjection, FullReport, Unsupported};
    use OutputFormatType::{Badge, Console, Csv, Html, Json, Markdown, OpenMetrics, Sarif};
    use ReportKind::{
        Analysis, Combined, Coupling, Diff, Estimation, EstimationComparison, Health, Hotspot,
        Trend,
    };

    match format {
        Console | Json | Markdown | Html => FullReport,
        Csv => match report {
            Hotspot | Coupling | Estimation | EstimationComparison => FullReport,
            Analysis => AggregateProjection("selected language/file/directory table"),
            Health => AggregateProjection("worst-file table"),
            Trend => AggregateProjection("metric delta table"),
            Diff => AggregateProjection("health and metric delta table"),
            Combined => AggregateProjection("analysis table; health and estimation omitted"),
        },
        OpenMetrics => AggregateProjection("scrape-oriented numeric metrics"),
        Badge => AggregateProjection("one shields.io badge value"),
        Sarif => match report {
            Health | Hotspot | Coupling | Diff | Combined => {
                AggregateProjection("actionable findings")
            }
            Analysis | Trend | Estimation | EstimationComparison => {
                Unsupported("report has no SARIF finding semantics")
            }
        },
    }
}

pub(crate) fn validate_report_format(
    format: OutputFormatType,
    report: &Report,
) -> Result<FormatCapability> {
    let capability = report_format_capability(format, report.kind());
    if let FormatCapability::Unsupported(reason) = capability {
        return Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            format!(
                "{} reports are not supported by {} output: {reason}",
                report.kind().name(),
                format_name(format)
            ),
        )
        .into());
    }
    Ok(capability)
}

fn format_name(format: OutputFormatType) -> &'static str {
    match format {
        OutputFormatType::Console => "console",
        OutputFormatType::Json => "json",
        OutputFormatType::Csv => "csv",
        OutputFormatType::Markdown => "markdown",
        OutputFormatType::Html => "html",
        OutputFormatType::OpenMetrics => "openmetrics",
        OutputFormatType::Badge => "badge",
        OutputFormatType::Sarif => "sarif",
    }
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
    pub dimensions: Vec<HealthDimensionPresentation>,
    pub languages: Vec<HealthLanguagePresentation>,
    pub directories: Vec<HealthDirectoryPresentation>,
    pub worst_directory: Option<HealthDirectoryPresentation>,
    pub worst_files: Vec<HealthFilePresentation>,
    pub regression: Option<HealthRegressionPresentation>,
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

    #[test]
    fn report_format_matrix_covers_full_projection_and_unsupported_semantics() {
        assert_eq!(
            report_format_capability(OutputFormatType::Json, ReportKind::Combined),
            FormatCapability::FullReport
        );
        assert_eq!(
            report_format_capability(OutputFormatType::Csv, ReportKind::Combined),
            FormatCapability::AggregateProjection("analysis table; health and estimation omitted")
        );
        assert_eq!(
            report_format_capability(OutputFormatType::Sarif, ReportKind::Trend),
            FormatCapability::Unsupported("report has no SARIF finding semantics")
        );
    }

    #[test]
    fn every_report_and_format_pair_has_an_explicit_capability() {
        let formats = [
            OutputFormatType::Console,
            OutputFormatType::Json,
            OutputFormatType::Csv,
            OutputFormatType::Markdown,
            OutputFormatType::Html,
            OutputFormatType::OpenMetrics,
            OutputFormatType::Badge,
            OutputFormatType::Sarif,
        ];
        let reports = [
            ReportKind::Analysis,
            ReportKind::Health,
            ReportKind::Hotspot,
            ReportKind::Coupling,
            ReportKind::Trend,
            ReportKind::Diff,
            ReportKind::Estimation,
            ReportKind::EstimationComparison,
            ReportKind::Combined,
        ];

        let capabilities = formats
            .into_iter()
            .flat_map(|format| {
                reports
                    .into_iter()
                    .map(move |report| report_format_capability(format, report))
            })
            .count();
        assert_eq!(capabilities, formats.len() * reports.len());
    }
}
