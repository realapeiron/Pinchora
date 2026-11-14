//! Opening a ticket: a panel's button or menu (`tk:`), its form (`tf:`) when it has one, then the
//! ticket's channel or private thread, the welcome message inside and the log line.

mod context;
pub(crate) use context::on_message_context;
use context::*;
mod channels;
pub(crate) use channels::overwrites;
use channels::*;
mod welcome;
use welcome::*;
pub(crate) use welcome::{log_embed, welcome_embed};
#[cfg(test)]
mod support_hours_tests;

mod forms;
use crate::community::tickets::open::forms::ask;
pub(crate) use crate::community::tickets::open::forms::on_form;

use std::{str::Split, time::Duration};

use chrono::{Datelike, Timelike, Utc};
use chrono_tz::Tz;

use pinchora_core::community::tickets::ANSWER_MAX;
use pinchora_core::community::tickets::Answer;
use pinchora_core::community::tickets::DEFAULT_NAMING;
use pinchora_core::community::tickets::Question;
use pinchora_core::community::tickets::Topic;
use pinchora_core::community::tickets::channel_name;
use pinchora_core::community::tickets::fill_welcome;
use pinchora_core::t;
use pinchora_db::{NewTicket, Ticket, TicketOpen, TicketPanel, TicketPanelOperations, TicketSettings};
use twilight_model::{
    channel::permission_overwrite::{PermissionOverwrite, PermissionOverwriteType},
    channel::{
        ChannelType,
        message::{
            Component, Embed,
            component::{
                ButtonStyle, FileUpload, Label, SelectMenu, SelectMenuOption, SelectMenuType, TextInput, TextInputStyle,
            },
        },
        thread::AutoArchiveDuration,
    },
    guild::Permissions,
    id::{Id, marker::ChannelMarker},
};
use twilight_util::builder::embed::{EmbedBuilder, EmbedFieldBuilder, EmbedFooterBuilder};

use crate::Cx;
use crate::admin::custom::scrub_value;
use crate::community::server_events::button;
use crate::community::server_events::row;
use crate::community::tickets::Need;
use crate::community::tickets::actions::controls;
use crate::community::tickets::discord_refusal;
use crate::community::tickets::guild_lang;
use crate::community::tickets::log;
use crate::community::tickets::log_channel;
use crate::community::tickets::member_allow;
use crate::community::tickets::missing;
use crate::community::tickets::number;
use crate::community::tickets::pinging;
use crate::community::tickets::private;
use crate::community::tickets::server_name;
use crate::community::tickets::tell;
use crate::infra::util::BRAND;
use crate::infra::util::single_line;
use crate::infra::util::strip_mentions;

/// `tk:o:<panel>:<topic>` (a button) or `tk:s:<panel>` with the picked topic (a menu).
pub(crate) async fn on_open(cx: &Cx, mut parts: Split<'_, char>, values: &[String]) -> anyhow::Result<()> {
    let action = parts.next();
    if action == Some("m") {
        let source = parse_source(&mut parts);
        let panel = values.first().and_then(|value| value.parse::<i64>().ok());
        return match panel {
            Some(panel) => choose_context_topic(cx, panel, source).await,
            None => tell(cx, t!(cx.lang(), "tickets.expired")).await,
        };
    }
    let (picked, source, faq_page) = match action {
        Some("o" | "c") => (parse_panel_topic(&mut parts), None, 0),
        Some("s") => (
            parts
                .next()
                .and_then(|p| p.parse::<i64>().ok())
                .zip(values.first().and_then(|v| v.parse::<usize>().ok())),
            None,
            0,
        ),
        Some("q") => {
            let panel = parts.next().and_then(|p| p.parse::<i64>().ok());
            let source = parse_source(&mut parts);
            (
                panel.zip(values.first().and_then(|value| value.parse::<usize>().ok())),
                source,
                0,
            )
        }
        Some("v") => {
            let picked = parse_panel_topic(&mut parts);
            let page = parts.next().and_then(|part| part.parse::<usize>().ok()).unwrap_or(0);
            (picked, parse_source(&mut parts), page)
        }
        Some("n" | "f") => {
            let picked = parse_panel_topic(&mut parts);
            (picked, parse_source(&mut parts), 0)
        }
        _ => (None, None, 0),
    };
    let Some((panel, index)) = picked else {
        return tell(cx, t!(cx.lang(), "tickets.expired")).await;
    };
    let Some((panel, topic)) = usable(cx, panel, index).await? else {
        return Ok(());
    };
    let ops = cx.app.db.ticket_panel_operations(panel.id).await?;
    if !matches!(action, Some("c" | "f" | "n")) && !ops.faq.is_empty() {
        return show_faq(cx, &panel, index, &ops, faq_page, source).await;
    }
    if let Some(blocked) = eligibility(cx, &ops).await? {
        return tell(cx, blocked).await;
    }
    if !panel.questions.is_empty() {
        let guild = cx
            .guild_id()
            .ok_or_else(|| anyhow::anyhow!("ticket form outside guild"))?;
        let saved = cx
            .app
            .db
            .ticket_form_draft(guild, cx.author()?.id.get(), panel.id, index as i32)
            .await?;
        if action == Some("f") && saved.is_none() {
            return tell(cx, t!(cx.lang(), "tickets.expired")).await;
        }
        let step = match saved {
            Some((_, step)) if (step as usize) * 5 < panel.questions.len() => step as usize,
            _ => {
                cx.app
                    .db
                    .save_ticket_form_draft(
                        guild,
                        cx.author()?.id.get(),
                        panel.id,
                        index as i32,
                        &serde_json::json!([]),
                        0,
                    )
                    .await?;
                0
            }
        };
        return ask(cx, &panel, index, &panel.questions, step, source).await;
    }
    open(cx, &panel, topic, with_source(cx, Vec::new(), source)).await
}

