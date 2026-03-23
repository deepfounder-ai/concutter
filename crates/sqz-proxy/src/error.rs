use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde_json::json;

#[derive(Debug, thiserror::Error)]
pub enum ProxyError {
    #[error("upstream error: {0}")]
    UpstreamError(String),

    #[error("deserialization error: {0}")]
    DeserializationError(String),

    #[error("compression error: {0}")]
    CompressionError(String),

    #[error("store error: {0}")]
    StoreError(String),

    #[error("config error: {0}")]
    ConfigError(String),

    #[error("not found")]
    NotFound,

    #[error("internal error: {0}")]
    Internal(String),
}

impl IntoResponse for ProxyError {
    fn into_response(self) -> Response {
        let (status, error_type) = match &self {
            ProxyError::UpstreamError(_) => (StatusCode::BAD_GATEWAY, "upstream_error"),
            ProxyError::DeserializationError(_) => (StatusCode::BAD_REQUEST, "deserialization_error"),
            ProxyError::CompressionError(_) => {
                (StatusCode::INTERNAL_SERVER_ERROR, "compression_error")
            }
            ProxyError::StoreError(_) => (StatusCode::INTERNAL_SERVER_ERROR, "store_error"),
            ProxyError::ConfigError(_) => (StatusCode::INTERNAL_SERVER_ERROR, "config_error"),
            ProxyError::NotFound => (StatusCode::NOT_FOUND, "not_found"),
            ProxyError::Internal(_) => (StatusCode::INTERNAL_SERVER_ERROR, "internal_error"),
        };

        let body = json!({
            "error": {
                "message": self.to_string(),
                "type": error_type,
            }
        });

        (status, axum::Json(body)).into_response()
    }
}

impl From<sqz_store::StoreError> for ProxyError {
    fn from(err: sqz_store::StoreError) -> Self {
        match err {
            sqz_store::StoreError::NotFound => ProxyError::NotFound,
            other => ProxyError::StoreError(other.to_string()),
        }
    }
}
