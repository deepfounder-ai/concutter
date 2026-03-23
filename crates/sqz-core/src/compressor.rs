use std::collections::HashMap;
use std::time::Instant;

use aho_corasick::AhoCorasick;

use crate::code_fence;
use crate::domain_detector;
use crate::layer2_domain::DomainLayer;
use crate::preprocessor::Preprocessor;
use crate::token_counter::TokenCounter;
use crate::types::*;

/// The main compression engine.
///
/// Builds an Aho-Corasick automaton from all enabled rules across the three
/// layers and applies pattern replacements in a single pass over the input
/// text, respecting protected regions (code fences, inline code, JSON blocks,
/// and domain-specific protected terms).
#[derive(Debug)]
pub struct Compressor {
    /// Aho-Corasick automaton built from all rule patterns.
    automaton: AhoCorasick,
    /// (pattern, replacement) pairs indexed identically to the automaton.
    patterns: Vec<(String, String)>,
    /// Rule IDs indexed identically to the automaton (for reporting).
    rule_ids: Vec<String>,
    /// Protected terms across all domains (retained for introspection).
    #[allow(dead_code)]
    protected_terms: Vec<String>,
    /// Aho-Corasick automaton for protected terms (if any).
    protected_automaton: Option<AhoCorasick>,
    /// Token counter.
    token_counter: TokenCounter,
    /// Configuration (retained for introspection / rebuilding).
    #[allow(dead_code)]
    config: CompressorConfig,
    /// Domain configs for auto-detection.
    domain_configs: HashMap<String, DomainConfig>,
    /// Optional regex preprocessor (runs before Aho-Corasick).
    preprocessor: Option<Preprocessor>,
}

impl Compressor {
    /// Build a `Compressor` from the three rule layers.
    ///
    /// All enabled rules are collected, sorted by priority (descending) then
    /// by pattern length (descending), and compiled into an Aho-Corasick
    /// automaton for efficient multi-pattern matching.
    pub fn build(
        static_rules: Vec<Rule>,
        domain_layer: &DomainLayer,
        learned_rules: Vec<Rule>,
        config: CompressorConfig,
    ) -> Result<Self, CoreError> {
        Self::build_with_preprocessor(static_rules, domain_layer, learned_rules, config, None)
    }

    /// Build a `Compressor` with an optional regex preprocessor.
    pub fn build_with_preprocessor(
        static_rules: Vec<Rule>,
        domain_layer: &DomainLayer,
        learned_rules: Vec<Rule>,
        config: CompressorConfig,
        preprocessor: Option<Preprocessor>,
    ) -> Result<Self, CoreError> {
        // Collect enabled rules from all three layers
        let mut all_rules: Vec<Rule> = Vec::new();

        if config.layers_enabled.static_enabled {
            all_rules.extend(static_rules.into_iter().filter(|r| r.enabled));
        }

        if config.layers_enabled.domain_enabled {
            all_rules.extend(domain_layer.all_rules().into_iter().filter(|r| r.enabled));
        }

        if config.layers_enabled.learned_enabled {
            all_rules.extend(learned_rules.into_iter().filter(|r| r.enabled));
        }

        // Sort: higher priority first, then longer patterns first (for tie-breaking)
        all_rules.sort_by(|a, b| {
            b.priority
                .cmp(&a.priority)
                .then_with(|| b.pattern.len().cmp(&a.pattern.len()))
        });

        // Build pattern/replacement pairs and rule IDs
        let mut patterns: Vec<(String, String)> = Vec::with_capacity(all_rules.len());
        let mut rule_ids: Vec<String> = Vec::with_capacity(all_rules.len());

        for rule in &all_rules {
            if !rule.pattern.is_empty() {
                patterns.push((rule.pattern.clone(), rule.replacement.clone()));
                rule_ids.push(rule.id.clone());
            }
        }

        // Build main automaton
        let pattern_strs: Vec<&str> = patterns.iter().map(|(p, _)| p.as_str()).collect();
        let automaton = AhoCorasick::builder()
            .ascii_case_insensitive(true)
            .build(&pattern_strs)
            .map_err(|e| CoreError::Other(format!("failed to build Aho-Corasick automaton: {e}")))?;

        // Collect all protected terms from all domains
        let mut protected_terms: Vec<String> = Vec::new();
        for domain_name in domain_layer.domains() {
            protected_terms.extend(domain_layer.protected_terms(&domain_name));
        }
        protected_terms.sort();
        protected_terms.dedup();

        // Build protected terms automaton
        let protected_automaton = if protected_terms.is_empty() {
            None
        } else {
            let terms_refs: Vec<&str> = protected_terms.iter().map(|s| s.as_str()).collect();
            Some(
                AhoCorasick::builder()
                    .ascii_case_insensitive(true)
                    .build(&terms_refs)
                    .map_err(|e| {
                        CoreError::Other(format!(
                            "failed to build protected terms automaton: {e}"
                        ))
                    })?,
            )
        };

        let domain_configs = domain_layer.domain_configs().clone();

        Ok(Compressor {
            automaton,
            patterns,
            rule_ids,
            protected_terms,
            protected_automaton,
            token_counter: TokenCounter::new(),
            config,
            domain_configs,
            preprocessor,
        })
    }

