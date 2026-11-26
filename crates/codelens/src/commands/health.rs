//! `codelens health` — score code health, optionally gated for CI via
//! `--fail-under` and `--fail-on-regression` against a baseline.

use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::{Context, Result};
use colored::Colorize;

use codelens_core::analyze;
use codelens_core::config::Config;
use codelens_core::git::GitClient;
use codelens_core::insight::{health, trend};
use codelens_core::output::Report;

use super::{
    compare::{
        analysis_is_valid, analyze_at_git_ref, prepare_comparison,
        rewrite_analysis_paths_repo_relative,
    },
    load_partial_config, resolve_config, scoring_model_for, write_report_targets,
};
use crate::cli;

mod gate;

use gate::{failure_reason, parse_fail_under};

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
    if !analysis_is_valid("worktree", &result) {
        return Ok(ExitCode::FAILURE);
    }
    let top_n = config.output.top_n.unwrap_or(10);

    let regression = if let Some(baseline_ref) = &args.baseline {
        let git_client = GitClient::detect(&args.paths[0]).ok();
        let baseline = resolve_baseline(baseline_ref, &args.paths, &config)?;
        let portable_snapshot = baseline
            .snapshot_version
            .is_some_and(|version| version >= trend::PORTABLE_PATH_VERSION);
        if portable_snapshot {
            rewrite_analysis_paths_repo_relative(&mut result, &args.paths[0]);
        }
        let mut submodules = git_client
            .as_ref()
            .map(GitClient::submodule_paths)
            .unwrap_or_default();
        if portable_snapshot {
            submodules = submodules_relative_to(&submodules, git_client.as_ref(), &args.paths[0]);
        }
        let Some(prepared) = prepare_comparison(
            baseline.result,
            &baseline.label,
            result,
            "worktree",
            (!portable_snapshot)
                .then(|| git_client.as_ref().map(GitClient::repo_path))
                .flatten(),
            &submodules,
        ) else {
            return Ok(ExitCode::FAILURE);
        };
        let compare_model = prepared.scoring_model(config.health_model);
        let comparison = health::compare_with_baseline(
            &prepared.baseline,
            &prepared.current,
            compare_model.as_ref(),
            &baseline.label,
        );
        result = prepared.current;
        Some(comparison)
    } else {
        None
    };

    // The main report and the absolute --fail-under gate always follow
    // what THIS analysis measured: the same tree must score the same no
    // matter how an unrelated baseline snapshot was collected.
    let model = scoring_model_for(config.health_model, result.summary.dup_scanned);
    let mut report = health::score(&result, model.as_ref(), top_n);
    report.regression = regression;
    let gate_failure = failure_reason(
        &report,
        threshold,
        args.fail_under.as_deref(),
        args.fail_on_regression,
        args.baseline.as_deref(),
    );
    let output_report = Report::Health(report);
    let additional_outputs = args
        .emit
        .iter()
        .map(|target| {
            let mut output = config.output.clone();
            output.format = target.format.into();
            output.file = Some(target.file.clone());
            output
        })
        .collect::<Vec<_>>();
    write_report_targets(
        &output_report,
        std::iter::once(&config.output).chain(additional_outputs.iter()),
    )?;

    if let Some(reason) = gate_failure {
        eprintln!("{}: {reason}", "gate failed".red().bold());
        return Ok(ExitCode::FAILURE);
    }
    Ok(ExitCode::SUCCESS)
}

/// Resolve a --baseline reference into an analyzed tree.
///
/// Snapshot references ("latest", "latest~N", date prefixes) win; anything
/// else is treated as a git ref and analyzed via a temporary worktree.
struct ResolvedBaseline {
    result: codelens_core::AnalysisResult,
    label: String,
    snapshot_version: Option<u32>,
}

fn resolve_baseline(
    reference: &str,
    paths: &[PathBuf],
    config: &Config,
) -> Result<ResolvedBaseline> {
    let project_root = paths.first().cloned().unwrap_or_else(|| PathBuf::from("."));

    if let Ok(snap_path) = trend::resolve_snapshot(&project_root, reference) {
        let snapshot = trend::load_snapshot(&snap_path)?;
        let label = snapshot
            .label
            .clone()
            .unwrap_or_else(|| snapshot.timestamp.format("%Y-%m-%d %H:%M").to_string());
        return Ok(ResolvedBaseline {
            result: snapshot.result,
            label: format!("snapshot {label}"),
            snapshot_version: Some(snapshot.version),
        });
    }

    let git_client = GitClient::detect(&project_root).with_context(|| {
        format!(
            "baseline '{reference}' is not a snapshot reference, and this is not a git repository"
        )
    })?;
    let result = analyze_at_git_ref(&git_client, reference, paths, config)?;
    Ok(ResolvedBaseline {
        result,
        label: format!("git:{reference}"),
        snapshot_version: None,
    })
}

