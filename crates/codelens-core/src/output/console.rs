//! Console output with colors and formatting.

mod analysis;
mod diff;
mod health;
mod hotspot;

use std::io::Write;

use colored::Colorize;
use comfy_table::{presets::UTF8_FULL, Attribute, Cell, Color, ContentArrangement, Table};

use super::format::{
    CouplingFilterPresentation, CouplingPairPresentation, CouplingPresentation, OutputFormat,
    OutputOptions, Report, TrendLanguagePresentation, TrendMetricPresentation, TrendPresentation,
    COST_LABEL, EFFORT_LABEL, PEOPLE_LABEL, SCHEDULE_LABEL, TOTAL_SLOC_LABEL,
};
use super::format::{EstimationComparisonPresentation, EstimationPresentation};
use crate::error::Result;

/// Console output formatter.
pub struct ConsoleOutput;

impl ConsoleOutput {
    /// Create a new console output formatter.
    pub fn new() -> Self {
        Self
    }

    fn format_size(bytes: u64) -> String {
        const KB: u64 = 1024;
        const MB: u64 = KB * 1024;
        const GB: u64 = MB * 1024;

        if bytes >= GB {
            format!("{:.2} GB", bytes as f64 / GB as f64)
        } else if bytes >= MB {
            format!("{:.2} MB", bytes as f64 / MB as f64)
        } else if bytes >= KB {
            format!("{:.2} KB", bytes as f64 / KB as f64)
        } else {
            format!("{} B", bytes)
        }
    }

    fn format_number(n: usize) -> String {
        let s = n.to_string();
        let mut result = String::new();
        for (i, c) in s.chars().rev().enumerate() {
            if i > 0 && i % 3 == 0 {
                result.push(',');
            }
            result.push(c);
        }
        result.chars().rev().collect()
    }
}

impl Default for ConsoleOutput {
    fn default() -> Self {
        Self::new()
    }
}

impl OutputFormat for ConsoleOutput {
    fn name(&self) -> &'static str {
        "console"
    }

    fn extension(&self) -> &'static str {
        "txt"
    }

    fn write(
        &self,
        report: &Report,
        options: &OutputOptions,
        writer: &mut dyn Write,
    ) -> Result<()> {
        super::format::validate_report_format(crate::config::OutputFormatType::Console, report)?;
        if options.colorize {
            return self.write_report(report, options, writer);
        }
        // colored/comfy_table decide colors from the process tty, not the
        // actual writer, so strip escape codes when colors are unwanted
        // (e.g. writing to a file via -O).
        let mut buf = Vec::new();
        self.write_report(report, options, &mut buf)?;
        writer.write_all(&strip_ansi(&buf))?;
        Ok(())
    }
}

/// Remove ANSI CSI escape sequences (`ESC [ ... <final byte>`).
fn strip_ansi(input: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(input.len());
    let mut i = 0;
    while i < input.len() {
        if input[i] == 0x1b && input.get(i + 1) == Some(&b'[') {
            i += 2;
            // Skip parameter/intermediate bytes until the final byte (0x40-0x7e)
            while i < input.len() && !(0x40..=0x7e).contains(&input[i]) {
                i += 1;
            }
            i += 1; // skip the final byte itself
        } else {
            out.push(input[i]);
            i += 1;
        }
    }
    out
}

impl ConsoleOutput {
    fn write_report(
        &self,
        report: &Report,
        options: &OutputOptions,
        writer: &mut dyn Write,
    ) -> Result<()> {
        match report {
            Report::Analysis(result) => self.write_analysis(result, options, writer),
            Report::Health(report) => self.write_health(report, options, writer),
            Report::Hotspot(report) => self.write_hotspot(report, options, writer),
            Report::Coupling(report) => self.write_coupling(report, options, writer),
            Report::Trend(report) => self.write_trend(report, options, writer),
            Report::Diff(report) => self.write_diff(report, writer),
            Report::Estimation(report) => self.write_estimation(report, options, writer),
            Report::EstimationComparison(report) => {
                self.write_estimation_comparison(report, writer)
            }
            Report::Combined(combined) => {
                self.write_analysis(&combined.analysis, options, writer)?;
                self.write_health(&combined.health, options, writer)?;
                self.write_estimation_comparison(&combined.estimation, writer)
            }
        }
    }
}

impl ConsoleOutput {
    fn write_hotspot(
        &self,
        report: &crate::insight::hotspot::HotspotReport,
        _options: &OutputOptions,
        writer: &mut dyn Write,
    ) -> Result<()> {
        hotspot::write(report, writer)
    }

