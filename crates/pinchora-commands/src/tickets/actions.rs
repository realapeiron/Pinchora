//! Everything done to an open or closed ticket, from its buttons (`tp:`), from `/ticket` and from the
//! ticker that closes the ones nobody wrote in: claim, lock, add and remove people, rename, move, close,
//! reopen, delete, the transcript, and the rating its opener gives afterwards (`tr:`).

use std::{str::Split, sync::Arc};

use pinchora_core::{
    i18n::Lang,
    snowflake, t,
    tickets::{Action, REASON_MAX, Refusal, channel_name, may, transition},
};
use pinchora_db::{Ticket, TicketPanel, TicketSettings, idle_cutoff};
use twilight_model::{
    channel::message::{
        Component,
        component::{ActionRow, ButtonStyle},
    },
    guild::Permissions,
    http::{
        attachment::Attachment,
        interaction::InteractionResponseData,
        permission_overwrite::{PermissionOverwrite, PermissionOverwriteType},
    },
    id::{
        Id,
        marker::{ChannelMarker, UserMarker},
    },
};
use twilight_util::builder::embed::EmbedBuilder;

use super::{
    Need, WAKE, discord_refusal, guild_lang, here, is_staff, log, log_channel, member_allow, missing, module_on,
    number,
    open::log_embed,
    place_of, private, refuse, server_name, tell,
    transcript::{self, Captured, Page},
};
use crate::{
    App, Cx,
    commands::games::quiet,
    custom::scrub_value,
    server_events::{button, row},
    util::{DANGER, SUCCESS, WARN},
};

const UNKNOWN_CHANNEL: u64 = 10003;

/// The controls of an open ticket, on its welcome message and after a reopen.
pub(crate) fn controls(ticket: i64, locked: bool, lang: Lang) -> Vec<Component> {
    vec![row(vec![
        button(
            format!("tp:cl:{ticket}"),
            &t!(lang, "tickets.button.close"),
            Some("🔒"),
            ButtonStyle::Danger,
            false,
        ),
        button(
            format!("tp:cm:{ticket}"),
            &t!(lang, "tickets.button.claim"),
            Some("🙋"),
            ButtonStyle::Success,
            false,
        ),
        button(
            format!("tp:tr:{ticket}"),
            &t!(lang, "tickets.button.transcript"),
            Some("📄"),
            ButtonStyle::Secondary,
            false,
        ),
        button(
            format!("tp:lk:{ticket}"),
            &t!(
                lang,
                if locked {
                    "tickets.button.unlock"
                } else {
                    "tickets.button.lock"
                }
            ),
            Some(if locked { "🔓" } else { "🔐" }),
            ButtonStyle::Secondary,
            false,
        ),
    ])]
}

/// The controls of a closed ticket's channel.
fn closed_controls(ticket: i64, lang: Lang) -> Vec<Component> {
    vec![row(vec![
        button(
            format!("tp:ro:{ticket}"),
            &t!(lang, "tickets.button.reopen"),
            Some("🔓"),
            ButtonStyle::Success,
            false,
        ),
        button(
            format!("tp:tr:{ticket}"),
            &t!(lang, "tickets.button.transcript"),
            Some("📄"),
            ButtonStyle::Secondary,
            false,
        ),
        button(
            format!("tp:dl:{ticket}"),
            &t!(lang, "tickets.button.delete"),
            Some("🗑️"),
            ButtonStyle::Danger,
            false,
        ),
    ])]
}

/// The stars a ticket's opener picks from, in the direct message after it closes.
fn rating_row(ticket: i64) -> Component {
    Component::ActionRow(ActionRow {
        id: None,
        components: (1..=5)
            .map(|stars| {
                button(
                    format!("tr:{ticket}:{stars}"),
                    &"★".repeat(stars),
                    None,
                    ButtonStyle::Secondary,
                    false,
                )
            })
            .collect(),
    })
}

/// What `/ticket` asks for, done in the ticket the command is run in.
pub(crate) enum TicketAct {
    Close(Option<String>),
    Claim,
    Unclaim,
    Add(u64),
    Remove(u64),
    Rename(String),
    Transcript,
    Reopen,
    Move(u64),
}

impl TicketAct {
    fn action(&self) -> Action {
        match self {
            Self::Close(_) => Action::Close,
            Self::Claim => Action::Claim,
            Self::Unclaim => Action::Unclaim,
            Self::Add(_) => Action::AddMember,
            Self::Remove(_) => Action::RemoveMember,
            Self::Rename(_) => Action::Rename,
            Self::Transcript => Action::Transcript,
            Self::Reopen => Action::Reopen,
            Self::Move(_) => Action::Move,
        }
    }
}

/// Whether the member may do `action` to `ticket` as it stands; if not they have been told why.
async fn allowed(cx: &Cx, ticket: &Ticket, panel: Option<&TicketPanel>, action: Action) -> anyhow::Result<bool> {
    let opener = cx.author()?.id.get() == ticket.opener_id as u64;
    let refusal = may(action, is_staff(cx, panel), opener)
        .err()
        .or_else(|| transition(ticket.state(), action).err());
    match refusal {
        Some(refusal) => {
            refuse(cx, refusal).await?;
            Ok(false)
        }
        None => Ok(true),
    }
}

