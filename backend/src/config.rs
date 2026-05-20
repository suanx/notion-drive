use config::{Config as ConfigLib, File};
use serde::Deserialize;
use std::sync::OnceLock;

static CONFIG: OnceLock<Config> = OnceLock::new();

#[derive(Debug, Deserialize, Clone)]
pub struct Config {
    pub database: DatabaseConfig,
    pub jwt: JwtConfig,
    pub storage: StorageConfig,
    pub minio: MinioConfig,
    pub server: ServerConfig,
    pub cors: CorsConfig,
    pub rate_limit: RateLimitConfig,
}

#[derive(Debug, Deserialize, Clone)]
pub struct DatabaseConfig {
    pub url: String,
    pub pool_size: u32,
}

#[derive(Debug, Deserialize, Clone)]
pub struct JwtConfig {
    pub secret: String,
    pub expiration_hours: u64,
}

#[derive(Debug, Deserialize, Clone)]
pub struct StorageConfig {
    pub default_policy_id: String,
    pub local_path: String,
}

#[derive(Debug, Deserialize, Clone)]
pub struct MinioConfig {
    pub endpoint: String,
    pub access_key: String,
    pub secret_key: String,
    pub bucket: String,
    pub use_ssl: bool,
}

#[derive(Debug, Deserialize, Clone)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
    pub base_url: Option<String>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct CorsConfig {
    pub allowed_origins: Vec<String>,
    pub allowed_methods: Vec<String>,
    pub allowed_headers: Vec<String>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct RateLimitConfig {
    pub requests_per_minute: u32,
    pub upload_bandwidth_mbps: u32,
    pub download_bandwidth_mbps: u32,
}

impl Config {
    pub fn init() -> Result<Self, Box<dyn std::error::Error>> {
        let config_lib = ConfigLib::builder()
            .add_source(File::with_name("config/config").required(false))
            .add_source(config::Environment::with_prefix("NOTION").separator("__"))
            .build()?;

        let config: Config = config_lib.try_deserialize()?;
        CONFIG.set(config.clone()).ok();
        
        Ok(config)
    }

    pub fn get() -> &'static Self {
        CONFIG.get().expect("Configuration not initialized")
    }
}
