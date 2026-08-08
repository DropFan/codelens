//! `codelens hotspot` — change hotspots from git churn x complexity, with
//! optional function-level breakdown.

use anyhow::{Context, Result};
use colored::Colorize;

use codelens_core::git::{self, GitClient};
use codelens_core::insight::hotspot;
use codelens_core::output::Report;
use codelens_core::{analyze, LanguageRegistry};

use super::{load_partial_config, resolve_config, rewrite_paths_repo_relative, write_report};
use crate::cli;

pub(crate) fn run_hotspot(args: &cli::HotspotArgs, advanced: &cli::AdvancedArgs) -> Result<()> {
    let partial = load_partial_config(advanced)?;
    let config = resolve_config(&args.filter, &args.output, advanced, partial.as_ref());
    let git_client = GitClient::detect(&args.paths[0]).context("Not a git repository")?;
    let since = git::parse_since(&args.since);
    let mut result = analyze(&args.paths, &config).context("Analysis failed")?;

    // git numstat paths are repo-root-relative; analysis paths are relative
    // to the walk root (or absolute). Rewrite them so the two sides match
    // even when running from a subdirectory or with absolute paths.
    rewrite_paths_repo_relative(&mut result.files, git_client.repo_path());

    // One log walk yields both churn and author-ownership data.
    let commits = git_client.commit_log(&since)?;
    let churns = git::churn_from_commits(&commits);
    let authors = git::aggregate_authors(&commits);
    let total_commits = git_client.commit_count(&since)?;
    if total_commits == 0 {
        eprintln!(
            "{}: no commits found since '{}'; check the --since format (e.g. 30d, 4w, 6m, 1y, YYYY-MM-DD)",
            "warning".yellow().bold(),
            args.since
        );
    }
    let top_n = config.output.top_n.unwrap_or(20);
    let mut report = hotspot::analyze(&churns, &result, &args.since, total_commits, top_n);
    hotspot::attach_knowledge(&mut report, &authors);

    // Age enrichment walks the full history; a failure here (or a clock
    // before the epoch) only costs the Age column, never the report.
    if let Ok(first_commits) = git_client.first_commit_times() {
        let now_ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        if now_ts > 0 {
            hotspot::attach_ages(&mut report, &first_commits, now_ts);
        }
    }
    if args.functions {
        attach_function_hotspots(&mut report, &git_client, &since)?;
    }
    write_report(Report::Hotspot(report), &config.output)
}

/// Break the top hotspot files down to function level: intersect each
/// file's diff hunks with its (heuristic) function spans. Best-effort per
/// file — unreadable or unrecognized files just get no breakdown.
fn attach_function_hotspots(
    report: &mut codelens_core::insight::hotspot::HotspotReport,
    git_client: &GitClient,
    since: &str,
) -> Result<()> {
    use codelens_core::insight::hotspot::FunctionHotspot;

    // git log -p per file is the expensive part; cap the breakdown.
    const MAX_FILES: usize = 10;
    const MAX_FUNCTIONS: usize = 5;

    let registry = LanguageRegistry::with_builtin()?;
    let analyzer = codelens_core::ComplexityAnalyzer::new();
    let repo_root = git_client.repo_path().to_path_buf();

    for file in report.files.iter_mut().take(MAX_FILES) {
        let abs = repo_root.join(&file.path);
        let Ok(content) = std::fs::read_to_string(&abs) else {
            continue;
        };
        let Some(lang) = registry.detect(&abs) else {
            continue;
        };
        let spans = analyzer.function_spans(&content, &lang);
        if spans.is_empty() {
            continue;
        }
        let Ok(hunks) = git_client.file_commit_hunks(&file.path, since) else {
            continue;
        };
        let touches = hotspot::count_function_touches(&spans, &hunks);

        let lines: Vec<&str> = content.lines().collect();
        let keywords_re = &lang.complexity_patterns().keywords_re;
        let mut functions: Vec<FunctionHotspot> = spans
            .iter()
            .zip(touches)
            .filter(|(_, touched)| *touched > 0)
            .map(|(span, touched)| {
                let end = span.end_line.min(lines.len());
                let body = lines[span.start_line - 1..end].join("\n");
                let cyclomatic = keywords_re
                    .as_ref()
                    .map(|re| re.find_iter(&body).count())
                    .unwrap_or(0)
                    + 1;
                FunctionHotspot {
                    name: span.name.clone(),
                    start_line: span.start_line,
                    end_line: span.end_line,
                    commits: touched,
                    cyclomatic,
                }
            })
            .collect();
        functions.sort_by_key(|f| std::cmp::Reverse((f.commits, f.cyclomatic)));
        functions.truncate(MAX_FUNCTIONS);
        if !functions.is_empty() {
            file.functions = Some(functions);
        }
    }
    Ok(())
}
