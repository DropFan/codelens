//! `codelens trend` — save metric snapshots and compare them over time.

use std::path::PathBuf;

use anyhow::{Context, Result};
use colored::Colorize;

use codelens_core::analyze;
use codelens_core::git::GitClient;
use codelens_core::insight::trend;
use codelens_core::output::Report;

use super::{load_partial_config, resolve_config, write_report};
use crate::cli;

pub(crate) fn run_trend(args: &cli::TrendArgs, advanced: &cli::AdvancedArgs) -> Result<()> {
    let project_root = args
        .paths
        .first()
        .cloned()
        .unwrap_or_else(|| PathBuf::from("."));

    if args.list {
        let metas = trend::list_snapshots(&project_root)?;
        if metas.is_empty() {
            println!("No snapshots found. Use --save to create one.");
            return Ok(());
        }
        for meta in &metas {
            let label = meta.label.as_deref().unwrap_or("");
            let commit = meta.git_commit.as_deref().unwrap_or("");
            println!(
                "  {}  {}  {}",
                meta.timestamp.format("%Y-%m-%d %H:%M:%S"),
                label,
                commit
            );
        }
        println!("\nTotal: {} snapshots", metas.len());
        return Ok(());
    }

    if args.save {
        // Use the same filter semantics as the main command so snapshot
        // numbers stay comparable with `codelens` output.
        let partial = load_partial_config(advanced)?;
        let config = resolve_config(&args.filter, &args.output, advanced, partial.as_ref());
        let result = analyze(&[&project_root], &config).context("Analysis failed")?;
        let (git_commit, git_branch) = GitClient::detect(&project_root)
            .and_then(|c| c.repo_info())
            .map(|info| (info.commit, info.branch))
            .unwrap_or((None, None));
        let path = trend::save_snapshot(
            &project_root,
            result,
            args.label.clone(),
            git_commit,
            git_branch,
        )?;
        println!("Snapshot saved to: {}", path.display().to_string().green());
        return Ok(());
    }

    let (from_ref, to_ref) = if let Some(ref refs) = args.compare {
        (refs[0].as_str(), refs[1].as_str())
    } else {
        ("latest~1", "latest")
    };

    let mut report = trend::diff(&project_root, from_ref, to_ref)?;
    // Full history feeds the HTML chart and JSON consumers; losing it
    // (e.g. one unreadable snapshot) never blocks the comparison.
    report.history = trend::history(&project_root).unwrap_or_default();
    let output_config = trend_output_config(args);
    write_report(Report::Trend(report), &output_config)
}

/// Trend's list/diff paths don't run an analysis; build output settings
/// directly from CLI args.
fn trend_output_config(args: &cli::TrendArgs) -> codelens_core::config::OutputConfig {
    let mut output = codelens_core::config::OutputConfig::default();
    if let Some(format) = args.output.format {
        output.format = format.into();
    }
    if let Some(ref path) = args.output.output_file {
        output.file = Some(path.clone());
    }
    if let Some(sort) = args.output.sort {
        output.sort_by = sort.into();
    }
    output.summary_only = args.output.summary;
    output.top_n = args.output.top;
    output.quiet = args.output.quiet;
    output
}
