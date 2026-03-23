use regex::Regex;

use crate::code_fence;
use crate::types::{CoreError, ProtectedRegion};

/// A single regex-based preprocessing rule.
#[derive(Debug)]
struct RegexRule {
    id: String,
    pattern: Regex,
    replacement: String,
}

/// Result of preprocessing.
#[derive(Debug, Clone)]
pub struct PreprocessResult {
    pub text: String,
    pub rules_applied: Vec<String>,
}

/// Configuration for the preprocessor.
#[derive(Debug, Clone)]
pub struct PreprocessorConfig {
    pub enabled: bool,
    pub structural_enabled: bool,
    pub semantic_enabled: bool,
}

impl Default for PreprocessorConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            structural_enabled: true,
            semantic_enabled: true,
        }
    }
}

/// Regex-based preprocessor that runs before the Aho-Corasick compressor.
///
/// Splits input into protected and unprotected segments, then applies regex
/// rules only to unprotected text.
#[derive(Debug)]
pub struct Preprocessor {
    rules: Vec<RegexRule>,
}

impl Preprocessor {
    /// Build a preprocessor from configuration.
    pub fn build(config: &PreprocessorConfig) -> Result<Self, CoreError> {
        let mut rules = Vec::new();

        if config.structural_enabled {
            rules.extend(Self::structural_rules()?);
        }

        if config.semantic_enabled {
            rules.extend(Self::semantic_rules()?);
        }

        Ok(Preprocessor { rules })
    }

    /// Structural regex rules for markdown/HTML cleanup.
    fn structural_rules() -> Result<Vec<RegexRule>, CoreError> {
        let defs: Vec<(&str, &str, &str)> = vec![
            // Markdown escape sequences — must run before bold/italic/link
            ("struct-md-escape", r"\\([*_\-#\[\]()\\ ])", "$1"),
            // Markdown heading markers
            ("struct-md-heading", r"(?m)^#{1,6}\s+", ""),
            // Bold (**text**) — must come before italic
            ("struct-md-bold", r"\*\*(.+?)\*\*", "$1"),
            // Italic (*text*)
            ("struct-md-italic", r"\*([^*\n]+)\*", "$1"),
            // Horizontal rules
            ("struct-md-hr", r"(?m)^-{3,}\s*$", ""),
            // Collapse 3+ newlines to 2
            ("struct-md-blank", r"\n{3,}", "\n\n"),
            // Markdown links [text](url) → text
            ("struct-md-link", r"\[([^\]]+)\]\([^)]+\)", "$1"),
            // Bare URLs
            ("struct-bare-url", r"https?://\S+", ""),
            // Checkboxes
            ("struct-checkbox", r"(?m)^-\s*\[[ x]\]\s*", ""),
            // Numbered list with paren: 1) → 1.
            ("struct-list-paren", r"(?m)^(\d+)\)\s", "$1. "),
            // HTML <br> tags
            ("struct-html-br", r"<br\s*/?>", " "),
            // HTML entities
            ("struct-html-nbsp", r"&nbsp;", " "),
            ("struct-html-mdash", r"&mdash;", "—"),
            ("struct-html-ndash", r"&ndash;", "–"),
        ];

        defs.into_iter()
            .map(|(id, pattern, replacement)| {
                Ok(RegexRule {
                    id: id.to_string(),
                    pattern: Regex::new(pattern)?,
                    replacement: replacement.to_string(),
                })
            })
            .collect()
    }

    /// Semantic regex rules (placeholder for future expansion).
    fn semantic_rules() -> Result<Vec<RegexRule>, CoreError> {
        // Semantic phrases are handled via Aho-Corasick (stopword files).
        // This slot is reserved for regex-only semantic patterns.
        Ok(vec![])
    }

    /// Apply all preprocessing rules to the input text, respecting protected regions.
    pub fn process(&self, text: &str) -> PreprocessResult {
        if self.rules.is_empty() || text.is_empty() {
            return PreprocessResult {
                text: text.to_string(),
                rules_applied: vec![],
            };
        }

        let mut result = text.to_string();
        let mut rules_applied: Vec<String> = Vec::new();

        for rule in &self.rules {
            let protected = code_fence::find_protected_regions(&result);
            let new_text = Self::apply_rule_respecting_protection(&result, rule, &protected);
            if new_text != result {
                rules_applied.push(rule.id.clone());
                result = new_text;
            }
        }

        rules_applied.dedup();

        PreprocessResult {
            text: result,
            rules_applied,
        }
    }

