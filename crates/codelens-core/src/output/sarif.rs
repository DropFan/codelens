//! SARIF 2.1.0 output for GitHub code scanning and reviewdog.
//!
//! Health, regression, hotspot, and coupling findings map to SARIF
//! results; pure statistics reports produce an empty (but valid) run.
//! Consumption paths: `github/codeql-action/upload-sarif` for native PR
//! annotations (public repos / GHAS), or `reviewdog -f=sarif` elsewhere.

use std::io::Write;

use serde_json::{json, Value};

use crate::error::Result;
use crate::insight::Grade;

use super::format::{OutputFormat, OutputOptions, Report};

/// SARIF output formatter.
pub struct SarifOutput;

impl SarifOutput {
    /// Create a new SARIF output formatter.
    pub fn new() -> Self {
        Self
    }
}

impl Default for SarifOutput {
    fn default() -> Self {
        Self::new()
    }
}

impl OutputFormat for SarifOutput {
    fn name(&self) -> &'static str {
        "sarif"
    }

    fn extension(&self) -> &'static str {
        "sarif"
    }

    fn write(
        &self,
        report: &Report,
        _options: &OutputOptions,
        writer: &mut dyn Write,
    ) -> Result<()> {
        let results = match report {
            Report::Health(health) => health_results(health),
            Report::Combined(combined) => health_results(&combined.health),
            Report::Hotspot(hotspot) => hotspot_results(hotspot),
            Report::Coupling(coupling) => coupling_results(coupling),
            // Statistics/trend/estimation reports carry no findings.
            _ => Vec::new(),
        };

        let doc = json!({
            "$schema": "https://raw.githubusercontent.com/oasis-tcs/sarif-spec/master/Schemata/sarif-schema-2.1.0.json",
            "version": "2.1.0",
            "runs": [{
                "tool": {
                    "driver": {
                        "name": "codelens",
                        "version": env!("CARGO_PKG_VERSION"),
                        "informationUri": "https://github.com/DropFan/codelens",
                        "rules": rules(),
                    }
                },
                "results": results,
            }]
        });
        serde_json::to_writer_pretty(&mut *writer, &doc)?;
        writeln!(writer)?;
        Ok(())
    }
}

fn rules() -> Value {
    json!([
        {
            "id": "codelens/health-grade",
            "shortDescription": { "text": "File health grade is poor" },
            "helpUri": "https://github.com/DropFan/codelens#health-score",
        },
        {
            "id": "codelens/health-regression",
            "shortDescription": { "text": "File health regressed against the baseline" },
            "helpUri": "https://github.com/DropFan/codelens#health-score",
        },
        {
            "id": "codelens/hotspot",
            "shortDescription": { "text": "File is a change hotspot (high churn x complexity)" },
            "helpUri": "https://github.com/DropFan/codelens#hotspot-detection",
        },
        {
            "id": "codelens/change-coupling",
            "shortDescription": { "text": "Files change together without a structural link" },
            "helpUri": "https://github.com/DropFan/codelens#change-coupling",
        },
    ])
}

fn location(uri: &str) -> Value {
    // GitHub matches artifact URIs against repo-relative paths; a leading
    // "./" breaks that matching.
    let uri = uri.strip_prefix("./").unwrap_or(uri);
    json!({
        "physicalLocation": {
            "artifactLocation": { "uri": uri },
        }
    })
}

fn health_results(health: &crate::insight::health::HealthReport) -> Vec<Value> {
    let mut results = Vec::new();

    for file in &health.worst_files {
        let level = match file.grade {
            Grade::F => "error",
            Grade::D => "warning",
            _ => continue,
        };
        results.push(json!({
            "ruleId": "codelens/health-grade",
            "level": level,
            "message": { "text": format!(
                "Health grade {} (score {:.1}); top issue: {}",
                file.grade, file.score, file.top_issue
            )},
            "locations": [location(&file.path.display().to_string())],
        }));
    }

    if let Some(regression) = &health.regression {
        for file in &regression.regressed_files {
            results.push(json!({
                "ruleId": "codelens/health-regression",
                "level": "error",
                "message": { "text": format!(
                    "Health regressed against {}: {} ({:.1}) → {} ({:.1})",
                    regression.baseline,
                    file.from_grade, file.from_score,
                    file.to_grade, file.to_score
                )},
                "locations": [location(&file.path.display().to_string())],
            }));
        }
    }

    results
}

