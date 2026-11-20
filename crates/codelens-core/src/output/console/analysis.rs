//! Console rendering for analysis reports.

use std::io::Write;

use colored::Colorize;
use comfy_table::{presets::UTF8_FULL, Attribute, Cell, Color, ContentArrangement, Table};

use crate::analyzer::stats::AnalysisResult;
use crate::error::Result;
use crate::output::format::{AnalysisPresentation, OutputOptions};

use super::ConsoleOutput;

impl ConsoleOutput {
    pub(super) fn write_analysis(
        &self,
        result: &AnalysisResult,
        options: &OutputOptions,
        writer: &mut dyn Write,
    ) -> Result<()> {
        let view = AnalysisPresentation::from_result(result, options);
        self.write_analysis_header(writer)?;
        self.write_analysis_summary(&view, options, writer)?;
        self.write_analysis_token_windows(&view, writer)?;
        self.write_analysis_languages(&view, options, writer)?;
        self.write_analysis_directories(&view, writer)?;
        self.write_analysis_files(&view, writer)?;
        self.write_analysis_footer(&view, writer)
    }

    fn write_analysis_header(&self, writer: &mut dyn Write) -> Result<()> {
        writeln!(writer)?;
        writeln!(writer, "{}", "═".repeat(60).dimmed())?;
        writeln!(
            writer,
            "{}",
            " CODELENS - Code Statistics Report ".bold().cyan()
        )?;
        writeln!(writer, "{}", "═".repeat(60).dimmed())?;
        writeln!(writer)?;
        Ok(())
    }

    fn write_analysis_summary(
        &self,
        view: &AnalysisPresentation,
        options: &OutputOptions,
        writer: &mut dyn Write,
    ) -> Result<()> {
        let mut table = analysis_table(&["Metric", "Value"]);
        table.add_row(vec![
            Cell::new("Total Files"),
            Cell::new(Self::format_number(view.total_files)).fg(Color::Green),
        ]);
        table.add_row(vec![
            Cell::new("Code Lines"),
            Cell::new(Self::format_number(view.code_lines)).fg(Color::Cyan),
        ]);
        table.add_row(vec![
            Cell::new("Comment Lines"),
            Cell::new(Self::format_number(view.comment_lines)).fg(Color::Yellow),
        ]);
        table.add_row(vec![
            Cell::new("Blank Lines"),
            Cell::new(Self::format_number(view.blank_lines)).fg(Color::DarkGrey),
        ]);
        table.add_row(vec![
            Cell::new("Total Lines"),
            Cell::new(Self::format_number(view.total_lines)).add_attribute(Attribute::Bold),
        ]);
        add_analysis_summary_details(&mut table, view, options);
        writeln!(writer, "{table}")?;
        writeln!(writer)?;
        Ok(())
    }

    fn write_analysis_token_windows(
        &self,
        view: &AnalysisPresentation,
        writer: &mut dyn Write,
    ) -> Result<()> {
        if view.token_windows.is_empty() {
            return Ok(());
        }
        for window in &view.token_windows {
            let verdict = match (window.fit_percent, window.over_ratio) {
                (Some(percent), _) => format!("fits ({percent:.0}% used)").green().to_string(),
                (_, Some(ratio)) => format!("{ratio:.1}x over").yellow().to_string(),
                _ => unreachable!("token window presentation always has a verdict"),
            };
            writeln!(writer, "  {}: {}", window.label.dimmed(), verdict)?;
        }
        writeln!(
            writer,
            "  {}",
            "token counts are byte-based estimates, not tokenizer-exact".dimmed()
        )?;
        writeln!(writer)?;
        Ok(())
    }

    fn write_analysis_languages(
        &self,
        view: &AnalysisPresentation,
        options: &OutputOptions,
        writer: &mut dyn Write,
    ) -> Result<()> {
        if !view.show_languages {
            return Ok(());
        }
        writeln!(writer, "{}", "By Language".bold())?;
        writeln!(writer)?;
        let mut headers = vec!["Language", "Files", "Code", "Comment", "Blank", "Total"];
        if options.show_tokens {
            headers.push("Tokens");
        }
        let mut table = analysis_table(&headers);
        for language in &view.languages {
            let mut row = vec![
                Cell::new(&language.name).fg(Color::Cyan),
                Cell::new(Self::format_number(language.files)),
                Cell::new(Self::format_number(language.code_lines)).fg(Color::Green),
                Cell::new(Self::format_number(language.comment_lines)).fg(Color::Yellow),
                Cell::new(Self::format_number(language.blank_lines)).fg(Color::DarkGrey),
                Cell::new(Self::format_number(language.total_lines)),
            ];
            if options.show_tokens {
                row.push(
                    Cell::new(crate::analyzer::tokens::format_tokens(language.tokens_est))
                        .fg(Color::Magenta),
                );
            }
            table.add_row(row);
        }
        writeln!(writer, "{table}")?;
        writeln!(writer)?;
        Ok(())
    }

