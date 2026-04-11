//! Codelens CLI - High performance code statistics tool.

mod cli;

use std::fs::File;
use std::io::{self, BufWriter, Write};
use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::{Context, Result};
use clap::Parser;
use colored::Colorize;
use tracing_subscriber::EnvFilter;

use codelens_core::output::{create_output, OutputOptions, Report};
use codelens_core::{analyze, Config, LanguageRegistry};

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

    if let Some(ref path) = cli.output.output_file {
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
