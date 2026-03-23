use std::collections::HashMap;
use std::path::Path;

use crate::types::{CoreError, DomainConfig, DomainRule, Rule, RuleLayer};

// ---------------------------------------------------------------------------
// TOML file schema (internal, maps to the on-disk format)
// ---------------------------------------------------------------------------

#[derive(Debug, serde::Deserialize)]
struct DomainFile {
    domain: DomainSection,
    rules: Option<Vec<TomlRule>>,
}

#[derive(Debug, serde::Deserialize)]
struct DomainSection {
    name: String,
    description: String,
    keywords: Vec<String>,
    protected_terms: Option<ProtectedTermsSection>,
}

#[derive(Debug, serde::Deserialize)]
struct ProtectedTermsSection {
    terms: Vec<String>,
}

#[derive(Debug, serde::Deserialize)]
struct TomlRule {
    pattern: String,
    replacement: String,
}

// ---------------------------------------------------------------------------
// DomainLayer
// ---------------------------------------------------------------------------

/// Layer 2: domain-specific compression rules.
///
/// Each domain is defined in a `.toml` file under `rules/domains/` and contains
/// domain keywords (used for auto-detection), protected terms (never compressed),
/// and pattern/replacement rules.
#[derive(Debug, Clone)]
pub struct DomainLayer {
    domains: HashMap<String, DomainConfig>,
}

impl DomainLayer {
    /// Load all `.toml` domain config files from `rules_dir/domains/`.
    pub fn load(rules_dir: &Path) -> Result<Self, CoreError> {
        let domains_dir = rules_dir.join("domains");
        let mut domains = HashMap::new();

        if !domains_dir.exists() {
            tracing::warn!("domains directory not found: {}", domains_dir.display());
            return Ok(DomainLayer { domains });
        }

        let entries = std::fs::read_dir(&domains_dir)?;
        for entry in entries {
            let entry = entry?;
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("toml") {
                let content = std::fs::read_to_string(&path)?;
                let file: DomainFile = toml::from_str(&content)?;

                let config = DomainConfig {
                    name: file.domain.name.clone(),
                    description: file.domain.description,
                    keywords: file.domain.keywords,
                    protected_terms: file
                        .domain
                        .protected_terms
                        .map(|pt| pt.terms)
                        .unwrap_or_default(),
                    rules: file
                        .rules
                        .unwrap_or_default()
                        .into_iter()
                        .map(|r| DomainRule {
                            pattern: r.pattern,
                            replacement: r.replacement,
                        })
                        .collect(),
                };

                domains.insert(file.domain.name, config);
            }
        }

        Ok(DomainLayer { domains })
    }

    /// Build a `DomainLayer` directly from pre-built configs.
    pub fn load_from_configs(configs: Vec<DomainConfig>) -> Self {
        let domains = configs
            .into_iter()
            .map(|c| (c.name.clone(), c))
            .collect();
        DomainLayer { domains }
    }

    /// Look up a domain by name.
    pub fn get_domain(&self, name: &str) -> Option<&DomainConfig> {
        self.domains.get(name)
    }

    /// Get rules for a specific domain, converted to `Rule` structs.
    pub fn rules_for_domain(&self, domain: &str) -> Vec<Rule> {
        match self.domains.get(domain) {
            Some(config) => config
                .rules
                .iter()
                .enumerate()
                .map(|(i, dr)| Rule {
                    id: format!("domain-{}-{i}", config.name),
                    pattern: dr.pattern.to_lowercase(),
                    replacement: dr.replacement.clone(),
                    layer: RuleLayer::Domain,
                    domain: Some(config.name.clone()),
                    confidence: 1.0,
                    samples: i64::MAX,
                    enabled: true,
                    priority: 200, // domain rules have higher priority than static
                })
                .collect(),
            None => Vec::new(),
        }
    }

    /// Get all rules across all domains.
    pub fn all_rules(&self) -> Vec<Rule> {
        let mut rules = Vec::new();
        for domain_name in self.domains.keys() {
            rules.extend(self.rules_for_domain(domain_name));
        }
        rules
    }

    /// Get protected terms for a specific domain.
    pub fn protected_terms(&self, domain: &str) -> Vec<String> {
        self.domains
            .get(domain)
            .map(|c| c.protected_terms.clone())
            .unwrap_or_default()
    }

    /// List all loaded domain names.
    pub fn domains(&self) -> Vec<String> {
        self.domains.keys().cloned().collect()
    }

    /// Get a reference to the underlying domain configs map.
    pub fn domain_configs(&self) -> &HashMap<String, DomainConfig> {
        &self.domains
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_config() -> DomainConfig {
        DomainConfig {
            name: "test".to_string(),
            description: "Test domain".to_string(),
            keywords: vec!["keyword1".to_string()],
            protected_terms: vec!["protect me".to_string()],
            rules: vec![DomainRule {
                pattern: "long pattern".to_string(),
                replacement: "short".to_string(),
            }],
        }
    }

    #[test]
    fn test_load_from_configs() {
        let layer = DomainLayer::load_from_configs(vec![sample_config()]);
        assert_eq!(layer.domains().len(), 1);
        assert!(layer.get_domain("test").is_some());
    }

    #[test]
    fn test_rules_for_domain() {
        let layer = DomainLayer::load_from_configs(vec![sample_config()]);
        let rules = layer.rules_for_domain("test");
        assert_eq!(rules.len(), 1);
        assert_eq!(rules[0].replacement, "short");
        assert_eq!(rules[0].layer, RuleLayer::Domain);
    }

    #[test]
    fn test_protected_terms() {
        let layer = DomainLayer::load_from_configs(vec![sample_config()]);
        let terms = layer.protected_terms("test");
        assert_eq!(terms, vec!["protect me"]);
    }

    #[test]
    fn test_unknown_domain() {
        let layer = DomainLayer::load_from_configs(vec![sample_config()]);
        assert!(layer.get_domain("nope").is_none());
        assert!(layer.rules_for_domain("nope").is_empty());
        assert!(layer.protected_terms("nope").is_empty());
    }
}
