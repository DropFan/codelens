//! `codelens coupling` — file pairs that keep changing together in git
//! history (change coupling).

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use colored::Colorize;

use codelens_core::analyze;
use codelens_core::analyzer::test_code::is_test_path;
use codelens_core::git::{self, CommitRecord, GitClient};
use codelens_core::insight::coupling;
use codelens_core::output::Report;
use codelens_core::Config;

use super::{
    git_client_for_paths, load_partial_config, resolve_config, rewrite_paths_repo_relative,
    write_report,
};
use crate::cli;

#[derive(Debug, Clone)]
pub(crate) struct CouplingQuery {
    pub(crate) since: String,
    pub(crate) focus: Option<PathBuf>,
    pub(crate) min_shared: usize,
    pub(crate) min_coupling: f64,
    pub(crate) max_changeset: usize,
    pub(crate) top_n: usize,
    pub(crate) include_tests: bool,
}

impl Default for CouplingQuery {
    fn default() -> Self {
        let defaults = coupling::CouplingOptions::default();
        Self {
            since: "90d".to_string(),
            focus: defaults.focus,
            min_shared: defaults.min_shared,
            min_coupling: defaults.min_degree,
            max_changeset: defaults.max_changeset,
            top_n: defaults.top_n,
            include_tests: false,
        }
    }
}

pub(crate) struct CouplingRun {
    pub(crate) report: coupling::CouplingReport,
    pub(crate) tests_included_for_focus: bool,
    pub(crate) history_is_empty: bool,
}

pub(crate) fn run_coupling(args: &cli::CouplingArgs, advanced: &cli::AdvancedArgs) -> Result<()> {
    let partial = load_partial_config(advanced)?;
    let config = resolve_config(&args.filter, &args.output, advanced, partial.as_ref());
    let query = query_from_cli(args, &config);
    let run = analyze_coupling(&args.paths, &config, &query)?;

    if run.tests_included_for_focus {
        eprintln!(
            "{}: '--for' targets a test file; test files included (--include-tests implied)",
            "note".cyan().bold()
        );
    }
    if run.history_is_empty {
        eprintln!(
            "{}: no commits found since '{}'; check the --since format (e.g. 30d, 4w, 6m, 1y, YYYY-MM-DD)",
            "warning".yellow().bold(),
            args.since
        );
    }

    write_report(Report::Coupling(run.report), &config.output)
}

pub(crate) fn query_from_cli(args: &cli::CouplingArgs, config: &Config) -> CouplingQuery {
    let defaults = CouplingQuery::default();
    CouplingQuery {
        since: args.since.clone(),
        focus: args.focus.clone(),
        min_shared: args.min_shared,
        min_coupling: args.min_coupling,
        max_changeset: args.max_changeset,
        top_n: config.output.top_n.unwrap_or(defaults.top_n),
        include_tests: args.include_tests,
    }
}

/// Analyze change coupling with the same scope and filtering semantics for
/// every frontend. Presentation, output files, and exit codes stay with the
/// caller.
pub(crate) fn analyze_coupling(
    paths: &[PathBuf],
    config: &Config,
    query: &CouplingQuery,
) -> Result<CouplingRun> {
    let git_client = git_client_for_paths(paths)?;
    let since = git::parse_since(&query.since);
    let result = analyze(paths, config).context("Analysis failed")?;
    let commits = git_client.commit_log(&since)?;
    Ok(analyze_coupling_from_history(
        &result,
        &git_client,
        &commits,
        query,
    ))
}

