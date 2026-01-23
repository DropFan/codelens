//! Console output with colors and formatting.

use std::io::Write;

use colored::Colorize;
use comfy_table::{presets::UTF8_FULL, Attribute, Cell, Color, ContentArrangement, Table};

use crate::analyzer::stats::AnalysisResult;
use crate::error::Result;

use super::format::{OutputFormat, OutputOptions};

/// Console output formatter.
pub struct ConsoleOutput;

impl ConsoleOutput {
    /// Create a new console output formatter.
    pub fn new() -> Self {
        Self
    }

    fn format_size(bytes: u64) -> String {
        const KB: u64 = 1024;
        const MB: u64 = KB * 1024;
        const GB: u64 = MB * 1024;

        if bytes >= GB {
            format!("{:.2} GB", bytes as f64 / GB as f64)
        } else if bytes >= MB {
            format!("{:.2} MB", bytes as f64 / MB as f64)
        } else if bytes >= KB {
            format!("{:.2} KB", bytes as f64 / KB as f64)
        } else {
            format!("{} B", bytes)
        }
    }

    fn format_number(n: usize) -> String {
        let s = n.to_string();
        let mut result = String::new();
        for (i, c) in s.chars().rev().enumerate() {
            if i > 0 && i % 3 == 0 {
                result.push(',');
            }
            result.push(c);
        }
        result.chars().rev().collect()
    }
}

impl Default for ConsoleOutput {
    fn default() -> Self {
        Self::new()
    }
}

impl OutputFormat for ConsoleOutput {
    fn name(&self) -> &'static str {
        "console"
    }

    fn extension(&self) -> &'static str {
        "txt"
    }

    fn write(
        &self,
        result: &AnalysisResult,
        options: &OutputOptions,
        writer: &mut dyn Write,
    ) -> Result<()> {
        let summary = &result.summary;

        // Header
        writeln!(writer)?;
        writeln!(writer, "{}", "═".repeat(60).dimmed())?;
        writeln!(
            writer,
            "{}",
            " CODELENS - Code Statistics Report ".bold().cyan()
        )?;
        writeln!(writer, "{}", "═".repeat(60).dimmed())?;
        writeln!(writer)?;

        // Summary table
        let mut table = Table::new();
        table
            .load_preset(UTF8_FULL)
            .set_content_arrangement(ContentArrangement::Dynamic);

        table.set_header(vec![
            Cell::new("Metric").add_attribute(Attribute::Bold),
            Cell::new("Value").add_attribute(Attribute::Bold),
        ]);

        table.add_row(vec![
            Cell::new("Total Files"),
            Cell::new(Self::format_number(summary.total_files)).fg(Color::Green),
        ]);
        table.add_row(vec![
            Cell::new("Code Lines"),
            Cell::new(Self::format_number(summary.lines.code)).fg(Color::Cyan),
        ]);
        table.add_row(vec![
            Cell::new("Comment Lines"),
            Cell::new(Self::format_number(summary.lines.comment)).fg(Color::Yellow),
        ]);
        table.add_row(vec![
            Cell::new("Blank Lines"),
            Cell::new(Self::format_number(summary.lines.blank)).fg(Color::DarkGrey),
        ]);
        table.add_row(vec![
            Cell::new("Total Lines"),
            Cell::new(Self::format_number(summary.lines.total)).add_attribute(Attribute::Bold),
        ]);
        table.add_row(vec![
            Cell::new("Total Size"),
            Cell::new(Self::format_size(summary.total_size)),
        ]);
        table.add_row(vec![
            Cell::new("Languages"),
            Cell::new(summary.by_language.len().to_string()),
        ]);
        table.add_row(vec![
            Cell::new("Functions"),
            Cell::new(Self::format_number(summary.complexity.functions)),
        ]);

        writeln!(writer, "{table}")?;
        writeln!(writer)?;

        // Language breakdown
        if !options.summary_only && !summary.by_language.is_empty() {
            writeln!(writer, "{}", "By Language".bold())?;
            writeln!(writer)?;

            let mut lang_table = Table::new();
            lang_table
                .load_preset(UTF8_FULL)
                .set_content_arrangement(ContentArrangement::Dynamic);

            lang_table.set_header(vec![
                Cell::new("Language").add_attribute(Attribute::Bold),
                Cell::new("Files").add_attribute(Attribute::Bold),
                Cell::new("Code").add_attribute(Attribute::Bold),
                Cell::new("Comment").add_attribute(Attribute::Bold),
                Cell::new("Blank").add_attribute(Attribute::Bold),
                Cell::new("Total").add_attribute(Attribute::Bold),
            ]);

            let mut langs: Vec<_> = summary.by_language.iter().collect();

            // Apply top_n limit
            if let Some(n) = options.top_n {
                langs.truncate(n);
            }

            for (name, stats) in langs {
                lang_table.add_row(vec![
                    Cell::new(name).fg(Color::Cyan),
                    Cell::new(Self::format_number(stats.files)),
                    Cell::new(Self::format_number(stats.lines.code)).fg(Color::Green),
                    Cell::new(Self::format_number(stats.lines.comment)).fg(Color::Yellow),
                    Cell::new(Self::format_number(stats.lines.blank)).fg(Color::DarkGrey),
                    Cell::new(Self::format_number(stats.lines.total)),
                ]);
            }

            writeln!(writer, "{lang_table}")?;
            writeln!(writer)?;
        }

        // Footer
        writeln!(writer, "{}", "─".repeat(60).dimmed())?;
        writeln!(
            writer,
            "Scanned {} files in {:.2}s",
            result.scanned_files.to_string().green(),
            result.elapsed.as_secs_f64()
        )?;

        Ok(())
    }
}
