//! CSV output format.

use std::io::Write;

use crate::analyzer::stats::AnalysisResult;
use crate::error::Result;

use super::format::{OutputFormat, OutputOptions};

/// CSV output formatter.
pub struct CsvOutput;

impl CsvOutput {
    /// Create a new CSV output formatter.
    pub fn new() -> Self {
        Self
    }
}

impl Default for CsvOutput {
    fn default() -> Self {
        Self::new()
    }
}

impl OutputFormat for CsvOutput {
    fn name(&self) -> &'static str {
        "csv"
    }

    fn extension(&self) -> &'static str {
        "csv"
    }

    fn write(
        &self,
        result: &AnalysisResult,
        _options: &OutputOptions,
        writer: &mut dyn Write,
    ) -> Result<()> {
        // Header
        writeln!(writer, "Language,Files,Code,Comment,Blank,Total,Size")?;

        // Data rows
        for (name, stats) in &result.summary.by_language {
            writeln!(
                writer,
                "{},{},{},{},{},{},{}",
                name,
                stats.files,
                stats.lines.code,
                stats.lines.comment,
                stats.lines.blank,
                stats.lines.total,
                stats.size
            )?;
        }

        Ok(())
    }
}
