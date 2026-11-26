//! MCP server: expose codelens analysis to AI agents over stdio.
//!
//! `codelens mcp` speaks the Model Context Protocol so coding agents
//! (Claude Code, Cursor, ...) can query repository statistics, health
//! scores, hotspots, and change coupling before editing code. Tools
//! return compact JSON strings mirroring the shape of the `-f json` CLI
//! output.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::SystemTime;

use anyhow::Context;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{ServerCapabilities, ServerConfig};
use rmcp::{schemars, tool, tool_handler, tool_router, ServerHandler};
use serde::Deserialize;

use codelens_core::filter::{Filter, FilterChain};
use codelens_core::insight::health;
use codelens_core::insight::scoring::HealthModelVersion;
use codelens_core::{analyze, Config};

/// Analysis defaults matching the CLI (smart excludes, gitignore, all cores).
fn default_config() -> Config {
    let mut config = Config::default();
    config.walker.threads = num_cpus::get();
    config.filter.smart_exclude = true;
    config
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct WorkspaceEntryState {
    path: PathBuf,
    kind: u8,
    len: u64,
    modified: Option<SystemTime>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct WorkspaceState {
    entries: Vec<WorkspaceEntryState>,
}

impl WorkspaceState {
    fn capture(root: &Path, config: &Config) -> std::io::Result<Self> {
        let filter = FilterChain::new(config).map_err(std::io::Error::other)?;
        let mut entries = Vec::new();
        let mut pending = vec![root.to_path_buf()];

        while let Some(path) = pending.pop() {
            let metadata = fs::symlink_metadata(&path)?;
            let file_type = metadata.file_type();
            entries.push(WorkspaceEntryState {
                path: path.clone(),
                kind: if file_type.is_dir() {
                    0
                } else if file_type.is_file() {
                    1
                } else {
                    2
                },
                len: metadata.len(),
                modified: metadata.modified().ok(),
            });

            if !file_type.is_dir() {
                continue;
            }

            let mut children = fs::read_dir(&path)?.collect::<std::io::Result<Vec<_>>>()?;
            children.sort_by_key(|entry| entry.path());
            for child in children.into_iter().rev() {
                let child_path = child.path();
                let child_type = child.file_type()?;
                if child_type.is_dir() && !filter.should_include(&child_path, true) {
                    continue;
                }
                pending.push(child_path);
            }
        }

        entries.sort_by(|left, right| left.path.cmp(&right.path));
        Ok(Self { entries })
    }
}

struct CachedAnalysis {
    state: WorkspaceState,
    result: Arc<codelens_core::AnalysisResult>,
}

#[derive(Default)]
struct AnalysisCache {
    entries: HashMap<PathBuf, CachedAnalysis>,
}

impl AnalysisCache {
    const MAX_ENTRIES: usize = 8;

    fn analysis(
        &mut self,
        path: &Path,
        config: &Config,
    ) -> anyhow::Result<Arc<codelens_core::AnalysisResult>> {
        let key = fs::canonicalize(path)
            .with_context(|| format!("analysis path does not exist: {}", path.display()))?;
        let before = WorkspaceState::capture(&key, config).ok();

        if let (Some(state), Some(cached)) = (before.as_ref(), self.entries.get(&key)) {
            if &cached.state == state {
                return Ok(Arc::clone(&cached.result));
            }
        }
        self.entries.remove(&key);

        let result = Arc::new(analyze(std::slice::from_ref(&key), config)?);
        let after = WorkspaceState::capture(&key, config).ok();
        if let (Some(before), Some(after)) = (before, after) {
            if before == after {
                if self.entries.len() >= Self::MAX_ENTRIES {
                    if let Some(oldest) = self.entries.keys().next().cloned() {
                        self.entries.remove(&oldest);
                    }
                }
                self.entries.insert(
                    key,
                    CachedAnalysis {
                        state: after,
                        result: Arc::clone(&result),
                    },
                );
            }
        }
        Ok(result)
    }
}

/// Run blocking analysis work off the async runtime; errors come back as
/// a JSON error object so agents always get parseable output.
async fn run_blocking<F>(f: F) -> String
where
    F: FnOnce() -> anyhow::Result<String> + Send + 'static,
{
    let outcome = tokio::task::spawn_blocking(f).await;
    match outcome {
        Ok(Ok(json)) => json,
        Ok(Err(e)) => serde_json::json!({ "error": e.to_string() }).to_string(),
        Err(e) => serde_json::json!({ "error": format!("task panicked: {e}") }).to_string(),
    }
}

#[derive(Deserialize, schemars::JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct PathArgs {
    /// Directory to analyze; defaults to the current directory.
    #[serde(default)]
    pub path: Option<String>,
}

#[derive(Deserialize, schemars::JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct GitWindowArgs {
    /// Directory inside the git repository; defaults to the current directory.
    #[serde(default)]
    pub path: Option<String>,
    /// Reserved time window argument; kept for MCP input compatibility even
    /// though code_health currently ignores it.
    #[allow(dead_code)]
    #[serde(default)]
    pub since: Option<String>,
    /// Maximum entries to return. Overrides the server configuration; defaults
    /// to the configured top value, then 10.
    #[serde(default)]
    pub top: Option<usize>,
}

