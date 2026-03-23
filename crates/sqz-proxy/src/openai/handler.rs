use std::sync::Arc;

use axum::body::Body;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::Response;
use bytes::Bytes;

use sqz_core::CompressionLevel;

use crate::error::ProxyError;
use crate::openai::stream::forward_stream;
use crate::openai::types::ChatCompletionRequest;
use crate::provider::{self, Provider};
use crate::state::AppState;

/// Handler for `POST /v1/chat/completions`.
///
/// Deserializes the request, optionally compresses user and system message
/// text, forwards the request to the upstream OpenAI API, and returns the
/// response (streaming or non-streaming).
pub async fn chat_completions(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, ProxyError> {
    let request_id = headers
        .get("x-request-id")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("unknown")
        .to_string();

    // Deserialize
    let mut req: ChatCompletionRequest = serde_json::from_slice(&body)
        .map_err(|e| ProxyError::DeserializationError(format!("invalid request body: {e}")))?;

    let model = req.model.clone();
    let is_streaming = req.stream.unwrap_or(false);

    // Compress messages with role-aware compression levels:
    // - Last user message: Skip (preserve user intent)
    // - Older user messages: Normal (full compression)
    // - System messages: Light (preprocessor only, no stopword removal)
    // - Assistant/tool: Skip (not touched)
    let mut compression_results = Vec::new();
    if state.compression_enabled {
        let compressor = state.compressor.read().await;
        let last_user_idx = req.messages.iter().rposition(|m| m.role == "user");

        for (idx, msg) in req.messages.iter_mut().enumerate() {
            let level = match msg.role.as_str() {
                "user" if Some(idx) == last_user_idx => CompressionLevel::Skip,
                "user" => CompressionLevel::Normal,
                "system" => CompressionLevel::Light,
                _ => continue,
            };
            for text in msg.content.text_mut() {
                let result = compressor.compress_with_level(text, None, level);
                *text = result.text.clone();
                compression_results.push(result);
            }
        }
    }

    // Re-serialize the (potentially compressed) request
    let compressed_body = serde_json::to_vec(&req)
        .map_err(|e| ProxyError::Internal(format!("failed to serialize request: {e}")))?;

    // Build upstream request
    let upstream_url =
        provider::upstream_url(&Provider::OpenAI, "/v1/chat/completions", &state.upstream_config);
    let forwarded_headers = provider::forward_headers(&headers, &Provider::OpenAI);

    let upstream_resp = state
        .http_client
        .post(&upstream_url)
        .headers(forwarded_headers)
        .body(compressed_body)
        .send()
        .await
        .map_err(|e| ProxyError::UpstreamError(format!("upstream request failed: {e}")))?;

    // Fire-and-forget: record compression stats
    if !compression_results.is_empty() {
        let store = Arc::clone(&state.store);
        let req_id = request_id.clone();
        let model_clone = model.clone();
        tokio::spawn(async move {
            for result in compression_results {
                let stat = sqz_store::CompressionStatRow {
                    id: uuid::Uuid::new_v4().to_string(),
                    request_id: req_id.clone(),
                    provider: "openai".to_string(),
                    model: model_clone.clone(),
                    domain_detected: result.domain_detected,
                    original_tokens: result.original_tokens as i64,
                    compressed_tokens: result.compressed_tokens as i64,
                    compression_ratio: result.compression_ratio,
                    rules_applied: serde_json::to_string(&result.rules_applied)
                        .unwrap_or_default(),
                    elapsed_us: result.elapsed_us as i64,
                    created_at: chrono::Utc::now()
                        .format("%Y-%m-%d %H:%M:%S")
                        .to_string(),
                };
                if let Err(e) = store.record_compression_stat(&stat).await {
                    tracing::warn!("failed to record compression stat: {e}");
                }
            }
        });
    }

    // Return response
    if is_streaming {
        Ok(forward_stream(upstream_resp))
    } else {
        // Non-streaming: forward status and body
        let status = StatusCode::from_u16(upstream_resp.status().as_u16())
            .unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
        let upstream_headers = upstream_resp.headers().clone();
        let response_body = upstream_resp
            .bytes()
            .await
            .map_err(|e| ProxyError::UpstreamError(format!("failed to read upstream body: {e}")))?;

        let mut builder = Response::builder().status(status);

        // Forward content-type from upstream
        if let Some(ct) = upstream_headers.get("content-type") {
            builder = builder.header("content-type", ct);
        }

        builder
            .body(Body::from(response_body))
            .map_err(|e| ProxyError::Internal(format!("failed to build response: {e}")))
    }
}
