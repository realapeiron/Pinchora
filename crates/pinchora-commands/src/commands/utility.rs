//! `/afk`, `/sticky` and `/schedule`.

use std::str::Split;

use chrono::{DateTime, Utc};
use pinchora_core::{
    t,
    utility::{Schedule, clean, next_run, offset, preview_runs},
};
use pinchora_db::{ScheduledRow, UtilityStickyRow};
use serde_json::json;
use twilight_interactions::command::{CommandModel, CreateCommand};
use twilight_model::{
    application::interaction::InteractionChannel,
    channel::message::{
        Component, MessageFlags,
        component::{ButtonStyle, Label, TextInput, TextInputStyle},
    },
    guild::Permissions,
    http::interaction::InteractionResponseData,
};

use crate::{
    Cx,
    custom::scrub_value,
    server_events::{button, row},
};

fn manage_guild() -> Permissions {
    Permissions::MANAGE_GUILD
}

#[derive(CommandModel, CreateCommand)]
#[command(
    name = "afk",
    desc = "Tell people you are away until your next message",
    contexts = "guild"
)]
pub enum AfkCommand {
    #[command(name = "set")]
    Set(AfkSet),
}

#[derive(CommandModel, CreateCommand)]
#[command(name = "set", desc = "Set your AFK reason")]
pub struct AfkSet {
    /// Why you are away
    #[command(max_length = 100)]
    reason: Option<String>,
    /// Set across all servers
    global: Option<bool>,
    /// Add [AFK] to your server nickname if allowed
    nickname: Option<bool>,
}

impl AfkCommand {
    pub async fn run(self, cx: &Cx) -> anyhow::Result<()> {
        let Some(guild) = cx.guild_id() else {
            return cx.reply_ephemeral(t!(cx.lang(), "utility.guild-only")).await;
        };
        let Self::Set(set) = self;
        let Some(reason) =
            crate::utility::afk_plain(&set.reason.unwrap_or_else(|| t!(cx.lang(), "utility.afk-default")))
        else {
            return cx.reply_ephemeral(t!(cx.lang(), "utility.afk-unsafe")).await;
        };
        let user = cx.author()?.id.get();
        let mut prefixed = false;
        if set.nickname.unwrap_or(false) && !set.global.unwrap_or(false) {
            let current = cx.author_display_name();
            if !current.starts_with("[AFK] ") {
                let current =
                    crate::utility::afk_plain(&current).unwrap_or_else(|| t!(cx.lang(), "utility.afk-member"));
                let nick = format!("[AFK] {current}");
                if cx
                    .app
                    .http
                    .update_guild_member(twilight_model::id::Id::new(guild), twilight_model::id::Id::new(user))
                    .nick(Some(&nick.chars().take(32).collect::<String>()))
                    .await
                    .is_ok()
                {
                    prefixed = true;
                }
            }
        }
        cx.app
            .db
            .set_afk(
                if set.global.unwrap_or(false) { 0 } else { guild },
                user,
                &reason,
                prefixed,
            )
            .await?;
        crate::utility::forget_afk(&cx.app, user).await?;
        cx.reply_ephemeral(t!(cx.lang(), "utility.afk-set", reason = scrub_value(&reason)))
            .await
    }
}

#[derive(CommandModel, CreateCommand)]
#[command(
    name = "sticky",
    desc = "Keep a message at the bottom of a channel",
    default_permissions = "manage_guild",
    contexts = "guild"
)]
pub enum StickyCommand {
    #[command(name = "set")]
    Set(StickySet),
    #[command(name = "remove")]
    Remove(StickyChannel),
    #[command(name = "show")]
    Show(StickyShow),
}

#[derive(CommandModel, CreateCommand)]
#[command(name = "set", desc = "Create or replace a sticky message")]
pub struct StickySet {
    /// Channel to keep the message in
    #[command(channel_types = "guild_text guild_announcement")]
    channel: InteractionChannel,
    /// Message text
    #[command(max_length = 1000)]
    text: String,
    /// Number of human messages before reposting
    #[command(min_value = 1, max_value = 1000)]
    messages: Option<i64>,
    /// Minimum seconds between posts
    #[command(min_value = 5, max_value = 86400)]
    seconds: Option<i64>,
}

