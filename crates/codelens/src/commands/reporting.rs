//! Shared report output plumbing.

use std::fs::File;
use std::io::{self, BufWriter, Write};

use anyhow::{Context, Result};
use codelens_core::output::{create_output, OutputOptions, Report};
use colored::Colorize;

use super::DefaultHtmlInsights;

fn should_colorize(output: &codelens_core::config::OutputConfig) -> bool {
    use std::io::IsTerminal;
    output.file.is_none() && io::stdout().is_terminal()
}

pub(crate) fn report_output_options(output: &codelens_core::config::OutputConfig) -> OutputOptions {
    OutputOptions {
        summary_only: output.summary_only,
        by_file: output.by_file,
        by_dir: output.by_dir,
        dir_depth: output.dir_depth,
        show_tokens: output.show_tokens,
        sort_by: output.sort_by,
        top_n: output.top_n,
        colorize: should_colorize(output),
        show_git_info: output.show_git_info,
    }
}

pub(crate) fn write_report(
    report: Report,
    output: &codelens_core::config::OutputConfig,
) -> Result<()> {
    write_report_targets(&report, std::iter::once(output))
}

pub(crate) fn write_report_targets<'a>(
    report: &Report,
    outputs: impl IntoIterator<Item = &'a codelens_core::config::OutputConfig>,
) -> Result<()> {
    for output in outputs {
        let output_options = report_output_options(output);
        let formatter = create_output(output.format);

        // Quiet suppresses terminal output only; an explicit -O file is still written.
        if output.quiet && output.file.is_none() {
            continue;
        }

        if let Some(ref path) = output.file {
            let file = File::create(path).context("Failed to create output file")?;
            let mut writer = BufWriter::new(file);
            formatter.write(report, &output_options, &mut writer)?;
            writer.flush()?;
            if !output.quiet {
                println!("Output written to: {}", path.display().to_string().green());
            }
        } else {
            let stdout = io::stdout();
            let mut writer = stdout.lock();
            formatter.write(report, &output_options, &mut writer)?;
        }
    }
    Ok(())
}

pub(super) fn write_combined_html_dashboard(
    combined: &codelens_core::output::CombinedReport,
    insights: &DefaultHtmlInsights,
    output: &codelens_core::config::OutputConfig,
) -> Result<()> {
    let output_options = report_output_options(output);
    let formatter = codelens_core::output::HtmlOutput::new();

    if output.quiet && output.file.is_none() {
        return Ok(());
    }

    if let Some(ref path) = output.file {
        let file = File::create(path).context("Failed to create output file")?;
        let mut writer = BufWriter::new(file);
        formatter.write_combined_dashboard(
            combined,
            insights.hotspot.as_ref(),
            insights.coupling.as_ref(),
            insights.trend.as_ref(),
            &insights.git_history_note,
            &insights.trend_note,
            &output_options,
            &mut writer,
        )?;
        writer.flush()?;
        if !output.quiet {
            println!("Output written to: {}", path.display().to_string().green());
        }
    } else {
        let stdout = io::stdout();
        let mut writer = stdout.lock();
        formatter.write_combined_dashboard(
            combined,
            insights.hotspot.as_ref(),
            insights.coupling.as_ref(),
            insights.trend.as_ref(),
            &insights.git_history_note,
            &insights.trend_note,
            &output_options,
            &mut writer,
        )?;
    }
    Ok(())
}
