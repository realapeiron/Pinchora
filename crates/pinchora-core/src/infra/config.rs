use std::{fmt, net::SocketAddr, path::PathBuf};

use figment::{
    Figment,
    providers::{Env, Format, Toml},
};
use serde::{Deserialize, Serialize};

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
    #[serde(default)]
    pub web: WebConfig,
    #[serde(default)]
    pub roblox: RobloxConfig,
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

/// A string that never shows up in logs or `Debug` output. It still serializes as itself, for
/// stores that must keep it (the dashboard's sessions in Redis).
#[derive(Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(transparent)]
pub struct Secret(String);

impl Secret {
    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl From<String> for Secret {
    fn from(value: String) -> Self {
        Self(value)
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
    /// Listen for message text (privileged "Message Content" intent): without it the bot cannot read chat,
    /// so keyword auto-responders stay locked, text commands only work after an @mention of the bot, the
    /// server log shows no message text and `/purge` cannot filter by text, links or files. Discord needs
    /// verification for it above 100 servers.
    pub message_content: bool,
}

impl Default for GatewayConfig {
    fn default() -> Self {
        Self {
            shards: None,
            cluster_id: None,
            cluster_count: 1,
            member_events: true,
            message_content: false,
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

/// The web dashboard (`pinchora web`). Only that role reads it; without a client secret it serves a
/// setup page instead of the login.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct WebConfig {
    pub bind: SocketAddr,
    /// Where browsers reach the dashboard; the OAuth redirect is `<public_url>/auth/callback`.
    pub public_url: String,
    /// The OAuth2 client id; defaults to `discord.application_id`.
    pub client_id: Option<u64>,
    /// The OAuth2 client secret from the Developer Portal.
    pub client_secret: Option<Secret>,
    /// Discord's REST base; tests point it at a mock.
    pub discord_api_base: String,
    /// Mark cookies `Secure`; defaults to whether `public_url` is https.
    pub cookie_secure: Option<bool>,
    /// Allow `POST /auth/dev-login` (sign in as anyone, without Discord). Only honoured by debug
    /// builds on a loopback bind.
    pub dev_login: bool,
    /// Skip the dev-login rate limit for local test suites. Only honoured when dev login is enabled
    /// in a debug build on loopback; other authentication routes keep their production limits.
    pub dev_login_unlimited: bool,
    /// Serve the SPA from this folder instead of the copy built into the binary.
    pub static_dir: Option<PathBuf>,
    /// Reverse proxies in front of the dashboard, as comma-separated addresses or CIDR ranges
    /// (`172.16.0.0/12, 10.0.0.1`). Only requests from these may name the client with
    /// `X-Forwarded-For`; everyone else is who the connection says.
    pub trusted_proxies: String,
    /// Comma-separated Reef moderation operator account ids.
    pub operator_ids: String,
}

impl Default for WebConfig {
    fn default() -> Self {
        Self {
            bind: ([0, 0, 0, 0], 8090).into(),
            public_url: "http://localhost:8090".into(),
            client_id: None,
            client_secret: None,
            discord_api_base: "https://discord.com/api/v10".into(),
            cookie_secure: None,
            dev_login: false,
            dev_login_unlimited: false,
            static_dir: None,
            trusted_proxies: String::new(),
            operator_ids: String::new(),
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

/// Reserved for Roblox OAuth. Profile-code verification uses no credentials.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct RobloxConfig {
    pub client_id: Option<String>,
    pub client_secret: Option<Secret>,
}
