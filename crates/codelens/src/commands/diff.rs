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
    analyze_at_git_ref, drop_submodule_files, load_partial_config, resolve_config,
    rewrite_paths_repo_relative, scoring_model_for, write_report,
};
use crate::cli;

pub(crate) fn run_diff(args: &cli::DiffArgs, advanced: &cli::AdvancedArgs) -> Result<ExitCode> {
    let partial = load_partial_config(advanced)?;
    let config = resolve_config(&args.filter, &args.output, advanced, partial.as_ref());
    let cwd = PathBuf::from(".");
    let git_client = GitClient::detect(&cwd).context("Not a git repository")?;
    let paths = vec![cwd];

    let (from_ref, to_ref) = match parse_ref_args(&args.from, args.to.as_deref())? {
        RefSpec::MergeBase { from, to } => {
            let base = git_client
                .merge_base(&from, &to)
                .with_context(|| format!("no merge base between '{from}' and '{to}'"))?;
            (base[..base.len().min(12)].to_string(), Some(to))
        }
        RefSpec::Plain { from, to } => (from, to),
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

    if !analysis_is_valid(&from_ref, &from_result) || !analysis_is_valid(&to_label, &to_result) {
        return Ok(ExitCode::FAILURE);
    }

    // Both sides were analyzed with the same config, so the duplication
    // dimension is either measured on both or excluded on both.
    let model = scoring_model_for(!config.no_dup_scan);
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

/// A diff is only meaningful when both inputs were analyzed completely and
/// contain at least one matching file.
fn analysis_is_valid(label: &str, result: &codelens_core::AnalysisResult) -> bool {
    if result.error_files > 0 {
        eprintln!(
            "{}: analysis of '{label}' skipped {} file(s) because they could not be read or analyzed",
            "gate failed".red().bold(),
            result.error_files,
        );
        return false;
    }
    if result.summary.total_files == 0 {
        eprintln!(
            "{}: analysis of '{label}' found no files matching the requested filters",
            "gate failed".red().bold(),
        );
        return false;
    }
    true
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

    fn analysis_result(total_files: usize, error_files: usize) -> codelens_core::AnalysisResult {
        let files = (0..total_files)
            .map(|index| codelens_core::FileStats {
                path: PathBuf::from(format!("file-{index}.rs")),
                language: "Rust".to_string(),
                ..Default::default()
            })
            .collect::<Vec<_>>();
        codelens_core::AnalysisResult {
            summary: codelens_core::Summary::from_file_stats(&files),
            files,
            elapsed: std::time::Duration::ZERO,
            scanned_files: total_files,
            skipped_files: 0,
            error_files,
        }
    }

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

    #[test]
    fn diff_rejects_empty_or_incomplete_analysis_on_either_side() {
        let valid = analysis_result(1, 0);
        let empty = analysis_result(0, 0);
        let incomplete = analysis_result(1, 1);

        assert!(analysis_is_valid("from", &valid));
        assert!(!analysis_is_valid("from", &empty));
        assert!(!analysis_is_valid("to", &empty));
        assert!(!analysis_is_valid("from", &incomplete));
        assert!(!analysis_is_valid("to", &incomplete));
    }
}
