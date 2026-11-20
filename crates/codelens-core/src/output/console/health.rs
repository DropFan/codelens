//! Console rendering for health reports.

use std::io::Write;

use colored::Colorize;
use comfy_table::{presets::UTF8_FULL, Attribute, Cell, Color, ContentArrangement, Table};

use crate::error::Result;
use crate::insight::health::HealthAssessment;
use crate::output::format::{HealthPresentation, OutputOptions};

use super::{format_optional_delta, format_optional_health, ConsoleOutput};

impl ConsoleOutput {
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
        report: &crate::insight::health::HealthReport,
        writer: &mut dyn Write,
    ) -> Result<()> {
        if let Some(reg) = &report.regression {
            let arrow = format!(
                "{} ({:.1}) → {} ({:.1})",
                reg.baseline_grade, reg.baseline_score, reg.current_grade, reg.current_score
            );
            let delta = if reg.score_delta >= 0.0 {
                format!("+{:.1}", reg.score_delta).green().to_string()
            } else {
                format!("{:.1}", reg.score_delta).red().to_string()
            };
            writeln!(
                writer,
                "  Baseline: {}  Model: {}  {}  Δ {}",
                reg.baseline.bold(),
                reg.model,
                arrow,
                delta
            )?;
            if reg.scope_changed {
                writeln!(
                    writer,
                    "  Scope changed: {} → {} (project delta is informational)",
                    reg.baseline_scope, reg.current_scope
                )?;
            }
            if reg.assessment == HealthAssessment::InsufficientData {
                writeln!(
                    writer,
                    "  Assessment: insufficient data (score delta is an estimate)"
                )?;
            }
            if reg.improved_files > 0 {
                writeln!(writer, "  Improved files: {}", reg.improved_files)?;
            }

            if !reg.regressed_files.is_empty() {
                writeln!(writer)?;
                writeln!(writer, "{}", "  Regressed Files".bold())?;
                let mut reg_table = Table::new();
                reg_table
                    .load_preset(UTF8_FULL)
                    .set_content_arrangement(ContentArrangement::Dynamic);
                reg_table.set_header(vec![
                    Cell::new("File").add_attribute(Attribute::Bold),
                    Cell::new("Before").add_attribute(Attribute::Bold),
                    Cell::new("After").add_attribute(Attribute::Bold),
                ]);
                for f in &reg.regressed_files {
                    let before = if f.is_new {
                        "NEW".to_string()
                    } else {
                        format!("{} ({:.1})", f.from_grade, f.from_score)
                    };
                    reg_table.add_row(vec![
                        Cell::new(f.path.display().to_string()).fg(Color::Cyan),
                        Cell::new(before).fg(if f.is_new {
                            Color::Cyan
                        } else {
                            Self::grade_color(f.from_grade)
                        }),
                        Cell::new(format!("{} ({:.1})", f.to_grade, f.to_score))
                            .fg(Self::grade_color(f.to_grade)),
                    ]);
                }
                writeln!(writer, "{reg_table}")?;
            }

            if !reg.by_language.is_empty() {
                writeln!(writer)?;
                writeln!(writer, "{}", "  Language Health Changes".bold())?;
                let mut language_table = Table::new();
                language_table
                    .load_preset(UTF8_FULL)
                    .set_content_arrangement(ContentArrangement::Dynamic);
                language_table.set_header(vec![
                    Cell::new("Language").add_attribute(Attribute::Bold),
                    Cell::new("Status").add_attribute(Attribute::Bold),
                    Cell::new("Before").add_attribute(Attribute::Bold),
                    Cell::new("After").add_attribute(Attribute::Bold),
                    Cell::new("Delta").add_attribute(Attribute::Bold),
                ]);
                for language in &reg.by_language {
                    language_table.add_row(vec![
                        Cell::new(&language.language).fg(Color::Cyan),
                        Cell::new(language.status.to_string()),
                        Cell::new(format_optional_health(
                            language.from_grade,
                            language.from_score,
                        )),
                        Cell::new(format_optional_health(language.to_grade, language.to_score)),
                        Cell::new(format_optional_delta(language.score_delta)),
                    ]);
                }
                writeln!(writer, "{language_table}")?;
            }

            let verdict = if reg.assessment == HealthAssessment::InsufficientData {
                "INSUFFICIENT DATA".yellow().bold().to_string()
            } else if reg.failed {
                "REGRESSED".red().bold().to_string()
            } else {
                "NO REGRESSION".green().bold().to_string()
            };
            writeln!(writer, "  Verdict: {verdict}")?;
            writeln!(writer)?;
        }
        Ok(())
    }

    fn write_health_dimensions(
        &self,
        presentation: &HealthPresentation,
        writer: &mut dyn Write,
    ) -> Result<()> {
        let mut dim_table = Table::new();
        dim_table
            .load_preset(UTF8_FULL)
            .set_content_arrangement(ContentArrangement::Dynamic);
        dim_table.set_header(vec![
            Cell::new("Dimension").add_attribute(Attribute::Bold),
            Cell::new("Score").add_attribute(Attribute::Bold),
            Cell::new("Grade").add_attribute(Attribute::Bold),
        ]);
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
            let mut language_table = Table::new();
            language_table
                .load_preset(UTF8_FULL)
                .set_content_arrangement(ContentArrangement::Dynamic);
            language_table.set_header(vec![
                Cell::new("Language").add_attribute(Attribute::Bold),
                Cell::new("Score").add_attribute(Attribute::Bold),
                Cell::new("Grade").add_attribute(Attribute::Bold),
                Cell::new("Files").add_attribute(Attribute::Bold),
                Cell::new("Code").add_attribute(Attribute::Bold),
                Cell::new("Confidence").add_attribute(Attribute::Bold),
                Cell::new("Tail 10%").add_attribute(Attribute::Bold),
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
            let mut dir_table = Table::new();
            dir_table
                .load_preset(UTF8_FULL)
                .set_content_arrangement(ContentArrangement::Dynamic);
            dir_table.set_header(vec![
                Cell::new("Directory").add_attribute(Attribute::Bold),
                Cell::new("Score").add_attribute(Attribute::Bold),
                Cell::new("Grade").add_attribute(Attribute::Bold),
                Cell::new("Files").add_attribute(Attribute::Bold),
            ]);
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
            let mut file_table = Table::new();
            file_table
                .load_preset(UTF8_FULL)
                .set_content_arrangement(ContentArrangement::Dynamic);
            file_table.set_header(vec![
                Cell::new("File").add_attribute(Attribute::Bold),
                Cell::new("Score").add_attribute(Attribute::Bold),
                Cell::new("Grade").add_attribute(Attribute::Bold),
                Cell::new("Top Issue").add_attribute(Attribute::Bold),
            ]);
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