    fn write_coupling(
        &self,
        report: &crate::insight::coupling::CouplingReport,
        _options: &OutputOptions,
        writer: &mut dyn Write,
    ) -> Result<()> {
        let coupling = CouplingPresentation::from_report(report);

        writeln!(writer)?;
        writeln!(writer, "{}", "═".repeat(60).dimmed())?;
        writeln!(writer, "{}", " CODELENS - Change Coupling ".bold().cyan())?;
        writeln!(writer, "{}", "═".repeat(60).dimmed())?;
        writeln!(writer)?;

        writeln!(
            writer,
            "  Period: {}  Total Commits: {}",
            coupling.period.bold(),
            Self::format_number(coupling.total_commits).bold()
        )?;
        if let Some(focus) = coupling.focus {
            writeln!(writer, "  Focus: {}", focus.bold())?;
        }
        for note in coupling.filter_notes {
            let note = match note {
                CouplingFilterPresentation::BulkCommits(count) => {
                    format!("{count} bulk commit(s) excluded from pairing")
                }
                CouplingFilterPresentation::TestFiles(count) => {
                    format!("{count} test file(s) excluded (--include-tests to include)")
                }
            };
            writeln!(writer, "  {note}")?;
        }
        writeln!(writer)?;

        if coupling.rows.is_empty() {
            writeln!(writer, "  No coupled file pairs found.")?;
            writeln!(
                writer,
                "  (thresholds: --min-shared / --min-coupling can be lowered)"
            )?;
            return Ok(());
        }

        let table = Self::coupling_table(&coupling.rows);
        writeln!(writer, "{table}")?;
        writeln!(writer)?;
        writeln!(
            writer,
            "  {} Files changing together share a dependency the module\n  structure does not express — review the strongest pairs first.",
            "hint:".dimmed()
        )?;
        writeln!(writer)?;

        Ok(())
    }

    fn coupling_table(rows: &[CouplingPairPresentation]) -> Table {
        let mut table = Table::new();
        table
            .load_preset(UTF8_FULL)
            .set_content_arrangement(ContentArrangement::Dynamic);
        table.set_header(vec![
            Cell::new("File A").add_attribute(Attribute::Bold),
            Cell::new("File B").add_attribute(Attribute::Bold),
            Cell::new("Shared").add_attribute(Attribute::Bold),
            Cell::new("Coupling").add_attribute(Attribute::Bold),
        ]);

        for row in rows {
            table.add_row(vec![
                Cell::new(&row.file_a).fg(Color::Cyan),
                Cell::new(&row.file_b).fg(Color::Cyan),
                Cell::new(&row.shared),
                Cell::new(&row.degree_text).fg(Self::coupling_degree_color(row.degree)),
            ]);
        }

        table
    }

    fn coupling_degree_color(degree: f64) -> Color {
        if degree >= 70.0 {
            Color::Red
        } else if degree >= 50.0 {
            Color::Yellow
        } else {
            Color::Green
        }
    }

    fn write_trend(
        &self,
        report: &crate::insight::trend::TrendReport,
        _options: &OutputOptions,
        writer: &mut dyn Write,
    ) -> Result<()> {
        let trend = TrendPresentation::from_report(report);

        writeln!(writer)?;
        writeln!(writer, "{}", "═".repeat(60).dimmed())?;
        writeln!(writer, "{}", " CODELENS - Trend Report ".bold().cyan())?;
        writeln!(writer, "{}", "═".repeat(60).dimmed())?;
        writeln!(writer)?;

        writeln!(
            writer,
            "  {} {} {}  {} {} {}",
            "From:".bold(),
            trend.from_date,
            trend.from_label,
            "To:".bold(),
            trend.to_date,
            trend.to_label,
        )?;
        writeln!(writer)?;

        let delta_table = Self::trend_delta_table(&trend.metric_rows);
        writeln!(writer, "{delta_table}")?;
        writeln!(writer)?;

        if !trend.language_rows.is_empty() {
            writeln!(writer, "{}", "By Language".bold())?;
            writeln!(writer)?;
            let lang_table = Self::trend_language_table(&trend.language_rows);
            writeln!(writer, "{lang_table}")?;
            writeln!(writer)?;
        }

        Ok(())
    }

