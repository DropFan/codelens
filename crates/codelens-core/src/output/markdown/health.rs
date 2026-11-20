//! Markdown rendering for health reports.

use std::io::Write;

use crate::error::Result;
use crate::insight::health::HealthAssessment;
use crate::output::format::{
    HealthLanguageChangePresentation, HealthPresentation, HealthRegressedFilePresentation,
    HealthRegressionVerdict, OutputOptions,
};

use super::MarkdownOutput;

impl MarkdownOutput {
    pub(super) fn write_health(
        &self,
        report: &crate::insight::health::HealthReport,
        options: &OutputOptions,
        writer: &mut dyn Write,
    ) -> Result<()> {
        let presentation = HealthPresentation::from_report(report);
        self.write_health_summary(report, &presentation, writer)?;
        self.write_health_regression(&presentation, writer)?;
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
        presentation: &HealthPresentation,
        writer: &mut dyn Write,
    ) -> Result<()> {
        let Some(regression) = &presentation.regression else {
            return Ok(());
        };
        writeln!(
            writer,
            "**Baseline:** {} | **Comparison model:** {} — {}, Δ {}",
            regression.baseline, regression.model, regression.transition, regression.score_delta
        )?;
        if let Some((baseline, current)) = &regression.scope_change {
            writeln!(
                writer,
                "**Scope changed:** {baseline} → {current} (project delta is informational and does not affect the gate)"
            )?;
        }
        if regression.insufficient_data {
            writeln!(
                writer,
                "**Assessment:** insufficient data (score delta is an estimate)"
            )?;
        }
        writeln!(writer)?;
        self.write_health_regressed_files(&regression.regressed_files, writer)?;
        self.write_health_language_changes(&regression.languages, writer)?;
        let verdict = match regression.verdict {
            HealthRegressionVerdict::InsufficientData => "🟡 INSUFFICIENT DATA",
            HealthRegressionVerdict::Regressed => "🔴 REGRESSED",
            HealthRegressionVerdict::NoRegression => "🟢 NO REGRESSION",
        };
        writeln!(writer, "**Verdict:** {verdict}")?;
        writeln!(writer)?;
        Ok(())
    }

    fn write_health_regressed_files(
        &self,
        files: &[HealthRegressedFilePresentation],
        writer: &mut dyn Write,
    ) -> Result<()> {
        if files.is_empty() {
            return Ok(());
        }
        writeln!(writer, "## Regressed Files")?;
        writeln!(writer)?;
        writeln!(writer, "| File | Before | After |")?;
        writeln!(writer, "|------|--------|-------|")?;
        for file in files {
            writeln!(
                writer,
                "| {} | {} | {} |",
                file.path, file.before, file.after
            )?;
        }
        writeln!(writer)?;
        Ok(())
    }

    fn write_health_language_changes(
        &self,
        languages: &[HealthLanguageChangePresentation],
        writer: &mut dyn Write,
    ) -> Result<()> {
        if languages.is_empty() {
            return Ok(());
        }
        writeln!(writer, "## Language Health Changes")?;
        writeln!(writer)?;
        writeln!(writer, "| Language | Status | Before | After | Delta |")?;
        writeln!(writer, "|----------|--------|--------|-------|-------|")?;
        for language in languages {
            writeln!(
                writer,
                "| {} | {} | {} | {} | {} |",
                language.language, language.status, language.before, language.after, language.delta,
            )?;
        }
        writeln!(writer)?;
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
