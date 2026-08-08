//! MCP server: expose codelens analysis to AI agents over stdio.
//!
//! `codelens mcp` speaks the Model Context Protocol so coding agents
//! (Claude Code, Cursor, ...) can query repository statistics, health
//! scores, hotspots, and change coupling before editing code. Tools
//! return compact JSON strings mirroring the `-f json` CLI output.

use std::path::PathBuf;

use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{ServerCapabilities, ServerInfo};
use rmcp::{schemars, tool, tool_handler, tool_router, ServerHandler};
use serde::Deserialize;

use codelens_core::insight::scoring::default::DefaultModel;
use codelens_core::insight::{coupling, health, hotspot};
use codelens_core::{analyze, Config, GitClient};

/// Analysis defaults matching the CLI (smart excludes, gitignore, all cores).
fn default_config() -> Config {
    let mut config = Config::default();
    config.walker.threads = num_cpus::get();
    config.filter.smart_exclude = true;
    config
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
    /// Time window like "30d", "6m", "1y", or YYYY-MM-DD; defaults to "90d".
    #[serde(default)]
    pub since: Option<String>,
    /// Maximum entries to return; defaults to 10.
    #[serde(default)]
    pub top: Option<usize>,
}

#[derive(Deserialize, schemars::JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct FileArgs {
    /// Path of one source file to inspect.
    pub file: String,
}

#[derive(Clone, Default)]
pub struct CodelensServer;

fn arg_path(path: Option<String>) -> PathBuf {
    PathBuf::from(path.unwrap_or_else(|| ".".to_string()))
}

#[tool_router]
impl CodelensServer {
    pub fn new() -> Self {
        Self
    }

    #[tool(
        description = "Repository overview: files, lines, languages, complexity, test ratio, and estimated LLM token count. Use before working in an unfamiliar repository."
    )]
    async fn repo_overview(&self, Parameters(args): Parameters<PathArgs>) -> String {
        run_blocking(move || {
            let result = analyze(&[arg_path(args.path)], &default_config())?;
            Ok(serde_json::to_string(&result.summary)?)
        })
        .await
    }

    #[tool(
        description = "Code health report (A-F grades): project score, per-dimension scores, worst directories and files. Use to find what most needs refactoring, or to check health before/after edits."
    )]
    async fn code_health(&self, Parameters(args): Parameters<GitWindowArgs>) -> String {
        run_blocking(move || {
            let result = analyze(&[arg_path(args.path)], &default_config())?;
            let report = health::score(&result, &DefaultModel::new(), args.top.unwrap_or(10));
            Ok(serde_json::to_string(&report)?)
        })
        .await
    }

    #[tool(
        description = "Change hotspots: files that are both complex and frequently changed (the likeliest bug sources), with code age and author concentration (knowledge islands: risky files effectively one person knows). Requires a git repository. Use to gauge risk before touching a file."
    )]
    async fn hotspots(&self, Parameters(args): Parameters<GitWindowArgs>) -> String {
        run_blocking(move || {
            let path = arg_path(args.path);
            let since_arg = args.since.unwrap_or_else(|| "90d".to_string());
            let since = codelens_core::git::parse_since(&since_arg);
            let git_client = GitClient::detect(&path)?;
            let mut result = analyze(&[path], &default_config())?;
            // Git paths are repo-root-relative; align the analysis side so
            // churn/knowledge joins work when `path` is a subdirectory.
            crate::commands::rewrite_paths_repo_relative(&mut result.files, git_client.repo_path());
            let commits = git_client.commit_log(&since)?;
            let churns = codelens_core::git::churn_from_commits(&commits);
            let authors = codelens_core::git::aggregate_authors(&commits);
            let total = git_client.commit_count(&since)?;
            let mut report =
                hotspot::analyze(&churns, &result, &since_arg, total, args.top.unwrap_or(10));
            hotspot::attach_knowledge(&mut report, &authors);
            if let Ok(first_commits) = git_client.first_commit_times() {
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_secs() as i64)
                    .unwrap_or(0);
                if now > 0 {
                    hotspot::attach_ages(&mut report, &first_commits, now);
                }
            }
            Ok(serde_json::to_string(&report)?)
        })
        .await
    }

    #[tool(
        description = "Change coupling: file pairs that keep changing in the same commits (hidden dependencies). Requires a git repository. Use to learn what else usually changes when a given area changes."
    )]
    async fn change_coupling(&self, Parameters(args): Parameters<GitWindowArgs>) -> String {
        run_blocking(move || {
            let path = arg_path(args.path);
            let since_arg = args.since.unwrap_or_else(|| "90d".to_string());
            let since = codelens_core::git::parse_since(&since_arg);
            let git_client = GitClient::detect(&path)?;
            let commits = git_client.commit_log(&since)?;
            let opts = coupling::CouplingOptions {
                top_n: args.top.unwrap_or(10),
                ..coupling::CouplingOptions::default()
            };
            let report = coupling::analyze(&commits, None, &since_arg, &opts);
            Ok(serde_json::to_string(&report)?)
        })
        .await
    }

    #[tool(
        description = "Metrics for one source file: lines, size, cyclomatic/cognitive complexity, nesting depth, and its health score with the weakest dimension."
    )]
    async fn file_metrics(&self, Parameters(args): Parameters<FileArgs>) -> String {
        run_blocking(move || {
            let result = analyze(&[PathBuf::from(&args.file)], &default_config())?;
            let Some(stats) = result.files.first() else {
                anyhow::bail!("file not recognized as source code: {}", args.file);
            };
            let report = health::score(&result, &DefaultModel::new(), 1);
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
    fn get_info(&self) -> ServerInfo {
        let mut info = ServerInfo::default();
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
pub fn run() -> anyhow::Result<()> {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    runtime.block_on(async {
        use rmcp::ServiceExt;
        let service = CodelensServer::new()
            .serve(rmcp::transport::stdio())
            .await?;
        service.waiting().await?;
        Ok(())
    })
}