/// `/ticket …`, run in a ticket.
pub(crate) async fn run_action(cx: &Cx, act: TicketAct) -> anyhow::Result<()> {
    let Some((ticket, panel)) = here(cx).await? else {
        return Ok(());
    };
    if !allowed(cx, &ticket, panel.as_ref(), act.action()).await? {
        return Ok(());
    }
    match act {
        TicketAct::Close(reason) => {
            tell(cx, t!(cx.lang(), "tickets.closing", number = number(ticket.number))).await?;
            let reason = reason.map(|r| single_reason(&r)).filter(|r| !r.is_empty());
            close(&cx.app, ticket, panel, Some(actor(cx)?), reason, None).await?;
            Ok(())
        }
        TicketAct::Claim => claim(cx, &ticket).await,
        TicketAct::Unclaim => unclaim(cx, &ticket).await,
        TicketAct::Add(user) => add(cx, &ticket, user).await,
        TicketAct::Remove(user) => remove(cx, &ticket, user).await,
        TicketAct::Rename(name) => rename(cx, &ticket, &name).await,
        TicketAct::Transcript => send_transcript(cx, &ticket).await,
        TicketAct::Reopen => {
            tell(cx, t!(cx.lang(), "tickets.reopening", number = number(ticket.number))).await?;
            reopen(cx, &ticket, panel.as_ref()).await
        }
        TicketAct::Move(category) => move_to(cx, &ticket, category).await,
    }
}

/// The member acting, as their id and the name the notices show.
fn actor(cx: &Cx) -> anyhow::Result<(u64, String)> {
    Ok((cx.author()?.id.get(), cx.author_display_name()))
}

fn single_reason(reason: &str) -> String {
    crate::util::single_line(&crate::util::strip_mentions(reason))
        .chars()
        .take(REASON_MAX)
        .collect()
}

/// A ticket's controls, `tp:<action>:<ticket>`.
pub(crate) async fn on_control(cx: &Cx, mut parts: Split<'_, char>) -> anyhow::Result<()> {
    let lang = cx.lang();
    let action = parts.next().unwrap_or_default().to_owned();
    let Some(id) = parts.next().and_then(|id| id.parse::<i64>().ok()) else {
        return tell(cx, t!(lang, "tickets.expired")).await;
    };
    // Only in the ticket itself, where the bot posts these: staff rights come from that channel's permissions.
    let here = cx.interaction.channel.as_ref().map(|c| c.id.get());
    let found = cx
        .app
        .db
        .ticket(id)
        .await?
        .filter(|t| Some(t.guild_id as u64) == cx.guild_id() && t.place() == here);
    let Some(ticket) = found else {
        return tell(cx, t!(lang, "tickets.gone")).await;
    };
    let panel = match ticket.panel_id {
        Some(panel) => cx.app.db.ticket_panel(panel).await?,
        None => None,
    };
    let wanted = match action.as_str() {
        "cl" | "cy" => Action::Close,
        "cm" => {
            if ticket.claimed_by.is_some() {
                Action::Unclaim
            } else {
                Action::Claim
            }
        }
        "tr" => Action::Transcript,
        "ro" => Action::Reopen,
        "lk" => {
            if ticket.locked {
                Action::Unlock
            } else {
                Action::Lock
            }
        }
        "dl" | "dy" => Action::Delete,
        _ => return tell(cx, t!(lang, "tickets.expired")).await,
    };
    if !allowed(cx, &ticket, panel.as_ref(), wanted).await? {
        return Ok(());
    }
    match action.as_str() {
        "cl" => {
            let confirm = row(vec![button(
                format!("tp:cy:{id}"),
                &t!(lang, "tickets.button.close-confirm"),
                Some("🔒"),
                ButtonStyle::Danger,
                false,
            )]);
            cx.reply(private(
                t!(lang, "tickets.close-ask", number = number(ticket.number)),
                vec![confirm],
            ))
            .await
        }
        "cy" => {
            cx.update(InteractionResponseData {
                content: Some(t!(lang, "tickets.closing", number = number(ticket.number))),
                components: Some(Vec::new()),
                allowed_mentions: quiet(),
                ..Default::default()
            })
            .await?;
            close(&cx.app, ticket, panel, Some(actor(cx)?), None, None).await?;
            Ok(())
        }
        "cm" if wanted == Action::Unclaim => unclaim(cx, &ticket).await,
        "cm" => claim(cx, &ticket).await,
        "tr" => send_transcript(cx, &ticket).await,
        "ro" => {
            cx.responder.defer(true).await?;
            reopen(cx, &ticket, panel.as_ref()).await?;
            tell(cx, t!(lang, "tickets.reopened-you", number = number(ticket.number))).await
        }
        "lk" => lock(cx, &ticket, wanted == Action::Lock).await,
        "dl" => {
            let confirm = row(vec![button(
                format!("tp:dy:{id}"),
                &t!(lang, "tickets.button.delete-confirm"),
                Some("🗑️"),
                ButtonStyle::Danger,
                false,
            )]);
            cx.reply(private(t!(lang, "tickets.delete-ask"), vec![confirm])).await
        }
        _ => delete(cx, &ticket).await,
    }
}

