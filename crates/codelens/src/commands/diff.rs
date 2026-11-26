//! `codelens diff` — compare two git refs (or a ref against the working
//! tree) with the health delta as the primary output.

use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::{Context, Result};
use colored::Colorize;

use codelens_core::analyze;
use codelens_core::git::GitClient;
use codelens_core::output::Report;

use super::{
    compare::{analyze_at_git_ref, prepare_comparison},
    load_partial_config, resolve_config, write_report,
};
use crate::cli;

pub(crate) fn run_diff(args: &cli::DiffArgs, advanced: &cli::AdvancedArgs) -> Result<ExitCode> {
    let partial = load_partial_config(advanced)?;
    let config = resolve_config(&args.filter, &args.output, advanced, partial.as_ref());
    let cwd = PathBuf::from(".");
    let git_client = GitClient::detect(&cwd).context("Not a git repository")?;
    let paths = vec![cwd];

    let (from_ref, to_ref) = resolve_refs(&git_client, args)?;
    let submodules = git_client.submodule_paths();
    let from_result = analyze_at_git_ref(&git_client, &from_ref, &paths, &config)?;
    let (to_result, to_label) = analyze_target(&git_client, to_ref.as_deref(), &paths, &config)?;
    let Some(prepared) = prepare_comparison(
        from_result,
        &from_ref,
        to_result,
        &to_label,
        Some(git_client.repo_path()),
        &submodules,
    ) else {
        return Ok(ExitCode::FAILURE);
    };

    let model = prepared.scoring_model(config.health_model);
    let report = codelens_core::insight::diff::build(
        &from_ref,
        &to_label,
        &prepared.baseline,
        &prepared.current,
        model.as_ref(),
    );
    let assessment = report.health.assessment;
    let failed = report.health.gate_failed();
    write_report(Report::Diff(Box::new(report)), &config.output)?;

    if args.fail_on_regression && failed {
        print_gate_failure(assessment, &from_ref, &to_label);
        return Ok(ExitCode::FAILURE);
    }
    Ok(ExitCode::SUCCESS)
}

fn resolve_refs(git_client: &GitClient, args: &cli::DiffArgs) -> Result<(String, Option<String>)> {
    Ok(match parse_ref_args(&args.from, args.to.as_deref())? {
        RefSpec::MergeBase { from, to } => {
            let base = git_client
                .merge_base(&from, &to)
                .with_context(|| format!("no merge base between '{from}' and '{to}'"))?;
            (base[..base.len().min(12)].to_string(), Some(to))
        }
        RefSpec::Plain { from, to } => (from, to),
    })
}

fn analyze_target(
    git_client: &GitClient,
    to_ref: Option<&str>,
    paths: &[PathBuf],
    config: &codelens_core::Config,
) -> Result<(codelens_core::AnalysisResult, String)> {
    Ok(match to_ref {
        Some(reference) => (
            analyze_at_git_ref(git_client, reference, paths, config)?,
            reference.to_string(),
        ),
        None => {
            let result = analyze(paths, config).context("Analysis failed")?;
            (result, "worktree".to_string())
        }
    })
}

fn print_gate_failure(
    assessment: codelens_core::insight::health::HealthAssessment,
    from_ref: &str,
    to_label: &str,
) {
    if assessment == codelens_core::insight::health::HealthAssessment::InsufficientData {
        eprintln!(
            "{}: comparison from '{from_ref}' to '{to_label}' has insufficient measurement coverage; its score delta is an estimate and cannot satisfy --fail-on-regression",
            "gate failed".red().bold(),
        );
    } else {
        eprintln!(
            "{}: health regressed from '{from_ref}' to '{to_label}'",
            "gate failed".red().bold(),
        );
    }
}

/// How the FROM/TO CLI arguments resolve into refs, before any git lookup.
#[derive(Debug, PartialEq)]
enum RefSpec {
    /// `FROM...TO`: compare from the merge base of the two refs.
    MergeBase { from: String, to: String },
    /// Plain refs; a missing `to` means the working tree.
    Plain { from: String, to: Option<String> },
}

/// Accept "FROM..TO", git's "FROM...TO" (compare from the merge base),
/// or FROM TO as two arguments. An empty side means HEAD, like git.
/// Mixing both forms would silently discard one TO — reject it instead.
fn parse_ref_args(from: &str, to: Option<&str>) -> Result<RefSpec> {
    // Covers "..." too; git forbids ".." inside ref names.
    if to.is_some() && from.contains("..") {
        anyhow::bail!("cannot combine FROM..TO syntax with a separate TO argument");
    }
    let head_if_empty = |r: &str| if r.is_empty() { "HEAD" } else { r }.to_string();
    Ok(if let Some((a, b)) = from.split_once("...") {
        RefSpec::MergeBase {
            from: head_if_empty(a),
            to: head_if_empty(b),
        }
    } else if let Some((a, b)) = from.split_once("..") {
        RefSpec::Plain {
            from: head_if_empty(a),
            to: Some(head_if_empty(b)),
        }
    } else {
        RefSpec::Plain {
            from: from.to_string(),
            to: to.map(String::from),
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn range_from_plus_positional_to_is_an_error() {
        // `codelens diff v1.0..v2.0 v3.0` used to silently ignore v3.0
        // and compare v1.0..v2.0 — reject the ambiguous combination.
        assert!(parse_ref_args("v1.0..v2.0", Some("v3.0")).is_err());
        assert!(parse_ref_args("v1.0...v2.0", Some("v3.0")).is_err());
    }

    #[test]
    fn plain_and_range_forms_parse() {
        assert_eq!(
            parse_ref_args("v1.0", Some("v2.0")).unwrap(),
            RefSpec::Plain {
                from: "v1.0".into(),
                to: Some("v2.0".into()),
            }
        );
        assert_eq!(
            parse_ref_args("v1.0", None).unwrap(),
            RefSpec::Plain {
                from: "v1.0".into(),
                to: None,
            }
        );
        assert_eq!(
            parse_ref_args("v1.0..v2.0", None).unwrap(),
            RefSpec::Plain {
                from: "v1.0".into(),
                to: Some("v2.0".into()),
            }
        );
        assert_eq!(
            parse_ref_args("v1.0...v2.0", None).unwrap(),
            RefSpec::MergeBase {
                from: "v1.0".into(),
                to: "v2.0".into(),
            }
        );
        // An empty side means HEAD, like git.
        assert_eq!(
            parse_ref_args("..v2.0", None).unwrap(),
            RefSpec::Plain {
                from: "HEAD".into(),
                to: Some("v2.0".into()),
            }
        );
    }
}
