use axum::http::HeaderMap;

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum Provider {
    OpenAI,
    Anthropic,
}

impl std::fmt::Display for Provider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Provider::OpenAI => write!(f, "openai"),
            Provider::Anthropic => write!(f, "anthropic"),
        }
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct UpstreamConfig {
    pub openai_base_url: String,
    pub anthropic_base_url: String,
}

impl Default for UpstreamConfig {
    fn default() -> Self {
        Self {
            openai_base_url: "https://api.openai.com".to_string(),
            anthropic_base_url: "https://api.anthropic.com".to_string(),
        }
    }
}

/// Detect the provider based on request headers.
///
/// If both `x-api-key` and `anthropic-version` headers are present, the request
/// is treated as Anthropic; otherwise it defaults to OpenAI.
pub fn detect_provider(headers: &HeaderMap) -> Provider {
    let has_x_api_key = headers.contains_key("x-api-key");
    let has_anthropic_version = headers.contains_key("anthropic-version");

    if has_x_api_key && has_anthropic_version {
        Provider::Anthropic
    } else {
        Provider::OpenAI
    }
}

/// Build the full upstream URL for the given provider and path.
pub fn upstream_url(provider: &Provider, path: &str, config: &UpstreamConfig) -> String {
    let base = match provider {
        Provider::OpenAI => &config.openai_base_url,
        Provider::Anthropic => &config.anthropic_base_url,
    };
    format!("{}{}", base.trim_end_matches('/'), path)
}

/// Forward only the relevant headers to the upstream provider.
///
/// Strips hop-by-hop headers (host, connection, transfer-encoding, etc.) and
/// only forwards authentication and content-type headers appropriate for the
/// provider.
pub fn forward_headers(original: &HeaderMap, provider: &Provider) -> HeaderMap {
    let mut forwarded = HeaderMap::new();

    // Headers we always forward if present
    let always_forward = ["content-type", "accept", "user-agent"];

    for name in &always_forward {
        if let Some(val) = original.get(*name) {
            if let Ok(header_name) = axum::http::header::HeaderName::from_bytes(name.as_bytes()) {
                forwarded.insert(header_name, val.clone());
            }
        }
    }

    match provider {
        Provider::OpenAI => {
            // Forward Authorization header
            if let Some(val) = original.get("authorization") {
                forwarded.insert(axum::http::header::AUTHORIZATION, val.clone());
            }
            // Forward OpenAI-specific headers
            for name in ["openai-organization", "openai-project"] {
                if let Some(val) = original.get(name) {
                    if let Ok(header_name) =
                        axum::http::header::HeaderName::from_bytes(name.as_bytes())
                    {
                        forwarded.insert(header_name, val.clone());
                    }
                }
            }
        }
        Provider::Anthropic => {
            // Forward x-api-key and anthropic-version
            if let Some(val) = original.get("x-api-key") {
                if let Ok(header_name) =
                    axum::http::header::HeaderName::from_bytes(b"x-api-key")
                {
                    forwarded.insert(header_name, val.clone());
                }
            }
            if let Some(val) = original.get("anthropic-version") {
                if let Ok(header_name) =
                    axum::http::header::HeaderName::from_bytes(b"anthropic-version")
                {
                    forwarded.insert(header_name, val.clone());
                }
            }
            if let Some(val) = original.get("anthropic-beta") {
                if let Ok(header_name) =
                    axum::http::header::HeaderName::from_bytes(b"anthropic-beta")
                {
                    forwarded.insert(header_name, val.clone());
                }
            }
        }
    }

    forwarded
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_openai() {
        let mut headers = HeaderMap::new();
        headers.insert("authorization", "Bearer sk-xxx".parse().unwrap());
        assert_eq!(detect_provider(&headers), Provider::OpenAI);
    }

    #[test]
    fn test_detect_anthropic() {
        let mut headers = HeaderMap::new();
        headers.insert("x-api-key", "sk-ant-xxx".parse().unwrap());
        headers.insert("anthropic-version", "2023-06-01".parse().unwrap());
        assert_eq!(detect_provider(&headers), Provider::Anthropic);
    }

    #[test]
    fn test_upstream_url_openai() {
        let config = UpstreamConfig::default();
        let url = upstream_url(&Provider::OpenAI, "/v1/chat/completions", &config);
        assert_eq!(url, "https://api.openai.com/v1/chat/completions");
    }

    #[test]
    fn test_upstream_url_anthropic() {
        let config = UpstreamConfig::default();
        let url = upstream_url(&Provider::Anthropic, "/v1/messages", &config);
        assert_eq!(url, "https://api.anthropic.com/v1/messages");
    }

    #[test]
    fn test_forward_headers_openai() {
        let mut original = HeaderMap::new();
        original.insert("authorization", "Bearer sk-xxx".parse().unwrap());
        original.insert("content-type", "application/json".parse().unwrap());
        original.insert("host", "localhost:8080".parse().unwrap());

        let forwarded = forward_headers(&original, &Provider::OpenAI);
        assert!(forwarded.contains_key("authorization"));
        assert!(forwarded.contains_key("content-type"));
        assert!(!forwarded.contains_key("host"));
    }

    #[test]
    fn test_forward_headers_anthropic() {
        let mut original = HeaderMap::new();
        original.insert("x-api-key", "sk-ant-xxx".parse().unwrap());
        original.insert("anthropic-version", "2023-06-01".parse().unwrap());
        original.insert("content-type", "application/json".parse().unwrap());
        original.insert("host", "localhost:8080".parse().unwrap());

        let forwarded = forward_headers(&original, &Provider::Anthropic);
        assert!(forwarded.contains_key("x-api-key"));
        assert!(forwarded.contains_key("anthropic-version"));
        assert!(forwarded.contains_key("content-type"));
        assert!(!forwarded.contains_key("host"));
    }
}
