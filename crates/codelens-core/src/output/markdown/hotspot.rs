//! Markdown rendering for hotspot reports.

use std::io::Write;

use crate::error::Result;
use crate::insight::hotspot::HotspotReport;
use crate::output::format::{
    HotspotFilePresentation, HotspotFunctionFilePresentation, HotspotKnowledgeIslandPresentation,
    HotspotPresentation,
};

pub(super) fn write(report: &HotspotReport, writer: &mut dyn Write) -> Result<()> {
    let presentation = HotspotPresentation::from_report(report);
    write_summary(&presentation, writer)?;
    if presentation.files.is_empty() {
        writeln!(writer, "No hotspots found.")?;
        return Ok(());
    }

    write_files(&presentation.files, writer)?;
    write_knowledge_islands(&presentation.knowledge_islands, writer)?;
    write_functions(&presentation.function_files, writer)
}

fn write_summary(presentation: &HotspotPresentation, writer: &mut dyn Write) -> Result<()> {
    writeln!(writer, "# Hotspot Analysis")?;
    writeln!(writer)?;
    writeln!(
        writer,
        "**Period:** {} | **Total Commits:** {}",
        presentation.since, presentation.total_commits
    )?;
    writeln!(writer)?;
    Ok(())
}

fn write_files(files: &[HotspotFilePresentation], writer: &mut dyn Write) -> Result<()> {
    writeln!(
        writer,
        "| File | Chg | +/- | CC | Age | Authors | Score | Risk |"
    )?;
    writeln!(
        writer,
        "|------|-----|-----|----|-----|---------|-------|------|"
    )?;
    for file in files {
        writeln!(
            writer,
            "| {} | {} | +{}/-{} | {} | {} | {} | {} | {} |",
            file.path,
            file.commits,
            file.lines_added,
            file.lines_deleted,
            file.cyclomatic,
            file.age,
            file.authors,
            file.score,
            file.risk,
        )?;
    }
    writeln!(writer)?;
    Ok(())
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
        "**★ Knowledge Islands** (risky files effectively one person knows):"
    )?;
    writeln!(writer)?;
    for island in islands {
        writeln!(
            writer,
            "- {} — {:.0}% by {}",
            island.path, island.ownership_percent, island.main_author
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

    writeln!(writer, "## Function Hotspots (approximate spans)")?;
    writeln!(writer)?;
    writeln!(writer, "| File | Function | Lines | Commits | CC |")?;
    writeln!(writer, "|------|----------|-------|---------|-----|")?;
    for file in files {
        for function in &file.functions {
            writeln!(
                writer,
                "| {} | {} | {}-{} | {} | {} |",
                file.path,
                function.name,
                function.start_line,
                function.end_line,
                function.commits,
                function.cyclomatic,
            )?;
        }
    }
    writeln!(writer)?;
    Ok(())
}
