//! Directory-level statistics derived from file statistics.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::Serialize;

use super::{FileStats, LineStats};

/// Cumulative statistics for one directory (includes all subdirectories).
#[derive(Debug, Clone, Serialize)]
pub struct DirStats {
    /// Directory path ("." for files at the walk root).
    pub path: PathBuf,
    /// Component depth, 1-based ("." and top-level dirs are 1).
    pub depth: usize,
    pub files: usize,
    pub lines: LineStats,
    pub size: u64,
    pub cyclomatic: usize,
    pub functions: usize,
}

/// Longest directory prefix shared by every file, used to anchor the
/// tree when files carry absolute paths (a `codelens /abs/dir` run) —
/// without it the rollup would emit meaningless "/", "/Users", ... rows.
fn common_dir_prefix(files: &[FileStats]) -> PathBuf {
    let mut iter = files.iter().filter(|file| file.path.is_absolute());
    let Some(first) = iter.next() else {
        return PathBuf::new();
    };
    let mut prefix: Vec<std::path::Component> = first
        .path
        .parent()
        .map(|path| path.components().collect())
        .unwrap_or_default();
    for file in iter {
        let parent: Vec<std::path::Component> = file
            .path
            .parent()
            .map(|path| path.components().collect())
            .unwrap_or_default();
        let shared = prefix
            .iter()
            .zip(parent.iter())
            .take_while(|(left, right)| left == right)
            .count();
        prefix.truncate(shared);
    }
    prefix.iter().collect()
}

/// Aggregate per-file statistics into a directory tree, cumulative per
/// directory, at most `max_depth` components deep (deeper files still
/// roll up into their visible ancestors). Rows come back in tree order
/// (parents first, siblings sorted by code lines descending).
pub fn aggregate_by_dir(files: &[FileStats], max_depth: usize) -> Vec<DirStats> {
    let max_depth = max_depth.max(1);
    let mut map: HashMap<PathBuf, DirStats> = HashMap::new();
    let abs_prefix = common_dir_prefix(files);

    for file in files {
        let path = file.path.strip_prefix("./").unwrap_or(&file.path);
        let path = path.strip_prefix(&abs_prefix).unwrap_or(path);
        let parent = path.parent().unwrap_or_else(|| Path::new(""));

        let mut dirs = Vec::new();
        let mut accumulated = PathBuf::new();
        for component in parent.components() {
            accumulated.push(component);
            dirs.push(accumulated.clone());
            if dirs.len() == max_depth {
                break;
            }
        }
        if dirs.is_empty() {
            dirs.push(PathBuf::from("."));
        }

        for (index, dir) in dirs.into_iter().enumerate() {
            let entry = map.entry(dir.clone()).or_insert_with(|| DirStats {
                path: dir,
                depth: index + 1,
                files: 0,
                lines: LineStats::default(),
                size: 0,
                cyclomatic: 0,
                functions: 0,
            });
            entry.files += 1;
            entry.lines.total += file.lines.total;
            entry.lines.code += file.lines.code;
            entry.lines.comment += file.lines.comment;
            entry.lines.blank += file.lines.blank;
            entry.size += file.size;
            entry.cyclomatic += file.complexity.cyclomatic;
            entry.functions += file.complexity.functions;
        }
    }

    // Emit in tree order: group children under parents, siblings by code desc.
    let mut children: HashMap<Option<PathBuf>, Vec<PathBuf>> = HashMap::new();
    for dir in map.keys() {
        let parent = dir
            .parent()
            .filter(|path| !path.as_os_str().is_empty() && map.contains_key(*path))
            .map(Path::to_path_buf);
        children.entry(parent).or_default().push(dir.clone());
    }
    for list in children.values_mut() {
        list.sort_by_key(|dir| std::cmp::Reverse(map[dir].lines.code));
    }

    let mut ordered = Vec::with_capacity(map.len());
    let mut stack = children.remove(&None).unwrap_or_default();
    stack.reverse();
    while let Some(dir) = stack.pop() {
        if let Some(mut descendants) = children.remove(&Some(dir.clone())) {
            descendants.reverse();
            stack.extend(descendants);
        }
        ordered.push(map.remove(&dir).expect("dir queued exactly once"));
    }
    ordered
}