#[derive(CommandModel, CreateCommand)]
#[command(name = "remove", desc = "Remove a channel's sticky message")]
pub struct StickyChannel {
    /// Channel to remove
    channel: InteractionChannel,
}
#[derive(CommandModel, CreateCommand)]
#[command(name = "show", desc = "List sticky messages")]
pub struct StickyShow;

impl StickyCommand {
    pub async fn run(self, cx: &Cx) -> anyhow::Result<()> {
        let Some(guild) = cx.guild_id() else {
            return cx.reply_ephemeral(t!(cx.lang(), "utility.guild-only")).await;
        };
        if !cx.can_manage_guild() {
            return cx.reply_ephemeral(t!(cx.lang(), "utility.manage-needed")).await;
        }
        match self {
            Self::Set(v) => {
                let content = clean(&v.text, 1000);
                if content.is_empty() {
                    return cx.reply_ephemeral(t!(cx.lang(), "utility.invalid")).await;
                }
                let perms = crate::utility::bot_permissions(&cx.app, guild, v.channel.id.get())
                    .await
                    .unwrap_or(Permissions::empty());
                if !perms.contains(Permissions::SEND_MESSAGES) {
                    return cx.reply_ephemeral(t!(cx.lang(), "utility.permission-send")).await;
                }
                if !perms.contains(Permissions::MANAGE_MESSAGES) {
                    return cx.reply_ephemeral(t!(cx.lang(), "utility.permission-manage")).await;
                }
                cx.app
                    .db
                    .upsert_sticky(&UtilityStickyRow {
                        channel_id: v.channel.id.get() as i64,
                        guild_id: guild as i64,
                        content,
                        embed: None,
                        last_message_id: None,
                        min_gap_secs: v.seconds.unwrap_or(30) as i32,
                        min_messages: v.messages.unwrap_or(5) as i32,
                        message_count: 0,
                        enabled: true,
                        lease_until: None,
                        pause_reason: None,
                        updated_by: Some(cx.author()?.id.get() as i64),
                    })
                    .await?;
                cx.reply_ephemeral(t!(cx.lang(), "utility.saved")).await
            }
            Self::Remove(v) => {
                let found = cx.app.db.remove_sticky(guild, v.channel.id.get()).await?;
                if let Some(copy) = found.and_then(|s| s.last_message_id) {
                    cx.app.db.queue_sticky_cleanup(v.channel.id.get(), copy as u64).await?;
                }
                cx.reply_ephemeral(t!(cx.lang(), "utility.removed")).await
            }
            Self::Show(_) => {
                let rows = cx.app.db.stickies(guild).await?;
                let lines = rows
                    .iter()
                    .map(|s| {
                        t!(
                            cx.lang(),
                            "utility.sticky-line",
                            channel = format!("<#{id}>", id = s.channel_id),
                            messages = s.min_messages,
                            seconds = s.min_gap_secs,
                            content = s.content.clone()
                        )
                    })
                    .collect::<Vec<_>>();
                private_with_form(
                    cx,
                    if lines.is_empty() {
                        t!(cx.lang(), "utility.empty")
                    } else {
                        lines.join("\n")
                    },
                    "sk:e",
                    &t!(cx.lang(), "utility.sticky-form-button"),
                )
                .await
            }
        }
    }
}

#[derive(CommandModel, CreateCommand)]
#[command(
    name = "schedule",
    desc = "Post messages on a schedule",
    default_permissions = "manage_guild",
    contexts = "guild"
)]
pub enum ScheduleCommand {
    #[command(name = "add")]
    Add(ScheduleAdd),
    #[command(name = "list")]
    List(ScheduleList),
    #[command(name = "remove")]
    Remove(ScheduleId),
    #[command(name = "pause")]
    Pause(SchedulePause),
    #[command(name = "resume")]
    Resume(ScheduleResume),
    #[command(name = "run-now")]
    RunNow(ScheduleRunNow),
    #[command(name = "preview")]
    Preview(SchedulePreview),
}

