//! Codelens CLI - High performance code analysis tool.

mod cli;

use std::fs::File;
use std::io::{self, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use anyhow::{Context, Result};
use clap::Parser;
use colored::Colorize;
use tracing_subscriber::EnvFilter;

use codelens_core::config::{Config, PartialConfig};
use codelens_core::git::{self, GitClient};
use codelens_core::insight::scoring::default::DefaultModel;
use codelens_core::insight::{coupling, health, hotspot, trend};
use codelens_core::output::{create_output, OutputOptions, Report};
use codelens_core::{analyze, LanguageRegistry};

use crate::cli::{Cli, OutputFormatArg, SortByArg};

fn main() -> ExitCode {
    match run() {
        Ok(code) => code,
        Err(e) => {
            eprintln!("{}: {e:#}", "error".red().bold());
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<ExitCode> {
    let cli = Cli::parse();

    // Initialize logging
    init_tracing(cli.output.verbose);

    // Handle special commands
    if cli.advanced.list_languages {
        return list_languages().map(|()| ExitCode::SUCCESS);
    }

    // Handle subcommands (they share config loading with the main command)
    if let Some(ref command) = cli.command {
        return match command {
            // health owns its exit code: --fail-under can gate CI
            cli::Command::Health(args) => run_health(args, &cli.advanced),
            cli::Command::Hotspot(args) => {
                run_hotspot(args, &cli.advanced).map(|()| ExitCode::SUCCESS)
            }
            cli::Command::Coupling(args) => {
                run_coupling(args, &cli.advanced).map(|()| ExitCode::SUCCESS)
            }
            cli::Command::Trend(args) => run_trend(args, &cli.advanced).map(|()| ExitCode::SUCCESS),
            cli::Command::Estimate(args) => {
                run_estimate(args, &cli.advanced).map(|()| ExitCode::SUCCESS)
            }
        };
    }

    // Build configuration
    let config = build_config(&cli)?;

    // Collect paths to analyze
    let paths: Vec<PathBuf> = if cli.paths.is_empty() {
        vec![PathBuf::from(".")]
    } else {
        cli.paths.clone()
    };

    // Run analysis
    let result = analyze(&paths, &config).context("Analysis failed")?;

    // Prepare output options from the merged config (defaults → file → CLI)
    let output_options = OutputOptions {
        summary_only: config.output.summary_only,
        by_file: config.output.by_file,
        sort_by: config.output.sort_by,
        top_n: config.output.top_n,
        colorize: should_colorize(&config.output),
        show_git_info: config.output.show_git_info,
    };

    // Get output formatter
    let formatter = create_output(config.output.format);

    // Quiet suppresses terminal output only; an explicit -O file is still written
    if config.output.quiet && config.output.file.is_none() {
        return Ok(ExitCode::SUCCESS);
    }

    // Bundle stats + health + estimation into ONE report so machine
    // formats (JSON/HTML) stay parseable as a single document.
    let scoring_model = DefaultModel::new();
    let health_top_n = config.output.top_n.unwrap_or(10);
    let health_report = health::score(&result, &scoring_model, health_top_n);

    let cost_config = codelens_core::CostConfig::default();
    let cocomo_basic = codelens_core::CocomoBasicModel::default();
    let cocomo2 = codelens_core::CocomoIIModel::default();
    let putnam = codelens_core::PutnamModel::default();
    let locomo = codelens_core::LocomoModel::default();
    let models: Vec<&dyn codelens_core::EstimationModel> =
        vec![&cocomo_basic, &cocomo2, &putnam, &locomo];
    let comparison =
        codelens_core::insight::estimation::estimate_all(&result.summary, &models, &cost_config);

    let report = Report::Combined(Box::new(codelens_core::output::CombinedReport {
        analysis: result,
        health: health_report,
        estimation: comparison,
    }));

    if let Some(ref path) = config.output.file {
        let file = File::create(path).context("Failed to create output file")?;
        let mut writer = BufWriter::new(file);
        formatter.write(&report, &output_options, &mut writer)?;
        writer.flush()?;
        if !config.output.quiet {
            println!("Output written to: {}", path.display().to_string().green());
        }
    } else {
        let stdout = io::stdout();
        let mut writer = stdout.lock();
        formatter.write(&report, &output_options, &mut writer)?;
    }

    Ok(ExitCode::SUCCESS)
}

fn init_tracing(verbose: bool) {
    let filter = if verbose {
        EnvFilter::new("debug")
    } else {
        EnvFilter::new("warn")
    };

    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .without_time()
        .init();
}

fn list_languages() -> Result<()> {
    let registry = LanguageRegistry::with_builtin()?;

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
fn load_partial_config(advanced: &cli::AdvancedArgs) -> Result<Option<PartialConfig>> {
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

fn build_config(cli: &Cli) -> Result<Config> {
    let partial = load_partial_config(&cli.advanced)?;
    Ok(resolve_config(
        &cli.filter,
        &cli.output,
        &cli.advanced,
        partial.as_ref(),
    ))
}

fn run_health(args: &cli::HealthArgs, advanced: &cli::AdvancedArgs) -> Result<ExitCode> {
    // Validate the threshold BEFORE the (potentially long) analysis
    let threshold = args
        .fail_under
        .as_deref()
        .map(parse_fail_under)
        .transpose()?;

    let partial = load_partial_config(advanced)?;
    let config = resolve_config(&args.filter, &args.output, advanced, partial.as_ref());
    let result = analyze(&args.paths, &config).context("Analysis failed")?;
    let model = DefaultModel::new();
    let top_n = config.output.top_n.unwrap_or(10);
    let report = health::score(&result, &model, top_n);
    let score = report.score;
    let grade = report.grade;
    write_report(Report::Health(report), &config.output)?;

    if let Some(threshold) = threshold {
        if score < threshold {
            eprintln!(
                "{}: health score {:.1} (grade {}) is below --fail-under threshold {}",
                "gate failed".red().bold(),
                score,
                grade,
                args.fail_under.as_deref().unwrap_or_default(),
            );
            return Ok(ExitCode::FAILURE);
        }
    }
    Ok(ExitCode::SUCCESS)
}

/// Parse a `--fail-under` threshold: a grade letter (A/B/C/D, using the
/// scoring model's grade boundaries) or a numeric score (0-100).
fn parse_fail_under(input: &str) -> Result<f64> {
    match input.trim() {
        "A" | "a" => Ok(90.0),
        "B" | "b" => Ok(80.0),
        "C" | "c" => Ok(70.0),
        "D" | "d" => Ok(60.0),
        other => {
            if let Ok(score) = other.parse::<f64>() {
                if (0.0..=100.0).contains(&score) {
                    return Ok(score);
                }
            }
            anyhow::bail!(
                "invalid --fail-under value '{other}': expected a grade (A/B/C/D) or a score between 0 and 100"
            )
        }
    }
}

fn run_hotspot(args: &cli::HotspotArgs, advanced: &cli::AdvancedArgs) -> Result<()> {
    let partial = load_partial_config(advanced)?;
    let config = resolve_config(&args.filter, &args.output, advanced, partial.as_ref());
    let git_client = GitClient::detect(&args.paths[0]).context("Not a git repository")?;
    let since = git::parse_since(&args.since);
    let mut result = analyze(&args.paths, &config).context("Analysis failed")?;

    // git numstat paths are repo-root-relative; analysis paths are relative
    // to the walk root (or absolute). Rewrite them so the two sides match
    // even when running from a subdirectory or with absolute paths.
    rewrite_paths_repo_relative(&mut result.files, git_client.repo_path());

    let churns = git_client.file_churn(&since)?;
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
    write_report(Report::Hotspot(report), &config.output)
}

fn run_coupling(args: &cli::CouplingArgs, advanced: &cli::AdvancedArgs) -> Result<()> {
    let partial = load_partial_config(advanced)?;
    let config = resolve_config(&args.filter, &args.output, advanced, partial.as_ref());
    let git_client = GitClient::detect(&args.paths[0]).context("Not a git repository")?;
    let since = git::parse_since(&args.since);

    // Restrict pairing to the files present in the current tree (after
    // filters), so deleted files and excluded directories stay out.
    let mut result = analyze(&args.paths, &config).context("Analysis failed")?;
    rewrite_paths_repo_relative(&mut result.files, git_client.repo_path());
    let universe: std::collections::BTreeSet<PathBuf> = result
        .files
        .iter()
        .map(|f| f.path.strip_prefix("./").unwrap_or(&f.path).to_path_buf())
        .collect();

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
    let report = coupling::analyze(&commits, Some(&universe), &args.since, &opts);
    write_report(Report::Coupling(report), &config.output)
}

/// Rewrite analysis file paths to be repo-root-relative.
fn rewrite_paths_repo_relative(files: &mut [codelens_core::FileStats], repo_root: &Path) {
    let repo_root = std::fs::canonicalize(repo_root).unwrap_or_else(|_| repo_root.to_path_buf());
    for f in files {
        if let Ok(abs) = std::fs::canonicalize(&f.path) {
            if let Ok(rel) = abs.strip_prefix(&repo_root) {
                f.path = rel.to_path_buf();
            }
        }
    }
}

fn run_trend(args: &cli::TrendArgs, advanced: &cli::AdvancedArgs) -> Result<()> {
    let project_root = args
        .paths
        .first()
        .cloned()
        .unwrap_or_else(|| PathBuf::from("."));

    if args.list {
        let metas = trend::list_snapshots(&project_root)?;
        if metas.is_empty() {
            println!("No snapshots found. Use --save to create one.");
            return Ok(());
        }
        for meta in &metas {
            let label = meta.label.as_deref().unwrap_or("");
            let commit = meta.git_commit.as_deref().unwrap_or("");
            println!(
                "  {}  {}  {}",
                meta.timestamp.format("%Y-%m-%d %H:%M:%S"),
                label,
                commit
            );
        }
        println!("\nTotal: {} snapshots", metas.len());
        return Ok(());
    }

    if args.save {
        // Use the same filter semantics as the main command so snapshot
        // numbers stay comparable with `codelens` output.
        let partial = load_partial_config(advanced)?;
        let config = resolve_config(&args.filter, &args.output, advanced, partial.as_ref());
        let result = analyze(&[&project_root], &config).context("Analysis failed")?;
        let (git_commit, git_branch) = GitClient::detect(&project_root)
            .and_then(|c| c.repo_info())
            .map(|info| (info.commit, info.branch))
            .unwrap_or((None, None));
        let path = trend::save_snapshot(
            &project_root,
            result,
            args.label.clone(),
            git_commit,
            git_branch,
        )?;
        println!("Snapshot saved to: {}", path.display().to_string().green());
        return Ok(());
    }

    let (from_ref, to_ref) = if let Some(ref refs) = args.compare {
        (refs[0].as_str(), refs[1].as_str())
    } else {
        ("latest~1", "latest")
    };

    let report = trend::diff(&project_root, from_ref, to_ref)?;
    let output_config = trend_output_config(args);
    write_report(Report::Trend(report), &output_config)
}

/// Trend's list/diff paths don't run an analysis; build output settings
/// directly from CLI args.
fn trend_output_config(args: &cli::TrendArgs) -> codelens_core::config::OutputConfig {
    let mut output = codelens_core::config::OutputConfig::default();
    if let Some(format) = args.output.format {
        output.format = format.into();
    }
    if let Some(ref path) = args.output.output_file {
        output.file = Some(path.clone());
    }
    if let Some(sort) = args.output.sort {
        output.sort_by = sort.into();
    }
    output.summary_only = args.output.summary;
    output.top_n = args.output.top;
    output.quiet = args.output.quiet;
    output
}

fn run_estimate(args: &cli::EstimateArgs, advanced: &cli::AdvancedArgs) -> Result<()> {
    let partial = load_partial_config(advanced)?;
    let config = resolve_config(&args.filter, &args.output, advanced, partial.as_ref());
    let result = analyze(&args.paths, &config).context("Analysis failed")?;

    let cost_config = codelens_core::CostConfig {
        average_wage: args.avg_wage,
        overhead: args.overhead,
    };

    if matches!(args.model, cli::ModelArg::All) {
        return run_estimate_all(&result.summary, args, &cost_config, &config.output);
    }

    let model: Box<dyn codelens_core::EstimationModel> = build_model(args);

    let report =
        codelens_core::insight::estimation::estimate(&result.summary, model.as_ref(), &cost_config);
    write_report(Report::Estimation(report), &config.output)
}

fn build_model(args: &cli::EstimateArgs) -> Box<dyn codelens_core::EstimationModel> {
    match args.model {
        cli::ModelArg::CocomoBasic | cli::ModelArg::All => {
            Box::new(codelens_core::CocomoBasicModel {
                project_type: args.project_type.into(),
                eaf: args.eaf,
            })
        }
        cli::ModelArg::Cocomo2 => {
            let mut m = codelens_core::CocomoIIModel::default();
            if let Some(sf) = args.sf_sum {
                let per_factor = sf / 5.0;
                m.scale_factors = [per_factor; 5];
            }
            m.eaf = args.eaf;
            Box::new(m)
        }
        cli::ModelArg::Putnam => Box::new(codelens_core::PutnamModel {
            ck: args.ck,
            d0: args.d0,
        }),
        cli::ModelArg::Locomo => Box::new(build_locomo_model(args)),
    }
}

/// Resolve LOCOMO pricing: preset base values (matching scc), overridden
/// by any explicitly passed --llm-* flag.
fn build_locomo_model(args: &cli::EstimateArgs) -> codelens_core::LocomoModel {
    let (base_in, base_out, base_tps) = match args.locomo_preset.unwrap_or_default() {
        cli::LocomoPresetArg::Large => (10.0, 30.0, 30.0),
        cli::LocomoPresetArg::Medium => (3.0, 15.0, 50.0),
        cli::LocomoPresetArg::Small => (0.5, 2.0, 100.0),
        cli::LocomoPresetArg::Local => (0.0, 0.0, 15.0),
    };
    codelens_core::LocomoModel {
        input_price_per_m: args.llm_input_price.unwrap_or(base_in),
        output_price_per_m: args.llm_output_price.unwrap_or(base_out),
        tokens_per_second: args.llm_tps.unwrap_or(base_tps),
        ..Default::default()
    }
}

fn run_estimate_all(
    summary: &codelens_core::Summary,
    args: &cli::EstimateArgs,
    cost_config: &codelens_core::CostConfig,
    output: &codelens_core::config::OutputConfig,
) -> Result<()> {
    let cocomo_basic = codelens_core::CocomoBasicModel {
        project_type: args.project_type.into(),
        eaf: args.eaf,
    };
    let mut cocomo2 = codelens_core::CocomoIIModel::default();
    if let Some(sf) = args.sf_sum {
        cocomo2.scale_factors = [sf / 5.0; 5];
    }
    cocomo2.eaf = args.eaf;
    let putnam = codelens_core::PutnamModel {
        ck: args.ck,
        d0: args.d0,
    };
    let locomo = build_locomo_model(args);

    let models: Vec<&dyn codelens_core::EstimationModel> =
        vec![&cocomo_basic, &cocomo2, &putnam, &locomo];
    let comparison =
        codelens_core::insight::estimation::estimate_all(summary, &models, cost_config);
    write_report(Report::EstimationComparison(comparison), output)
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

fn write_report(report: Report, output: &codelens_core::config::OutputConfig) -> Result<()> {
    let output_options = OutputOptions {
        summary_only: output.summary_only,
        by_file: output.by_file,
        sort_by: output.sort_by,
        top_n: output.top_n,
        colorize: should_colorize(output),
        show_git_info: false,
    };
    let formatter = create_output(output.format);

    // Quiet suppresses terminal output only; an explicit -O file is still written
    if output.quiet && output.file.is_none() {
        return Ok(());
    }

    if let Some(ref path) = output.file {
        let file = File::create(path).context("Failed to create output file")?;
        let mut writer = BufWriter::new(file);
        formatter.write(&report, &output_options, &mut writer)?;
        writer.flush()?;
        if !output.quiet {
            println!("Output written to: {}", path.display().to_string().green());
        }
    } else {
        let stdout = io::stdout();
        let mut writer = stdout.lock();
        formatter.write(&report, &output_options, &mut writer)?;
    }
    Ok(())
}

impl From<OutputFormatArg> for codelens_core::config::OutputFormatType {
    fn from(arg: OutputFormatArg) -> Self {
        match arg {
            OutputFormatArg::Console => Self::Console,
            OutputFormatArg::Json => Self::Json,
            OutputFormatArg::Csv => Self::Csv,
            OutputFormatArg::Markdown => Self::Markdown,
            OutputFormatArg::Html => Self::Html,
            OutputFormatArg::Openmetrics => Self::OpenMetrics,
            OutputFormatArg::Badge => Self::Badge,
        }
    }
}

impl From<SortByArg> for codelens_core::config::SortBy {
    fn from(arg: SortByArg) -> Self {
        match arg {
            SortByArg::Lines => Self::Lines,
            SortByArg::Files => Self::Files,
            SortByArg::Code => Self::Code,
            SortByArg::Name => Self::Name,
            SortByArg::Size => Self::Size,
        }
    }
}

impl From<cli::ProjectTypeArg> for codelens_core::ProjectType {
    fn from(arg: cli::ProjectTypeArg) -> Self {
        match arg {
            cli::ProjectTypeArg::Organic => Self::Organic,
            cli::ProjectTypeArg::SemiDetached => Self::SemiDetached,
            cli::ProjectTypeArg::Embedded => Self::Embedded,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use codelens_core::config::OutputFormatType;

    fn parse(args: &[&str]) -> Cli {
        Cli::try_parse_from(args).unwrap()
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
    fn fail_under_accepts_grades_and_scores() {
        assert_eq!(parse_fail_under("A").unwrap(), 90.0);
        assert_eq!(parse_fail_under("b").unwrap(), 80.0);
        assert_eq!(parse_fail_under("C").unwrap(), 70.0);
        assert_eq!(parse_fail_under("D").unwrap(), 60.0);
        assert_eq!(parse_fail_under("75").unwrap(), 75.0);
        assert_eq!(parse_fail_under("62.5").unwrap(), 62.5);

        // F would always pass — reject it along with garbage and out-of-range
        assert!(parse_fail_under("F").is_err());
        assert!(parse_fail_under("great").is_err());
        assert!(parse_fail_under("101").is_err());
        assert!(parse_fail_under("-1").is_err());
    }

    #[test]
    fn defaults_apply_without_config_file() {
        let cli = parse(&["codelens"]);
        let config = resolve_config(&cli.filter, &cli.output, &cli.advanced, None);

        assert_eq!(config.output.format, OutputFormatType::Console);
        assert!(config.filter.languages.is_empty());
        assert!(config.filter.smart_exclude);
        assert!(config.walker.use_gitignore);
    }
}
