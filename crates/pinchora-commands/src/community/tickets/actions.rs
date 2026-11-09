//! Everything done to an open or closed ticket, from its buttons (`tp:`), from `/ticket` and from the
//! ticker that closes the ones nobody wrote in: claim, lock, add and remove people, rename, move, close,
//! reopen, delete, the transcript, and the rating its opener gives afterwards (`tr:`).

mod lifecycle;
mod operations;
pub(crate) use lifecycle::close;
pub use lifecycle::{spawn_ticker, tick};
mod access;
use access::{add, lock, member_overwrite, move_to, remove, rename};

use lifecycle::{delete, reopen, send_transcript};

use crate::community::tickets::actions::operations::note;
use crate::community::tickets::actions::operations::priority;
use crate::community::tickets::actions::operations::request_close;
use crate::community::tickets::actions::operations::snippet;
use crate::community::tickets::actions::operations::transfer;

use std::{str::Split, sync::Arc};

use pinchora_core::community::tickets::Action;
use pinchora_core::community::tickets::REASON_MAX;
use pinchora_core::community::tickets::Refusal;
use pinchora_core::community::tickets::channel_name;
use pinchora_core::community::tickets::may;
use pinchora_core::community::tickets::transition;
use pinchora_core::infra::i18n::Lang;
use pinchora_core::infra::snowflake;
use pinchora_core::t;
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

use crate::App;
use crate::Cx;
use crate::admin::custom::scrub_value;
use crate::commands::games::games::quiet;
use crate::community::server_events::button;
use crate::community::server_events::row;
use crate::community::tickets::Need;
use crate::community::tickets::WAKE;
use crate::community::tickets::discord_refusal;
use crate::community::tickets::guild_lang;
use crate::community::tickets::here;
use crate::community::tickets::is_staff;
use crate::community::tickets::log;
use crate::community::tickets::log_channel;
use crate::community::tickets::member_allow;
use crate::community::tickets::missing;
use crate::community::tickets::module_on;
use crate::community::tickets::number;
use crate::community::tickets::open::log_embed;
use crate::community::tickets::place_of;
use crate::community::tickets::private;
use crate::community::tickets::refuse;
use crate::community::tickets::server_name;
use crate::community::tickets::tell;
use crate::community::tickets::transcript;
use crate::community::tickets::transcript::Captured;
use crate::community::tickets::transcript::Page;
use crate::infra::util::DANGER;
use crate::infra::util::SUCCESS;
use crate::infra::util::WARN;

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
    Priority(String),
    Escalate,
    Transfer(Option<u64>, Option<u64>),
    RequestClose,
    ForceOpen,
    Note(String),
    Snippet(String),
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
            Self::Priority(_) | Self::Escalate | Self::Transfer(_, _) | Self::Snippet(_) => Action::Rename,
            Self::RequestClose => Action::Close,
            Self::ForceOpen => Action::Reopen,
            Self::Note(_) => Action::Transcript,
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
    if matches!(
        act,
        TicketAct::Priority(_)
            | TicketAct::Escalate
            | TicketAct::Transfer(_, _)
            | TicketAct::RequestClose
            | TicketAct::ForceOpen
            | TicketAct::Note(_)
            | TicketAct::Snippet(_)
    ) && !is_staff(cx, panel.as_ref())
    {
        return refuse(cx, Refusal::StaffOnly).await;
    }
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
            reopen(cx, &ticket, panel.as_ref(), false).await.map(|_| ())
        }
        TicketAct::Move(category) => move_to(cx, &ticket, category).await,
        TicketAct::Priority(level) => priority(cx, &ticket, &level, false).await,
        TicketAct::Escalate => priority(cx, &ticket, "urgent", true).await,
        TicketAct::Transfer(user, role) => transfer(cx, &ticket, panel.as_ref(), user, role).await,
        TicketAct::RequestClose => request_close(cx, &ticket).await,
        TicketAct::ForceOpen => {
            if !reopen(cx, &ticket, panel.as_ref(), true).await? {
                return Ok(());
            }
            tell(cx, t!(cx.lang(), "tickets.v2.force-open")).await
        }
        TicketAct::Note(body) => note(cx, &ticket, &body).await,
        TicketAct::Snippet(name) => snippet(cx, &ticket, &name).await,
    }
}

/// The member acting, as their id and the name the notices show.
fn actor(cx: &Cx) -> anyhow::Result<(u64, String)> {
    Ok((cx.author()?.id.get(), cx.author_display_name()))
}

fn single_reason(reason: &str) -> String {
    crate::infra::util::single_line(&crate::infra::util::strip_mentions(reason))
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
        "cl" | "cy" | "rc" => Action::Close,
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
        "rc" => {
            if cx.author()?.id.get() != ticket.opener_id as u64
                || cx
                    .app
                    .db
                    .ticket_operations(ticket.id)
                    .await?
                    .close_request_expires_at
                    .is_none()
            {
                return tell(cx, t!(lang, "tickets.expired")).await;
            }
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
            reopen(cx, &ticket, panel.as_ref(), false).await?;
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
    let board = cx.app.clone();
    let guild = ticket.guild_id as u64;
    tokio::spawn(async move {
        let _ = crate::community::tickets::refresh_status_board(&board, guild).await;
    });
    tell(cx, t!(lang, "tickets.claimed-you", number = number(ticket.number))).await
}

async fn unclaim(cx: &Cx, ticket: &Ticket) -> anyhow::Result<()> {
    if !cx.app.db.unclaim_ticket_by(ticket.id, cx.author()?.id.get()).await? {
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
