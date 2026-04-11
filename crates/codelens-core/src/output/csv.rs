//! CSV output format.

use std::io::Write;

use crate::analyzer::stats::AnalysisResult;
use crate::error::Result;

use super::format::{OutputFormat, OutputOptions, Report};

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
        report: &Report,
        options: &OutputOptions,
        writer: &mut dyn Write,
    ) -> Result<()> {
        match report {
            Report::Analysis(result) => self.write_analysis(result, options, writer),
            Report::Health(_) => {
                writeln!(writer, "# Health report CSV output not yet implemented")?;
                Ok(())
            }
            Report::Hotspot(_) => {
                writeln!(writer, "# Hotspot report CSV output not yet implemented")?;
                Ok(())
            }
            Report::Trend(_) => {
                writeln!(writer, "# Trend report CSV output not yet implemented")?;
                Ok(())
            }
        }
    }
}

impl CsvOutput {
    fn write_analysis(
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

#[cfg(test)]
mod tests {
    use super::*;
    use super::Report;
    use crate::analyzer::stats::{FileStats, LineStats, Summary};
    use std::path::PathBuf;
    use std::time::Duration;

    fn make_test_result() -> AnalysisResult {
        let files = vec![
            FileStats {
                path: PathBuf::from("main.rs"),
                language: "Rust".to_string(),
                lines: LineStats {
                    total: 100,
                    code: 80,
                    comment: 10,
                    blank: 10,
                },
                size: 2000,
                complexity: Default::default(),
            },
            FileStats {
                path: PathBuf::from("test.py"),
                language: "Python".to_string(),
                lines: LineStats {
                    total: 50,
                    code: 40,
                    comment: 5,
                    blank: 5,
                },
                size: 1000,
                complexity: Default::default(),
            },
        ];
        AnalysisResult {
            summary: Summary::from_file_stats(&files),
            files,
            elapsed: Duration::from_millis(100),
            scanned_files: 2,
            skipped_files: 0,
        }
    }

    #[test]
    fn test_csv_output_name() {
        let output = CsvOutput::new();
        assert_eq!(output.name(), "csv");
        assert_eq!(output.extension(), "csv");
    }

    #[test]
    fn test_csv_output_header() {
        let output = CsvOutput;
        let result = make_test_result();
        let options = OutputOptions::default();

        let mut buffer = Vec::new();
        output
            .write(&Report::Analysis(result), &options, &mut buffer)
            .unwrap();

        let csv_str = String::from_utf8(buffer).unwrap();
        let lines: Vec<&str> = csv_str.lines().collect();

        assert_eq!(lines[0], "Language,Files,Code,Comment,Blank,Total,Size");
    }

    #[test]
    fn test_csv_output_data() {
        let output = CsvOutput;
        let result = make_test_result();
        let options = OutputOptions::default();

        let mut buffer = Vec::new();
        output
            .write(&Report::Analysis(result), &options, &mut buffer)
            .unwrap();

        let csv_str = String::from_utf8(buffer).unwrap();

        // Should contain language data
        assert!(csv_str.contains("Rust"));
        assert!(csv_str.contains("Python"));
        assert!(csv_str.contains(",80,")); // Rust code lines
        assert!(csv_str.contains(",40,")); // Python code lines
    }

    #[test]
    fn test_csv_output_line_count() {
        let output = CsvOutput;
        let result = make_test_result();
        let options = OutputOptions::default();

        let mut buffer = Vec::new();
        output
            .write(&Report::Analysis(result), &options, &mut buffer)
            .unwrap();

        let csv_str = String::from_utf8(buffer).unwrap();
        let lines: Vec<&str> = csv_str.lines().collect();

        // 1 header + 2 languages = 3 lines
        assert_eq!(lines.len(), 3);
    }
}