/// A star rating from the closed ticket's opener, `tr:<ticket>:<stars>`.
pub(crate) async fn on_rating(cx: &Cx, mut parts: Split<'_, char>) -> anyhow::Result<()> {
    let lang = cx.lang();
    let picked = parts
        .next()
        .and_then(|t| t.parse::<i64>().ok())
        .zip(parts.next().and_then(|s| s.parse::<i16>().ok()))
        .filter(|(_, stars)| (1..=5).contains(stars));
    let Some((id, stars)) = picked else {
        return tell(cx, t!(lang, "tickets.expired")).await;
    };
    let user = cx.author()?.id.get();
    if !cx.app.db.rate_ticket(id, user, stars).await? {
        return tell(cx, t!(lang, "tickets.rated-already")).await;
    }
    cx.update(InteractionResponseData {
        components: Some(Vec::new()),
        content: Some(t!(lang, "tickets.rated", stars = "★".repeat(stars as usize))),
        allowed_mentions: quiet(),
        ..Default::default()
    })
    .await
}

/// Posts a notice for everybody in the ticket; one that cannot be posted is only noted.
async fn notice(
    app: &App,
    guild: u64,
    place: Option<Id<ChannelMarker>>,
    color: u32,
    text: String,
    components: Vec<Component>,
) {
    let Some(place) = place else { return };
    let embeds = [app
        .brand_embed(guild, EmbedBuilder::new().color(color).description(text).build())
        .await];
    let sent = app
        .http
        .create_message(place)
        .embeds(&embeds)
        .components(&components)
        .allowed_mentions(quiet().as_ref())
        .await;
    if let Err(error) = sent {
        tracing::debug!(place = place.get(), %error, "ticket notice not posted");
    }
}

fn mention(user: u64) -> String {
    format!("<@{user}>")
}

async fn claim(cx: &Cx, ticket: &Ticket) -> anyhow::Result<()> {
    let lang = cx.lang();
    let (user, _) = actor(cx)?;
    if !cx.app.db.claim_ticket(ticket.id, user).await? {
        return refuse(cx, Refusal::AlreadyClaimed).await;
    }
    let server = guild_lang(&cx.app, ticket.guild_id as u64).await;
    notice(
        &cx.app,
        ticket.guild_id as u64,
        place_of(ticket),
        SUCCESS,
        t!(server, "tickets.notice.claimed", user = mention(user)),
        Vec::new(),
    )
    .await;
    let settings = cx.app.db.ticket_settings(ticket.guild_id as u64).await?;
    let panel = match ticket.panel_id {
        Some(id) => cx.app.db.ticket_panel(id).await?,
        None => None,
    };
    log(
        &cx.app,
        ticket.guild_id as u64,
        log_channel(&settings, panel.as_ref()),
        log_embed(
            SUCCESS,
            t!(server, "tickets.log.claimed-title", number = number(ticket.number)),
            t!(
                server,
                "tickets.log.claimed",
                user = mention(user),
                place = ticket.place().map(|p| format!("<#{p}>")).unwrap_or_default()
            ),
        ),
        None,
    )
    .await;
    tell(cx, t!(lang, "tickets.claimed-you", number = number(ticket.number))).await
}

async fn unclaim(cx: &Cx, ticket: &Ticket) -> anyhow::Result<()> {
    if !cx.app.db.unclaim_ticket(ticket.id).await? {
        return refuse(cx, Refusal::NotClaimed).await;
    }
    let server = guild_lang(&cx.app, ticket.guild_id as u64).await;
    let (user, _) = actor(cx)?;
    notice(
        &cx.app,
        ticket.guild_id as u64,
        place_of(ticket),
        WARN,
        t!(server, "tickets.notice.unclaimed", user = mention(user)),
        Vec::new(),
    )
    .await;
    tell(
        cx,
        t!(cx.lang(), "tickets.unclaimed-you", number = number(ticket.number)),
    )
    .await
}

/// What a member may do in a locked ticket: read, not write.
fn read_only() -> PermissionOverwrite {
    PermissionOverwrite {
        allow: Some(Permissions::VIEW_CHANNEL | Permissions::READ_MESSAGE_HISTORY),
        deny: Some(Permissions::SEND_MESSAGES | Permissions::ATTACH_FILES | Permissions::ADD_REACTIONS),
        id: Id::new(1),
        kind: PermissionOverwriteType::Member,
    }
}

fn member_overwrite(user: u64) -> PermissionOverwrite {
    PermissionOverwrite {
        allow: Some(member_allow()),
        deny: None,
        id: Id::new(user),
        kind: PermissionOverwriteType::Member,
    }
}

