//! Statistics data structures.

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;

/// Statistics for a single file.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FileStats {
    /// File path.
    pub path: PathBuf,
    /// Detected language name.
    pub language: String,
    /// Line statistics.
    pub lines: LineStats,
    /// File size in bytes.
    pub size: u64,
    /// Complexity metrics.
    pub complexity: Complexity,
}

/// Line count statistics.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct LineStats {
    /// Total number of lines.
    pub total: usize,
    /// Number of code lines (non-blank, non-comment).
    pub code: usize,
    /// Number of comment lines.
    pub comment: usize,
    /// Number of blank lines.
    pub blank: usize,
}

impl LineStats {
    /// Create a new LineStats with all zeros.
    pub fn new() -> Self {
        Self::default()
    }

    /// Add another LineStats to this one.
    pub fn add(&mut self, other: &LineStats) {
        self.total += other.total;
        self.code += other.code;
        self.comment += other.comment;
        self.blank += other.blank;
    }
}

impl std::ops::Add for LineStats {
    type Output = Self;

    fn add(self, other: Self) -> Self {
        Self {
            total: self.total + other.total,
            code: self.code + other.code,
            comment: self.comment + other.comment,
            blank: self.blank + other.blank,
        }
    }
}

impl std::ops::AddAssign for LineStats {
    fn add_assign(&mut self, other: Self) {
        self.add(&other);
    }
}

/// Code complexity metrics.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Complexity {
    /// Number of functions/methods.
    pub functions: usize,
    /// Total cyclomatic complexity.
    pub cyclomatic: usize,
    /// Maximum nesting depth.
    pub max_depth: usize,
    /// Average lines per function.
    pub avg_func_lines: f64,
}

impl Complexity {
    /// Add another Complexity to this one.
    pub fn add(&mut self, other: &Complexity) {
        self.functions += other.functions;
        self.cyclomatic += other.cyclomatic;
        self.max_depth = self.max_depth.max(other.max_depth);
    }
}

/// File size distribution buckets.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SizeDistribution {
    /// Files < 1KB
    pub tiny: usize,
    /// Files 1KB - 10KB
    pub small: usize,
    /// Files 10KB - 100KB
    pub medium: usize,
    /// Files 100KB - 1MB
    pub large: usize,
    /// Files > 1MB
    pub huge: usize,
}

impl SizeDistribution {
    /// Add a file size to the distribution.
    pub fn add(&mut self, size: u64) {
        match size {
            s if s < 1024 => self.tiny += 1,
            s if s < 10 * 1024 => self.small += 1,
            s if s < 100 * 1024 => self.medium += 1,
            s if s < 1024 * 1024 => self.large += 1,
            _ => self.huge += 1,
        }
    }
}

/// Statistics grouped by language.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LanguageSummary {
    /// Number of files.
    pub files: usize,
    /// Line statistics.
    pub lines: LineStats,
    /// Total size in bytes.
    pub size: u64,
    /// Complexity metrics.
    pub complexity: Complexity,
    /// Estimated LLM tokens (rule-of-thumb, see `analyzer::tokens`).
    /// `serde(default)` keeps snapshots from older versions loadable.
    #[serde(default)]
    pub tokens_est: u64,
}

/// Repository statistics.
#[derive(Debug, Clone, Serialize)]
pub struct RepoStats {
    /// Repository name.
    pub name: String,
    /// Repository path.
    pub path: PathBuf,
    /// Primary language (by code lines).
    pub primary_language: String,
    /// All file statistics.
    pub files: Vec<FileStats>,
    /// Summary statistics.
    pub summary: RepoSummary,
    /// Statistics by language.
    pub by_language: IndexMap<String, LanguageSummary>,
    /// Git information (if available).
    pub git_info: Option<GitInfo>,
}

/// Repository summary statistics.
#[derive(Debug, Clone, Default, Serialize)]
pub struct RepoSummary {
    /// Total number of code files.
    pub total_files: usize,
    /// Line statistics.
    pub lines: LineStats,
    /// Total size in bytes.
    pub total_size: u64,
    /// Complexity metrics.
    pub complexity: Complexity,
    /// File size distribution.
    pub size_distribution: SizeDistribution,
}

