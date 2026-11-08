//! Opening a ticket: a panel's button or menu (`tk:`), its form (`tf:`) when it has one, then the
//! ticket's channel or private thread, the welcome message inside and the log line.

mod forms;
use forms::ask;
pub(crate) use forms::on_form;

use std::{str::Split, time::Duration};

use chrono::{Datelike, Timelike, Utc};
use chrono_tz::Tz;

use pinchora_core::{
    t,
    tickets::{ANSWER_MAX, Answer, DEFAULT_NAMING, Question, Topic, channel_name, fill_welcome},
};
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

use super::{
    Need, actions::controls, discord_refusal, guild_lang, log, log_channel, member_allow, missing, number, pinging,
    private, server_name, tell,
};
use crate::{
    Cx,
    custom::scrub_value,
    server_events::{button, row},
    util::{BRAND, single_line, strip_mentions},
};

/// The text input of question `n` in a panel's form.
fn question_id(n: usize) -> String {
    format!("q{n}")
}

/// The message context command lets the member pick a live panel and topic before the normal
/// FAQ, eligibility and form flow. The source link is carried through those steps.
pub(crate) async fn on_message_context(cx: &Cx, guild: u64, channel: u64, message: u64) -> anyhow::Result<()> {
    if !super::module_on(&cx.app, guild, Some(channel)).await {
        return tell(cx, t!(cx.lang(), "tickets.v2.context-unavailable")).await;
    }
    let options: Vec<SelectMenuOption> = cx
        .app
        .db
        .ticket_panels(guild)
        .await?
        .into_iter()
        .filter(|panel| panel.enabled && !panel.topics.is_empty())
        .map(|panel| SelectMenuOption {
            default: false,
            description: None,
            emoji: None,
            label: single_line(&panel.title).chars().take(100).collect(),
            value: panel.id.to_string(),
        })
        .collect();
    if options.is_empty() {
        return tell(cx, t!(cx.lang(), "tickets.v2.context-unavailable")).await;
    }
    cx.reply(private(
        t!(cx.lang(), "tickets.v2.context-pick"),
        vec![row(vec![Component::SelectMenu(SelectMenu {
            channel_types: None,
            custom_id: format!("tk:m:{channel}:{message}"),
            default_values: None,
            disabled: false,
            id: None,
            kind: SelectMenuType::Text,
            max_values: Some(1),
            min_values: Some(1),
            options: Some(options),
            placeholder: Some(t!(cx.lang(), "tickets.v2.context-pick")),
            required: Some(true),
        })])],
    ))
    .await
}

async fn choose_context_topic(cx: &Cx, panel_id: i64, source: Option<(u64, u64)>) -> anyhow::Result<()> {
    let Some((panel, _)) = usable(cx, panel_id, 0).await? else {
        return Ok(());
    };
    let Some((channel, message)) = source else {
        return tell(cx, t!(cx.lang(), "tickets.expired")).await;
    };
    let options = panel
        .topics
        .iter()
        .enumerate()
        .map(|(index, topic)| SelectMenuOption {
            default: false,
            description: None,
            emoji: None,
            label: single_line(&topic.label).chars().take(100).collect(),
            value: index.to_string(),
        })
        .collect();
    cx.reply(private(
        t!(cx.lang(), "tickets.v2.context-topic"),
        vec![row(vec![Component::SelectMenu(SelectMenu {
            channel_types: None,
            custom_id: format!("tk:q:{panel_id}:{channel}:{message}"),
            default_values: None,
            disabled: false,
            id: None,
            kind: SelectMenuType::Text,
            max_values: Some(1),
            min_values: Some(1),
            options: Some(options),
            placeholder: Some(t!(cx.lang(), "tickets.v2.context-topic")),
            required: Some(true),
        })])],
    ))
    .await
}

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
    lang: pinchora_core::i18n::Lang,
) -> Option<String> {
    if roles.any(|role| ops.bypass_roles.contains(&(role as i64))) {
        return None;
    }
    let created = pinchora_core::snowflake::timestamp(user);
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
    crate::workflows::triggers::simple(app, guild, opener, "ticket_opened", opened.place());
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
        let _ = super::refresh_status_board(&board, guild).await;
    });
    tell(cx, t!(lang, "tickets.opened", place = format!("<#{place}>"))).await
}