    fn write_analysis_directories(
        &self,
        view: &AnalysisPresentation,
        writer: &mut dyn Write,
    ) -> Result<()> {
        if view.directories.is_empty() {
            return Ok(());
        }
        writeln!(writer, "{}", "By Directory".bold())?;
        writeln!(writer)?;
        let mut table = analysis_table(&[
            "Directory",
            "Files",
            "Code",
            "Comment",
            "Blank",
            "Total",
            "CC",
        ]);
        for directory in &view.directories {
            let label = format!("{}{}/", "  ".repeat(directory.depth - 1), directory.name);
            table.add_row(vec![
                Cell::new(label).fg(Color::Cyan),
                Cell::new(Self::format_number(directory.files)),
                Cell::new(Self::format_number(directory.code_lines)).fg(Color::Green),
                Cell::new(Self::format_number(directory.comment_lines)).fg(Color::Yellow),
                Cell::new(Self::format_number(directory.blank_lines)).fg(Color::DarkGrey),
                Cell::new(Self::format_number(directory.total_lines)),
                Cell::new(Self::format_number(directory.cyclomatic)),
            ]);
        }
        writeln!(writer, "{table}")?;
        writeln!(writer)?;
        Ok(())
    }

    fn write_analysis_files(
        &self,
        view: &AnalysisPresentation,
        writer: &mut dyn Write,
    ) -> Result<()> {
        if view.files.is_empty() {
            return Ok(());
        }
        writeln!(writer, "{}", "By File".bold())?;
        writeln!(writer)?;
        let mut table = analysis_table(&["File", "Language", "Code", "Comment", "Blank", "Total"]);
        for file in &view.files {
            table.add_row(vec![
                Cell::new(&file.path),
                Cell::new(&file.language).fg(Color::Cyan),
                Cell::new(Self::format_number(file.code_lines)).fg(Color::Green),
                Cell::new(Self::format_number(file.comment_lines)).fg(Color::Yellow),
                Cell::new(Self::format_number(file.blank_lines)).fg(Color::DarkGrey),
                Cell::new(Self::format_number(file.total_lines)),
            ]);
        }
        writeln!(writer, "{table}")?;
        writeln!(writer)?;
        Ok(())
    }

    fn write_analysis_footer(
        &self,
        view: &AnalysisPresentation,
        writer: &mut dyn Write,
    ) -> Result<()> {
        writeln!(writer, "{}", "─".repeat(60).dimmed())?;
        let mut extras = Vec::new();
        if view.skipped_files > 0 {
            extras.push(format!("{} skipped", view.skipped_files));
        }
        if view.error_files > 0 {
            extras.push(format!("{} errors", view.error_files));
        }
        let extras = if extras.is_empty() {
            String::new()
        } else {
            format!(" ({})", extras.join(", "))
        };
        let extras = if view.error_files > 0 {
            extras.yellow().to_string()
        } else {
            extras.dimmed().to_string()
        };
        writeln!(
            writer,
            "Scanned {} files in {:.2}s{}",
            view.scanned_files.to_string().green(),
            view.elapsed_secs,
            extras
        )?;
        Ok(())
    }
}

fn analysis_table(headers: &[&str]) -> Table {
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

fn add_analysis_summary_details(
    table: &mut Table,
    view: &AnalysisPresentation,
    options: &OutputOptions,
) {
    table.add_row(vec![
        Cell::new("Total Size"),
        Cell::new(ConsoleOutput::format_size(view.total_size)),
    ]);
    table.add_row(vec![
        Cell::new("Languages"),
        Cell::new(view.language_count.to_string()),
    ]);
    table.add_row(vec![
        Cell::new("Functions"),
        Cell::new(ConsoleOutput::format_number(view.functions)),
    ]);
    if let Some(complexity) = &view.complexity {
        table.add_row(vec![
            Cell::new("Complexity (CC / Cognitive)"),
            Cell::new(format!(
                "{} / {}",
                ConsoleOutput::format_number(complexity.cyclomatic),
                ConsoleOutput::format_number(complexity.cognitive)
            )),
        ]);
    }
    if let Some(tests) = &view.tests {
        table.add_row(vec![
            Cell::new("Test Files"),
            Cell::new(ConsoleOutput::format_number(tests.files)),
        ]);
        table.add_row(vec![
            Cell::new("Test Code Lines"),
            Cell::new(ConsoleOutput::format_number(tests.code_lines)),
        ]);
        if let Some(ratio) = tests.code_ratio {
            table.add_row(vec![
                Cell::new("Test/Code Ratio"),
                Cell::new(format!("{ratio:.2}")).fg(Color::Cyan),
            ]);
        }
    }
    if let Some(duplication) = &view.duplication {
        table.add_row(vec![
            Cell::new("ULOC / DRYness"),
            Cell::new(format!(
                "{} / {:.0}%",
                ConsoleOutput::format_number(duplication.unique_lines),
                duplication.dryness_percent
            )),
        ]);
    }
    if options.show_tokens {
        table.add_row(vec![
            Cell::new("LLM Tokens (est.)"),
            Cell::new(crate::analyzer::tokens::format_tokens(view.tokens_est)).fg(Color::Magenta),
        ]);
    }
}