fn hotspot_results(report: &crate::insight::hotspot::HotspotReport) -> Vec<Value> {
    use crate::insight::hotspot::RiskLevel;

    report
        .files
        .iter()
        .filter_map(|file| {
            let level = match file.risk {
                RiskLevel::High => "warning",
                RiskLevel::Medium => "note",
                RiskLevel::Low => return None,
            };
            Some(json!({
                "ruleId": "codelens/hotspot",
                "level": level,
                "message": { "text": format!(
                    "Change hotspot: {} commits in {}, cyclomatic complexity {}, score {:.2}",
                    file.churn.commits, report.since,
                    file.complexity.cyclomatic, file.hotspot_score
                )},
                "locations": [location(&file.path.display().to_string())],
            }))
        })
        .collect()
}

fn coupling_results(report: &crate::insight::coupling::CouplingReport) -> Vec<Value> {
    report
        .pairs
        .iter()
        .map(|pair| {
            json!({
                "ruleId": "codelens/change-coupling",
                "level": "note",
                "message": { "text": format!(
                    "Changes together with {} in {:.0}% of commits ({} shared)",
                    pair.file_b.display(), pair.degree, pair.shared_commits
                )},
                "locations": [
                    location(&pair.file_a.display().to_string()),
                    location(&pair.file_b.display().to_string()),
                ],
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::insight::health::{FileHealth, FileRegression, HealthReport, RegressionReport};
    use crate::insight::scoring::HealthDimension;
    use std::path::PathBuf;

    fn make_health() -> HealthReport {
        HealthReport {
            score: 55.0,
            grade: Grade::F,
            model: "default".to_string(),
            dimensions: vec![],
            by_directory: vec![],
            worst_files: vec![
                FileHealth {
                    path: PathBuf::from("src/bad.rs"),
                    score: 42.0,
                    grade: Grade::F,
                    top_issue: HealthDimension::Complexity,
                    dimensions: vec![],
                },
                FileHealth {
                    path: PathBuf::from("src/ok.rs"),
                    score: 85.0,
                    grade: Grade::B,
                    top_issue: HealthDimension::FileSize,
                    dimensions: vec![],
                },
            ],
            regression: Some(RegressionReport {
                baseline: "git:main".to_string(),
                baseline_score: 80.0,
                baseline_grade: Grade::B,
                score_delta: -25.0,
                project_regressed: true,
                regressed_files: vec![FileRegression {
                    path: PathBuf::from("src/worse.rs"),
                    from_score: 82.0,
                    from_grade: Grade::B,
                    to_score: 71.0,
                    to_grade: Grade::C,
                }],
                improved_files: 0,
                failed: true,
            }),
        }
    }

    #[test]
    fn test_sarif_health_shape() {
        let mut buf = Vec::new();
        SarifOutput::new()
            .write(
                &Report::Health(make_health()),
                &OutputOptions::default(),
                &mut buf,
            )
            .unwrap();
        let doc: Value = serde_json::from_slice(&buf).unwrap();
        assert_eq!(doc["version"], "2.1.0");
        let results = doc["runs"][0]["results"].as_array().unwrap();
        // bad.rs (grade F) + worse.rs (regression); ok.rs (B) excluded.
        assert_eq!(results.len(), 2);
        assert_eq!(results[0]["ruleId"], "codelens/health-grade");
        assert_eq!(results[0]["level"], "error");
        assert_eq!(
            results[0]["locations"][0]["physicalLocation"]["artifactLocation"]["uri"],
            "src/bad.rs"
        );
        assert_eq!(results[1]["ruleId"], "codelens/health-regression");
    }

    #[test]
    fn test_sarif_empty_run_for_stats() {
        use crate::analyzer::stats::Summary;
        let result = crate::AnalysisResult {
            files: vec![],
            summary: Summary::default(),
            elapsed: std::time::Duration::from_millis(1),
            scanned_files: 0,
            skipped_files: 0,
            error_files: 0,
        };
        let mut buf = Vec::new();
        SarifOutput::new()
            .write(
                &Report::Analysis(result),
                &OutputOptions::default(),
                &mut buf,
            )
            .unwrap();
        let doc: Value = serde_json::from_slice(&buf).unwrap();
        assert!(doc["runs"][0]["results"].as_array().unwrap().is_empty());
    }
}
