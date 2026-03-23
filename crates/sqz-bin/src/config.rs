use serde::Deserialize;

// ---------------------------------------------------------------------------
// Default value functions
// ---------------------------------------------------------------------------

fn default_host() -> String {
    "127.0.0.1".to_string()
}

fn default_port() -> u16 {
    8080
}

fn default_timeout() -> u64 {
    120
}

fn default_openai_url() -> String {
    "https://api.openai.com".to_string()
}

fn default_anthropic_url() -> String {
    "https://api.anthropic.com".to_string()
}

fn default_db_path() -> String {
    "concutter.db".to_string()
}

fn default_true() -> bool {
    true
}

fn default_confidence() -> f64 {
    0.8
}

fn default_min_samples() -> i64 {
    10
}

fn default_language() -> String {
    "en".to_string()
}

fn default_languages() -> Vec<String> {
    vec!["en".to_string()]
}

fn default_sample_rate() -> f64 {
    0.1
}

fn default_embedding_model() -> String {
    "text-embedding-3-small".to_string()
}

fn default_max_concurrency() -> usize {
    5
}

fn default_ema_alpha() -> f64 {
    0.1
}

fn default_log_level() -> String {
    "info".to_string()
}

fn default_reload_interval() -> u64 {
    300
}

fn default_server() -> ServerConfig {
    ServerConfig::default()
}

// ---------------------------------------------------------------------------
// AppConfig
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize, Clone)]
pub struct AppConfig {
    #[serde(default = "default_server")]
    pub server: ServerConfig,
    #[serde(default)]
    pub upstream: UpstreamConfig,
    #[serde(default)]
    pub database: DatabaseConfig,
    #[serde(default)]
    pub compression: CompressionConfig,
    #[serde(default)]
    pub shadow: ShadowAppConfig,
    #[serde(default)]
    pub logging: LoggingConfig,
    #[serde(default)]
    pub admin: AdminConfig,
    #[serde(default)]
    pub reload: ReloadConfig,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            server: ServerConfig::default(),
            upstream: UpstreamConfig::default(),
            database: DatabaseConfig::default(),
            compression: CompressionConfig::default(),
            shadow: ShadowAppConfig::default(),
            logging: LoggingConfig::default(),
            admin: AdminConfig::default(),
            reload: ReloadConfig::default(),
        }
    }
}

impl AppConfig {
    pub fn load(path: &str) -> anyhow::Result<Self> {
        if std::path::Path::new(path).exists() {
            let content = std::fs::read_to_string(path)?;
            let config: AppConfig = toml::from_str(&content)?;
            Ok(config)
        } else {
            tracing::warn!("Config file not found at {}, using defaults", path);
            Ok(AppConfig::default())
        }
    }
}

// ---------------------------------------------------------------------------
// ServerConfig
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize, Clone)]
pub struct ServerConfig {
    #[serde(default = "default_host")]
    pub host: String,
    #[serde(default = "default_port")]
    pub port: u16,
    #[serde(default = "default_timeout")]
    pub request_timeout: u64,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            host: default_host(),
            port: default_port(),
            request_timeout: default_timeout(),
        }
    }
}

// ---------------------------------------------------------------------------
// UpstreamConfig
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize, Clone)]
pub struct UpstreamConfig {
    #[serde(default = "default_openai_url")]
    pub openai_base_url: String,
    #[serde(default = "default_anthropic_url")]
    pub anthropic_base_url: String,
}

impl Default for UpstreamConfig {
    fn default() -> Self {
        Self {
            openai_base_url: default_openai_url(),
            anthropic_base_url: default_anthropic_url(),
        }
    }
}

// ---------------------------------------------------------------------------
// DatabaseConfig
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize, Clone)]
pub struct DatabaseConfig {
    #[serde(default = "default_db_path")]
    pub path: String,
}

impl Default for DatabaseConfig {
    fn default() -> Self {
        Self {
            path: default_db_path(),
        }
    }
}

// ---------------------------------------------------------------------------
// CompressionConfig
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize, Clone)]
pub struct CompressionConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_confidence")]
    pub confidence_threshold: f64,
    #[serde(default = "default_min_samples")]
    pub min_samples: i64,
    #[serde(default = "default_language")]
    pub default_language: String,
    #[serde(default = "default_languages")]
    pub languages: Vec<String>,
    #[serde(default)]
    pub layers: LayersConfig,
}

impl Default for CompressionConfig {
    fn default() -> Self {
        Self {
            enabled: default_true(),
            confidence_threshold: default_confidence(),
            min_samples: default_min_samples(),
            default_language: default_language(),
            languages: default_languages(),
            layers: LayersConfig::default(),
        }
    }
}

// ---------------------------------------------------------------------------
// LayersConfig
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize, Clone)]
pub struct LayersConfig {
    #[serde(default = "default_true")]
    pub static_enabled: bool,
    #[serde(default = "default_true")]
    pub domain_enabled: bool,
    #[serde(default = "default_true")]
    pub learned_enabled: bool,
}

impl Default for LayersConfig {
    fn default() -> Self {
        Self {
            static_enabled: default_true(),
            domain_enabled: default_true(),
            learned_enabled: default_true(),
        }
    }
}

// ---------------------------------------------------------------------------
// ShadowAppConfig
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize, Clone)]
pub struct ShadowAppConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_sample_rate")]
    pub sample_rate: f64,
    #[serde(default = "default_embedding_model")]
    pub embedding_model: String,
    #[serde(default = "default_max_concurrency")]
    pub max_concurrency: usize,
    #[serde(default = "default_ema_alpha")]
    pub ema_alpha: f64,
}

impl Default for ShadowAppConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            sample_rate: default_sample_rate(),
            embedding_model: default_embedding_model(),
            max_concurrency: default_max_concurrency(),
            ema_alpha: default_ema_alpha(),
        }
    }
}

// ---------------------------------------------------------------------------
// LoggingConfig
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize, Clone)]
pub struct LoggingConfig {
    #[serde(default = "default_log_level")]
    pub level: String,
    #[serde(default)]
    pub json: bool,
}

impl Default for LoggingConfig {
    fn default() -> Self {
        Self {
            level: default_log_level(),
            json: false,
        }
    }
}

// ---------------------------------------------------------------------------
// AdminConfig
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize, Clone)]
pub struct AdminConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    pub api_key: Option<String>,
}

impl Default for AdminConfig {
    fn default() -> Self {
        Self {
            enabled: default_true(),
            api_key: None,
        }
    }
}

// ---------------------------------------------------------------------------
// ReloadConfig
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize, Clone)]
pub struct ReloadConfig {
    #[serde(default = "default_reload_interval")]
    pub interval: u64,
}

impl Default for ReloadConfig {
    fn default() -> Self {
        Self {
            interval: default_reload_interval(),
        }
    }
}
