use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Requests
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateRuleRequest {
    pub pattern: String,
    pub replacement: String,
    pub layer: String,
    #[serde(default)]
    pub domain: Option<String>,
    #[serde(default)]
    pub priority: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateRuleRequest {
    #[serde(default)]
    pub pattern: Option<String>,
    #[serde(default)]
    pub replacement: Option<String>,
    #[serde(default)]
    pub enabled: Option<bool>,
    #[serde(default)]
    pub priority: Option<i32>,
    #[serde(default)]
    pub domain: Option<String>,
}

// ---------------------------------------------------------------------------
// Responses
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuleResponse {
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

impl From<sqz_store::RuleRow> for RuleResponse {
    fn from(row: sqz_store::RuleRow) -> Self {
        RuleResponse {
            id: row.id,
            pattern: row.pattern,
            replacement: row.replacement,
            layer: row.layer,
            domain: row.domain,
            confidence: row.confidence,
            samples: row.samples,
            enabled: row.enabled,
            priority: row.priority,
            created_at: row.created_at,
            updated_at: row.updated_at,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatsResponse {
    pub total_requests: i64,
    pub total_tokens_saved: i64,
    pub avg_compression_ratio: f64,
    pub total_rules: i64,
    pub active_rules: i64,
}

impl From<sqz_store::StatsOverview> for StatsResponse {
    fn from(overview: sqz_store::StatsOverview) -> Self {
        StatsResponse {
            total_requests: overview.total_requests,
            total_tokens_saved: overview.total_tokens_saved,
            avg_compression_ratio: overview.avg_compression_ratio,
            total_rules: overview.total_rules,
            active_rules: overview.active_rules,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReloadResponse {
    pub success: bool,
    pub rules_count: i64,
    pub elapsed_ms: f64,
}

// ---------------------------------------------------------------------------
// Query parameters
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaginationParams {
    #[serde(default = "default_limit")]
    pub limit: i64,
    #[serde(default)]
    pub offset: i64,
    #[serde(default)]
    pub layer: Option<String>,
    #[serde(default)]
    pub domain: Option<String>,
}

fn default_limit() -> i64 {
    50
}