    /// Compress the given text.
    ///
    /// If `domain_hint` is `Some`, that domain's rules get priority;
    /// otherwise, auto-detection is attempted based on keyword frequency.
    pub fn compress(&self, text: &str, domain_hint: Option<&str>) -> CompressionResult {
        let start_time = Instant::now();

        // Count original tokens
        let original_tokens = self.token_counter.count(text);

        // If the text is empty, return immediately
        if text.is_empty() {
            return CompressionResult {
                text: String::new(),
                original_tokens: 0,
                compressed_tokens: 0,
                compression_ratio: 1.0,
                rules_applied: Vec::new(),
                elapsed_us: start_time.elapsed().as_micros() as u64,
                domain_detected: None,
            };
        }

        // Run preprocessor first (if configured)
        let (working_text, mut preprocess_rules) = if let Some(ref pp) = self.preprocessor {
            let pp_result = pp.process(text);
            (pp_result.text, pp_result.rules_applied)
        } else {
            (text.to_string(), vec![])
        };
        let text = &working_text;

        // Detect domain
        let domain_detected = domain_hint
            .map(String::from)
            .or_else(|| domain_detector::detect_domain(text, &self.domain_configs));

        // Find protected regions from code fences / inline code / JSON blocks
        let mut protected_regions = code_fence::find_protected_regions(text);

        // Also mark protected terms as protected regions
        if let Some(ref prot_auto) = self.protected_automaton {
            let text_lower = text.to_lowercase();
            for mat in prot_auto.find_iter(&text_lower) {
                protected_regions.push(ProtectedRegion {
                    start: mat.start(),
                    end: mat.end(),
                });
            }
        }

        // Find all matches using Aho-Corasick
        let text_lower = text.to_lowercase();
        let mut matches: Vec<(usize, usize, usize)> = Vec::new(); // (start, end, pattern_index)

        for mat in self.automaton.find_iter(&text_lower) {
            let m_start = mat.start();
            let m_end = mat.end();
            let pat_idx = mat.pattern().as_usize();

            // Skip if inside a protected region
            if is_in_any_protected_region(m_start, m_end, &protected_regions) {
                continue;
            }

            // Word-boundary check
            if !is_word_boundary(text.as_bytes(), m_start, m_end) {
                continue;
            }

            matches.push((m_start, m_end, pat_idx));
        }

        // Remove overlapping matches: greedily keep the first (highest priority) match
        // at each position. Since Aho-Corasick returns matches in text order, we keep
        // track of the rightmost end so far and skip any match that overlaps.
        let mut filtered: Vec<(usize, usize, usize)> = Vec::with_capacity(matches.len());
        let mut rightmost_end: usize = 0;
        for (m_start, m_end, pat_idx) in &matches {
            if *m_start >= rightmost_end {
                filtered.push((*m_start, *m_end, *pat_idx));
                rightmost_end = *m_end;
            }
        }

        // Apply replacements from end to start so character offsets stay valid
        let mut result = text.to_string();
        let mut rules_applied: Vec<String> = Vec::new();

        for &(m_start, m_end, pat_idx) in filtered.iter().rev() {
            let (_, replacement) = &self.patterns[pat_idx];
            let rule_id = &self.rule_ids[pat_idx];

            // Build the replacement, preserving leading/trailing whitespace logic:
            // If the replacement is empty and the match is surrounded by spaces,
            // collapse to a single space.
            let new_text = if replacement.is_empty() {
                let has_space_before =
                    m_start > 0 && result.as_bytes()[m_start - 1] == b' ';
                let has_space_after =
                    m_end < result.len() && result.as_bytes()[m_end] == b' ';

                if has_space_before && has_space_after {
                    // Remove the match and one surrounding space
                    // We'll remove from (m_start - 1)..m_end which replaces "X matched Y" with "XY"
                    // but actually we want " matched " -> " ", so just use empty and let the
                    // spaces collapse naturally by removing one space.
                    result.replace_range(m_start..m_end, "");
                    // Now we might have double space at m_start if there was space before and after.
                    // Remove one space if we now have double space.
                    if m_start > 0
                        && m_start < result.len()
                        && result.as_bytes()[m_start - 1] == b' '
                        && result.as_bytes()[m_start] == b' '
                    {
                        result.remove(m_start);
                    }
                    rules_applied.push(rule_id.clone());
                    continue;
                } else {
                    String::new()
                }
            } else {
                replacement.clone()
            };

            result.replace_range(m_start..m_end, &new_text);
            rules_applied.push(rule_id.clone());
        }

        // Clean up any double spaces introduced by replacements
        while result.contains("  ") {
            result = result.replace("  ", " ");
        }

        // Trim leading/trailing whitespace
        let result = result.trim().to_string();

        // Count compressed tokens
        let compressed_tokens = self.token_counter.count(&result);

        let compression_ratio = if original_tokens > 0 {
            compressed_tokens as f64 / original_tokens as f64
        } else {
            1.0
        };

        rules_applied.reverse(); // put in forward order
        // Prepend preprocessor rules
        preprocess_rules.append(&mut rules_applied);
        let mut rules_applied = preprocess_rules;
        rules_applied.dedup();

        CompressionResult {
            text: result,
            original_tokens,
            compressed_tokens,
            compression_ratio,
            rules_applied,
            elapsed_us: start_time.elapsed().as_micros() as u64,
            domain_detected,
        }
    }
}

