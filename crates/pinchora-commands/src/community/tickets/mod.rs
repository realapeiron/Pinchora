//! Tickets: members open a private channel (or private thread) from a panel's button or menu, staff
//! claim, lock and close it, and a transcript is kept, logged and sent to the member when it closes.
//!
//! Custom ids carry everything: `tk:o:<panel>:<topic>` (a panel button), `tk:s:<panel>` (a panel menu),
//! `tf:<panel>:<topic>` (the panel's form), `tp:<action>:<ticket>` (the controls inside a ticket) and
//! `tr:<ticket>:<stars>` (the rating a member gets by direct message). Closing is claimed in the database
//! first, so on many pods a ticket closes, logs and is transcribed exactly once.

mod actions;
mod open;
mod panel;
mod transcript;

use std::{sync::Arc, time::Duration};

use pinchora_core::admin::modules::Module;
use pinchora_core::community::tickets::Refusal;
use pinchora_core::infra::i18n::Lang;
use pinchora_core::t;
use pinchora_db::{Ticket, TicketPanel, TicketSettings};
use twilight_http::request::channel::message::CreateMessage;
use twilight_model::{
    channel::Message,
    channel::message::{AllowedMentions, Component, Embed, MessageFlags},
    guild::Permissions,
    http::{attachment::Attachment, interaction::InteractionResponseData},
    id::{
        Id,
        marker::{ChannelMarker, RoleMarker, UserMarker},
    },
};

/// Enforce required and exclusive claim modes for direct Discord replies by staff.
pub(crate) async fn on_guild_message(app: &Arc<App>, msg: &Message) -> crate::infra::hooks::Verdict {
    let Some(guild) = msg.guild_id.map(Id::get) else {
        return crate::infra::hooks::Verdict::Continue;
    };
    if !module_on(app, guild, Some(msg.channel_id.get())).await {
        return crate::infra::hooks::Verdict::Continue;
    }
    let Some(ticket) = (match app.db.ticket_at(guild, msg.channel_id.get()).await {
        Ok(ticket) => ticket,
        Err(error) => {
            tracing::warn!(guild, %error, "ticket claim mode not checked");
            return crate::infra::hooks::Verdict::Continue;
        }
    })
    .filter(Ticket::is_open) else {
        return crate::infra::hooks::Verdict::Continue;
    };
    if ticket.opener_id as u64 == msg.author.id.get() {
        return crate::infra::hooks::Verdict::Continue;
    }
    let Some(panel) = (match ticket.panel_id {
        Some(id) => app.db.ticket_panel(id).await.ok().flatten(),
        None => None,
    }) else {
        return crate::infra::hooks::Verdict::Continue;
    };
    let Ok(config) = app.db.ticket_panel_operations(panel.id).await else {
        return crate::infra::hooks::Verdict::Continue;
    };
    if config.claim_mode == "optional" {
        return crate::infra::hooks::Verdict::Continue;
    }
    let held = msg.member.as_ref().map(|member| member.roles.as_slice()).unwrap_or(&[]);
    let support = held
        .iter()
        .any(|role| panel.support_roles.contains(&(role.get() as i64)));
    let manager = msg
        .member
        .as_ref()
        .and_then(|member| member.permissions)
        .is_some_and(|permissions| STAFF_PERMISSIONS.iter().any(|needed| permissions.contains(*needed)))
        || match app.http.roles(Id::new(guild)).await {
            Ok(response) => response.models().await.is_ok_and(|roles| {
                roles.iter().any(|role| {
                    held.contains(&role.id)
                        && STAFF_PERMISSIONS
                            .iter()
                            .any(|needed| role.permissions.contains(*needed))
                })
            }),
            Err(_) => false,
        }
        || match app.http.guild(Id::new(guild)).await {
            Ok(response) => response
                .model()
                .await
                .is_ok_and(|guild| guild.owner_id == msg.author.id),
            Err(_) => false,
        };
    if !support && !manager {
        return crate::infra::hooks::Verdict::Continue;
    }
    let blocked = ticket.claimed_by.is_none()
        || config.claim_mode == "exclusive" && ticket.claimed_by != Some(msg.author.id.get() as i64);
    if !blocked {
        return crate::infra::hooks::Verdict::Continue;
    }
    if let Err(error) = app.http.delete_message(msg.channel_id, msg.id).await {
        tracing::warn!(ticket = ticket.id, %error, "unclaimed staff reply not removed");
        return crate::infra::hooks::Verdict::Continue;
    }
    let lang = guild_lang(app, guild).await;
    let text = t!(
        lang,
        if ticket.claimed_by.is_none() {
            "tickets.v2.claimed-required"
        } else {
            "tickets.v2.exclusive-claim"
        }
    );
    let mentions = AllowedMentions::default();
    let _ = app
        .http
        .create_message(msg.channel_id)
        .content(&text)
        .allowed_mentions(Some(&mentions))
        .await;
    crate::infra::hooks::Verdict::Blocked
}