async fn show_faq(
    cx: &Cx,
    panel: &TicketPanel,
    index: usize,
    ops: &TicketPanelOperations,
    faq_page: usize,
    source: Option<(u64, u64)>,
) -> anyhow::Result<()> {
    let faq_page = faq_page.min(ops.faq.len().saturating_sub(1));
    let Some(faq) = ops.faq.get(faq_page) else {
        return tell(cx, t!(cx.lang(), "tickets.expired")).await;
    };
    let mut text = t!(cx.lang(), "tickets.v2.faq-intro");
    text.push_str(&format!(
        "\n\n{}\n**{}**\n{}",
        t!(
            cx.lang(),
            "tickets.v2.faq-position",
            current = faq_page + 1,
            total = ops.faq.len()
        ),
        scrub_value(&faq.question),
        scrub_value(&faq.answer)
    ));
    let page_id = |page: usize| {
        source.map_or_else(
            || format!("tk:v:{}:{index}:{page}", panel.id),
            |(channel, message)| format!("tk:v:{}:{index}:{page}:{channel}:{message}", panel.id),
        )
    };
    let mut buttons = Vec::new();
    if faq_page > 0 {
        buttons.push(button(
            page_id(faq_page - 1),
            &t!(cx.lang(), "tickets.v2.faq-previous"),
            None,
            ButtonStyle::Secondary,
            false,
        ));
    }
    if faq_page + 1 < ops.faq.len() {
        buttons.push(button(
            page_id(faq_page + 1),
            &t!(cx.lang(), "tickets.v2.faq-next"),
            None,
            ButtonStyle::Secondary,
            false,
        ));
    }
    buttons.push(button(
        source.map_or_else(
            || format!("tk:c:{}:{index}", panel.id),
            |(channel, message)| format!("tk:n:{}:{index}:{channel}:{message}", panel.id),
        ),
        &t!(cx.lang(), "tickets.v2.continue"),
        None,
        ButtonStyle::Primary,
        false,
    ));
    let text = text.chars().take(2000).collect::<String>();
    cx.reply(private(text, vec![row(buttons)])).await
}

fn parse_panel_topic<'a>(parts: &mut impl Iterator<Item = &'a str>) -> Option<(i64, usize)> {
    parts
        .next()
        .and_then(|p| p.parse::<i64>().ok())
        .zip(parts.next().and_then(|t| t.parse::<usize>().ok()))
}

fn parse_source<'a>(parts: &mut impl Iterator<Item = &'a str>) -> Option<(u64, u64)> {
    parts
        .next()
        .and_then(|id| id.parse::<u64>().ok())
        .zip(parts.next().and_then(|id| id.parse::<u64>().ok()))
}

fn with_source(cx: &Cx, mut answers: Vec<Answer>, source: Option<(u64, u64)>) -> Vec<Answer> {
    if let (Some(guild), Some((channel, message))) = (cx.guild_id(), source) {
        answers.push(Answer {
            question: t!(cx.lang(), "tickets.v2.source-message"),
            answer: format!("https://discord.com/channels/{guild}/{channel}/{message}"),
        });
    }
    answers
}

async fn eligibility(cx: &Cx, ops: &TicketPanelOperations) -> anyhow::Result<Option<String>> {
    let Some(member) = cx.interaction.member.as_ref() else {
        return Ok(None);
    };
    Ok(eligibility_refusal(
        ops,
        cx.author()?.id.get(),
        member.roles.iter().map(|role| role.get()),
        member.joined_at.map(|joined| joined.as_secs()),
        cx.app.now(),
        cx.lang(),
    ))
}

