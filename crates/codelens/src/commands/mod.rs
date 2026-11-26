//! Subcommand implementations and the shared CLI plumbing they build on:
//! config loading/merging, report writing, and git path normalization.

pub(crate) mod compare;
pub(crate) mod coupling;
pub(crate) mod diff;
pub(crate) mod estimate;
pub(crate) mod health;
pub(crate) mod hotspot;
pub(crate) mod trend;

use std::fs::File;
use std::io::{self, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use anyhow::{bail, Context, Result};
use colored::Colorize;

use codelens_core::config::{Config, PartialConfig};
use codelens_core::git::GitClient;
use codelens_core::insight::scoring::{HealthModelVersion, ScoringModel};
use codelens_core::output::{create_output, OutputOptions, Report};
use codelens_core::{analyze, LanguageRegistry};

use crate::cli::{self, Cli};

/// Run the default (no subcommand) analysis and print the combined report.
pub(crate) fn run_default(cli: &Cli) -> Result<ExitCode> {
    // Build configuration
    let config = build_config(cli)?;

    // Collect paths to analyze
    let paths: Vec<PathBuf> = if cli.paths.is_empty() {
        vec![PathBuf::from(".")]
    } else {
        cli.paths.clone()
    };

    // Run analysis
    let result = analyze(&paths, &config).context("Analysis failed")?;

    // Quiet with no -O file means nothing gets written; skip building
    // the report at all (write_report re-checks this for other callers).
    if config.output.quiet && config.output.file.is_none() {
        return Ok(ExitCode::SUCCESS);
    }

    // Bundle stats + health + estimation into ONE report so machine
    // formats (JSON/HTML) stay parseable as a single document.
    let scoring_model = scoring_model_for(config.health_model, result.summary.dup_scanned);
    let health_top_n = config.output.top_n.unwrap_or(10);
    let health_report =
        codelens_core::insight::health::score(&result, scoring_model.as_ref(), health_top_n);

    let cost_config = codelens_core::CostConfig::default();
    let cocomo_basic = codelens_core::CocomoBasicModel::default();
    let cocomo2 = codelens_core::CocomoIIModel::default();
    let putnam = codelens_core::PutnamModel::default();
    let locomo = codelens_core::LocomoModel::default();
    let models: Vec<&dyn codelens_core::EstimationModel> =
        vec![&cocomo_basic, &cocomo2, &putnam, &locomo];
    let comparison =
        codelens_core::insight::estimation::estimate_all(&result.summary, &models, &cost_config);

    let combined = codelens_core::output::CombinedReport {
        analysis: result,
        health: health_report,
        estimation: comparison,
    };

    if config.output.format == codelens_core::config::OutputFormatType::Html {
        let insights =
            collect_default_html_insights(&paths, &combined.analysis, &config, health_top_n);
        write_combined_html_dashboard(&combined, &insights, &config.output)?;
    } else {
        write_report(Report::Combined(Box::new(combined)), &config.output)?;
    }
    Ok(ExitCode::SUCCESS)
}

#[derive(Default)]
struct DefaultHtmlInsights {
    hotspot: Option<codelens_core::insight::hotspot::HotspotReport>,
    coupling: Option<codelens_core::insight::coupling::CouplingReport>,
    trend: Option<codelens_core::insight::trend::TrendReport>,
    git_history_note: String,
    trend_note: String,
}

fn collect_default_html_insights(
    paths: &[PathBuf],
    analysis: &codelens_core::AnalysisResult,
    config: &Config,
    top_n: usize,
) -> DefaultHtmlInsights {
    use codelens_core::git;
    use codelens_core::insight::trend;

    const HISTORY_WINDOW: &str = "90d";

    let project_path = paths.first().cloned().unwrap_or_else(|| PathBuf::from("."));
    let lookup_path = if project_path.is_file() {
        project_path
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from("."))
    } else {
        project_path.clone()
    };
    let mut insights = DefaultHtmlInsights::default();

    match git_client_for_paths(paths) {
        Ok(git_client) => {
            let since = git::parse_since(HISTORY_WINDOW);
            match git_client.commit_log(&since) {
                Ok(commits) => {
                    let first_commits = git_client.first_commit_times().ok();
                    let now_ts = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .ok()
                        .map(|duration| duration.as_secs() as i64);
                    let hotspot_query = hotspot::HotspotQuery {
                        since: HISTORY_WINDOW.to_string(),
                        top_n,
                        functions: false,
                    };
                    match hotspot::analyze_hotspots_from_history(
                        analysis,
                        &git_client,
                        &commits,
                        &hotspot_query,
                        first_commits.as_ref(),
                        now_ts,
                        config,
                    ) {
                        Ok(run) => insights.hotspot = Some(run.report),
                        Err(error) => {
                            insights.git_history_note =
                                format!("Hotspot analysis could not be completed: {error}");
                        }
                    }

                    let coupling_query = coupling::CouplingQuery {
                        top_n,
                        ..coupling::CouplingQuery::default()
                    };
                    let run = coupling::analyze_coupling_from_history(
                        analysis,
                        &git_client,
                        &commits,
                        &coupling_query,
                    );
                    insights.coupling = Some(run.report);
                }
                Err(error) => {
                    insights.git_history_note =
                        format!("Git history could not be read for this report: {error}");
                }
            }
        }
        Err(error) => {
            insights.git_history_note = format!(
                "Git history is unavailable for this report: {error}. Run Codelens inside one Git repository to include hotspots and coupling."
            );
        }
    }

    match trend::list_snapshots(&lookup_path) {
        Ok(snapshots) if !snapshots.is_empty() => {
            let latest = snapshots.last().expect("checked as non-empty").clone();
            match trend::load_snapshot(&latest.file_path) {
                Ok(snapshot) => {
                    let (delta, by_language) =
                        trend::compute_delta(&snapshot.result.summary, &analysis.summary);
                    let now = chrono::Utc::now();
                    let mut history = trend::history(&lookup_path).unwrap_or_default();
                    history.push(trend::HistoryPoint {
                        timestamp: now,
                        label: Some("Current".to_string()),
                        files: analysis.summary.total_files,
                        code: analysis.summary.lines.code,
                        cyclomatic: analysis.summary.complexity.cyclomatic,
                    });
                    insights.trend = Some(trend::TrendReport {
                        from: latest,
                        to: trend::SnapshotMeta {
                            timestamp: now,
                            label: Some("Current".to_string()),
                            git_commit: None,
                            file_path: PathBuf::new(),
                        },
                        delta,
                        by_language,
                        history,
                    });
                }
                Err(error) => {
                    insights.trend_note =
                        format!("The latest Codelens snapshot could not be read: {error}");
                }
            }
        }
        Ok(_) => {
            insights.trend_note = "No saved snapshot is available. Run `codelens trend --save` once, then regenerate this report to see historical changes.".to_string();
        }
        Err(error) => {
            insights.trend_note = format!("Codelens snapshots could not be read: {error}");
        }
    }

    insights
}