/// Git repository information.
#[derive(Debug, Clone, Serialize)]
pub struct GitInfo {
    /// Current branch name.
    pub branch: Option<String>,
    /// Last commit hash.
    pub commit: Option<String>,
    /// Last commit author.
    pub author: Option<String>,
    /// Last commit date.
    pub date: Option<String>,
}

/// Overall analysis summary.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Summary {
    /// Total number of files.
    pub total_files: usize,
    /// Total line statistics.
    pub lines: LineStats,
    /// Total size in bytes.
    pub total_size: u64,
    /// Statistics by language.
    pub by_language: IndexMap<String, LanguageSummary>,
    /// File size distribution.
    pub size_distribution: SizeDistribution,
    /// Complexity metrics.
    pub complexity: Complexity,
    /// Estimated LLM tokens for the whole tree (rule-of-thumb).
    /// `serde(default)` keeps snapshots from older versions loadable.
    #[serde(default)]
    pub tokens_est: u64,
}

impl Summary {
    /// Build summary from a list of file statistics.
    pub fn from_file_stats(files: &[FileStats]) -> Self {
        let mut summary = Summary::default();
        let mut by_language: HashMap<String, LanguageSummary> = HashMap::new();

        for file in files {
            summary.total_files += 1;
            summary.lines.add(&file.lines);
            summary.total_size += file.size;
            summary.size_distribution.add(file.size);
            summary.complexity.add(&file.complexity);

            let file_tokens = super::tokens::estimate_tokens(&file.language, file.size);
            summary.tokens_est += file_tokens;

            let lang_summary = by_language.entry(file.language.clone()).or_default();
            lang_summary.files += 1;
            lang_summary.lines.add(&file.lines);
            lang_summary.size += file.size;
            lang_summary.complexity.add(&file.complexity);
            lang_summary.tokens_est += file_tokens;
        }

        // Sort by code lines (descending)
        let mut sorted: Vec<_> = by_language.into_iter().collect();
        sorted.sort_by_key(|(_, stats)| std::cmp::Reverse(stats.lines.code));
        summary.by_language = sorted.into_iter().collect();

        // Calculate average function lines
        if summary.complexity.functions > 0 {
            summary.complexity.avg_func_lines =
                summary.lines.code as f64 / summary.complexity.functions as f64;
        }

        summary
    }

    /// Re-order the per-language table according to `sort_by`.
    ///
    /// Formatters iterate `by_language` in map order, so this defines the
    /// output order for every format (console, JSON, CSV, ...).
    pub fn sort_languages(&mut self, sort_by: crate::config::SortBy) {
        use crate::config::SortBy;
        use std::cmp::Reverse;

        let mut entries: Vec<_> = std::mem::take(&mut self.by_language).into_iter().collect();
        match sort_by {
            SortBy::Lines => entries.sort_by_key(|e| Reverse(e.1.lines.total)),
            SortBy::Files => entries.sort_by_key(|e| Reverse(e.1.files)),
            SortBy::Code => entries.sort_by_key(|e| Reverse(e.1.lines.code)),
            SortBy::Name => entries.sort_by(|a, b| a.0.cmp(&b.0)),
            SortBy::Size => entries.sort_by_key(|e| Reverse(e.1.size)),
        }
        self.by_language = entries.into_iter().collect();
    }
}

/// Complete analysis result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalysisResult {
    /// All file statistics.
    pub files: Vec<FileStats>,
    /// Overall summary.
    pub summary: Summary,
    /// Analysis duration.
    #[serde(with = "duration_serde")]
    pub elapsed: Duration,
    /// Number of files scanned.
    pub scanned_files: usize,
    /// Number of files skipped (filters, unrecognized language, binary).
    pub skipped_files: usize,
    /// Number of files that failed to read or analyze.
    /// `serde(default)` keeps snapshots from older versions loadable.
    #[serde(default)]
    pub error_files: usize,
}

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