pub(crate) use crate::community::tickets::actions::TicketAct;
pub(crate) use crate::community::tickets::actions::on_control;
pub(crate) use crate::community::tickets::actions::on_rating;
pub(crate) use crate::community::tickets::actions::run_action;
pub use crate::community::tickets::actions::spawn_ticker;
pub use crate::community::tickets::actions::tick;
pub(crate) use crate::community::tickets::open::on_form;
pub(crate) use crate::community::tickets::open::on_message_context;
pub(crate) use crate::community::tickets::open::on_open;
pub use crate::community::tickets::panel::PanelPost;
pub use crate::community::tickets::panel::post_panel;

/// A deleted ticket place is closed through the regular close path, once across gateway pods.
pub async fn on_place_deleted(app: &Arc<App>, guild: u64, place: u64) -> anyhow::Result<()> {
    let Some(ticket) = app.db.ticket_at(guild, place).await?.filter(Ticket::is_open) else {
        return Ok(());
    };
    let panel = match ticket.panel_id {
        Some(id) => app.db.ticket_panel(id).await?,
        None => None,
    };
    let lang = guild_lang(app, guild).await;
    crate::community::tickets::actions::close(
        app,
        ticket,
        panel,
        None,
        Some(t!(lang, "tickets.channel-deleted")),
        None,
    )
    .await?;
    Ok(())
}

/// Dashboard close uses the same transcript, log and DM path as Discord controls.
pub async fn close_from_web(app: &Arc<App>, ticket: Ticket, by: u64, name: String) -> anyhow::Result<Option<Ticket>> {
    let panel = match ticket.panel_id {
        Some(id) => app.db.ticket_panel(id).await?,
        None => None,
    };
    crate::community::tickets::actions::close(app, ticket, panel, Some((by, name)), None, None).await
}

/// Create or edit the configured status message. The ticker also calls this after missed events.
pub async fn refresh_status_board(app: &Arc<App>, guild: u64) -> anyhow::Result<()> {
    let worker = app.clone();
    pinchora_services::infra::Job::new("ticket_status")
        .leased(
            &app.services.infrastructure,
            format!("ticket-status:{}", guild as i64),
            async move { refresh_status_board_locked(&worker, guild).await },
        )
        .await?;
    Ok(())
}

