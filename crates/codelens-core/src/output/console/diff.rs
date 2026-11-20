//! Console rendering for diff reports.

use std::io::Write;

use colored::Colorize;
use comfy_table::{presets::UTF8_FULL, Attribute, Cell, Color, ContentArrangement, Table};

use crate::error::Result;
use crate::output::format::{
    DiffMetricPresentation, DiffPresentation, HealthLanguageChangePresentation,
    HealthRegressedFilePresentation,
};

use super::ConsoleOutput;

impl ConsoleOutput {
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
        self.write_diff_verdict(presentation.failed, writer)
    }

    fn write_diff_summary(
        &self,
        presentation: &DiffPresentation,
        writer: &mut dyn Write,
    ) -> Result<()> {
        writeln!(writer)?;
        writeln!(writer, "{}", "═".repeat(60).dimmed())?;
        writeln!(writer, "{}", " CODELENS - Diff Report ".bold().cyan())?;
        writeln!(writer, "{}", "═".repeat(60).dimmed())?;
        writeln!(writer)?;
        writeln!(
            writer,
            "  {} → {}",
            presentation.from.bold(),
            presentation.to.bold()
        )?;

        let health = &presentation.health;
        let delta = if health.score_nonnegative {
            health.score_delta.green().to_string()
        } else {
            health.score_delta.red().to_string()
        };
        writeln!(
            writer,
            "  Health [{}]: {}  Δ {}",
            health.model, health.transition, delta
        )?;
        if let Some((baseline, current)) = &health.scope_change {
            writeln!(
                writer,
                "  Scope changed: {baseline} → {current} (project delta is informational)"
            )?;
        }
        if health.insufficient_data {
            writeln!(
                writer,
                "  Assessment: insufficient data (score delta is an estimate)"
            )?;
        }
        if health.improved_files > 0 {
            writeln!(writer, "  Improved files: {}", health.improved_files)?;
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
        writeln!(writer, "{}", "  Regressed Files".bold())?;
        let mut table = diff_table(&["File", "Before", "After"]);
        for file in files {
            table.add_row(vec![
                Cell::new(&file.path).fg(Color::Cyan),
                Cell::new(&file.before).fg(if file.is_new {
                    Color::Cyan
                } else {
                    Self::grade_color(file.from_grade)
                }),
                Cell::new(&file.after).fg(Self::grade_color(file.to_grade)),
            ]);
        }
        writeln!(writer, "{table}")?;
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
        writeln!(writer, "{}", "  Language Health Changes".bold())?;
        let mut table = diff_table(&["Language", "Status", "Before", "After", "Delta"]);
        for language in languages {
            table.add_row(vec![
                Cell::new(&language.language).fg(Color::Cyan),
                Cell::new(&language.status),
                Cell::new(&language.before),
                Cell::new(&language.after),
                Cell::new(&language.delta),
            ]);
        }
        writeln!(writer, "{table}")?;
        writeln!(writer)?;
        Ok(())
    }

    fn write_diff_metrics(
        &self,
        metrics: &[DiffMetricPresentation],
        writer: &mut dyn Write,
    ) -> Result<()> {
        let mut table = diff_table(&["Metric", "Before", "After", "Delta"]);
        for metric in metrics {
            let delta = if metric.delta > 0 {
                Cell::new(format!("+{}", metric.delta)).fg(Color::Yellow)
            } else if metric.delta < 0 {
                Cell::new(metric.delta.to_string()).fg(Color::Cyan)
            } else {
                Cell::new("0").fg(Color::DarkGrey)
            };
            table.add_row(vec![
                Cell::new(metric.label),
                Cell::new(Self::format_number(metric.before)),
                Cell::new(Self::format_number(metric.after)),
                delta,
            ]);
        }
        writeln!(writer, "{table}")?;
        writeln!(writer)?;
        Ok(())
    }

    fn write_diff_verdict(&self, failed: bool, writer: &mut dyn Write) -> Result<()> {
        let verdict = if failed {
            "REGRESSED".red().bold().to_string()
        } else {
            "NO REGRESSION".green().bold().to_string()
        };
        writeln!(writer, "  Verdict: {verdict}")?;
        writeln!(writer)?;
        Ok(())
    }
}

fn diff_table(headers: &[&str]) -> Table {
    let mut table = Table::new();
    table
        .load_preset(UTF8_FULL)
        .set_content_arrangement(ContentArrangement::Dynamic);
    table.set_header(
        headers
            .iter()
            .map(|header| Cell::new(*header).add_attribute(Attribute::Bold)),
    );
    table
}
