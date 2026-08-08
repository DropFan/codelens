//! `codelens coupling` — file pairs that keep changing together in git
//! history (change coupling).

use std::path::PathBuf;

use anyhow::{Context, Result};
use colored::Colorize;

use codelens_core::analyze;
use codelens_core::git::{self, GitClient};
use codelens_core::insight::coupling;
use codelens_core::output::Report;

use super::{load_partial_config, resolve_config, rewrite_paths_repo_relative, write_report};
use crate::cli;

pub(crate) fn run_coupling(args: &cli::CouplingArgs, advanced: &cli::AdvancedArgs) -> Result<()> {
    let partial = load_partial_config(advanced)?;
    let config = resolve_config(&args.filter, &args.output, advanced, partial.as_ref());
    let git_client = GitClient::detect(&args.paths[0]).context("Not a git repository")?;
    let since = git::parse_since(&args.since);

    // Restrict pairing to the files present in the current tree (after
    // filters), so deleted files and excluded directories stay out.
    let mut result = analyze(&args.paths, &config).context("Analysis failed")?;
    rewrite_paths_repo_relative(&mut result.files, git_client.repo_path());
    let universe: std::collections::BTreeSet<PathBuf> = result
        .files
        .iter()
        .map(|f| f.path.strip_prefix("./").unwrap_or(&f.path).to_path_buf())
        .collect();

    let commits = git_client.commit_log(&since)?;
    if commits.is_empty() {
        eprintln!(
            "{}: no commits found since '{}'; check the --since format (e.g. 30d, 4w, 6m, 1y, YYYY-MM-DD)",
            "warning".yellow().bold(),
            args.since
        );
    }

    let opts = coupling::CouplingOptions {
        min_shared: args.min_shared,
        min_degree: args.min_coupling,
        max_changeset: args.max_changeset,
        top_n: config.output.top_n.unwrap_or(20),
        focus: args.focus.clone(),
    };
    let report = coupling::analyze(&commits, Some(&universe), &args.since, &opts);
    write_report(Report::Coupling(report), &config.output)
}