/// Check whether a match range overlaps with any protected region.
fn is_in_any_protected_region(
    start: usize,
    end: usize,
    regions: &[ProtectedRegion],
) -> bool {
    regions
        .iter()
        .any(|r| start < r.end && end > r.start)
}

/// Check whether a match at `start..end` is on a word boundary.
///
/// A match is on a word boundary if:
/// - `start == 0` OR the byte before `start` is whitespace/punctuation
/// - `end == len` OR the byte after `end - 1` is whitespace/punctuation
fn is_word_boundary(bytes: &[u8], start: usize, end: usize) -> bool {
    let len = bytes.len();

    let left_ok = start == 0 || is_boundary_byte(bytes[start - 1]);
    let right_ok = end >= len || is_boundary_byte(bytes[end]);

    left_ok && right_ok
}

/// Returns `true` if the byte is considered a word boundary character
/// (whitespace or common punctuation).
#[inline]
fn is_boundary_byte(b: u8) -> bool {
    matches!(
        b,
        b' ' | b'\t'
            | b'\n'
            | b'\r'
            | b'.'
            | b','
            | b';'
            | b':'
            | b'!'
            | b'?'
            | b'"'
            | b'\''
            | b'('
            | b')'
            | b'['
            | b']'
            | b'{'
            | b'}'
            | b'/'
            | b'\\'
            | b'-'
            | b'_'
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layer1_static::StaticLayer;
    use crate::layer2_domain::DomainLayer;

    fn build_test_compressor() -> Compressor {
        let static_lines = vec![
            "please".to_string(),
            "could you please".to_string(),
            "just".to_string(),
        ];
        let static_layer = StaticLayer::load_from_strings(&static_lines);

        let domain_layer = DomainLayer::load_from_configs(vec![DomainConfig {
            name: "code".to_string(),
            description: "Code".to_string(),
            keywords: vec!["function".to_string(), "class".to_string()],
            protected_terms: vec![],
            rules: vec![DomainRule {
                pattern: "write a function that".to_string(),
                replacement: "function:".to_string(),
            }],
        }]);

        let config = CompressorConfig {
            confidence_threshold: 0.8,
            min_samples: 10,
            languages: vec!["en".to_string()],
            layers_enabled: LayersEnabled {
                static_enabled: true,
                domain_enabled: true,
                learned_enabled: true,
            },
        };

        Compressor::build(static_layer.rules(), &domain_layer, vec![], config).unwrap()
    }

    #[test]
    fn test_basic_compression() {
        let c = build_test_compressor();
        let result = c.compress("Could you please write a function that adds two numbers", None);
        // "could you please" should be removed, "write a function that" -> "function:"
        assert!(result.text.contains("function:"));
        assert!(!result.text.to_lowercase().contains("could you please"));
        assert!(!result.rules_applied.is_empty());
    }

    #[test]
    fn test_empty_input() {
        let c = build_test_compressor();
        let result = c.compress("", None);
        assert_eq!(result.text, "");
        assert_eq!(result.original_tokens, 0);
    }

    #[test]
    fn test_no_matches() {
        let c = build_test_compressor();
        let result = c.compress("hello world", None);
        assert_eq!(result.text, "hello world");
    }

    #[test]
    fn test_protected_code_block() {
        let c = build_test_compressor();
        let text = "```\nplease just do it\n```";
        let result = c.compress(text, None);
        // Content inside code block should be unchanged
        assert!(result.text.contains("please"));
    }

    #[test]
    fn test_word_boundary() {
        // "just" should not match inside "adjustment"
        let c = build_test_compressor();
        let result = c.compress("make an adjustment here", None);
        assert!(result.text.contains("adjustment"));
    }

    #[test]
    fn test_compression_ratio() {
        let c = build_test_compressor();
        let result = c.compress(
            "Could you please just write a function that does something simple",
            None,
        );
        assert!(result.compression_ratio <= 1.0);
        assert!(result.original_tokens > 0);
    }

    #[test]
    fn test_is_word_boundary() {
        let text = b"hello world";
        assert!(is_word_boundary(text, 0, 5)); // "hello" at start
        assert!(is_word_boundary(text, 6, 11)); // "world" at end
        assert!(!is_word_boundary(text, 1, 4)); // "ello" not on boundary
    }
}
