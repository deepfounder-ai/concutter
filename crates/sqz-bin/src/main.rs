use anyhow::Result;
use clap::Parser;
use std::sync::Arc;
use tokio::sync::RwLock;

mod cli;
mod config;

use cli::Cli;
use config::AppConfig;

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    // Load config
    let mut config = AppConfig::load(&cli.config)?;

    // Env overrides (after TOML, before CLI)
    config.apply_env_overrides();

    // CLI overrides
    if let Some(host) = cli.host {
        config.server.host = host;
    }
    if let Some(port) = cli.port {
        config.server.port = port;
    }
    if let Some(db) = cli.db {
        config.database.path = db;
    }
    if let Some(level) = cli.log_level {
        config.logging.level = level;
    }

    // Init logging
    let env_filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(&config.logging.level));

    if config.logging.json {
        tracing_subscriber::fmt()
            .json()
            .with_env_filter(env_filter)
            .init();
    } else {
        tracing_subscriber::fmt()
            .with_env_filter(env_filter)
            .init();
    }

    tracing::info!("Starting sqz proxy v{}", env!("CARGO_PKG_VERSION"));

    // Init database
    let store = sqz_store::Store::new(&config.database.path).await?;
    let store = Arc::new(store);

    // Determine rules directory
    let rules_dir = std::env::current_dir()?.join("rules");

    // Build compressor
    let compressor_config = sqz_core::CompressorConfig {
        confidence_threshold: config.compression.confidence_threshold,
        min_samples: config.compression.min_samples,
        languages: config.compression.languages.clone(),
        layers_enabled: sqz_core::LayersEnabled {
            static_enabled: config.compression.layers.static_enabled,
            domain_enabled: config.compression.layers.domain_enabled,
            learned_enabled: config.compression.layers.learned_enabled,
        },
    };

    // Load layers
    let static_layer = sqz_core::layer1_static::StaticLayer::load(
        &compressor_config.languages,
        &rules_dir,
    )
    .unwrap_or_else(|e| {
        tracing::warn!("Failed to load static rules: {}, using empty set", e);
        sqz_core::layer1_static::StaticLayer::load_from_strings(&[])
    });

    let domain_layer = sqz_core::layer2_domain::DomainLayer::load(&rules_dir).unwrap_or_else(
        |e| {
            tracing::warn!("Failed to load domain rules: {}, using empty set", e);
            sqz_core::layer2_domain::DomainLayer::load_from_configs(vec![])
        },
    );

    // Get learned rules from DB
    let learned_rules: Vec<sqz_core::Rule> = store
        .get_learned_rules()
        .await
        .unwrap_or_default()
        .into_iter()
        .map(|r| r.into())
        .collect();

    let learned_layer = sqz_core::layer3_learned::LearnedLayer::new(
        learned_rules,
        compressor_config.confidence_threshold,
        compressor_config.min_samples,
    );

    // Build preprocessor
    let preprocessor = if config.compression.preprocessor.enabled {
        Some(sqz_core::Preprocessor::build(
            &sqz_core::PreprocessorConfig {
                enabled: config.compression.preprocessor.enabled,
                structural_enabled: config.compression.preprocessor.structural_enabled,
                semantic_enabled: config.compression.preprocessor.semantic_enabled,
            },
        )?)
    } else {
        None
    };

    let compressor = sqz_core::Compressor::build_with_preprocessor(
        static_layer.rules(),
        &domain_layer,
        learned_layer.active_rules(),
        compressor_config.clone(),
        preprocessor,
    )?;

    let compressor = Arc::new(RwLock::new(compressor));

    // Build HTTP client
    let http_client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(
            config.server.request_timeout,
        ))
        .build()?;

    // Build app state
    let state = Arc::new(sqz_proxy::AppState {
        compressor,
        compressor_config,
        preprocessor_config: sqz_core::PreprocessorConfig {
            enabled: config.compression.preprocessor.enabled,
            structural_enabled: config.compression.preprocessor.structural_enabled,
            semantic_enabled: config.compression.preprocessor.semantic_enabled,
        },
        store: store.clone(),
        http_client,
        upstream_config: sqz_proxy::provider::UpstreamConfig {
            openai_base_url: config.upstream.openai_base_url.clone(),
            anthropic_base_url: config.upstream.anthropic_base_url.clone(),
        },
        shadow_config: sqz_proxy::state::ShadowConfig {
            enabled: config.shadow.enabled,
            sample_rate: config.shadow.sample_rate,
            embedding_model: config.shadow.embedding_model.clone(),
            max_concurrency: config.shadow.max_concurrency,
            ema_alpha: config.shadow.ema_alpha,
        },
        compression_enabled: config.compression.enabled,
        rules_dir: rules_dir.clone(),
    });

    // Start auto-reload task if interval > 0
    if config.reload.interval > 0 {
        let reload_state = state.clone();
        let interval = config.reload.interval;
        tokio::spawn(async move {
            let mut ticker =
                tokio::time::interval(std::time::Duration::from_secs(interval));
            ticker.tick().await; // skip first immediate tick
            loop {
                ticker.tick().await;
                tracing::debug!("Auto-reloading compressor");
                if let Err(e) = reload_state.rebuild_compressor().await {
                    tracing::error!("Auto-reload failed: {}", e);
                }
            }
        });
    }

    // Start server
    sqz_proxy::run_server(state, &config.server.host, config.server.port).await?;

    Ok(())
}
