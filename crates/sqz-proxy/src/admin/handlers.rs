use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;

use crate::admin::types::*;
use crate::error::ProxyError;
use crate::state::AppState;

// ---------------------------------------------------------------------------
// Health
// ---------------------------------------------------------------------------

/// `GET /health` - simple liveness check.
pub async fn health_check() -> impl IntoResponse {
    Json(serde_json::json!({"status": "ok"}))
}

// ---------------------------------------------------------------------------
// Rules CRUD
// ---------------------------------------------------------------------------

/// `GET /admin/rules` - list rules with optional filters and pagination.
pub async fn list_rules(
    State(state): State<Arc<AppState>>,
    Query(params): Query<PaginationParams>,
) -> Result<Json<Vec<RuleResponse>>, ProxyError> {
    let rows = state
        .store
        .list_rules(params.layer, params.domain, params.limit, params.offset)
        .await?;

    let rules: Vec<RuleResponse> = rows.into_iter().map(RuleResponse::from).collect();
    Ok(Json(rules))
}

/// `POST /admin/rules` - create a new rule.
pub async fn create_rule(
    State(state): State<Arc<AppState>>,
    Json(req): Json<CreateRuleRequest>,
) -> Result<(StatusCode, Json<RuleResponse>), ProxyError> {
    let now = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();

    let row = sqz_store::RuleRow {
        id: uuid::Uuid::new_v4().to_string(),
        pattern: req.pattern,
        replacement: req.replacement,
        layer: req.layer,
        domain: req.domain,
        confidence: 0.0,
        samples: 0,
        enabled: true,
        priority: req.priority.unwrap_or(0),
        created_at: now.clone(),
        updated_at: now,
    };

    state.store.create_rule(&row).await?;

    Ok((StatusCode::CREATED, Json(RuleResponse::from(row))))
}

/// `PUT /admin/rules/:id` - update an existing rule.
pub async fn update_rule(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(req): Json<UpdateRuleRequest>,
) -> Result<Json<RuleResponse>, ProxyError> {
    // Fetch existing rule
    let mut existing = state.store.get_rule(&id).await?;

    // Apply partial updates
    if let Some(pattern) = req.pattern {
        existing.pattern = pattern;
    }
    if let Some(replacement) = req.replacement {
        existing.replacement = replacement;
    }
    if let Some(enabled) = req.enabled {
        existing.enabled = enabled;
    }
    if let Some(priority) = req.priority {
        existing.priority = priority;
    }
    if let Some(domain) = req.domain {
        existing.domain = Some(domain);
    }

    existing.updated_at = chrono::Utc::now()
        .format("%Y-%m-%d %H:%M:%S")
        .to_string();

    state.store.update_rule(&existing).await?;

    Ok(Json(RuleResponse::from(existing)))
}

/// `DELETE /admin/rules/:id` - delete a rule by id.
pub async fn delete_rule(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<StatusCode, ProxyError> {
    state.store.delete_rule(&id).await?;
    Ok(StatusCode::NO_CONTENT)
}

// ---------------------------------------------------------------------------
// Stats
// ---------------------------------------------------------------------------

/// `GET /admin/stats` - get overall compression statistics.
pub async fn get_stats(
    State(state): State<Arc<AppState>>,
) -> Result<Json<StatsResponse>, ProxyError> {
    let overview = state.store.get_stats_overview().await?;
    Ok(Json(StatsResponse::from(overview)))
}

/// `GET /admin/stats/compression` - get recent compression stat entries.
pub async fn get_compression_stats(
    State(state): State<Arc<AppState>>,
    Query(params): Query<PaginationParams>,
) -> Result<Json<Vec<sqz_store::CompressionStatRow>>, ProxyError> {
    let stats = state
        .store
        .get_compression_stats(params.limit, params.offset)
        .await?;
    Ok(Json(stats))
}

// ---------------------------------------------------------------------------
// Reload
// ---------------------------------------------------------------------------

/// `POST /admin/reload` - rebuild the compressor from current rules.
pub async fn reload_compressor(
    State(state): State<Arc<AppState>>,
) -> Result<Json<ReloadResponse>, ProxyError> {
    let start = std::time::Instant::now();

    state.rebuild_compressor().await?;

    let elapsed_ms = start.elapsed().as_secs_f64() * 1000.0;

    // Count active rules for the response
    let overview = state.store.get_stats_overview().await?;

    Ok(Json(ReloadResponse {
        success: true,
        rules_count: overview.active_rules,
        elapsed_ms,
    }))
}

// ---------------------------------------------------------------------------
// Experiments
// ---------------------------------------------------------------------------

/// `GET /admin/experiments` - list experiments with pagination.
pub async fn list_experiments(
    State(state): State<Arc<AppState>>,
    Query(params): Query<PaginationParams>,
) -> Result<Json<Vec<sqz_store::ExperimentRow>>, ProxyError> {
    let experiments = state
        .store
        .list_experiments(params.limit, params.offset)
        .await?;
    Ok(Json(experiments))
}
