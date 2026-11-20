//! Markdown rendering for health reports.

use std::io::Write;

use crate::error::Result;
use crate::insight::health::HealthAssessment;
use crate::output::format::{HealthPresentation, OutputOptions};

use super::{write_language_health_changes, MarkdownOutput};

impl MarkdownOutput {
    pub(super) fn write_health(
        &self,
        report: &crate::insight::health::HealthReport,
        options: &OutputOptions,
        writer: &mut dyn Write,
    ) -> Result<()> {
        let presentation = HealthPresentation::from_report(report);
        self.write_health_summary(report, &presentation, writer)?;
        self.write_health_regression(report, writer)?;
        self.write_health_dimensions(&presentation, writer)?;
        self.write_health_languages(&presentation, writer)?;
        if !options.summary_only {
            self.write_health_directories(&presentation, writer)?;
            self.write_health_files(&presentation, writer)?;
        }
        Ok(())
    }

    fn write_health_summary(
        &self,
        report: &crate::insight::health::HealthReport,
        presentation: &HealthPresentation,
        writer: &mut dyn Write,
    ) -> Result<()> {
        writeln!(writer, "# Code Health Report")?;
        writeln!(writer)?;
        if report.assessment == HealthAssessment::InsufficientData {
            writeln!(
                writer,
                "**Project Assessment:** Insufficient data | **Estimated score:** {} | **Model:** {}",
                presentation.score, presentation.model
            )?;
        } else {
            writeln!(
                writer,
                "**Project Score:** {} | **Grade:** {} | **Model:** {}",
                presentation.score, report.grade, presentation.model
            )?;
        }
        writeln!(
            writer,
            "**Health analysis scope:** {} | **Measurement confidence:** {}",
            presentation.scope, presentation.confidence,
        )?;
        writeln!(
            writer,
            "**Worst 10% file average:** {} | **F-grade files:** {} ({})",
            presentation.tail_score, presentation.failing_files, presentation.failing_ratio,
        )?;
        if let Some(directory) = &presentation.worst_directory {
            writeln!(
                writer,
                "**Worst directory:** {} — {} ({}) | {} files",
                directory.path, directory.grade, directory.score_display, directory.file_count,
            )?;
        }
        if let Some(test_health) = &presentation.test_health {
            writeln!(writer, "**Test code health:** {test_health}")?;
        }
        if presentation.failing_files > 0 {
            writeln!(
                writer,
                "> **Risk:** {} file(s) are graded F despite the overall {} grade.",
                presentation.failing_files, report.grade,
            )?;
        }
        writeln!(writer)?;
        Ok(())
    }

    fn write_health_regression(
        &self,
        report: &crate::insight::health::HealthReport,
        writer: &mut dyn Write,
    ) -> Result<()> {
        if let Some(reg) = &report.regression {
            let delta = if reg.score_delta >= 0.0 {
                format!("+{:.1}", reg.score_delta)
            } else {
                format!("{:.1}", reg.score_delta)
            };
            writeln!(
                writer,
                "**Baseline:** {} | **Comparison model:** {} — {} ({:.1}) → {} ({:.1}), Δ {}",
                reg.baseline,
                reg.model,
                reg.baseline_grade,
                reg.baseline_score,
                reg.current_grade,
                reg.current_score,
                delta
            )?;
            if reg.scope_changed {
                writeln!(
                    writer,
                    "**Scope changed:** {} → {} (project delta is informational and does not affect the gate)",
                    reg.baseline_scope, reg.current_scope
                )?;
            }
            if reg.assessment == HealthAssessment::InsufficientData {
                writeln!(
                    writer,
                    "**Assessment:** insufficient data (score delta is an estimate)"
                )?;
            }
            writeln!(writer)?;
            if !reg.regressed_files.is_empty() {
                writeln!(writer, "## Regressed Files")?;
                writeln!(writer)?;
                writeln!(writer, "| File | Before | After |")?;
                writeln!(writer, "|------|--------|-------|")?;
                for f in &reg.regressed_files {
                    let before = if f.is_new {
                        "NEW".to_string()
                    } else {
                        format!("{} ({:.1})", f.from_grade, f.from_score)
                    };
                    writeln!(
                        writer,
                        "| {} | {} | {} ({:.1}) |",
                        f.path.display(),
                        before,
                        f.to_grade,
                        f.to_score
                    )?;
                }
                writeln!(writer)?;
            }
            write_language_health_changes(writer, &reg.by_language)?;
            writeln!(
                writer,
                "**Verdict:** {}",
                if reg.assessment == HealthAssessment::InsufficientData {
                    "🟡 INSUFFICIENT DATA"
                } else if reg.failed {
                    "🔴 REGRESSED"
                } else {
                    "🟢 NO REGRESSION"
                }
            )?;
            writeln!(writer)?;
        }
        Ok(())
    }