async fn refresh_status_board_locked(app: &Arc<App>, guild: u64) -> anyhow::Result<()> {
    let settings = app.db.ticket_operation_settings(guild).await?;
    let Some(channel) = settings.status_channel_id.map(|id| id as u64) else {
        return Ok(());
    };
    if app
        .cache
        .try_cooldown(&format!("ticket-status:{guild}"), Duration::from_secs(30))
        .await?
        .is_some()
    {
        return Ok(());
    }
    let (open, claimed, closed_today) = app.db.ticket_status_counts(guild).await?;
    let lang = app.guild_lang(guild).await;
    let embed = twilight_util::builder::embed::EmbedBuilder::new()
        .title(t!(lang, "tickets.v2.board-title"))
        .description(t!(
            lang,
            "tickets.v2.board-body",
            open = open,
            claimed = claimed,
            closed = closed_today
        ))
        .build();
    let embeds = [app.brand_embed(guild, embed).await];
    let mentions = AllowedMentions::default();
    if let Some(message) = settings.status_message_id {
        let edited = app
            .http
            .update_message(Id::new(channel), Id::new(message as u64))
            .embeds(Some(&embeds))
            .allowed_mentions(Some(&mentions))
            .await;
        match edited {
            Ok(_) => return Ok(()),
            Err(error) if crate::roles::roles::error_code(&error) == Some(10008) => {}
            Err(error) => return Err(error.into()),
        }
    }
    let sent = app
        .http
        .create_message(Id::new(channel))
        .embeds(&embeds)
        .allowed_mentions(Some(&mentions))
        .await?
        .model()
        .await?;
    if !app.db.set_ticket_status_message(guild, channel, sent.id.get()).await? {
        app.http.delete_message(Id::new(channel), sent.id).await?;
    }
    Ok(())
}

use crate::App;
use crate::Cx;
use crate::commands::games::games::quiet;

/// The ticker wakes this often to look for tickets nobody wrote in.
const WAKE: Duration = Duration::from_secs(300);

/// What staff may do in every ticket, whatever its panel: run the server or its channels.
const STAFF_PERMISSIONS: [Permissions; 3] = [
    Permissions::ADMINISTRATOR,
    Permissions::MANAGE_GUILD,
    Permissions::MANAGE_CHANNELS,
];

/// What the opener, added members and support roles may do in a ticket's channel.
pub(crate) fn member_allow() -> Permissions {
    Permissions::VIEW_CHANNEL
        | Permissions::SEND_MESSAGES
        | Permissions::READ_MESSAGE_HISTORY
        | Permissions::ATTACH_FILES
        | Permissions::EMBED_LINKS
}

/// A private answer that pings nobody.
pub(crate) async fn tell(cx: &Cx, text: impl Into<String>) -> anyhow::Result<()> {
    crate::community::server_events::tell(cx, text).await
}

/// Whether the member behind `cx` is staff for tickets of `panel`: Manage Server, Manage Channels or
/// Administrator, or one of the panel's support roles.
pub(crate) fn is_staff(cx: &Cx, panel: Option<&TicketPanel>) -> bool {
    let Some(member) = cx.interaction.member.as_ref() else {
        return false;
    };
    if member
        .permissions
        .is_some_and(|p| STAFF_PERMISSIONS.iter().any(|needed| p.contains(*needed)))
    {
        return true;
    }
    panel.is_some_and(|panel| {
        member
            .roles
            .iter()
            .any(|role| panel.support_roles.contains(&(role.get() as i64)))
    })
}

/// The ticket of the channel `cx` came from, or a private answer saying this is no ticket (and `None`).
pub(crate) async fn here(cx: &Cx) -> anyhow::Result<Option<(Ticket, Option<TicketPanel>)>> {
    let (Some(guild), Some(channel)) = (cx.guild_id(), cx.interaction.channel.as_ref().map(|c| c.id.get())) else {
        tell(cx, t!(cx.lang(), "tickets.not-a-ticket")).await?;
        return Ok(None);
    };
    let Some(ticket) = cx.app.db.ticket_at(guild, channel).await? else {
        tell(cx, t!(cx.lang(), "tickets.not-a-ticket")).await?;
        return Ok(None);
    };
    let panel = match ticket.panel_id {
        Some(id) => cx.app.db.ticket_panel(id).await?,
        None => None,
    };
    Ok(Some((ticket, panel)))
}

