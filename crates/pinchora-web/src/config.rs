//! The dashboard's configuration, checked and filled in once at start.

use std::{net::SocketAddr, path::PathBuf};

use anyhow::Context as _;
use pinchora_core::infra::config::Secret;
use pinchora_core::infra::config::WebConfig;
use reqwest::Url;

use crate::client_ip::TrustedProxies;

/// [`WebConfig`] resolved: URLs parsed, defaults applied, what is missing listed.
#[derive(Debug, Clone)]
pub struct WebSettings {
    pub bind: SocketAddr,
    /// `scheme://host[:port]` of the public URL; `Origin` headers must match it.
    pub origin: String,
    /// `<public_url>/auth/callback`, as registered in the Developer Portal.
    pub redirect_uri: String,
    pub client_id: u64,
    pub client_secret: Option<Secret>,
    /// Discord's REST base, without a trailing slash.
    pub discord_api_base: String,
    /// Where the browser is sent to authorise: Discord's (or the mock's) `/oauth2/authorize`.
    pub authorize_url: String,
    pub cookie_secure: bool,
    /// Whether `POST /auth/dev-login` works: asked for, on a loopback bind, in a debug build (a
    /// release build never has it: a loopback bind is still reachable through a same-host proxy).
    pub dev_login: bool,
    pub dev_login_unlimited: bool,
    pub static_dir: Option<PathBuf>,
    /// Whether the gateway runs with the privileged Message Content intent, without which the bot cannot
    /// read chat and keyword auto-responders stay locked. Read from the same `gateway.message_content`.
    pub message_content: bool,
    /// Proxies whose `X-Forwarded-For` names the client.
    pub trusted_proxies: TrustedProxies,
    pub operator_ids: Vec<u64>,
    /// Environment variables still needed before anyone can log in.
    pub missing: Vec<&'static str>,
}

impl WebSettings {
    pub fn resolve(web: &WebConfig, application_id: u64) -> anyhow::Result<Self> {
        let public = Url::parse(&web.public_url).context("PINCHORA_WEB__PUBLIC_URL is not a URL")?;
        let origin = public.origin().ascii_serialization();
        let api = Url::parse(&web.discord_api_base).context("PINCHORA_WEB__DISCORD_API_BASE is not a URL")?;
        let client_secret = web
            .client_secret
            .as_ref()
            .map(|s| s.expose().trim().to_owned())
            .filter(|s| !s.is_empty())
            .map(Secret::from);
        let mut missing = Vec::new();
        if client_secret.is_none() {
            missing.push("PINCHORA_WEB__CLIENT_SECRET");
        }
        let dev_login = cfg!(debug_assertions) && web.dev_login && web.bind.ip().is_loopback();
        Ok(Self {
            bind: web.bind,
            redirect_uri: format!("{}/auth/callback", web.public_url.trim_end_matches('/')),
            origin,
            client_id: web.client_id.unwrap_or(application_id),
            client_secret,
            discord_api_base: web.discord_api_base.trim_end_matches('/').to_owned(),
            authorize_url: format!("{}/oauth2/authorize", api.origin().ascii_serialization()),
            cookie_secure: web.cookie_secure.unwrap_or(public.scheme() == "https"),
            dev_login,
            dev_login_unlimited: dev_login && web.dev_login_unlimited,
            static_dir: web.static_dir.clone(),
            message_content: false,
            trusted_proxies: TrustedProxies::parse(&web.trusted_proxies)?,
            operator_ids: web
                .operator_ids
                .split(',')
                .filter(|s| !s.trim().is_empty())
                .map(|s| {
                    let id = s.trim().parse::<u64>().context("invalid PINCHORA_WEB__OPERATOR_IDS")?;
                    anyhow::ensure!(id > 0 && id <= i64::MAX as u64, "invalid operator id");
                    Ok(id)
                })
                .collect::<anyhow::Result<Vec<_>>>()?,
            missing,
        })
    }

    /// Whether Discord login can work: the client secret is set.
    pub fn login_ready(&self) -> bool {
        self.client_secret.is_some()
    }
}

