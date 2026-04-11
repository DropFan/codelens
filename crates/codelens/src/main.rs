//! Codelens CLI - High performance code analysis tool.

mod cli;

use std::fs::File;
use std::io::{self, BufWriter, Write};
use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::{Context, Result};
use clap::Parser;
use colored::Colorize;
use tracing_subscriber::EnvFilter;

use codelens_core::config::Config;
use codelens_core::git::{self, GitClient};
use codelens_core::insight::scoring::default::DefaultModel;
use codelens_core::insight::{health, hotspot, trend};
use codelens_core::output::{create_output, OutputOptions, Report};
use codelens_core::{analyze, LanguageRegistry};

use crate::cli::{Cli, OutputFormatArg, SortByArg};

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("{}: {e:#}", "error".red().bold());
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<()> {
    let cli = Cli::parse();

    // Initialize logging
    init_tracing(cli.output.verbose);

    // Handle special commands
    if cli.advanced.list_languages {
        return list_languages();
    }

    // Handle subcommands
    if let Some(ref command) = cli.command {
        return match command {
            cli::Command::Health(args) => run_health(args),
            cli::Command::Hotspot(args) => run_hotspot(args),
            cli::Command::Trend(args) => run_trend(args),
            cli::Command::Estimate(args) => run_estimate(args),
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

    // Prepare output options
    let output_options = OutputOptions {
        summary_only: cli.output.summary,
        sort_by: cli.output.sort.into(),
        top_n: cli.output.top,
        colorize: !cli.output.quiet,
        show_git_info: cli.advanced.git_info,
    };

    // Get output formatter
    let formatter = create_output(cli.output.format.into());

    // Write output
    if cli.output.quiet {
        return Ok(());
    }

    let report = Report::Analysis(result);

    // Collect all reports to write: analysis + health + estimation comparison
    let mut reports = vec![report.clone()];

    if let Report::Analysis(ref analysis) = report {
        // Health
        let scoring_model = DefaultModel::new();
        let health_top_n = cli.output.top.unwrap_or(10);
        let health_report = health::score(analysis, &scoring_model, health_top_n);
        reports.push(Report::Health(health_report));

        // Estimation comparison (all models)
        let cost_config = codelens_core::CostConfig::default();
        let cocomo_basic = codelens_core::CocomoBasicModel::default();
        let cocomo2 = codelens_core::CocomoIIModel::default();
        let putnam = codelens_core::PutnamModel::default();
        let locomo = codelens_core::LocomoModel::default();
        let models: Vec<&dyn codelens_core::EstimationModel> =
            vec![&cocomo_basic, &cocomo2, &putnam, &locomo];
        let comparison = codelens_core::insight::estimation::estimate_all(
            &analysis.summary,
            &models,
            &cost_config,
        );
        reports.push(Report::EstimationComparison(comparison));
    }

    if let Some(ref path) = cli.output.output_file {
        let file = File::create(path).context("Failed to create output file")?;
        let mut writer = BufWriter::new(file);
        for r in &reports {
            formatter.write(r, &output_options, &mut writer)?;
        }
        writer.flush()?;
        println!("Output written to: {}", path.display().to_string().green());
    } else {
        let stdout = io::stdout();
        let mut writer = stdout.lock();
        for r in &reports {
            formatter.write(r, &output_options, &mut writer)?;
        }
    }

    Ok(())
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

fn build_config(cli: &Cli) -> Result<Config> {
    use codelens_core::config::{FilterConfig, OutputConfig};
    use codelens_core::walker::WalkerConfig;

    // Load config file if specified
    let base_config = if !cli.advanced.no_config {
        if let Some(ref path) = cli.advanced.config {
            Some(codelens_core::config::load_config_file(path)?)
        } else {
            // Try to load default config files
            let default_paths = [".codelens.toml", ".code_stats.yaml", ".code_stats.yml"];
            default_paths.iter().find_map(|p| {
                let path = PathBuf::from(p);
                if path.exists() {
                    codelens_core::config::load_config_file(&path).ok()
                } else {
                    None
                }
            })
        }
    } else {
        None
    };

    let mut config = base_config.unwrap_or_default();

    // Override with CLI arguments
    config.walker = WalkerConfig {
        threads: cli.advanced.threads.unwrap_or_else(num_cpus::get),
        use_gitignore: !cli.filter.no_gitignore,
        max_depth: cli.filter.depth,
        ..config.walker
    };

    config.filter = FilterConfig {
        excludes: cli.filter.exclude.clone().unwrap_or_default(),
        exclude_files: cli
            .filter
            .exclude_files
            .as_ref()
            .map(|s| vec![s.clone()])
            .unwrap_or_default(),
        include_files: cli
            .filter
            .include_files
            .as_ref()
            .map(|s| vec![s.clone()])
            .unwrap_or_default(),
        languages: cli.filter.lang.clone().unwrap_or_default(),
        min_lines: cli.filter.min_lines,
        max_lines: cli.filter.max_lines,
        smart_exclude: !cli.filter.no_smart_exclude,
        include_all: cli.filter.all,
        ..config.filter
    };

    config.output = OutputConfig {
        format: cli.output.format.into(),
        file: cli.output.output_file.clone(),
        summary_only: cli.output.summary,
        sort_by: cli.output.sort.into(),
        top_n: cli.output.top,
        verbose: cli.output.verbose,
        quiet: cli.output.quiet,
        show_git_info: cli.advanced.git_info,
    };

    Ok(config)
}

fn run_health(args: &cli::HealthArgs) -> Result<()> {
    let config = build_config_from_args(&args.filter, &args.output)?;
    let result = analyze(&args.paths, &config).context("Analysis failed")?;
    let model = DefaultModel::new();
    let top_n = args.output.top.unwrap_or(10);
    let report = health::score(&result, &model, top_n);
    write_report(Report::Health(report), &args.output)
}

fn run_hotspot(args: &cli::HotspotArgs) -> Result<()> {
    let config = build_config_from_args(&args.filter, &args.output)?;
    let git_client = GitClient::detect(&args.paths[0]).context("Not a git repository")?;
    let since = git::parse_since(&args.since);
    let result = analyze(&args.paths, &config).context("Analysis failed")?;
    let churns = git_client.file_churn(&since)?;
    let total_commits = git_client.commit_count(&since)?;
    let top_n = args.output.top.unwrap_or(20);
    let report = hotspot::analyze(&churns, &result, &args.since, total_commits, top_n);
    write_report(Report::Hotspot(report), &args.output)
}

fn run_trend(args: &cli::TrendArgs) -> Result<()> {
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
        let config = Config::default();
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
    write_report(Report::Trend(report), &args.output)
}

fn run_estimate(args: &cli::EstimateArgs) -> Result<()> {
    let config = build_config_from_args(&args.filter, &args.output)?;
    let result = analyze(&args.paths, &config).context("Analysis failed")?;

    let cost_config = codelens_core::CostConfig {
        average_wage: args.avg_wage,
        overhead: args.overhead,
    };

    if matches!(args.model, cli::ModelArg::All) {
        return run_estimate_all(&result.summary, args, &cost_config);
    }

    let model: Box<dyn codelens_core::EstimationModel> = build_model(args);

    let report =
        codelens_core::insight::estimation::estimate(&result.summary, model.as_ref(), &cost_config);
    write_report(Report::Estimation(report), &args.output)
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
        cli::ModelArg::Locomo => Box::new(codelens_core::LocomoModel {
            input_price_per_m: args.llm_input_price,
            output_price_per_m: args.llm_output_price,
            tokens_per_second: args.llm_tps,
            ..Default::default()
        }),
    }
}

fn run_estimate_all(
    summary: &codelens_core::Summary,
    args: &cli::EstimateArgs,
    cost_config: &codelens_core::CostConfig,
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
    let locomo = codelens_core::LocomoModel {
        input_price_per_m: args.llm_input_price,
        output_price_per_m: args.llm_output_price,
        tokens_per_second: args.llm_tps,
        ..Default::default()
    };

    let models: Vec<&dyn codelens_core::EstimationModel> =
        vec![&cocomo_basic, &cocomo2, &putnam, &locomo];
    let comparison =
        codelens_core::insight::estimation::estimate_all(summary, &models, cost_config);
    write_report(Report::EstimationComparison(comparison), &args.output)
}

fn write_report(report: Report, output_args: &cli::OutputArgs) -> Result<()> {
    let output_options = OutputOptions {
        summary_only: output_args.summary,
        sort_by: output_args.sort.into(),
        top_n: output_args.top,
        colorize: !output_args.quiet,
        show_git_info: false,
    };
    let formatter = create_output(output_args.format.into());

    if output_args.quiet {
        return Ok(());
    }

    if let Some(ref path) = output_args.output_file {
        let file = File::create(path).context("Failed to create output file")?;
        let mut writer = BufWriter::new(file);
        formatter.write(&report, &output_options, &mut writer)?;
        writer.flush()?;
        println!("Output written to: {}", path.display().to_string().green());
    } else {
        let stdout = io::stdout();
        let mut writer = stdout.lock();
        formatter.write(&report, &output_options, &mut writer)?;
    }
    Ok(())
}

fn build_config_from_args(filter: &cli::FilterArgs, _output: &cli::OutputArgs) -> Result<Config> {
    use codelens_core::config::FilterConfig;
    use codelens_core::walker::WalkerConfig;

    Ok(Config {
        walker: WalkerConfig {
            threads: num_cpus::get(),
            use_gitignore: !filter.no_gitignore,
            max_depth: filter.depth,
            ..WalkerConfig::default()
        },
        filter: FilterConfig {
            excludes: filter.exclude.clone().unwrap_or_default(),
            languages: filter.lang.clone().unwrap_or_default(),
            smart_exclude: !filter.no_smart_exclude,
            include_all: filter.all,
            ..FilterConfig::default()
        },
        ..Config::default()
    })
}

impl From<OutputFormatArg> for codelens_core::config::OutputFormatType {
    fn from(arg: OutputFormatArg) -> Self {
        match arg {
            OutputFormatArg::Console => Self::Console,
            OutputFormatArg::Json => Self::Json,
            OutputFormatArg::Csv => Self::Csv,
            OutputFormatArg::Markdown => Self::Markdown,
            OutputFormatArg::Html => Self::Html,
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