fn submodules_relative_to(
    submodules: &[PathBuf],
    git_client: Option<&GitClient>,
    project_root: &PathBuf,
) -> Vec<PathBuf> {
    let Some(git_client) = git_client else {
        return Vec::new();
    };
    let root = std::fs::canonicalize(project_root).unwrap_or_else(|_| project_root.clone());
    submodules
        .iter()
        .filter_map(|submodule| {
            let absolute = git_client.repo_path().join(submodule);
            std::fs::canonicalize(absolute)
                .ok()
                .and_then(|path| path.strip_prefix(&root).ok().map(PathBuf::from))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[cfg(unix)]
    use std::os::unix::fs::PermissionsExt;

    #[cfg(unix)]
    #[test]
    fn unreadable_file_fails_health_gate() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("blocked.rs");
        std::fs::write(&source, "fn blocked() {}\n").unwrap();
        std::fs::set_permissions(&source, std::fs::Permissions::from_mode(0o000)).unwrap();

        // Root can still read mode-000 files. Normal CI and developer users
        // exercise the real unreadable-file path; skip only in that special
        // environment while still restoring permissions for cleanup.
        if std::fs::File::open(&source).is_ok() {
            std::fs::set_permissions(&source, std::fs::Permissions::from_mode(0o644)).unwrap();
            return;
        }

        let cli = cli::Cli::try_parse_from([
            "codelens",
            "health",
            dir.path().to_str().unwrap(),
            "--quiet",
            "--no-config",
        ])
        .unwrap();
        let Some(cli::Command::Health(args)) = &cli.command else {
            panic!("expected health subcommand");
        };
        let code = run_health(args, &cli.advanced).unwrap();

        std::fs::set_permissions(&source, std::fs::Permissions::from_mode(0o644)).unwrap();
        assert_eq!(format!("{code:?}"), format!("{:?}", ExitCode::FAILURE));
    }

    #[test]
    fn empty_analysis_fails_health_gate() {
        let dir = tempfile::tempdir().unwrap();
        let cli = cli::Cli::try_parse_from([
            "codelens",
            "health",
            dir.path().to_str().unwrap(),
            "--fail-under",
            "A",
            "--quiet",
            "--no-config",
        ])
        .unwrap();
        let Some(cli::Command::Health(args)) = &cli.command else {
            panic!("expected health subcommand");
        };

        let code = run_health(args, &cli.advanced).unwrap();

        assert_eq!(format!("{code:?}"), format!("{:?}", ExitCode::FAILURE));
    }

    #[test]
    fn fail_under_gate_follows_current_measurement_not_baseline() {
        // Current tree: heavy measured duplication drags the six-dim
        // score below the gate. A baseline snapshot that never measured
        // duplication must not renormalize the absolute score above the
        // threshold — the joint (both-sides-measured) model is only for
        // the regression comparison, never the main report or the
        // --fail-under gate.
        let dir = tempfile::tempdir().unwrap();
        let mut src = String::new();
        for i in 0..4 {
            src.push_str(&format!("# distinct comment line number {i}\n"));
        }
        for _ in 0..40 {
            src.push_str("value = compute_something(1, 2, 3)\n");
        }
        std::fs::write(dir.path().join("dup.py"), src).unwrap();

        // Valid baseline snapshot whose analysis skipped duplication
        // collection. It differs only in measurement scope, so an early
        // completeness failure cannot make this test pass accidentally.
        let baseline_config = Config {
            no_dup_scan: true,
            ..Config::default()
        };
        let baseline = analyze(&[dir.path()], &baseline_config).unwrap();
        assert!(!baseline.summary.dup_scanned);
        assert!(baseline.summary.total_files > 0);
        trend::save_snapshot(dir.path(), baseline, None, None, None).unwrap();

        let cli = cli::Cli::try_parse_from([
            "codelens",
            "health",
            dir.path().to_str().unwrap(),
            "--baseline",
            "latest",
            "--fail-under",
            "95",
            "-l",
            "python",
            "--no-config",
        ])
        .unwrap();
        let Some(cli::Command::Health(args)) = &cli.command else {
            panic!("expected health subcommand");
        };
        let code = run_health(args, &cli.advanced).unwrap();
        assert_eq!(
            format!("{code:?}"),
            format!("{:?}", ExitCode::FAILURE),
            "measured duplication must keep failing the gate no matter \
             how the baseline snapshot was collected"
        );
    }

    #[test]
    fn empty_baseline_is_rejected_even_without_a_gate_flag() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("current.rs"), "fn current() {}\n").unwrap();
        trend::save_snapshot(
            dir.path(),
            codelens_core::AnalysisResult {
                files: Vec::new(),
                summary: codelens_core::Summary::default(),
                elapsed: std::time::Duration::ZERO,
                scanned_files: 0,
                skipped_files: 0,
                error_files: 0,
            },
            None,
            None,
            None,
        )
        .unwrap();

        let cli = cli::Cli::try_parse_from([
            "codelens",
            "health",
            dir.path().to_str().unwrap(),
            "--baseline",
            "latest",
            "--quiet",
            "--no-config",
        ])
        .unwrap();
        let Some(cli::Command::Health(args)) = &cli.command else {
            panic!("expected health subcommand");
        };

        assert_eq!(
            format!("{:?}", run_health(args, &cli.advanced).unwrap()),
            format!("{:?}", ExitCode::FAILURE)
        );
    }

    #[test]
    fn snapshot_file_regressions_survive_project_moves() {
        let parent = tempfile::tempdir().unwrap();
        let original = parent.path().join("original");
        let moved = parent.path().join("moved");
        std::fs::create_dir_all(original.join("src")).unwrap();
        let healthy = include_str!("../../../codelens-core/src/error.rs");
        let unhealthy = include_str!("../../../codelens-core/src/output/html.rs");
        std::fs::write(original.join("src/a.rs"), healthy).unwrap();
        std::fs::write(original.join("src/b.rs"), unhealthy).unwrap();
        let baseline = analyze(&[&original], &Config::default()).unwrap();
        trend::save_snapshot(&original, baseline, None, None, None).unwrap();

        std::fs::rename(&original, &moved).unwrap();
        std::fs::write(moved.join("src/a.rs"), unhealthy).unwrap();
        std::fs::write(moved.join("src/b.rs"), healthy).unwrap();
        let output = parent.path().join("health.json");
        let cli = cli::Cli::try_parse_from([
            "codelens",
            "health",
            moved.to_str().unwrap(),
            "--baseline",
            "latest",
            "--fail-on-regression",
            "--no-config",
            "-f",
            "json",
            "-O",
            output.to_str().unwrap(),
        ])
        .unwrap();
        let Some(cli::Command::Health(args)) = &cli.command else {
            panic!("expected health subcommand");
        };

        let code = run_health(args, &cli.advanced).unwrap();
        let report: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(output).unwrap()).unwrap();
        let regressed = report["regression"]["regressed_files"].as_array().unwrap();

        assert_eq!(format!("{code:?}"), format!("{:?}", ExitCode::FAILURE));
        assert_eq!(regressed.len(), 1);
        assert!(regressed[0]["path"].as_str().unwrap().ends_with("src/a.rs"));
    }

    #[test]
    fn v2_insufficient_data_is_reportable_but_cannot_pass_fail_under() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("styles.css"), ".card { color: red; }\n").unwrap();

        let report_only = cli::Cli::try_parse_from([
            "codelens",
            "health",
            dir.path().to_str().unwrap(),
            "--health-model",
            "v2",
            "--quiet",
            "--no-config",
        ])
        .unwrap();
        let Some(cli::Command::Health(args)) = &report_only.command else {
            panic!("expected health subcommand");
        };
        let code = run_health(args, &report_only.advanced).unwrap();
        assert_eq!(format!("{code:?}"), format!("{:?}", ExitCode::SUCCESS));

        let gated = cli::Cli::try_parse_from([
            "codelens",
            "health",
            dir.path().to_str().unwrap(),
            "--health-model",
            "v2",
            "--fail-under",
            "A",
            "--quiet",
            "--no-config",
        ])
        .unwrap();
        let Some(cli::Command::Health(args)) = &gated.command else {
            panic!("expected health subcommand");
        };
        let code = run_health(args, &gated.advanced).unwrap();
        assert_eq!(format!("{code:?}"), format!("{:?}", ExitCode::FAILURE));
    }

    #[test]
    fn v2_insufficient_comparison_cannot_pass_regression_gate() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("styles.css"), ".card { color: red; }\n").unwrap();
        let baseline = analyze(&[dir.path()], &Config::default()).unwrap();
        trend::save_snapshot(dir.path(), baseline, None, None, None).unwrap();

        let cli = cli::Cli::try_parse_from([
            "codelens",
            "health",
            dir.path().to_str().unwrap(),
            "--health-model",
            "v2",
            "--baseline",
            "latest",
            "--fail-on-regression",
            "--quiet",
            "--no-config",
        ])
        .unwrap();
        let Some(cli::Command::Health(args)) = &cli.command else {
            panic!("expected health subcommand");
        };

        let code = run_health(args, &cli.advanced).unwrap();

        assert_eq!(format!("{code:?}"), format!("{:?}", ExitCode::FAILURE));
    }

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
