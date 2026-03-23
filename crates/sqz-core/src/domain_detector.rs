use std::collections::HashMap;

use crate::types::DomainConfig;

/// Minimum number of keyword hits required to activate a domain.
const MIN_KEYWORD_HITS: usize = 2;

/// Detect the most likely domain for a piece of text based on keyword
/// frequency.
///
/// Returns `None` if no domain has at least `MIN_KEYWORD_HITS` keyword
/// occurrences in the text.
pub fn detect_domain(
    text: &str,
    domains: &HashMap<String, DomainConfig>,
) -> Option<String> {
    let text_lower = text.to_lowercase();

    let mut best_domain: Option<String> = None;
    let mut best_count: usize = 0;

    for (name, config) in domains {
        let mut count: usize = 0;
        for keyword in &config.keywords {
            let kw_lower = keyword.to_lowercase();
            // Count non-overlapping occurrences of the keyword in the text
            let mut start = 0;
            while let Some(pos) = text_lower[start..].find(&kw_lower) {
                count += 1;
                start += pos + kw_lower.len();
            }
        }
        if count > best_count {
            best_count = count;
            best_domain = Some(name.clone());
        }
    }

    if best_count >= MIN_KEYWORD_HITS {
        best_domain
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_domains() -> HashMap<String, DomainConfig> {
        let mut m = HashMap::new();
        m.insert(
            "code".to_string(),
            DomainConfig {
                name: "code".to_string(),
                description: "Programming".to_string(),
                keywords: vec![
                    "function".to_string(),
                    "class".to_string(),
                    "variable".to_string(),
                ],
                protected_terms: vec![],
                rules: vec![],
            },
        );
        m.insert(
            "legal".to_string(),
            DomainConfig {
                name: "legal".to_string(),
                description: "Legal".to_string(),
                keywords: vec![
                    "contract".to_string(),
                    "clause".to_string(),
                    "liability".to_string(),
                ],
                protected_terms: vec![],
                rules: vec![],
            },
        );
        m
    }

    #[test]
    fn test_detect_code_domain() {
        let domains = make_domains();
        let text = "Write a function that takes a variable and returns a class instance";
        assert_eq!(detect_domain(text, &domains), Some("code".to_string()));
    }

    #[test]
    fn test_detect_legal_domain() {
        let domains = make_domains();
        let text = "The contract includes a clause about liability for damages";
        assert_eq!(detect_domain(text, &domains), Some("legal".to_string()));
    }

    #[test]
    fn test_no_domain_detected() {
        let domains = make_domains();
        let text = "Hello world, how are you today?";
        assert_eq!(detect_domain(text, &domains), None);
    }

    #[test]
    fn test_minimum_hits_required() {
        let domains = make_domains();
        // Only one keyword hit -- should not activate
        let text = "This function does something";
        assert_eq!(detect_domain(text, &domains), None);
    }

    #[test]
    fn test_case_insensitive() {
        let domains = make_domains();
        let text = "A FUNCTION with a VARIABLE and a CLASS";
        assert_eq!(detect_domain(text, &domains), Some("code".to_string()));
    }
}