    fn trend_delta_table(metrics: &[TrendMetricPresentation]) -> Table {
        let mut table = Table::new();
        table
            .load_preset(UTF8_FULL)
            .set_content_arrangement(ContentArrangement::Dynamic);
        table.set_header(vec![
            Cell::new("Metric").add_attribute(Attribute::Bold),
            Cell::new("Before").add_attribute(Attribute::Bold),
            Cell::new("After").add_attribute(Attribute::Bold),
            Cell::new("Delta").add_attribute(Attribute::Bold),
            Cell::new("Change").add_attribute(Attribute::Bold),
        ]);
        for metric in metrics {
            let color = Self::trend_delta_color(metric.signed_change);
            table.add_row(vec![
                Cell::new(metric.name),
                Cell::new(Self::format_number(metric.start)),
                Cell::new(Self::format_number(metric.end)),
                Cell::new(&metric.signed_change_text).fg(color),
                Cell::new(&metric.percent_change_text).fg(color),
            ]);
        }
        table
    }

    fn trend_language_table(languages: &[TrendLanguagePresentation]) -> Table {
        let mut table = Table::new();
        table
            .load_preset(UTF8_FULL)
            .set_content_arrangement(ContentArrangement::Dynamic);
        table.set_header(vec![
            Cell::new("Language").add_attribute(Attribute::Bold),
            Cell::new("Status").add_attribute(Attribute::Bold),
            Cell::new("Before").add_attribute(Attribute::Bold),
            Cell::new("After").add_attribute(Attribute::Bold),
            Cell::new("Delta").add_attribute(Attribute::Bold),
        ]);
        for language in languages {
            table.add_row(vec![
                Cell::new(&language.name).fg(Color::Cyan),
                Cell::new(&language.marker),
                Cell::new(Self::format_number(language.code_start)),
                Cell::new(Self::format_number(language.code_end)),
                Cell::new(&language.signed_code_change_text)
                    .fg(Self::trend_delta_color(language.signed_code_change)),
            ]);
        }
        table
    }

    fn trend_delta_color(delta: i64) -> Color {
        if delta > 0 {
            Color::Green
        } else if delta < 0 {
            Color::Red
        } else {
            Color::White
        }
    }

    fn write_estimation(
        &self,
        report: &crate::insight::estimation::EstimationReport,
        options: &OutputOptions,
        writer: &mut dyn Write,
    ) -> Result<()> {
        let estimation = EstimationPresentation::from_report(report, options);
        writeln!(writer)?;
        writeln!(writer, "{}", "═".repeat(60).dimmed())?;
        writeln!(
            writer,
            "{}",
            " CODELENS - Cost Estimation Report ".bold().cyan()
        )?;
        writeln!(writer, "{}", "═".repeat(60).dimmed())?;
        writeln!(writer)?;
        writeln!(writer, "  Model: {}", estimation.model.bold())?;
        writeln!(writer)?;

        let mut table = Table::new();
        table
            .load_preset(UTF8_FULL)
            .set_content_arrangement(ContentArrangement::Dynamic);
        table.set_header(vec![
            Cell::new("Metric").add_attribute(Attribute::Bold),
            Cell::new("Value").add_attribute(Attribute::Bold),
        ]);
        table.add_row(vec![
            Cell::new(TOTAL_SLOC_LABEL),
            Cell::new(&estimation.total_sloc_display).fg(Color::Cyan),
        ]);
        table.add_row(vec![
            Cell::new(EFFORT_LABEL),
            Cell::new(&estimation.effort_months),
        ]);
        table.add_row(vec![
            Cell::new(SCHEDULE_LABEL),
            Cell::new(&estimation.schedule_months).fg(Color::Yellow),
        ]);
        table.add_row(vec![
            Cell::new(PEOPLE_LABEL),
            Cell::new(&estimation.people_required).fg(Color::Magenta),
        ]);
        table.add_row(vec![
            Cell::new(COST_LABEL),
            Cell::new(format!("${}", estimation.estimated_cost)).fg(Color::Green),
        ]);
        writeln!(writer, "{table}")?;
        writeln!(writer)?;

        if !estimation.languages.is_empty() {
            writeln!(writer, "{}", "By Language".bold())?;
            writeln!(writer)?;
            let mut lang_table = Table::new();
            lang_table
                .load_preset(UTF8_FULL)
                .set_content_arrangement(ContentArrangement::Dynamic);
            lang_table.set_header(vec![
                Cell::new("Language").add_attribute(Attribute::Bold),
                Cell::new("Code").add_attribute(Attribute::Bold),
                Cell::new(EFFORT_LABEL).add_attribute(Attribute::Bold),
                Cell::new(COST_LABEL).add_attribute(Attribute::Bold),
            ]);
            for lang in &estimation.languages {
                lang_table.add_row(vec![
                    Cell::new(&lang.language).fg(Color::Cyan),
                    Cell::new(&lang.code_lines_display),
                    Cell::new(&lang.effort_months),
                    Cell::new(format!("${}", lang.cost)).fg(Color::Green),
                ]);
            }
            writeln!(writer, "{lang_table}")?;
            writeln!(writer)?;
        }

        writeln!(writer, "{}", "─".repeat(60).dimmed())?;
        for param in &estimation.params {
            write!(
                writer,
                "{}  ",
                format!("{}: {}", param.key, param.value).dimmed()
            )?;
        }
        writeln!(writer)?;

        Ok(())
    }