/// Shared admission policy for member interactions and workflow-created tickets.
pub(crate) fn eligibility_refusal(
    ops: &TicketPanelOperations,
    user: u64,
    mut roles: impl Iterator<Item = u64>,
    joined_at: Option<i64>,
    now: chrono::DateTime<Utc>,
    lang: pinchora_core::infra::i18n::Lang,
) -> Option<String> {
    if roles.any(|role| ops.bypass_roles.contains(&(role as i64))) {
        return None;
    }
    let created = pinchora_core::infra::snowflake::timestamp(user);
    if (now - created).num_days() < i64::from(ops.minimum_account_age_days) {
        return Some(t!(lang, "tickets.v2.account-age", days = ops.minimum_account_age_days));
    }
    if ops.minimum_membership_days > 0
        && joined_at.is_none_or(|joined| now.timestamp() - joined < i64::from(ops.minimum_membership_days) * 86400)
    {
        return Some(t!(
            lang,
            "tickets.v2.membership-age",
            days = ops.minimum_membership_days
        ));
    }
    None
}

/// The panel and topic a click names, if both still exist and the panel takes tickets; otherwise the
/// member has been told.
async fn usable(cx: &Cx, panel: i64, topic: usize) -> anyhow::Result<Option<(TicketPanel, Topic)>> {
    let found = cx
        .app
        .db
        .ticket_panel(panel)
        .await?
        .filter(|p| Some(p.guild_id as u64) == cx.guild_id());
    let Some(panel) = found else {
        tell(cx, t!(cx.lang(), "tickets.panel-gone")).await?;
        return Ok(None);
    };
    if !panel.enabled {
        tell(cx, t!(cx.lang(), "tickets.panel-off")).await?;
        return Ok(None);
    }
    let Some(topic) = panel.topics.get(topic).cloned() else {
        tell(cx, t!(cx.lang(), "tickets.panel-changed")).await?;
        return Ok(None);
    };
    Ok(Some((panel, topic)))
}

/// The Redis key of a member's wait between two tickets of one server.
fn cooldown_key(guild: u64, user: u64) -> String {
    format!("tk:{guild}:{user}")
}

/// Opens the ticket: checks the wait, the member's open tickets and the bot's permissions, makes the
/// channel or thread, says hello in it and logs it. The click is acknowledged first: making a channel
/// can take longer than Discord waits for an answer.
async fn open(cx: &Cx, panel: &TicketPanel, topic: Topic, answers: Vec<Answer>) -> anyhow::Result<()> {
    let lang = cx.lang();
    let Some(guild) = cx.guild_id() else {
        return tell(cx, t!(lang, "tickets.guild-only")).await;
    };
    let author = cx.author()?;
    let opener = author.id.get();
    let app = &cx.app;
    let settings = app.db.ticket_settings(guild).await?;
    let ops = app.db.ticket_panel_operations(panel.id).await?;
    let key = cooldown_key(guild, opener);
    let cooldown = if ops.cooldown_seconds > 0 {
        app.cache
            .try_cooldown(&key, Duration::from_secs(ops.cooldown_seconds as u64))
            .await?
    } else {
        None
    };
    if let Some(left) = cooldown {
        return tell(cx, t!(lang, "tickets.wait", secs = left.as_secs().max(1))).await;
    }
    cx.responder.defer(true).await?;
    let opened = match create(cx, guild, panel, &settings, &topic, &answers).await {
        Ok(Ok(ticket)) => ticket,
        Ok(Err(refusal)) => {
            let _ = app.cache.release_cooldown(&key).await;
            return tell(cx, refusal).await;
        }
        Err(error) => {
            let _ = app.cache.release_cooldown(&key).await;
            return Err(error);
        }
    };
    crate::admin::workflows::triggers::simple(app, guild, opener, "ticket_opened", opened.place());
    let place = opened.place().unwrap_or_default();
    let server_lang = guild_lang(app, guild).await;
    welcome(cx, &opened, panel, &topic, &answers, server_lang).await;
    let opener_name = scrub_value(&cx.author_display_name());
    log(
        app,
        guild,
        log_channel(&settings, Some(panel)),
        log_embed(
            BRAND,
            t!(server_lang, "tickets.log.opened-title", number = number(opened.number)),
            t!(
                server_lang,
                "tickets.log.opened",
                user = format!("<@{opener}>"),
                name = opener_name,
                place = format!("<#{place}>"),
                topic = scrub_value(&topic.label)
            ),
        ),
        None,
    )
    .await;
    let board = app.clone();
    tokio::spawn(async move {
        let _ = crate::community::tickets::refresh_status_board(&board, guild).await;
    });
    tell(cx, t!(lang, "tickets.opened", place = format!("<#{place}>"))).await
}
