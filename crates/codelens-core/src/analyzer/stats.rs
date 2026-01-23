//! Statistics data structures.

use indexmap::IndexMap;
use serde::Serialize;
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;

/// Statistics for a single file.
#[derive(Debug, Clone, Default, Serialize)]
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
#[derive(Debug, Clone, Default, Serialize, PartialEq, Eq)]
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
#[derive(Debug, Clone, Default, Serialize)]
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
#[derive(Debug, Clone, Default, Serialize)]
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
#[derive(Debug, Clone, Default, Serialize)]
pub struct LanguageSummary {
    /// Number of files.
    pub files: usize,
    /// Line statistics.
    pub lines: LineStats,
    /// Total size in bytes.
    pub size: u64,
    /// Complexity metrics.
    pub complexity: Complexity,
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
#[derive(Debug, Clone, Default, Serialize)]
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

            let lang_summary = by_language.entry(file.language.clone()).or_default();
            lang_summary.files += 1;
            lang_summary.lines.add(&file.lines);
            lang_summary.size += file.size;
            lang_summary.complexity.add(&file.complexity);
        }

        // Sort by code lines (descending)
        let mut sorted: Vec<_> = by_language.into_iter().collect();
        sorted.sort_by(|a, b| b.1.lines.code.cmp(&a.1.lines.code));
        summary.by_language = sorted.into_iter().collect();

        // Calculate average function lines
        if summary.complexity.functions > 0 {
            summary.complexity.avg_func_lines =
                summary.lines.code as f64 / summary.complexity.functions as f64;
        }

        summary
    }
}

/// Complete analysis result.
#[derive(Debug, Clone, Serialize)]
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
    /// Number of files skipped.
    pub skipped_files: usize,
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
