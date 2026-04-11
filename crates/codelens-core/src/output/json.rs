//! JSON output format.

use std::io::Write;

use crate::analyzer::stats::AnalysisResult;
use crate::error::Result;

use super::format::{OutputFormat, OutputOptions, Report};

/// JSON output formatter.
pub struct JsonOutput {
    pretty: bool,
}

impl JsonOutput {
    /// Create a new JSON output formatter.
    pub fn new(pretty: bool) -> Self {
        Self { pretty }
    }
}

impl OutputFormat for JsonOutput {
    fn name(&self) -> &'static str {
        "json"
    }

    fn extension(&self) -> &'static str {
        "json"
    }

    fn write(
        &self,
        report: &Report,
        options: &OutputOptions,
        writer: &mut dyn Write,
    ) -> Result<()> {
        match report {
            Report::Analysis(result) => self.write_analysis(result, options, writer),
            Report::Health(report) => self.write_json(report, writer),
            Report::Hotspot(report) => self.write_json(report, writer),
            Report::Trend(report) => self.write_json(report, writer),
            Report::Estimation(report) => self.write_json(report, writer),
            Report::EstimationComparison(report) => self.write_json(report, writer),
        }
    }
}

impl JsonOutput {
    fn write_analysis(
        &self,
        result: &AnalysisResult,
        _options: &OutputOptions,
        writer: &mut dyn Write,
    ) -> Result<()> {
        self.write_json(result, writer)
    }

    fn write_json<T: serde::Serialize>(&self, data: &T, writer: &mut dyn Write) -> Result<()> {
        if self.pretty {
            serde_json::to_writer_pretty(&mut *writer, data)?;
        } else {
            serde_json::to_writer(&mut *writer, data)?;
        }
        writeln!(writer)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::Report;
    use super::*;
    use crate::analyzer::stats::{FileStats, LineStats, Summary};
    use std::path::PathBuf;
    use std::time::Duration;

    fn make_test_result() -> AnalysisResult {
        AnalysisResult {
            files: vec![FileStats {
                path: PathBuf::from("test.rs"),
                language: "Rust".to_string(),
                lines: LineStats {
                    total: 100,
                    code: 80,
                    comment: 10,
                    blank: 10,
                },
                size: 2000,
                complexity: Default::default(),
            }],
            summary: Summary::from_file_stats(&[FileStats {
                path: PathBuf::from("test.rs"),
                language: "Rust".to_string(),
                lines: LineStats {
                    total: 100,
                    code: 80,
                    comment: 10,
                    blank: 10,
                },
                size: 2000,
                complexity: Default::default(),
            }]),
            elapsed: Duration::from_millis(100),
            scanned_files: 1,
            skipped_files: 0,
        }
    }

    #[test]
    fn test_json_output_name() {
        let output = JsonOutput::new(false);
        assert_eq!(output.name(), "json");
        assert_eq!(output.extension(), "json");
    }

    #[test]
    fn test_json_output_compact() {
        let output = JsonOutput::new(false);
        let result = make_test_result();
        let options = OutputOptions::default();

        let mut buffer = Vec::new();
        output
            .write(&Report::Analysis(result), &options, &mut buffer)
            .unwrap();

        let json_str = String::from_utf8(buffer).unwrap();
        assert!(json_str.contains("\"scanned_files\":1"));
        assert!(json_str.contains("\"language\":\"Rust\""));
        // Compact JSON should not have indentation
        assert!(!json_str.contains("  \""));
    }

    #[test]
    fn test_json_output_pretty() {
        let output = JsonOutput::new(true);
        let result = make_test_result();
        let options = OutputOptions::default();

        let mut buffer = Vec::new();
        output
            .write(&Report::Analysis(result), &options, &mut buffer)
            .unwrap();

        let json_str = String::from_utf8(buffer).unwrap();
        // Pretty JSON should have indentation
        assert!(json_str.contains("  "));
    }

    #[test]
    fn test_json_output_valid_json() {
        let output = JsonOutput::new(false);
        let result = make_test_result();
        let options = OutputOptions::default();

        let mut buffer = Vec::new();
        output
            .write(&Report::Analysis(result), &options, &mut buffer)
            .unwrap();

        let json_str = String::from_utf8(buffer).unwrap();
        // Should be valid JSON
        let parsed: serde_json::Value = serde_json::from_str(&json_str).unwrap();
        assert!(parsed.is_object());
    }
}
