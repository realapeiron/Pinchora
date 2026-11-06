//! AutoMod's shared limits, profiles and pure matchers. No Discord or database access.
mod matcher;
mod native;
pub use matcher::{Input, matches, words};
pub use native::{NativeRule, native_rule};

use crate::{i18n::Lang, t};
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};

pub const MAX_RULES: usize = 30;
pub const KINDS: [&str; 16] = [
    "spam",
    "duplicate",
    "repeat",
    "bad_words",
    "invites",
    "links",
    "caps",
    "emoji",
    "mentions",
    "attachments",
    "newlines",
    "zalgo",
    "media_only",
    "youtube_only",
    "honeypot",
    "presets",
];
pub const ACTIONS: [&str; 8] = ["delete", "warn", "timeout", "kick", "ban", "log", "dm", "add_role"];
pub const PROFILES: [&str; 4] = ["relaxed", "standard", "strict", "off"];

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Action {
    pub kind: String,
    #[serde(default)]
    pub secs: u32,
    #[serde(default)]
    pub role: Option<String>,
}
impl Action {
    pub fn new(kind: &str) -> Self {
        Self {
            kind: kind.into(),
            secs: if kind == "timeout" { 600 } else { 0 },
            role: None,
        }
    }
    pub fn valid(&self) -> bool {
        ACTIONS.contains(&self.kind.as_str())
            && (self.kind != "timeout" || (1..=2_419_200).contains(&self.secs))
            && (self.kind == "timeout" || self.secs == 0)
            && (self.kind != "add_role" || self.role.as_deref().is_some_and(valid_id))
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Params {
    pub threshold: u32,
    pub window_secs: u32,
    pub words: Vec<String>,
    pub regex: Vec<String>,
    pub allow_domains: Vec<String>,
    pub block_domains: Vec<String>,
    pub presets: Vec<u8>,
}
impl Default for Params {
    fn default() -> Self {
        Self {
            threshold: 5,
            window_secs: 10,
            words: vec![],
            regex: vec![],
            allow_domains: vec![],
            block_domains: vec![],
            presets: vec![1, 2, 3],
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Rule {
    #[serde(default)]
    pub id: Option<String>,
    pub name: String,
    pub kind: String,
    pub enabled: bool,
    pub log_only: bool,
    pub native: bool,
    #[serde(default)]
    pub discord_rule_id: Option<String>,
    pub params: Params,
    pub actions: Vec<Action>,
    #[serde(default)]
    pub exempt_roles: Vec<String>,
    #[serde(default)]
    pub exempt_channels: Vec<String>,
    #[serde(default)]
    pub only_channels: Vec<String>,
}
impl Rule {
    pub fn needs_content(&self) -> bool {
        !matches!(self.kind.as_str(), "spam" | "mentions" | "honeypot")
    }
    pub fn native_kind(&self) -> bool {
        matches!(
            self.kind.as_str(),
            "spam" | "mentions" | "bad_words" | "invites" | "links" | "presets"
        )
    }
    pub fn applies(&self, channels: &[String], roles: &[String]) -> bool {
        self.enabled
            && !self.exempt_roles.iter().any(|r| roles.contains(r))
            && !self.exempt_channels.iter().any(|c| channels.contains(c))
            && (self.only_channels.is_empty() || self.only_channels.iter().any(|c| channels.contains(c)))
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Escalation {
    pub strikes: u32,
    pub window_secs: u32,
    pub action: Action,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub profile: String,
    pub escalation: Vec<Escalation>,
    pub dm_on_action: bool,
    pub alert_channel_id: Option<String>,
    pub observe_until: Option<DateTime<Utc>>,
    pub rules: Vec<Rule>,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            profile: "off".into(),
            escalation: vec![],
            dm_on_action: false,
            alert_channel_id: None,
            observe_until: None,
            rules: vec![],
        }
    }
}
impl Config {
    pub fn dry(&self, rule: &Rule, now: DateTime<Utc>) -> bool {
        rule.log_only || self.observe_until.is_some_and(|until| now < until)
    }
}
pub fn valid_id(id: &str) -> bool {
    id.parse::<i64>().is_ok_and(|id| id > 0)
}

/// Validate before any write; regex compilation has a strict allocation limit.
pub fn validate(config: &Config, lang: Lang) -> Result<(), String> {
    let bad = || t!(lang, "automod.invalid");
    if !PROFILES.contains(&config.profile.as_str())
        || config.rules.len() > MAX_RULES
        || config.alert_channel_id.as_deref().is_some_and(|id| !valid_id(id))
        || config.escalation.len() > 10
    {
        return Err(bad());
    }
    let mut names = std::collections::HashSet::new();
    let mut ids = std::collections::HashSet::new();
    let mut strikes = std::collections::HashSet::new();
    let window = config.escalation.first().map(|e| e.window_secs);
    for e in &config.escalation {
        if !(1..=100).contains(&e.strikes)
            || !(1..=2_592_000).contains(&e.window_secs)
            || Some(e.window_secs) != window
            || !e.action.valid()
            || !strikes.insert(e.strikes)
        {
            return Err(bad());
        }
    }
    for rule in &config.rules {
        if rule.name.trim().is_empty()
            || rule.name.chars().count() > 40
            || !names.insert(rule.name.to_lowercase())
            || !KINDS.contains(&rule.kind.as_str())
            || rule.actions.is_empty()
            || rule.actions.len() > 8
            || rule.actions.iter().any(|a| !a.valid())
            || rule.id.as_deref().is_some_and(|id| !valid_id(id) || !ids.insert(id))
            || !(1..=10_000).contains(&rule.params.threshold)
            || !(1..=3600).contains(&rule.params.window_secs)
            || rule.exempt_roles.len() > 20
            || rule.exempt_channels.len() > 50
            || rule.only_channels.len() > 50
            || rule
                .exempt_roles
                .iter()
                .chain(&rule.exempt_channels)
                .chain(&rule.only_channels)
                .any(|id| !valid_id(id))
            || (matches!(rule.kind.as_str(), "honeypot" | "media_only" | "youtube_only")
                && rule.enabled
                && rule.only_channels.is_empty())
            || (rule.enabled
                && rule.kind == "bad_words"
                && rule.params.words.is_empty()
                && rule.params.regex.is_empty())
            || (rule.enabled
                && rule.kind == "presets"
                && (!rule.native || rule.params.presets.is_empty() || !rule.only_channels.is_empty()))
            || (rule.kind == "caps" && rule.params.threshold > 100)
            || (rule.kind == "mentions" && rule.params.threshold > 50)
            || (matches!(rule.kind.as_str(), "spam" | "duplicate") && rule.params.threshold > 1000)
            || rule.params.words.len() > 1000
            || rule
                .params
                .words
                .iter()
                .any(|s| s.trim().is_empty() || s.chars().count() > 60)
            || rule.params.regex.len() > 10
            || rule
                .params
                .regex
                .iter()
                .any(|s| s.is_empty() || s.chars().count() > 260 || compile(s).is_err())
            || rule.params.presets.iter().any(|n| !(1..=3).contains(n))
            || rule.params.presets.len() > 3
            || rule.params.allow_domains.len() > 100
            || rule.params.block_domains.len() > 100
            || rule
                .params
                .allow_domains
                .iter()
                .chain(&rule.params.block_domains)
                .any(|d| !domain_valid(d))
        {
            return Err(bad());
        }
    }
    Ok(())
}
fn domain_valid(d: &str) -> bool {
    !d.is_empty()
        && d.len() <= 253
        && d.contains('.')
        && d.rsplit('.')
            .next()
            .is_some_and(|tld| tld.chars().any(char::is_alphabetic))
        && d.split('.').all(|part| {
            !part.is_empty()
                && part.len() <= 63
                && !part.starts_with('-')
                && !part.ends_with('-')
                && part.chars().all(|c| c.is_alphanumeric() || c == '-')
        })
}
pub fn compile(s: &str) -> Result<regex::Regex, regex::Error> {
    regex::RegexBuilder::new(s)
        .size_limit(256 * 1024)
        .dfa_size_limit(256 * 1024)
        .build()
}

/// Profiles populate all cards, leaving channel-specific and empty word lists off.
pub fn profile(name: &str, now: DateTime<Utc>) -> Option<Config> {
    if !PROFILES.contains(&name) {
        return None;
    }
    let strict = name == "strict";
    let relaxed = name == "relaxed";
    let rules = KINDS
        .into_iter()
        .map(|kind| {
            let enabled = name != "off"
                && matches!(
                    kind,
                    "spam" | "duplicate" | "repeat" | "invites" | "mentions" | "caps" | "emoji" | "newlines" | "zalgo"
                );
            let threshold = match kind {
                "caps" => {
                    if strict {
                        65
                    } else {
                        80
                    }
                }
                "spam" | "duplicate" | "mentions" => {
                    if strict {
                        4
                    } else if relaxed {
                        8
                    } else {
                        6
                    }
                }
                "repeat" | "emoji" | "zalgo" => {
                    if strict {
                        10
                    } else {
                        15
                    }
                }
                "newlines" => {
                    if strict {
                        8
                    } else {
                        15
                    }
                }
                _ => 1,
            };
            Rule {
                id: None,
                name: kind.into(),
                kind: kind.into(),
                enabled,
                log_only: false,
                native: matches!(
                    kind,
                    "spam" | "mentions" | "bad_words" | "invites" | "links" | "presets"
                ),
                discord_rule_id: None,
                params: Params {
                    threshold,
                    ..Params::default()
                },
                actions: if kind == "honeypot" {
                    vec![Action::new("delete"), Action::new("ban"), Action::new("log")]
                } else if relaxed {
                    vec![Action::new("delete"), Action::new("log")]
                } else {
                    vec![Action::new("delete"), Action::new("warn"), Action::new("log")]
                },
                exempt_roles: vec![],
                exempt_channels: vec![],
                only_channels: vec![],
            }
        })
        .collect();
    Some(Config {
        profile: name.into(),
        observe_until: (name == "standard").then_some(now + Duration::hours(48)),
        rules,
        ..Config::default()
    })
}

/// Applying a profile keeps trusted exceptions and the alert destination. Off pauses the current set.
pub fn apply_profile(current: &Config, name: &str, now: DateTime<Utc>) -> Option<Config> {
    let mut next = profile(name, now)?;
    if name == "off" {
        next = current.clone();
        next.profile = "off".into();
        next.observe_until = None;
        return Some(next);
    }
    next.alert_channel_id = current.alert_channel_id.clone();
    next.dm_on_action = current.dm_on_action;
    for rule in &mut next.rules {
        if let Some(old) = current.rules.iter().find(|old| old.name == rule.name) {
            rule.exempt_roles = old.exempt_roles.clone();
            rule.exempt_channels = old.exempt_channels.clone();
            rule.only_channels = old.only_channels.clone();
        }
    }
    Some(next)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn profiles_are_valid_and_standard_observes_for_exactly_48_hours() {
        let now = Utc::now();
        for p in PROFILES {
            validate(&profile(p, now).unwrap(), Lang::En).unwrap();
        }
        let c = profile("standard", now).unwrap();
        assert!(c.dry(&c.rules[0], now + Duration::hours(47)));
        assert!(!c.dry(&c.rules[0], now + Duration::hours(48)));
    }
    #[test]
    fn validation_rejects_unbounded_regex_roles_and_unscoped_honeypots() {
        let mut c = profile("strict", Utc::now()).unwrap();
        c.rules[0].params.regex = vec!["[".into()];
        assert!(validate(&c, Lang::En).is_err());
        c.rules[0].params.regex.clear();
        c.rules[14].enabled = true;
        assert!(validate(&c, Lang::En).is_err());
        c.rules[14].only_channels.push("1".into());
        validate(&c, Lang::En).unwrap();
        c.rules[0].actions.push(Action {
            kind: "add_role".into(),
            secs: 0,
            role: Some("0".into()),
        });
        assert!(validate(&c, Lang::En).is_err());
    }
}
