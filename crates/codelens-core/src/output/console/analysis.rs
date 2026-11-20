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
        let view = AnalysisPresentation::from_result(result, options.top_n);

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
        table.add_row(vec![
            Cell::new("Total Size"),
            Cell::new(Self::format_size(view.total_size)),
        ]);
        table.add_row(vec![
            Cell::new("Languages"),
            Cell::new(view.language_count.to_string()),
        ]);
        table.add_row(vec![
            Cell::new("Functions"),
            Cell::new(Self::format_number(view.functions)),
        ]);
        if let Some(complexity) = &view.complexity {
            table.add_row(vec![
                Cell::new("Complexity (CC / Cognitive)"),
                Cell::new(format!(
                    "{} / {}",
                    Self::format_number(complexity.cyclomatic),
                    Self::format_number(complexity.cognitive)
                )),
            ]);
        }
        if let Some(tests) = &view.tests {
            table.add_row(vec![
                Cell::new("Test Files"),
                Cell::new(Self::format_number(tests.files)),
            ]);
            table.add_row(vec![
                Cell::new("Test Code Lines"),
                Cell::new(Self::format_number(tests.code_lines)),
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
                    Self::format_number(duplication.unique_lines),
                    duplication.dryness_percent
                )),
            ]);
        }
        if options.show_tokens {
            table.add_row(vec![
                Cell::new("LLM Tokens (est.)"),
                Cell::new(crate::analyzer::tokens::format_tokens(view.tokens_est))
                    .fg(Color::Magenta),
            ]);
        }

        writeln!(writer, "{table}")?;
        writeln!(writer)?;

        // Context-window fit (--tokens)
        if options.show_tokens && view.tokens_est > 0 {
            for window in crate::analyzer::tokens::CONTEXT_WINDOWS {
                let ratio = view.tokens_est as f64 / window.tokens as f64;
                let verdict = if ratio <= 1.0 {
                    format!("fits ({:.0}% used)", ratio * 100.0)
                        .green()
                        .to_string()
                } else {
                    format!("{ratio:.1}x over").yellow().to_string()
                };
                writeln!(writer, "  {}: {}", window.label.dimmed(), verdict)?;
            }
            writeln!(
                writer,
                "  {}",
                "token counts are byte-based estimates, not tokenizer-exact".dimmed()
            )?;
            writeln!(writer)?;
        }

        // Language breakdown
        if !options.summary_only && !view.languages.is_empty() {
            writeln!(writer, "{}", "By Language".bold())?;
            writeln!(writer)?;

            let mut lang_table = Table::new();
            lang_table
                .load_preset(UTF8_FULL)
                .set_content_arrangement(ContentArrangement::Dynamic);

            let mut headers = vec![
                Cell::new("Language").add_attribute(Attribute::Bold),
                Cell::new("Files").add_attribute(Attribute::Bold),
                Cell::new("Code").add_attribute(Attribute::Bold),
                Cell::new("Comment").add_attribute(Attribute::Bold),
                Cell::new("Blank").add_attribute(Attribute::Bold),
                Cell::new("Total").add_attribute(Attribute::Bold),
            ];
            if options.show_tokens {
                headers.push(Cell::new("Tokens").add_attribute(Attribute::Bold));
            }
            lang_table.set_header(headers);

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
                lang_table.add_row(row);
            }

            writeln!(writer, "{lang_table}")?;
            writeln!(writer)?;
        }

        // Per-directory breakdown (--by-dir)
        if options.by_dir && !result.files.is_empty() {
            writeln!(writer, "{}", "By Directory".bold())?;
            writeln!(writer)?;

            let mut dir_table = Table::new();
            dir_table
                .load_preset(UTF8_FULL)
                .set_content_arrangement(ContentArrangement::Dynamic);
            dir_table.set_header(vec![
                Cell::new("Directory").add_attribute(Attribute::Bold),
                Cell::new("Files").add_attribute(Attribute::Bold),
                Cell::new("Code").add_attribute(Attribute::Bold),
                Cell::new("Comment").add_attribute(Attribute::Bold),
                Cell::new("Blank").add_attribute(Attribute::Bold),
                Cell::new("Total").add_attribute(Attribute::Bold),
                Cell::new("CC").add_attribute(Attribute::Bold),
            ]);

            let dirs = crate::analyzer::stats::aggregate_by_dir(&result.files, options.dir_depth);
            for d in &dirs {
                let name = d
                    .path
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_else(|| d.path.display().to_string());
                let label = format!("{}{}/", "  ".repeat(d.depth - 1), name);
                dir_table.add_row(vec![
                    Cell::new(label).fg(Color::Cyan),
                    Cell::new(Self::format_number(d.files)),
                    Cell::new(Self::format_number(d.lines.code)).fg(Color::Green),
                    Cell::new(Self::format_number(d.lines.comment)).fg(Color::Yellow),
                    Cell::new(Self::format_number(d.lines.blank)).fg(Color::DarkGrey),
                    Cell::new(Self::format_number(d.lines.total)),
                    Cell::new(Self::format_number(d.cyclomatic)),
                ]);
            }

            writeln!(writer, "{dir_table}")?;
            writeln!(writer)?;
        }

        // Per-file breakdown (--by-file)
        if options.by_file && !result.files.is_empty() {
            writeln!(writer, "{}", "By File".bold())?;
            writeln!(writer)?;

            let mut file_table = Table::new();
            file_table
                .load_preset(UTF8_FULL)
                .set_content_arrangement(ContentArrangement::Dynamic);
            file_table.set_header(vec![
                Cell::new("File").add_attribute(Attribute::Bold),
                Cell::new("Language").add_attribute(Attribute::Bold),
                Cell::new("Code").add_attribute(Attribute::Bold),
                Cell::new("Comment").add_attribute(Attribute::Bold),
                Cell::new("Blank").add_attribute(Attribute::Bold),
                Cell::new("Total").add_attribute(Attribute::Bold),
            ]);

            for f in
                crate::output::format::sorted_files(&result.files, options.sort_by, options.top_n)
            {
                file_table.add_row(vec![
                    Cell::new(f.path.display().to_string()),
                    Cell::new(&f.language).fg(Color::Cyan),
                    Cell::new(Self::format_number(f.lines.code)).fg(Color::Green),
                    Cell::new(Self::format_number(f.lines.comment)).fg(Color::Yellow),
                    Cell::new(Self::format_number(f.lines.blank)).fg(Color::DarkGrey),
                    Cell::new(Self::format_number(f.lines.total)),
                ]);
            }

            writeln!(writer, "{file_table}")?;
            writeln!(writer)?;
        }

        // Footer — surface skipped/failed counts so partial results are visible
        writeln!(writer, "{}", "─".repeat(60).dimmed())?;
        let mut extras = Vec::new();
        if view.skipped_files > 0 {
            extras.push(format!("{} skipped", view.skipped_files));
        }
        if view.error_files > 0 {
            extras.push(format!("{} errors", view.error_files));
        }
        let extras_text = if extras.is_empty() {
            String::new()
        } else {
            format!(" ({})", extras.join(", "))
        };
        writeln!(
            writer,
            "Scanned {} files in {:.2}s{}",
            view.scanned_files.to_string().green(),
            view.elapsed_secs,
            if view.error_files > 0 {
                extras_text.yellow().to_string()
            } else {
                extras_text.dimmed().to_string()
            }
        )?;

        Ok(())
    }
}
