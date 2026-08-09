//! `codelens coupling` — file pairs that keep changing together in git
//! history (change coupling).

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use colored::Colorize;

use codelens_core::analyze;
use codelens_core::analyzer::test_code::is_test_path;
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
    let mut universe: BTreeSet<PathBuf> = result
        .files
        .iter()
        .map(|f| f.path.strip_prefix("./").unwrap_or(&f.path).to_path_buf())
        .collect();

    // Test files are excluded by default: a test changing together with
    // the code it tests is expected behavior, not a hidden dependency.
    // Note that bulk-commit detection (--max-changeset) still sees the
    // commit's full file count, so excluding tests here never shrinks a
    // bulk commit under the threshold.
    let (include_tests, excluded_test_files) =
        apply_test_filter(&mut universe, args.include_tests, args.focus.as_deref());
    if include_tests && !args.include_tests {
        eprintln!(
            "{}: '--for' targets a test file; test files included (--include-tests implied)",
            "note".cyan().bold()
        );
    }

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
    let mut report = coupling::analyze(&commits, Some(&universe), &args.since, &opts);
    report.excluded_test_files = excluded_test_files;
    write_report(Report::Coupling(report), &config.output)
}

/// Drop test files from the pairing universe unless they are included.
///
/// Returns the effective include-tests decision and the number of files
/// removed. Focusing on a test file (`--for foo_test.go`) implies
/// `--include-tests`: the focus itself would otherwise be filtered away
/// and the report always empty.
fn apply_test_filter(
    universe: &mut BTreeSet<PathBuf>,
    include_tests: bool,
    focus: Option<&Path>,
) -> (bool, usize) {
    if include_tests || focus.is_some_and(is_test_path) {
        return (true, 0);
    }
    let before = universe.len();
    universe.retain(|p| !is_test_path(p));
    (false, before - universe.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn universe(paths: &[&str]) -> BTreeSet<PathBuf> {
        paths.iter().map(PathBuf::from).collect()
    }

    #[test]
    fn default_filters_test_files_and_counts_them() {
        let mut set = universe(&["src/a.rs", "src/a_test.rs", "tests/b.py"]);
        let (include, excluded) = apply_test_filter(&mut set, false, None);
        assert!(!include);
        assert_eq!(excluded, 2);
        assert_eq!(set, universe(&["src/a.rs"]));
    }

    #[test]
    fn include_tests_flag_keeps_everything() {
        let mut set = universe(&["src/a.rs", "src/a_test.rs", "tests/b.py"]);
        let (include, excluded) = apply_test_filter(&mut set, true, None);
        assert!(include);
        assert_eq!(excluded, 0);
        assert_eq!(set.len(), 3);
    }

    #[test]
    fn focus_on_test_file_implies_include_tests() {
        let mut set = universe(&["src/a.rs", "src/a_test.rs", "tests/b.py"]);
        let (include, excluded) =
            apply_test_filter(&mut set, false, Some(Path::new("src/a_test.rs")));
        assert!(include);
        assert_eq!(excluded, 0);
        assert_eq!(set.len(), 3);
    }

    #[test]
    fn focus_on_regular_file_still_filters() {
        let mut set = universe(&["src/a.rs", "src/a_test.rs", "tests/b.py"]);
        let (include, excluded) = apply_test_filter(&mut set, false, Some(Path::new("src/a.rs")));
        assert!(!include);
        assert_eq!(excluded, 2);
        assert_eq!(set, universe(&["src/a.rs"]));
    }
}