async fn lock(cx: &Cx, ticket: &Ticket, locking: bool) -> anyhow::Result<()> {
    let lang = cx.lang();
    let Some(place) = place_of(ticket) else {
        return tell(cx, t!(lang, "tickets.gone")).await;
    };
    if let Some(lacking) = missing(cx, Need::Manage(place.get())) {
        return tell(cx, lacking).await;
    }
    let done = if ticket.thread_id.is_some() {
        cx.app
            .http
            .update_thread(place)
            .locked(locking)
            .await
            .map(|_| ())
            .map_err(anyhow::Error::from)
    } else {
        lock_channel_members(cx, ticket, place, locking).await
    };
    if let Err(error) = done {
        let message = error.downcast_ref::<twilight_http::Error>().map_or_else(
            || t!(lang, "tickets.discord.refused"),
            |error| discord_refusal(error, lang, Need::Manage(place.get())),
        );
        return tell(cx, message).await;
    }
    cx.app.db.set_ticket_locked(ticket.id, locking).await?;
    let server = guild_lang(&cx.app, ticket.guild_id as u64).await;
    let (user, _) = actor(cx)?;
    let key = if locking {
        "tickets.notice.locked"
    } else {
        "tickets.notice.unlocked"
    };
    notice(
        &cx.app,
        ticket.guild_id as u64,
        Some(place),
        WARN,
        t!(server, key, user = mention(user)),
        Vec::new(),
    )
    .await;
    let key = if locking {
        "tickets.locked-you"
    } else {
        "tickets.unlocked-you"
    };
    tell(cx, t!(lang, key)).await
}

/// A role's staff allowance beats the @everyone write deny; explicit member allowances do not.
async fn lock_channel_members(cx: &Cx, ticket: &Ticket, place: Id<ChannelMarker>, locking: bool) -> anyhow::Result<()> {
    let channel = cx.app.http.channel(place).await?.model().await?;
    let everyone = PermissionOverwrite {
        allow: None,
        deny: Some(if locking {
            Permissions::VIEW_CHANNEL
                | Permissions::SEND_MESSAGES
                | Permissions::ATTACH_FILES
                | Permissions::ADD_REACTIONS
        } else {
            Permissions::VIEW_CHANNEL
        }),
        id: Id::new(ticket.guild_id as u64),
        kind: PermissionOverwriteType::Role,
    };
    cx.app.http.update_channel_permission(place, &everyone).await?;
    let bot = cx.app.application_id.get();
    let mut members = vec![ticket.opener_id as u64];
    let support = match ticket.panel_id {
        Some(id) => cx
            .app
            .db
            .ticket_panel(id)
            .await?
            .map(|p| p.support_roles)
            .unwrap_or_default(),
        None => Vec::new(),
    };
    let guild = Id::new(ticket.guild_id as u64);
    let staff_roles: Vec<u64> = match cx.app.http.roles(guild).await {
        Ok(response) => response
            .models()
            .await?
            .into_iter()
            .filter(|role| {
                support.contains(&(role.id.get() as i64))
                    || super::STAFF_PERMISSIONS
                        .iter()
                        .any(|needed| role.permissions.contains(*needed))
            })
            .map(|role| role.id.get())
            .collect(),
        Err(_) => support.into_iter().map(|id| id as u64).collect(),
    };
    if locking {
        for role in &staff_roles {
            let overwrite = PermissionOverwrite {
                allow: Some(member_allow()),
                deny: None,
                id: Id::new(*role),
                kind: PermissionOverwriteType::Role,
            };
            cx.app.http.update_channel_permission(place, &overwrite).await?;
        }
    }
    for overwrite in channel.permission_overwrites.unwrap_or_default() {
        let id = overwrite.id.get();
        if overwrite.kind == twilight_model::channel::permission_overwrite::PermissionOverwriteType::Member
            && id != bot
            && !members.contains(&id)
        {
            members.push(id);
        }
    }
    for member in members {
        if locking
            && let Ok(response) = cx.app.http.guild_member(guild, Id::new(member)).await
            && let Ok(found) = response.model().await
            && found.roles.iter().any(|role| staff_roles.contains(&role.get()))
        {
            continue;
        }
        let overwrite = if locking {
            PermissionOverwrite {
                id: Id::new(member),
                ..read_only()
            }
        } else {
            member_overwrite(member)
        };
        cx.app.http.update_channel_permission(place, &overwrite).await?;
    }
    Ok(())
}

