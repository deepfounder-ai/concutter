use std::sync::Arc;
use tokio::sync::RwLock;

use sqz_core::layer1_static::StaticLayer;
use sqz_core::layer2_domain::DomainLayer;
use sqz_core::{CompressorConfig, Compressor, Preprocessor, PreprocessorConfig, RuleLayer};

use crate::error::ProxyError;
use crate::provider::UpstreamConfig;

// ---------------------------------------------------------------------------
// ShadowConfig
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct ShadowConfig {
    pub enabled: bool,
    pub sample_rate: f64,
    pub embedding_model: String,
    pub max_concurrency: usize,
    pub ema_alpha: f64,
}

impl Default for ShadowConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            sample_rate: 0.1,
            embedding_model: "text-embedding-3-small".to_string(),
            max_concurrency: 4,
            ema_alpha: 0.1,
        }
    }
}

// ---------------------------------------------------------------------------
// AppState
// ---------------------------------------------------------------------------

pub struct AppState {
    pub compressor: Arc<RwLock<Compressor>>,
    pub compressor_config: CompressorConfig,
    pub preprocessor_config: PreprocessorConfig,
    pub store: Arc<sqz_store::Store>,
    pub http_client: reqwest::Client,
    pub upstream_config: UpstreamConfig,
    pub shadow_config: ShadowConfig,
    pub compression_enabled: bool,
    pub rules_dir: std::path::PathBuf,
}

impl AppState {
    /// Rebuild the compressor by re-reading all rules from the store and from
    /// the static/domain rule files on disk.
    ///
    /// The new compressor is swapped in under the write lock so that in-flight
    /// requests see a consistent snapshot.
    pub async fn rebuild_compressor(&self) -> Result<(), ProxyError> {
        let config = self.compressor_config.clone();

        // Load static layer from disk
        let static_layer = StaticLayer::load(&config.languages, &self.rules_dir)
            .map_err(|e| ProxyError::ConfigError(format!("failed to load static rules: {e}")))?;

        // Load domain layer from disk
        let domain_layer = DomainLayer::load(&self.rules_dir)
            .map_err(|e| ProxyError::ConfigError(format!("failed to load domain rules: {e}")))?;

        // Load learned rules from the store
        let learned_rows = self
            .store
            .get_learned_rules()
            .await
            .map_err(|e| ProxyError::StoreError(format!("failed to load learned rules: {e}")))?;

        let learned_rules: Vec<sqz_core::Rule> = learned_rows
            .into_iter()
            .map(sqz_core::Rule::from)
            .filter(|r| r.layer == RuleLayer::Learned)
            .collect();

        // Build preprocessor
        let preprocessor = if self.preprocessor_config.enabled {
            Some(
                Preprocessor::build(&self.preprocessor_config)
                    .map_err(|e| {
                        ProxyError::ConfigError(format!("failed to build preprocessor: {e}"))
                    })?,
            )
        } else {
            None
        };

        // Build new compressor
        let new_compressor = Compressor::build_with_preprocessor(
            static_layer.rules(),
            &domain_layer,
            learned_rules,
            config,
            preprocessor,
        )
        .map_err(|e| ProxyError::CompressionError(format!("failed to build compressor: {e}")))?;

        // Swap under write lock
        let mut compressor = self.compressor.write().await;
        *compressor = new_compressor;

        tracing::info!("compressor rebuilt successfully");
        Ok(())
    }
}