/// The ticket row and its place, or the sentence that says why there is none.
async fn create(
    cx: &Cx,
    guild: u64,
    panel: &TicketPanel,
    settings: &TicketSettings,
    topic: &Topic,
    answers: &[Answer],
) -> anyhow::Result<Result<Ticket, String>> {
    let app = &cx.app;
    let lang = cx.lang();
    let opener = cx.author()?.id.get();
    let thread = settings.mode().threads(channel_count(cx, guild, settings).await);
    let need = if thread {
        Need::Thread(panel.channel_id as u64)
    } else {
        Need::Channel
    };
    if let Some(lacking) = missing(cx, need) {
        return Ok(Err(lacking));
    }
    let new = NewTicket {
        guild_id: guild,
        panel_id: Some(panel.id),
        opener,
        topic: &topic.label,
        answers,
        max_open: settings.max_open_per_user,
        at: app.now(),
    };
    let mut opened = app.db.open_ticket(&new).await?;
    if matches!(opened, TicketOpen::TooMany { .. }) {
        let mut recovered = false;
        for stale in app.db.open_tickets_for_user(guild, opener).await? {
            let Some(place) = stale.place() else { continue };
            if let Err(error) = app.http.channel(Id::new(place)).await
                && crate::roles::error_code(&error) == Some(super::UNKNOWN_CHANNEL)
            {
                super::on_place_deleted(app, guild, place).await?;
                recovered = true;
            }
        }
        if recovered {
            opened = app.db.open_ticket(&new).await?;
        }
    }
    let ticket = match opened {
        TicketOpen::Opened(ticket) => *ticket,
        TicketOpen::Cooldown { seconds } => return Ok(Err(t!(lang, "tickets.wait", secs = seconds))),
        TicketOpen::TooMany { open } => {
            let ops = app.db.ticket_panel_operations(panel.id).await?;
            return Ok(Err(if ops.overflow_message.is_empty() {
                t!(lang, "tickets.too-many", count = open)
            } else {
                ops.overflow_message
            }));
        }
    };
    let user_name = cx.author()?.name.clone();
    let mut name = channel_name(&settings.naming, ticket.number, &user_name, &topic.label);
    if name.is_empty() {
        name = channel_name(DEFAULT_NAMING, ticket.number, "", "");
    }
    let made = if thread {
        make_thread(cx, panel, &name).await
    } else {
        make_channel(cx, guild, panel, settings, &name, &ticket).await
    };
    match made {
        Ok((channel, thread)) => {
            app.db.set_ticket_place(ticket.id, channel, thread).await?;
            Ok(Ok(Ticket {
                channel_id: Some(channel as i64),
                thread_id: thread.map(|t| t as i64),
                ..ticket
            }))
        }
        Err(refusal) => {
            app.db.abort_ticket(ticket.id).await?;
            Ok(Err(refusal))
        }
    }
}

/// How many channels the server has, when `auto` mode has to know; 0 when it does not or Discord will
/// not say (channels it is, then).
async fn channel_count(cx: &Cx, guild: u64, settings: &TicketSettings) -> usize {
    if settings.mode() != pinchora_core::tickets::Mode::Auto {
        return 0;
    }
    match cx.app.http.guild_channels(Id::new(guild)).await {
        Ok(response) => response.models().await.map(|c| c.len()).unwrap_or(0),
        Err(error) => {
            tracing::debug!(guild, %error, "channel count not read");
            0
        }
    }
}

/// Who sees a ticket channel: not everyone, the opener, the panel's support roles and the bot.
pub(crate) fn overwrites(guild: u64, opener: u64, bot: u64, support: &[i64]) -> Vec<PermissionOverwrite> {
    let mut list = vec![
        PermissionOverwrite {
            allow: Permissions::empty(),
            deny: Permissions::VIEW_CHANNEL,
            id: Id::new(guild),
            kind: PermissionOverwriteType::Role,
        },
        PermissionOverwrite {
            allow: member_allow(),
            deny: Permissions::empty(),
            id: Id::new(opener),
            kind: PermissionOverwriteType::Member,
        },
        PermissionOverwrite {
            allow: member_allow(),
            deny: Permissions::empty(),
            id: Id::new(bot),
            kind: PermissionOverwriteType::Member,
        },
    ];
    for role in support {
        list.push(PermissionOverwrite {
            allow: member_allow(),
            deny: Permissions::empty(),
            id: Id::new(*role as u64),
            kind: PermissionOverwriteType::Role,
        });
    }
    list
}