/// A refusal of the ticket rules, said privately.
pub(crate) async fn refuse(cx: &Cx, refusal: Refusal) -> anyhow::Result<()> {
    tell(cx, refusal.text(cx.lang())).await
}

/// `#0042`: how a ticket's number is shown.
pub(crate) fn number(n: i32) -> String {
    format!("#{n:04}")
}

/// The panel's override, else the server's setting.
pub(crate) fn log_channel(settings: &TicketSettings, panel: Option<&TicketPanel>) -> Option<u64> {
    panel
        .and_then(|p| p.log_channel_id)
        .or(settings.log_channel_id)
        .map(|id| id as u64)
}

/// Posts `embed` (and `file`, a transcript) in the ticket log; a log that cannot be written to is only
/// noted, it never stops the ticket.
pub(crate) async fn log(app: &App, guild: u64, channel: Option<u64>, embed: Embed, file: Option<(String, Vec<u8>)>) {
    let Some(channel) = channel else { return };
    let embeds = [app.brand_embed(guild, embed).await];
    let mentions = quiet();
    let request: CreateMessage<'_> = app
        .http
        .create_message(Id::new(channel))
        .embeds(&embeds)
        .allowed_mentions(mentions.as_ref());
    if let Err(error) = request.await {
        tracing::warn!(channel, %error, "ticket log not written");
    }
    if let Some((name, bytes)) = file {
        let files = [Attachment::from_bytes(name, bytes, 1)];
        if let Err(error) = app.http.create_message(Id::new(channel)).attachments(&files).await {
            tracing::warn!(channel, %error, "ticket transcript not uploaded to log");
        }
    }
}

/// Pings exactly these users and roles and nobody else.
pub(crate) fn pinging(users: &[u64], roles: &[u64]) -> AllowedMentions {
    AllowedMentions {
        users: users.iter().map(|u| Id::<UserMarker>::new(*u)).collect(),
        roles: roles.iter().map(|r| Id::<RoleMarker>::new(*r)).collect(),
        ..AllowedMentions::default()
    }
}

/// A private answer with components.
pub(crate) fn private(content: String, components: Vec<Component>) -> InteractionResponseData {
    InteractionResponseData {
        content: Some(content),
        components: Some(components),
        flags: Some(MessageFlags::EPHEMERAL),
        allowed_mentions: quiet(),
        ..Default::default()
    }
}

/// The channel id of a ticket's place, for REST calls.
pub(crate) fn place_of(ticket: &Ticket) -> Option<Id<ChannelMarker>> {
    ticket.place().map(Id::new)
}

/// Whether the tickets module works for the guild (and the place) a timer acts on.
pub(crate) async fn module_on(app: &App, guild: u64, place: Option<u64>) -> bool {
    app.module_allowed(guild, place, Module::Tickets).await
}

/// The server's name, for transcripts and direct messages; empty when Discord will not say.
pub(crate) async fn server_name(app: &App, guild: u64) -> String {
    match app.http.guild(Id::new(guild)).await {
        Ok(response) => response.model().await.map(|g| g.name).unwrap_or_default(),
        Err(error) => {
            tracing::debug!(guild, %error, "server name not read");
            String::new()
        }
    }
}

/// The language of messages a ticket posts for everybody in it.
pub(crate) async fn guild_lang(app: &Arc<App>, guild: u64) -> Lang {
    app.guild_lang(guild).await
}

const UNKNOWN_CHANNEL: u64 = 10003;
const MISSING_ACCESS: u64 = 50001;
const MISSING_PERMISSIONS: u64 = 50013;
const MAX_CHANNELS: u64 = 30013;

/// What the bot was trying when Discord said no, for the sentence that names what it lacks.
#[derive(Debug, Clone, Copy)]
pub(crate) enum Need {
    /// Posting a message in the channel.
    Post(u64),
    /// Making a ticket channel (under a category, when there is one).
    Channel,
    /// Making a private thread off the channel.
    Thread(u64),
    /// Changing the ticket's channel or thread: who sees it, its name, its category.
    Manage(u64),
}