#[cfg(test)]
mod tests {
    use crate::config::*;

    fn web() -> WebConfig {
        WebConfig::default()
    }

    #[test]
    fn defaults_point_at_discord_and_localhost() {
        let s = WebSettings::resolve(&web(), 77).unwrap();
        assert_eq!(s.redirect_uri, "http://localhost:8090/auth/callback");
        assert_eq!(s.origin, "http://localhost:8090");
        assert_eq!(s.authorize_url, "https://discord.com/oauth2/authorize");
        assert_eq!(s.discord_api_base, "https://discord.com/api/v10");
        assert_eq!(s.client_id, 77);
        assert!(!s.cookie_secure);
        assert!(!s.login_ready());
        assert_eq!(s.missing, ["PINCHORA_WEB__CLIENT_SECRET"]);
    }

    #[test]
    fn https_makes_cookies_secure_and_a_secret_enables_login() {
        let mut config = web();
        config.public_url = "https://dash.example.com/".into();
        config.client_secret = Some(String::from("  s3cret ").into());
        config.client_id = Some(5);
        let s = WebSettings::resolve(&config, 77).unwrap();
        assert!(s.cookie_secure);
        assert_eq!(s.redirect_uri, "https://dash.example.com/auth/callback");
        assert_eq!(s.client_secret.as_ref().map(Secret::expose), Some("s3cret"));
        assert!(!format!("{s:?}").contains("s3cret"), "the secret never prints");
        assert_eq!(s.client_id, 5);
        assert!(s.missing.is_empty());

        config.client_secret = Some(String::from(" ").into());
        assert!(!WebSettings::resolve(&config, 77).unwrap().login_ready());
    }

    #[test]
    fn dev_login_needs_a_loopback_bind() {
        let mut config = web();
        config.dev_login = true;
        assert!(
            !WebSettings::resolve(&config, 1).unwrap().dev_login,
            "0.0.0.0 is not loopback"
        );
        config.bind = ([127, 0, 0, 1], 8090).into();
        assert_eq!(
            WebSettings::resolve(&config, 1).unwrap().dev_login,
            cfg!(debug_assertions),
            "release builds never have it"
        );
    }

    #[test]
    fn unlimited_dev_login_requires_explicit_debug_loopback_login() {
        let mut config = web();
        assert!(!WebSettings::resolve(&config, 1).unwrap().dev_login_unlimited);
        config.dev_login_unlimited = true;
        config.bind = ([127, 0, 0, 1], 8090).into();
        assert!(!WebSettings::resolve(&config, 1).unwrap().dev_login_unlimited);
        config.dev_login = true;
        assert_eq!(
            WebSettings::resolve(&config, 1).unwrap().dev_login_unlimited,
            cfg!(debug_assertions)
        );
        config.bind = ([0, 0, 0, 0], 8090).into();
        assert!(!WebSettings::resolve(&config, 1).unwrap().dev_login_unlimited);
    }

    #[test]
    fn trusted_proxies_are_parsed_at_start() {
        let mut config = web();
        assert!(WebSettings::resolve(&config, 1).unwrap().trusted_proxies.is_empty());
        config.trusted_proxies = "172.16.0.0/12".into();
        let s = WebSettings::resolve(&config, 1).unwrap();
        assert!(s.trusted_proxies.contains("172.17.0.1".parse().unwrap()));
        config.trusted_proxies = "the proxy".into();
        assert!(WebSettings::resolve(&config, 1).is_err(), "a typo fails the start");
    }

    #[test]
    fn a_mock_api_base_authorises_on_the_mock() {
        let mut config = web();
        config.discord_api_base = "http://127.0.0.1:4555/api/v10/".into();
        let s = WebSettings::resolve(&config, 1).unwrap();
        assert_eq!(s.discord_api_base, "http://127.0.0.1:4555/api/v10");
        assert_eq!(s.authorize_url, "http://127.0.0.1:4555/oauth2/authorize");
    }
}