#[derive(Deserialize, schemars::JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct HotspotArgs {
    /// Directory inside the git repository; defaults to the current directory.
    /// Only files under this path that survive the configured filters are
    /// eligible for hotspot results.
    #[serde(default)]
    pub path: Option<String>,
    /// Time window like "30d", "6m", "1y", or YYYY-MM-DD; defaults to "90d".
    #[serde(default)]
    pub since: Option<String>,
    /// Maximum entries to return. Overrides the server configuration; defaults
    /// to the configured top value, then 20.
    #[serde(default)]
    pub top: Option<usize>,
    /// Break top files down to function-level hotspots using batched Git
    /// history reads; defaults to false.
    #[serde(default)]
    pub functions: bool,
}

#[derive(Deserialize, schemars::JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct CouplingArgs {
    /// Directory to analyze; only files under this path that survive the
    /// configured filters are eligible for coupling results.
    #[serde(default)]
    pub path: Option<String>,
    /// Time window like "30d", "6m", "1y", or YYYY-MM-DD; defaults to "90d".
    #[serde(default)]
    pub since: Option<String>,
    /// Maximum pairs to return. Overrides the server configuration; defaults
    /// to the configured top value, then 20.
    #[serde(default)]
    pub top: Option<usize>,
    /// Only return pairs involving this repo-relative file. A test file
    /// automatically enables test files.
    #[serde(default)]
    pub focus: Option<String>,
    /// Minimum number of shared commits; defaults to 5.
    #[serde(default)]
    pub min_shared: Option<usize>,
    /// Minimum coupling percentage; defaults to 30.
    #[serde(default)]
    pub min_coupling: Option<f64>,
    /// Ignore commits touching more files than this; defaults to 30.
    #[serde(default)]
    pub max_changeset: Option<usize>,
    /// Include test files. Tests are excluded by default unless focus points
    /// to a test file.
    #[serde(default)]
    pub include_tests: Option<bool>,
}

#[derive(Deserialize, schemars::JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct FileArgs {
    /// Path of one source file to inspect.
    pub file: String,
}

#[derive(Clone)]
pub struct CodelensServer {
    config: Config,
    cache: Arc<Mutex<AnalysisCache>>,
}

impl Default for CodelensServer {
    fn default() -> Self {
        Self::new()
    }
}

fn arg_path(path: Option<String>) -> PathBuf {
    PathBuf::from(path.unwrap_or_else(|| ".".to_string()))
}

fn health_report(
    result: &codelens_core::AnalysisResult,
    top_n: usize,
    version: HealthModelVersion,
) -> codelens_core::insight::health::HealthReport {
    let model = crate::commands::scoring_model_for(version, result.summary.dup_scanned);
    health::score(result, model.as_ref(), top_n)
}

