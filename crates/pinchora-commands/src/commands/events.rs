//! `/event`: managers publish events, members RSVP on the public card.

use std::str::Split;

use chrono::Duration;
use pinchora_core::{
    events::{Recurrence, Repeat, default_options, parse_when},
    t,
};
use pinchora_db::{EventDraft, EventRow, RsvpResult};
use twilight_interactions::command::{CommandModel, CreateCommand};
use twilight_model::{
    channel::message::{
        AllowedMentions, Component, MessageFlags,
        component::{ActionRow, Button, ButtonStyle, Label, TextInput, TextInputStyle},
    },
    http::interaction::InteractionResponseData,
};
use twilight_util::builder::embed::{EmbedBuilder, EmbedFieldBuilder};

fn manage_guild() -> twilight_model::guild::Permissions {
    twilight_model::guild::Permissions::MANAGE_GUILD
}

use crate::{
    Cx,
    event_rsvp::{publish, refresh_card, sync_discord},
    util::BRAND,
};

#[derive(CommandModel, CreateCommand)]
#[command(
    name = "event",
    desc = "Create events and manage RSVPs",
    default_permissions = "manage_guild",
    contexts = "guild"
)]
#[allow(clippy::large_enum_variant)] // parsed once per interaction
pub enum EventCommand {
    #[command(name = "create")]
    Create(EventCreate),
    #[command(name = "list")]
    List(EventList),
    #[command(name = "edit")]
    Edit(EventEdit),
    #[command(name = "cancel")]
    Cancel(EventId),
    #[command(name = "attendees")]
    Attendees(EventAttendees),
    #[command(name = "remind")]
    Remind(EventRemind),
    #[command(name = "template")]
    Template(EventTemplate),
}
#[derive(CommandModel, CreateCommand)]
#[command(name = "create", desc = "Open a form for a new event")]
pub struct EventCreate;
#[derive(CommandModel, CreateCommand)]
#[command(name = "list", desc = "List upcoming events")]
pub struct EventList;
#[derive(CommandModel, CreateCommand)]
#[command(name = "edit", desc = "Change an event, its capacity, role or repeat schedule")]
pub struct EventEdit {
    /// Event ID
    id: i64,
    /// New title
    #[command(max_length = 100)]
    title: Option<String>,
    /// New start time in the server time zone
    #[command(max_length = 60)]
    when: Option<String>,
    /// Duration in minutes
    #[command(min_value = 1, max_value = 10080)]
    duration: Option<i64>,
    /// New description
    #[command(max_length = 1000)]
    description: Option<String>,
    /// Maximum going RSVPs; zero is unlimited
    #[command(min_value = 0, max_value = 100000)]
    capacity: Option<i64>,
    /// Maximum Maybe RSVPs; zero is unlimited
    #[command(min_value = 0, max_value = 100000)]
    maybe_capacity: Option<i64>,
    /// Maximum Declined RSVPs; zero is unlimited
    #[command(min_value = 0, max_value = 100000)]
    declined_capacity: Option<i64>,
    /// First custom RSVP choice; use - to remove it
    #[command(max_length = 40)]
    custom_one: Option<String>,
    /// Capacity for the first custom choice
    #[command(min_value = 0, max_value = 100000)]
    custom_one_capacity: Option<i64>,
    /// Second custom RSVP choice; use - to remove it
    #[command(max_length = 40)]
    custom_two: Option<String>,
    /// Capacity for the second custom choice
    #[command(min_value = 0, max_value = 100000)]
    custom_two_capacity: Option<i64>,
    /// Required role ID; 0 clears it
    role: Option<i64>,
    /// daily, weekly or off
    #[command(max_length = 10)]
    repeat: Option<String>,
    /// Repeat through YYYY-MM-DD (inclusive)
    #[command(max_length = 10)]
    until: Option<String>,
    /// Mirror to Discord Scheduled Events
    sync: Option<bool>,
}
#[derive(CommandModel, CreateCommand)]
#[command(name = "cancel", desc = "Cancel an event")]
pub struct EventId {
    /// Event ID from /event list
    id: i64,
}
#[derive(CommandModel, CreateCommand)]
#[command(name = "attendees", desc = "Show a paged list of RSVPs")]
pub struct EventAttendees {
    /// Event ID from /event list
    id: i64,
}
#[derive(CommandModel, CreateCommand)]
#[command(name = "remind", desc = "Schedule the event reminders again")]
pub struct EventRemind {
    /// Event ID from /event list
    id: i64,
}
#[derive(CommandModel, CreateCommand)]
#[command(name = "template", desc = "Show an event template and create button")]
pub struct EventTemplate;