#[derive(CommandModel, CreateCommand)]
#[command(name = "add", desc = "Save a once or recurring message")]
pub struct ScheduleAdd {
    /// Channel for the message
    #[command(channel_types = "guild_text guild_announcement")]
    channel: InteractionChannel,
    /// Message text
    #[command(max_length = 2000)]
    text: String,
    /// First run (RFC3339, such as 2026-12-01T09:00:00+03:00)
    when: String,
    /// once, daily, weekly, every:seconds, or cron:minute hour day month weekday
    every: Option<String>,
    /// Fixed UTC offset, such as +03:00
    tz: Option<String>,
}

#[derive(CommandModel, CreateCommand)]
#[command(name = "list", desc = "List scheduled messages")]
pub struct ScheduleList;
#[derive(CommandModel, CreateCommand)]
#[command(name = "remove", desc = "Remove a scheduled message")]
pub struct ScheduleId {
    /// Scheduled message ID
    id: i64,
}
#[derive(CommandModel, CreateCommand)]
#[command(name = "pause", desc = "Pause a scheduled message")]
pub struct SchedulePause {
    /// Scheduled message ID
    id: i64,
}
#[derive(CommandModel, CreateCommand)]
#[command(name = "resume", desc = "Resume a scheduled message")]
pub struct ScheduleResume {
    /// Scheduled message ID
    id: i64,
}
#[derive(CommandModel, CreateCommand)]
#[command(name = "run-now", desc = "Send a scheduled message now")]
pub struct ScheduleRunNow {
    /// Scheduled message ID
    id: i64,
}
#[derive(CommandModel, CreateCommand)]
#[command(name = "preview", desc = "Show the next five runs")]
pub struct SchedulePreview {
    /// Scheduled message ID
    id: i64,
}

fn recurrence(text: Option<&str>, first: DateTime<Utc>, tz: &str) -> Option<Schedule> {
    let value = match text.unwrap_or("once") {
        "once" => Schedule::Once,
        "daily" => {
            let local = first.with_timezone(&offset(tz)?);
            Schedule::Daily {
                hour: chrono::Timelike::hour(&local),
                minute: chrono::Timelike::minute(&local),
            }
        }
        "weekly" => {
            let local = first.with_timezone(&offset(tz)?);
            Schedule::Weekly {
                weekday: chrono::Datelike::weekday(&local).num_days_from_sunday(),
                hour: chrono::Timelike::hour(&local),
                minute: chrono::Timelike::minute(&local),
            }
        }
        other if other.starts_with("every:") => Schedule::Interval {
            seconds: other[6..].parse().ok()?,
        },
        other if other.starts_with("cron:") => {
            let fields: Vec<_> = other[5..].split_whitespace().collect();
            if fields.len() != 5 {
                return None;
            }
            Schedule::Cron {
                minute: fields[0].into(),
                hour: fields[1].into(),
                day: fields[2].into(),
                month: fields[3].into(),
                weekday: fields[4].into(),
            }
        }
        _ => return None,
    };
    pinchora_core::utility::validate(&value).then_some(value)
}