fn health_top_n(args: &GitWindowArgs, config: &Config) -> usize {
    args.top.or(config.output.top_n).unwrap_or(10)
}

fn coupling_query_from_mcp(
    args: &CouplingArgs,
    config: &Config,
) -> crate::commands::coupling::CouplingQuery {
    let mut query = crate::commands::coupling::CouplingQuery::default();
    query.top_n = config.output.top_n.unwrap_or(query.top_n);

    if let Some(since) = &args.since {
        query.since.clone_from(since);
    }
    if let Some(top) = args.top {
        query.top_n = top;
    }
    if let Some(focus) = &args.focus {
        query.focus = Some(PathBuf::from(focus));
    }
    if let Some(min_shared) = args.min_shared {
        query.min_shared = min_shared;
    }
    if let Some(min_coupling) = args.min_coupling {
        query.min_coupling = min_coupling;
    }
    if let Some(max_changeset) = args.max_changeset {
        query.max_changeset = max_changeset;
    }
    if let Some(include_tests) = args.include_tests {
        query.include_tests = include_tests;
    }
    query
}

fn hotspot_query_from_mcp(
    args: &HotspotArgs,
    config: &Config,
) -> crate::commands::hotspot::HotspotQuery {
    let mut query = crate::commands::hotspot::HotspotQuery::default();
    query.top_n = config.output.top_n.unwrap_or(query.top_n);

    if let Some(since) = &args.since {
        query.since.clone_from(since);
    }
    if let Some(top) = args.top {
        query.top_n = top;
    }
    query.functions = args.functions;
    query
}

#[tool_router]
impl CodelensServer {
    pub fn new() -> Self {
        Self {
            config: default_config(),
            cache: Arc::new(Mutex::new(AnalysisCache::default())),
        }
    }

    pub fn with_config(config: Config) -> Self {
        Self {
            config,
            cache: Arc::new(Mutex::new(AnalysisCache::default())),
        }
    }

