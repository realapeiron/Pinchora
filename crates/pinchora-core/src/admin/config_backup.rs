//! Versioned, explicitly allowlisted server configuration. Never discovers tables dynamically.
use crate::infra::i18n::Lang;
use crate::t;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

mod comparison;
mod documents;
mod references;
mod schema;

pub use comparison::{diff, validate_shape};
pub use documents::{align_serial_ids, redact_feed_credentials, redact_url_credentials, row_key};
pub use references::references;
pub use schema::{EXCLUDED_REEF_TABLES, SECTIONS, Section};

pub const VERSION: u32 = 1;
// Twelve 1 MiB card uploads use up to 24 MiB in PostgreSQL's portable hex encoding.
pub const MAX_BYTES: usize = 32 * 1024 * 1024;
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
                | "review_roles"
                | "staff_roles"
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

#[cfg(test)]
mod tests {
    use crate::admin::config_backup::*;
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
    fn community_review_and_staff_roles_are_remapped_and_checked() {
        let mut backup = Backup {
            version: VERSION,
            source_guild: "10".into(),
            names: Names {
                roles: BTreeMap::from([("1".into(), "Staff".into())]),
                channels: BTreeMap::new(),
            },
            sections: BTreeMap::from([
                ("forms".into(), vec![json!({"review_roles":["1"]})]),
                ("modmail_settings".into(), vec![json!({"staff_roles":["1"]})]),
            ]),
        };
        assert_eq!(references(&backup), vec![(true, "1".into())]);
        let target = Names {
            roles: BTreeMap::from([("2".into(), "Staff".into())]),
            channels: BTreeMap::new(),
        };
        assert!(remap(&mut backup, 20, &target).is_empty());
        assert_eq!(backup.sections["forms"][0]["review_roles"], json!(["2"]));
        assert_eq!(backup.sections["modmail_settings"][0]["staff_roles"], json!(["2"]));
        assert_eq!(references(&backup), vec![(true, "2".into())]);
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
