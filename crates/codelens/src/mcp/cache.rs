//! File-system state tracking for reusable MCP analysis results.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::SystemTime;

use anyhow::Context;
use codelens_core::filter::{Filter, FilterChain};
use codelens_core::{analyze, Config};

#[derive(Debug, Clone, PartialEq, Eq)]
struct WorkspaceEntryState {
    path: PathBuf,
    kind: u8,
    len: u64,
    modified: Option<SystemTime>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct WorkspaceState {
    entries: Vec<WorkspaceEntryState>,
}

impl WorkspaceState {
    fn capture(root: &Path, config: &Config) -> std::io::Result<Self> {
        let filter = FilterChain::new(config).map_err(std::io::Error::other)?;
        let mut entries = Vec::new();
        let mut pending = vec![root.to_path_buf()];

        while let Some(path) = pending.pop() {
            let metadata = fs::symlink_metadata(&path)?;
            let file_type = metadata.file_type();
            entries.push(WorkspaceEntryState {
                path: path.clone(),
                kind: if file_type.is_dir() {
                    0
                } else if file_type.is_file() {
                    1
                } else {
                    2
                },
                len: metadata.len(),
                modified: metadata.modified().ok(),
            });

            if !file_type.is_dir() {
                continue;
            }

            let mut children = fs::read_dir(&path)?.collect::<std::io::Result<Vec<_>>>()?;
            children.sort_by_key(|entry| entry.path());
            for child in children.into_iter().rev() {
                let child_path = child.path();
                let child_type = child.file_type()?;
                if child_type.is_dir() && !filter.should_include(&child_path, true) {
                    continue;
                }
                pending.push(child_path);
            }
        }

        entries.sort_by(|left, right| left.path.cmp(&right.path));
        Ok(Self { entries })
    }
}

struct CachedAnalysis {
    state: WorkspaceState,
    result: Arc<codelens_core::AnalysisResult>,
}

#[derive(Default)]
pub(super) struct AnalysisCache {
    entries: HashMap<PathBuf, CachedAnalysis>,
}

impl AnalysisCache {
    const MAX_ENTRIES: usize = 8;

    pub(super) fn analysis(
        &mut self,
        path: &Path,
        config: &Config,
    ) -> anyhow::Result<Arc<codelens_core::AnalysisResult>> {
        let key = fs::canonicalize(path)
            .with_context(|| format!("analysis path does not exist: {}", path.display()))?;
        let before = WorkspaceState::capture(&key, config).ok();

        if let (Some(state), Some(cached)) = (before.as_ref(), self.entries.get(&key)) {
            if &cached.state == state {
                return Ok(Arc::clone(&cached.result));
            }
        }
        self.entries.remove(&key);

        let result = Arc::new(analyze(std::slice::from_ref(&key), config)?);
        let after = WorkspaceState::capture(&key, config).ok();
        if let (Some(before), Some(after)) = (before, after) {
            if before == after {
                if self.entries.len() >= Self::MAX_ENTRIES {
                    if let Some(oldest) = self.entries.keys().next().cloned() {
                        self.entries.remove(&oldest);
                    }
                }
                self.entries.insert(
                    key,
                    CachedAnalysis {
                        state: after,
                        result: Arc::clone(&result),
                    },
                );
            }
        }
        Ok(result)
    }

    #[cfg(test)]
    pub(super) fn len(&self) -> usize {
        self.entries.len()
    }
}