impl ScheduleCommand {
    pub async fn run(self, cx: &Cx) -> anyhow::Result<()> {
        let Some(guild) = cx.guild_id() else {
            return cx.reply_ephemeral(t!(cx.lang(), "utility.guild-only")).await;
        };
        if !cx.can_manage_guild() {
            return cx.reply_ephemeral(t!(cx.lang(), "utility.manage-needed")).await;
        }
        match self {
            Self::Add(v) => {
                let content = clean(&v.text, 2000);
                let tz = match v.tz {
                    Some(tz) => tz,
                    None => cx.app.db.utility_offset(guild).await?,
                };
                let Some(first) = v.when.parse::<DateTime<Utc>>().ok().filter(|at| *at > Utc::now()) else {
                    return cx.reply_ephemeral(t!(cx.lang(), "utility.invalid")).await;
                };
                let Some(schedule) = recurrence(v.every.as_deref(), first, &tz) else {
                    return cx.reply_ephemeral(t!(cx.lang(), "utility.invalid")).await;
                };
                if content.is_empty() || offset(&tz).is_none() {
                    return cx.reply_ephemeral(t!(cx.lang(), "utility.invalid")).await;
                }
                let perms = crate::utility::bot_permissions(&cx.app, guild, v.channel.id.get())
                    .await
                    .unwrap_or(Permissions::empty());
                if !perms.contains(Permissions::SEND_MESSAGES) {
                    return cx.reply_ephemeral(t!(cx.lang(), "utility.permission-send")).await;
                }
                let broad = content.contains("@everyone") || content.contains("@here") || content.contains("<@&");
                let actor_may_ping = !broad
                    || crate::utility::creator_may_ping(&cx.app, guild, v.channel.id.get(), cx.author()?.id.get())
                        .await;
                if broad && (!actor_may_ping || !perms.contains(Permissions::MENTION_EVERYONE)) {
                    return cx.reply_ephemeral(t!(cx.lang(), "utility.permission-mention")).await;
                }
                let row = ScheduledRow {
                    id: 0,
                    guild_id: guild as i64,
                    channel_id: v.channel.id.get() as i64,
                    content,
                    embed: None,
                    schedule: json!(schedule),
                    tz,
                    next_run_at: first,
                    last_run_at: None,
                    runs: 0,
                    enabled: true,
                    lease_until: None,
                    pause_reason: None,
                    created_by: Some(cx.author()?.id.get() as i64),
                    ping_allowed: broad && actor_may_ping && perms.contains(Permissions::MENTION_EVERYONE),
                };
                match cx.app.db.add_schedule(&row).await? {
                    Some(id) => {
                        cx.reply_ephemeral(t!(cx.lang(), "utility.schedule-added", id = id))
                            .await
                    }
                    None => cx.reply_ephemeral(t!(cx.lang(), "utility.full")).await,
                }
            }
            Self::List(_) => {
                let rows = cx.app.db.schedules(guild).await?;
                let lines = rows
                    .iter()
                    .map(|r| {
                        t!(
                            cx.lang(),
                            "utility.schedule-line",
                            id = r.id,
                            channel = format!("<#{}>", r.channel_id),
                            when = r.next_run_at.timestamp(),
                            state = t!(cx.lang(), if r.enabled { "utility.on" } else { "utility.off" }),
                            content = scrub_value(&r.content.chars().take(80).collect::<String>())
                        )
                    })
                    .collect::<Vec<_>>();
                private_with_form(
                    cx,
                    if lines.is_empty() {
                        t!(cx.lang(), "utility.empty")
                    } else {
                        lines.join("\n")
                    },
                    "sm:c",
                    &t!(cx.lang(), "utility.schedule-form-button"),
                )
                .await
            }
            Self::Remove(v) => {
                let found = cx.app.db.remove_schedule(guild, v.id).await?;
                cx.reply_ephemeral(t!(cx.lang(), if found { "utility.removed" } else { "utility.missing" }))
                    .await
            }
            Self::Pause(v) => {
                let found = cx.app.db.enable_schedule(guild, v.id, false).await?;
                cx.reply_ephemeral(t!(cx.lang(), if found { "utility.paused" } else { "utility.missing" }))
                    .await
            }
            Self::Resume(v) => {
                let found = cx.app.db.enable_schedule(guild, v.id, true).await?;
                cx.reply_ephemeral(t!(cx.lang(), if found { "utility.resumed" } else { "utility.missing" }))
                    .await
            }
            Self::RunNow(v) => {
                let Some(row) = cx.app.db.claim_schedule_now(guild, v.id).await? else {
                    return cx.reply_ephemeral(t!(cx.lang(), "utility.missing")).await;
                };
                if !cx.app.db.begin_schedule(&row).await? {
                    return cx.reply_ephemeral(t!(cx.lang(), "utility.missing")).await;
                }
                match crate::utility::send_scheduled(&cx.app, &row).await {
                    Ok(_) => {
                        let rule: Schedule = serde_json::from_value(row.schedule.clone())?;
                        cx.app
                            .db
                            .finish_schedule(&row, next_run(&rule, &row.tz, Utc::now()))
                            .await?;
                    }
                    Err(error) => {
                        cx.app
                            .db
                            .pause_schedule(&row, crate::utility::failure_reason(&error))
                            .await?;
                        return cx.reply_ephemeral(t!(cx.lang(), "utility.delivery-paused")).await;
                    }
                }
                cx.reply_ephemeral(t!(cx.lang(), "utility.saved")).await
            }
            Self::Preview(v) => {
                let Some(row) = cx.app.db.schedule(guild, v.id).await? else {
                    return cx.reply_ephemeral(t!(cx.lang(), "utility.missing")).await;
                };
                let schedule: Schedule = serde_json::from_value(row.schedule)?;
                let runs = preview_runs(&schedule, &row.tz, row.next_run_at, Utc::now())
                    .iter()
                    .map(|at| format!("<t:{}:F>", at.timestamp()))
                    .collect::<Vec<_>>();
                cx.reply_ephemeral(t!(cx.lang(), "utility.schedule-preview", runs = runs.join(" · ")))
                    .await
            }
        }
    }
}