async fn add(cx: &Cx, ticket: &Ticket, user: u64) -> anyhow::Result<()> {
    let lang = cx.lang();
    let Some(place) = place_of(ticket) else {
        return tell(cx, t!(lang, "tickets.gone")).await;
    };
    let done = if ticket.thread_id.is_some() {
        cx.app.http.add_thread_member(place, Id::new(user)).await.map(|_| ())
    } else {
        cx.app
            .http
            .update_channel_permission(place, &member_overwrite(user))
            .await
            .map(|_| ())
    };
    if let Err(error) = done {
        return tell(cx, discord_refusal(&error, lang, Need::Manage(place.get()))).await;
    }
    let server = guild_lang(&cx.app, ticket.guild_id as u64).await;
    let (by, _) = actor(cx)?;
    notice(
        &cx.app,
        ticket.guild_id as u64,
        Some(place),
        SUCCESS,
        t!(server, "tickets.notice.added", user = mention(user), by = mention(by)),
        Vec::new(),
    )
    .await;
    tell(cx, t!(lang, "tickets.added-you", user = mention(user))).await
}

async fn remove(cx: &Cx, ticket: &Ticket, user: u64) -> anyhow::Result<()> {
    let lang = cx.lang();
    if user == ticket.opener_id as u64 {
        return tell(cx, t!(lang, "tickets.remove-opener")).await;
    }
    if user == cx.app.application_id.get() {
        return tell(cx, t!(lang, "tickets.remove-bot")).await;
    }
    let Some(place) = place_of(ticket) else {
        return tell(cx, t!(lang, "tickets.gone")).await;
    };
    let done = if ticket.thread_id.is_some() {
        cx.app.http.remove_thread_member(place, Id::new(user)).await.map(|_| ())
    } else {
        cx.app
            .http
            .delete_channel_permission(place)
            .member(Id::new(user))
            .await
            .map(|_| ())
    };
    if let Err(error) = done {
        return tell(cx, discord_refusal(&error, lang, Need::Manage(place.get()))).await;
    }
    let server = guild_lang(&cx.app, ticket.guild_id as u64).await;
    let (by, _) = actor(cx)?;
    notice(
        &cx.app,
        ticket.guild_id as u64,
        Some(place),
        WARN,
        t!(server, "tickets.notice.removed", user = mention(user), by = mention(by)),
        Vec::new(),
    )
    .await;
    tell(cx, t!(lang, "tickets.removed-you", user = mention(user))).await
}

async fn rename(cx: &Cx, ticket: &Ticket, wanted: &str) -> anyhow::Result<()> {
    let lang = cx.lang();
    let Some(place) = place_of(ticket) else {
        return tell(cx, t!(lang, "tickets.gone")).await;
    };
    let opener = cx
        .app
        .user(ticket.opener_id as u64)
        .await
        .map(|u| u.name.clone())
        .unwrap_or_default();
    let name = channel_name(wanted, ticket.number, &opener, &ticket.topic);
    if name.is_empty() {
        return tell(cx, t!(lang, "tickets.rename-empty")).await;
    }
    let done = if ticket.thread_id.is_some() {
        cx.app.http.update_thread(place).name(&name).await.map(|_| ())
    } else {
        cx.app.http.update_channel(place).name(&name).await.map(|_| ())
    };
    if let Err(error) = done {
        return tell(cx, discord_refusal(&error, lang, Need::Manage(place.get()))).await;
    }
    tell(cx, t!(lang, "tickets.renamed", name = name)).await
}

async fn move_to(cx: &Cx, ticket: &Ticket, category: u64) -> anyhow::Result<()> {
    let lang = cx.lang();
    if ticket.thread_id.is_some() {
        return tell(cx, t!(lang, "tickets.move-thread")).await;
    }
    let Some(place) = place_of(ticket) else {
        return tell(cx, t!(lang, "tickets.gone")).await;
    };
    let done = cx
        .app
        .http
        .update_channel(place)
        .parent_id(Some(Id::new(category)))
        .await;
    if let Err(error) = done {
        return tell(cx, discord_refusal(&error, lang, Need::Manage(place.get()))).await;
    }
    tell(cx, t!(lang, "tickets.moved", category = format!("<#{category}>"))).await
}

/// The transcript as a file only the member sees: the kept one of a closed ticket, else read now.
async fn send_transcript(cx: &Cx, ticket: &Ticket) -> anyhow::Result<()> {
    let lang = cx.lang();
    cx.responder.defer(true).await?;
    let server = server_name(&cx.app, ticket.guild_id as u64).await;
    let html = match cx.app.db.transcript(ticket.id).await? {
        Some(stored) if !ticket.is_open() => {
            let names = names(&cx.app, ticket, None).await;
            let page = Page {
                server: &server,
                opener: names.0.as_deref(),
                closed_by: names.1.as_deref(),
            };
            transcript::stored_html(ticket, &page, &stored, lang)
        }
        _ => {
            let captured = match transcript::capture(&cx.app, ticket).await {
                Ok(captured) => captured,
                Err(error) => {
                    tracing::debug!(ticket = ticket.id, %error, "ticket history not read");
                    return tell(cx, t!(lang, "tickets.discord.history")).await;
                }
            };
            let names = names(&cx.app, ticket, Some(&captured)).await;
            let page = Page {
                server: &server,
                opener: names.0.as_deref(),
                closed_by: names.1.as_deref(),
            };
            transcript::page_html(
                ticket,
                &page,
                &captured.lines,
                &captured.authors(),
                &captured.mention_names(),
                captured.with_content,
                lang,
            )
        }
    };
    cx.reply(InteractionResponseData {
        content: Some(t!(lang, "tickets.transcript-here", number = number(ticket.number))),
        attachments: Some(vec![Attachment::from_bytes(transcript::file_name(ticket), html, 1)]),
        flags: Some(twilight_model::channel::message::MessageFlags::EPHEMERAL),
        allowed_mentions: quiet(),
        ..Default::default()
    })
    .await
}