/// Git-backed reports can combine multiple paths only when one repository
/// owns every path. Otherwise a single commit log cannot describe the full
/// analysis and returning partial history would be misleading.
pub(crate) fn git_client_for_paths(paths: &[PathBuf]) -> Result<GitClient> {
    let first_path = paths.first().context("No analysis path provided")?;
    let first_lookup = if first_path.is_file() {
        first_path.parent().unwrap_or_else(|| Path::new("."))
    } else {
        first_path.as_path()
    };
    let first = GitClient::detect(first_lookup).context("Not a git repository")?;
    let expected = std::fs::canonicalize(first.repo_path())
        .unwrap_or_else(|_| first.repo_path().to_path_buf());

    for path in &paths[1..] {
        let lookup = if path.is_file() {
            path.parent().unwrap_or_else(|| Path::new("."))
        } else {
            path.as_path()
        };
        let client = GitClient::detect(lookup).with_context(|| {
            format!(
                "analysis path '{}' is not in a Git repository",
                path.display()
            )
        })?;
        let repository = std::fs::canonicalize(client.repo_path())
            .unwrap_or_else(|_| client.repo_path().to_path_buf());
        if repository != expected {
            bail!(
                "all analysis paths must belong to the same Git repository; '{}' belongs to '{}' instead of '{}'",
                path.display(),
                client.repo_path().display(),
                first.repo_path().display(),
            );
        }
    }

    Ok(first)
}

