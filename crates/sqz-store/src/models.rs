use serde::{Deserialize, Serialize};
use sqz_core::{Rule, RuleLayer};
use std::str::FromStr;

// ---------------------------------------------------------------------------
// RuleRow
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuleRow {
    pub id: String,
    pub pattern: String,
    pub replacement: String,
    pub layer: String,
    pub domain: Option<String>,
    pub confidence: f64,
    pub samples: i64,
    pub enabled: bool,
    pub priority: i32,
    pub created_at: String,
    pub updated_at: String,
}

// ---------------------------------------------------------------------------
// CompressionStatRow
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompressionStatRow {
    pub id: String,
    pub request_id: String,
    pub provider: String,
    pub model: String,
    pub domain_detected: Option<String>,
    pub original_tokens: i64,
    pub compressed_tokens: i64,
    pub compression_ratio: f64,
    pub rules_applied: String, // JSON string
    pub elapsed_us: i64,
    pub created_at: String,
}

// ---------------------------------------------------------------------------
// ExperimentRow
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExperimentRow {
    pub id: String,
    pub rule_id: String,
    pub original_prompt: String,
    pub compressed_prompt: String,
    pub original_response: Option<String>,
    pub compressed_response: Option<String>,
    pub similarity_score: Option<f64>,
    pub status: String,
    pub created_at: String,
    pub completed_at: Option<String>,
}

// ---------------------------------------------------------------------------
// RuleStatRow
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuleStatRow {
    pub rule_id: String,
    pub times_applied: i64,
    pub total_tokens_saved: i64,
    pub avg_compression: f64,
    pub last_applied_at: Option<String>,
}

// ---------------------------------------------------------------------------
// StatsOverview
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatsOverview {
    pub total_requests: i64,
    pub total_tokens_saved: i64,
    pub avg_compression_ratio: f64,
    pub total_rules: i64,
    pub active_rules: i64,
}

// ---------------------------------------------------------------------------
// Conversions: RuleRow <-> sqz_core::Rule
// ---------------------------------------------------------------------------

impl From<RuleRow> for Rule {
    fn from(row: RuleRow) -> Self {
        Rule {
            id: row.id,
            pattern: row.pattern,
            replacement: row.replacement,
            layer: RuleLayer::from_str(&row.layer).unwrap_or(RuleLayer::Static),
            domain: row.domain,
            confidence: row.confidence,
            samples: row.samples,
            enabled: row.enabled,
            priority: row.priority,
        }
    }
}

impl From<Rule> for RuleRow {
    fn from(rule: Rule) -> Self {
        let now = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();
        RuleRow {
            id: rule.id,
            pattern: rule.pattern,
            replacement: rule.replacement,
            layer: rule.layer.to_string(),
            domain: rule.domain,
            confidence: rule.confidence,
            samples: rule.samples,
            enabled: rule.enabled,
            priority: rule.priority,
            created_at: now.clone(),
            updated_at: now,
        }
    }
}
