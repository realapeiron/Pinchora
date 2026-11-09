//! Additive command policy shared by Discord, text commands, and the dashboard.
use crate::infra::i18n::Lang;
use crate::t;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const MAX_ALIASES: usize = 50;
pub const PROTECTED: &[&str] = &[
    // registrations:protected:start
    "commandcontrol",
    "backup",
    "config show",
    "config modules",
    "config prefix",
    "config language",
    "setup server",
    "setup commands show",
    "setup commands set",
    "setup commands reset",
    "setup commands alias",
    "setup backup",
    "help",
    "ping",
    "privacy",
    "language",
    "remind", // registrations:protected:end
];

/// Recovery paths are protected individually; unrelated setup actions remain policy-controlled.
pub fn protected(path: &str) -> bool {
    PROTECTED
        .iter()
        .any(|p| path == *p || path.strip_prefix(p).is_some_and(|s| s.starts_with(' ')))
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default, deny_unknown_fields)]
pub struct Rule {
    /// Independent migrated parent/leaf restrictions, keyed by their original path.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub constraints: BTreeMap<String, Rule>,
    pub enabled: bool,
    pub allowed_roles: Vec<String>,
    pub denied_roles: Vec<String>,
    pub allowed_channels: Vec<String>,
    pub denied_channels: Vec<String>,
    pub cooldown_secs: u32,
    pub delete_reply_secs: u32,
    pub delete_invocation_secs: u32,
    pub ephemeral: bool,
}
impl Default for Rule {
    fn default() -> Self {
        Self {
            constraints: BTreeMap::new(),
            enabled: true,
            allowed_roles: vec![],
            denied_roles: vec![],
            allowed_channels: vec![],
            denied_channels: vec![],
            cooldown_secs: 0,
            delete_reply_secs: 0,
            delete_invocation_secs: 0,
            ephemeral: false,
        }
    }
}
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub rules: BTreeMap<String, Rule>,
    pub aliases: BTreeMap<String, String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Command {
    pub path: String,
    pub description: String,
    pub permissions: String,
    pub ephemeral_supported: bool,
    pub protected: bool,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Denial {
    Disabled,
    DeniedRole,
    RequiredRole,
    DeniedChannel,
    RequiredChannel,
}
impl Denial {
    pub fn message(self, lang: Lang) -> String {
        t!(
            lang,
            match self {
                Self::Disabled => "command_control.disabled",
                Self::DeniedRole => "command_control.denied_role",
                Self::RequiredRole => "command_control.required_role",
                Self::DeniedChannel => "command_control.denied_channel",
                Self::RequiredChannel => "command_control.required_channel",
            }
        )
    }
}
impl Rule {
    pub fn check(&self, roles: &[String], lineage: &[String]) -> Result<(), Denial> {
        for rule in self.constraints.values() {
            rule.check(roles, lineage)?;
        }
        let intersects = |a: &[String], b: &[String]| a.iter().any(|id| b.contains(id));
        if !self.enabled {
            return Err(Denial::Disabled);
        }
        if intersects(&self.denied_roles, roles) {
            return Err(Denial::DeniedRole);
        }
        if !self.allowed_roles.is_empty() && !intersects(&self.allowed_roles, roles) {
            return Err(Denial::RequiredRole);
        }
        if intersects(&self.denied_channels, lineage) {
            return Err(Denial::DeniedChannel);
        }
        if !self.allowed_channels.is_empty() && !intersects(&self.allowed_channels, lineage) {
            return Err(Denial::RequiredChannel);
        }
        Ok(())
    }
}
impl Config {
    /// Parent restrictions cannot be undone by a more specific rule.
    pub fn matching<'a>(&'a self, path: &'a str) -> impl Iterator<Item = (&'a String, &'a Rule)> {
        self.rules.iter().filter(move |(key, _)| {
            path == key.as_str() || path.strip_prefix(key.as_str()).is_some_and(|s| s.starts_with(' '))
        })
    }
    pub fn check(&self, path: &str, roles: &[String], lineage: &[String]) -> Result<(), Denial> {
        if protected(path) {
            return Ok(());
        }
        for (_, rule) in self.matching(path) {
            rule.check(roles, lineage)?;
        }
        Ok(())
    }
    /// Expand only the first token. Alias chains are deliberately impossible.
    pub fn expand(&self, text: &str) -> String {
        let text = text.trim_start();
        let end = text.find(char::is_whitespace).unwrap_or(text.len());
        match self.aliases.get(&text[..end].to_ascii_lowercase()) {
            Some(path) => format!("{path}{}", &text[end..]),
            None => text.to_owned(),
        }
    }
    /// Existing aliases that collide with a newly shipped root keep their text meaning.
    /// Editing an unrelated rule must not delete them; new collisions are still rejected.
    pub fn validate_update(
        &self,
        previous: &Self,
        commands: &[Command],
        reserved: &[&str],
        lang: Lang,
    ) -> Result<(), String> {
        let mut check = self.clone();
        check.aliases.retain(|name, target| {
            !(previous.aliases.get(name) == Some(target)
                && commands.iter().any(|c| &c.path == target)
                && (reserved.contains(&name.as_str())
                    || commands.iter().any(|c| c.path.split(' ').next() == Some(name))))
        });
        if self.aliases.len() > MAX_ALIASES {
            return Err(t!(lang, "command_control.alias_invalid"));
        }
        check.validate(commands, reserved, lang)
    }

    pub fn validate(&self, commands: &[Command], reserved: &[&str], lang: Lang) -> Result<(), String> {
        let invalid = || t!(lang, "command_control.invalid");
        if self.aliases.len() > MAX_ALIASES || self.rules.len() > commands.len() {
            return Err(invalid());
        }
        for (path, parent) in &self.rules {
            if parent.constraints.len() > 16 || parent.constraints.values().any(|r| !r.constraints.is_empty()) {
                return Err(invalid());
            }
            for rule in std::iter::once(parent).chain(parent.constraints.values()) {
                let command = commands.iter().find(|c| &c.path == path).ok_or_else(invalid)?;
                if command.protected && rule != &Rule::default() {
                    return Err(t!(lang, "command_control.protected"));
                }
                if rule.ephemeral && !command.ephemeral_supported {
                    return Err(t!(lang, "command_control.no_ephemeral"));
                }
                if rule.cooldown_secs > 86400 || rule.delete_reply_secs > 840 || rule.delete_invocation_secs > 86400 {
                    return Err(invalid());
                }
                for ids in [
                    &rule.allowed_roles,
                    &rule.denied_roles,
                    &rule.allowed_channels,
                    &rule.denied_channels,
                ] {
                    if ids.len() > 50
                        || ids.iter().any(|id| {
                            id.parse::<u64>()
                                .ok()
                                .filter(|id| *id > 0 && *id <= i64::MAX as u64)
                                .is_none()
                        })
                    {
                        return Err(invalid());
                    }
                }
            }
        }
        for (name, path) in &self.aliases {
            if name.is_empty()
                || name.len() > 32
                || !name
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_')
                || reserved.contains(&name.as_str())
                || commands.iter().any(|c| c.path.split(' ').next() == Some(name))
                || !commands.iter().any(|c| &c.path == path)
            {
                return Err(t!(lang, "command_control.alias_invalid"));
            }
        }
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use crate::admin::command_control::*;
    #[test]
    fn enforcement_matrix() {
        let cases = [
            (Rule::default(), vec![], vec![], Ok(())),
            (
                Rule {
                    enabled: false,
                    ..Rule::default()
                },
                vec![],
                vec![],
                Err(Denial::Disabled),
            ),
            (
                Rule {
                    allowed_roles: vec!["1".into()],
                    ..Rule::default()
                },
                vec![],
                vec![],
                Err(Denial::RequiredRole),
            ),
            (
                Rule {
                    allowed_roles: vec!["1".into()],
                    denied_roles: vec!["2".into()],
                    ..Rule::default()
                },
                vec!["1".into(), "2".into()],
                vec![],
                Err(Denial::DeniedRole),
            ),
            (
                Rule {
                    allowed_channels: vec!["9".into()],
                    ..Rule::default()
                },
                vec![],
                vec!["3".into(), "9".into()],
                Ok(()),
            ),
            (
                Rule {
                    denied_channels: vec!["9".into()],
                    ..Rule::default()
                },
                vec![],
                vec!["3".into(), "9".into()],
                Err(Denial::DeniedChannel),
            ),
            (
                Rule {
                    allowed_channels: vec!["9".into()],
                    ..Rule::default()
                },
                vec![],
                vec![],
                Err(Denial::RequiredChannel),
            ),
        ];
        for surface in ["slash", "button", "text"] {
            for (rule, roles, lineage, expected) in &cases {
                assert_eq!(&rule.check(roles, lineage), expected, "{surface}");
            }
        }
        let c = Config {
            rules: BTreeMap::from([
                (
                    "play".into(),
                    Rule {
                        enabled: false,
                        ..Rule::default()
                    },
                ),
                ("play wordle".into(), Rule::default()),
            ]),
            ..Config::default()
        };
        assert_eq!(c.check("play wordle", &[], &[]), Err(Denial::Disabled));
    }
    #[test]
    fn aliases_expand_one_token_only() {
        let c = Config {
            aliases: BTreeMap::from([("w".into(), "play wordle".into())]),
            ..Config::default()
        };
        assert_eq!(c.expand("W daily"), "play wordle daily");
        assert_eq!(c.expand("word"), "word");
    }
}
