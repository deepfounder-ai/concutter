use std::sync::Arc;

use axum::routing::{get, post, put};
use axum::{middleware, Router};

use crate::state::AppState;

/// Build the complete axum router with all routes and middleware.
pub fn build_router(state: Arc<AppState>) -> Router {
    Router::new()
        // OpenAI compatible endpoint
        .route(
            "/v1/chat/completions",
            post(crate::openai::handler::chat_completions),
        )
        // Anthropic compatible endpoint
        .route(
            "/v1/messages",
            post(crate::anthropic::handler::messages),
        )
        // Admin: rules CRUD
        .route(
            "/admin/rules",
            get(crate::admin::handlers::list_rules).post(crate::admin::handlers::create_rule),
        )
        .route(
            "/admin/rules/{id}",
            put(crate::admin::handlers::update_rule).delete(crate::admin::handlers::delete_rule),
        )
        // Admin: stats
        .route("/admin/stats", get(crate::admin::handlers::get_stats))
        .route(
            "/admin/stats/compression",
            get(crate::admin::handlers::get_compression_stats),
        )
        // Admin: reload compressor
        .route(
            "/admin/reload",
            post(crate::admin::handlers::reload_compressor),
        )
        // Admin: experiments
        .route(
            "/admin/experiments",
            get(crate::admin::handlers::list_experiments),
        )
        // Health check
        .route("/health", get(crate::admin::handlers::health_check))
        // Middleware
        .layer(middleware::from_fn(crate::middleware::timing_layer))
        .layer(middleware::from_fn(crate::middleware::request_id_layer))
        // State
        .with_state(state)
}