/// The opener's and the closer's names for a transcript: from the messages when they wrote, else from Discord.
async fn names(app: &App, ticket: &Ticket, captured: Option<&Captured>) -> (Option<String>, Option<String>) {
    let lookup = |id: Option<i64>| async move {
        let id = id? as u64;
        if let Some((_, name, _)) = captured.and_then(|c| c.authors.iter().find(|(user, _, _)| *user == id)) {
            return Some(name.clone());
        }
        app.user(id)
            .await
            .map(|u| u.global_name.clone().unwrap_or_else(|| u.name.clone()))
    };
    (lookup(Some(ticket.opener_id)).await, lookup(ticket.closed_by).await)
}

/// Closes `ticket` once, whoever asked (a member, staff, the ticker): keeps the transcript, tells the
/// ticket, takes the opener's access away (or archives the thread), moves the channel to the closed
/// category, logs it with the transcript and sends the opener theirs with the rating buttons. `None` when
/// somebody else closed it first.
pub(crate) async fn close(
    app: &Arc<App>,
    ticket: Ticket,
    panel: Option<TicketPanel>,
    by: Option<(u64, String)>,
    reason: Option<String>,
    idle_before: Option<chrono::DateTime<chrono::Utc>>,
) -> anyhow::Result<Option<Ticket>> {
    let guild = ticket.guild_id as u64;
    let Some(closed) = app
        .db
        .close_ticket(
            ticket.id,
            by.as_ref().map(|(id, _)| *id),
            reason.as_deref(),
            app.now(),
            idle_before,
        )
        .await?
    else {
        return Ok(None);
    };
    let lang = guild_lang(app, guild).await;
    let settings = app.db.ticket_settings(guild).await?;
    let captured = match transcript::capture(app, &closed).await {
        Ok(captured) => captured,
        Err(error) => {
            tracing::warn!(ticket = closed.id, %error, "ticket history not read, the transcript is empty");
            Captured {
                lines: Vec::new(),
                authors: Vec::new(),
                with_content: app.message_content,
                first_reply: None,
            }
        }
    };
    if let Err(error) = app
        .db
        .save_transcript(
            closed.id,
            &captured.lines,
            &captured.authors,
            captured.with_content,
            captured.first_reply,
        )
        .await
    {
        tracing::warn!(ticket = closed.id, %error, "ticket transcript not saved");
    }
    let who = by
        .as_ref()
        .map_or_else(|| t!(lang, "tickets.notice.nobody"), |(id, _)| mention(*id));
    let mut text = t!(lang, "tickets.notice.closed", user = who);
    if let Some(reason) = &reason {
        text.push('\n');
        text.push_str(&t!(lang, "tickets.notice.reason", reason = scrub_value(reason)));
    }
    notice(
        app,
        closed.guild_id as u64,
        place_of(&closed),
        DANGER,
        text.clone(),
        closed_controls(closed.id, lang),
    )
    .await;
    shut(app, &closed, panel.as_ref(), &settings).await;

    let server = server_name(app, guild).await;
    let names = names(app, &closed, Some(&captured)).await;
    let page = Page {
        server: &server,
        opener: names.0.as_deref(),
        closed_by: names.1.as_deref(),
    };
    let html = transcript::page_html(
        &closed,
        &page,
        &captured.lines,
        &captured.authors(),
        &captured.mention_names(),
        captured.with_content,
        lang,
    );
    let file = transcript::file_name(&closed);
    let mut summary = t!(
        lang,
        "tickets.log.closed",
        opener = mention(closed.opener_id as u64),
        user = who,
        count = captured.lines.len()
    );
    if let Some(reason) = &reason {
        summary.push('\n');
        summary.push_str(&t!(lang, "tickets.notice.reason", reason = scrub_value(reason)));
    }
    log(
        app,
        closed.guild_id as u64,
        log_channel(&settings, panel.as_ref()),
        log_embed(
            DANGER,
            t!(lang, "tickets.log.closed-title", number = number(closed.number)),
            summary,
        ),
        Some((file.clone(), html.clone())),
    )
    .await;
    tell_opener(app, &closed, &settings, &server, (file, html)).await;
    Ok(Some(closed))
}

