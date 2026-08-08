//! Markdown output format.

use std::io::Write;

use crate::analyzer::stats::AnalysisResult;
use crate::error::Result;

use super::format::{OutputFormat, OutputOptions, Report};

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
    fn write_analysis(
        &self,
        result: &AnalysisResult,
        options: &OutputOptions,
        writer: &mut dyn Write,
    ) -> Result<()> {
        let summary = &result.summary;

        writeln!(writer, "# Code Statistics Report")?;
        writeln!(writer)?;

        // Summary
        writeln!(writer, "## Summary")?;
        writeln!(writer)?;
        writeln!(writer, "| Metric | Value |")?;
        writeln!(writer, "|--------|-------|")?;
        writeln!(writer, "| Total Files | {} |", summary.total_files)?;
        writeln!(writer, "| Code Lines | {} |", summary.lines.code)?;
        writeln!(writer, "| Comment Lines | {} |", summary.lines.comment)?;
        writeln!(writer, "| Blank Lines | {} |", summary.lines.blank)?;
        writeln!(writer, "| Total Lines | {} |", summary.lines.total)?;
        writeln!(writer, "| Languages | {} |", summary.by_language.len())?;
        writeln!(writer)?;

        // Language breakdown
        if !options.summary_only && !summary.by_language.is_empty() {
            writeln!(writer, "## By Language")?;
            writeln!(writer)?;
            writeln!(
                writer,
                "| Language | Files | Code | Comment | Blank | Total |"
            )?;
            writeln!(
                writer,
                "|----------|-------|------|---------|-------|-------|"
            )?;

            let mut langs: Vec<_> = summary.by_language.iter().collect();
            if let Some(n) = options.top_n {
                langs.truncate(n);
            }

            for (name, stats) in langs {
                writeln!(
                    writer,
                    "| {} | {} | {} | {} | {} | {} |",
                    name,
                    stats.files,
                    stats.lines.code,
                    stats.lines.comment,
                    stats.lines.blank,
                    stats.lines.total
                )?;
            }
            writeln!(writer)?;
        }

        // Per-directory breakdown (--by-dir)
        if options.by_dir && !result.files.is_empty() {
            writeln!(writer, "## By Directory")?;
            writeln!(writer)?;
            writeln!(
                writer,
                "| Directory | Files | Code | Comment | Blank | Total | CC |"
            )?;
            writeln!(
                writer,
                "|-----------|-------|------|---------|-------|-------|-----|"
            )?;
            for d in crate::analyzer::stats::aggregate_by_dir(&result.files, options.dir_depth) {
                writeln!(
                    writer,
                    "| {} | {} | {} | {} | {} | {} | {} |",
                    d.path.display(),
                    d.files,
                    d.lines.code,
                    d.lines.comment,
                    d.lines.blank,
                    d.lines.total,
                    d.cyclomatic,
                )?;
            }
            writeln!(writer)?;
        }

        // Per-file breakdown (--by-file)
        if options.by_file && !result.files.is_empty() {
            writeln!(writer, "## By File")?;
            writeln!(writer)?;
            writeln!(
                writer,
                "| File | Language | Code | Comment | Blank | Total |"
            )?;
            writeln!(
                writer,
                "|------|----------|------|---------|-------|-------|"
            )?;
            for f in super::format::sorted_files(&result.files, options.sort_by, options.top_n) {
                writeln!(
                    writer,
                    "| {} | {} | {} | {} | {} | {} |",
                    f.path.display(),
                    f.language,
                    f.lines.code,
                    f.lines.comment,
                    f.lines.blank,
                    f.lines.total
                )?;
            }
            writeln!(writer)?;
        }

        writeln!(writer, "---")?;
        writeln!(
            writer,
            "*Generated by codelens in {:.2}s*",
            result.elapsed.as_secs_f64()
        )?;

        Ok(())
    }

    fn write_health(
        &self,
        report: &crate::insight::health::HealthReport,
        options: &OutputOptions,
        writer: &mut dyn Write,
    ) -> Result<()> {
        writeln!(writer, "# Code Health Report")?;
        writeln!(writer)?;
        writeln!(
            writer,
            "**Project Score:** {:.1} | **Grade:** {}",
            report.score, report.grade
        )?;
        writeln!(writer)?;

        // Baseline comparison (--baseline)
        if let Some(reg) = &report.regression {
            let delta = if reg.score_delta >= 0.0 {
                format!("+{:.1}", reg.score_delta)
            } else {
                format!("{:.1}", reg.score_delta)
            };
            writeln!(
                writer,
                "**Baseline:** {} — {} ({:.1}) → {} ({:.1}), Δ {}",
                reg.baseline,
                reg.baseline_grade,
                reg.baseline_score,
                report.grade,
                report.score,
                delta
            )?;
            writeln!(writer)?;
            if !reg.regressed_files.is_empty() {
                writeln!(writer, "## Regressed Files")?;
                writeln!(writer)?;
                writeln!(writer, "| File | Before | After |")?;
                writeln!(writer, "|------|--------|-------|")?;
                for f in &reg.regressed_files {
                    writeln!(
                        writer,
                        "| {} | {} ({:.1}) | {} ({:.1}) |",
                        f.path.display(),
                        f.from_grade,
                        f.from_score,
                        f.to_grade,
                        f.to_score
                    )?;
                }
                writeln!(writer)?;
            }
            writeln!(
                writer,
                "**Verdict:** {}",
                if reg.failed {
                    "🔴 REGRESSED"
                } else {
                    "🟢 NO REGRESSION"
                }
            )?;
            writeln!(writer)?;
        }

        // Dimensions
        writeln!(writer, "## Dimensions")?;
        writeln!(writer)?;
        writeln!(writer, "| Dimension | Score | Grade |")?;
        writeln!(writer, "|-----------|-------|-------|")?;
        for dim in &report.dimensions {
            writeln!(
                writer,
                "| {} | {:.1} | {} |",
                dim.dimension, dim.score, dim.grade
            )?;
        }
        writeln!(writer)?;

        if !options.summary_only {
            if !report.by_directory.is_empty() {
                writeln!(writer, "## By Directory")?;
                writeln!(writer)?;
                writeln!(writer, "| Directory | Score | Grade | Files |")?;
                writeln!(writer, "|-----------|-------|-------|-------|")?;
                for dir in &report.by_directory {
                    writeln!(
                        writer,
                        "| {} | {:.1} | {} | {} |",
                        dir.path.display(),
                        dir.score,
                        dir.grade,
                        dir.file_count
                    )?;
                }
                writeln!(writer)?;
            }

            if !report.worst_files.is_empty() {
                writeln!(writer, "## Worst Files")?;
                writeln!(writer)?;
                writeln!(writer, "| File | Score | Grade | Top Issue |")?;
                writeln!(writer, "|------|-------|-------|-----------|")?;
                for file in &report.worst_files {
                    writeln!(
                        writer,
                        "| {} | {:.1} | {} | {} |",
                        file.path.display(),
                        file.score,
                        file.grade,
                        file.top_issue
                    )?;
                }
                writeln!(writer)?;
            }
        }

        Ok(())
    }

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

        writeln!(writer, "| File | Chg | +/- | CC | Age | Score | Risk |")?;
        writeln!(writer, "|------|-----|-----|----|-----|-------|------|")?;
        for file in &report.files {
            let age = file
                .age_days
                .map(crate::insight::hotspot::format_age)
                .unwrap_or_else(|| "-".to_string());
            writeln!(
                writer,
                "| {} | {} | +{}/-{} | {} | {} | {:.2} | {} |",
                file.path.display(),
                file.churn.commits,
                file.churn.lines_added,
                file.churn.lines_deleted,
                file.complexity.cyclomatic,
                age,
                file.hotspot_score,
                file.risk,
            )?;
        }
        writeln!(writer)?;

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
}
