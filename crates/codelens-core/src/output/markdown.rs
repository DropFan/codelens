//! Markdown output format.

mod analysis;
mod diff;
mod health;
mod hotspot;

use std::io::Write;

use super::format::{
    CouplingFilterPresentation, CouplingPresentation, OutputFormat, OutputOptions, Report,
    TrendPresentation, COST_LABEL, EFFORT_LABEL, PEOPLE_LABEL, SCHEDULE_LABEL, TOTAL_SLOC_LABEL,
};
use super::format::{EstimationComparisonPresentation, EstimationPresentation};
use crate::error::Result;

/// Markdown output formatter.
pub struct MarkdownOutput;

impl MarkdownOutput {
    /// Create a new Markdown output formatter.
    pub fn new() -> Self {
        Self
    }
}

impl Default for MarkdownOutput {
    fn default() -> Self {
        Self::new()
    }
}

impl OutputFormat for MarkdownOutput {
    fn name(&self) -> &'static str {
        "markdown"
    }

    fn extension(&self) -> &'static str {
        "md"
    }

    fn write(
        &self,
        report: &Report,
        options: &OutputOptions,
        writer: &mut dyn Write,
    ) -> Result<()> {
        super::format::validate_report_format(crate::config::OutputFormatType::Markdown, report)?;
        match report {
            Report::Analysis(result) => self.write_analysis(result, options, writer),
            Report::Health(report) => self.write_health(report, options, writer),
            Report::Hotspot(report) => self.write_hotspot(report, options, writer),
            Report::Coupling(report) => self.write_coupling(report, options, writer),
            Report::Diff(report) => self.write_diff(report, writer),
            Report::Trend(report) => self.write_trend(report, options, writer),
            // Concatenated sections are valid Markdown
            Report::Combined(combined) => {
                self.write_analysis(&combined.analysis, options, writer)?;
                self.write_health(&combined.health, options, writer)?;
                self.write_estimation_comparison(&combined.estimation, writer)
            }
            Report::Estimation(report) => self.write_estimation(report, options, writer),
            Report::EstimationComparison(report) => {
                self.write_estimation_comparison(report, writer)
            }
        }
    }
}

impl MarkdownOutput {
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

        writeln!(writer, "# Change Coupling")?;
        writeln!(writer)?;
        writeln!(
            writer,
            "**Period:** {} | **Total Commits:** {}",
            coupling.period, coupling.total_commits
        )?;
        if let Some(focus) = &coupling.focus {
            writeln!(writer)?;
            writeln!(writer, "**Focus:** {focus}")?;
        }
        for note in &coupling.filter_notes {
            writeln!(writer)?;
            let note = match note {
                CouplingFilterPresentation::BulkCommits(count) => {
                    format!("{count} bulk commit(s) excluded from pairing")
                }
                CouplingFilterPresentation::TestFiles(count) => {
                    format!("{count} test file(s) excluded (`--include-tests` to include)")
                }
            };
            writeln!(writer, "_{note}._")?;
        }
        writeln!(writer)?;

        if coupling.rows.is_empty() {
            writeln!(writer, "No coupled file pairs found.")?;
            return Ok(());
        }

        writeln!(writer, "| File A | File B | Shared | Coupling |")?;
        writeln!(writer, "|--------|--------|--------|----------|")?;
        for row in coupling.rows {
            writeln!(
                writer,
                "| {} | {} | {} | {} |",
                row.file_a, row.file_b, row.shared, row.degree_text,
            )?;
        }
        writeln!(writer)?;