/// Why Discord refused, in `lang`, naming the exact permission the bot needs for `need`.
pub(crate) fn discord_refusal(error: &twilight_http::Error, lang: Lang, need: Need) -> String {
    match crate::roles::roles::error_code(error) {
        Some(MISSING_ACCESS | MISSING_PERMISSIONS) => match need {
            Need::Post(channel) => t!(lang, "tickets.discord.post", channel = format!("<#{channel}>")),
            Need::Channel => t!(lang, "tickets.discord.channel"),
            Need::Thread(channel) => t!(lang, "tickets.discord.thread", channel = format!("<#{channel}>")),
            Need::Manage(channel) => t!(lang, "tickets.discord.manage", channel = format!("<#{channel}>")),
        },
        Some(UNKNOWN_CHANNEL) => t!(lang, "tickets.discord.gone"),
        Some(MAX_CHANNELS) => t!(lang, "tickets.discord.full"),
        Some(_) => t!(lang, "tickets.discord.refused"),
        None => t!(lang, "tickets.discord.unreachable"),
    }
}

/// The permissions the bot lacks for `need`, as a sentence in `cx`'s language (`None` when it has them
/// all or Discord did not say), from what the interaction says the bot has in the channel it came from.
pub(crate) fn missing(cx: &Cx, need: Need) -> Option<String> {
    let keys = missing_keys(cx, need);
    if keys.is_empty() {
        return None;
    }
    let names: Vec<String> = keys
        .iter()
        .map(|key| format!("**{}**", t!(cx.lang(), &format!("tickets.perm.{key}"))))
        .collect();
    Some(t!(cx.lang(), "tickets.discord.lacking", permissions = names.join(", ")))
}

fn missing_keys(cx: &Cx, need: Need) -> Vec<&'static str> {
    let Some(have) = cx.interaction.app_permissions else {
        return Vec::new();
    };
    if have.contains(Permissions::ADMINISTRATOR) {
        return Vec::new();
    }
    let wanted: &[(Permissions, &str)] = match need {
        Need::Channel => &[
            (Permissions::MANAGE_CHANNELS, "manage-channels"),
            (Permissions::MANAGE_ROLES, "manage-roles"),
            (Permissions::VIEW_CHANNEL, "view-channel"),
            (Permissions::SEND_MESSAGES, "send-messages"),
            (Permissions::EMBED_LINKS, "embed-links"),
            (Permissions::READ_MESSAGE_HISTORY, "read-history"),
        ],
        Need::Thread(_) => &[
            (Permissions::CREATE_PRIVATE_THREADS, "private-threads"),
            (Permissions::SEND_MESSAGES_IN_THREADS, "send-in-threads"),
            (Permissions::MANAGE_THREADS, "manage-threads"),
            (Permissions::READ_MESSAGE_HISTORY, "read-history"),
        ],
        Need::Post(_) => &[
            (Permissions::VIEW_CHANNEL, "view-channel"),
            (Permissions::SEND_MESSAGES, "send-messages"),
            (Permissions::EMBED_LINKS, "embed-links"),
        ],
        Need::Manage(_) => &[
            (Permissions::MANAGE_CHANNELS, "manage-channels"),
            (Permissions::MANAGE_ROLES, "manage-roles"),
        ],
    };
    wanted
        .iter()
        .filter(|(bit, _)| !have.contains(*bit))
        .map(|(_, name)| *name)
        .collect()
}

pub(crate) use crate::community::tickets::open::overwrites as workflow_overwrites;

pub(crate) use crate::community::tickets::actions::controls as workflow_controls;
pub(crate) use crate::community::tickets::open::welcome_embed as workflow_welcome_embed;
pub(crate) use server_name as workflow_server_name;

pub(crate) use crate::community::tickets::open::eligibility_refusal;
