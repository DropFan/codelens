//! Codelens CLI - High performance code analysis tool.

mod cli;
mod commands;
#[cfg(feature = "mcp")]
mod mcp;

use std::process::ExitCode;

use anyhow::Result;
use clap::Parser;
use colored::Colorize;
use tracing_subscriber::EnvFilter;

use crate::cli::Cli;

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
        return commands::list_languages().map(|()| ExitCode::SUCCESS);
    }

    // Handle subcommands (they share config loading with the main command)
    if let Some(ref command) = cli.command {
        return match command {
            // health owns its exit code: --fail-under can gate CI
            cli::Command::Health(args) => commands::health::run_health(args, &cli.advanced),
            cli::Command::Hotspot(args) => {
                commands::hotspot::run_hotspot(args, &cli.advanced).map(|()| ExitCode::SUCCESS)
            }
            cli::Command::Coupling(args) => {
                commands::coupling::run_coupling(args, &cli.advanced).map(|()| ExitCode::SUCCESS)
            }
            // diff owns its exit code: --fail-on-regression can gate CI
            cli::Command::Diff(args) => commands::diff::run_diff(args, &cli.advanced),
            cli::Command::Trend(args) => {
                commands::trend::run_trend(args, &cli.advanced).map(|()| ExitCode::SUCCESS)
            }
            #[cfg(feature = "mcp")]
            cli::Command::Mcp => mcp::run().map(|()| ExitCode::SUCCESS),
            cli::Command::Estimate(args) => {
                commands::estimate::run_estimate(args, &cli.advanced).map(|()| ExitCode::SUCCESS)
            }
        };
    }

    commands::run_default(&cli)
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