    fn write_health_dimensions(
        &self,
        presentation: &HealthPresentation,
        writer: &mut dyn Write,
    ) -> Result<()> {
        writeln!(writer, "## Dimensions")?;
        writeln!(writer)?;
        writeln!(writer, "| Dimension | Score | Grade |")?;
        writeln!(writer, "|-----------|-------|-------|")?;
        for dimension in &presentation.dimensions {
            writeln!(
                writer,
                "| {} | {} | {} |",
                dimension.dimension, dimension.score_display, dimension.grade
            )?;
        }
        writeln!(writer)?;
        Ok(())
    }

    fn write_health_languages(
        &self,
        presentation: &HealthPresentation,
        writer: &mut dyn Write,
    ) -> Result<()> {
        if !presentation.languages.is_empty() {
            writeln!(writer, "## By Language")?;
            writeln!(writer)?;
            writeln!(
                writer,
                "| Language | Score | Grade | Files | Code | Confidence | Tail 10% |"
            )?;
            writeln!(
                writer,
                "|----------|-------|-------|-------|------|------------|----------|"
            )?;
            for language in &presentation.languages {
                let score = if language.estimated {
                    format!("est. {}", language.score_display)
                } else {
                    language.score_display.clone()
                };
                writeln!(
                    writer,
                    "| {} | {} | {} | {} | {} | {} | {} |",
                    language.language,
                    score,
                    language.grade_display,
                    language.file_count,
                    language.code_lines,
                    language.confidence,
                    language.tail_score,
                )?;
            }
            writeln!(writer)?;
        }
        Ok(())
    }

    fn write_health_directories(
        &self,
        presentation: &HealthPresentation,
        writer: &mut dyn Write,
    ) -> Result<()> {
        if !presentation.directories.is_empty() {
            writeln!(writer, "## By Directory")?;
            writeln!(writer)?;
            writeln!(writer, "| Directory | Score | Grade | Files |")?;
            writeln!(writer, "|-----------|-------|-------|-------|")?;
            for directory in &presentation.directories {
                writeln!(
                    writer,
                    "| {} | {} | {} | {} |",
                    directory.path, directory.score_display, directory.grade, directory.file_count
                )?;
            }
            writeln!(writer)?;
        }
        Ok(())
    }

    fn write_health_files(
        &self,
        presentation: &HealthPresentation,
        writer: &mut dyn Write,
    ) -> Result<()> {
        if !presentation.worst_files.is_empty() {
            writeln!(writer, "## Worst Files")?;
            writeln!(writer)?;
            writeln!(writer, "| File | Score | Grade | Top Issue |")?;
            writeln!(writer, "|------|-------|-------|-----------|")?;
            for file in &presentation.worst_files {
                writeln!(
                    writer,
                    "| {} | {} | {} | {} |",
                    file.path, file.score_display, file.grade, file.top_issue,
                )?;
            }
            writeln!(writer)?;
        }
        Ok(())
    }
}
