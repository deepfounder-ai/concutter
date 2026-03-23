use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

// ---------------------------------------------------------------------------
// CoreError
// ---------------------------------------------------------------------------

#[derive(Debug, thiserror::Error)]
pub enum CoreError {
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("TOML parse error: {0}")]
    TomlError(#[from] toml::de::Error),

    #[error("Regex error: {0}")]
    RegexError(#[from] regex::Error),

    #[error("{0}")]
    Other(String),
}

// ---------------------------------------------------------------------------
// RuleLayer
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum RuleLayer {
    Static,
    Domain,
    Learned,
}

impl fmt::Display for RuleLayer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RuleLayer::Static => write!(f, "static"),
            RuleLayer::Domain => write!(f, "domain"),
            RuleLayer::Learned => write!(f, "learned"),
        }
    }
}

impl FromStr for RuleLayer {
    type Err = CoreError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "static" => Ok(RuleLayer::Static),
            "domain" => Ok(RuleLayer::Domain),
            "learned" => Ok(RuleLayer::Learned),
            other => Err(CoreError::Other(format!("unknown rule layer: {other}"))),
        }
    }
}

// ---------------------------------------------------------------------------
// Rule
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Rule {
    pub id: String,
    pub pattern: String,
    pub replacement: String,
    pub layer: RuleLayer,
    pub domain: Option<String>,
    pub confidence: f64,
    pub samples: i64,
    pub enabled: bool,
    pub priority: i32,
}

impl Default for Rule {
    fn default() -> Self {
        Self {
            id: String::new(),
            pattern: String::new(),
            replacement: String::new(),
            layer: RuleLayer::Static,
            domain: None,
            confidence: 1.0,
            samples: 0,
            enabled: true,
            priority: 0,
        }
    }
}

// ---------------------------------------------------------------------------
// Domain configuration (parsed from TOML files)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DomainConfig {
    pub name: String,
    pub description: String,
    pub keywords: Vec<String>,
    pub protected_terms: Vec<String>,
    pub rules: Vec<DomainRule>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DomainRule {
    pub pattern: String,
    pub replacement: String,
}

// ---------------------------------------------------------------------------
// CompressionResult
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompressionResult {
    pub text: String,
    pub original_tokens: usize,
    pub compressed_tokens: usize,
    pub compression_ratio: f64,
    pub rules_applied: Vec<String>,
    pub elapsed_us: u64,
    pub domain_detected: Option<String>,
}

// ---------------------------------------------------------------------------
// ProtectedRegion
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProtectedRegion {
    pub start: usize,
    pub end: usize,
}

// ---------------------------------------------------------------------------
// CompressorConfig
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompressorConfig {
    pub confidence_threshold: f64,
    pub min_samples: i64,
    pub languages: Vec<String>,
    pub layers_enabled: LayersEnabled,
}

impl Default for CompressorConfig {
    fn default() -> Self {
        Self {
            confidence_threshold: 0.8,
            min_samples: 10,
            languages: vec![
                "en".to_string(),
                "ru".to_string(),
                "es".to_string(),
                "de".to_string(),
                "fr".to_string(),
                "pt".to_string(),
                "zh".to_string(),
                "ja".to_string(),
                "ko".to_string(),
                "ar".to_string(),
                "hi".to_string(),
            ],
            layers_enabled: LayersEnabled::default(),
        }
    }
}

// ---------------------------------------------------------------------------
// LayersEnabled
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LayersEnabled {
    pub static_enabled: bool,
    pub domain_enabled: bool,
    pub learned_enabled: bool,
}

impl Default for LayersEnabled {
    fn default() -> Self {
        Self {
            static_enabled: true,
            domain_enabled: true,
            learned_enabled: true,
        }
    }
}

// ---------------------------------------------------------------------------
// CompressionLevel
// ---------------------------------------------------------------------------

/// Controls how aggressively a message is compressed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompressionLevel {
    /// Do not compress (last user message, tool results).
    Skip,
    /// Preprocessor only — no Aho-Corasick stopword removal (system prompts).
    Light,
    /// Full pipeline — preprocessor + Aho-Corasick (old messages).
    Normal,
}

// ---------------------------------------------------------------------------
// PreprocessorConfig (re-exported from preprocessor module)
// ---------------------------------------------------------------------------

pub use crate::preprocessor::PreprocessorConfig;