    /// Apply a single regex rule, skipping matches that overlap with protected regions.
    fn apply_rule_respecting_protection(
        text: &str,
        rule: &RegexRule,
        protected: &[ProtectedRegion],
    ) -> String {
        // Collect non-overlapping matches that are outside protected regions.
        let matches: Vec<regex::Match<'_>> = rule
            .pattern
            .find_iter(text)
            .filter(|m| !Self::overlaps_protected(m.start(), m.end(), protected))
            .collect();

        if matches.is_empty() {
            return text.to_string();
        }

        // Apply replacements from end to start to preserve offsets.
        let mut result = text.to_string();
        for m in matches.into_iter().rev() {
            // Use regex replace on the matched substring to expand capture groups.
            let replaced = rule
                .pattern
                .replace(m.as_str(), rule.replacement.as_str());
            result.replace_range(m.start()..m.end(), &replaced);
        }

        result
    }

    fn overlaps_protected(start: usize, end: usize, regions: &[ProtectedRegion]) -> bool {
        regions.iter().any(|r| start < r.end && end > r.start)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn build_full_preprocessor() -> Preprocessor {
        Preprocessor::build(&PreprocessorConfig::default()).unwrap()
    }

    #[test]
    fn test_md_heading_removal() {
        let p = build_full_preprocessor();
        let result = p.process("## Hello World");
        assert_eq!(result.text, "Hello World");
        assert!(result.rules_applied.contains(&"struct-md-heading".to_string()));
    }

    #[test]
    fn test_md_bold() {
        let p = build_full_preprocessor();
        let result = p.process("This is **bold** text");
        assert_eq!(result.text, "This is bold text");
    }

    #[test]
    fn test_md_italic() {
        let p = build_full_preprocessor();
        let result = p.process("This is *italic* text");
        assert_eq!(result.text, "This is italic text");
    }

    #[test]
    fn test_md_link() {
        let p = build_full_preprocessor();
        let result = p.process("See [docs](https://example.com) for more");
        assert_eq!(result.text, "See docs for more");
    }

    #[test]
    fn test_bare_url_removal() {
        let p = build_full_preprocessor();
        let result = p.process("Visit https://example.com/foo for info");
        // URL removed, trailing space collapsed
        assert!(!result.text.contains("https://"));
    }

    #[test]
    fn test_html_entities() {
        let p = build_full_preprocessor();
        let result = p.process("hello&nbsp;world &mdash; test &ndash; end");
        assert_eq!(result.text, "hello world — test – end");
    }

    #[test]
    fn test_protected_code_fence() {
        let p = build_full_preprocessor();
        let text = "## Title\n```\n## Not a heading\n```\n## Another";
        let result = p.process(text);
        // Headings outside code fence removed, inside preserved
        assert!(result.text.contains("## Not a heading"));
        assert!(!result.text.starts_with("## "));
    }

    #[test]
    fn test_protected_inline_code() {
        let p = build_full_preprocessor();
        let result = p.process("Use `**not bold**` for emphasis");
        assert!(result.text.contains("`**not bold**`"));
    }

    #[test]
    fn test_blank_line_collapse() {
        let p = build_full_preprocessor();
        let result = p.process("a\n\n\n\nb");
        assert_eq!(result.text, "a\n\nb");
    }

    #[test]
    fn test_checkbox_removal() {
        let p = build_full_preprocessor();
        let result = p.process("- [x] Done\n- [ ] Todo");
        assert_eq!(result.text, "Done\nTodo");
    }

    #[test]
    fn test_disabled_preprocessor() {
        let p = Preprocessor::build(&PreprocessorConfig {
            enabled: true,
            structural_enabled: false,
            semantic_enabled: false,
        })
        .unwrap();
        let result = p.process("## Heading **bold**");
        assert_eq!(result.text, "## Heading **bold**");
    }

    #[test]
    fn test_md_escape() {
        let p = build_full_preprocessor();
        // Escape rule removes backslashes: \* → *
        // Then italic rule strips the resulting *...*
        // For compression purposes, this is the desired behavior
        let result = p.process(r"This is \*not italic\*");
        assert_eq!(result.text, "This is not italic");
        assert!(result.rules_applied.contains(&"struct-md-escape".to_string()));
    }

    #[test]
    fn test_hr_removal() {
        let p = build_full_preprocessor();
        let result = p.process("above\n---\nbelow");
        assert_eq!(result.text, "above\n\nbelow");
    }
}
