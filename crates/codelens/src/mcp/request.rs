//! MCP request schemas and their command-query adapters.

use std::path::PathBuf;

use codelens_core::Config;
use rmcp::schemars;
use serde::Deserialize;

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

pub(super) fn arg_path(path: Option<String>) -> PathBuf {
    PathBuf::from(path.unwrap_or_else(|| ".".to_string()))
}

pub(super) fn health_top_n(args: &GitWindowArgs, config: &Config) -> usize {
    args.top.or(config.output.top_n).unwrap_or(10)
}

pub(super) fn coupling_query_from_mcp(
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

pub(super) fn hotspot_query_from_mcp(
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