async fn private_with_form(cx: &Cx, content: String, prefix: &str, label: &str) -> anyhow::Result<()> {
    let guild = cx.guild_id().unwrap_or(0);
    let actor = cx.author()?.id.get();
    cx.reply(InteractionResponseData {
        content: Some(content.chars().take(1900).collect()),
        flags: Some(MessageFlags::EPHEMERAL),
        components: Some(vec![row(vec![button(
            format!("{prefix}:{guild}:{actor}"),
            label,
            None,
            ButtonStyle::Primary,
            false,
        )])]),
        ..Default::default()
    })
    .await
}

#[allow(deprecated)]
fn field(id: &str, label: String, max: u16, paragraph: bool) -> Component {
    Component::Label(Label {
        component: Box::new(Component::TextInput(TextInput {
            custom_id: id.into(),
            id: None,
            label: None,
            max_length: Some(max),
            min_length: Some(1),
            placeholder: None,
            required: Some(true),
            style: if paragraph {
                TextInputStyle::Paragraph
            } else {
                TextInputStyle::Short
            },
            value: None,
        })),
        description: None,
        id: None,
        label,
    })
}

pub(crate) async fn on_component(cx: &Cx, prefix: &str, mut parts: Split<'_, char>) -> anyhow::Result<()> {
    let action = parts.next();
    let guild = parts.next().and_then(|s| s.parse::<u64>().ok());
    let actor = parts.next().and_then(|s| s.parse::<u64>().ok());
    if guild != cx.guild_id() || actor != Some(cx.author()?.id.get()) || !cx.can_manage_guild() {
        return cx.reply_ephemeral(t!(cx.lang(), "utility.manage-needed")).await;
    }
    match (prefix, action) {
        ("sk", Some("e")) => {
            cx.modal(
                format!("sk:e:{}:{}", guild.unwrap_or(0), actor.unwrap_or(0)),
                &t!(cx.lang(), "utility.sticky-form-title"),
                vec![
                    field("channel", t!(cx.lang(), "utility.form-channel"), 20, false),
                    field("content", t!(cx.lang(), "utility.form-content"), 1000, true),
                    field("messages", t!(cx.lang(), "utility.form-messages"), 4, false),
                    field("gap", t!(cx.lang(), "utility.form-gap"), 5, false),
                ],
            )
            .await
        }
        ("sm", Some("c")) => {
            cx.modal(
                format!("sm:c:{}:{}", guild.unwrap_or(0), actor.unwrap_or(0)),
                &t!(cx.lang(), "utility.schedule-form-title"),
                vec![
                    field("channel", t!(cx.lang(), "utility.form-channel"), 20, false),
                    field("content", t!(cx.lang(), "utility.form-content"), 2000, true),
                    field("when", t!(cx.lang(), "utility.form-when"), 40, false),
                    field("every", t!(cx.lang(), "utility.form-every"), 80, false),
                    field("tz", t!(cx.lang(), "utility.form-tz"), 6, false),
                ],
            )
            .await
        }
        _ => cx.reply_ephemeral(t!(cx.lang(), "utility.missing")).await,
    }
}

