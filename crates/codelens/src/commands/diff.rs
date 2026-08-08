//! `codelens diff` — compare two git refs (or a ref against the working
//! tree) with the health delta as the primary output.

use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::{Context, Result};
use colored::Colorize;

use codelens_core::analyze;
use codelens_core::git::GitClient;
use codelens_core::insight::scoring::default::DefaultModel;
use codelens_core::output::Report;

use super::{
    analyze_at_git_ref, drop_submodule_files, load_partial_config, resolve_config,
    rewrite_paths_repo_relative, write_report,
};
use crate::cli;

pub(crate) fn run_diff(args: &cli::DiffArgs, advanced: &cli::AdvancedArgs) -> Result<ExitCode> {
    let partial = load_partial_config(advanced)?;
    let config = resolve_config(&args.filter, &args.output, advanced, partial.as_ref());
    let cwd = PathBuf::from(".");
    let git_client = GitClient::detect(&cwd).context("Not a git repository")?;
    let paths = vec![cwd];

    // Accept "FROM..TO", git's "FROM...TO" (compare from the merge base),
    // or FROM TO as two arguments. An empty side means HEAD, like git.
    let (from_ref, to_ref) = if let Some((a, b)) = args.from.split_once("...") {
        let a = if a.is_empty() { "HEAD" } else { a };
        let b = if b.is_empty() { "HEAD" } else { b };
        let base = git_client
            .merge_base(a, b)
            .with_context(|| format!("no merge base between '{a}' and '{b}'"))?;
        (base[..base.len().min(12)].to_string(), Some(b.to_string()))
    } else if let Some((a, b)) = args.from.split_once("..") {
        let a = if a.is_empty() { "HEAD" } else { a };
        let b = if b.is_empty() { "HEAD" } else { b };
        (a.to_string(), Some(b.to_string()))
    } else {
        (args.from.clone(), args.to.clone())
    };

    // Temporary worktrees never materialize submodules; exclude them on
    // both sides or a clean tree would diff non-zero against its own HEAD.
    let submodules = git_client.submodule_paths();

    let mut from_result = analyze_at_git_ref(&git_client, &from_ref, &paths, &config)?;
    drop_submodule_files(&mut from_result, &submodules);
    let (mut to_result, to_label) = match &to_ref {
        Some(reference) => (
            analyze_at_git_ref(&git_client, reference, &paths, &config)?,
            reference.clone(),
        ),
        None => {
            let mut result = analyze(&paths, &config).context("Analysis failed")?;
            rewrite_paths_repo_relative(&mut result.files, git_client.repo_path());
            (result, "worktree".to_string())
        }
    };
    drop_submodule_files(&mut to_result, &submodules);

    let model = DefaultModel::new();
    let report =
        codelens_core::insight::diff::build(&from_ref, &to_label, &from_result, &to_result, &model);
    let failed = report.health.failed;
    write_report(Report::Diff(Box::new(report)), &config.output)?;

    if args.fail_on_regression && failed {
        eprintln!(
            "{}: health regressed from '{from_ref}' to '{to_label}'",
            "gate failed".red().bold(),
        );
        return Ok(ExitCode::FAILURE);
    }
    Ok(ExitCode::SUCCESS)
}
