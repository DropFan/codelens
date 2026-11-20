//! Markdown output format.

mod analysis;
mod diff;
mod health;

use std::io::Write;

use super::format::{OutputFormat, OutputOptions, Report};
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
        writeln!(writer, "# Hotspot Analysis")?;
        writeln!(writer)?;
        writeln!(
            writer,
            "**Period:** {} | **Total Commits:** {}",
            report.since, report.total_commits
        )?;
        writeln!(writer)?;

        if report.files.is_empty() {
            writeln!(writer, "No hotspots found.")?;
            return Ok(());
        }

        writeln!(
            writer,
            "| File | Chg | +/- | CC | Age | Authors | Score | Risk |"
        )?;
        writeln!(
            writer,
            "|------|-----|-----|----|-----|---------|-------|------|"
        )?;
        for file in &report.files {
            let age = file
                .age_days
                .map(crate::insight::hotspot::format_age)
                .unwrap_or_else(|| "-".to_string());
            let auth = match &file.knowledge {
                Some(k) if k.knowledge_island => format!("{} ★", k.authors),
                Some(k) => k.authors.to_string(),
                None => "-".to_string(),
            };
            writeln!(
                writer,
                "| {} | {} | +{}/-{} | {} | {} | {} | {:.2} | {} |",
                file.path.display(),
                file.churn.commits,
                file.churn.lines_added,
                file.churn.lines_deleted,
                file.complexity.cyclomatic,
                age,
                auth,
                file.hotspot_score,
                file.risk,
            )?;
        }
        writeln!(writer)?;

        let islands: Vec<_> = report
            .files
            .iter()
            .filter_map(|f| {
                f.knowledge
                    .as_ref()
                    .filter(|k| k.knowledge_island)
                    .map(|k| (f, k))
            })
            .collect();
        if !islands.is_empty() {
            writeln!(
                writer,
                "**★ Knowledge Islands** (risky files effectively one person knows):"
            )?;
            writeln!(writer)?;
            for (file, k) in islands {
                writeln!(
                    writer,
                    "- {} — {:.0}% by {}",
                    file.path.display(),
                    k.ownership * 100.0,
                    k.main_author
                )?;
            }
            writeln!(writer)?;
        }

        // Function-level breakdown (--functions)
        if report.files.iter().any(|f| f.functions.is_some()) {
            writeln!(writer, "## Function Hotspots (approximate spans)")?;
            writeln!(writer)?;
            writeln!(writer, "| File | Function | Lines | Commits | CC |")?;
            writeln!(writer, "|------|----------|-------|---------|-----|")?;
            for file in report.files.iter().filter(|f| f.functions.is_some()) {
                for func in file.functions.as_deref().unwrap_or_default() {
                    writeln!(
                        writer,
                        "| {} | {} | {}-{} | {} | {} |",
                        file.path.display(),
                        func.name,
                        func.start_line,
                        func.end_line,
                        func.commits,
                        func.cyclomatic,
                    )?;
                }
            }
            writeln!(writer)?;
        }

        Ok(())
    }

    fn write_coupling(
        &self,
        report: &crate::insight::coupling::CouplingReport,
        _options: &OutputOptions,
        writer: &mut dyn Write,
    ) -> Result<()> {
        writeln!(writer, "# Change Coupling")?;
        writeln!(writer)?;
        writeln!(
            writer,
            "**Period:** {} | **Total Commits:** {}",
            report.since, report.total_commits
        )?;
        if let Some(focus) = &report.focus {
            writeln!(writer)?;
            writeln!(writer, "**Focus:** {}", focus.display())?;
        }
        if report.skipped_large_commits > 0 {
            writeln!(writer)?;
            writeln!(
                writer,
                "_{} bulk commit(s) excluded from pairing._",
                report.skipped_large_commits
            )?;
        }
        if report.excluded_test_files > 0 {
            writeln!(writer)?;
            writeln!(
                writer,
                "_{} test file(s) excluded (`--include-tests` to include)._",
                report.excluded_test_files
            )?;
        }
        writeln!(writer)?;

        if report.pairs.is_empty() {
            writeln!(writer, "No coupled file pairs found.")?;
            return Ok(());
        }

        writeln!(writer, "| File A | File B | Shared | Coupling |")?;
        writeln!(writer, "|--------|--------|--------|----------|")?;
        for pair in &report.pairs {
            writeln!(
                writer,
                "| {} | {} | {} ({}/{}) | {:.0}% |",
                pair.file_a.display(),
                pair.file_b.display(),
                pair.shared_commits,
                pair.commits_a,
                pair.commits_b,
                pair.degree,
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
        let from_label = report.from.label.as_deref().unwrap_or("");
        let to_label = report.to.label.as_deref().unwrap_or("");

        writeln!(writer, "# Trend Report")?;
        writeln!(writer)?;
        writeln!(
            writer,
            "**From:** {} {} | **To:** {} {}",
            report.from.timestamp.format("%Y-%m-%d"),
            from_label,
            report.to.timestamp.format("%Y-%m-%d"),
            to_label,
        )?;
        writeln!(writer)?;

        // Delta table
        writeln!(writer, "## Delta")?;
        writeln!(writer)?;
        writeln!(writer, "| Metric | Before | After | Delta | Change |")?;
        writeln!(writer, "|--------|--------|-------|-------|--------|")?;
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
            let signed = dv.signed_delta();
            let sign = if signed > 0 { "+" } else { "" };
            writeln!(
                writer,
                "| {} | {} | {} | {}{} | {:+.1}% |",
                name, dv.from, dv.to, sign, signed, dv.percent,
            )?;
        }
        writeln!(writer)?;

        // By Language
        if !report.by_language.is_empty() {
            writeln!(writer, "## By Language")?;
            writeln!(writer)?;
            writeln!(writer, "| Language | Status | Before | After | Delta |")?;
            writeln!(writer, "|----------|--------|--------|-------|-------|")?;
            for lang in &report.by_language {
                let signed = lang.code.signed_delta();
                let sign = if signed > 0 { "+" } else { "" };
                writeln!(
                    writer,
                    "| {} | {} | {} | {} | {}{} |",
                    lang.language, lang.status, lang.code.from, lang.code.to, sign, signed,
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
        writeln!(writer, "# Cost Estimation Report")?;
        writeln!(writer)?;
        writeln!(writer, "**Model:** {}", report.model)?;
        writeln!(writer)?;
        writeln!(writer, "## Summary")?;
        writeln!(writer)?;
        writeln!(writer, "| Metric | Value |")?;
        writeln!(writer, "|--------|-------|")?;
        writeln!(writer, "| Total SLOC | {} |", report.total_sloc)?;
        writeln!(writer, "| Estimated Cost | ${:.2} |", report.estimated_cost)?;
        writeln!(
            writer,
            "| Schedule Effort | {:.2} months |",
            report.schedule_months
        )?;
        writeln!(
            writer,
            "| People Required | {:.2} |",
            report.people_required
        )?;
        writeln!(writer)?;
        if !options.summary_only && !report.by_language.is_empty() {
            writeln!(writer, "## By Language")?;
            writeln!(writer)?;
            writeln!(writer, "| Language | Code | Effort (PM) | Cost |")?;
            writeln!(writer, "|----------|------|-------------|------|")?;
            let mut langs = report.by_language.iter().collect::<Vec<_>>();
            if let Some(n) = options.top_n {
                langs.truncate(n);
            }
            for lang in langs {
                writeln!(
                    writer,
                    "| {} | {} | {:.2} | ${:.2} |",
                    lang.language, lang.code_lines, lang.effort_months, lang.cost,
                )?;
            }
            writeln!(writer)?;
        }
        writeln!(writer, "---")?;
        let params_str: Vec<String> = report
            .params
            .iter()
            .map(|(k, v)| format!("{k}: {v}"))
            .collect();
        writeln!(writer, "*{}*", params_str.join(" | "))?;
        Ok(())
    }

    fn write_estimation_comparison(
        &self,
        report: &crate::insight::estimation::EstimationComparison,
        writer: &mut dyn Write,
    ) -> Result<()> {
        writeln!(writer, "# Cost Estimation Comparison")?;
        writeln!(writer)?;
        writeln!(writer, "**Total SLOC:** {}", report.total_sloc)?;
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
            "| Model | Effort (PM) | Schedule (M) | People | Cost |"
        )?;
        writeln!(
            writer,
            "|-------|-------------|--------------|--------|------|"
        )?;
        for r in &report.reports {
            writeln!(
                writer,
                "| {} | {:.2} | {:.2} | {:.2} | ${:.2} |",
                r.model, r.effort_months, r.schedule_months, r.people_required, r.estimated_cost,
            )?;
        }
        writeln!(writer)?;
        writeln!(writer, "## Calculation Assumptions")?;
        writeln!(writer)?;
        for report in &report.reports {
            let params = report
                .params
                .iter()
                .map(|(key, value)| format!("{key}: {value}"))
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