impl EventCommand {
    pub async fn run(self, cx: &Cx) -> anyhow::Result<()> {
        let Some(guild) = cx.guild_id() else {
            return cx.reply_ephemeral(t!(cx.lang(), "events.guild-only")).await;
        };
        if !cx.can_manage_guild() {
            return cx.reply_ephemeral(t!(cx.lang(), "events.admin-only")).await;
        }
        match self {
            Self::Create(_) => open_form(cx).await,
            Self::List(_) => list(cx, guild).await,
            Self::Edit(c) => edit(cx, guild, c).await,
            Self::Cancel(c) => {
                let Some(existing) = cx.app.db.event(guild, c.id).await? else {
                    return gone(cx).await;
                };
                if existing.discord_event_id.is_some()
                    && let Err(error) = sync_discord(&cx.app, &existing, false).await
                {
                    return cx
                        .reply_ephemeral(t!(cx.lang(), "events.sync-failed", reason = error))
                        .await;
                }
                let Some(row) = cx.app.db.cancel_event(guild, c.id).await? else {
                    return gone(cx).await;
                };
                refresh_card(&cx.app, &row).await;
                cx.reply_ephemeral(t!(cx.lang(), "events.cancelled", id = c.id)).await
            }
            Self::Attendees(c) => attendees(cx, guild, c.id, 0).await,
            Self::Remind(c) => {
                let Some(row) = cx.app.db.event(guild, c.id).await? else {
                    return gone(cx).await;
                };
                crate::event_rsvp::schedule_all(&cx.app, &row).await?;
                cx.reply_ephemeral(t!(cx.lang(), "events.reminders-set", id = c.id))
                    .await
            }
            Self::Template(_) => {
                let button = Component::ActionRow(ActionRow {
                    id: None,
                    components: vec![button(
                        t!(cx.lang(), "events.create"),
                        "ev:c".into(),
                        ButtonStyle::Primary,
                    )],
                });
                cx.reply(InteractionResponseData {
                    content: Some(t!(cx.lang(), "events.template")),
                    components: Some(vec![button]),
                    flags: Some(MessageFlags::EPHEMERAL),
                    ..Default::default()
                })
                .await
            }
        }
    }
}

fn button(label: String, custom_id: String, style: ButtonStyle) -> Component {
    Component::Button(Button {
        id: None,
        custom_id: Some(custom_id),
        disabled: false,
        emoji: None,
        label: Some(label),
        style,
        url: None,
        sku_id: None,
    })
}
#[allow(deprecated)]
fn field(id: &str, label: String, max: u16, required: bool, paragraph: bool) -> Component {
    Component::Label(Label {
        id: None,
        label,
        description: None,
        component: Box::new(Component::TextInput(TextInput {
            custom_id: id.into(),
            id: None,
            label: None,
            max_length: Some(max),
            min_length: required.then_some(1),
            placeholder: None,
            required: Some(required),
            style: if paragraph {
                TextInputStyle::Paragraph
            } else {
                TextInputStyle::Short
            },
            value: None,
        })),
    })
}
async fn open_form(cx: &Cx) -> anyhow::Result<()> {
    let l = cx.lang();
    let Some(guild) = cx.guild_id() else {
        return gone(cx).await;
    };
    let timezone = cx.app.db.event_timezone_for(guild, cx.author()?.id.get()).await?;
    cx.modal(
        "ev:c".into(),
        &t!(l, "events.form-title"),
        vec![
            field("title", t!(l, "events.field-title"), 100, true, false),
            field("when", t!(l, "events.field-when", tz = timezone), 60, true, false),
            field("duration", t!(l, "events.field-duration"), 5, true, false),
            field("description", t!(l, "events.field-description"), 1000, false, true),
        ],
    )
    .await
}
fn value<'a>(fields: &'a [(String, String)], key: &str) -> &'a str {
    fields.iter().find(|(id, _)| id == key).map_or("", |(_, v)| v.as_str())
}

