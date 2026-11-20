//! Markdown rendering for diff reports.

use std::io::Write;

use crate::error::Result;
use crate::output::format::{
    DiffLanguagePresentation, DiffMetricPresentation, DiffPresentation,
    HealthLanguageChangePresentation, HealthRegressedFilePresentation,
};

use super::MarkdownOutput;

impl MarkdownOutput {
    pub(super) fn write_diff(
        &self,
        report: &crate::insight::diff::DiffReport,
        writer: &mut dyn Write,
    ) -> Result<()> {
        let presentation = DiffPresentation::from_report(report);
        self.write_diff_summary(&presentation, writer)?;
        self.write_diff_regressed_files(&presentation.health.regressed_files, writer)?;
        self.write_diff_language_health(&presentation.health.languages, writer)?;
        self.write_diff_metrics(&presentation.metrics, writer)?;
        self.write_diff_languages(&presentation.languages, writer)
    }

    fn write_diff_summary(
        &self,
        presentation: &DiffPresentation,
        writer: &mut dyn Write,
    ) -> Result<()> {
        let health = &presentation.health;
        writeln!(
            writer,
            "# Diff Report: {} → {}",
            presentation.from, presentation.to
        )?;
        writeln!(writer)?;
        writeln!(
            writer,
            "**Health [{}]:** {}, Δ {} — {}",
            health.model,
            health.transition,
            health.score_delta,
            if presentation.failed {
                "🔴 REGRESSED"
            } else {
                "🟢 NO REGRESSION"
            }
        )?;
        if let Some((baseline, current)) = &health.scope_change {
            writeln!(
                writer,
                "**Scope changed:** {baseline} → {current} (project delta is informational and does not affect the gate)"
            )?;
        }
        if health.insufficient_data {
            writeln!(
                writer,
                "**Assessment:** insufficient data (score delta is an estimate)"
            )?;
        }
        writeln!(writer)?;
        Ok(())
    }

    fn write_diff_regressed_files(
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

    fn write_diff_language_health(
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

    fn write_diff_metrics(
        &self,
        metrics: &[DiffMetricPresentation],
        writer: &mut dyn Write,
    ) -> Result<()> {
        writeln!(writer, "## Metrics")?;
        writeln!(writer)?;
        writeln!(writer, "| Metric | Before | After | Delta |")?;
        writeln!(writer, "|--------|--------|-------|-------|")?;
        for metric in metrics {
            writeln!(
                writer,
                "| {} | {} | {} | {:+} |",
                metric.label, metric.before, metric.after, metric.delta
            )?;
        }
        writeln!(writer)?;
        Ok(())
    }

    fn write_diff_languages(
        &self,
        languages: &[DiffLanguagePresentation],
        writer: &mut dyn Write,
    ) -> Result<()> {
        if languages.is_empty() {
            return Ok(());
        }
        writeln!(writer, "## By Language")?;
        writeln!(writer)?;
        writeln!(writer, "| Language | Code Before | Code After | Delta |")?;
        writeln!(writer, "|----------|-------------|------------|-------|")?;
        for language in languages {
            writeln!(
                writer,
                "| {} | {} | {} | {:+} |",
                language.language, language.code_before, language.code_after, language.code_delta
            )?;
        }
        writeln!(writer)?;
        Ok(())
    }
}