pub(crate) fn list_languages(languages_file: Option<&Path>) -> Result<()> {
    let mut registry = LanguageRegistry::with_builtin()?;
    if let Some(path) = languages_file {
        registry.load_file(path)?;
    }

    println!("{}", "Supported Languages".bold());
    println!("{}", "─".repeat(40));

    let mut languages: Vec<_> = registry.all().collect();
    languages.sort_by(|a, b| a.name.cmp(&b.name));

    let count = languages.len();
    for lang in languages {
        let exts = lang.extensions.join(", ");
        println!("  {} {}", lang.name.cyan(), format!("({})", exts).dimmed());
    }

    println!();
    println!("Total: {} languages", count.to_string().green());

    Ok(())
}

/// Load the config file specified by `--config`, or search default locations.
///
/// Unlike the previous behavior, a config file that exists but fails to parse
/// is a hard error even when found via the default search path — silently
/// ignoring it made bad configs undiagnosable.
pub(crate) fn load_partial_config(advanced: &cli::AdvancedArgs) -> Result<Option<PartialConfig>> {
    if advanced.no_config {
        return Ok(None);
    }
    if let Some(ref path) = advanced.config {
        return Ok(Some(codelens_core::config::load_config_file(path)?));
    }
    let default_path = PathBuf::from(".codelens.toml");
    if default_path.exists() {
        return Ok(Some(codelens_core::config::load_config_file(
            &default_path,
        )?));
    }
    Ok(None)
}

/// Merge configuration from three layers, later layers winning:
/// built-in defaults → config file → explicitly passed CLI arguments.
fn resolve_config(
    filter: &cli::FilterArgs,
    output: &cli::OutputArgs,
    advanced: &cli::AdvancedArgs,
    partial: Option<&PartialConfig>,
) -> Config {
    let mut config = Config::default();
    config.walker.threads = num_cpus::get();
    config.filter.smart_exclude = true;

    if let Some(partial) = partial {
        partial.apply_to(&mut config);
    }

    // CLI overrides: only fields the user explicitly passed.
    if let Some(threads) = advanced.threads {
        config.walker.threads = threads;
    }
    if advanced.no_dup_scan {
        config.no_dup_scan = true;
    }
    if let Some(health_model) = advanced.health_model {
        config.health_model = health_model.into();
    }
    if let Some(ref path) = advanced.languages_file {
        config.languages_file = Some(path.clone());
    }
    if filter.no_gitignore {
        config.walker.use_gitignore = false;
    }
    if let Some(depth) = filter.depth {
        config.walker.max_depth = Some(depth);
    }

    if let Some(ref excludes) = filter.exclude {
        config.filter.excludes = excludes.clone();
    }
    if let Some(ref pattern) = filter.exclude_files {
        config.filter.exclude_files = vec![pattern.clone()];
    }
    if let Some(ref pattern) = filter.include_files {
        config.filter.include_files = vec![pattern.clone()];
    }
    if let Some(ref langs) = filter.lang {
        config.filter.languages = langs.clone();
    }
    if let Some(min_lines) = filter.min_lines {
        config.filter.min_lines = Some(min_lines);
    }
    if let Some(max_lines) = filter.max_lines {
        config.filter.max_lines = Some(max_lines);
    }
    if filter.no_smart_exclude {
        config.filter.smart_exclude = false;
    }
    if filter.all {
        config.filter.include_all = true;
    }
    if filter.no_duplicates {
        config.filter.no_duplicates = true;
    }
    if filter.no_min_gen {
        config.filter.no_min_gen = true;
    }
    if filter.no_linguist {
        config.filter.no_linguist = true;
    }
    if let Some(ref count_as) = filter.count_as {
        config.count_as = parse_count_as(count_as);
    }

    if let Some(format) = output.format {
        config.output.format = format.into();
    }
    if let Some(ref path) = output.output_file {
        config.output.file = Some(path.clone());
    }
    if output.summary {
        config.output.summary_only = true;
    }
    if output.by_file {
        config.output.by_file = true;
    }
    if output.by_dir {
        config.output.by_dir = true;
    }
    if let Some(depth) = output.dir_depth {
        config.output.dir_depth = depth.max(1);
    }
    if output.tokens {
        config.output.show_tokens = true;
    }
    if let Some(sort) = output.sort {
        config.output.sort_by = sort.into();
    }
    if let Some(top) = output.top {
        config.output.top_n = Some(top);
    }
    if output.verbose {
        config.output.verbose = true;
    }
    if output.quiet {
        config.output.quiet = true;
    }
    if advanced.git_info {
        config.output.show_git_info = true;
    }

    config
}

