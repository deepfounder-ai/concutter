use std::path::Path;

use crate::types::{CoreError, Rule, RuleLayer};

/// Layer 1: static stopword/filler-phrase removal rules.
///
/// Patterns are loaded from plain-text files (one pattern per line) and sorted
/// by descending length so that longer (more specific) patterns are matched
/// before shorter ones.
#[derive(Debug, Clone)]
pub struct StaticLayer {
    /// (pattern, replacement) pairs. Replacement is always empty for stopwords.
    patterns: Vec<(String, String)>,
}

impl StaticLayer {
    /// Load stopword files for the given languages from `rules_dir/stopwords/{lang}.txt`
    /// and `rules_dir/stopwords/{lang}_semantic.txt`.
    pub fn load(languages: &[String], rules_dir: &Path) -> Result<Self, CoreError> {
        let mut all_lines: Vec<String> = Vec::new();

        for lang in languages {
            // Load main stopwords: {lang}.txt
            let path = rules_dir.join("stopwords").join(format!("{lang}.txt"));
            if path.exists() {
                let content = std::fs::read_to_string(&path)?;
                for line in content.lines() {
                    let trimmed = line.trim();
                    if !trimmed.is_empty() {
                        all_lines.push(trimmed.to_string());
                    }
                }
            } else {
                tracing::warn!("stopword file not found: {}", path.display());
            }

            // Load semantic phrases: {lang}_semantic.txt
            let semantic_path = rules_dir.join("stopwords").join(format!("{lang}_semantic.txt"));
            if semantic_path.exists() {
                let content = std::fs::read_to_string(&semantic_path)?;
                for line in content.lines() {
                    let trimmed = line.trim();
                    if !trimmed.is_empty() {
                        all_lines.push(trimmed.to_string());
                    }
                }
            }
        }

        Ok(Self::load_from_strings(&all_lines))
    }

    /// Build a `StaticLayer` directly from a list of pattern strings (useful
    /// for testing or when patterns are already in memory).
    ///
    /// Supports optional ` -> ` separator for replacement rules:
    /// - `"please"` — removes "please" (empty replacement)
    /// - `"in order to -> to"` — replaces "in order to" with "to"
    pub fn load_from_strings(lines: &[String]) -> Self {
        let mut patterns: Vec<(String, String)> = lines
            .iter()
            .filter(|l| !l.trim().is_empty())
            .map(|l| {
                let trimmed = l.trim();
                if let Some((pat, repl)) = trimmed.split_once(" -> ") {
                    (pat.trim().to_lowercase(), repl.trim().to_string())
                } else {
                    (trimmed.to_lowercase(), String::new())
                }
            })
            .collect();

        // De-duplicate
        patterns.sort_by(|a, b| a.0.cmp(&b.0));
        patterns.dedup_by(|a, b| a.0 == b.0);

        // Sort by descending pattern length so longer matches come first
        patterns.sort_by(|a, b| b.0.len().cmp(&a.0.len()));

        StaticLayer { patterns }
    }

    /// Convert the loaded patterns into `Rule` structs.
    pub fn rules(&self) -> Vec<Rule> {
        self.patterns
            .iter()
            .enumerate()
            .map(|(i, (pattern, replacement))| Rule {
                id: format!("static-{i}"),
                pattern: pattern.clone(),
                replacement: replacement.clone(),
                layer: RuleLayer::Static,
                domain: None,
                confidence: 1.0,
                samples: i64::MAX, // static rules are always considered proven
                enabled: true,
                priority: 100, // static rules have base priority 100
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_load_from_strings() {
        let lines = vec![
            "please".to_string(),
            "could you please".to_string(),
            "just".to_string(),
        ];
        let layer = StaticLayer::load_from_strings(&lines);
        let rules = layer.rules();
        assert_eq!(rules.len(), 3);
        // Longest first
        assert_eq!(rules[0].pattern, "could you please");
        assert_eq!(rules[1].pattern, "please");
        assert_eq!(rules[2].pattern, "just");
    }

    #[test]
    fn test_empty_replacement() {
        let lines = vec!["basically".to_string()];
        let layer = StaticLayer::load_from_strings(&lines);
        let rules = layer.rules();
        assert_eq!(rules[0].replacement, "");
    }

    #[test]
    fn test_arrow_replacement() {
        let lines = vec![
            "in order to -> to".to_string(),
            "due to the fact that -> because".to_string(),
        ];
        let layer = StaticLayer::load_from_strings(&lines);
        let rules = layer.rules();
        assert_eq!(rules.len(), 2);
        // Longest first
        assert_eq!(rules[0].pattern, "due to the fact that");
        assert_eq!(rules[0].replacement, "because");
        assert_eq!(rules[1].pattern, "in order to");
        assert_eq!(rules[1].replacement, "to");
    }

    #[test]
    fn test_dedup() {
        let lines = vec![
            "please".to_string(),
            "Please".to_string(), // same after lowercasing
        ];
        let layer = StaticLayer::load_from_strings(&lines);
        let rules = layer.rules();
        assert_eq!(rules.len(), 1);
    }
}