async fn make_channel(
    cx: &Cx,
    guild: u64,
    panel: &TicketPanel,
    settings: &TicketSettings,
    name: &str,
    ticket: &Ticket,
) -> Result<(u64, Option<u64>), String> {
    let lang = cx.lang();
    let overwrites = overwrites(
        guild,
        ticket.opener_id as u64,
        cx.app.application_id.get(),
        &panel.support_roles,
    );
    let topic = t!(
        lang,
        "tickets.channel-topic",
        number = number(ticket.number),
        topic = &ticket.topic
    );
    let mut request = cx
        .app
        .http
        .create_guild_channel(Id::new(guild), name)
        .kind(ChannelType::GuildText)
        .permission_overwrites(&overwrites)
        .topic(&topic);
    if let Some(category) = panel.open_category_id.or(settings.open_category_id) {
        request = request.parent_id(Id::new(category as u64));
    }
    match request.await {
        Ok(response) => match response.model().await {
            Ok(channel) => Ok((channel.id.get(), None)),
            Err(error) => Err({
                tracing::warn!(guild, %error, "ticket channel answer unreadable");
                t!(lang, "tickets.discord.refused")
            }),
        },
        Err(error) => Err(discord_refusal(&error, lang, Need::Channel)),
    }
}

async fn make_thread(cx: &Cx, panel: &TicketPanel, name: &str) -> Result<(u64, Option<u64>), String> {
    let lang = cx.lang();
    let parent: Id<ChannelMarker> = Id::new(panel.channel_id as u64);
    let made = cx
        .app
        .http
        .create_thread(parent, name, ChannelType::PrivateThread)
        .auto_archive_duration(AutoArchiveDuration::Week)
        .invitable(false)
        .await;
    let thread = match made {
        Ok(response) => match response.model().await {
            Ok(thread) => thread.id,
            Err(_) => return Err(t!(lang, "tickets.discord.refused")),
        },
        Err(error) => return Err(discord_refusal(&error, lang, Need::Thread(parent.get()))),
    };
    let opener = cx.interaction.author_id().unwrap_or(Id::new(1));
    if let Err(error) = cx.app.http.add_thread_member(thread, opener).await {
        tracing::warn!(thread = thread.get(), %error, "ticket opener not added to the thread");
    }
    Ok((parent.get(), Some(thread.get())))
}

/// The first message in a ticket: who it is for, what the panel says to them, their form answers, and the
/// controls. It pings the opener and the panel's support roles, nobody else.
async fn welcome(
    cx: &Cx,
    ticket: &Ticket,
    panel: &TicketPanel,
    topic: &Topic,
    answers: &[Answer],
    lang: pinchora_core::i18n::Lang,
) {
    let Some(place) = ticket.place() else { return };
    let opener = ticket.opener_id as u64;
    let server = server_name(&cx.app, ticket.guild_id as u64).await;
    let template = if panel.welcome_text.trim().is_empty() {
        t!(lang, "tickets.welcome.default")
    } else {
        strip_mentions(&panel.welcome_text)
    };
    let mut text = fill_welcome(
        &template,
        &format!("<@{opener}>"),
        &scrub_value(&server),
        ticket.number,
        &scrub_value(&topic.label),
    );
    if let Ok(ops) = cx.app.db.ticket_operation_settings(ticket.guild_id as u64).await
        && outside_support_hours(&ops, cx.app.now())
    {
        text.push_str("\n\n");
        text.push_str(&t!(
            lang,
            "tickets.v2.out-of-hours",
            message = scrub_value(&ops.out_of_hours_message)
        ));
    }
    let embed = welcome_embed(ticket, &text, answers, lang);
    let roles: Vec<u64> = panel.support_roles.iter().map(|r| *r as u64).collect();
    let mut content = format!("<@{opener}>");
    for role in &roles {
        content.push_str(&format!(" <@&{role}>"));
    }
    let embeds = [cx.app.brand_embed(ticket.guild_id as u64, embed).await];
    let components = controls(ticket.id, false, lang);
    let mentions = pinging(&[opener], &roles);
    let sent = cx
        .app
        .http
        .create_message(Id::new(place))
        .content(&content)
        .embeds(&embeds)
        .components(&components)
        .allowed_mentions(Some(&mentions))
        .await;
    if let Err(error) = sent {
        tracing::warn!(ticket = ticket.id, %error, "ticket welcome not posted");
    }
    for (index, batch) in answers.iter().skip(5).collect::<Vec<_>>().chunks(5).enumerate() {
        let mut embed = EmbedBuilder::new()
            .color(BRAND)
            .title(t!(lang, "tickets.v2.more-answers", page = index + 2));
        for answer in batch {
            embed = embed.field(EmbedFieldBuilder::new(
                single_line(&answer.question).chars().take(256).collect::<String>(),
                scrub_value(&answer.answer).chars().take(900).collect::<String>(),
            ));
        }
        let embeds = [cx.app.brand_embed(ticket.guild_id as u64, embed.build()).await];
        if let Err(error) = cx.app.http.create_message(Id::new(place)).embeds(&embeds).await {
            tracing::warn!(ticket = ticket.id, %error, "additional ticket form answers not posted");
        }
    }
}

