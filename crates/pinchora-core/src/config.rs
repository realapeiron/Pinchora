use std::{fmt, net::SocketAddr};

use figment::{
    Figment,
    providers::{Env, Format, Toml},
};
use serde::Deserialize;

/// Runtime configuration.
///
/// Sources, later ones win: `pinchora.toml` (or the file named by `PINCHORA_CONFIG`),
/// then `PINCHORA_*` environment variables with `__` as the section separator,
/// e.g. `PINCHORA_DISCORD__TOKEN`, `PINCHORA_GATEWAY__CLUSTER_COUNT`.
#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    pub discord: DiscordConfig,
    #[serde(default)]
    pub gateway: GatewayConfig,
    #[serde(default)]
    pub interactions: InteractionsConfig,
    pub database: DatabaseConfig,
    pub redis: RedisConfig,
    #[serde(default)]
    pub ops: OpsConfig,
    #[serde(default)]
    pub log: LogConfig,
}

impl Config {
    pub fn load() -> anyhow::Result<Self> {
        let path = std::env::var("PINCHORA_CONFIG").unwrap_or_else(|_| "pinchora.toml".into());
        Ok(Figment::new()
            .merge(Toml::file(path))
            .merge(Env::prefixed("PINCHORA_").split("__"))
            .extract()?)
    }
}

/// A string that never shows up in logs or `Debug` output.
#[derive(Clone, Deserialize)]
#[serde(transparent)]
pub struct Secret(String);

impl Secret {
    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Secret(***)")
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct DiscordConfig {
    pub token: Secret,
    pub application_id: u64,
    /// Hex Ed25519 public key from the Developer Portal. Required by the HTTP interactions server.
    pub public_key: Option<String>,
    /// Address of a twilight http-proxy (`http-proxy:3000`), so every pod shares one REST ratelimit view.
    pub http_proxy: Option<String>,
    /// Register commands to this guild only; guild commands update instantly while developing.
    pub dev_guild_id: Option<u64>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct GatewayConfig {
    /// Total shards across every cluster. `None` uses Discord's recommendation.
    pub shards: Option<u32>,
    /// This process's cluster. `None` reads the StatefulSet ordinal from `HOSTNAME`.
    pub cluster_id: Option<u16>,
    pub cluster_count: u16,
    /// Listen for `GUILD_MEMBER_ADD` (privileged "Server Members" intent) for welcome cards.
    pub member_events: bool,
}

impl Default for GatewayConfig {
    fn default() -> Self {
        Self {
            shards: None,
            cluster_id: None,
            cluster_count: 1,
            member_events: true,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct InteractionsConfig {
    pub bind: SocketAddr,
}

impl Default for InteractionsConfig {
    fn default() -> Self {
        Self {
            bind: ([0, 0, 0, 0], 8080).into(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct DatabaseConfig {
    pub url: Secret,
    #[serde(default = "default_db_connections")]
    pub max_connections: u32,
}

fn default_db_connections() -> u32 {
    20
}

#[derive(Debug, Clone, Deserialize)]
pub struct RedisConfig {
    pub url: Secret,
    #[serde(default = "default_redis_pool")]
    pub pool_size: usize,
}

fn default_redis_pool() -> usize {
    4
}

/// Metrics and health probes.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct OpsConfig {
    pub bind: SocketAddr,
}

impl Default for OpsConfig {
    fn default() -> Self {
        Self {
            bind: ([0, 0, 0, 0], 9000).into(),
        }
    }
}

#[derive(Debug, Clone, Copy, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum LogFormat {
    #[default]
    Pretty,
    Json,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct LogConfig {
    pub format: LogFormat,
    pub filter: String,
}

impl Default for LogConfig {
    fn default() -> Self {
        Self {
            format: LogFormat::Pretty,
            filter: "info,pinchora=debug".into(),
        }
    }
}
