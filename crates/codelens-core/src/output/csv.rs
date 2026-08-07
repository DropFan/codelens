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
            Report::Health(report) => self.write_health(report, options, writer),
            Report::Hotspot(report) => self.write_hotspot(report, options, writer),
            Report::Trend(report) => self.write_trend(report, options, writer),
            // CSV can't hold three differently-shaped tables in one file;
            // emit the analysis table only (health/estimation are available
            // via their subcommands).
            Report::Combined(combined) => self.write_analysis(&combined.analysis, options, writer),
            Report::Estimation(report) => self.write_estimation(report, options, writer),
            Report::EstimationComparison(report) => {
                self.write_estimation_comparison(report, writer)
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

    fn write_health(
        &self,
        report: &crate::insight::health::HealthReport,
        _options: &OutputOptions,
        writer: &mut dyn Write,
    ) -> Result<()> {
        writeln!(writer, "File,Score,Grade,TopIssue")?;
        for file in &report.worst_files {
            writeln!(
                writer,
                "{},{:.1},{},{}",
                file.path.display(),
                file.score,
                file.grade,
                file.top_issue,
            )?;
        }
        Ok(())
    }

    fn write_hotspot(
        &self,
        report: &crate::insight::hotspot::HotspotReport,
        _options: &OutputOptions,
        writer: &mut dyn Write,
    ) -> Result<()> {
        writeln!(writer, "File,Commits,Added,Deleted,Churn,CC,Score,Risk")?;
        for file in &report.files {
            writeln!(
                writer,
                "{},{},{},{},{},{},{:.2},{}",
                file.path.display(),
                file.churn.commits,
                file.churn.lines_added,
                file.churn.lines_deleted,
                file.churn.lines_churn,
                file.complexity.cyclomatic,
                file.hotspot_score,
                file.risk,
            )?;
        }
        Ok(())
    }

    fn write_trend(
        &self,
        report: &crate::insight::trend::TrendReport,
        _options: &OutputOptions,
        writer: &mut dyn Write,
    ) -> Result<()> {
        writeln!(writer, "Metric,Before,After,Delta,Percent")?;
        let deltas = [
            ("Files", &report.delta.files),
            ("Lines", &report.delta.lines),
            ("Code", &report.delta.code),
            ("Comments", &report.delta.comment),
            ("Blank", &report.delta.blank),
            ("Complexity", &report.delta.complexity),
            ("Functions", &report.delta.functions),
        ];
        for (name, dv) in &deltas {
            writeln!(
                writer,
                "{},{},{},{},{:.1}",
                name,
                dv.from,
                dv.to,
                dv.signed_delta(),
                dv.percent,
            )?;
        }
        Ok(())
    }

    fn write_estimation(
        &self,
        report: &crate::insight::estimation::EstimationReport,
        _options: &OutputOptions,
        writer: &mut dyn Write,
    ) -> Result<()> {
        writeln!(writer, "Language,Code,Effort(PM),Cost")?;
        for lang in &report.by_language {
            writeln!(
                writer,
                "{},{},{:.2},{:.2}",
                lang.language, lang.code_lines, lang.effort_months, lang.cost,
            )?;
        }
        Ok(())
    }

    fn write_estimation_comparison(
        &self,
        report: &crate::insight::estimation::EstimationComparison,
        writer: &mut dyn Write,
    ) -> Result<()> {
        writeln!(writer, "Model,SLOC,Effort(PM),Schedule(M),People,Cost")?;
        for r in &report.reports {
            writeln!(
                writer,
                "{},{},{:.2},{:.2},{:.2},{:.2}",
                r.model,
                r.total_sloc,
                r.effort_months,
                r.schedule_months,
                r.people_required,
                r.estimated_cost,
            )?;
        }
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
