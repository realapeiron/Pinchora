//! Versioned, explicitly allowlisted server configuration. Never discovers tables dynamically.
use crate::{i18n::Lang, t};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

pub const VERSION: u32 = 1;
// Twelve 1 MiB card uploads use up to 24 MiB in PostgreSQL's portable hex encoding.
pub const MAX_BYTES: usize = 32 * 1024 * 1024;
/// A configuration key; records without a unique editor key retain their local ids.
pub struct Section {
    pub table: &'static str,
    pub columns: &'static str,
    pub key: &'static str,
    pub serial: bool,
}
macro_rules! section {
    ($table:literal, $cols:literal, $key:literal, $serial:literal) => {
        Section {
            table: $table,
            columns: $cols,
            key: $key,
            serial: $serial,
        }
    };
}
/// Reef records are player data, never server configuration (including moderation evidence).
pub const EXCLUDED_REEF_TABLES: &[&str] = &[
    "reef_consents",
    "reef_sanctions",
    "reef_chat_evidence",
    "reef_chat_recipients",
    "reef_reports",
    "reef_moderator_audit",
];
pub const SECTIONS: &[Section] = &[
    section!(
        "guilds",
        "leveling_enabled,levelup_channel_id,levelup_message,welcome_enabled,welcome_channel_id,welcome_message,welcome_theme,welcome_title,welcome_accent,level_roles_stack,points_enabled,points_name,points_emoji,points_min,points_max,join_role_id,goodbye_channel_id,goodbye_message,log_channel_id,log_members,log_names,log_bans,log_voice,levelup_mode,voice_xp_enabled,voice_xp_amount,gambling_enabled,prefix,language,sticky_roles,stats_public",
        "",
        false
    ),
    section!("guild_modules", "module,enabled,channel_mode,channels", "module", false),
    section!("command_control", "config", "", false),
    // AFK policy is stored in guild_modules/command_control; member AFK records stay private.
    section!("utility_settings", "timezone", "", false),
    section!(
        "sticky_messages",
        "channel_id,content,embed,min_gap_secs,min_messages,enabled",
        "channel_id",
        false
    ),
    section!(
        "scheduled_messages",
        "channel_id,content,embed,schedule,tz,next_run_at,enabled",
        "id",
        true
    ),
    section!("event_settings", "timezone", "", false),
    section!("card_uploads", "id,png,width,height", "id", false),
    section!("card_designs", "kind,design,delivery", "kind", false),
    section!("mod_settings", "log_channel_id,dm_members", "", false),
    section!("mod_warn_thresholds", "warns,action,duration_secs", "warns", false),
    section!(
        "log_settings",
        "disabled_events,ignored_channels,ignored_roles,ignore_bots",
        "",
        false
    ),
    section!("log_channels", "category,channel_id", "category", false),
    section!("level_roles", "level,role_id", "role_id", false),
    section!("rep_roles", "reputation,role_id", "role_id", false),
    section!("xp_rules", "kind,target_id,multiplier", "kind,target_id", false),
    section!("store_items", "role_id,price,stock", "role_id", false),
    section!("suggestion_settings", "channel_id", "", false),
    section!(
        "party_settings",
        "surge_enabled,surge_channel_id,surge_every_hours",
        "",
        false
    ),
    section!("collection_settings", "enabled,channel_id", "", false),
    section!(
        "auto_roles",
        "role_id,kind,delay_secs,remove_after_secs,enabled",
        "role_id,kind",
        true
    ),
    section!("color_settings", "enabled,allowed_roles,mode,anchor_role_id", "", false),
    section!("color_roles", "role_id,hex,name", "role_id", false),
    section!(
        "role_panels",
        "title,description,channel_id,style,exclusive,required_role,min_picks,max_picks",
        "title",
        true
    ),
    section!(
        "role_panel_roles",
        "panel_id,role_id,label,emoji,position,description,emoji_id,temp_secs",
        "panel_id,role_id",
        false
    ),
    section!("panel_reactions", "panel_id,emoji,role_id", "panel_id,emoji", false),
    section!(
        "ticket_settings",
        "open_category_id,closed_category_id,log_channel_id,auto_close_hours,dm_transcript,rating_enabled,naming,mode,max_open_per_user",
        "",
        false
    ),
    section!("ticket_forms", "name,questions", "name", true),
    section!(
        "ticket_panels",
        "name,channel_id,title,description,color,style,buttons,form_id,support_roles,welcome_text,open_category_id,log_channel_id,enabled",
        "name",
        true
    ),
    section!(
        "automod_settings",
        "profile,escalation,dm_on_action,alert_channel_id",
        "",
        false
    ),
    section!(
        "automod_rules",
        "name,kind,enabled,log_only,native,params,actions,exempt_roles,exempt_channels,only_channels",
        "name",
        true
    ),
    section!(
        "feeds",
        "kind,source,display_name,channel_id,message,embed,ping_role_id,filter_include,filter_exclude,enabled",
        "kind,source,channel_id",
        true
    ),
    section!(
        "starboards",
        "name,channel_id,emoji,threshold,self_star,allow_bots,only_channels,except_channels,require_image,color,role_weights,remove_below,enabled",
        "channel_id",
        true
    ),
    section!(
        "vote_channels",
        "channel_id,up_emoji,down_emoji,only_media,create_thread,enabled",
        "channel_id",
        false
    ),
    section!(
        "counter_channels",
        "channel_id,kind,role_id,template",
        "channel_id",
        true
    ),
    section!(
        "invite_settings",
        "enabled,log_channel_id,fake_days,temp_enabled,temp_channel_id,temp_max_age_secs,temp_max_uses,temp_per_user_daily",
        "",
        false
    ),
    section!("invite_rewards", "invites,role_id", "invites", false),
    section!(
        "verification",
        "enabled,mode,verified_role_id,unverified_role_id,channel_id,kick_after_mins,min_account_age_hours,block_default_avatar,block_name_patterns,dm_on_block,log_channel_id,message,rule_action,exempt_roles",
        "",
        false
    ),
    section!(
        "raid_settings",
        "enabled,join_threshold,window_secs,action,lockdown_minutes,alert_channel_id,similar_threshold,similar_names,similar_avatars,new_account_hours,channels,slowmode_secs,lock_channels,pause_invites",
        "",
        false
    ),
    section!(
        "voice_creators",
        "channel_id,category_id,name_template,default_limit,default_locked,max_per_user,panel,panel_channel_id,enabled",
        "channel_id",
        false
    ),
    section!(
        "custom_commands",
        "name,description,responses,embed,reply_mode,actions,allow_roles,deny_roles,allow_channels,deny_channels,cooldown_secs,enabled,allow_mentions",
        "name",
        true
    ),
    section!(
        "custom_responders",
        "name,trigger_kind,trigger_value,match_kind,responses,embed,actions,destination,channel_id,chance,allow_channels,deny_channels,cooldown_secs,enabled,allow_mentions",
        "id",
        true
    ),
];
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Names {
    pub roles: BTreeMap<String, String>,
    pub channels: BTreeMap<String, String>,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Backup {
    pub version: u32,
    pub source_guild: String,
    pub names: Names,
    pub sections: BTreeMap<String, Vec<Value>>,
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Policy {
    Merge,
    Replace,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Import {
    pub backup: Backup,
    pub policy: Policy,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Difference {
    pub section: String,
    pub added: usize,
    pub changed: usize,
    pub removed: usize,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Preview {
    pub differences: Vec<Difference>,
    pub unmapped: Vec<String>,
    pub errors: Vec<String>,
    pub can_import: bool,
}
/// Feed sources may carry private RSS credentials even though no token column is exported.
/// Keep the subscription recoverable, but require fresh credentials before it can run again.
pub fn redact_feed_credentials(backup: &mut Backup) {
    for row in backup.sections.get_mut("feeds").into_iter().flatten() {
        let Some(source) = row["source"].as_str() else { continue };
        let Ok(mut url) = url::Url::parse(source) else { continue };
        let mut sensitive = !url.username().is_empty() || url.password().is_some();
        let query: Vec<(String, String)> = url
            .query_pairs()
            .map(|(key, value)| {
                let name = key.to_ascii_lowercase().replace('-', "_");
                let secret = name.contains("token")
                    || name.contains("secret")
                    || name.contains("password")
                    || matches!(
                        name.as_str(),
                        "key" | "api_key" | "apikey" | "auth" | "authorization" | "signature" | "sig"
                    );
                sensitive |= secret;
                (
                    key.into_owned(),
                    if secret { "REDACTED".into() } else { value.into_owned() },
                )
            })
            .collect();
        if sensitive {
            let _ = url.set_username("");
            let _ = url.set_password(None);
            if url.query().is_some() {
                url.query_pairs_mut().clear().extend_pairs(query);
            }
            row["source"] = Value::String(url.into());
            row["enabled"] = Value::Bool(false);
        }
    }
}

pub fn row_key(section: &Section, row: &Value) -> String {
    section
        .key
        .split(',')
        .filter(|k| !k.is_empty())
        .map(|k| {
            let value = &row[k];
            if section.table == "role_panels" {
                value.as_str().unwrap_or_default().to_lowercase()
            } else {
                value.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\u{0}")
}
/// Align document-local ids with matching destination rows before merging or comparing children.
/// Records keyed only by id match within the same guild; another guild gets new records.
/// SQL still allocates actual ids for new rows; unused document ids cannot alias another parent.
pub fn align_serial_ids(before: &Backup, incoming: &mut Backup, same_guild: bool) {
    let mut mapped = BTreeMap::new();
    for section in SECTIONS.iter().filter(|s| s.serial) {
        let existing = &before.sections[section.table];
        let rows = incoming.sections.get_mut(section.table).unwrap();
        let mut used: std::collections::BTreeSet<String> = existing
            .iter()
            .chain(rows.iter())
            .map(|r| r["id"].to_string())
            .collect();
        let mut next = i64::MAX;
        for row in rows {
            let key = row_key(section, row);
            let id = if let Some(old) = existing
                .iter()
                .find(|r| (section.key != "id" || same_guild) && row_key(section, r) == key)
            {
                old["id"].clone()
            } else {
                while used.contains(&Value::String(next.to_string()).to_string()) {
                    next -= 1;
                }
                let id = Value::String(next.to_string());
                used.insert(id.to_string());
                next -= 1;
                id
            };
            mapped.insert((section.table, row["id"].to_string()), id.clone());
            row["id"] = id;
        }
    }
    for rows in incoming.sections.values_mut() {
        for row in rows {
            for (field, parent) in [("panel_id", "role_panels"), ("form_id", "ticket_forms")] {
                if let Some(old) = row.get(field)
                    && let Some(id) = mapped.get(&(parent, old.to_string()))
                {
                    row[field] = id.clone();
                }
            }
        }
    }
}

pub fn validate_shape(backup: &Backup, lang: Lang) -> Result<(), String> {
    let invalid = || t!(lang, "config_backup.invalid");
    if backup.version != VERSION
        || backup.source_guild.parse::<u64>().ok().filter(|id| *id > 0).is_none()
        || serde_json::to_vec(backup).map_err(|_| invalid())?.len() > MAX_BYTES
    {
        return Err(invalid());
    }
    if backup.sections.len() != SECTIONS.len() {
        return Err(t!(lang, "config_backup.incomplete"));
    }
    for section in SECTIONS {
        let rows = backup.sections.get(section.table).ok_or_else(invalid)?;
        if rows.len() > 1000
            || (section.key.is_empty() && rows.len() > 1)
            || (section.table == "guilds" && rows.len() != 1)
        {
            return Err(invalid());
        }
        let mut keys = std::collections::BTreeSet::new();
        let mut serial_ids = std::collections::BTreeSet::new();
        let mut colours = std::collections::BTreeSet::new();
        for row in rows {
            let obj = row.as_object().ok_or_else(invalid)?;
            if obj
                .keys()
                .any(|key| !(section.serial && key == "id") && !section.columns.split(',').any(|c| c == key))
                || section.columns.split(',').any(|c| !obj.contains_key(c))
            {
                return Err(invalid());
            }
            if section.serial
                && row
                    .get("id")
                    .and_then(Value::as_str)
                    .and_then(|id| id.parse::<i64>().ok())
                    .is_none_or(|id| id <= 0)
            {
                return Err(invalid());
            }
            if !keys.insert(row_key(section, row)) {
                return Err(invalid());
            }
            if section.table == "color_roles" && !colours.insert(row["hex"].to_string()) {
                return Err(invalid());
            }
            if section.serial && !serial_ids.insert(row["id"].to_string()) {
                return Err(invalid());
            }
        }
    }
    Ok(())
}
pub fn diff(before: &Backup, after: &Backup, policy: Policy) -> Vec<Difference> {
    SECTIONS
        .iter()
        .map(|section| {
            let old: BTreeMap<_, _> = before
                .sections
                .get(section.table)
                .into_iter()
                .flatten()
                .map(|r| (row_key(section, r), r))
                .collect();
            let new: BTreeMap<_, _> = after
                .sections
                .get(section.table)
                .into_iter()
                .flatten()
                .map(|r| (row_key(section, r), r))
                .collect();
            let normalized = |value: &Value| {
                let mut v = value.clone();
                if section.serial {
                    v.as_object_mut().unwrap().remove("id");
                }
                v
            };
            Difference {
                section: section.table.into(),
                added: new.keys().filter(|k| !old.contains_key(*k)).count(),
                changed: new
                    .iter()
                    .filter(|(k, v)| old.get(*k).is_some_and(|o| normalized(o) != normalized(v)))
                    .count(),
                removed: if policy == Policy::Replace {
                    old.keys().filter(|k| !new.contains_key(*k)).count()
                } else {
                    0
                },
            }
        })
        .collect()
}
/// Reference type is explicit, so numbers in content, prices, IDs of forms, and thresholds are untouched.
fn reference_kind(field: &str) -> Option<bool> {
    if field == "role_id"
        || field.ends_with("_role_id")
        || matches!(
            field,
            "role"
                | "required_role"
                | "allowed_roles"
                | "denied_roles"
                | "allow_roles"
                | "deny_roles"
                | "ignored_roles"
                | "exempt_roles"
                | "support_roles"
        )
    {
        return Some(true);
    }
    if field == "channel_id"
        || field.ends_with("_channel_id")
        || field.ends_with("category_id")
        || matches!(
            field,
            "channels"
                | "allowed_channels"
                | "denied_channels"
                | "allow_channels"
                | "deny_channels"
                | "only_channels"
                | "except_channels"
                | "ignored_channels"
                | "exempt_channels"
        )
    {
        return Some(false);
    }
    None
}

pub fn remap(backup: &mut Backup, guild: u64, names: &Names) -> Vec<String> {
    if backup.source_guild == guild.to_string() {
        return vec![];
    }
    let mut missing = std::collections::BTreeSet::new();
    fn one(
        v: &mut Value,
        roles: bool,
        source: &Names,
        target: &Names,
        missing: &mut std::collections::BTreeSet<String>,
    ) {
        if v.is_null() {
            return;
        }
        if let Some(values) = v.as_array_mut() {
            for v in values {
                one(v, roles, source, target, missing);
            }
            return;
        }
        let id = v.as_str().map(str::to_owned).unwrap_or_else(|| v.to_string());
        let (old, new) = if roles {
            (&source.roles, &target.roles)
        } else {
            (&source.channels, &target.channels)
        };
        let name = old.get(&id);
        let matches: Vec<_> = new.iter().filter(|(_, n)| Some(*n) == name).collect();
        if matches.len() == 1 {
            *v = Value::String(matches[0].0.clone());
        } else {
            missing.insert(format!(
                "{}:{} ({})",
                if roles { "role" } else { "channel" },
                id,
                name.map(String::as_str).unwrap_or("?")
            ));
        }
    }
    fn walk(value: &mut Value, source: &Names, target: &Names, missing: &mut std::collections::BTreeSet<String>) {
        match value {
            Value::Object(map) => {
                let trigger_role = map
                    .get("trigger_kind")
                    .and_then(Value::as_str)
                    .is_some_and(|s| s.starts_with("role_"));
                let xp_role = map.get("kind").and_then(Value::as_str) == Some("role");
                for (key, value) in map {
                    if key == "role_weights" {
                        if let Some(weights) = value.as_object_mut() {
                            let mut mapped = serde_json::Map::new();
                            for (id, weight) in std::mem::take(weights) {
                                let mut id = Value::String(id);
                                one(&mut id, true, source, target, missing);
                                mapped.insert(id.as_str().unwrap_or_default().into(), weight);
                            }
                            *weights = mapped;
                        }
                    } else if key == "trigger_value" && trigger_role {
                        one(value, true, source, target, missing);
                    } else if key == "target_id" {
                        one(value, xp_role, source, target, missing);
                    } else if let Some(roles) = reference_kind(key) {
                        if value.is_array() || value.is_string() || value.is_number() || value.is_null() {
                            one(value, roles, source, target, missing);
                        }
                    } else {
                        walk(value, source, target, missing);
                    }
                }
            }
            Value::Array(values) => {
                for v in values {
                    walk(v, source, target, missing);
                }
            }
            Value::String(text) => {
                let mut replacements = vec![];
                for (prefix, roles) in [("<@&", true), ("<#", false)] {
                    for part in text.split(prefix).skip(1) {
                        if let Some((id, _)) = part.split_once('>')
                            && id.parse::<u64>().is_ok()
                        {
                            let mut value = Value::String(id.into());
                            one(&mut value, roles, source, target, missing);
                            replacements.push((
                                format!("{prefix}{id}>"),
                                format!("{prefix}{}>", value.as_str().unwrap_or(id)),
                            ));
                        }
                    }
                }
                for (old, new) in replacements {
                    *text = text.replace(&old, &new);
                }
            }
            _ => {}
        }
    }
    for rows in backup.sections.values_mut() {
        for row in rows {
            walk(row, &backup.names, names, &mut missing);
        }
    }
    backup.source_guild = guild.to_string();
    backup.names = names.clone();
    missing.into_iter().collect()
}

/// Discord references, including nested action and command policies, for editor-side existence checks.
pub fn references(backup: &Backup) -> Vec<(bool, String)> {
    fn ids(v: &Value, role: bool, out: &mut Vec<(bool, String)>) {
        match v {
            Value::Array(a) => {
                for v in a {
                    ids(v, role, out)
                }
            }
            Value::String(s) => out.push((role, s.clone())),
            Value::Number(n) => out.push((role, n.to_string())),
            _ => {}
        }
    }
    fn walk(v: &Value, out: &mut Vec<(bool, String)>) {
        match v {
            Value::Object(map) => {
                for (key, v) in map {
                    if key == "role_weights" {
                        if let Some(map) = v.as_object() {
                            out.extend(map.keys().map(|id| (true, id.clone())));
                        }
                    } else if key == "target_id" {
                        ids(v, map.get("kind").and_then(Value::as_str) == Some("role"), out);
                    } else if key == "trigger_value"
                        && map
                            .get("trigger_kind")
                            .and_then(Value::as_str)
                            .is_some_and(|s| s.starts_with("role_"))
                    {
                        ids(v, true, out);
                    } else if let Some(role) = reference_kind(key) {
                        ids(v, role, out);
                    } else {
                        walk(v, out);
                    }
                }
            }
            Value::Array(a) => {
                for v in a {
                    walk(v, out)
                }
            }
            _ => {}
        }
    }
    let mut out = Vec::new();
    for rows in backup.sections.values() {
        for row in rows {
            walk(row, &mut out);
        }
    }
    out.sort();
    out.dedup();
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn merged_parent_ids_and_child_references_follow_destination_natural_keys() {
        let mut before = Backup {
            version: VERSION,
            source_guild: "1".into(),
            names: Names::default(),
            sections: SECTIONS.iter().map(|s| (s.table.into(), vec![])).collect(),
        };
        before
            .sections
            .insert("role_panels".into(), vec![json!({"id":"20","title":"Roles"})]);
        before
            .sections
            .insert("ticket_forms".into(), vec![json!({"id":"30","name":"Support"})]);
        let mut incoming = before.clone();
        incoming.sections.insert(
            "role_panels".into(),
            vec![json!({"id":"10","title":"roles"}), json!({"id":"20","title":"Other"})],
        );
        incoming.sections.insert(
            "role_panel_roles".into(),
            vec![
                json!({"panel_id":"10","role_id":"50"}),
                json!({"panel_id":"20","role_id":"51"}),
            ],
        );
        incoming
            .sections
            .insert("ticket_forms".into(), vec![json!({"id":"40","name":"Support"})]);
        incoming.sections.insert(
            "ticket_panels".into(),
            vec![json!({"id":"60","name":"Help","form_id":"40"})],
        );
        align_serial_ids(&before, &mut incoming, true);
        assert_eq!(incoming.sections["role_panels"][0]["id"], "20");
        assert_eq!(incoming.sections["role_panel_roles"][0]["panel_id"], "20");
        let other = &incoming.sections["role_panels"][1]["id"];
        assert_ne!(other, &json!("20"));
        assert_eq!(&incoming.sections["role_panel_roles"][1]["panel_id"], other);
        assert_eq!(incoming.sections["ticket_panels"][0]["form_id"], "30");
        before
            .sections
            .insert("custom_responders".into(), vec![json!({"id":"77","name":"Welcome"})]);
        incoming.sections.insert(
            "custom_responders".into(),
            vec![json!({"id":"77","name":"Welcome"}), json!({"id":"88","name":"Welcome"})],
        );
        let mut other_guild = incoming.clone();
        align_serial_ids(&before, &mut incoming, true);
        assert_eq!(incoming.sections["custom_responders"][0]["id"], "77");
        align_serial_ids(&before, &mut other_guild, false);
        assert_ne!(other_guild.sections["custom_responders"][0]["id"], "77");
        assert_ne!(
            other_guild.sections["custom_responders"][0]["id"],
            other_guild.sections["custom_responders"][1]["id"]
        );
    }
    #[test]
    fn remapping_preserves_restrictions_and_reports_ambiguous_names() {
        let names = Names {
            roles: BTreeMap::from([("1".into(), "Member".into())]),
            channels: BTreeMap::from([("2".into(), "Games".into())]),
        };
        let mut b = Backup {
            version: VERSION,
            source_guild: "10".into(),
            names,
            sections: BTreeMap::from([(
                "command_control".into(),
                vec![
                    json!({"config":{"rules":{"roles post":{"allowed_roles":["1"],"denied_channels":["2"]}},"aliases":{}}}),
                ],
            )]),
        };
        let target = Names {
            roles: BTreeMap::from([("11".into(), "Member".into())]),
            channels: BTreeMap::from([("12".into(), "Games".into())]),
        };
        assert!(remap(&mut b, 20, &target).is_empty());
        assert_eq!(
            b.sections["command_control"][0]["config"]["rules"]["roles post"]["allowed_roles"],
            json!(["11"])
        );
        let ambiguous = Names {
            roles: BTreeMap::from([("21".into(), "Member".into()), ("22".into(), "Member".into())]),
            channels: target.channels,
        };
        assert!(!remap(&mut b, 30, &ambiguous).is_empty());
        assert_eq!(
            b.sections["command_control"][0]["config"]["rules"]["roles post"]["allowed_roles"],
            json!(["11"]),
            "unmapped restrictions are never emptied"
        );
    }
    #[test]
    fn automod_rule_and_escalation_roles_are_remapped_and_checked() {
        let mut backup = Backup {
            version: VERSION,
            source_guild: "10".into(),
            names: Names {
                roles: BTreeMap::from([("1".into(), "Muted".into())]),
                channels: BTreeMap::new(),
            },
            sections: BTreeMap::from([
                (
                    "automod_rules".into(),
                    vec![json!({"actions":[{"kind":"add_role","role":"1"}]})],
                ),
                (
                    "automod_settings".into(),
                    vec![json!({"escalation":[{"action":{"kind":"add_role","role":"1"}}]})],
                ),
            ]),
        };
        assert_eq!(references(&backup), vec![(true, "1".into())]);
        let names = Names {
            roles: BTreeMap::from([("2".into(), "Muted".into())]),
            channels: BTreeMap::new(),
        };
        assert!(remap(&mut backup, 20, &names).is_empty());
        assert_eq!(references(&backup), vec![(true, "2".into())]);
        assert_eq!(
            backup.sections["automod_settings"][0]["escalation"][0]["action"]["role"],
            "2"
        );
    }

    #[test]
    fn member_tables_and_unknown_fields_are_never_accepted() {
        let mut backup = Backup {
            version: VERSION,
            source_guild: "1".into(),
            names: Names::default(),
            sections: SECTIONS.iter().map(|s| (s.table.into(), vec![])).collect(),
        };
        backup.sections.insert(
            "guilds".into(),
            vec![Value::Object(
                SECTIONS[0]
                    .columns
                    .split(',')
                    .map(|key| (key.into(), Value::Null))
                    .collect(),
            )],
        );
        validate_shape(&backup, Lang::En).unwrap();
        for table in std::iter::once("members").chain(EXCLUDED_REEF_TABLES.iter().copied()) {
            assert!(!SECTIONS.iter().any(|section| section.table == table));
            backup.sections.insert(table.into(), vec![json!({"user_id":"1234"})]);
            assert!(
                validate_shape(&backup, Lang::En).is_err(),
                "{table} must never be imported"
            );
            backup.sections.remove(table);
        }
        backup
            .sections
            .get_mut("command_control")
            .unwrap()
            .push(json!({"config":{},"token":"secret"}));
        assert!(validate_shape(&backup, Lang::En).is_err());
    }
}