    fn write_estimation_comparison(
        &self,
        report: &crate::insight::estimation::EstimationComparison,
        writer: &mut dyn Write,
    ) -> Result<()> {
        let estimation = EstimationComparisonPresentation::from_report(report);
        writeln!(writer)?;
        writeln!(writer, "{}", "═".repeat(60).dimmed())?;
        writeln!(
            writer,
            "{}",
            " CODELENS - Cost Estimation Comparison ".bold().cyan()
        )?;
        writeln!(writer, "{}", "═".repeat(60).dimmed())?;
        writeln!(writer)?;
        writeln!(
            writer,
            "  Total SLOC: {}",
            estimation.total_sloc_display.bold()
        )?;
        writeln!(
            writer,
            "  Scope: all counted SLOC, including tests and recognized documentation/configuration files"
        )?;
        writeln!(
            writer,
            "  Note: LOCOMO estimates LLM generation cost and review effort, not total software development cost"
        )?;
        writeln!(writer)?;

        let mut table = Table::new();
        table
            .load_preset(UTF8_FULL)
            .set_content_arrangement(ContentArrangement::Dynamic);
        table.set_header(vec![
            Cell::new("Model").add_attribute(Attribute::Bold),
            Cell::new(EFFORT_LABEL).add_attribute(Attribute::Bold),
            Cell::new(SCHEDULE_LABEL).add_attribute(Attribute::Bold),
            Cell::new(PEOPLE_LABEL).add_attribute(Attribute::Bold),
            Cell::new(COST_LABEL).add_attribute(Attribute::Bold),
        ]);
        for r in &estimation.reports {
            table.add_row(vec![
                Cell::new(&r.model).fg(Color::Cyan),
                Cell::new(&r.effort_months),
                Cell::new(&r.schedule_months).fg(Color::Yellow),
                Cell::new(&r.people_required).fg(Color::Magenta),
                Cell::new(format!("${}", r.estimated_cost)).fg(Color::Green),
            ]);
        }
        writeln!(writer, "{table}")?;
        writeln!(writer)?;

        // Per-model parameter summary
        writeln!(writer, "{}", "─".repeat(60).dimmed())?;
        for r in &estimation.reports {
            let params: Vec<String> = r
                .params
                .iter()
                .map(|param| format!("{}={}", param.key, param.value))
                .collect();
            writeln!(
                writer,
                "{}",
                format!("{}: {}", r.model, params.join(", ")).dimmed()
            )?;
        }

        Ok(())
    }

