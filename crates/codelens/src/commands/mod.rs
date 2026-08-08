//! Subcommand implementations and the shared CLI plumbing they build on:
//! config loading/merging, report writing, and git path normalization.

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

use anyhow::{Context, Result};
use colored::Colorize;

use codelens_core::config::{Config, PartialConfig};
use codelens_core::git::GitClient;
use codelens_core::insight::scoring::default::DefaultModel;
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
    let scoring_model = scoring_model_for(result.summary.dup_scanned);
    let health_top_n = config.output.top_n.unwrap_or(10);
    let health_report =
        codelens_core::insight::health::score(&result, &scoring_model, health_top_n);

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

    write_report(report, &config.output)?;
    Ok(ExitCode::SUCCESS)
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
    if advanced.no_dup_scan {
        config.no_dup_scan = true;
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

fn build_config(cli: &Cli) -> Result<Config> {
    let partial = load_partial_config(&cli.advanced)?;
    Ok(resolve_config(
        &cli.filter,
        &cli.output,
        &cli.advanced,
        partial.as_ref(),
    ))
}

/// Materialize `reference` in a temporary worktree and analyze the same
/// locations there; file paths come back repo-root-relative.
fn analyze_at_git_ref(
    git_client: &GitClient,
    reference: &str,
    paths: &[PathBuf],
    config: &Config,
) -> Result<codelens_core::AnalysisResult> {
    if !git_client.rev_exists(reference) {
        anyhow::bail!("'{reference}' is not a git ref in this repository");
    }

    let worktree = git_client.temp_worktree(reference)?;
    // A path that does not exist in that tree must be a hard error:
    // falling back to the whole tree would compare mismatched scopes and
    // fabricate a project-level "regression" with zero regressed files.
    let repo_root = std::fs::canonicalize(git_client.repo_path())
        .unwrap_or_else(|_| git_client.repo_path().to_path_buf());
    let mut mapped: Vec<PathBuf> = Vec::with_capacity(paths.len());
    for p in paths {
        let inside = std::fs::canonicalize(p).ok().and_then(|abs| {
            abs.strip_prefix(&repo_root)
                .ok()
                .map(|rel| worktree.path().join(rel))
        });
        match inside.filter(|m| m.exists()) {
            Some(m) => mapped.push(m),
            None => anyhow::bail!(
                "path '{}' does not exist in '{reference}'; \
                 compare a path that exists in both trees",
                p.display()
            ),
        }
    }
    let mut result = analyze(&mapped, config).context("Analysis of the git ref failed")?;
    rewrite_paths_repo_relative(&mut result.files, worktree.path());
    Ok(result)
}

/// Drop files living inside git submodules and rebuild the summary.
/// Ref comparisons need this symmetrically on both sides: temporary
/// worktrees never materialize submodules, the real tree does.
fn drop_submodule_files(result: &mut codelens_core::AnalysisResult, submodules: &[PathBuf]) {
    if submodules.is_empty() {
        return;
    }
    result.files.retain(|f| {
        let p = f.path.strip_prefix("./").unwrap_or(&f.path);
        !submodules.iter().any(|s| p.starts_with(s))
    });
    // Rebuilding from file stats cannot know whether duplication was
    // collected — carry the flag over or measured trees would silently
    // read as unmeasured downstream. ULOC must survive the rebuild too;
    // the pre-filter value is an approximation (lines of the dropped
    // submodule files are still counted in it), but losing it entirely
    // would zero the Duplication dimension's input.
    let dup_scanned = result.summary.dup_scanned;
    let uloc = result.summary.uloc;
    result.summary = codelens_core::Summary::from_file_stats(&result.files);
    result.summary.dup_scanned = dup_scanned;
    result.summary.uloc = uloc;
}

/// Scoring model matching an analysis: when duplication was not
/// collected the Duplication dimension is excluded (its weight
/// redistributed) instead of scoring the absent data as clean.
pub(crate) fn scoring_model_for(dup_scanned: bool) -> DefaultModel {
    if dup_scanned {
        DefaultModel::new()
    } else {
        DefaultModel::without_duplication()
    }
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
    let output_options = report_output_options(output);
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

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;
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
        let mut result = codelens_core::AnalysisResult {
            files,
            summary,
            elapsed: std::time::Duration::from_millis(1),
            scanned_files: 2,
            skipped_files: 0,
            error_files: 0,
        };

        drop_submodule_files(&mut result, &[PathBuf::from("vendor/sub")]);
        assert_eq!(result.files.len(), 1);
        assert!(
            !result.summary.dup_scanned,
            "summary rebuild must not resurrect the flag"
        );

        // And the measured state survives too.
        result.summary.dup_scanned = true;
        drop_submodule_files(&mut result, &[PathBuf::from("other")]);
        assert!(result.summary.dup_scanned);
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

        drop_submodule_files(&mut result, &[PathBuf::from("vendor/sub")]);
        assert_eq!(
            result.summary.uloc, 42,
            "summary rebuild must not zero out the measured ULOC"
        );
    }

    #[test]
    fn scoring_model_drops_duplication_when_not_scanned() {
        use codelens_core::insight::scoring::{HealthDimension, ScoringModel};

        let with_dup = scoring_model_for(true);
        assert!(with_dup
            .dimensions()
            .iter()
            .any(|d| d.dimension == HealthDimension::Duplication));

        let without_dup = scoring_model_for(false);
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
    fn defaults_apply_without_config_file() {
        let cli = parse(&["codelens"]);
        let config = resolve_config(&cli.filter, &cli.output, &cli.advanced, None);

        assert_eq!(config.output.format, OutputFormatType::Console);
        assert!(config.filter.languages.is_empty());
        assert!(config.filter.smart_exclude);
        assert!(config.walker.use_gitignore);
    }
}