fn outside_support_hours(settings: &pinchora_db::TicketOperationSettings, now: chrono::DateTime<Utc>) -> bool {
    if settings.support_hours.is_empty() {
        return false;
    }
    let Ok(tz) = settings.support_timezone.parse::<Tz>() else {
        return false;
    };
    let local = now.with_timezone(&tz);
    let day = local.weekday().number_from_monday() as u8;
    let minute = (local.hour() * 60 + local.minute()) as u16;
    !settings
        .support_hours
        .iter()
        .any(|window| window.day == day && window.start <= minute && minute < window.end)
}

pub(crate) fn welcome_embed(ticket: &Ticket, text: &str, answers: &[Answer], lang: pinchora_core::i18n::Lang) -> Embed {
    let mut embed = EmbedBuilder::new()
        .color(BRAND)
        .title(t!(
            lang,
            "tickets.welcome.title",
            number = number(ticket.number),
            topic = single_line(&ticket.topic)
        ))
        .description(text.chars().take(4_000).collect::<String>())
        .footer(EmbedFooterBuilder::new(t!(lang, "tickets.welcome.footer")));
    for answer in answers.iter().take(5) {
        let name: String = single_line(&answer.question).chars().take(256).collect();
        let value: String = scrub_value(&answer.answer).chars().take(1_024).collect();
        embed = embed.field(EmbedFieldBuilder::new(name, value));
    }
    embed.build()
}

/// A log entry: a coloured embed with a title and a line or two.
pub(crate) fn log_embed(color: u32, title: String, text: String) -> Embed {
    EmbedBuilder::new().color(color).title(title).description(text).build()
}

#[cfg(test)]
mod support_hours_tests {
    use super::*;
    #[test]
    fn support_hours_follow_local_weekdays_and_both_dst_transitions() {
        let settings = pinchora_db::TicketOperationSettings {
            support_timezone: "America/New_York".into(),
            support_hours: vec![pinchora_db::SupportWindow {
                day: 7,
                start: 60,
                end: 120,
            }],
            ..Default::default()
        };
        for utc in ["2026-11-01T05:30:00Z", "2026-11-01T06:30:00Z", "2026-03-08T06:30:00Z"] {
            assert!(!outside_support_hours(&settings, utc.parse().unwrap()), "{utc}");
        }
        for utc in ["2026-11-01T07:00:00Z", "2026-03-08T07:00:00Z", "2026-11-02T06:30:00Z"] {
            assert!(outside_support_hours(&settings, utc.parse().unwrap()), "{utc}");
        }
        let settings = pinchora_db::TicketOperationSettings {
            support_timezone: "Europe/Istanbul".into(),
            support_hours: vec![pinchora_db::SupportWindow {
                day: 1,
                start: 0,
                end: 60,
            }],
            ..settings
        };
        assert!(!outside_support_hours(
            &settings,
            "2026-11-08T21:30:00Z".parse().unwrap()
        ));
    }
}