pub(crate) async fn on_modal(cx: &Cx, mut parts: Split<'_, char>, fields: &[(String, String)]) -> anyhow::Result<()> {
    if parts.next() != Some("c") || !cx.can_manage_guild() {
        return cx.reply_ephemeral(t!(cx.lang(), "events.admin-only")).await;
    }
    let Some(guild) = cx.guild_id() else {
        return gone(cx).await;
    };
    let tz_name = cx.app.db.event_timezone_for(guild, cx.author()?.id.get()).await?;
    let tz: chrono_tz::Tz = tz_name.parse()?;
    let start = match parse_when(value(fields, "when"), cx.app.now(), tz) {
        Ok(v) if v > cx.app.now() => v,
        _ => return cx.reply_ephemeral(t!(cx.lang(), "events.bad-time", tz = tz_name)).await,
    };
    let minutes: i64 = match value(fields, "duration").parse() {
        Ok(v) if (1..=10080).contains(&v) => v,
        _ => return cx.reply_ephemeral(t!(cx.lang(), "events.bad-duration")).await,
    };
    let draft = EventDraft {
        title: value(fields, "title").into(),
        description: value(fields, "description").into(),
        starts_at: start,
        ends_at: start + Duration::minutes(minutes),
        tz: tz_name,
        location: String::new(),
        channel_id: cx.interaction.channel.as_ref().map(|c| c.id.get() as i64),
        host_id: Some(cx.author()?.id.get() as i64),
        options: default_options(),
        required_role_id: None,
        reminders: vec![60, 15],
        recurrence: None,
    };
    if draft.validate().is_err() {
        return cx.reply_ephemeral(t!(cx.lang(), "events.invalid")).await;
    }
    let row = cx.app.db.create_event(guild, &draft).await?;
    publish(&cx.app, &row).await?;
    cx.reply_ephemeral(t!(cx.lang(), "events.created", id = row.id)).await
}

