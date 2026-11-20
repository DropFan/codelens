//! Console rendering for health reports.

use std::io::Write;

use colored::Colorize;
use comfy_table::{presets::UTF8_FULL, Attribute, Cell, Color, ContentArrangement, Table};

use crate::error::Result;
use crate::insight::health::HealthAssessment;
use crate::output::format::{
    HealthLanguageChangePresentation, HealthPresentation, HealthRegressedFilePresentation,
    HealthRegressionVerdict, OutputOptions,
};

use super::ConsoleOutput;

impl ConsoleOutput {
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
        writeln!(writer)?;
        writeln!(writer, "{}", "═".repeat(60).dimmed())?;
        writeln!(
            writer,
            "{}",
            " CODELENS - Code Health Report ".bold().cyan()
        )?;
        writeln!(writer, "{}", "═".repeat(60).dimmed())?;
        writeln!(writer)?;

        // Project score and grade
        if report.assessment == HealthAssessment::InsufficientData {
            writeln!(
                writer,
                "  Project Assessment: {}  Estimated Score: {}  Model: {}",
                "INSUFFICIENT DATA".yellow().bold(),
                presentation.score.bold(),
                presentation.model,
            )?;
        } else {
            let grade_color = Self::grade_color(report.grade);
            let grade_str = report.grade.to_string();
            let colored_grade = match grade_color {
                Color::Green => grade_str.green().bold().to_string(),
                Color::Cyan => grade_str.cyan().bold().to_string(),
                Color::Yellow => grade_str.yellow().bold().to_string(),
                Color::Red => grade_str.red().bold().to_string(),
                Color::DarkRed => grade_str.red().bold().to_string(),
                _ => grade_str.bold().to_string(),
            };
            writeln!(
                writer,
                "  Project Score: {}  Grade: {}  Model: {}",
                presentation.score.bold(),
                colored_grade,
                presentation.model,
            )?;
        }
        writeln!(
            writer,
            "  Health Scope: {}  Measurement Confidence: {}",
            presentation.scope, presentation.confidence,
        )?;
        writeln!(
            writer,
            "  Worst 10% File Average: {}  F-grade Files: {} ({})",
            presentation.tail_score, presentation.failing_files, presentation.failing_ratio,
        )?;
        if let Some(directory) = &presentation.worst_directory {
            writeln!(
                writer,
                "  Worst Directory: {}  {} ({})  {} files",
                directory.path, directory.grade, directory.score_display, directory.file_count,
            )?;
        }
        if let Some(test_health) = &presentation.test_health {
            writeln!(writer, "  Test Code Health: {test_health}")?;
        }
        if presentation.failing_files > 0 {
            writeln!(
                writer,
                "  Risk: {} file(s) are graded F despite the overall {} grade.",
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
        let delta = if regression.score_nonnegative {
            regression.score_delta.green().to_string()
        } else {
            regression.score_delta.red().to_string()
        };
        writeln!(
            writer,
            "  Baseline: {}  Model: {}  {}  Δ {}",
            regression.baseline.bold(),
            regression.model,
            regression.transition,
            delta
        )?;
        if let Some((baseline, current)) = &regression.scope_change {
            writeln!(
                writer,
                "  Scope changed: {baseline} → {current} (project delta is informational)"
            )?;
        }
        if regression.insufficient_data {
            writeln!(
                writer,
                "  Assessment: insufficient data (score delta is an estimate)"
            )?;
        }
        if regression.improved_files > 0 {
            writeln!(writer, "  Improved files: {}", regression.improved_files)?;
        }
        self.write_health_regressed_files(&regression.regressed_files, writer)?;
        self.write_health_language_changes(&regression.languages, writer)?;
        let verdict = match regression.verdict {
            HealthRegressionVerdict::InsufficientData => {
                "INSUFFICIENT DATA".yellow().bold().to_string()
            }
            HealthRegressionVerdict::Regressed => "REGRESSED".red().bold().to_string(),
            HealthRegressionVerdict::NoRegression => "NO REGRESSION".green().bold().to_string(),
        };
        writeln!(writer, "  Verdict: {verdict}")?;
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
        writeln!(writer)?;
        writeln!(writer, "{}", "  Regressed Files".bold())?;
        let mut table = health_table(&["File", "Before", "After"]);
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
        writeln!(writer)?;
        writeln!(writer, "{}", "  Language Health Changes".bold())?;
        let mut table = health_table(&["Language", "Status", "Before", "After", "Delta"]);
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
        Ok(())
    }

    fn write_health_dimensions(
        &self,
        presentation: &HealthPresentation,
        writer: &mut dyn Write,
    ) -> Result<()> {
        let mut dim_table = health_table(&["Dimension", "Score", "Grade"]);
        for dimension in &presentation.dimensions {
            dim_table.add_row(vec![
                Cell::new(dimension.dimension.to_string()),
                Cell::new(&dimension.score_display),
                Cell::new(dimension.grade.to_string()).fg(Self::grade_color(dimension.grade)),
            ]);
        }
        writeln!(writer, "{dim_table}")?;
        writeln!(writer)?;
        Ok(())
    }

    fn write_health_languages(
        &self,
        presentation: &HealthPresentation,
        writer: &mut dyn Write,
    ) -> Result<()> {
        if !presentation.languages.is_empty() {
            writeln!(writer, "{}", "By Language".bold())?;
            writeln!(writer)?;
            let mut language_table = health_table(&[
                "Language",
                "Score",
                "Grade",
                "Files",
                "Code",
                "Confidence",
                "Tail 10%",
            ]);
            for language in &presentation.languages {
                language_table.add_row(vec![
                    Cell::new(&language.language).fg(Color::Cyan),
                    Cell::new(if language.estimated {
                        format!("est. {}", language.score_display)
                    } else {
                        language.score_display.clone()
                    }),
                    if language.estimated {
                        Cell::new("Insufficient data").fg(Color::Yellow)
                    } else {
                        Cell::new(&language.grade_display).fg(Self::grade_color(language.grade))
                    },
                    Cell::new(Self::format_number(language.file_count)),
                    Cell::new(Self::format_number(language.code_lines)),
                    Cell::new(&language.confidence),
                    Cell::new(&language.tail_score),
                ]);
            }
            writeln!(writer, "{language_table}")?;
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
            writeln!(writer, "{}", "By Directory".bold())?;
            writeln!(writer)?;
            let mut dir_table = health_table(&["Directory", "Score", "Grade", "Files"]);
            for directory in &presentation.directories {
                dir_table.add_row(vec![
                    Cell::new(&directory.path).fg(Color::Cyan),
                    Cell::new(&directory.score_display),
                    Cell::new(directory.grade.to_string()).fg(Self::grade_color(directory.grade)),
                    Cell::new(Self::format_number(directory.file_count)),
                ]);
            }
            writeln!(writer, "{dir_table}")?;
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
            writeln!(writer, "{}", "Worst Files".bold())?;
            writeln!(writer)?;
            let mut file_table = health_table(&["File", "Score", "Grade", "Top Issue"]);
            for file in &presentation.worst_files {
                file_table.add_row(vec![
                    Cell::new(&file.path).fg(Color::Cyan),
                    Cell::new(&file.score_display),
                    Cell::new(file.grade.to_string()).fg(Self::grade_color(file.grade)),
                    Cell::new(file.top_issue.to_string()).fg(Color::Yellow),
                ]);
            }
            writeln!(writer, "{file_table}")?;
            writeln!(writer)?;
        }
        Ok(())
    }
}

fn health_table(headers: &[&str]) -> Table {
    let mut table = Table::new();
    table
        .load_preset(UTF8_FULL)
        .set_content_arrangement(ContentArrangement::Dynamic);
    table.set_header(
        headers
            .iter()
            .map(|header| Cell::new(header).add_attribute(Attribute::Bold))
            .collect::<Vec<_>>(),
    );
    table
}