/// Aggregate per-file statistics into a directory tree, cumulative per
/// directory, at most `max_depth` components deep (deeper files still
/// roll up into their visible ancestors). Rows come back in tree order
/// (parents first, siblings sorted by code lines descending).
pub fn aggregate_by_dir(files: &[FileStats], max_depth: usize) -> Vec<DirStats> {
    use std::collections::HashMap;
    use std::path::Path;

    let max_depth = max_depth.max(1);
    let mut map: HashMap<PathBuf, DirStats> = HashMap::new();

    for f in files {
        let path = f.path.strip_prefix("./").unwrap_or(&f.path);
        let parent = path.parent().unwrap_or_else(|| Path::new(""));

        let mut dirs: Vec<PathBuf> = Vec::new();
        let mut acc = PathBuf::new();
        for comp in parent.components() {
            acc.push(comp);
            dirs.push(acc.clone());
            if dirs.len() == max_depth {
                break;
            }
        }
        if dirs.is_empty() {
            dirs.push(PathBuf::from("."));
        }

        for (i, dir) in dirs.into_iter().enumerate() {
            let entry = map.entry(dir.clone()).or_insert_with(|| DirStats {
                path: dir,
                depth: i + 1,
                files: 0,
                lines: LineStats::default(),
                size: 0,
                cyclomatic: 0,
                functions: 0,
            });
            entry.files += 1;
            entry.lines.total += f.lines.total;
            entry.lines.code += f.lines.code;
            entry.lines.comment += f.lines.comment;
            entry.lines.blank += f.lines.blank;
            entry.size += f.size;
            entry.cyclomatic += f.complexity.cyclomatic;
            entry.functions += f.complexity.functions;
        }
    }

    // Emit in tree order: group children under parents, siblings by code desc.
    let mut children: HashMap<Option<PathBuf>, Vec<PathBuf>> = HashMap::new();
    for dir in map.keys() {
        let parent = dir
            .parent()
            .filter(|p| !p.as_os_str().is_empty() && map.contains_key(*p))
            .map(|p| p.to_path_buf());
        children.entry(parent).or_default().push(dir.clone());
    }
    for list in children.values_mut() {
        list.sort_by_key(|d| std::cmp::Reverse(map[d].lines.code));
    }

    let mut ordered = Vec::with_capacity(map.len());
    let mut stack: Vec<PathBuf> = children.remove(&None).unwrap_or_default();
    stack.reverse();
    while let Some(dir) = stack.pop() {
        if let Some(mut kids) = children.remove(&Some(dir.clone())) {
            kids.reverse();
            stack.extend(kids);
        }
        ordered.push(map.remove(&dir).expect("dir queued exactly once"));
    }
    ordered
}