/// Takes the closed ticket away from its members: their overwrites go and the channel moves to the closed
/// category; a thread loses its opener and is archived and locked.
async fn shut(app: &App, ticket: &Ticket, _panel: Option<&TicketPanel>, settings: &TicketSettings) {
    let Some(place) = place_of(ticket) else { return };
    let bot = app.application_id.get();
    if ticket.thread_id.is_some() {
        let mut members = vec![ticket.opener_id as u64];
        let mut after: Option<Id<UserMarker>> = None;
        loop {
            let request = app.http.thread_members(place).limit(100);
            let response = match after {
                Some(after) => request.after(after).await,
                None => request.await,
            };
            let list = match response {
                Ok(response) => match response.models().await {
                    Ok(list) => list,
                    Err(error) => {
                        tracing::debug!(ticket = ticket.id, %error, "thread members not read");
                        break;
                    }
                },
                Err(error) => {
                    tracing::debug!(ticket = ticket.id, %error, "thread members not read");
                    break;
                }
            };
            let full = list.len() == 100;
            let last = list.last().and_then(|member| member.user_id);
            for member in list {
                if let Some(id) = member.user_id.map(Id::get)
                    && id != bot
                    && !members.contains(&id)
                {
                    members.push(id);
                }
            }
            if !full || last.is_none() || last == after {
                break;
            }
            after = last;
        }
        for member in members {
            if let Err(error) = app
                .http
                .remove_thread_member(place, Id::<UserMarker>::new(member))
                .await
            {
                tracing::debug!(ticket = ticket.id, member, %error, "ticket member not taken out of the thread");
            }
        }
        if let Err(error) = app.http.update_thread(place).archived(true).locked(true).await {
            tracing::debug!(ticket = ticket.id, %error, "ticket thread not archived");
        }
        return;
    }
    let mut members = vec![ticket.opener_id as u64];
    if let Ok(response) = app.http.channel(place).await
        && let Ok(channel) = response.model().await
    {
        for overwrite in channel.permission_overwrites.unwrap_or_default() {
            let id = overwrite.id.get();
            if overwrite.kind == twilight_model::channel::permission_overwrite::PermissionOverwriteType::Member
                && id != bot
                && !members.contains(&id)
            {
                members.push(id);
            }
        }
    }
    for member in members {
        if let Err(error) = app.http.delete_channel_permission(place).member(Id::new(member)).await {
            tracing::debug!(ticket = ticket.id, member, %error, "ticket access not taken back");
        }
    }
    if let Some(category) = settings.closed_category_id
        && let Err(error) = app
            .http
            .update_channel(place)
            .parent_id(Some(Id::new(category as u64)))
            .await
    {
        tracing::debug!(ticket = ticket.id, %error, "closed ticket not moved");
    }
}

/// The opener's direct message after a close: the transcript when the server sends it, the stars when it
/// asks for a rating. Members who switched off direct messages from servers get nothing.
async fn tell_opener(app: &App, ticket: &Ticket, settings: &TicketSettings, server: &str, file: (String, Vec<u8>)) {
    if !settings.dm_transcript && !settings.rating_enabled {
        return;
    }
    let opener = ticket.opener_id as u64;
    let Some(channel) = crate::leveling::private_channel(app, opener).await else {
        return;
    };
    let lang = match app.user_lang_choice(opener).await {
        Some(lang) => lang,
        None => app.guild_lang(ticket.guild_id as u64).await,
    };
    let mut text = t!(
        lang,
        "tickets.dm.closed",
        number = number(ticket.number),
        server = scrub_value(server)
    );
    if settings.rating_enabled {
        text.push('\n');
        text.push_str(&t!(lang, "tickets.dm.rate"));
    }
    let components: Vec<Component> = if settings.rating_enabled {
        vec![rating_row(ticket.id)]
    } else {
        Vec::new()
    };
    let sent = app
        .http
        .create_message(channel)
        .content(&text)
        .components(&components)
        .allowed_mentions(quiet().as_ref())
        .await;
    if let Err(error) = sent {
        tracing::debug!(ticket = ticket.id, %error, "ticket direct message not delivered");
    }
    if settings.dm_transcript {
        let files = [Attachment::from_bytes(file.0, file.1, 1)];
        if let Err(error) = app.http.create_message(channel).attachments(&files).await {
            tracing::debug!(ticket = ticket.id, %error, "ticket transcript direct message not delivered");
        }
    }
}

