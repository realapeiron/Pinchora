//! Bounded, data-only workflows. Planning is pure and is also the dashboard's test runner.
use crate::{
    modules::Module,
    tags::{self, Action, EmbedSpec},
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
pub const MAX_WORKFLOWS: usize = 25;
pub const MAX_CONDITIONS: usize = 5;
pub const MAX_ACTIONS: usize = 10;
pub const MAX_WRITES: usize = 5;
pub const MAX_RUNS: i64 = 100;
pub const MAX_VARIABLES: usize = 100;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Definition {
    pub name: String,
    pub enabled: bool,
    pub trigger: Trigger,
    pub conditions: Vec<Condition>,
    pub actions: Vec<StepAction>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Trigger {
    Command {},
    Button {},
    MemberJoin {},
    MemberLeave {},
    LevelUp {},
    RoleAdded { role_id: String },
    RoleRemoved { role_id: String },
    ReactionAdded { emoji: String, channel_id: String },
    MessageKeyword { keyword: String },
    Schedule { spec: Schedule, tz: String },
    VoiceJoin {},
    Birthday {},
    Achievement {},
    TicketOpened {},
    TicketClosed {},
    StarboardPost {},
    FeedItem {},
}
impl Trigger {
    pub fn key(&self) -> &'static str {
        match self {
            Self::Command {} => "command",
            Self::Button {} => "button",
            Self::MemberJoin {} => "member_join",
            Self::MemberLeave {} => "member_leave",
            Self::LevelUp {} => "level_up",
            Self::RoleAdded { .. } => "role_added",
            Self::RoleRemoved { .. } => "role_removed",
            Self::ReactionAdded { .. } => "reaction_added",
            Self::MessageKeyword { .. } => "message_keyword",
            Self::Schedule { .. } => "schedule",
            Self::VoiceJoin {} => "voice_join",
            Self::Birthday {} => "birthday",
            Self::Achievement {} => "achievement",
            Self::TicketOpened {} => "ticket_opened",
            Self::TicketClosed {} => "ticket_closed",
            Self::StarboardPost {} => "starboard_post",
            Self::FeedItem {} => "feed_item",
        }
    }
    pub fn owner(&self) -> Option<Module> {
        match self {
            Self::LevelUp {} => Some(Module::Leveling),
            Self::Birthday {} => Some(Module::Birthdays),
            Self::Achievement {} => Some(Module::Achievements),
            Self::TicketOpened {} | Self::TicketClosed {} => Some(Module::Tickets),
            Self::StarboardPost {} => Some(Module::Starboard),
            Self::FeedItem {} => Some(Module::Feeds),
            _ => None,
        }
    }
    pub fn matches(&self, event: &Event) -> bool {
        if self.key() != event.kind || event.depth != 0 {
            return false;
        }
        match self {
            Self::RoleAdded { role_id } | Self::RoleRemoved { role_id } => role_id == &event.role_id,
            Self::ReactionAdded { emoji, channel_id } => emoji == &event.value && channel_id == &event.channel_id,
            Self::MessageKeyword { keyword } => event.value.to_lowercase().contains(&keyword.to_lowercase()),
            _ => true,
        }
    }
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Schedule {
    Daily { hour: u32, minute: u32 },
    Weekly { weekday: u32, hour: u32, minute: u32 },
    Interval { minutes: u32 },
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Scope {
    Server,
    Member,
}
impl Scope {
    pub fn key(self) -> &'static str {
        match self {
            Self::Server => "server",
            Self::Member => "member",
        }
    }
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Condition {
    HasRole { role_id: String },
    LacksRole { role_id: String },
    InChannel { channel_id: String },
    MinLevel { level: u32 },
    AccountAge { days: u32 },
    Chance { percent: u32 },
    Cooldown { scope: Scope, seconds: u32 },
    VarEquals { scope: Scope, key: String, value: String },
    VarGreater { scope: Scope, key: String, value: i64 },
    TimeWindow { start: u32, end: u32, tz: String },
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum StepAction {
    SendMessage {
        channel_id: String,
        text: String,
        #[serde(default)]
        embed: Option<EmbedSpec>,
    },
    Reply {
        text: String,
        #[serde(default)]
        embed: Option<EmbedSpec>,
    },
    /// Existing tag actions retain their validation and daily points bucket.
    Tag {
        action: Action,
    },
    TakePoints {
        amount: i64,
    },
    SetVar {
        scope: Scope,
        key: String,
        value: String,
    },
    IncrVar {
        scope: Scope,
        key: String,
        amount: i64,
    },
    CreateThread {
        channel_id: String,
        name: String,
    },
    AddReaction {
        emoji: String,
    },
    Pin {},
    OpenTicket {
        panel_id: i64,
        topic: usize,
    },
    Log {
        text: String,
    },
    RandomBranch {
        branches: Vec<Vec<StepAction>>,
    },
}
impl StepAction {
    pub fn writes(&self) -> usize {
        match self {
            Self::Tag {
                action: Action::Dm { .. },
            } => 2,
            Self::Tag {
                action: Action::GivePoints { .. },
            }
            | Self::TakePoints { .. }
            | Self::SetVar { .. }
            | Self::IncrVar { .. }
            | Self::Log { .. } => 0,
            Self::OpenTicket { .. } => 5,
            Self::RandomBranch { branches } => branches
                .iter()
                .map(|b| b.iter().map(Self::writes).sum())
                .max()
                .unwrap_or(0),
            _ => 1,
        }
    }
}
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Event {
    pub kind: String,
    pub user_id: String,
    pub channel_id: String,
    pub message_id: String,
    pub role_id: String,
    pub value: String,
    pub roles: Vec<String>,
    pub level: u32,
    pub account_age_days: u32,
    pub depth: u8,
    pub roll: u32,
    pub variables: BTreeMap<String, String>,
    pub cooldowns: Vec<usize>,
}
impl Event {
    pub fn variable(&self, scope: Scope, key: &str) -> Option<&str> {
        self.variables
            .get(&format!("{}:{key}", scope.key()))
            .map(String::as_str)
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Playback {
    pub phase: String,
    pub index: usize,
    pub status: String,
    pub detail: serde_json::Value,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Plan {
    pub outcome: String,
    pub steps: Vec<Playback>,
    pub actions: Vec<StepAction>,
}
pub fn id(value: &str) -> bool {
    value.parse::<u64>().is_ok_and(|n| n > 0 && n <= i64::MAX as u64)
}
pub fn valid_key(key: &str) -> bool {
    !key.is_empty() && key.len() <= 32 && key.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
}
mod planning;
mod schedule;
#[cfg(test)]
mod tests;
mod validation;
pub use planning::{condition_passes, plan, plan_with_preview, preview_action, render, variable_value, variables};
