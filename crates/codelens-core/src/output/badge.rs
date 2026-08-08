//! shields.io endpoint badge JSON output.
//!
//! Produces the JSON shape consumed by `https://img.shields.io/endpoint`,
//! so a repository can embed a live "code health: B" badge in its README.

use std::io::Write;

use serde::Serialize;

use crate::error::Result;
use crate::insight::Grade;

use super::format::{OutputFormat, OutputOptions, Report};

/// shields.io endpoint badge formatter.
pub struct BadgeOutput;

impl BadgeOutput {
    pub fn new() -> Self {
        Self
    }
}

impl Default for BadgeOutput {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Serialize)]
struct ShieldsBadge {
    #[serde(rename = "schemaVersion")]
    schema_version: u8,
    label: String,
    message: String,
    color: String,
}

fn grade_color(grade: Grade) -> &'static str {
    match grade {
        Grade::A => "brightgreen",
        Grade::B => "green",
        Grade::C => "yellow",
        Grade::D => "orange",
        Grade::F => "red",
    }
}

fn format_thousands(n: usize) -> String {
    let s = n.to_string();
    let mut out = String::new();
    for (i, c) in s.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    out.chars().rev().collect()
}

impl OutputFormat for BadgeOutput {
    fn name(&self) -> &'static str {
        "badge"
    }

    fn extension(&self) -> &'static str {
        "json"
    }

    fn write(
        &self,
        report: &Report,
        _options: &OutputOptions,
        writer: &mut dyn Write,
    ) -> Result<()> {
        let badge = match report {
            // The health grade is the badge with the most signal
            Report::Health(r) => ShieldsBadge {
                schema_version: 1,
                label: "code health".to_string(),
                message: format!("{} ({:.1})", r.grade, r.score),
                color: grade_color(r.grade).to_string(),
            },
            Report::Combined(c) => ShieldsBadge {
                schema_version: 1,
                label: "code health".to_string(),
                message: format!("{} ({:.1})", c.health.grade, c.health.score),
                color: grade_color(c.health.grade).to_string(),
            },
            Report::Analysis(r) => ShieldsBadge {
                schema_version: 1,
                label: "lines of code".to_string(),
                message: format_thousands(r.summary.lines.code),
                color: "blue".to_string(),
            },
            Report::Hotspot(r) => ShieldsBadge {
                schema_version: 1,
                label: "hotspots".to_string(),
                message: r.files.len().to_string(),
                color: if r.files.is_empty() {
                    "brightgreen"
                } else {
                    "orange"
                }
                .to_string(),
            },
            Report::Coupling(r) => ShieldsBadge {
                schema_version: 1,
                label: "coupled pairs".to_string(),
                message: r.pairs.len().to_string(),
                color: if r.pairs.is_empty() {
                    "brightgreen"
                } else {
                    "orange"
                }
                .to_string(),
            },
            Report::Trend(r) => {
                let delta = r.delta.code.signed_delta();
                ShieldsBadge {
                    schema_version: 1,
                    label: "code trend".to_string(),
                    message: if delta >= 0 {
                        format!("+{delta} lines")
                    } else {
                        format!("{delta} lines")
                    },
                    color: "blue".to_string(),
                }
            }
            Report::Estimation(r) => ShieldsBadge {
                schema_version: 1,
                label: "est. cost".to_string(),
                message: format!("${:.0}", r.estimated_cost),
                color: "blue".to_string(),
            },
            Report::EstimationComparison(r) => ShieldsBadge {
                schema_version: 1,
                label: "est. cost".to_string(),
                message: r
                    .reports
                    .first()
                    .map(|first| format!("${:.0}", first.estimated_cost))
                    .unwrap_or_else(|| "n/a".to_string()),
                color: "blue".to_string(),
            },
        };

        serde_json::to_writer(&mut *writer, &badge)?;
        writeln!(writer)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::insight::health::HealthReport;

    #[test]
    fn test_health_badge_shape() {
        let report = HealthReport {
            score: 82.3,
            grade: Grade::B,
            model: "default".to_string(),
            dimensions: vec![],
            by_directory: vec![],
            worst_files: vec![],
        };

        let mut buf = Vec::new();
        BadgeOutput::new()
            .write(&Report::Health(report), &OutputOptions::default(), &mut buf)
            .unwrap();

        let v: serde_json::Value = serde_json::from_slice(&buf).unwrap();
        assert_eq!(v["schemaVersion"], 1);
        assert_eq!(v["label"], "code health");
        assert_eq!(v["message"], "B (82.3)");
        assert_eq!(v["color"], "green");
    }
}
