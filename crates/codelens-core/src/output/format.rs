//! Output format trait definition.

use std::io::Write;

use crate::analyzer::stats::AnalysisResult;
use crate::config::SortBy;
use crate::error::Result;
use crate::insight::estimation::{EstimationComparison, EstimationReport};
use crate::insight::health::HealthReport;
use crate::insight::hotspot::HotspotReport;
use crate::insight::trend::TrendReport;

/// Unified report type for all output formatters.
#[derive(Debug, Clone)]
pub enum Report {
    Analysis(AnalysisResult),
    Health(HealthReport),
    Hotspot(HotspotReport),
    Trend(TrendReport),
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
