//! Formatter-neutral presentation data for hotspot reports.

use crate::insight::hotspot::{format_age, FileHotspot, FunctionHotspot, HotspotReport, RiskLevel};

#[derive(Debug, Clone)]
pub(crate) struct HotspotPresentation {
    pub since: String,
    pub total_commits: usize,
    pub files: Vec<HotspotFilePresentation>,
    pub knowledge_islands: Vec<HotspotKnowledgeIslandPresentation>,
    pub function_files: Vec<HotspotFunctionFilePresentation>,
}

#[derive(Debug, Clone)]
pub(crate) struct HotspotFilePresentation {
    pub path: String,
    pub commits: usize,
    pub lines_added: usize,
    pub lines_deleted: usize,
    pub cyclomatic: usize,
    pub age: String,
    pub authors: String,
    pub score: String,
    pub risk: RiskLevel,
}

#[derive(Debug, Clone)]
pub(crate) struct HotspotKnowledgeIslandPresentation {
    pub path: String,
    pub ownership_percent: f64,
    pub main_author: String,
}

#[derive(Debug, Clone)]
pub(crate) struct HotspotFunctionFilePresentation {
    pub path: String,
    pub functions: Vec<HotspotFunctionPresentation>,
}

#[derive(Debug, Clone)]
pub(crate) struct HotspotFunctionPresentation {
    pub name: String,
    pub start_line: usize,
    pub end_line: usize,
    pub commits: usize,
    pub cyclomatic: usize,
}

impl HotspotPresentation {
    pub(crate) fn from_report(report: &HotspotReport) -> Self {
        Self {
            since: report.since.clone(),
            total_commits: report.total_commits,
            files: report
                .files
                .iter()
                .map(HotspotFilePresentation::from)
                .collect(),
            knowledge_islands: report
                .files
                .iter()
                .filter_map(HotspotKnowledgeIslandPresentation::from_file)
                .collect(),
            function_files: report
                .files
                .iter()
                .filter_map(HotspotFunctionFilePresentation::from_file)
                .collect(),
        }
    }
}

impl From<&FileHotspot> for HotspotFilePresentation {
    fn from(file: &FileHotspot) -> Self {
        Self {
            path: file.path.display().to_string(),
            commits: file.churn.commits,
            lines_added: file.churn.lines_added,
            lines_deleted: file.churn.lines_deleted,
            cyclomatic: file.complexity.cyclomatic,
            age: file.age_days.map(format_age).unwrap_or_else(|| "-".into()),
            authors: match &file.knowledge {
                Some(knowledge) if knowledge.knowledge_island => {
                    format!("{} ★", knowledge.authors)
                }
                Some(knowledge) => knowledge.authors.to_string(),
                None => "-".into(),
            },
            score: format!("{:.2}", file.hotspot_score),
            risk: file.risk,
        }
    }
}

impl HotspotKnowledgeIslandPresentation {
    fn from_file(file: &FileHotspot) -> Option<Self> {
        file.knowledge
            .as_ref()
            .filter(|knowledge| knowledge.knowledge_island)
            .map(|knowledge| Self {
                path: file.path.display().to_string(),
                ownership_percent: knowledge.ownership * 100.0,
                main_author: knowledge.main_author.clone(),
            })
    }
}

impl HotspotFunctionFilePresentation {
    fn from_file(file: &FileHotspot) -> Option<Self> {
        file.functions.as_ref().map(|functions| Self {
            path: file.path.display().to_string(),
            functions: functions
                .iter()
                .map(HotspotFunctionPresentation::from)
                .collect(),
        })
    }
}

impl From<&FunctionHotspot> for HotspotFunctionPresentation {
    fn from(function: &FunctionHotspot) -> Self {
        Self {
            name: function.name.clone(),
            start_line: function.start_line,
            end_line: function.end_line,
            commits: function.commits,
            cyclomatic: function.cyclomatic,
        }
    }
}