/// Build a coupling report from an existing code scan and commit log.
pub(crate) fn analyze_coupling_from_history(
    analysis: &codelens_core::AnalysisResult,
    git_client: &GitClient,
    commits: &[CommitRecord],
    query: &CouplingQuery,
) -> CouplingRun {
    // Restrict pairing to the files present in the current tree (after
    // filters), so deleted files and excluded directories stay out.
    let mut result = analysis.clone();
    rewrite_paths_repo_relative(&mut result.files, git_client.repo_path());
    let mut universe: BTreeSet<PathBuf> = result
        .files
        .iter()
        .map(|f| f.path.strip_prefix("./").unwrap_or(&f.path).to_path_buf())
        .collect();

    // Test files are excluded by default: a test changing together with
    // the code it tests is expected behavior, not a hidden dependency.
    // Bulk-commit detection still sees the commit's full file count, so
    // filtering never turns a bulk commit into a small one.
    let (include_tests, excluded_test_files) =
        apply_test_filter(&mut universe, query.include_tests, query.focus.as_deref());

    let opts = coupling::CouplingOptions {
        min_shared: query.min_shared,
        min_degree: query.min_coupling,
        max_changeset: query.max_changeset,
        top_n: query.top_n,
        focus: query.focus.clone(),
    };
    let mut report = coupling::analyze(commits, Some(&universe), &query.since, &opts);
    report.excluded_test_files = excluded_test_files;
    CouplingRun {
        report,
        tests_included_for_focus: include_tests && !query.include_tests,
        history_is_empty: commits.is_empty(),
    }
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
pub(crate) mod tests {
    use super::*;
    use std::fs::{self, OpenOptions};
    use std::io::Write;
    use std::process::Command;
    use tempfile::TempDir;

    fn universe(paths: &[&str]) -> BTreeSet<PathBuf> {
        paths.iter().map(PathBuf::from).collect()
    }

    fn git(repo: &Path, args: &[&str]) {
        let output = Command::new("git")
            .arg("-C")
            .arg(repo)
            .args(args)
            .env("GIT_AUTHOR_DATE", "2020-01-01T00:00:00Z")
            .env("GIT_COMMITTER_DATE", "2020-01-01T00:00:00Z")
            .output()
            .expect("git should run");
        assert!(
            output.status.success(),
            "git {args:?} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    fn append(path: &Path, text: &str) {
        let mut file = OpenOptions::new()
            .append(true)
            .open(path)
            .expect("fixture file should open");
        file.write_all(text.as_bytes())
            .expect("fixture file should update");
    }

    fn commit_all(repo: &Path, message: &str) {
        git(repo, &["add", "-A"]);
        git(
            repo,
            &[
                "-c",
                "user.name=Codelens Test",
                "-c",
                "user.email=codelens@example.invalid",
                "-c",
                "commit.gpgsign=false",
                "commit",
                "-q",
                "-m",
                message,
            ],
        );
    }

    pub(crate) fn coupling_fixture() -> TempDir {
        let repo = tempfile::tempdir().expect("temporary repo should be created");
        git(repo.path(), &["init", "-q"]);
        fs::create_dir_all(repo.path().join("src")).unwrap();
        fs::create_dir_all(repo.path().join("outside")).unwrap();

        for path in [
            "src/a.rs",
            "src/b.rs",
            "src/c.rs",
            "src/a_test.rs",
            "src/filtered.rs",
            "src/deleted.rs",
            "outside/out.rs",
        ] {
            fs::write(repo.path().join(path), "pub fn value() -> usize { 1 }\n").unwrap();
        }
        commit_all(repo.path(), "initial files");

        fs::remove_file(repo.path().join("src/deleted.rs")).unwrap();
        commit_all(repo.path(), "delete stale file");

        for index in 0..4 {
            for path in [
                "src/a.rs",
                "src/b.rs",
                "src/a_test.rs",
                "src/filtered.rs",
                "outside/out.rs",
            ] {
                append(
                    &repo.path().join(path),
                    &format!("pub fn value_{index}() -> usize {{ {index} }}\n"),
                );
            }
            commit_all(repo.path(), &format!("coupled change {index}"));
        }
        repo
    }

    pub(crate) fn fixture_config() -> Config {
        let mut config = Config::default();
        config.walker.threads = 1;
        config.filter.exclude_files = vec![r"filtered[.]rs$".to_string()];
        config
    }

    fn fixture_query() -> CouplingQuery {
        CouplingQuery {
            since: "2000-01-01".to_string(),
            ..CouplingQuery::default()
        }
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

    #[test]
    fn shared_analysis_respects_subdirectory_config_current_tree_and_test_default() {
        let repo = coupling_fixture();
        let mut query = CouplingQuery {
            min_shared: 1,
            min_coupling: 0.0,
            top_n: 100,
            ..fixture_query()
        };
        let run = analyze_coupling(&[repo.path().join("src")], &fixture_config(), &query).unwrap();

        assert_eq!(run.report.excluded_test_files, 1);
        assert!(!run.tests_included_for_focus);
        assert!(!run.history_is_empty);
        assert!(!run.report.pairs.is_empty());
        assert!(run.report.pairs.iter().all(|pair| {
            [&pair.file_a, &pair.file_b]
                .into_iter()
                .all(|path| matches!(path.to_str(), Some("src/a.rs" | "src/b.rs" | "src/c.rs")))
        }));

        query.since = "2021-01-01".to_string();
        let empty =
            analyze_coupling(&[repo.path().join("src")], &fixture_config(), &query).unwrap();
        assert!(empty.history_is_empty);
        assert_eq!(empty.report.total_commits, 0);
        assert!(empty.report.pairs.is_empty());
    }

    #[test]
    fn coupling_rejects_paths_from_different_repositories() {
        let first = coupling_fixture();
        let second = coupling_fixture();
        let paths = vec![first.path().join("src"), second.path().join("src")];

        let error = analyze_coupling(&paths, &fixture_config(), &fixture_query())
            .err()
            .expect("cross-repository analysis must fail");

        assert!(error.to_string().contains("same Git repository"));
    }

    #[test]
    fn shared_analysis_applies_test_focus_thresholds_bulk_limit_and_top() {
        let repo = coupling_fixture();
        let path = repo.path().join("src");
        let config = fixture_config();

        let included = analyze_coupling(
            std::slice::from_ref(&path),
            &config,
            &CouplingQuery {
                include_tests: true,
                ..fixture_query()
            },
        )
        .unwrap();
        assert_eq!(included.report.excluded_test_files, 0);
        assert!(included.report.pairs.iter().any(|pair| {
            pair.file_a == Path::new("src/a_test.rs") || pair.file_b == Path::new("src/a_test.rs")
        }));

        let focused = analyze_coupling(
            std::slice::from_ref(&path),
            &config,
            &CouplingQuery {
                focus: Some(PathBuf::from("src/a_test.rs")),
                ..fixture_query()
            },
        )
        .unwrap();
        assert!(focused.tests_included_for_focus);
        assert_eq!(focused.report.excluded_test_files, 0);
        assert_eq!(focused.report.focus, Some(PathBuf::from("src/a_test.rs")));
        assert!(!focused.report.pairs.is_empty());

        for query in [
            CouplingQuery {
                min_shared: 6,
                ..fixture_query()
            },
            CouplingQuery {
                min_coupling: 101.0,
                ..fixture_query()
            },
        ] {
            let run = analyze_coupling(std::slice::from_ref(&path), &config, &query).unwrap();
            assert!(run.report.pairs.is_empty());
        }

        let bulk_filtered = analyze_coupling(
            std::slice::from_ref(&path),
            &config,
            &CouplingQuery {
                min_shared: 1,
                min_coupling: 0.0,
                max_changeset: 4,
                ..fixture_query()
            },
        )
        .unwrap();
        assert_eq!(bulk_filtered.report.skipped_large_commits, 5);
        assert!(bulk_filtered.report.pairs.is_empty());

        let top_one = analyze_coupling(
            &[path],
            &config,
            &CouplingQuery {
                min_shared: 1,
                min_coupling: 0.0,
                top_n: 1,
                ..fixture_query()
            },
        )
        .unwrap();
        assert_eq!(top_one.report.pairs.len(), 1);
    }
}
