//! Change hotspot analysis: churn × complexity.

use std::collections::HashMap;
use std::path::PathBuf;

use serde::Serialize;

use crate::analyzer::stats::{AnalysisResult, Complexity};
use crate::git::FileChurn;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum RiskLevel {
    High,
    Medium,
    Low,
}

impl std::fmt::Display for RiskLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RiskLevel::High => write!(f, "HIGH"),
            RiskLevel::Medium => write!(f, "MED"),
            RiskLevel::Low => write!(f, "LOW"),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct ChurnMetrics {
    pub commits: usize,
    pub lines_added: usize,
    pub lines_deleted: usize,
    pub lines_churn: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct FileHotspot {
    pub path: PathBuf,
    pub language: String,
    pub churn: ChurnMetrics,
    pub complexity: Complexity,
    pub hotspot_score: f64,
    pub risk: RiskLevel,
}

#[derive(Debug, Clone, Serialize)]
pub struct HotspotReport {
    pub files: Vec<FileHotspot>,
    pub since: String,
    pub total_commits: usize,
}

pub fn analyze(
    churns: &[FileChurn],
    analysis: &AnalysisResult,
    since: &str,
    total_commits: usize,
    top_n: usize,
) -> HotspotReport {
    let stats_map: HashMap<&PathBuf, _> = analysis.files.iter().map(|f| (&f.path, f)).collect();

    let mut hotspots: Vec<FileHotspot> = churns
        .iter()
        .filter_map(|churn| {
            stats_map.get(&churn.path).map(|stats| FileHotspot {
                path: churn.path.clone(),
                language: stats.language.clone(),
                churn: ChurnMetrics {
                    commits: churn.commits,
                    lines_added: churn.lines_added,
                    lines_deleted: churn.lines_deleted,
                    lines_churn: churn.lines_added + churn.lines_deleted,
                },
                complexity: stats.complexity.clone(),
                hotspot_score: 0.0,
                risk: RiskLevel::Low,
            })
        })
        .collect();

    normalize_and_score(&mut hotspots);
    hotspots.sort_by(|a, b| {
        b.hotspot_score
            .partial_cmp(&a.hotspot_score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    hotspots.truncate(top_n);

    HotspotReport {
        files: hotspots,
        since: since.to_string(),
        total_commits,
    }
}

fn normalize_and_score(hotspots: &mut [FileHotspot]) {
    if hotspots.is_empty() {
        return;
    }
    let max_churn = hotspots.iter().map(|h| h.churn.commits).max().unwrap_or(1) as f64;
    let max_cc = hotspots
        .iter()
        .map(|h| h.complexity.cyclomatic)
        .max()
        .unwrap_or(1) as f64;

    for h in hotspots.iter_mut() {
        let norm_churn = if max_churn > 0.0 {
            h.churn.commits as f64 / max_churn
        } else {
            0.0
        };
        let norm_cc = if max_cc > 0.0 {
            h.complexity.cyclomatic as f64 / max_cc
        } else {
            0.0
        };
        h.hotspot_score = norm_churn * norm_cc;
        h.risk = match h.hotspot_score {
            s if s > 0.7 => RiskLevel::High,
            s if s > 0.3 => RiskLevel::Medium,
            _ => RiskLevel::Low,
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analyzer::stats::{FileStats, LineStats, Summary};
    use std::time::Duration;

    fn make_churns() -> Vec<FileChurn> {
        vec![
            FileChurn {
                path: PathBuf::from("hot.rs"),
                commits: 50,
                lines_added: 500,
                lines_deleted: 200,
            },
            FileChurn {
                path: PathBuf::from("warm.rs"),
                commits: 10,
                lines_added: 100,
                lines_deleted: 50,
            },
            FileChurn {
                path: PathBuf::from("cold.rs"),
                commits: 2,
                lines_added: 10,
                lines_deleted: 5,
            },
            FileChurn {
                path: PathBuf::from("deleted.rs"),
                commits: 5,
                lines_added: 30,
                lines_deleted: 30,
            },
        ]
    }

    fn make_analysis() -> AnalysisResult {
        let files = vec![
            FileStats {
                path: PathBuf::from("hot.rs"),
                language: "Rust".to_string(),
                lines: LineStats {
                    total: 300,
                    code: 250,
                    comment: 20,
                    blank: 30,
                },
                size: 5000,
                complexity: Complexity {
                    functions: 5,
                    cyclomatic: 40,
                    max_depth: 6,
                    avg_func_lines: 50.0,
                },
            },
            FileStats {
                path: PathBuf::from("warm.rs"),
                language: "Rust".to_string(),
                lines: LineStats {
                    total: 100,
                    code: 80,
                    comment: 10,
                    blank: 10,
                },
                size: 2000,
                complexity: Complexity {
                    functions: 4,
                    cyclomatic: 10,
                    max_depth: 3,
                    avg_func_lines: 20.0,
                },
            },
            FileStats {
                path: PathBuf::from("cold.rs"),
                language: "Rust".to_string(),
                lines: LineStats {
                    total: 50,
                    code: 40,
                    comment: 5,
                    blank: 5,
                },
                size: 1000,
                complexity: Complexity {
                    functions: 2,
                    cyclomatic: 4,
                    max_depth: 2,
                    avg_func_lines: 20.0,
                },
            },
        ];
        AnalysisResult {
            summary: Summary::from_file_stats(&files),
            files,
            elapsed: Duration::from_millis(50),
            scanned_files: 3,
            skipped_files: 0,
        }
    }

    #[test]
    fn test_hotspot_basic() {
        let report = analyze(&make_churns(), &make_analysis(), "90d", 100, 10);
        assert_eq!(report.since, "90d");
        assert_eq!(report.total_commits, 100);
        assert_eq!(report.files.len(), 3); // deleted.rs not in analysis
    }

    #[test]
    fn test_hotspot_sorted_descending() {
        let report = analyze(&make_churns(), &make_analysis(), "90d", 100, 10);
        for window in report.files.windows(2) {
            assert!(window[0].hotspot_score >= window[1].hotspot_score);
        }
    }

    #[test]
    fn test_hotspot_highest_is_hot() {
        let report = analyze(&make_churns(), &make_analysis(), "90d", 100, 10);
        assert_eq!(report.files[0].path, PathBuf::from("hot.rs"));
        assert!((report.files[0].hotspot_score - 1.0).abs() < 0.01);
        assert_eq!(report.files[0].risk, RiskLevel::High);
    }

    #[test]
    fn test_hotspot_skips_deleted_files() {
        let report = analyze(&make_churns(), &make_analysis(), "90d", 100, 10);
        assert!(report
            .files
            .iter()
            .all(|f| f.path != std::path::Path::new("deleted.rs")));
    }

    #[test]
    fn test_hotspot_top_n() {
        let report = analyze(&make_churns(), &make_analysis(), "90d", 100, 1);
        assert_eq!(report.files.len(), 1);
    }

    #[test]
    fn test_hotspot_empty_churns() {
        let report = analyze(&[], &make_analysis(), "90d", 0, 10);
        assert!(report.files.is_empty());
    }

    #[test]
    fn test_hotspot_churn_metrics() {
        let report = analyze(&make_churns(), &make_analysis(), "90d", 100, 10);
        let hot = &report.files[0];
        assert_eq!(hot.churn.commits, 50);
        assert_eq!(hot.churn.lines_added, 500);
        assert_eq!(hot.churn.lines_deleted, 200);
        assert_eq!(hot.churn.lines_churn, 700);
    }

    #[test]
    fn test_risk_levels() {
        let report = analyze(&make_churns(), &make_analysis(), "90d", 100, 10);
        let risks: Vec<RiskLevel> = report.files.iter().map(|f| f.risk).collect();
        assert!(risks.contains(&RiskLevel::High));
    }
}