pub(crate) fn build_config(cli: &Cli) -> Result<Config> {
    let partial = load_partial_config(&cli.advanced)?;
    Ok(resolve_config(
        &cli.filter,
        &cli.output,
        &cli.advanced,
        partial.as_ref(),
    ))
}

/// Scoring model matching an analysis: when duplication was not
/// collected the Duplication dimension is excluded (its weight
/// redistributed) instead of scoring the absent data as clean.
pub(crate) fn scoring_model_for(
    version: HealthModelVersion,
    dup_scanned: bool,
) -> Box<dyn ScoringModel> {
    codelens_core::insight::scoring::scoring_model_for(version, dup_scanned)
}

/// Rewrite analysis file paths to be repo-root-relative.
pub(crate) fn rewrite_paths_repo_relative(
    files: &mut [codelens_core::FileStats],
    repo_root: &Path,
) {
    let repo_root = std::fs::canonicalize(repo_root).unwrap_or_else(|_| repo_root.to_path_buf());
    for f in files {
        if let Ok(abs) = std::fs::canonicalize(&f.path) {
            if let Ok(rel) = abs.strip_prefix(&repo_root) {
                f.path = rel.to_path_buf();
            }
        }
    }
}

/// Parse "jsp:html,tpl:php" into (extension, language) pairs.
/// Entries without a ':' are ignored.
fn parse_count_as(spec: &str) -> Vec<(String, String)> {
    spec.split(',')
        .filter_map(|pair| {
            let (ext, lang) = pair.split_once(':')?;
            let (ext, lang) = (ext.trim(), lang.trim());
            if ext.is_empty() || lang.is_empty() {
                return None;
            }
            Some((ext.to_string(), lang.to_string()))
        })
        .collect()
}

/// Colors belong on an interactive terminal only — never in an -O file,
/// and not when stdout is piped/redirected.
fn should_colorize(output: &codelens_core::config::OutputConfig) -> bool {
    use std::io::IsTerminal;
    output.file.is_none() && io::stdout().is_terminal()
}

/// Formatter options derived from the merged output config.
fn report_output_options(output: &codelens_core::config::OutputConfig) -> OutputOptions {
    OutputOptions {
        summary_only: output.summary_only,
        by_file: output.by_file,
        by_dir: output.by_dir,
        dir_depth: output.dir_depth,
        show_tokens: output.show_tokens,
        sort_by: output.sort_by,
        top_n: output.top_n,
        colorize: should_colorize(output),
        show_git_info: output.show_git_info,
    }
}

fn write_report(report: Report, output: &codelens_core::config::OutputConfig) -> Result<()> {
    write_report_targets(&report, std::iter::once(output))
}

fn write_report_targets<'a>(
    report: &Report,
    outputs: impl IntoIterator<Item = &'a codelens_core::config::OutputConfig>,
) -> Result<()> {
    for output in outputs {
        let output_options = report_output_options(output);
        let formatter = create_output(output.format);

        // Quiet suppresses terminal output only; an explicit -O file is still written.
        if output.quiet && output.file.is_none() {
            continue;
        }

        if let Some(ref path) = output.file {
            let file = File::create(path).context("Failed to create output file")?;
            let mut writer = BufWriter::new(file);
            formatter.write(report, &output_options, &mut writer)?;
            writer.flush()?;
            if !output.quiet {
                println!("Output written to: {}", path.display().to_string().green());
            }
        } else {
            let stdout = io::stdout();
            let mut writer = stdout.lock();
            formatter.write(report, &output_options, &mut writer)?;
        }
    }
    Ok(())
}

