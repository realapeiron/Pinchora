//! Bounded, data-only workflows. Planning is pure and is also the dashboard's test runner.
use crate::{
    modules::Module,
    tags::{self, Action, EmbedSpec},
};
use chrono::{DateTime, Datelike, Duration, Timelike, Utc};
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
    Command,
    Button,
    MemberJoin,
    MemberLeave,
    LevelUp,
    RoleAdded { role_id: String },
    RoleRemoved { role_id: String },
    ReactionAdded { emoji: String, channel_id: String },
    MessageKeyword { keyword: String },
    Schedule { spec: Schedule, tz: String },
    VoiceJoin,
    Birthday,
    Achievement,
    TicketOpened,
    TicketClosed,
    StarboardPost,
    FeedItem,
}
impl Trigger {
    pub fn key(&self) -> &'static str {
        match self {
            Self::Command => "command",
            Self::Button => "button",
            Self::MemberJoin => "member_join",
            Self::MemberLeave => "member_leave",
            Self::LevelUp => "level_up",
            Self::RoleAdded { .. } => "role_added",
            Self::RoleRemoved { .. } => "role_removed",
            Self::ReactionAdded { .. } => "reaction_added",
            Self::MessageKeyword { .. } => "message_keyword",
            Self::Schedule { .. } => "schedule",
            Self::VoiceJoin => "voice_join",
            Self::Birthday => "birthday",
            Self::Achievement => "achievement",
            Self::TicketOpened => "ticket_opened",
            Self::TicketClosed => "ticket_closed",
            Self::StarboardPost => "starboard_post",
            Self::FeedItem => "feed_item",
        }
    }
    pub fn owner(&self) -> Option<Module> {
        match self {
            Self::LevelUp => Some(Module::Leveling),
            Self::Birthday => Some(Module::Birthdays),
            Self::Achievement => Some(Module::Achievements),
            Self::TicketOpened | Self::TicketClosed => Some(Module::Tickets),
            Self::StarboardPost => Some(Module::Starboard),
            Self::FeedItem => Some(Module::Feeds),
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
impl Schedule {
    pub fn next(&self, after: DateTime<Utc>, tz: &str) -> Option<DateTime<Utc>> {
        let tz: chrono_tz::Tz = tz.parse().ok()?;
        match *self {
            Self::Interval { minutes } if (1..=525600).contains(&minutes) => {
                after.checked_add_signed(Duration::minutes(minutes.into()))
            }
            Self::Daily { hour, minute } | Self::Weekly { hour, minute, .. } if hour < 24 && minute < 60 => {
                // Iterate UTC minutes: nonexistent local times are skipped; a repeated local minute fires only once.
                let start = after.with_second(0)?.with_nanosecond(0)?;
                for offset in 1..=15 * 24 * 60 {
                    let next = start.checked_add_signed(Duration::minutes(offset))?;
                    let local = next.with_timezone(&tz);
                    let day = match self {
                        Self::Weekly { weekday, .. } => {
                            *weekday < 7 && local.weekday().num_days_from_monday() == *weekday
                        }
                        _ => true,
                    };
                    if day && local.hour() == hour && local.minute() == minute {
                        let naive = local.naive_local();
                        use chrono::TimeZone;
                        if tz.from_local_datetime(&naive).earliest()?.with_timezone(&Utc) == next {
                            return Some(next);
                        }
                    }
                }
                None
            }
            _ => None,
        }
    }
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
    Pin,
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
fn text(value: &str, max: usize) -> bool {
    !value.trim().is_empty() && value.chars().count() <= max
}
impl Definition {
    pub fn validate(&self) -> Result<(), &'static str> {
        if !text(&self.name, 40)
            || self
                .name
                .chars()
                .any(|c| !(c.is_alphanumeric() || c == '-' || c == '_'))
        {
            return Err("name");
        }
        if self.conditions.len() > MAX_CONDITIONS {
            return Err("conditions");
        }
        match &self.trigger {
            Trigger::RoleAdded { role_id } | Trigger::RoleRemoved { role_id } if !id(role_id) => return Err("trigger"),
            Trigger::ReactionAdded { emoji, channel_id } if !id(channel_id) || !text(emoji, 100) => {
                return Err("trigger");
            }
            Trigger::MessageKeyword { keyword } if !text(keyword, 200) => return Err("trigger"),
            Trigger::Schedule { spec, tz } if spec.next(DateTime::from_timestamp(0, 0).unwrap(), tz).is_none() => {
                return Err("trigger");
            }
            _ => {}
        }
        for c in &self.conditions {
            let ok = match c {
                Condition::HasRole { role_id } | Condition::LacksRole { role_id } => id(role_id),
                Condition::InChannel { channel_id } => id(channel_id),
                Condition::MinLevel { level } => *level <= 100000,
                Condition::AccountAge { days } => *days <= 36500,
                Condition::Chance { percent } => *percent <= 100,
                Condition::Cooldown { seconds, .. } => (1..=86400).contains(seconds),
                Condition::VarEquals { key, value, .. } => valid_key(key) && value.chars().count() <= 200,
                Condition::VarGreater { key, .. } => valid_key(key),
                Condition::TimeWindow { start, end, tz } => {
                    *start < 1440 && *end < 1440 && tz.parse::<chrono_tz::Tz>().is_ok()
                }
            };
            if !ok {
                return Err("conditions");
            }
        }
        let mut count = 0;
        fn check(actions: &[StepAction], nested: bool, count: &mut usize) -> bool {
            for a in actions {
                *count += 1;
                let ok = match a {
                    StepAction::Tag { action } => tags::clean_actions(vec![action.clone()], true, true).is_ok(),
                    StepAction::TakePoints { amount } => (1..=1000).contains(amount),
                    StepAction::SendMessage {
                        channel_id,
                        text: body,
                        embed,
                    } => id(channel_id) && text(body, 2000) && tags::clean_embed(embed.clone()).is_ok(),
                    StepAction::Reply { text: body, embed } => {
                        text(body, 2000) && tags::clean_embed(embed.clone()).is_ok()
                    }
                    StepAction::SetVar { key, value, .. } => valid_key(key) && value.chars().count() <= 200,
                    StepAction::IncrVar { key, amount, .. } => valid_key(key) && amount.unsigned_abs() <= 1_000_000_000,
                    StepAction::CreateThread { channel_id, name } => id(channel_id) && text(name, 100),
                    StepAction::AddReaction { emoji } => {
                        tags::clean_actions(vec![Action::React { emoji: emoji.clone() }], true, true).is_ok()
                    }
                    StepAction::OpenTicket { panel_id, topic } => *panel_id > 0 && *topic < 25,
                    StepAction::Log { text: body } => text(body, 2000),
                    StepAction::Pin => true,
                    StepAction::RandomBranch { branches } => {
                        !nested
                            && !branches.is_empty()
                            && branches.len() <= 5
                            && branches.iter().all(|b| !b.is_empty() && check(b, true, count))
                    }
                };
                if !ok || *count > MAX_ACTIONS {
                    return false;
                }
            }
            true
        }
        if self.actions.is_empty() || !check(&self.actions, false, &mut count) {
            return Err("actions");
        }
        if self.actions.iter().map(StepAction::writes).sum::<usize>() > MAX_WRITES {
            return Err("writes");
        }
        Ok(())
    }
}
pub fn condition_passes(c: &Condition, index: usize, event: &Event, now: DateTime<Utc>) -> bool {
    match c {
        Condition::HasRole { role_id } => event.roles.contains(role_id),
        Condition::LacksRole { role_id } => !event.roles.contains(role_id),
        Condition::InChannel { channel_id } => *channel_id == event.channel_id,
        Condition::MinLevel { level } => event.level >= *level,
        Condition::AccountAge { days } => event.account_age_days >= *days,
        Condition::Chance { percent } => event.roll % 100 < *percent,
        Condition::Cooldown { .. } => !event.cooldowns.contains(&index),
        Condition::VarEquals { scope, key, value } => event.variable(*scope, key) == Some(value),
        Condition::VarGreater { scope, key, value } => event
            .variable(*scope, key)
            .and_then(|v| v.parse::<i64>().ok())
            .is_some_and(|v| v > *value),
        Condition::TimeWindow { start, end, tz } => tz.parse::<chrono_tz::Tz>().is_ok_and(|tz| {
            let n = now.with_timezone(&tz);
            let m = n.hour() * 60 + n.minute();
            if start <= end {
                m >= *start && m < *end
            } else {
                m >= *start || m < *end
            }
        }),
    }
}
/// The same deterministic placeholder values for tests and live messages.
pub fn variables(event: &Event) -> tags::Vars {
    let mut vars = tags::Vars::new();
    vars.set("user.id", &event.user_id)
        .set("user", format!("<@{}>", event.user_id))
        .set("channel", format!("<#{}>", event.channel_id))
        .set("level", event.level.to_string())
        .set("event", &event.value);
    for (key, value) in &event.variables {
        vars.set(&format!("var.{}", key.replace(':', ".")), value);
    }
    vars
}
pub fn render(text: &str, event: &Event) -> String {
    tags::truncate(
        &tags::render(text, &variables(event), &mut |n| event.roll as usize % n.max(1)),
        2000,
    )
}
fn preview_value(value: &mut serde_json::Value, event: &Event) {
    match value {
        serde_json::Value::String(s) => *s = render(s, event),
        serde_json::Value::Object(o) => {
            for v in o.values_mut() {
                preview_value(v, event)
            }
        }
        serde_json::Value::Array(a) => {
            for v in a {
                preview_value(v, event)
            }
        }
        _ => {}
    }
}
pub fn plan(def: &Definition, event: &Event, now: DateTime<Utc>) -> Plan {
    let mut result = Plan {
        outcome: "ready".into(),
        steps: Vec::new(),
        actions: Vec::new(),
    };
    if def.validate().is_err() || event.depth != 0 {
        result.outcome = "refused".into();
        return result;
    }
    let matched = def.trigger.matches(event);
    result.steps.push(Playback {
        phase: "trigger".into(),
        index: 0,
        status: if matched { "passed" } else { "failed" }.into(),
        detail: serde_json::json!({"kind":def.trigger.key()}),
    });
    let mut passed = matched;
    for (index, c) in def.conditions.iter().enumerate() {
        let ok = condition_passes(c, index, event, now);
        passed &= ok;
        result.steps.push(Playback {
            phase: "condition".into(),
            index,
            status: if ok { "passed" } else { "failed" }.into(),
            detail: serde_json::to_value(c).unwrap_or_default(),
        });
    }
    for a in &def.actions {
        if let StepAction::RandomBranch { branches } = a {
            result
                .actions
                .extend(branches[event.roll as usize % branches.len()].clone());
        } else {
            result.actions.push(a.clone());
        }
    }
    let mut simulated = event.clone();
    for (index, a) in result.actions.iter().enumerate() {
        let mut detail = serde_json::to_value(a).unwrap_or_default();
        preview_value(&mut detail, &simulated);
        if passed {
            match a {
                StepAction::SetVar { scope, key, value } => {
                    let value = render(value, &simulated);
                    simulated.variables.insert(format!("{}:{key}", scope.key()), value);
                }
                StepAction::IncrVar { scope, key, amount } => {
                    if let Some(value) = simulated
                        .variable(*scope, key)
                        .unwrap_or("0")
                        .parse::<i64>()
                        .ok()
                        .and_then(|v| v.checked_add(*amount))
                    {
                        simulated
                            .variables
                            .insert(format!("{}:{key}", scope.key()), value.to_string());
                    }
                }
                _ => {}
            }
        }

        result.steps.push(Playback {
            phase: "action".into(),
            index,
            status: if passed { "would_run" } else { "skipped" }.into(),
            detail,
        });
    }
    if !passed {
        result.outcome = "filtered".into();
        result.actions.clear();
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    fn def() -> Definition {
        Definition {
            name: "hello".into(),
            enabled: true,
            trigger: Trigger::Button,
            conditions: vec![],
            actions: vec![StepAction::Reply {
                text: "Hi".into(),
                embed: None,
            }],
        }
    }
    #[test]
    fn budgets_and_unknown_actions_fail_closed() {
        let mut d = def();
        d.actions = vec![d.actions[0].clone(); 6];
        assert_eq!(d.validate(), Err("writes"));
        d.actions = vec![StepAction::Log { text: "x".into() }; 11];
        assert_eq!(d.validate(), Err("actions"));
        d = def();
        d.conditions = vec![Condition::Chance { percent: 100 }; 6];
        assert_eq!(d.validate(), Err("conditions"));
        for kind in ["wait", "give_shells", "run_workflow", "execute", "http"] {
            assert!(serde_json::from_value::<StepAction>(serde_json::json!({"kind":kind})).is_err());
        }
        d = def();
        d.actions = vec![StepAction::RandomBranch {
            branches: vec![vec![StepAction::RandomBranch {
                branches: vec![d.actions.clone()],
            }]],
        }];
        assert!(d.validate().is_err());
    }
    #[test]
    fn conditions_and_depth() {
        let mut d = def();
        d.conditions = vec![
            Condition::HasRole { role_id: "42".into() },
            Condition::VarGreater {
                scope: Scope::Server,
                key: "visits".into(),
                value: 2,
            },
        ];
        let mut e = Event {
            kind: "button".into(),
            roles: vec!["42".into()],
            ..Default::default()
        };
        e.variables.insert("server:visits".into(), "3".into());
        assert_eq!(plan(&d, &e, Utc::now()).outcome, "ready");
        e.depth = 1;
        assert_eq!(plan(&d, &e, Utc::now()).outcome, "refused");
        e.depth = 0;
        e.roles.clear();
        assert_eq!(plan(&d, &e, Utc::now()).outcome, "filtered");
    }
    #[test]
    fn schedule_dst_and_intervals() {
        let now = "2026-03-08T06:59:00Z".parse().unwrap();
        assert_eq!(
            Schedule::Daily { hour: 2, minute: 30 }
                .next(now, "America/New_York")
                .unwrap()
                .to_rfc3339(),
            "2026-03-09T06:30:00+00:00"
        );
        assert_eq!(
            Schedule::Weekly {
                weekday: 6,
                hour: 2,
                minute: 30
            }
            .next("2026-03-01T07:30:00Z".parse().unwrap(), "America/New_York")
            .unwrap()
            .to_rfc3339(),
            "2026-03-15T06:30:00+00:00"
        );
        assert!(Schedule::Interval { minutes: 0 }.next(now, "UTC").is_none());
        assert!(
            Schedule::Weekly {
                weekday: 7,
                hour: 1,
                minute: 0
            }
            .next(now, "UTC")
            .is_none()
        );
    }
}

#[cfg(test)]
mod contract_tests {
    use super::*;
    #[test]
    fn validation_table_and_write_accounting() {
        let sample = serde_json::json!({"name":"workflow","enabled":true,"trigger":{"kind":"command"},"conditions":[],"actions":[{"kind":"log","text":"ok"}]});
        for condition in [
            serde_json::json!({"kind":"chance","percent":101}),
            serde_json::json!({"kind":"cooldown","scope":"server","seconds":0}),
            serde_json::json!({"kind":"has_role","role_id":"0"}),
            serde_json::json!({"kind":"var_equals","scope":"member","key":"bad.key","value":"x"}),
            serde_json::json!({"kind":"time_window","start":1440,"end":1,"tz":"UTC"}),
        ] {
            let mut value = sample.clone();
            value["conditions"] = serde_json::json!([condition]);
            assert_eq!(
                serde_json::from_value::<Definition>(value).unwrap().validate(),
                Err("conditions")
            );
        }
        assert_eq!(
            StepAction::Tag {
                action: Action::Dm { text: "hi".into() }
            }
            .writes(),
            2
        );
        let mut value = sample;
        value["actions"] = serde_json::json!([{"kind":"random_branch","branches":[[{"kind":"log","text":"ok"}],[{"kind":"open_ticket","panel_id":1,"topic":0},{"kind":"reply","text":"extra"}]]}]);
        assert_eq!(
            serde_json::from_value::<Definition>(value).unwrap().validate(),
            Err("writes")
        );
    }
    #[test]
    fn preview_evaluates_variables_sequentially_without_mutating_fake_event() {
        let def:Definition=serde_json::from_value(serde_json::json!({"name":"preview","enabled":false,"trigger":{"kind":"command"},"conditions":[],"actions":[{"kind":"set_var","scope":"server","key":"visits","value":"2"},{"kind":"incr_var","scope":"server","key":"visits","amount":3},{"kind":"reply","text":"Visits: {var.server.visits}"}]})).unwrap();
        let event = Event {
            kind: "command".into(),
            ..Default::default()
        };
        let result = plan(&def, &event, Utc::now());
        assert_eq!(result.steps[3].detail["text"], "Visits: 5");
        assert!(event.variables.is_empty());
    }
    #[test]
    fn timezone_window_cooldown_and_random_boundaries() {
        let now = "2026-11-08T21:30:00Z".parse().unwrap();
        let event = Event {
            cooldowns: vec![2],
            roll: 99,
            ..Default::default()
        };
        assert!(condition_passes(
            &Condition::TimeWindow {
                start: 1380,
                end: 60,
                tz: "Europe/Istanbul".into()
            },
            0,
            &event,
            now
        ));
        assert!(!condition_passes(
            &Condition::Cooldown {
                scope: Scope::Member,
                seconds: 60
            },
            2,
            &event,
            now
        ));
        assert!(!condition_passes(&Condition::Chance { percent: 99 }, 0, &event, now));
        assert!(condition_passes(&Condition::Chance { percent: 100 }, 0, &event, now));
    }
}