    #[tool(
        description = "Repository overview: files, lines, languages, complexity, test ratio, and estimated LLM token count. Use before working in an unfamiliar repository."
    )]
    async fn repo_overview(&self, Parameters(args): Parameters<PathArgs>) -> String {
        let config = self.config.clone();
        let cache = Arc::clone(&self.cache);
        run_blocking(move || {
            let path = arg_path(args.path);
            let result = cache
                .lock()
                .map_err(|_| anyhow::anyhow!("analysis cache lock was poisoned"))?
                .analysis(&path, &config)?;
            Ok(serde_json::to_string(&result.summary)?)
        })
        .await
    }

    #[tool(
        description = "Code health report (A-F grades): project score, per-dimension scores, worst directories and files. Use to find what most needs refactoring, or to check health before/after edits."
    )]
    async fn code_health(&self, Parameters(args): Parameters<GitWindowArgs>) -> String {
        let config = self.config.clone();
        let cache = Arc::clone(&self.cache);
        run_blocking(move || {
            let top_n = health_top_n(&args, &config);
            let GitWindowArgs {
                path,
                since: _,
                top: _,
            } = args;
            let path = arg_path(path);
            let result = cache
                .lock()
                .map_err(|_| anyhow::anyhow!("analysis cache lock was poisoned"))?
                .analysis(&path, &config)?;
            let report = health_report(&result, top_n, config.health_model);
            Ok(serde_json::to_string(&report)?)
        })
        .await
    }

    #[tool(
        description = "Change hotspots: files that are both complex and frequently changed (the likeliest bug sources), with code age and author concentration (knowledge islands: risky files effectively one person knows). Requires a git repository. The path limits results to the current filtered scan tree. Supports since, top, and functions; functions adds an approximate function-level breakdown using batched Git history reads. Use to gauge risk before touching a file."
    )]
    async fn hotspots(&self, Parameters(args): Parameters<HotspotArgs>) -> String {
        let config = self.config.clone();
        let cache = Arc::clone(&self.cache);
        run_blocking(move || {
            let path = arg_path(args.path.clone());
            let query = hotspot_query_from_mcp(&args, &config);
            let result = cache
                .lock()
                .map_err(|_| anyhow::anyhow!("analysis cache lock was poisoned"))?
                .analysis(&path, &config)?;
            let git_client =
                codelens_core::GitClient::detect(&path).context("Not a git repository")?;
            let since = codelens_core::git::parse_since(&query.since);
            let commits = git_client.commit_log(&since)?;
            let first_commits = git_client
                .first_commit_times()
                .context("Failed to read file age history")?;
            let now_ts = SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .context("System clock is before the Unix epoch")?
                .as_secs() as i64;
            let run = crate::commands::hotspot::analyze_hotspots_from_history(
                &result,
                &git_client,
                &commits,
                &query,
                Some(&first_commits),
                Some(now_ts),
                &config,
            )?;
            Ok(serde_json::to_string(&run.report)?)
        })
        .await
    }

    #[tool(
        description = "Change coupling: file pairs that keep changing in the same commits (hidden dependencies). Requires a git repository. The path limits eligible files to the current filtered scan tree; deleted and out-of-scope files are excluded. Tests are excluded by default, include_tests restores them, and focusing a test file includes tests automatically. Supports since, top, focus, min_shared, min_coupling, and max_changeset. Use to learn what else usually changes when a given area changes."
    )]
    async fn change_coupling(&self, Parameters(args): Parameters<CouplingArgs>) -> String {
        let config = self.config.clone();
        let cache = Arc::clone(&self.cache);
        run_blocking(move || {
            let path = arg_path(args.path.clone());
            let query = coupling_query_from_mcp(&args, &config);
            let result = cache
                .lock()
                .map_err(|_| anyhow::anyhow!("analysis cache lock was poisoned"))?
                .analysis(&path, &config)?;
            let git_client =
                codelens_core::GitClient::detect(&path).context("Not a git repository")?;
            let since = codelens_core::git::parse_since(&query.since);
            let commits = git_client.commit_log(&since)?;
            let run = crate::commands::coupling::analyze_coupling_from_history(
                &result,
                &git_client,
                &commits,
                &query,
            );
            Ok(serde_json::to_string(&run.report)?)
        })
        .await
    }

    #[tool(
        description = "Metrics for one source file: lines, size, cyclomatic/cognitive complexity, nesting depth, and its health score with the weakest dimension."
    )]
    async fn file_metrics(&self, Parameters(args): Parameters<FileArgs>) -> String {
        let config = self.config.clone();
        let cache = Arc::clone(&self.cache);
        run_blocking(move || {
            let path = PathBuf::from(&args.file);
            let result = cache
                .lock()
                .map_err(|_| anyhow::anyhow!("analysis cache lock was poisoned"))?
                .analysis(&path, &config)?;
            let Some(stats) = result.files.first() else {
                anyhow::bail!("file not recognized as source code: {}", args.file);
            };
            let report = health_report(&result, 1, config.health_model);
            Ok(serde_json::json!({
                "stats": stats,
                "health": report.worst_files.first(),
            })
            .to_string())
        })
        .await
    }
}

#[tool_handler]
impl ServerHandler for CodelensServer {
    fn get_info(&self) -> ServerConfig {
        let mut info = ServerConfig::default();
        info.capabilities = ServerCapabilities::builder().enable_tools().build();
        info.instructions = Some(
            "codelens analyzes a code repository: statistics (repo_overview), \
             health grades (code_health), git change hotspots (hotspots), \
             change coupling (change_coupling), and per-file metrics \
             (file_metrics). Paths are directories unless noted; analysis \
             respects .gitignore and skips vendored/generated code."
                .to_string(),
        );
        info
    }
}