/// Opens a closed ticket again: the opener sees it again (the thread is unarchived), it moves back to the
/// open category, and the ticket says so with its controls.
async fn reopen(cx: &Cx, ticket: &Ticket, panel: Option<&TicketPanel>) -> anyhow::Result<()> {
    let app = &cx.app;
    let Some(opened) = app.db.reopen_ticket(ticket.id, app.now()).await? else {
        return refuse(cx, Refusal::NotClosed).await;
    };
    let guild = ticket.guild_id as u64;
    let settings = app.db.ticket_settings(guild).await?;
    if let Some(place) = place_of(&opened) {
        let opener = Id::<UserMarker>::new(opened.opener_id as u64);
        if opened.thread_id.is_some() {
            if let Err(error) = app.http.update_thread(place).archived(false).locked(false).await {
                tracing::debug!(ticket = opened.id, %error, "ticket thread not unarchived");
            }
            if let Err(error) = app.http.add_thread_member(place, opener).await {
                tracing::debug!(ticket = opened.id, %error, "opener not added back");
            }
        } else {
            if let Err(error) = app
                .http
                .update_channel_permission(place, &member_overwrite(opener.get()))
                .await
            {
                tracing::debug!(ticket = opened.id, %error, "opener's access not given back");
            }
            let open_category = panel.and_then(|p| p.open_category_id).or(settings.open_category_id);
            if settings.closed_category_id.is_some()
                && let Some(category) = open_category
                && let Err(error) = app
                    .http
                    .update_channel(place)
                    .parent_id(Some(Id::new(category as u64)))
                    .await
            {
                tracing::debug!(ticket = opened.id, %error, "reopened ticket not moved back");
            }
        }
    }
    let lang = guild_lang(app, guild).await;
    let (user, _) = actor(cx)?;
    notice(
        app,
        guild,
        place_of(&opened),
        SUCCESS,
        t!(
            lang,
            "tickets.notice.reopened",
            user = mention(user),
            opener = mention(opened.opener_id as u64)
        ),
        controls(opened.id, false, lang),
    )
    .await;
    log(
        app,
        guild,
        log_channel(&settings, panel),
        log_embed(
            SUCCESS,
            t!(lang, "tickets.log.reopened-title", number = number(opened.number)),
            t!(lang, "tickets.log.reopened", user = mention(user)),
        ),
        None,
    )
    .await;
    Ok(())
}

/// Deletes a closed ticket's channel or thread for good; the ticket and its transcript stay.
async fn delete(cx: &Cx, ticket: &Ticket) -> anyhow::Result<()> {
    let lang = cx.lang();
    let Some(place) = place_of(ticket) else {
        return tell(cx, t!(lang, "tickets.gone")).await;
    };
    cx.update(InteractionResponseData {
        content: Some(t!(lang, "tickets.deleting")),
        components: Some(Vec::new()),
        allowed_mentions: quiet(),
        ..Default::default()
    })
    .await?;
    if let Err(error) = cx.app.http.delete_channel(place).await
        && crate::roles::error_code(&error) != Some(UNKNOWN_CHANNEL)
    {
        tell(cx, discord_refusal(&error, lang, Need::Manage(place.get()))).await?;
    }
    Ok(())
}

/// One step of the ticker: closes the open tickets nobody wrote in for longer than their server allows.
/// Discord is asked for each one's last message first, since the bot does not count messages itself:
/// a newer one makes it busy again. A ticket whose channel is gone closes too. Returns how many closed.
pub async fn tick(app: &Arc<App>) -> anyhow::Result<usize> {
    let now = app.now();
    let mut closed = 0;
    for idle in app.db.idle_tickets(now, 25).await? {
        let (guild, place) = (idle.guild_id as u64, idle.place as u64);
        // A ticket skipped here goes to the back of the queue, so 25 that can never close (a switched-off
        // module, a server the bot left) cannot hold up every other server's.
        if !module_on(app, guild, Some(place)).await {
            app.db.touch_ticket(idle.id, now).await?;
            continue;
        }
        let cutoff = idle_cutoff(now, idle.auto_close_hours);
        match app.http.channel(Id::new(place)).await {
            Ok(response) => {
                let channel = response.model().await?;
                if let Some(last) = channel.last_message_id {
                    let at = snowflake::timestamp(last.get());
                    if at > idle.last_activity_at {
                        app.db.touch_ticket(idle.id, at).await?;
                        if at >= cutoff {
                            continue;
                        }
                    }
                }
            }
            Err(error) if crate::roles::error_code(&error) == Some(UNKNOWN_CHANNEL) => {}
            Err(error) => {
                tracing::debug!(ticket = idle.id, %error, "idle ticket's channel not read");
                app.db.touch_ticket(idle.id, now).await?;
                continue;
            }
        }
        let Some(ticket) = app.db.ticket(idle.id).await? else {
            continue;
        };
        let panel = match ticket.panel_id {
            Some(id) => app.db.ticket_panel(id).await?,
            None => None,
        };
        let lang = guild_lang(app, guild).await;
        let reason = t!(lang, "tickets.auto-reason", count = idle.auto_close_hours);
        match close(app, ticket, panel, None, Some(reason), Some(cutoff)).await {
            Ok(Some(_)) => closed += 1,
            Ok(None) => {}
            Err(error) => tracing::warn!(ticket = idle.id, %error, "idle ticket not closed"),
        }
    }
    Ok(closed)
}

/// Runs [`tick`] for the life of the process.
pub fn spawn_ticker(app: Arc<App>) {
    tokio::spawn(async move {
        let mut wake = tokio::time::interval(WAKE);
        wake.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            wake.tick().await;
            if let Err(error) = Box::pin(tick(&app)).await {
                tracing::warn!(%error, "tickets tick failed");
            }
        }
    });
}
