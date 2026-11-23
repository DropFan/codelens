//! `codelens hotspot` — change hotspots from git churn x complexity, with
//! optional function-level breakdown.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{bail, Context, Result};
use colored::Colorize;

use codelens_core::analyze;
use codelens_core::git::{self, GitClient};
use codelens_core::insight::hotspot;
use codelens_core::output::Report;

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
    let total_commits = commits.len();
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
        attach_function_hotspots(&mut report, &git_client, &since, &config)?;
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
    config: &codelens_core::Config,
) -> Result<()> {
    use codelens_core::insight::hotspot::FunctionHotspot;

    // git log -p per file is the expensive part; cap the breakdown.
    const MAX_FILES: usize = 10;
    const MAX_FUNCTIONS: usize = 5;

    // The config-described registry, not a bare builtin one: custom
    // languages (--languages-file) and remapped extensions (--count-as)
    // must get the same breakdown as builtin ones.
    let registry = codelens_core::build_registry(config)?;
    let analyzer = codelens_core::ComplexityAnalyzer::new();
    let repo_root = git_client.repo_path().to_path_buf();

    let mut candidates = Vec::new();
    for (index, file) in report.files.iter().take(MAX_FILES).enumerate() {
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
        candidates.push((index, file.path.clone(), content, spans, lang));
    }

    let paths: Vec<PathBuf> = candidates
        .iter()
        .map(|(_, path, _, _, _)| path.clone())
        .collect();
    // Function enrichment is best-effort: preserve the file-level report if
    // Git history cannot be read, matching the previous per-file behavior.
    let hunks_by_path = batch_file_commit_hunks(&repo_root, &paths, since).unwrap_or_default();

    for (index, path, content, spans, lang) in candidates {
        let Some(hunks) = hunks_by_path.get(&path) else {
            continue;
        };
        let touches = hotspot::count_function_touches(&spans, hunks);

        let lines: Vec<&str> = content.lines().collect();
        let mut functions: Vec<FunctionHotspot> = spans
            .iter()
            .zip(touches)
            .filter(|(_, touched)| *touched > 0)
            .map(|(span, touched)| {
                let end = span.end_line.min(lines.len());
                let body = lines[span.start_line - 1..end].join("\n");
                let cyclomatic = analyzer.analyze(&body, &lang).cyclomatic;
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
            report.files[index].functions = Some(functions);
        }
    }
    Ok(())
}

#[derive(Debug)]
enum RawChange {
    Path(PathBuf),
    Rename { old: PathBuf, new: PathBuf },
}

type CommitHunks = Vec<Vec<(usize, usize)>>;

/// Read all requested files' patch history in one Git process. A lightweight
/// rename-only query first discovers historical aliases so pre-rename hunks
/// are included without widening the expensive patch query to the whole repo.
fn batch_file_commit_hunks(
    repo_root: &Path,
    paths: &[PathBuf],
    since: &str,
) -> Result<HashMap<PathBuf, CommitHunks>> {
    if paths.is_empty() {
        return Ok(HashMap::new());
    }

    let selected: HashSet<PathBuf> = paths.iter().cloned().collect();
    let mut rename_command = Command::new("git");
    rename_command.args([
        "--literal-pathspecs",
        "-c",
        "core.quotepath=false",
        "log",
        "--raw",
        "-z",
        "-M",
        "--diff-filter=R",
        "--format=%x01%x00",
    ]);
    rename_command.arg(format!("--since={since}"));
    let rename_output = run_git_output(repo_root, rename_command)?;
    let mut aliases = rename_aliases(&rename_output, &selected);
    aliases.extend(selected.iter().cloned());

    let mut aliases: Vec<PathBuf> = aliases.into_iter().collect();
    aliases.sort();

    let mut patch_command = Command::new("git");
    patch_command.args([
        "--literal-pathspecs",
        "-c",
        "core.quotepath=false",
        "log",
        "--raw",
        "-z",
        "-M",
        "-p",
        "-U0",
        "--format=%x01%H%x00",
    ]);
    patch_command.arg(format!("--since={since}"));
    patch_command.arg("--");
    patch_command.args(&aliases);
    let patch_output = run_git_output(repo_root, patch_command)?;

    Ok(parse_batch_patch_hunks(&patch_output, &selected))
}

fn run_git_output(repo_root: &Path, mut command: Command) -> Result<Vec<u8>> {
    let output = command
        .current_dir(repo_root)
        .output()
        .context("failed to execute git log")?;
    if output.status.success() {
        return Ok(output.stdout);
    }

    let has_head = Command::new("git")
        .args(["rev-parse", "--verify", "HEAD"])
        .current_dir(repo_root)
        .output()
        .is_ok_and(|head| head.status.success());
    if !has_head {
        return Ok(Vec::new());
    }

    bail!(
        "git log failed: {}",
        String::from_utf8_lossy(&output.stderr).trim()
    )
}

fn rename_aliases(output: &[u8], selected: &HashSet<PathBuf>) -> HashSet<PathBuf> {
    let mut aliases = HashSet::new();
    let mut final_name: HashMap<PathBuf, PathBuf> = HashMap::new();

    for chunk in output.split(|byte| *byte == 0x01).skip(1) {
        for change in parse_raw_changes(chunk) {
            let RawChange::Rename { old, new } = change else {
                continue;
            };
            let target = final_name.get(&new).cloned().unwrap_or(new);
            final_name.insert(old.clone(), target.clone());
            if selected.contains(&target) {
                aliases.insert(old);
            }
        }
    }

    aliases
}

fn parse_batch_patch_hunks(
    output: &[u8],
    selected: &HashSet<PathBuf>,
) -> HashMap<PathBuf, CommitHunks> {
    let mut all_hunks: HashMap<PathBuf, CommitHunks> = HashMap::new();
    let mut final_name: HashMap<PathBuf, PathBuf> = HashMap::new();

    for chunk in output.split(|byte| *byte == 0x01).skip(1) {
        let Some(separator) = find_bytes(chunk, b"\0\0diff --git ") else {
            continue;
        };
        let patch_start = separator + 2;
        let changes = parse_raw_changes(&chunk[..patch_start]);
        let mut resolved = Vec::with_capacity(changes.len());
        for change in changes {
            let path = match change {
                RawChange::Rename { old, new } => {
                    let target = final_name.get(&new).cloned().unwrap_or(new);
                    final_name.insert(old, target.clone());
                    target
                }
                RawChange::Path(raw) => final_name.get(&raw).cloned().unwrap_or(raw),
            };
            resolved.push(path);
        }

        let patch = String::from_utf8_lossy(&chunk[patch_start..]);
        let mut diff_index = 0;
        let mut current_path: Option<&PathBuf> = None;
        let mut commit_hunks: HashMap<PathBuf, Vec<(usize, usize)>> = HashMap::new();
        for line in patch.lines() {
            if line.starts_with("diff --git ") {
                current_path = resolved.get(diff_index);
                diff_index += 1;
                continue;
            }
            let (Some(path), Some(range)) = (current_path, parse_hunk_range(line)) else {
                continue;
            };
            if selected.contains(path) {
                commit_hunks.entry(path.clone()).or_default().push(range);
            }
        }

        for (path, hunks) in commit_hunks {
            if !hunks.is_empty() {
                all_hunks.entry(path).or_default().push(hunks);
            }
        }
    }

    all_hunks
}

fn parse_raw_changes(output: &[u8]) -> Vec<RawChange> {
    let fields: Vec<&[u8]> = output.split(|byte| *byte == 0).collect();
    let mut changes = Vec::new();
    let mut index = 0;

    while index < fields.len() {
        let metadata = fields[index].strip_prefix(b"\n").unwrap_or(fields[index]);
        if !metadata.starts_with(b":") {
            index += 1;
            continue;
        }
        let status = metadata
            .split(|byte| byte.is_ascii_whitespace())
            .next_back()
            .unwrap_or_default();
        if status.starts_with(b"R") {
            let (Some(old), Some(new)) = (fields.get(index + 1), fields.get(index + 2)) else {
                break;
            };
            changes.push(RawChange::Rename {
                old: raw_path(old),
                new: raw_path(new),
            });
            index += 3;
        } else {
            let Some(path) = fields.get(index + 1) else {
                break;
            };
            changes.push(RawChange::Path(raw_path(path)));
            index += 2;
        }
    }

    changes
}

fn parse_hunk_range(line: &str) -> Option<(usize, usize)> {
    let rest = line.strip_prefix("@@ ")?;
    let plus = rest.split(' ').find(|part| part.starts_with('+'))?;
    let mut nums = plus[1..].splitn(2, ',');
    let start = nums.next()?.parse::<usize>().ok()?;
    let count = nums.next().and_then(|n| n.parse().ok()).unwrap_or(1);
    Some((start.max(1), count.max(1)))
}

fn find_bytes(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

fn raw_path(bytes: &[u8]) -> PathBuf {
    PathBuf::from(String::from_utf8_lossy(bytes).into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    use codelens_core::insight::hotspot::{ChurnMetrics, FileHotspot, HotspotReport, RiskLevel};
    use codelens_core::{Complexity, Config};

    fn git(dir: &Path, args: &[&str]) {
        let output = Command::new("git")
            .args(args)
            .current_dir(dir)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "git {args:?} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    fn init_repo(dir: &Path) {
        git(dir, &["init"]);
        git(dir, &["config", "user.email", "test@test.com"]);
        git(dir, &["config", "user.name", "Test"]);
        git(dir, &["config", "commit.gpgsign", "false"]);
    }

    #[test]
    fn function_breakdown_honors_mapping_and_ignores_non_code_keywords() {
        let temp = tempfile::TempDir::new().unwrap();
        init_repo(temp.path());

        // Rust code behind an extension the builtin registry does not
        // know: only the config's count_as mapping makes it detectable.
        std::fs::write(
            temp.path().join("main.foo"),
            concat!(
                "fn alpha() {\n",
                "    let marker = \"if for while match && || one\";\n",
                "    // if for while match && ||\n",
                "    if true { println!(\"{marker}\"); }\n",
                "}\n",
            ),
        )
        .unwrap();
        git(temp.path(), &["add", "."]);
        git(temp.path(), &["commit", "-m", "init"]);

        // The second commit touches lines inside the function body so
        // the span registers at least one commit.
        std::fs::write(
            temp.path().join("main.foo"),
            concat!(
                "fn alpha() {\n",
                "    let marker = \"if for while match && || two\";\n",
                "    // if for while match && ||\n",
                "    if true { println!(\"{marker}\"); }\n",
                "}\n",
            ),
        )
        .unwrap();
        git(temp.path(), &["add", "."]);
        git(temp.path(), &["commit", "-m", "change"]);

        let client = GitClient::detect(temp.path()).unwrap();
        let mut report = HotspotReport {
            files: vec![FileHotspot {
                path: PathBuf::from("main.foo"),
                language: "Rust".to_string(),
                churn: ChurnMetrics {
                    commits: 2,
                    lines_added: 0,
                    lines_deleted: 0,
                    lines_churn: 0,
                },
                complexity: Complexity::default(),
                hotspot_score: 1.0,
                risk: RiskLevel::High,
                age_days: None,
                knowledge: None,
                functions: None,
            }],
            since: "90 days ago".to_string(),
            total_commits: 2,
        };
        let config = Config {
            count_as: vec![("foo".to_string(), "Rust".to_string())],
            ..Config::default()
        };

        attach_function_hotspots(&mut report, &client, "90 days ago", &config).unwrap();

        let functions = report.files[0]
            .functions
            .as_ref()
            .expect("custom-mapped language must get a function breakdown");
        let alpha = functions.iter().find(|f| f.name == "alpha").unwrap();
        assert_eq!(
            alpha.cyclomatic, 2,
            "only the function base and real if should count"
        );
    }

    #[test]
    fn batch_hunks_follow_rename_chain_without_merging_recreated_path() {
        let temp = tempfile::TempDir::new().unwrap();
        init_repo(temp.path());
        let old = "diff --git 旧 路径[一].rs";
        let middle = "中间{路径}.rs";
        let final_path = ":最终 路径.rs";

        std::fs::write(
            temp.path().join(old),
            "fn alpha() {\n    let value = 1;\n    println!(\"{value}\");\n}\n",
        )
        .unwrap();
        git(temp.path(), &["add", "--", old]);
        git(temp.path(), &["commit", "-m", "initial special path"]);

        git(temp.path(), &["mv", "--", old, middle]);
        git(temp.path(), &["commit", "-m", "first rename"]);
        git(
            temp.path(),
            &["--literal-pathspecs", "mv", "--", middle, final_path],
        );
        git(temp.path(), &["commit", "-m", "second rename"]);

        std::fs::write(
            temp.path().join(final_path),
            "fn alpha() {\n    let value = 2;\n    println!(\"{value}\");\n}\n",
        )
        .unwrap();
        git(
            temp.path(),
            &["--literal-pathspecs", "add", "--", final_path],
        );
        git(temp.path(), &["commit", "-m", "modify final path"]);

        std::fs::write(temp.path().join(old), "fn separate() {}\n").unwrap();
        git(temp.path(), &["add", "--", old]);
        git(temp.path(), &["commit", "-m", "recreate old path"]);

        std::fs::write(
            temp.path().join(final_path),
            "fn alpha() {\n    let value = 2;\n}\n",
        )
        .unwrap();
        git(
            temp.path(),
            &["--literal-pathspecs", "add", "--", final_path],
        );
        git(temp.path(), &["commit", "-m", "delete final line"]);
        git(
            temp.path(),
            &["commit", "--allow-empty", "-m", "empty commit"],
        );

        let paths = vec![PathBuf::from(final_path), PathBuf::from(old)];
        let hunks = batch_file_commit_hunks(temp.path(), &paths, "90 days ago").unwrap();
        let ages = GitClient::detect(temp.path())
            .unwrap()
            .first_commit_times()
            .unwrap();

        assert_eq!(
            hunks[Path::new(final_path)].len(),
            3,
            "initial, modification, and deletion commits must follow the rename chain"
        );
        assert_eq!(
            hunks[Path::new(old)].len(),
            1,
            "the recreated old path must keep only its new history"
        );
        assert!(
            hunks[Path::new(final_path)]
                .iter()
                .flatten()
                .any(|range| *range == (2, 1)),
            "a deletion-only hunk must anchor to the nearest surviving line"
        );
        assert!(ages.contains_key(Path::new(final_path)));
        assert!(ages.contains_key(Path::new(old)));
        assert!(!ages.contains_key(Path::new(middle)));
    }

    #[test]
    fn batch_hunks_handle_empty_history_and_empty_selection() {
        let temp = tempfile::TempDir::new().unwrap();
        init_repo(temp.path());

        assert!(batch_file_commit_hunks(temp.path(), &[], "90 days ago")
            .unwrap()
            .is_empty());
        assert!(batch_file_commit_hunks(
            temp.path(),
            &[PathBuf::from("missing.rs")],
            "90 days ago"
        )
        .unwrap()
        .is_empty());
    }
}
