//! Console rendering for hotspot reports.

use std::io::Write;

use colored::Colorize;
use comfy_table::{presets::UTF8_FULL, Attribute, Cell, Color, ContentArrangement, Table};

use crate::error::Result;
use crate::insight::hotspot::{HotspotReport, RiskLevel};
use crate::output::format::{
    HotspotFilePresentation, HotspotFunctionFilePresentation, HotspotKnowledgeIslandPresentation,
    HotspotPresentation,
};

use super::ConsoleOutput;

pub(super) fn write(report: &HotspotReport, writer: &mut dyn Write) -> Result<()> {
    let presentation = HotspotPresentation::from_report(report);
    write_summary(&presentation, writer)?;
    if presentation.files.is_empty() {
        writeln!(writer, "  No hotspots found.")?;
        return Ok(());
    }

    write_files(&presentation.files, writer)?;
    write_knowledge_islands(&presentation.knowledge_islands, writer)?;
    write_functions(&presentation.function_files, writer)
}

fn write_summary(presentation: &HotspotPresentation, writer: &mut dyn Write) -> Result<()> {
    writeln!(writer)?;
    writeln!(writer, "{}", "═".repeat(60).dimmed())?;
    writeln!(writer, "{}", " CODELENS - Hotspot Analysis ".bold().cyan())?;
    writeln!(writer, "{}", "═".repeat(60).dimmed())?;
    writeln!(writer)?;
    writeln!(
        writer,
        "  Period: {}  Total Commits: {}",
        presentation.since.bold(),
        ConsoleOutput::format_number(presentation.total_commits).bold()
    )?;
    writeln!(writer)?;
    Ok(())
}

fn write_files(files: &[HotspotFilePresentation], writer: &mut dyn Write) -> Result<()> {
    let mut table = Table::new();
    table
        .load_preset(UTF8_FULL)
        .set_content_arrangement(ContentArrangement::Dynamic);
    table.set_header(vec![
        Cell::new("File").add_attribute(Attribute::Bold),
        Cell::new("Chg").add_attribute(Attribute::Bold),
        Cell::new("+/-").add_attribute(Attribute::Bold),
        Cell::new("CC").add_attribute(Attribute::Bold),
        Cell::new("Age").add_attribute(Attribute::Bold),
        Cell::new("Auth").add_attribute(Attribute::Bold),
        Cell::new("Score").add_attribute(Attribute::Bold),
        Cell::new("Risk").add_attribute(Attribute::Bold),
    ]);

    for file in files {
        table.add_row(vec![
            Cell::new(&file.path).fg(Color::Cyan),
            Cell::new(ConsoleOutput::format_number(file.commits)),
            Cell::new(format!("+{}/-{}", file.lines_added, file.lines_deleted)),
            Cell::new(file.cyclomatic.to_string()),
            Cell::new(&file.age),
            Cell::new(&file.authors),
            Cell::new(&file.score),
            Cell::new(file.risk.to_string()).fg(risk_color(file.risk)),
        ]);
    }

    writeln!(writer, "{table}")?;
    writeln!(writer)?;
    Ok(())
}

fn risk_color(risk: RiskLevel) -> Color {
    match risk {
        RiskLevel::High => Color::Red,
        RiskLevel::Medium => Color::Yellow,
        RiskLevel::Low => Color::Green,
    }
}

fn write_knowledge_islands(
    islands: &[HotspotKnowledgeIslandPresentation],
    writer: &mut dyn Write,
) -> Result<()> {
    if islands.is_empty() {
        return Ok(());
    }

    writeln!(
        writer,
        "{} {}",
        "★ Knowledge Islands".bold().red(),
        "(risky files effectively one person knows)".dimmed()
    )?;
    for island in islands {
        writeln!(
            writer,
            "    {}  {:.0}% by {}",
            island.path.cyan(),
            island.ownership_percent,
            island.main_author
        )?;
    }
    writeln!(writer)?;
    Ok(())
}

fn write_functions(
    files: &[HotspotFunctionFilePresentation],
    writer: &mut dyn Write,
) -> Result<()> {
    if files.is_empty() {
        return Ok(());
    }

    writeln!(
        writer,
        "{} {}",
        "Function Hotspots".bold(),
        "(approximate spans)".dimmed()
    )?;
    writeln!(writer)?;
    for file in files {
        writeln!(writer, "  {}", file.path.cyan().bold())?;
        for function in &file.functions {
            writeln!(
                writer,
                "    {:<32} L{:<9} {} commits  CC {}",
                function.name,
                format!("{}-{}", function.start_line, function.end_line),
                function.commits,
                function.cyclomatic,
            )?;
        }
    }
    writeln!(writer)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn risk_colors_match_existing_console_convention() {
        assert_eq!(risk_color(RiskLevel::High), Color::Red);
        assert_eq!(risk_color(RiskLevel::Medium), Color::Yellow);
        assert_eq!(risk_color(RiskLevel::Low), Color::Green);
    }
}