    fn grade_color(grade: crate::insight::Grade) -> Color {
        use crate::insight::Grade;
        match grade {
            Grade::A => Color::Green,
            Grade::B => Color::Cyan,
            Grade::C => Color::Yellow,
            Grade::D => Color::Red,
            Grade::F => Color::DarkRed,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analyzer::stats::{AnalysisResult, FileStats, LineStats, Summary};
    use crate::insight::health::{
        FileRegression, HealthReport, LanguageHealthChange, LanguageHealthChangeStatus,
        RegressionReport,
    };
    use crate::insight::Grade;
    use std::path::PathBuf;
    use std::time::Duration;

    fn make_result() -> AnalysisResult {
        let files = vec![FileStats {
            path: PathBuf::from("test.rs"),
            language: "Rust".to_string(),
            lines: LineStats {
                total: 10,
                code: 8,
                comment: 1,
                blank: 1,
            },
            size: 100,
            duplicate_lines: 0,
            complexity: Default::default(),
        }];
        AnalysisResult {
            summary: Summary::from_file_stats(&files),
            files,
            elapsed: Duration::from_millis(5),
            scanned_files: 1,
            skipped_files: 0,
            error_files: 0,
        }
    }

    #[test]
    fn test_strip_ansi_removes_csi_sequences() {
        let input = b"\x1b[1;32mgreen\x1b[0m plain";
        assert_eq!(strip_ansi(input), b"green plain");
    }

    #[test]
    fn trend_delta_colors_preserve_existing_mapping() {
        assert_eq!(ConsoleOutput::trend_delta_color(1), Color::Green);
        assert_eq!(ConsoleOutput::trend_delta_color(-1), Color::Red);
        assert_eq!(ConsoleOutput::trend_delta_color(0), Color::White);
    }

    #[test]
    fn coupling_degree_colors_preserve_threshold_boundaries() {
        assert_eq!(ConsoleOutput::coupling_degree_color(70.0), Color::Red);
        assert_eq!(
            ConsoleOutput::coupling_degree_color(69.999_999),
            Color::Yellow
        );
        assert_eq!(ConsoleOutput::coupling_degree_color(50.0), Color::Yellow);
        assert_eq!(
            ConsoleOutput::coupling_degree_color(49.999_999),
            Color::Green
        );
    }

    #[test]
    fn test_no_ansi_codes_when_colorize_disabled() {
        let output = ConsoleOutput::new();
        let options = OutputOptions {
            colorize: false,
            ..Default::default()
        };
        let mut buf = Vec::new();
        output
            .write(&Report::Analysis(make_result()), &options, &mut buf)
            .unwrap();
        assert!(
            !buf.contains(&0x1b),
            "colorize=false output must not contain ANSI escape codes"
        );
        // Content is still intact
        let text = String::from_utf8(buf).unwrap();
        assert!(text.contains("Rust"));
    }

    #[test]
    fn low_confidence_health_is_labeled_as_an_estimate() {
        let result = make_result();
        let model = crate::insight::scoring::default::DefaultModel::without_duplication();
        let report = crate::insight::health::score(&result, &model, 10);
        let options = OutputOptions {
            colorize: false,
            ..Default::default()
        };
        let mut buffer = Vec::new();

        ConsoleOutput::new()
            .write(&Report::Health(report), &options, &mut buffer)
            .unwrap();

        let text = String::from_utf8(buffer).unwrap();
        assert!(text.contains("Project Assessment: INSUFFICIENT DATA"));
        assert!(text.contains("Estimated Score:"));
        assert!(text.contains("Insufficient data"));
        assert!(!text.contains("Project Score:"));
    }

    #[test]
    fn test_health_comparison_uses_joint_model_score_and_marks_new_files() {
        let report = HealthReport {
            score: 99.0,
            grade: Grade::A,
            model: "default-v2".to_string(),
            dimensions: vec![],
            by_directory: vec![],
            worst_files: vec![],
            scope: crate::insight::health::HealthScope::Production,
            confidence: Default::default(),
            assessment: Default::default(),
            tail_risk: Default::default(),
            by_language: vec![],
            test_health: None,
            regression: Some(RegressionReport {
                baseline: "snapshot old".to_string(),
                model: "default-v2-no-dup".to_string(),
                assessment: Default::default(),
                baseline_score: 93.3,
                baseline_grade: Grade::A,
                baseline_scope: crate::insight::health::HealthScope::TestsOnly,
                current_score: 92.0,
                current_grade: Grade::A,
                current_scope: crate::insight::health::HealthScope::Production,
                scope_changed: true,
                score_delta: -1.3,
                project_regressed: false,
                regressed_files: vec![FileRegression {
                    path: PathBuf::from("src/new_bad.rs"),
                    is_new: true,
                    from_score: 100.0,
                    from_grade: Grade::A,
                    to_score: 40.0,
                    to_grade: Grade::F,
                }],
                improved_files: 0,
                by_language: vec![LanguageHealthChange {
                    language: "Rust".to_string(),
                    status: LanguageHealthChangeStatus::Changed,
                    from_score: Some(93.3),
                    from_grade: Some(Grade::A),
                    to_score: Some(92.0),
                    to_grade: Some(Grade::A),
                    score_delta: Some(-1.3),
                }],
                failed: true,
            }),
        };
        let options = OutputOptions {
            colorize: false,
            ..Default::default()
        };
        let mut buf = Vec::new();

        ConsoleOutput::new()
            .write(&Report::Health(report), &options, &mut buf)
            .unwrap();

        let text = String::from_utf8(buf).unwrap();
        assert!(text.contains("Model: default-v2-no-dup"));
        assert!(text.contains("A (93.3) → A (92.0)"));
        assert!(text.contains("Scope changed: tests-only → production"));
        assert!(text.contains("Language Health Changes"));
        assert!(text.contains("Rust"));
        assert!(text.contains("-1.3"));
        assert!(text.contains("NEW"));
        assert!(!text.contains("A (93.3) → A (99.0)"));
    }
}
