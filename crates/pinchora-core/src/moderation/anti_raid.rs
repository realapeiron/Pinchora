//! Raid thresholds. Similarity is a signal, never a substitute for hierarchy checks.
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct RaidSettings {
    pub enabled: bool,
    pub join_threshold: u32,
    pub window_secs: u32,
    pub action: String,
    pub lockdown_minutes: u32,
    pub alert_channel_id: Option<String>,
    pub similar_threshold: u32,
    pub similar_names: bool,
    pub similar_avatars: bool,
    pub new_account_hours: u32,
    pub channels: Vec<String>,
    pub slowmode_secs: u32,
    pub lock_channels: bool,
    pub pause_invites: bool,
}
impl Default for RaidSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            join_threshold: 10,
            window_secs: 30,
            action: "alert".into(),
            lockdown_minutes: 10,
            alert_channel_id: None,
            similar_threshold: 5,
            similar_names: true,
            similar_avatars: true,
            new_account_hours: 24,
            channels: vec![],
            slowmode_secs: 10,
            lock_channels: false,
            pause_invites: false,
        }
    }
}
pub fn name_family(name: &str) -> String {
    name.to_lowercase()
        .chars()
        .filter(|c| c.is_alphabetic())
        .take(32)
        .collect()
}
#[cfg(test)]
mod tests {
    #[test]
    fn families_ignore_number_suffixes() {
        assert_eq!(
            crate::moderation::anti_raid::name_family("Raider_123"),
            crate::moderation::anti_raid::name_family("raider456")
        );
        assert_ne!(
            crate::moderation::anti_raid::name_family("ada"),
            crate::moderation::anti_raid::name_family("efe")
        );
    }
}
