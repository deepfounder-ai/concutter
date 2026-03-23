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

    /// Compress text with a specific compression level.
    ///
    /// - `Skip`   — return text unchanged (with token counts).
    /// - `Light`  — run only the regex preprocessor (no Aho-Corasick).
    /// - `Normal` — full pipeline (equivalent to [`compress`]).
    pub fn compress_with_level(
        &self,
        text: &str,
        domain_hint: Option<&str>,
        level: CompressionLevel,
    ) -> CompressionResult {
        match level {
            CompressionLevel::Skip => self.compress_skip(text),
            CompressionLevel::Light => self.compress_light(text),
            CompressionLevel::Normal => self.compress(text, domain_hint),
        }
    }

    /// Skip compression — return the original text with token counts.
    fn compress_skip(&self, text: &str) -> CompressionResult {
        let tokens = self.token_counter.count(text);
        CompressionResult {
            text: text.to_string(),
            original_tokens: tokens,
            compressed_tokens: tokens,
            compression_ratio: 1.0,
            rules_applied: Vec::new(),
            elapsed_us: 0,
            domain_detected: None,
        }
    }

    /// Light compression — only run the regex preprocessor, skip Aho-Corasick.
    fn compress_light(&self, text: &str) -> CompressionResult {
        let start_time = Instant::now();
        let original_tokens = self.token_counter.count(text);

        if text.is_empty() {
            return CompressionResult {
                text: String::new(),
                original_tokens: 0,
                compressed_tokens: 0,
                compression_ratio: 1.0,
                rules_applied: Vec::new(),
                elapsed_us: 0,
                domain_detected: None,
            };
        }

        let (result, rules_applied) = if let Some(ref pp) = self.preprocessor {
            let pp_result = pp.process(text);
            (pp_result.text, pp_result.rules_applied)
        } else {
            (text.to_string(), vec![])
        };

        let compressed_tokens = self.token_counter.count(&result);
        let compression_ratio = if original_tokens > 0 {
            compressed_tokens as f64 / original_tokens as f64
        } else {
            1.0
        };

        CompressionResult {
            text: result,
            original_tokens,
            compressed_tokens,
            compression_ratio,
            rules_applied,
            elapsed_us: start_time.elapsed().as_micros() as u64,
            domain_detected: None,
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
/// - `start == 0` OR the character before `start` is whitespace/punctuation
/// - `end == len` OR the character after `end - 1` is whitespace/punctuation
///
/// For non-ASCII text (Cyrillic, CJK, etc.) this decodes the full UTF-8
/// character to check Unicode properties.
fn is_word_boundary(bytes: &[u8], start: usize, end: usize) -> bool {
    let len = bytes.len();

    let left_ok = start == 0 || is_boundary_at(bytes, start, false);
    let right_ok = end >= len || is_boundary_at(bytes, end, true);

    left_ok && right_ok
}

/// Check if there is a word boundary at the given byte position.
///
/// When `forward` is true, we look at the character starting at `pos`.
/// When `forward` is false, we look at the character ending just before `pos`.
#[inline]
fn is_boundary_at(bytes: &[u8], pos: usize, forward: bool) -> bool {
    if forward {
        // Look at byte at `pos`
        if pos >= bytes.len() {
            return true;
        }
        let b = bytes[pos];
        if b.is_ascii() {
            return is_ascii_boundary(b);
        }
        // Decode UTF-8 character at pos and check Unicode properties
        if let Some(ch) = decode_utf8_char_at(bytes, pos) {
            is_unicode_boundary(ch)
        } else {
            false
        }
    } else {
        // Look at character ending just before `pos`
        if pos == 0 {
            return true;
        }
        let b = bytes[pos - 1];
        if b.is_ascii() {
            return is_ascii_boundary(b);
        }
        // Walk backwards to find the start of the UTF-8 character
        if let Some(ch) = decode_utf8_char_before(bytes, pos) {
            is_unicode_boundary(ch)
        } else {
            false
        }
    }
}

/// Decode the UTF-8 character starting at the given byte position.
fn decode_utf8_char_at(bytes: &[u8], pos: usize) -> Option<char> {
    let s = std::str::from_utf8(&bytes[pos..]).ok()?;
    s.chars().next()
}

/// Decode the UTF-8 character ending just before the given byte position.
fn decode_utf8_char_before(bytes: &[u8], pos: usize) -> Option<char> {
    // Walk backwards up to 4 bytes to find a valid UTF-8 start byte
    for offset in 1..=4.min(pos) {
        let start = pos - offset;
        if let Ok(s) = std::str::from_utf8(&bytes[start..pos]) {
            if let Some(ch) = s.chars().last() {
                return Some(ch);
            }
        }
    }
    None
}

/// Returns `true` if the ASCII byte is a word boundary character.
#[inline]
fn is_ascii_boundary(b: u8) -> bool {
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

/// Returns `true` if the Unicode character is a word boundary (not alphanumeric).
#[inline]
fn is_unicode_boundary(ch: char) -> bool {
    !ch.is_alphanumeric()
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

    #[test]
    fn test_is_word_boundary_cyrillic() {
        let text = "привет просто мир".as_bytes();
        // "просто" starts after "привет " (12 + 1 = 13 bytes for "привет ")
        let start = "привет ".len();
        let end = start + "просто".len();
        assert!(is_word_boundary(text, start, end)); // space-separated
    }

    #[test]
    fn test_is_word_boundary_cyrillic_no_space() {
        let text = "приветпростомир".as_bytes();
        let start = "привет".len();
        let end = start + "просто".len();
        assert!(!is_word_boundary(text, start, end)); // no spaces, not a boundary
    }

    #[test]
    fn test_russian_compression() {
        let static_lines = vec![
            "пожалуйста".to_string(),
            "просто".to_string(),
        ];
        let static_layer = StaticLayer::load_from_strings(&static_lines);
        let domain_layer = DomainLayer::load_from_configs(vec![]);
        let config = CompressorConfig {
            languages: vec!["ru".to_string()],
            ..Default::default()
        };
        let c = Compressor::build(static_layer.rules(), &domain_layer, vec![], config).unwrap();
        let result = c.compress("пожалуйста просто сделай это", None);
        assert!(!result.text.contains("пожалуйста"));
        assert!(!result.text.contains("просто"));
        assert!(result.text.contains("сделай"));
        assert!(!result.rules_applied.is_empty());
    }

    #[test]
    fn test_spanish_compression() {
        let static_lines = vec![
            "por favor".to_string(),
            "realmente".to_string(),
        ];
        let static_layer = StaticLayer::load_from_strings(&static_lines);
        let domain_layer = DomainLayer::load_from_configs(vec![]);
        let config = CompressorConfig::default();
        let c = Compressor::build(static_layer.rules(), &domain_layer, vec![], config).unwrap();
        let result = c.compress("por favor realmente hazlo", None);
        assert!(!result.text.contains("por favor"));
        assert!(!result.text.contains("realmente"));
        assert!(result.text.contains("hazlo"));
    }

    #[test]
    fn test_german_compression() {
        let static_lines = vec![
            "bitte".to_string(),
            "wirklich".to_string(),
        ];
        let static_layer = StaticLayer::load_from_strings(&static_lines);
        let domain_layer = DomainLayer::load_from_configs(vec![]);
        let config = CompressorConfig::default();
        let c = Compressor::build(static_layer.rules(), &domain_layer, vec![], config).unwrap();
        let result = c.compress("bitte wirklich mach das", None);
        assert!(!result.text.contains("bitte"));
        assert!(!result.text.contains("wirklich"));
        assert!(result.text.contains("mach das"));
    }

    #[test]
    fn test_french_compression() {
        let static_lines = vec![
            "s'il vous plaît".to_string(),
            "vraiment".to_string(),
        ];
        let static_layer = StaticLayer::load_from_strings(&static_lines);
        let domain_layer = DomainLayer::load_from_configs(vec![]);
        let config = CompressorConfig::default();
        let c = Compressor::build(static_layer.rules(), &domain_layer, vec![], config).unwrap();
        let result = c.compress("s'il vous plaît vraiment faites-le", None);
        assert!(!result.text.contains("s'il vous plaît"));
        assert!(!result.text.contains("vraiment"));
        assert!(result.text.contains("faites-le"));
    }

    #[test]
    fn test_unicode_punctuation_boundary() {
        // Russian with «» quotes — Unicode punctuation should be boundaries
        let static_lines = vec!["просто".to_string()];
        let static_layer = StaticLayer::load_from_strings(&static_lines);
        let domain_layer = DomainLayer::load_from_configs(vec![]);
        let config = CompressorConfig::default();
        let c = Compressor::build(static_layer.rules(), &domain_layer, vec![], config).unwrap();
        let result = c.compress("это «просто» тест", None);
        assert!(!result.text.contains("просто"));
    }

    // -----------------------------------------------------------------------
    // compress_with_level tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_compress_with_level_skip() {
        let c = build_test_compressor();
        let text = "Could you please just do it";
        let result = c.compress_with_level(text, None, CompressionLevel::Skip);
        assert_eq!(result.text, text);
        assert_eq!(result.compression_ratio, 1.0);
        assert!(result.rules_applied.is_empty());
    }

    #[test]
    fn test_compress_with_level_normal() {
        let c = build_test_compressor();
        let text = "Could you please just do it";
        let normal = c.compress_with_level(text, None, CompressionLevel::Normal);
        let direct = c.compress(text, None);
        assert_eq!(normal.text, direct.text);
        assert_eq!(normal.rules_applied, direct.rules_applied);
    }

    #[test]
    fn test_compress_with_level_light_no_preprocessor() {
        // Compressor without preprocessor — Light should be a no-op
        let c = build_test_compressor();
        let text = "## Heading with **bold** and please remove this";
        let result = c.compress_with_level(text, None, CompressionLevel::Light);
        // No preprocessor configured, so text is unchanged
        assert_eq!(result.text, text);
        assert!(result.rules_applied.is_empty());
    }

    fn build_test_compressor_with_preprocessor() -> Compressor {
        let static_lines = vec![
            "please".to_string(),
            "could you please".to_string(),
            "just".to_string(),
        ];
        let static_layer = crate::layer1_static::StaticLayer::load_from_strings(&static_lines);
        let domain_layer = DomainLayer::load_from_configs(vec![]);
        let config = CompressorConfig::default();
        let pp = crate::preprocessor::Preprocessor::build(
            &crate::preprocessor::PreprocessorConfig::default(),
        )
        .unwrap();
        Compressor::build_with_preprocessor(
            static_layer.rules(),
            &domain_layer,
            vec![],
            config,
            Some(pp),
        )
        .unwrap()
    }

    #[test]
    fn test_compress_with_level_light_with_preprocessor() {
        let c = build_test_compressor_with_preprocessor();
        let text = "## Please just do **something**";
        let result = c.compress_with_level(text, None, CompressionLevel::Light);
        // Preprocessor strips markdown: heading marker + bold markers
        assert!(!result.text.contains("##"));
        assert!(!result.text.contains("**"));
        // But stopwords ("please", "just") are NOT removed by Light
        assert!(result.text.to_lowercase().contains("please"));
        assert!(result.text.to_lowercase().contains("just"));
    }

    #[test]
    fn test_compress_with_level_normal_with_preprocessor() {
        let c = build_test_compressor_with_preprocessor();
        let text = "## Please just do **something**";
        let result = c.compress_with_level(text, None, CompressionLevel::Normal);
        // Both markdown AND stopwords removed
        assert!(!result.text.contains("##"));
        assert!(!result.text.contains("**"));
        assert!(!result.text.to_lowercase().contains("please"));
        assert!(!result.text.to_lowercase().contains("just"));
        assert!(result.text.contains("do"));
        assert!(result.text.contains("something"));
    }

    #[test]
    fn test_arrow_replacement_in_compression() {
        let static_lines = vec![
            "in order to -> to".to_string(),
            "due to the fact that -> because".to_string(),
        ];
        let static_layer = StaticLayer::load_from_strings(&static_lines);
        let domain_layer = DomainLayer::load_from_configs(vec![]);
        let config = CompressorConfig::default();
        let c = Compressor::build(static_layer.rules(), &domain_layer, vec![], config).unwrap();
        let result = c.compress("in order to succeed due to the fact that it matters", None);
        assert!(result.text.contains("to succeed"));
        assert!(result.text.contains("because it matters"));
        assert!(!result.text.contains("in order to"));
        assert!(!result.text.contains("due to the fact that"));
    }
}
