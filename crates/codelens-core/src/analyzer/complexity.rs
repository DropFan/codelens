//! Code complexity analysis.

use regex::Regex;

use crate::language::Language;

use super::stats::Complexity;

/// Analyzes code complexity metrics.
pub struct ComplexityAnalyzer {
    // Cached regex patterns
}

impl ComplexityAnalyzer {
    /// Create a new complexity analyzer.
    pub fn new() -> Self {
        Self {}
    }

    /// Analyze complexity metrics for the given content.
    pub fn analyze(&self, content: &str, lang: &Language) -> Complexity {
        let mut complexity = Complexity::default();

        // Count functions
        if let Some(ref pattern) = lang.function_pattern {
            if let Ok(re) = Regex::new(pattern) {
                complexity.functions = re.find_iter(content).count();
            }
        }

        // Count complexity keywords (cyclomatic complexity approximation)
        for keyword in &lang.complexity_keywords {
            // Match whole words only
            let pattern = format!(r"\b{}\b", regex::escape(keyword));
            if let Ok(re) = Regex::new(&pattern) {
                complexity.cyclomatic += re.find_iter(content).count();
            }
        }

        // Base complexity is 1 per function
        complexity.cyclomatic += complexity.functions;

        // Calculate max nesting depth
        complexity.max_depth = self.calculate_max_depth(content);

        // Calculate average lines per function
        if complexity.functions > 0 {
            let total_lines = content.lines().count();
            complexity.avg_func_lines = total_lines as f64 / complexity.functions as f64;
        }

        complexity
    }

    /// Calculate maximum nesting depth based on braces/indentation.
    fn calculate_max_depth(&self, content: &str) -> usize {
        let mut max_depth: usize = 0;
        let mut current_depth: usize = 0;

        for c in content.chars() {
            match c {
                '{' | '(' | '[' => {
                    current_depth += 1;
                    max_depth = max_depth.max(current_depth);
                }
                '}' | ')' | ']' => {
                    current_depth = current_depth.saturating_sub(1);
                }
                _ => {}
            }
        }

        max_depth
    }
}

impl Default for ComplexityAnalyzer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_rust_lang() -> Language {
        Language {
            name: "Rust".to_string(),
            extensions: vec![".rs".to_string()],
            filenames: vec![],
            line_comments: vec!["//".to_string()],
            block_comments: vec![("/*".to_string(), "*/".to_string())],
            string_delimiters: vec![],
            function_pattern: Some(r"(?m)^\s*(pub\s+)?(async\s+)?fn\s+\w+".to_string()),
            complexity_keywords: vec![
                "if".to_string(),
                "else".to_string(),
                "for".to_string(),
                "while".to_string(),
                "match".to_string(),
            ],
            nested_comments: true,
        }
    }

    #[test]
    fn test_count_functions() {
        let analyzer = ComplexityAnalyzer::new();
        let lang = make_rust_lang();

        let content = r#"
fn main() {
    println!("hello");
}

pub fn helper() {}

pub async fn async_fn() {}
"#;

        let complexity = analyzer.analyze(content, &lang);
        assert_eq!(complexity.functions, 3);
    }

    #[test]
    fn test_cyclomatic_complexity() {
        let analyzer = ComplexityAnalyzer::new();
        let lang = make_rust_lang();

        let content = r#"
fn main() {
    if true {
        for i in 0..10 {
            if i > 5 {
                println!("{}", i);
            }
        }
    } else {
        while false {}
    }
}
"#;

        let complexity = analyzer.analyze(content, &lang);
        // 1 function + 2 if + 1 for + 1 else + 1 while = 6
        assert_eq!(complexity.cyclomatic, 6);
    }

    #[test]
    fn test_max_depth() {
        let analyzer = ComplexityAnalyzer::new();
        let lang = make_rust_lang();

        let content = r#"
fn main() {
    if true {
        for i in 0..10 {
            match i {
                _ => {}
            }
        }
    }
}
"#;

        let complexity = analyzer.analyze(content, &lang);
        assert!(complexity.max_depth >= 4);
    }
}