fn write_combined_html_dashboard(
    combined: &codelens_core::output::CombinedReport,
    insights: &DefaultHtmlInsights,
    output: &codelens_core::config::OutputConfig,
) -> Result<()> {
    let output_options = report_output_options(output);
    let formatter = codelens_core::output::HtmlOutput::new();

    if output.quiet && output.file.is_none() {
        return Ok(());
    }

    if let Some(ref path) = output.file {
        let file = File::create(path).context("Failed to create output file")?;
        let mut writer = BufWriter::new(file);
        formatter.write_combined_dashboard(
            combined,
            insights.hotspot.as_ref(),
            insights.coupling.as_ref(),
            insights.trend.as_ref(),
            &insights.git_history_note,
            &insights.trend_note,
            &output_options,
            &mut writer,
        )?;
        writer.flush()?;
        if !output.quiet {
            println!("Output written to: {}", path.display().to_string().green());
        }
    } else {
        let stdout = io::stdout();
        let mut writer = stdout.lock();
        formatter.write_combined_dashboard(
            combined,
            insights.hotspot.as_ref(),
            insights.coupling.as_ref(),
            insights.trend.as_ref(),
            &insights.git_history_note,
            &insights.trend_note,
            &output_options,
            &mut writer,
        )?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;
    use codelens_core::config::OutputFormatType;

    fn parse(args: &[&str]) -> Cli {
        Cli::try_parse_from(args).unwrap()
    }

    #[test]
    fn default_html_insights_keep_shared_hotspot_and_coupling_sections() {
        use std::process::Command;

        let repository = tempfile::tempdir().unwrap();
        let git = |args: &[&str]| {
            let output = Command::new("git")
                .args(args)
                .current_dir(repository.path())
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "git {args:?} failed: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        };
        git(&["init"]);
        git(&["config", "user.email", "test@example.com"]);
        git(&["config", "user.name", "Test"]);
        git(&["config", "commit.gpgsign", "false"]);
        std::fs::write(repository.path().join("a.rs"), "fn a() {}\n").unwrap();
        std::fs::write(repository.path().join("b.rs"), "fn b() {}\n").unwrap();
        git(&["add", "a.rs", "b.rs"]);
        git(&["commit", "-m", "initial"]);
        std::fs::write(
            repository.path().join("a.rs"),
            "fn a() { println!(\"a\"); }\n",
        )
        .unwrap();
        std::fs::write(
            repository.path().join("b.rs"),
            "fn b() { println!(\"b\"); }\n",
        )
        .unwrap();
        git(&["add", "a.rs", "b.rs"]);
        git(&["commit", "-m", "change together"]);

        let config = Config::default();
        let paths = vec![repository.path().to_path_buf()];
        let analysis = codelens_core::analyze(&paths, &config).unwrap();
        let insights = collect_default_html_insights(&paths, &analysis, &config, 20);

        assert!(insights.hotspot.is_some());
        assert!(insights.coupling.is_some());
    }

    #[test]
    fn git_client_accepts_multiple_paths_in_one_repository() {
        let repository = tempfile::tempdir().unwrap();
        let output = std::process::Command::new("git")
            .args(["init", "-q"])
            .current_dir(repository.path())
            .output()
            .unwrap();
        assert!(output.status.success());
        let first = repository.path().join("first");
        let second = repository.path().join("second");
        std::fs::create_dir_all(&first).unwrap();
        std::fs::create_dir_all(&second).unwrap();

        let client = git_client_for_paths(&[first, second]).unwrap();

        assert_eq!(
            std::fs::canonicalize(client.repo_path()).unwrap(),
            std::fs::canonicalize(repository.path()).unwrap()
        );
    }

    fn partial(toml_src: &str) -> PartialConfig {
        toml::from_str(toml_src).unwrap()
    }

    #[test]
    fn config_file_values_survive_when_cli_args_absent() {
        let cli = parse(&["codelens"]);
        let p = partial(
            r#"
            output = "json"
            lang = "rust,go"
            excludes = "vendor"
            threads = 3
            sort = "code"
            quiet = true
        "#,
        );
        let config = resolve_config(&cli.filter, &cli.output, &cli.advanced, Some(&p));

        assert_eq!(config.output.format, OutputFormatType::Json);
        assert_eq!(config.filter.languages, vec!["rust", "go"]);
        assert_eq!(config.filter.excludes, vec!["vendor"]);
        assert_eq!(config.walker.threads, 3);
        assert_eq!(config.output.sort_by, codelens_core::config::SortBy::Code);
        assert!(config.output.quiet);
    }

    #[test]
    fn explicit_cli_args_override_config_file() {
        let cli = parse(&["codelens", "-f", "csv", "-l", "python", "-j", "8"]);
        let p = partial(
            r#"
            output = "json"
            lang = "rust"
            threads = 3
        "#,
        );
        let config = resolve_config(&cli.filter, &cli.output, &cli.advanced, Some(&p));

        assert_eq!(config.output.format, OutputFormatType::Csv);
        assert_eq!(config.filter.languages, vec!["python"]);
        assert_eq!(config.walker.threads, 8);
    }

    #[test]
    fn subcommands_accept_global_advanced_args() {
        // -j and --config used to be rejected after a subcommand.
        let cli = parse(&["codelens", "health", ".", "-j", "2", "--no-config"]);
        assert_eq!(cli.advanced.threads, Some(2));
        assert!(cli.advanced.no_config);
    }

    #[test]
    fn subcommand_filter_args_reach_config() {
        // --min-lines & co. used to be accepted but silently dropped.
        let cli = parse(&["codelens", "health", ".", "--min-lines", "5"]);
        let cli::Command::Health(args) = cli.command.as_ref().unwrap() else {
            panic!("expected health subcommand");
        };
        let config = resolve_config(&args.filter, &args.output, &cli.advanced, None);
        assert_eq!(config.filter.min_lines, Some(5));
    }

    #[test]
    fn no_dup_scan_reaches_config_from_subcommands() {
        // Global flag: must parse after a subcommand and land in Config.
        let cli = parse(&["codelens", "health", ".", "--no-dup-scan"]);
        assert!(cli.advanced.no_dup_scan);
        let cli::Command::Health(args) = cli.command.as_ref().unwrap() else {
            panic!("expected health subcommand");
        };
        let config = resolve_config(&args.filter, &args.output, &cli.advanced, None);
        assert!(config.no_dup_scan);

        // Default stays off.
        let cli = parse(&["codelens"]);
        let config = resolve_config(&cli.filter, &cli.output, &cli.advanced, None);
        assert!(!config.no_dup_scan);
    }

    #[test]
    fn languages_file_reaches_config_from_subcommands() {
        // Global flag: must parse after a subcommand and land in Config.
        let cli = parse(&["codelens", "health", ".", "--languages-file", "custom.toml"]);
        let cli::Command::Health(args) = cli.command.as_ref().unwrap() else {
            panic!("expected health subcommand");
        };
        let config = resolve_config(&args.filter, &args.output, &cli.advanced, None);
        assert_eq!(config.languages_file, Some(PathBuf::from("custom.toml")));

        // Default stays off.
        let cli = parse(&["codelens"]);
        let config = resolve_config(&cli.filter, &cli.output, &cli.advanced, None);
        assert!(config.languages_file.is_none());
    }

    #[test]
    fn languages_file_cli_overrides_config_file() {
        let p = partial(r#"languages_file = "from-config.toml""#);

        // Config file value survives when the CLI flag is absent.
        let cli = parse(&["codelens"]);
        let config = resolve_config(&cli.filter, &cli.output, &cli.advanced, Some(&p));
        assert_eq!(
            config.languages_file,
            Some(PathBuf::from("from-config.toml"))
        );

        // An explicit CLI flag wins.
        let cli = parse(&["codelens", "--languages-file", "from-cli.toml"]);
        let config = resolve_config(&cli.filter, &cli.output, &cli.advanced, Some(&p));
        assert_eq!(config.languages_file, Some(PathBuf::from("from-cli.toml")));
    }

    #[test]
    fn drop_submodule_files_preserves_dup_scanned() {
        let files = vec![
            codelens_core::FileStats {
                path: PathBuf::from("src/a.rs"),
                ..Default::default()
            },
            codelens_core::FileStats {
                path: PathBuf::from("vendor/sub/b.rs"),
                ..Default::default()
            },
        ];
        let mut summary = codelens_core::Summary::from_file_stats(&files);
        summary.dup_scanned = false;
        summary.inline_test_regions_scanned = true;
        summary.health_regions.insert(
            PathBuf::from("src/a.rs"),
            codelens_core::HealthRegions::default(),
        );
        summary.health_regions.insert(
            PathBuf::from("vendor/sub/b.rs"),
            codelens_core::HealthRegions::default(),
        );
        let mut result = codelens_core::AnalysisResult {
            files,
            summary,
            elapsed: std::time::Duration::from_millis(1),
            scanned_files: 2,
            skipped_files: 0,
            error_files: 0,
        };

        compare::drop_submodule_files(&mut result, &[PathBuf::from("vendor/sub")]);
        assert_eq!(result.files.len(), 1);
        assert!(
            !result.summary.dup_scanned,
            "summary rebuild must not resurrect the flag"
        );
        assert!(result.summary.inline_test_regions_scanned);
        assert_eq!(result.summary.health_regions.len(), 1);
        assert!(result
            .summary
            .health_regions
            .contains_key(std::path::Path::new("src/a.rs")));

        // And the measured state survives too.
        result.summary.dup_scanned = true;
        compare::drop_submodule_files(&mut result, &[PathBuf::from("other")]);
        assert!(result.summary.dup_scanned);
    }

    #[test]
    fn rewrite_analysis_paths_keeps_health_regions_aligned() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("src/lib.rs");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, "fn production() {}\n").unwrap();
        let files = vec![codelens_core::FileStats {
            path: path.clone(),
            ..Default::default()
        }];
        let mut summary = codelens_core::Summary::from_file_stats(&files);
        summary
            .health_regions
            .insert(path, codelens_core::HealthRegions::default());
        let mut result = codelens_core::AnalysisResult {
            files,
            summary,
            elapsed: std::time::Duration::ZERO,
            scanned_files: 1,
            skipped_files: 0,
            error_files: 0,
        };

        compare::rewrite_analysis_paths_repo_relative(&mut result, dir.path());

        assert_eq!(result.files[0].path, PathBuf::from("src/lib.rs"));
        assert!(result
            .summary
            .health_regions
            .contains_key(std::path::Path::new("src/lib.rs")));
    }

    #[test]
    fn drop_submodule_files_preserves_uloc() {
        let files = vec![
            codelens_core::FileStats {
                path: PathBuf::from("src/a.rs"),
                ..Default::default()
            },
            codelens_core::FileStats {
                path: PathBuf::from("vendor/sub/b.rs"),
                ..Default::default()
            },
        ];
        let mut summary = codelens_core::Summary::from_file_stats(&files);
        summary.dup_scanned = true;
        summary.uloc = 42;
        let mut result = codelens_core::AnalysisResult {
            files,
            summary,
            elapsed: std::time::Duration::from_millis(1),
            scanned_files: 2,
            skipped_files: 0,
            error_files: 0,
        };

        compare::drop_submodule_files(&mut result, &[PathBuf::from("vendor/sub")]);
        assert_eq!(
            result.summary.uloc, 42,
            "summary rebuild must not zero out the measured ULOC"
        );
    }

    #[test]
    fn scoring_model_drops_duplication_when_not_scanned() {
        use codelens_core::insight::scoring::HealthDimension;

        let with_dup = scoring_model_for(HealthModelVersion::V2, true);
        assert!(with_dup
            .dimensions()
            .iter()
            .any(|d| d.dimension == HealthDimension::Duplication));

        let without_dup = scoring_model_for(HealthModelVersion::V2, false);
        assert!(without_dup
            .dimensions()
            .iter()
            .all(|d| d.dimension != HealthDimension::Duplication));
    }

    #[test]
    fn report_output_options_follow_config() {
        // write_report used to hardcode show_git_info: false while
        // run_default's inline copy read it from config — the formatter
        // options must be config-driven in both paths.
        let mut output = codelens_core::config::OutputConfig {
            show_git_info: true,
            ..Default::default()
        };
        assert!(report_output_options(&output).show_git_info);

        output.show_git_info = false;
        assert!(!report_output_options(&output).show_git_info);
    }

    #[test]
    fn one_report_can_be_written_to_multiple_format_targets() {
        let dir = tempfile::tempdir().unwrap();
        let analysis = codelens_core::AnalysisResult {
            files: Vec::new(),
            summary: codelens_core::Summary::default(),
            elapsed: std::time::Duration::ZERO,
            scanned_files: 0,
            skipped_files: 0,
            error_files: 0,
        };
        let health = codelens_core::insight::health::score(
            &analysis,
            &codelens_core::insight::scoring::default::DefaultModel::default(),
            10,
        );
        let report = Report::Health(health);
        let outputs = [
            (
                codelens_core::config::OutputFormatType::Markdown,
                dir.path().join("health.md"),
            ),
            (
                codelens_core::config::OutputFormatType::Json,
                dir.path().join("health.json"),
            ),
            (
                codelens_core::config::OutputFormatType::Badge,
                dir.path().join("badge.json"),
            ),
        ]
        .map(|(format, file)| codelens_core::config::OutputConfig {
            format,
            file: Some(file),
            quiet: true,
            ..Default::default()
        });

        write_report_targets(&report, outputs.iter()).unwrap();

        assert!(std::fs::read_to_string(dir.path().join("health.md"))
            .unwrap()
            .contains("Code Health Report"));
        assert!(serde_json::from_str::<serde_json::Value>(
            &std::fs::read_to_string(dir.path().join("health.json")).unwrap()
        )
        .is_ok());
        assert!(serde_json::from_str::<serde_json::Value>(
            &std::fs::read_to_string(dir.path().join("badge.json")).unwrap()
        )
        .is_ok());
    }

    #[test]
    fn defaults_apply_without_config_file() {
        let cli = parse(&["codelens"]);
        let config = resolve_config(&cli.filter, &cli.output, &cli.advanced, None);

        assert_eq!(config.output.format, OutputFormatType::Console);
        assert!(config.filter.languages.is_empty());
        assert!(config.filter.smart_exclude);
        assert!(config.walker.use_gitignore);
        assert_eq!(config.health_model, HealthModelVersion::V1);
    }

    #[test]
    fn all_flag_enables_include_all_without_disabling_ignore_files() {
        let cli = parse(&["codelens", "--all"]);
        let config = resolve_config(&cli.filter, &cli.output, &cli.advanced, None);

        assert!(config.filter.include_all);
        assert!(config.filter.smart_exclude);
        assert!(config.walker.use_gitignore);
    }

    #[test]
    fn health_model_cli_overrides_config_file() {
        let partial = partial("health_model = \"v2\"");
        let cli = parse(&["codelens"]);
        let config = resolve_config(&cli.filter, &cli.output, &cli.advanced, Some(&partial));
        assert_eq!(config.health_model, HealthModelVersion::V2);

        let cli = parse(&["codelens", "health", ".", "--health-model", "v1"]);
        let cli::Command::Health(args) = cli.command.as_ref().unwrap() else {
            panic!("expected health subcommand");
        };
        let config = resolve_config(&args.filter, &args.output, &cli.advanced, Some(&partial));
        assert_eq!(config.health_model, HealthModelVersion::V1);
    }
}
