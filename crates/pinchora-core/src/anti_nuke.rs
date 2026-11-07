//! Shared anti-nuke limits, authorization and bounded recovery policy.
use crate::{i18n::Lang, settings, t};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
pub const KINDS: [&str; 13] = [
    "channel_delete",
    "channel_create",
    "role_delete",
    "role_create",
    "ban",
    "kick",
    "webhook_create",
    "permission_grant",
    "bot_add",
    "guild_update",
    "emoji_delete",
    "vip_ban",
    "vip_action",
];
pub const RESPONSES: [&str; 4] = ["alert", "quarantine", "kick", "ban"];
pub const MAX_LIST: usize = 50;
pub const MAX_SNAPSHOTS: i64 = 750;
pub const MAX_SNAPSHOT_BYTES: usize = 65536;
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Limit {
    pub count: u32,
    pub window_secs: u32,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub enabled: bool,
    pub limits: BTreeMap<String, Limit>,
    pub response: String,
    pub log_channel_id: Option<String>,
    pub quarantine_role_id: Option<String>,
    pub vip_roles: Vec<String>,
    pub trusted_users: Vec<String>,
    pub owner_only_config: bool,
    pub auto_restore: bool,
    pub revert_vip_bans: bool,
    pub remove_bots: bool,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            enabled: false,
            limits: KINDS
                .into_iter()
                .map(|k| {
                    (
                        k.into(),
                        Limit {
                            count: if k.starts_with("vip_") { 1 } else { 5 },
                            window_secs: 60,
                        },
                    )
                })
                .collect(),
            response: "alert".into(),
            log_channel_id: None,
            quarantine_role_id: None,
            vip_roles: vec![],
            trusted_users: vec![],
            owner_only_config: true,
            auto_restore: false,
            revert_vip_bans: false,
            remove_bots: false,
        }
    }
}
pub fn valid_id(s: &str) -> bool {
    s.parse::<u64>()
        .is_ok_and(|n| n > 0 && n <= i64::MAX as u64 && n.to_string() == s)
}
pub fn validate(c: &Config, lang: Lang) -> Result<(), String> {
    let lists = [&c.vip_roles, &c.trusted_users];
    if !RESPONSES.contains(&c.response.as_str())
        || c.limits.len() != KINDS.len()
        || KINDS.iter().any(|k| {
            c.limits
                .get(*k)
                .is_none_or(|l| !(1..=100).contains(&l.count) || !(5..=300).contains(&l.window_secs))
        })
        || lists.iter().any(|list| {
            list.len() > MAX_LIST
                || list.iter().any(|s| !valid_id(s))
                || list.iter().collect::<std::collections::BTreeSet<_>>().len() != list.len()
        })
        || [&c.log_channel_id, &c.quarantine_role_id]
            .into_iter()
            .flatten()
            .any(|s| !valid_id(s))
        || (c.response == "quarantine" && c.quarantine_role_id.is_none())
    {
        return Err(t!(lang, "anti_nuke.invalid"));
    }
    Ok(())
}
pub fn exempt(c: &Config, owner: u64, bot: u64, actor: u64) -> bool {
    actor == owner || actor == bot || c.trusted_users.contains(&actor.to_string())
}
pub fn may_configure(c: &Config, owner: u64, actor: u64) -> bool {
    actor == owner || (!c.owner_only_config && c.trusted_users.contains(&actor.to_string()))
}
pub fn vip(c: &Config, roles: impl IntoIterator<Item = u64>) -> bool {
    roles.into_iter().any(|r| c.vip_roles.contains(&r.to_string()))
}
pub fn dangerous_grant(old: u64, new: u64) -> bool {
    !settings::dangerous_permissions(new & !old).is_empty()
}
/// Changing trust and its ownership policy is reserved to the owner, including on the web.
pub fn check_edit(old: &Config, next: &Config, owner: u64, actor: u64, lang: Lang) -> Result<(), String> {
    validate(next, lang)?;
    if !may_configure(old, owner, actor)
        || (actor != owner
            && (old.trusted_users != next.trusted_users || old.owner_only_config != next.owner_only_config))
    {
        return Err(t!(lang, "anti_nuke.owner_only"));
    }
    if next.trusted_users.contains(&actor.to_string()) && !old.trusted_users.contains(&actor.to_string()) {
        return Err(t!(lang, "anti_nuke.self_trust"));
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ownership_and_no_self_trust() {
        let c = Config::default();
        let mut n = c.clone();
        n.trusted_users = vec!["2".into()];
        assert!(check_edit(&c, &n, 1, 1, Lang::En).is_ok());
        assert!(check_edit(&c, &n, 1, 2, Lang::En).is_err());
        n.trusted_users = vec!["1".into()];
        assert!(check_edit(&c, &n, 1, 1, Lang::En).is_err());
        assert!(exempt(&c, 1, 3, 1));
        assert!(exempt(&c, 1, 3, 3));
        assert!(!exempt(&c, 1, 3, 2));
    }
    #[test]
    fn limits_and_permission_deltas() {
        let mut c = Config::default();
        assert!(validate(&c, Lang::En).is_ok());
        c.limits.get_mut("ban").unwrap().count = 0;
        assert!(validate(&c, Lang::En).is_err());
        assert!(dangerous_grant(0, 1 << 3));
        assert!(!dangerous_grant(1 << 3, 1 << 3));
        assert!(!dangerous_grant(0, 1 << 10));
    }
}