/// Run the MCP server over stdio until the client disconnects.
pub fn run(config: Config) -> anyhow::Result<()> {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    runtime.block_on(async {
        use rmcp::ServiceExt;
        let service = CodelensServer::with_config(config)
            .serve(rmcp::transport::stdio())
            .await?;
        service.waiting().await?;
        Ok(())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;
    use codelens_core::{AnalysisResult, Summary};
    use std::time::Duration;

    fn empty_result(dup_scanned: bool) -> AnalysisResult {
        let summary = Summary {
            dup_scanned,
            ..Summary::default()
        };
        AnalysisResult {
            files: Vec::new(),
            summary,
            elapsed: Duration::ZERO,
            scanned_files: 0,
            skipped_files: 0,
            error_files: 0,
        }
    }

    #[test]
    fn health_report_uses_actual_duplication_scope() {
        assert_eq!(
            health_report(&empty_result(true), 10, HealthModelVersion::V2).model,
            "default-v2"
        );
        assert_eq!(
            health_report(&empty_result(false), 10, HealthModelVersion::V2).model,
            "default-v2-no-dup"
        );
        assert_eq!(
            health_report(&empty_result(true), 10, HealthModelVersion::V1).model,
            "default"
        );
    }

    fn empty_coupling_args() -> CouplingArgs {
        CouplingArgs {
            path: None,
            since: None,
            top: None,
            focus: None,
            min_shared: None,
            min_coupling: None,
            max_changeset: None,
            include_tests: None,
        }
    }

    fn empty_hotspot_args() -> HotspotArgs {
        HotspotArgs {
            path: None,
            since: None,
            top: None,
            functions: false,
        }
    }

    fn empty_health_args() -> GitWindowArgs {
        GitWindowArgs {
            path: None,
            since: None,
            top: None,
        }
    }

    #[test]
    fn health_arguments_override_server_config_then_defaults() {
        let mut config = Config::default();
        config.output.top_n = Some(7);
        assert_eq!(health_top_n(&empty_health_args(), &config), 7);

        let args = GitWindowArgs {
            top: Some(3),
            ..empty_health_args()
        };
        assert_eq!(health_top_n(&args, &config), 3);

        config.output.top_n = None;
        assert_eq!(health_top_n(&empty_health_args(), &config), 10);
    }

    #[test]
    fn hotspot_arguments_override_server_config_then_defaults() {
        let mut config = Config::default();
        config.output.top_n = Some(7);
        let configured = hotspot_query_from_mcp(&empty_hotspot_args(), &config);
        assert_eq!(configured.top_n, 7);
        assert_eq!(configured.since, "90d");
        assert!(!configured.functions);

        let args = HotspotArgs {
            since: Some("30d".to_string()),
            top: Some(3),
            functions: true,
            ..empty_hotspot_args()
        };
        let overridden = hotspot_query_from_mcp(&args, &config);
        assert_eq!(overridden.top_n, 3);
        assert_eq!(overridden.since, "30d");
        assert!(overridden.functions);

        config.output.top_n = None;
        assert_eq!(
            hotspot_query_from_mcp(&empty_hotspot_args(), &config).top_n,
            crate::commands::hotspot::DEFAULT_HOTSPOT_TOP
        );
    }

    #[test]
    fn coupling_arguments_override_server_config_then_defaults() {
        let mut config = Config::default();
        config.output.top_n = Some(7);
        assert_eq!(
            coupling_query_from_mcp(&empty_coupling_args(), &config).top_n,
            7
        );

        let args = CouplingArgs {
            top: Some(3),
            min_shared: Some(2),
            min_coupling: Some(40.0),
            max_changeset: Some(12),
            include_tests: Some(true),
            ..empty_coupling_args()
        };
        let query = coupling_query_from_mcp(&args, &config);
        assert_eq!(query.top_n, 3);
        assert_eq!(query.min_shared, 2);
        assert_eq!(query.min_coupling, 40.0);
        assert_eq!(query.max_changeset, 12);
        assert!(query.include_tests);

        config.output.top_n = None;
        assert_eq!(
            coupling_query_from_mcp(&empty_coupling_args(), &config).top_n,
            20
        );
    }

    #[test]
    fn cli_and_mcp_coupling_return_the_same_report() {
        let repo = crate::commands::coupling::tests::coupling_fixture();
        let path = repo.path().join("src");
        let focus = "src/a_test.rs";

        let cli = crate::cli::Cli::try_parse_from([
            "codelens",
            "coupling",
            path.to_str().unwrap(),
            "--since",
            "2000-01-01",
            "--for",
            focus,
            "--min-shared",
            "1",
            "--min-coupling",
            "0",
            "--max-changeset",
            "30",
            "--top",
            "1",
        ])
        .unwrap();
        let crate::cli::Command::Coupling(cli_args) = cli.command.unwrap() else {
            panic!("expected coupling command");
        };
        let mut cli_config = crate::commands::coupling::tests::fixture_config();
        cli_config.output.top_n = Some(1);
        let cli_query = crate::commands::coupling::query_from_cli(&cli_args, &cli_config);
        let cli_report =
            crate::commands::coupling::analyze_coupling(&cli_args.paths, &cli_config, &cli_query)
                .unwrap()
                .report;

        let mut mcp_config = crate::commands::coupling::tests::fixture_config();
        mcp_config.output.top_n = Some(9);
        let mcp_args = CouplingArgs {
            path: Some(path.to_string_lossy().into_owned()),
            since: Some("2000-01-01".to_string()),
            top: Some(1),
            focus: Some(focus.to_string()),
            min_shared: Some(1),
            min_coupling: Some(0.0),
            max_changeset: Some(30),
            include_tests: Some(false),
        };
        let mcp_query = coupling_query_from_mcp(&mcp_args, &mcp_config);
        let mcp_report = crate::commands::coupling::analyze_coupling(
            &[arg_path(mcp_args.path)],
            &mcp_config,
            &mcp_query,
        )
        .unwrap()
        .report;

        assert_eq!(
            serde_json::to_value(cli_report).unwrap(),
            serde_json::to_value(mcp_report).unwrap()
        );
    }

    #[test]
    fn health_report_scores_empty_analysis_as_f() {
        let report = health_report(&empty_result(false), 10, HealthModelVersion::V2);
        assert_eq!(report.score, 0.0);
        assert_eq!(report.grade.to_string(), "F");
        assert_eq!(report.assessment.to_string(), "insufficient-data");
        assert!(report.dimensions.is_empty());
    }

    #[test]
    fn analysis_cache_reuses_stable_trees_and_invalidates_workspace_changes() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("src/lib.rs");
        std::fs::create_dir_all(source.parent().unwrap()).unwrap();
        std::fs::write(&source, "pub fn first() {}\n").unwrap();
        let empty = dir.path().join("empty");
        std::fs::create_dir(&empty).unwrap();

        let config = default_config();
        let mut cache = AnalysisCache::default();
        let first = cache.analysis(dir.path(), &config).unwrap();
        let reused = cache.analysis(dir.path(), &config).unwrap();
        assert!(std::sync::Arc::ptr_eq(&first, &reused));

        std::fs::write(&source, "pub fn first() {}\npub fn second() {}\n").unwrap();
        let changed = cache.analysis(dir.path(), &config).unwrap();
        assert!(!std::sync::Arc::ptr_eq(&first, &changed));
        assert_eq!(changed.summary.complexity.functions, 2);

        std::fs::write(empty.join("new.rs"), "pub fn added() {}\n").unwrap();
        let added = cache.analysis(dir.path(), &config).unwrap();
        assert!(!std::sync::Arc::ptr_eq(&changed, &added));
        assert_eq!(added.summary.total_files, 2);
    }

    #[test]
    fn analysis_cache_reuses_repeated_file_scans() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("src/lib.rs");
        std::fs::create_dir_all(source.parent().unwrap()).unwrap();
        std::fs::write(&source, "pub fn cached() {}\n").unwrap();

        let config = default_config();
        let mut cache = AnalysisCache::default();
        let first = cache.analysis(&source, &config).unwrap();
        let reused = cache.analysis(&source, &config).unwrap();

        assert_eq!(cache.entries.len(), 1);
        assert!(std::sync::Arc::ptr_eq(&first, &reused));
        assert_eq!(reused.summary.total_files, 1);
    }
}