pub(crate) async fn on_modal(
    cx: &Cx,
    prefix: &str,
    mut parts: Split<'_, char>,
    fields: &[(String, String)],
) -> anyhow::Result<()> {
    let action = parts.next();
    let guild = parts.next().and_then(|s| s.parse::<u64>().ok());
    let actor = parts.next().and_then(|s| s.parse::<u64>().ok());
    if guild != cx.guild_id() || actor != Some(cx.author()?.id.get()) || !cx.can_manage_guild() {
        return cx.reply_ephemeral(t!(cx.lang(), "utility.manage-needed")).await;
    }
    let guild = guild.unwrap_or(0);
    let value = |id: &str| {
        fields
            .iter()
            .find(|(key, _)| key == id)
            .map(|(_, value)| value.as_str())
            .unwrap_or("")
    };
    let channel = value("channel").parse::<u64>().ok().filter(|n| *n > 0);
    let Some(channel) = channel else {
        return cx.reply_ephemeral(t!(cx.lang(), "utility.invalid")).await;
    };
    let content = clean(value("content"), if prefix == "sk" { 1000 } else { 2000 });
    if content.is_empty() {
        return cx.reply_ephemeral(t!(cx.lang(), "utility.invalid")).await;
    }
    let permissions = crate::utility::bot_permissions(&cx.app, guild, channel)
        .await
        .unwrap_or(Permissions::empty());
    if !permissions.contains(Permissions::SEND_MESSAGES) {
        return cx.reply_ephemeral(t!(cx.lang(), "utility.permission-send")).await;
    }
    match (prefix, action) {
        ("sk", Some("e")) => {
            if !permissions.contains(Permissions::MANAGE_MESSAGES) {
                return cx.reply_ephemeral(t!(cx.lang(), "utility.permission-manage")).await;
            }
            let messages = value("messages").parse::<i32>().ok().filter(|n| (1..=1000).contains(n));
            let gap = value("gap").parse::<i32>().ok().filter(|n| (5..=86400).contains(n));
            let (Some(messages), Some(gap)) = (messages, gap) else {
                return cx.reply_ephemeral(t!(cx.lang(), "utility.invalid")).await;
            };
            cx.app
                .db
                .upsert_sticky(&UtilityStickyRow {
                    channel_id: channel as i64,
                    guild_id: guild as i64,
                    content,
                    embed: None,
                    last_message_id: None,
                    min_gap_secs: gap,
                    min_messages: messages,
                    message_count: 0,
                    enabled: true,
                    lease_until: None,
                    pause_reason: None,
                    updated_by: Some(actor.unwrap_or(0) as i64),
                })
                .await?;
            cx.reply_ephemeral(t!(cx.lang(), "utility.saved")).await
        }
        ("sm", Some("c")) => {
            let tz = value("tz").to_owned();
            let first = value("when")
                .parse::<DateTime<Utc>>()
                .ok()
                .filter(|at| *at > Utc::now());
            let Some((first, schedule)) =
                first.and_then(|at| recurrence(Some(value("every")), at, &tz).map(|s| (at, s)))
            else {
                return cx.reply_ephemeral(t!(cx.lang(), "utility.invalid")).await;
            };
            let broad = content.contains("@everyone") || content.contains("@here") || content.contains("<@&");
            let actor_may_ping =
                !broad || crate::utility::creator_may_ping(&cx.app, guild, channel, cx.author()?.id.get()).await;
            if broad && (!actor_may_ping || !permissions.contains(Permissions::MENTION_EVERYONE)) {
                return cx.reply_ephemeral(t!(cx.lang(), "utility.permission-mention")).await;
            }
            let row = ScheduledRow {
                id: 0,
                guild_id: guild as i64,
                channel_id: channel as i64,
                content,
                embed: None,
                schedule: json!(schedule),
                tz,
                next_run_at: first,
                last_run_at: None,
                runs: 0,
                enabled: true,
                lease_until: None,
                pause_reason: None,
                created_by: Some(actor.unwrap_or(0) as i64),
                ping_allowed: broad && actor_may_ping && permissions.contains(Permissions::MENTION_EVERYONE),
            };
            match cx.app.db.add_schedule(&row).await? {
                Some(id) => {
                    cx.reply_ephemeral(t!(cx.lang(), "utility.schedule-added", id = id))
                        .await
                }
                None => cx.reply_ephemeral(t!(cx.lang(), "utility.full")).await,
            }
        }
        _ => cx.reply_ephemeral(t!(cx.lang(), "utility.missing")).await,
    }
}