        Ok(())
    }

    fn write_trend(
        &self,
        report: &crate::insight::trend::TrendReport,
        _options: &OutputOptions,
        writer: &mut dyn Write,
    ) -> Result<()> {
        let trend = TrendPresentation::from_report(report);

        writeln!(writer, "# Trend Report")?;
        writeln!(writer)?;
        writeln!(
            writer,
            "**From:** {} {} | **To:** {} {}",
            trend.from_date, trend.from_label, trend.to_date, trend.to_label,
        )?;
        writeln!(writer)?;

        writeln!(writer, "## Delta")?;
        writeln!(writer)?;
        writeln!(writer, "| Metric | Before | After | Delta | Change |")?;
        writeln!(writer, "|--------|--------|-------|-------|--------|")?;
        for metric in &trend.metric_rows {
            writeln!(
                writer,
                "| {} | {} | {} | {} | {} |",
                metric.name,
                metric.start,
                metric.end,
                metric.signed_change_text,
                metric.percent_change_text,
            )?;
        }
        writeln!(writer)?;

        if !trend.language_rows.is_empty() {
            writeln!(writer, "## By Language")?;
            writeln!(writer)?;
            writeln!(writer, "| Language | Status | Before | After | Delta |")?;
            writeln!(writer, "|----------|--------|--------|-------|-------|")?;
            for language in &trend.language_rows {
                writeln!(
                    writer,
                    "| {} | {} | {} | {} | {} |",
                    language.name,
                    language.marker,
                    language.code_start,
                    language.code_end,
                    language.signed_code_change_text,
                )?;
            }
            writeln!(writer)?;
        }

        Ok(())
    }

    fn write_estimation(
        &self,
        report: &crate::insight::estimation::EstimationReport,
        options: &OutputOptions,
        writer: &mut dyn Write,
    ) -> Result<()> {
        let estimation = EstimationPresentation::from_report(report, options);
        writeln!(writer, "# Cost Estimation Report")?;
        writeln!(writer)?;
        writeln!(writer, "**Model:** {}", estimation.model)?;
        writeln!(writer)?;
        writeln!(writer, "## Summary")?;
        writeln!(writer)?;
        writeln!(writer, "| Metric | Value |")?;
        writeln!(writer, "|--------|-------|")?;
        writeln!(writer, "| {TOTAL_SLOC_LABEL} | {} |", estimation.total_sloc)?;
        writeln!(writer, "| {EFFORT_LABEL} | {} |", estimation.effort_months)?;
        writeln!(
            writer,
            "| {SCHEDULE_LABEL} | {} |",
            estimation.schedule_months
        )?;
        writeln!(
            writer,
            "| {PEOPLE_LABEL} | {} |",
            estimation.people_required
        )?;
        writeln!(writer, "| {COST_LABEL} | ${} |", estimation.estimated_cost)?;
        writeln!(writer)?;
        if !estimation.languages.is_empty() {
            writeln!(writer, "## By Language")?;
            writeln!(writer)?;
            writeln!(
                writer,
                "| Language | Code | {EFFORT_LABEL} | {COST_LABEL} |"
            )?;
            writeln!(writer, "|----------|------|-------------|------|")?;
            for lang in &estimation.languages {
                writeln!(
                    writer,
                    "| {} | {} | {} | ${} |",
                    lang.language, lang.code_lines, lang.effort_months, lang.cost,
                )?;
            }
            writeln!(writer)?;
        }
        writeln!(writer, "---")?;
        let params_str: Vec<String> = estimation
            .params
            .iter()
            .map(|param| format!("{}: {}", param.key, param.value))
            .collect();
        writeln!(writer, "*{}*", params_str.join(" | "))?;
        Ok(())
    }

    fn write_estimation_comparison(
        &self,
        report: &crate::insight::estimation::EstimationComparison,
        writer: &mut dyn Write,
    ) -> Result<()> {
        let estimation = EstimationComparisonPresentation::from_report(report);
        writeln!(writer, "# Cost Estimation Comparison")?;
        writeln!(writer)?;
        writeln!(writer, "**{TOTAL_SLOC_LABEL}:** {}", estimation.total_sloc)?;
        writeln!(
            writer,
            "**Scope:** all counted SLOC, including tests and recognized documentation/configuration files."
        )?;
        writeln!(
            writer,
            "> LOCOMO estimates LLM generation cost and review effort, not total software development cost."
        )?;
        writeln!(writer)?;
        writeln!(
            writer,
            "| Model | {EFFORT_LABEL} | {SCHEDULE_LABEL} | {PEOPLE_LABEL} | {COST_LABEL} |"
        )?;
        writeln!(
            writer,
            "|-------|-------------|--------------|--------|------|"
        )?;
        for r in &estimation.reports {
            writeln!(
                writer,
                "| {} | {} | {} | {} | ${} |",
                r.model, r.effort_months, r.schedule_months, r.people_required, r.estimated_cost,
            )?;
        }
        writeln!(writer)?;
        writeln!(writer, "## Calculation Assumptions")?;
        writeln!(writer)?;
        for report in &estimation.reports {
            let params = report
                .params
                .iter()
                .map(|param| format!("{}: {}", param.key, param.value))
                .collect::<Vec<_>>()
                .join("; ");
            writeln!(writer, "- **{}:** {}", report.model, params)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::Report;
    use super::*;
    use crate::analyzer::stats::{AnalysisResult, FileStats, LineStats, Summary};
    use crate::insight::health::{
        FileRegression, HealthReport, LanguageHealthChange, LanguageHealthChangeStatus,
        RegressionReport,
    };
    use crate::insight::Grade;
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
                duplicate_lines: 0,
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
                duplicate_lines: 0,
                complexity: Default::default(),
            },
        ];
        AnalysisResult {
            summary: Summary::from_file_stats(&files),
            files,
            elapsed: Duration::from_secs(1),
            scanned_files: 2,
            skipped_files: 0,
            error_files: 0,
        }
    }

    #[test]
    fn test_markdown_output_name() {
        let output = MarkdownOutput::new();
        assert_eq!(output.name(), "markdown");
        assert_eq!(output.extension(), "md");
    }

    #[test]
    fn test_markdown_output_title() {
        let output = MarkdownOutput;
        let result = make_test_result();
        let options = OutputOptions::default();

        let mut buffer = Vec::new();
        output
            .write(&Report::Analysis(result), &options, &mut buffer)
            .unwrap();

        let md_str = String::from_utf8(buffer).unwrap();
        assert!(md_str.starts_with("# Code Statistics Report"));
    }

    #[test]
    fn test_markdown_output_summary_table() {
        let output = MarkdownOutput;
        let result = make_test_result();
        let options = OutputOptions::default();

        let mut buffer = Vec::new();
        output
            .write(&Report::Analysis(result), &options, &mut buffer)
            .unwrap();

        let md_str = String::from_utf8(buffer).unwrap();

        assert!(md_str.contains("## Summary"));
        assert!(md_str.contains("| Total Files | 2 |"));
        assert!(md_str.contains("| Code Lines | 120 |"));
    }

    #[test]
    fn test_markdown_output_language_breakdown() {
        let output = MarkdownOutput;
        let result = make_test_result();
        let options = OutputOptions {
            summary_only: false,
            ..Default::default()
        };

        let mut buffer = Vec::new();
        output
            .write(&Report::Analysis(result), &options, &mut buffer)
            .unwrap();

        let md_str = String::from_utf8(buffer).unwrap();

        assert!(md_str.contains("## By Language"));
        assert!(md_str.contains("| Rust |"));
        assert!(md_str.contains("| Python |"));
    }

    #[test]
    fn test_markdown_output_summary_only() {
        let output = MarkdownOutput;
        let result = make_test_result();
        let options = OutputOptions {
            summary_only: true,
            ..Default::default()
        };

        let mut buffer = Vec::new();
        output
            .write(&Report::Analysis(result), &options, &mut buffer)
            .unwrap();

        let md_str = String::from_utf8(buffer).unwrap();

        assert!(md_str.contains("## Summary"));
        assert!(!md_str.contains("## By Language"));
    }

    #[test]
    fn test_markdown_output_top_n() {
        let output = MarkdownOutput;
        let result = make_test_result();
        let options = OutputOptions {
            top_n: Some(1),
            ..Default::default()
        };

        let mut buffer = Vec::new();
        output
            .write(&Report::Analysis(result), &options, &mut buffer)
            .unwrap();

        let md_str = String::from_utf8(buffer).unwrap();

        // Only Rust should appear (it has more code lines)
        assert!(md_str.contains("| Rust |"));
        // Count occurrences of language rows (excluding header)
        let rust_count = md_str.matches("| Rust |").count();
        let python_count = md_str.matches("| Python |").count();
        assert_eq!(rust_count, 1);
        assert_eq!(python_count, 0);
    }

    #[test]
    fn test_markdown_output_footer() {
        let output = MarkdownOutput;
        let result = make_test_result();
        let options = OutputOptions::default();

        let mut buffer = Vec::new();
        output
            .write(&Report::Analysis(result), &options, &mut buffer)
            .unwrap();

        let md_str = String::from_utf8(buffer).unwrap();

        assert!(md_str.contains("---"));
        assert!(md_str.contains("*Generated by codelens"));
    }

    #[test]
    fn low_confidence_health_is_labeled_as_an_estimate() {
        let result = make_test_result();
        let model = crate::insight::scoring::default::DefaultModel::without_duplication();
        let report = crate::insight::health::score(&result, &model, 10);
        let mut buffer = Vec::new();

        MarkdownOutput::new()
            .write(
                &Report::Health(report),
                &OutputOptions::default(),
                &mut buffer,
            )
            .unwrap();

        let markdown = String::from_utf8(buffer).unwrap();
        assert!(markdown.contains("**Project Assessment:** Insufficient data"));
        assert!(markdown.contains("**Estimated score:**"));
        assert!(markdown.contains("Insufficient data"));
        assert!(!markdown.contains("**Project Score:**"));
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
        let mut buffer = Vec::new();

        MarkdownOutput::new()
            .write(
                &Report::Health(report),
                &OutputOptions::default(),
                &mut buffer,
            )
            .unwrap();

        let markdown = String::from_utf8(buffer).unwrap();
        assert!(markdown.contains("**Comparison model:** default-v2-no-dup"));
        assert!(markdown.contains("A (93.3) → A (92.0)"));
        assert!(markdown.contains("**Scope changed:** tests-only → production"));
        assert!(markdown.contains("## Language Health Changes"));
        assert!(markdown.contains("| Rust | changed | A (93.3) | A (92.0) | -1.3 |"));
        assert!(markdown.contains("| src/new_bad.rs | NEW | F (40.0) |"));
        assert!(!markdown.contains("A (93.3) → A (99.0)"));
    }
}
