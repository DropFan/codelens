//! CLI configuration loading and precedence rules.

use std::path::PathBuf;

use anyhow::Result;
use codelens_core::config::{Config, PartialConfig};

use crate::cli::{self, Cli};

/// Load the config file specified by `--config`, or search default locations.
///
/// A config file that exists but fails to parse is a hard error even when
/// found through the default search path.
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

/// Merge built-in defaults, config-file values, then explicit CLI arguments.
pub(crate) fn resolve_config(
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

/// Parse `jsp:html,tpl:php` into extension/language pairs.
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