mod duration_serde {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};
    use std::time::Duration;

    pub fn serialize<S>(duration: &Duration, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        duration.as_secs_f64().serialize(serializer)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Duration, D::Error>
    where
        D: Deserializer<'de>,
    {
        let secs = f64::deserialize(deserializer)?;
        Ok(Duration::from_secs_f64(secs))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file_at(path: &str, code: usize) -> FileStats {
        FileStats {
            path: PathBuf::from(path),
            language: "Rust".to_string(),
            lines: LineStats {
                total: code + 10,
                code,
                comment: 5,
                blank: 5,
            },
            size: 100,
            complexity: Complexity {
                functions: 1,
                cyclomatic: 2,
                max_depth: 1,
                avg_func_lines: 10.0,
            },
        }
    }

    #[test]
    fn test_aggregate_by_dir_cumulative() {
        let files = vec![
            file_at("src/a.rs", 100),
            file_at("src/parser/b.rs", 200),
            file_at("src/parser/c.rs", 50),
            file_at("tests/t.rs", 30),
        ];
        let dirs = aggregate_by_dir(&files, 5);
        let src = dirs
            .iter()
            .find(|d| d.path == std::path::Path::new("src"))
            .unwrap();
        assert_eq!(src.files, 3, "src must include parser/ files");
        assert_eq!(src.lines.code, 350);
        assert_eq!(src.cyclomatic, 6);
        let parser = dirs
            .iter()
            .find(|d| d.path == std::path::Path::new("src/parser"))
            .unwrap();
        assert_eq!(parser.files, 2);
        assert_eq!(parser.depth, 2);
    }

    #[test]
    fn test_aggregate_by_dir_tree_order() {
        let files = vec![
            file_at("src/a.rs", 100),
            file_at("src/parser/b.rs", 200),
            file_at("tests/t.rs", 30),
        ];
        let dirs = aggregate_by_dir(&files, 5);
        let paths: Vec<&str> = dirs.iter().map(|d| d.path.to_str().unwrap()).collect();
        // src (300 code) before tests (30); src/parser directly after src.
        assert_eq!(paths, vec!["src", "src/parser", "tests"]);
    }

    #[test]
    fn test_aggregate_by_dir_depth_limit() {
        let files = vec![file_at("a/b/c/d.rs", 10)];
        let dirs = aggregate_by_dir(&files, 2);
        let paths: Vec<&str> = dirs.iter().map(|d| d.path.to_str().unwrap()).collect();
        assert_eq!(paths, vec!["a", "a/b"], "depth 3 must roll into a/b");
        assert_eq!(dirs[1].files, 1);
    }

    #[test]
    fn test_aggregate_by_dir_root_files() {
        let files = vec![file_at("README.md", 20), file_at("src/a.rs", 10)];
        let dirs = aggregate_by_dir(&files, 3);
        let root = dirs
            .iter()
            .find(|d| d.path == std::path::Path::new("."))
            .unwrap();
        assert_eq!(root.files, 1, "only direct root files land in '.'");
    }

    #[test]
    fn test_aggregate_by_dir_empty() {
        assert!(aggregate_by_dir(&[], 3).is_empty());
    }

    #[test]
    fn test_summary_estimates_tokens() {
        let mut rust = file_at("src/a.rs", 100);
        rust.size = 3300;
        let mut md = file_at("README.md", 50);
        md.language = "Markdown".to_string();
        md.size = 4000;

        let summary = Summary::from_file_stats(&[rust, md]);
        // Rust: 3300 / 3.3 = 1000; Markdown: 4000 / 4.0 = 1000.
        assert_eq!(summary.tokens_est, 2000);
        assert_eq!(summary.by_language["Rust"].tokens_est, 1000);
        assert_eq!(summary.by_language["Markdown"].tokens_est, 1000);
    }

    fn summary_with_langs() -> Summary {
        let mut summary = Summary::default();
        // Go: few files, many total lines, small size
        summary.by_language.insert(
            "Go".to_string(),
            LanguageSummary {
                files: 1,
                lines: LineStats {
                    total: 900,
                    code: 100,
                    ..Default::default()
                },
                size: 10,
                ..Default::default()
            },
        );
        // Rust: many files, few total lines, big size
        summary.by_language.insert(
            "Rust".to_string(),
            LanguageSummary {
                files: 5,
                lines: LineStats {
                    total: 300,
                    code: 200,
                    ..Default::default()
                },
                size: 999,
                ..Default::default()
            },
        );
        summary
    }

    fn lang_order(summary: &Summary) -> Vec<&str> {
        summary.by_language.keys().map(String::as_str).collect()
    }

    #[test]
    fn test_sort_languages_by_each_key() {
        use crate::config::SortBy;

        let mut s = summary_with_langs();
        s.sort_languages(SortBy::Lines);
        assert_eq!(lang_order(&s), ["Go", "Rust"], "total lines desc");

        s.sort_languages(SortBy::Files);
        assert_eq!(lang_order(&s), ["Rust", "Go"], "files desc");

        s.sort_languages(SortBy::Code);
        assert_eq!(lang_order(&s), ["Rust", "Go"], "code lines desc");

        s.sort_languages(SortBy::Name);
        assert_eq!(lang_order(&s), ["Go", "Rust"], "name asc");

        s.sort_languages(SortBy::Size);
        assert_eq!(lang_order(&s), ["Rust", "Go"], "size desc");
    }

    #[test]
    fn test_line_stats_default() {
        let stats = LineStats::default();
        assert_eq!(stats.total, 0);
        assert_eq!(stats.code, 0);
        assert_eq!(stats.comment, 0);
        assert_eq!(stats.blank, 0);
    }

    #[test]
    fn test_line_stats_add() {
        let mut stats1 = LineStats {
            total: 100,
            code: 80,
            comment: 10,
            blank: 10,
        };
        let stats2 = LineStats {
            total: 50,
            code: 40,
            comment: 5,
            blank: 5,
        };

        stats1.add(&stats2);

        assert_eq!(stats1.total, 150);
        assert_eq!(stats1.code, 120);
        assert_eq!(stats1.comment, 15);
        assert_eq!(stats1.blank, 15);
    }

    #[test]
    fn test_line_stats_add_trait() {
        let stats1 = LineStats {
            total: 100,
            code: 80,
            comment: 10,
            blank: 10,
        };
        let stats2 = LineStats {
            total: 50,
            code: 40,
            comment: 5,
            blank: 5,
        };

        let result = stats1 + stats2;

        assert_eq!(result.total, 150);
        assert_eq!(result.code, 120);
    }

    #[test]
    fn test_line_stats_add_assign() {
        let mut stats1 = LineStats {
            total: 100,
            code: 80,
            comment: 10,
            blank: 10,
        };
        let stats2 = LineStats {
            total: 50,
            code: 40,
            comment: 5,
            blank: 5,
        };

        stats1 += stats2;

        assert_eq!(stats1.total, 150);
        assert_eq!(stats1.code, 120);
    }

    #[test]
    fn test_complexity_add() {
        let mut c1 = Complexity {
            functions: 10,
            cyclomatic: 20,
            max_depth: 5,
            avg_func_lines: 0.0,
        };
        let c2 = Complexity {
            functions: 5,
            cyclomatic: 10,
            max_depth: 8,
            avg_func_lines: 0.0,
        };

        c1.add(&c2);

        assert_eq!(c1.functions, 15);
        assert_eq!(c1.cyclomatic, 30);
        assert_eq!(c1.max_depth, 8); // max of 5 and 8
    }

    #[test]
    fn test_size_distribution() {
        let mut dist = SizeDistribution::default();

        dist.add(500); // tiny: < 1KB
        dist.add(1024); // small: 1KB - 10KB
        dist.add(5000); // small
        dist.add(15000); // medium: 10KB - 100KB
        dist.add(500_000); // large: 100KB - 1MB
        dist.add(2_000_000); // huge: > 1MB

        assert_eq!(dist.tiny, 1);
        assert_eq!(dist.small, 2);
        assert_eq!(dist.medium, 1);
        assert_eq!(dist.large, 1);
        assert_eq!(dist.huge, 1);
    }

    #[test]
    fn test_summary_from_file_stats() {
        let files = vec![
            FileStats {
                path: PathBuf::from("src/main.rs"),
                language: "Rust".to_string(),
                lines: LineStats {
                    total: 100,
                    code: 80,
                    comment: 10,
                    blank: 10,
                },
                size: 2000,
                complexity: Complexity {
                    functions: 5,
                    cyclomatic: 10,
                    max_depth: 3,
                    avg_func_lines: 16.0,
                },
            },
            FileStats {
                path: PathBuf::from("src/lib.rs"),
                language: "Rust".to_string(),
                lines: LineStats {
                    total: 50,
                    code: 40,
                    comment: 5,
                    blank: 5,
                },
                size: 1000,
                complexity: Complexity {
                    functions: 3,
                    cyclomatic: 6,
                    max_depth: 2,
                    avg_func_lines: 13.3,
                },
            },
            FileStats {
                path: PathBuf::from("test.py"),
                language: "Python".to_string(),
                lines: LineStats {
                    total: 30,
                    code: 20,
                    comment: 5,
                    blank: 5,
                },
                size: 500,
                complexity: Complexity {
                    functions: 2,
                    cyclomatic: 4,
                    max_depth: 2,
                    avg_func_lines: 10.0,
                },
            },
        ];

        let summary = Summary::from_file_stats(&files);

        assert_eq!(summary.total_files, 3);
        assert_eq!(summary.lines.total, 180);
        assert_eq!(summary.lines.code, 140);
        assert_eq!(summary.total_size, 3500);
        assert_eq!(summary.by_language.len(), 2);
        assert_eq!(summary.complexity.functions, 10);

        // Rust should be first (more code lines)
        let first_lang = summary.by_language.keys().next().unwrap();
        assert_eq!(first_lang, "Rust");

        let rust_stats = summary.by_language.get("Rust").unwrap();
        assert_eq!(rust_stats.files, 2);
        assert_eq!(rust_stats.lines.code, 120);
    }

    #[test]
    fn test_summary_empty() {
        let summary = Summary::from_file_stats(&[]);

        assert_eq!(summary.total_files, 0);
        assert_eq!(summary.lines.total, 0);
        assert!(summary.by_language.is_empty());
    }

    #[test]
    fn test_file_stats_default() {
        let stats = FileStats::default();
        assert!(stats.path.as_os_str().is_empty());
        assert!(stats.language.is_empty());
        assert_eq!(stats.size, 0);
    }
}
