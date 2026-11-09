//! Join-gate settings and deterministic account signals, shared by Discord and the dashboard.
use crate::infra::i18n::Lang;
use crate::t;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct Verification {
    pub enabled: bool,
    pub mode: String,
    pub verified_role_id: Option<String>,
    pub unverified_role_id: Option<String>,
    pub channel_id: Option<String>,
    pub message_id: Option<String>,
    pub kick_after_mins: u32,
    pub min_account_age_hours: u32,
    pub block_default_avatar: bool,
    pub block_name_patterns: bool,
    pub dm_on_block: bool,
    pub log_channel_id: Option<String>,
    pub message: String,
    pub rule_action: String,
    pub exempt_roles: Vec<String>,
    pub exempt_users: Vec<String>,
}
impl Default for Verification {
    fn default() -> Self {
        Self {
            enabled: false,
            mode: "captcha".into(),
            verified_role_id: None,
            unverified_role_id: None,
            channel_id: None,
            message_id: None,
            kick_after_mins: 0,
            min_account_age_hours: 0,
            block_default_avatar: false,
            block_name_patterns: false,
            dm_on_block: false,
            log_channel_id: None,
            message: String::new(),
            rule_action: "hold".into(),
            exempt_roles: vec![],
            exempt_users: vec![],
        }
    }
}
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub verification: Verification,
    pub raid: crate::moderation::anti_raid::RaidSettings,
}

pub fn valid_id(id: &str) -> bool {
    id.parse::<i64>().is_ok_and(|n| n > 0)
}
pub fn validate(c: &Config, lang: Lang) -> Result<(), String> {
    let v = &c.verification;
    let r = &c.raid;
    let ids = v
        .verified_role_id
        .iter()
        .chain(&v.unverified_role_id)
        .chain(&v.channel_id)
        .chain(&v.log_channel_id)
        .chain(&v.exempt_roles)
        .chain(&v.exempt_users)
        .chain(&r.alert_channel_id)
        .chain(&r.channels);
    if !["button", "captcha", "web"].contains(&v.mode.as_str())
        || !["hold", "kick", "notify"].contains(&v.rule_action.as_str())
        || !["alert", "lockdown", "kick", "ban"].contains(&r.action.as_str())
        || v.kick_after_mins > 10080
        || v.min_account_age_hours > 8760
        || v.message.chars().count() > 1500
        || v.exempt_roles.len() > 50
        || v.exempt_users.len() > 50
        || !(2..=500).contains(&r.join_threshold)
        || !(5..=600).contains(&r.window_secs)
        || !(1..=1440).contains(&r.lockdown_minutes)
        || r.slowmode_secs > 21600
        || r.channels.len() > 25
        || !(2..=500).contains(&r.similar_threshold)
        || r.new_account_hours > 8760
        || ids.into_iter().any(|id| !valid_id(id))
    {
        return Err(t!(lang, "verification.invalid"));
    }
    if v.enabled && v.verified_role_id.is_none() && v.unverified_role_id.is_none() {
        return Err(t!(lang, "verification.role_required"));
    }
    if v.verified_role_id.is_some() && v.verified_role_id == v.unverified_role_id {
        return Err(t!(lang, "verification.same_role"));
    }
    Ok(())
}
/// Conservative generated-name signal: a long digit suffix or an uninterrupted consonant run.
pub fn generated_name(name: &str) -> bool {
    let suffix = name.chars().rev().take_while(char::is_ascii_digit).count();
    let mut run = 0;
    let consonants = name.to_ascii_lowercase().bytes().any(|b| {
        if b.is_ascii_alphabetic() && !b"aeiouy".contains(&b) {
            run += 1;
        } else {
            run = 0;
        }
        run >= 8
    });
    suffix >= 6 || consonants
}
pub fn age_hours(user: u64, now: DateTime<Utc>) -> i64 {
    (now - crate::infra::snowflake::timestamp(user)).num_hours().max(0)
}
pub fn flagged(v: &Verification, user: u64, name: &str, default_avatar: bool, now: DateTime<Utc>) -> bool {
    age_hours(user, now) < i64::from(v.min_account_age_hours)
        || (v.block_default_avatar && default_avatar)
        || (v.block_name_patterns && generated_name(name))
}
#[cfg(test)]
mod tests {
    use crate::moderation::verification::*;
    #[test]
    fn names_are_conservative() {
        for (name, expected) in [
            ("mia", false),
            ("çınar", false),
            ("player123", false),
            ("bot123456", true),
            ("bcdfghjkl", true),
            ("alex_2026", false),
        ] {
            assert_eq!(generated_name(name), expected, "{name}");
        }
    }
    #[test]
    fn snowflake_age_and_future() {
        let now = DateTime::from_timestamp(1800000000, 0).unwrap();
        let id = ((now.timestamp_millis() - 1420070400000 - 7200000) as u64) << 22;
        assert_eq!(age_hours(id, now), 2);
        assert_eq!(age_hours(id, now - chrono::Duration::hours(3)), 0);
    }
    #[test]
    fn validation_limits_and_roles() {
        let mut c = Config::default();
        assert!(validate(&c, Lang::En).is_ok());
        c.verification.enabled = true;
        assert!(validate(&c, Lang::En).is_err());
        c.verification.verified_role_id = Some("123".into());
        assert!(validate(&c, Lang::En).is_ok());
        c.verification.unverified_role_id = Some("123".into());
        assert!(validate(&c, Lang::Tr).is_err());
        c.verification.unverified_role_id = None;
        c.raid.window_secs = 0;
        assert!(validate(&c, Lang::En).is_err());
    }
}

/// One readable choice per lookalike group in the Silkscreen font.
pub const CAPTCHA_ALPHABET: &[u8] = b"ACDEFHJKPRTUXY34";
#[test]
fn captcha_alphabet_has_no_ambiguous_pairs() {
    assert_eq!(CAPTCHA_ALPHABET.len(), 16);
    assert!(CAPTCHA_ALPHABET.iter().all(|c| !b"0OQ1IL75S2Z8B6G9VWMN".contains(c)));
    assert!(
        CAPTCHA_ALPHABET
            .iter()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
    );
}
