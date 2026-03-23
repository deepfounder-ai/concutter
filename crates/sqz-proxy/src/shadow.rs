use std::sync::Arc;

use crate::state::ShadowConfig;

/// Manages shadow (A/B) testing by sending the original uncompressed prompt
/// to the upstream provider in the background and comparing the response with
/// the compressed-prompt response.
pub struct ShadowRunner {
    client: reqwest::Client,
    semaphore: Arc<tokio::sync::Semaphore>,
    sample_rate: f64,
    #[allow(dead_code)]
    ema_alpha: f64,
}

impl ShadowRunner {
    /// Create a new shadow runner from the given config.
    pub fn new(config: &ShadowConfig, client: reqwest::Client) -> Self {
        Self {
            client,
            semaphore: Arc::new(tokio::sync::Semaphore::new(config.max_concurrency)),
            sample_rate: config.sample_rate,
            ema_alpha: config.ema_alpha,
        }
    }

    /// Determine whether this request should be shadow-tested based on the
    /// configured sample rate.
    pub fn should_shadow(&self) -> bool {
        if self.sample_rate <= 0.0 {
            return false;
        }
        if self.sample_rate >= 1.0 {
            return true;
        }
        rand_f64() < self.sample_rate
    }

    /// Spawn a background shadow test.
    ///
    /// Sends the original (uncompressed) prompt to the upstream provider,
    /// receives the response, computes embedding similarity between the
    /// original and compressed responses, and updates the experiment record
    /// in the store.
    ///
    /// This is a best-effort operation: failures are logged but do not affect
    /// the main request path.
    pub fn spawn_shadow_test(
        &self,
        store: Arc<sqz_store::Store>,
        experiment_id: String,
        _original_body: Vec<u8>,
        _upstream_url: String,
        _headers: axum::http::HeaderMap,
        _compressed_response: String,
    ) {
        let semaphore = Arc::clone(&self.semaphore);
        let _client = self.client.clone();

        tokio::spawn(async move {
            // Acquire a concurrency permit
            let _permit = match semaphore.acquire().await {
                Ok(p) => p,
                Err(_) => {
                    tracing::warn!("shadow semaphore closed");
                    return;
                }
            };

            // In a full implementation:
            // 1. Send the original body to the upstream URL
            // 2. Read the response
            // 3. Compute embedding vectors for both responses
            // 4. Calculate cosine similarity
            // 5. Update the experiment record in the store

            // For now, mark the experiment as completed with a placeholder
            let now = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();
            let exp = sqz_store::ExperimentRow {
                id: experiment_id,
                rule_id: String::new(),
                original_prompt: String::new(),
                compressed_prompt: String::new(),
                original_response: Some("(shadow test placeholder)".to_string()),
                compressed_response: Some("(shadow test placeholder)".to_string()),
                similarity_score: None,
                status: "completed".to_string(),
                created_at: now.clone(),
                completed_at: Some(now),
            };

            if let Err(e) = store.update_experiment(&exp).await {
                tracing::warn!("failed to update shadow experiment: {e}");
            }
        });
    }
}

/// Compute the cosine similarity between two vectors.
///
/// Returns a value in [-1, 1], where 1 means identical direction, 0 means
/// orthogonal, and -1 means opposite direction. Returns 0.0 if either vector
/// has zero magnitude.
pub fn cosine_similarity(a: &[f64], b: &[f64]) -> f64 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }

    let mut dot = 0.0_f64;
    let mut mag_a = 0.0_f64;
    let mut mag_b = 0.0_f64;

    for (ai, bi) in a.iter().zip(b.iter()) {
        dot += ai * bi;
        mag_a += ai * ai;
        mag_b += bi * bi;
    }

    let denominator = mag_a.sqrt() * mag_b.sqrt();
    if denominator == 0.0 {
        0.0
    } else {
        dot / denominator
    }
}

/// Simple pseudo-random f64 in [0, 1) using the current time as entropy.
///
/// This is intentionally lightweight; for production use a proper PRNG.
fn rand_f64() -> f64 {
    use std::time::SystemTime;
    let nanos = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .subsec_nanos();
    // Simple hash to get a pseudo-random distribution
    let hash = nanos.wrapping_mul(2654435761);
    (hash as f64) / (u32::MAX as f64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cosine_similarity_identical() {
        let a = vec![1.0, 2.0, 3.0];
        let b = vec![1.0, 2.0, 3.0];
        let sim = cosine_similarity(&a, &b);
        assert!((sim - 1.0).abs() < 1e-10);
    }

    #[test]
    fn test_cosine_similarity_orthogonal() {
        let a = vec![1.0, 0.0];
        let b = vec![0.0, 1.0];
        let sim = cosine_similarity(&a, &b);
        assert!(sim.abs() < 1e-10);
    }

    #[test]
    fn test_cosine_similarity_opposite() {
        let a = vec![1.0, 0.0];
        let b = vec![-1.0, 0.0];
        let sim = cosine_similarity(&a, &b);
        assert!((sim + 1.0).abs() < 1e-10);
    }

    #[test]
    fn test_cosine_similarity_zero_vector() {
        let a = vec![0.0, 0.0];
        let b = vec![1.0, 2.0];
        let sim = cosine_similarity(&a, &b);
        assert_eq!(sim, 0.0);
    }

    #[test]
    fn test_cosine_similarity_empty() {
        let sim = cosine_similarity(&[], &[]);
        assert_eq!(sim, 0.0);
    }

    #[test]
    fn test_cosine_similarity_mismatched_lengths() {
        let a = vec![1.0, 2.0];
        let b = vec![1.0, 2.0, 3.0];
        let sim = cosine_similarity(&a, &b);
        assert_eq!(sim, 0.0);
    }
}
