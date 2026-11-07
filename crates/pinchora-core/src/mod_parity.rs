//! Shared validation for moderation operations and portable policy.
use serde::{Deserialize, Serialize};

pub const MAX_BATCH: usize = 100;
pub const MAX_RULES: usize = 25;
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub report_channel: Option<String>,
    pub mute_role: Option<String>,
    pub jail_role: Option<String>,
    pub appeals: bool,
    pub booster_role: Option<String>,
    pub booster_personal: bool,
    pub autopurge: Vec<AutoPurge>,
    pub conditions: Vec<RoleCondition>,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct AutoPurge {
    pub channel: String,
    pub age_secs: i64,
    pub bots_only: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct RoleCondition {
    pub role: String,
    pub requires: Vec<String>,
    pub all: bool,
    pub voice_channel: Option<String>,
}
pub fn id(s: &str) -> Option<u64> {
    s.parse::<u64>().ok().filter(|n| *n > 0 && *n <= i64::MAX as u64)
}
impl Config {
    pub fn valid(&self) -> bool {
        let ids = [
            &self.report_channel,
            &self.mute_role,
            &self.jail_role,
            &self.booster_role,
        ];
        ids.into_iter().flatten().all(|s| id(s).is_some())
            && self.autopurge.len() <= MAX_RULES && self.conditions.len() <= MAX_RULES
            && self.autopurge.iter().all(|r| id(&r.channel).is_some() && (3600..=1_209_600).contains(&r.age_secs))
            && self.conditions.iter().all(|r| id(&r.role).is_some() && r.requires.len() <= MAX_RULES
                && (!r.requires.is_empty() || r.voice_channel.is_some())
                && r.requires.iter().all(|s| id(s).is_some() && s != &r.role)
                && r.voice_channel.as_deref().is_none_or(|s| id(s).is_some()))
            // No dependency cycles: output roles cannot be input roles of any rule.
            && self.conditions.iter().all(|r| !self.conditions.iter().any(|other| other.requires.contains(&r.role)))
            && self.conditions.iter().enumerate().all(|(i,r)| !self.conditions[..i].iter().any(|p| p.role == r.role))
            && self.autopurge.iter().enumerate().all(|(i,r)| !self.autopurge[..i].iter().any(|p| p.channel == r.channel))
    }
}
pub fn parse_ids(s: &str) -> Option<Vec<u64>> {
    let mut ids = s
        .split(|c: char| c == ',' || c.is_whitespace())
        .filter(|s| !s.is_empty())
        .map(id)
        .collect::<Option<Vec<_>>>()?;
    ids.sort_unstable();
    ids.dedup();
    (!ids.is_empty() && ids.len() <= MAX_BATCH).then_some(ids)
}
/// Keep Unicode letters and numbers, remove hoist prefixes and invisible/control characters.
pub fn nickname(s: &str, normalize: bool) -> String {
    let s = s.trim_start_matches(|c: char| !c.is_alphanumeric());
    let value: String = s
        .chars()
        .map(|c| {
            if normalize && ('\u{FF01}'..='\u{FF5E}').contains(&c) {
                char::from_u32(c as u32 - 0xFEE0).unwrap_or(c)
            } else {
                c
            }
        })
        .filter(|c| {
            if normalize {
                c.is_alphanumeric() || c.is_whitespace()
            } else {
                !c.is_control()
            }
        })
        .take(32)
        .collect();
    if value.trim().is_empty() {
        "Member".into()
    } else {
        value.trim().into()
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ids_are_bounded_and_deduplicated() {
        assert_eq!(parse_ids("12, 15 12"), Some(vec![12, 15]));
        assert!(parse_ids("0").is_none());
        assert!(parse_ids("12 nope").is_none());
        assert!(parse_ids(&(1..=101).map(|i| i.to_string()).collect::<Vec<_>>().join(" ")).is_none());
    }
    #[test]
    fn names_preserve_turkish() {
        assert_eq!(nickname("!! Şule İzmir", true), "Şule İzmir");
        assert_eq!(nickname("!!!", true), "Member");
    }
    #[test]
    fn rejects_cycles_and_invalid_policy() {
        let mut c = Config::default();
        assert!(c.valid());
        c.conditions.push(RoleCondition {
            role: "1".into(),
            requires: vec!["2".into()],
            all: true,
            voice_channel: None,
        });
        assert!(c.valid());
        c.conditions.push(RoleCondition {
            role: "2".into(),
            requires: vec!["1".into()],
            all: true,
            voice_channel: None,
        });
        assert!(!c.valid());
    }
}