async fn list(cx: &Cx, guild: u64) -> anyhow::Result<()> {
    let rows = cx.app.db.events(guild, false).await?;
    if rows.is_empty() {
        return cx.reply_ephemeral(t!(cx.lang(), "events.empty")).await;
    }
    let lines = rows
        .iter()
        .take(20)
        .map(|r| {
            format!(
                "`#{}` **{}** <t:{}:F> · <t:{}:R>",
                r.id,
                r.title,
                r.starts_at.timestamp(),
                r.starts_at.timestamp()
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    cx.reply(InteractionResponseData {
        embeds: Some(vec![
            EmbedBuilder::new()
                .color(BRAND)
                .title(t!(cx.lang(), "events.list-title"))
                .description(lines)
                .build(),
        ]),
        flags: Some(MessageFlags::EPHEMERAL),
        ..Default::default()
    })
    .await
}
fn draft_of(row: &EventRow) -> EventDraft {
    EventDraft {
        title: row.title.clone(),
        description: row.description.clone(),
        starts_at: row.starts_at,
        ends_at: row.ends_at,
        tz: row.tz.clone(),
        location: row.location.clone(),
        channel_id: row.channel_id,
        host_id: row.host_id,
        options: row.options(),
        required_role_id: row.required_role_id,
        reminders: row.reminders.clone(),
        recurrence: row.recurrence(),
    }
}
async fn edit(cx: &Cx, guild: u64, c: EventEdit) -> anyhow::Result<()> {
    let Some(old) = cx.app.db.event(guild, c.id).await? else {
        return gone(cx).await;
    };
    if (c.sync == Some(true) || old.discord_event_id.is_some())
        && let Err(e) = crate::event_rsvp::check_manage_events(&cx.app, guild).await
    {
        return cx
            .reply_ephemeral(t!(cx.lang(), "events.sync-failed", reason = e))
            .await;
    }
    let mut d = draft_of(&old);
    if let Some(v) = c.title {
        d.title = v;
    }
    if let Some(v) = c.description {
        d.description = v;
    }
    if let Some(v) = c.when {
        let tz = d.tz.parse()?;
        d.starts_at = match parse_when(&v, cx.app.now(), tz) {
            Ok(at) => at,
            Err(_) => return cx.reply_ephemeral(t!(cx.lang(), "events.bad-time", tz = d.tz)).await,
        };
        d.ends_at = d.starts_at + (old.ends_at - old.starts_at);
    }
    if let Some(m) = c.duration {
        d.ends_at = d.starts_at + Duration::minutes(m);
    }
    if let Some(n) = c.capacity {
        d.options[0].capacity = n as i32;
    }
    if let Some(n) = c.maybe_capacity {
        d.options[1].capacity = n as i32;
    }
    if let Some(n) = c.declined_capacity {
        d.options[2].capacity = n as i32;
    }
    for (index, label, capacity) in [
        (3, c.custom_one, c.custom_one_capacity),
        (4, c.custom_two, c.custom_two_capacity),
    ] {
        if let Some(label) = label {
            if label == "-" {
                d.options.truncate(index);
            } else if let Some(option) = d.options.get_mut(index) {
                option.label = label;
            } else if d.options.len() == index {
                d.options.push(pinchora_core::events::RsvpOption {
                    key: format!("extra{}", index - 2),
                    label,
                    capacity: 0,
                });
            }
        }
        if let Some(n) = capacity
            && let Some(option) = d.options.get_mut(index)
        {
            option.capacity = n as i32;
        }
    }
    if let Some(id) = c.role {
        d.required_role_id = (id > 0).then_some(id);
    }
    if let Some(repeat) = c.repeat {
        d.recurrence = match repeat.as_str() {
            "off" => None,
            "daily" | "weekly" => {
                let Some(date) = c
                    .until
                    .as_deref()
                    .and_then(|s| chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d").ok())
                else {
                    return cx.reply_ephemeral(t!(cx.lang(), "events.bad-until")).await;
                };
                Some(Recurrence {
                    repeat: if repeat == "daily" {
                        Repeat::Daily
                    } else {
                        Repeat::Weekly
                    },
                    until: date,
                })
            }
            _ => return cx.reply_ephemeral(t!(cx.lang(), "events.bad-repeat")).await,
        }
    }
    let Some(row) = cx.app.db.update_event(guild, c.id, &d).await? else {
        return gone(cx).await;
    };
    if let Some(sync) = c.sync.or_else(|| row.discord_event_id.map(|_| true))
        && let Err(e) = sync_discord(&cx.app, &row, sync).await
    {
        return cx
            .reply_ephemeral(t!(cx.lang(), "events.sync-failed", reason = e))
            .await;
    }
    refresh_card(&cx.app, &row).await;
    crate::event_rsvp::schedule_all(&cx.app, &row).await?;
    cx.reply_ephemeral(t!(cx.lang(), "events.updated", id = c.id)).await
}
async fn gone(cx: &Cx) -> anyhow::Result<()> {
    cx.reply_ephemeral(t!(cx.lang(), "events.gone")).await
}
async fn attendees(cx: &Cx, guild: u64, id: i64, page: i64) -> anyhow::Result<()> {
    let Some(row) = cx.app.db.event(guild, id).await? else {
        return gone(cx).await;
    };
    let count = cx.app.db.event_rsvp_count(id).await?;
    let people = cx.app.db.event_rsvps(id, page * 20, 20).await?;
    let lines = people
        .iter()
        .map(|r| {
            format!(
                "<@{}> — {}{}",
                r.user_id,
                r.option,
                if r.waitlisted {
                    format!(" ({})", t!(cx.lang(), "events.waitlisted"))
                } else {
                    String::new()
                }
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let mut components = Vec::new();
    if count > 20 {
        components.push(Component::ActionRow(ActionRow {
            id: None,
            components: vec![
                button(
                    t!(cx.lang(), "events.previous"),
                    format!("ev:p:{id}:{}", (page - 1).max(0)),
                    ButtonStyle::Secondary,
                ),
                button(
                    t!(cx.lang(), "events.next"),
                    format!("ev:p:{id}:{}", if (page + 1) * 20 < count { page + 1 } else { page }),
                    ButtonStyle::Secondary,
                ),
            ],
        }));
    }
    cx.reply(InteractionResponseData {
        embeds: Some(vec![
            EmbedBuilder::new()
                .color(BRAND)
                .title(t!(
                    cx.lang(),
                    "events.attendees-title",
                    title = row.title,
                    count = count
                ))
                .description(if lines.is_empty() {
                    t!(cx.lang(), "events.no-attendees")
                } else {
                    lines
                })
                .field(
                    EmbedFieldBuilder::new(
                        t!(cx.lang(), "events.page"),
                        format!("{} / {}", page + 1, ((count + 19) / 20).max(1)),
                    )
                    .build(),
                )
                .build(),
        ]),
        components: Some(components),
        flags: Some(MessageFlags::EPHEMERAL),
        allowed_mentions: Some(AllowedMentions::default()),
        ..Default::default()
    })
    .await
}
pub(crate) async fn on_component(cx: &Cx, mut parts: Split<'_, char>) -> anyhow::Result<()> {
    let Some(guild) = cx.guild_id() else {
        return gone(cx).await;
    };
    match parts.next() {
        Some("c") if cx.can_manage_guild() => open_form(cx).await,
        Some("p") if cx.can_manage_guild() => {
            let id = parts.next().unwrap_or("").parse().unwrap_or(0);
            let page = parts.next().unwrap_or("").parse::<i64>().unwrap_or(0).max(0);
            attendees(cx, guild, id, page).await
        }
        Some("r") => {
            let id = parts.next().unwrap_or("").parse().unwrap_or(0);
            let option = parts.next().unwrap_or("");
            let Some(row) = cx.app.db.event(guild, id).await? else {
                return gone(cx).await;
            };
            if cx.interaction.message.as_ref().map(|m| m.id.get() as i64) != row.message_id {
                return gone(cx).await;
            }
            if let Some(role) = row.required_role_id
                && !cx
                    .interaction
                    .member
                    .as_ref()
                    .is_some_and(|m| m.roles.iter().any(|r| r.get() == role as u64))
            {
                return cx
                    .reply_ephemeral(t!(cx.lang(), "events.required-role", role = role))
                    .await;
            }
            let user = cx.author()?.id.get();
            let result = cx.app.db.rsvp_event(guild, id, user, option).await?;
            if result == RsvpResult::Confirmed && option != "declined" {
                crate::event_rsvp::schedule_user(&cx.app, &row, user).await?;
            } else {
                crate::event_rsvp::unschedule_user(&cx.app, id, user).await?;
            }
            refresh_card(&cx.app, &row).await;
            let key = match result {
                RsvpResult::Confirmed => "events.rsvp-confirmed",
                RsvpResult::Waitlisted => "events.rsvp-waitlisted",
                RsvpResult::Removed => "events.rsvp-removed",
                _ => "events.gone",
            };
            cx.reply_ephemeral(t!(cx.lang(), key)).await
        }
        _ => gone(cx).await,
    }
}
