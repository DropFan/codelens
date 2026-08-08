//! `codelens health` — score code health, optionally gated for CI via
//! `--fail-under` and `--fail-on-regression` against a baseline.

use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::{Context, Result};
use colored::Colorize;

use codelens_core::analyze;
use codelens_core::config::Config;
use codelens_core::git::GitClient;
use codelens_core::insight::scoring::default::DefaultModel;
use codelens_core::insight::{health, trend};
use codelens_core::output::Report;

use super::{
    analyze_at_git_ref, drop_submodule_files, load_partial_config, resolve_config,
    rewrite_paths_repo_relative, write_report,
};
use crate::cli;

pub(crate) fn run_health(args: &cli::HealthArgs, advanced: &cli::AdvancedArgs) -> Result<ExitCode> {
    // Validate the threshold BEFORE the (potentially long) analysis
    let threshold = args
        .fail_under
        .as_deref()
        .map(parse_fail_under)
        .transpose()?;

    let partial = load_partial_config(advanced)?;
    let config = resolve_config(&args.filter, &args.output, advanced, partial.as_ref());
    let mut result = analyze(&args.paths, &config).context("Analysis failed")?;
    let model = DefaultModel::new();
    let top_n = config.output.top_n.unwrap_or(10);

    let regression = if let Some(baseline_ref) = &args.baseline {
        // Normalize current paths to repo-root-relative so they line up
        // with the baseline tree regardless of the working directory.
        let git_client = GitClient::detect(&args.paths[0]).ok();
        if let Some(client) = &git_client {
            rewrite_paths_repo_relative(&mut result.files, client.repo_path());
        }
        let (mut baseline_result, label) = resolve_baseline(baseline_ref, &args.paths, &config)?;
        // Worktree-materialized baselines never contain submodules;
        // exclude them on both sides for a like-for-like comparison.
        if let Some(client) = &git_client {
            let submodules = client.submodule_paths();
            drop_submodule_files(&mut result, &submodules);
            drop_submodule_files(&mut baseline_result, &submodules);
        }
        Some(health::compare_with_baseline(
            &baseline_result,
            &result,
            &model,
            &label,
        ))
    } else {
        None
    };

    let mut report = health::score(&result, &model, top_n);
    report.regression = regression;
    let score = report.score;
    let grade = report.grade;
    let regression_failed = report.regression.as_ref().is_some_and(|r| r.failed);
    write_report(Report::Health(report), &config.output)?;

    if let Some(threshold) = threshold {
        if score < threshold {
            eprintln!(
                "{}: health score {:.1} (grade {}) is below --fail-under threshold {}",
                "gate failed".red().bold(),
                score,
                grade,
                args.fail_under.as_deref().unwrap_or_default(),
            );
            return Ok(ExitCode::FAILURE);
        }
    }
    if args.fail_on_regression && regression_failed {
        eprintln!(
            "{}: health regressed against baseline '{}'",
            "gate failed".red().bold(),
            args.baseline.as_deref().unwrap_or_default(),
        );
        return Ok(ExitCode::FAILURE);
    }
    Ok(ExitCode::SUCCESS)
}

/// Resolve a --baseline reference into an analyzed tree.
///
/// Snapshot references ("latest", "latest~N", date prefixes) win; anything
/// else is treated as a git ref and analyzed via a temporary worktree.
fn resolve_baseline(
    reference: &str,
    paths: &[PathBuf],
    config: &Config,
) -> Result<(codelens_core::AnalysisResult, String)> {
    let project_root = paths.first().cloned().unwrap_or_else(|| PathBuf::from("."));

    if let Ok(snap_path) = trend::resolve_snapshot(&project_root, reference) {
        let snapshot = trend::load_snapshot(&snap_path)?;
        let label = snapshot
            .label
            .clone()
            .unwrap_or_else(|| snapshot.timestamp.format("%Y-%m-%d %H:%M").to_string());
        return Ok((snapshot.result, format!("snapshot {label}")));
    }

    let git_client = GitClient::detect(&project_root).with_context(|| {
        format!(
            "baseline '{reference}' is not a snapshot reference, and this is not a git repository"
        )
    })?;
    let result = analyze_at_git_ref(&git_client, reference, paths, config)?;
    Ok((result, format!("git:{reference}")))
}

/// Parse a `--fail-under` threshold: a grade letter (A/B/C/D, using the
/// scoring model's grade boundaries) or a numeric score (0-100).
fn parse_fail_under(input: &str) -> Result<f64> {
    match input.trim() {
        "A" | "a" => Ok(90.0),
        "B" | "b" => Ok(80.0),
        "C" | "c" => Ok(70.0),
        "D" | "d" => Ok(60.0),
        other => {
            if let Ok(score) = other.parse::<f64>() {
                if (0.0..=100.0).contains(&score) {
                    return Ok(score);
                }
            }
            anyhow::bail!(
                "invalid --fail-under value '{other}': expected a grade (A/B/C/D) or a score between 0 and 100"
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fail_under_accepts_grades_and_scores() {
        assert_eq!(parse_fail_under("A").unwrap(), 90.0);
        assert_eq!(parse_fail_under("b").unwrap(), 80.0);
        assert_eq!(parse_fail_under("C").unwrap(), 70.0);
        assert_eq!(parse_fail_under("D").unwrap(), 60.0);
        assert_eq!(parse_fail_under("75").unwrap(), 75.0);
        assert_eq!(parse_fail_under("62.5").unwrap(), 62.5);

        // F would always pass — reject it along with garbage and out-of-range
        assert!(parse_fail_under("F").is_err());
        assert!(parse_fail_under("great").is_err());
        assert!(parse_fail_under("101").is_err());
        assert!(parse_fail_under("-1").is_err());
    }
}
