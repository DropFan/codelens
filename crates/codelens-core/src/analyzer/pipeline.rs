//! End-to-end source analysis pipeline.

use std::collections::HashSet;
use std::path::Path;
use std::sync::Arc;
use std::time::Instant;

use crate::config::Config;
use crate::error::{Error, Result};
use crate::filter::{Filter, FilterChain};
use crate::language::LinguistAttributes;
use crate::walker::{ParallelWalker, SkipReason};

use super::duplication::DuplicationSink;
use super::stats::{AnalysisResult, FileStats, Summary};
use super::test_code::InlineTestSink;
use super::FileAnalyzer;

pub(crate) fn analyze_paths<P: AsRef<Path>>(
    paths: &[P],
    config: &Config,
) -> Result<AnalysisResult> {
    let start = Instant::now();
    let registry = Arc::new(crate::build_registry(config)?);
    let duplication = (!config.no_dup_scan).then(|| Arc::new(DuplicationSink::default()));
    let inline_tests = Arc::new(InlineTestSink::default());
    let analyzer = build_file_analyzer(paths, config, registry, &duplication, &inline_tests)?;
    let filter: Arc<dyn Filter> = Arc::new(FilterChain::new(config)?);
    let walker = ParallelWalker::new(config.walker.clone());
    let (mut files, scanned_files, skipped_files, error_files) =
        collect_files(paths, analyzer, filter, &walker)?;
    let mut summary = finalize_summary(&mut files, duplication, inline_tests);
    summary.sort_languages(config.output.sort_by);

    Ok(AnalysisResult {
        files,
        summary,
        elapsed: start.elapsed(),
        scanned_files,
        skipped_files,
        error_files,
    })
}

fn build_file_analyzer<P: AsRef<Path>>(
    paths: &[P],
    config: &Config,
    registry: Arc<crate::LanguageRegistry>,
    duplication: &Option<Arc<DuplicationSink>>,
    inline_tests: &Arc<InlineTestSink>,
) -> Result<Arc<FileAnalyzer>> {
    let mut analyzer =
        FileAnalyzer::new(registry, config).with_inline_test_sink(Arc::clone(inline_tests));
    if let Some(sink) = duplication {
        analyzer = analyzer.with_duplication_sink(Arc::clone(sink));
    }
    if !config.filter.no_linguist {
        if let Some(root) = paths.first().map(|path| path.as_ref()) {
            let dir = if root.is_dir() {
                root
            } else {
                root.parent().unwrap_or_else(|| Path::new("."))
            };
            if let Some(attributes) = LinguistAttributes::load(dir) {
                analyzer = analyzer.with_linguist(Arc::new(attributes));
            }
        }
    }
    Ok(Arc::new(analyzer))
}

fn collect_files<P: AsRef<Path>>(
    paths: &[P],
    analyzer: Arc<FileAnalyzer>,
    filter: Arc<dyn Filter>,
    walker: &ParallelWalker,
) -> Result<(Vec<FileStats>, usize, usize, usize)> {
    let mut files = Vec::new();
    let mut seen_paths = HashSet::new();
    let mut scanned_files = 0;
    let mut skipped_files = 0;
    let mut error_files = 0;

    for path in paths {
        let path = path.as_ref();
        if !path.exists() {
            return Err(Error::DirectoryNotFound {
                path: path.to_path_buf(),
            });
        }
        walker.walk_and_analyze(
            path,
            Arc::clone(&analyzer),
            Arc::clone(&filter),
            |stats| {
                if seen_paths.insert(stats.path.clone()) {
                    files.push(stats);
                    scanned_files += 1;
                }
            },
            |_, reason| match reason {
                SkipReason::Filtered => skipped_files += 1,
                SkipReason::Error => error_files += 1,
            },
        )?;
    }

    Ok((files, scanned_files, skipped_files, error_files))
}

fn finalize_summary(
    files: &mut [FileStats],
    duplication: Option<Arc<DuplicationSink>>,
    inline_tests: Arc<InlineTestSink>,
) -> Summary {
    let mut uloc = 0;
    let mut health_regions = inline_tests.finish();
    if let Some(sink) = &duplication {
        let (unique, mut per_file_dup, per_region_dup) = sink.finish_with_regions();
        uloc = unique;
        for stats in files.iter_mut() {
            if let Some(duplicated) = per_file_dup.remove(&stats.path) {
                stats.duplicate_lines = duplicated;
            }
        }
        for (path, duplicated) in per_region_dup {
            if let Some(regions) = health_regions.get_mut(&path) {
                regions.production.duplicate_lines = duplicated.production;
                regions.tests.duplicate_lines = duplicated.tests;
            }
        }
    }

    let mut summary = Summary::from_file_stats(files);
    summary.uloc = uloc;
    summary.dup_scanned = duplication.is_some();
    summary.health_regions = health_regions;
    summary.inline_test_regions_scanned = true;
    summary
}
