//! Shared anti-nuke limits, authorization and bounded recovery policy.
use crate::admin::settings;
use crate::infra::i18n::Lang;
use crate::t;
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
            && (old.trusted_users != next.trusted_users
                || old.owner_only_config != next.owner_only_config
                || (old.enabled && !next.enabled)
                || old.response != next.response
                || old.quarantine_role_id != next.quarantine_role_id
                || old.log_channel_id != next.log_channel_id
                || old.vip_roles.iter().any(|r| !next.vip_roles.contains(r))
                || (old.auto_restore && !next.auto_restore)
                || (old.revert_vip_bans && !next.revert_vip_bans)
                || (old.remove_bots && !next.remove_bots)
                || old
                    .limits
                    .iter()
                    .any(|(k, l)| next.limits[k].count > l.count || next.limits[k].window_secs < l.window_secs)))
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
    use crate::moderation::anti_nuke::*;
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
    fn trusted_staff_cannot_weaken_owner_protections() {
        let old = Config {
            enabled: true,
            owner_only_config: false,
            trusted_users: vec!["2".into()],
            vip_roles: vec!["3".into()],
            auto_restore: true,
            revert_vip_bans: true,
            remove_bots: true,
            ..Config::default()
        };
        for change in 0..8 {
            let mut next = old.clone();
            match change {
                0 => next.enabled = false,
                1 => next.vip_roles.clear(),
                2 => next.trusted_users.push("4".into()),
                3 => next.auto_restore = false,
                4 => next.revert_vip_bans = false,
                5 => next.remove_bots = false,
                6 => next.limits.get_mut("ban").unwrap().count += 1,
                _ => next.limits.get_mut("ban").unwrap().window_secs -= 1,
            }
            assert!(check_edit(&old, &next, 1, 2, Lang::En).is_err(), "change {change}");
            assert!(check_edit(&old, &next, 1, 1, Lang::En).is_ok(), "owner change {change}");
        }
        let mut next = old.clone();
        next.vip_roles.push("4".into());
        next.limits.get_mut("ban").unwrap().count = 1;
        assert!(check_edit(&old, &next, 1, 2, Lang::En).is_ok());
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
