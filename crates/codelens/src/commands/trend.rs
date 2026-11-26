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
        let result = analyze(&args.paths, &config).context("Analysis failed")?;
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
    let partial = load_partial_config(advanced)?;
    let config = resolve_config(&args.filter, &args.output, advanced, partial.as_ref());
    write_report(Report::Trend(report), &config.output)
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;
    use codelens_core::insight::trend::save_snapshot;

    fn empty_analysis() -> codelens_core::AnalysisResult {
        codelens_core::AnalysisResult {
            summary: codelens_core::Summary::default(),
            files: vec![],
            elapsed: std::time::Duration::ZERO,
            scanned_files: 0,
            skipped_files: 0,
            error_files: 0,
        }
    }

    #[test]
    fn compare_merges_config_file_then_cli_output_overrides() {
        let project = tempfile::tempdir().unwrap();
        save_snapshot(
            project.path(),
            empty_analysis(),
            Some("before".into()),
            None,
            None,
        )
        .unwrap();
        save_snapshot(
            project.path(),
            empty_analysis(),
            Some("after".into()),
            None,
            None,
        )
        .unwrap();

        let config_output = project.path().join("from-config.json");
        let config_path = project.path().join("codelens.toml");
        std::fs::write(
            &config_path,
            format!(
                "output = \"json\"\noutput_file = \"{}\"\n",
                config_output.display()
            ),
        )
        .unwrap();

        let cli = crate::cli::Cli::try_parse_from([
            "codelens",
            "trend",
            project.path().to_str().unwrap(),
            "--compare",
            "latest~1",
            "latest",
            "--config",
            config_path.to_str().unwrap(),
        ])
        .unwrap();
        let crate::cli::Command::Trend(args) = cli.command.as_ref().unwrap() else {
            panic!("expected trend command");
        };
        run_trend(args, &cli.advanced).unwrap();
        let json = std::fs::read_to_string(&config_output).unwrap();
        assert!(serde_json::from_str::<serde_json::Value>(&json).is_ok());

        let cli_output = project.path().join("from-cli.md");
        let cli = crate::cli::Cli::try_parse_from([
            "codelens",
            "trend",
            project.path().to_str().unwrap(),
            "--compare",
            "latest~1",
            "latest",
            "--config",
            config_path.to_str().unwrap(),
            "-f",
            "markdown",
            "-O",
            cli_output.to_str().unwrap(),
        ])
        .unwrap();
        let crate::cli::Command::Trend(args) = cli.command.as_ref().unwrap() else {
            panic!("expected trend command");
        };
        run_trend(args, &cli.advanced).unwrap();
        let markdown = std::fs::read_to_string(cli_output).unwrap();
        assert!(markdown.starts_with("# Trend Report"));
    }

    #[test]
    fn save_analyzes_every_requested_path() {
        let project = tempfile::tempdir().unwrap();
        let first = project.path().join("first");
        let second = project.path().join("second");
        std::fs::create_dir_all(&first).unwrap();
        std::fs::create_dir_all(&second).unwrap();
        std::fs::write(first.join("first.rs"), "fn first() {}\n").unwrap();
        std::fs::write(second.join("second.rs"), "fn second() {}\n").unwrap();

        let cli = crate::cli::Cli::try_parse_from([
            "codelens",
            "trend",
            first.to_str().unwrap(),
            second.to_str().unwrap(),
            "--save",
            "--no-config",
        ])
        .unwrap();
        let crate::cli::Command::Trend(args) = cli.command.as_ref().unwrap() else {
            panic!("expected trend command");
        };

        run_trend(args, &cli.advanced).unwrap();

        let path = trend::resolve_snapshot(&first, "latest").unwrap();
        let snapshot = trend::load_snapshot(&path).unwrap();
        assert_eq!(snapshot.result.summary.total_files, 2);
        assert!(snapshot
            .result
            .files
            .iter()
            .any(|stats| stats.path.ends_with("first.rs")));
        assert!(snapshot
            .result
            .files
            .iter()
            .any(|stats| stats.path.ends_with("second.rs")));
    }
}
