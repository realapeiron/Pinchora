//! Opening a ticket: a panel's button or menu (`tk:`), its form (`tf:`) when it has one, then the
//! ticket's channel or private thread, the welcome message inside and the log line.

use std::{str::Split, time::Duration};

use pinchora_core::{
    t,
    tickets::{ANSWER_MAX, Answer, DEFAULT_NAMING, OPEN_COOLDOWN_SECS, Question, Topic, channel_name, fill_welcome},
};
use pinchora_db::{NewTicket, Ticket, TicketOpen, TicketPanel, TicketSettings};
use twilight_model::{
    channel::permission_overwrite::{PermissionOverwrite, PermissionOverwriteType},
    channel::{
        ChannelType,
        message::{
            Component, Embed,
            component::{Label, TextInput, TextInputStyle},
        },
        thread::AutoArchiveDuration,
    },
    guild::Permissions,
    id::{Id, marker::ChannelMarker},
};
use twilight_util::builder::embed::{EmbedBuilder, EmbedFieldBuilder, EmbedFooterBuilder};

use super::{
    Need, actions::controls, discord_refusal, guild_lang, log, log_channel, member_allow, missing, number, pinging,
    server_name, tell,
};
use crate::{
    Cx,
    custom::scrub_value,
    util::{BRAND, single_line, strip_mentions},
};

/// The text input of question `n` in a panel's form.
fn question_id(n: usize) -> String {
    format!("q{n}")
}

/// `tk:o:<panel>:<topic>` (a button) or `tk:s:<panel>` with the picked topic (a menu).
pub(crate) async fn on_open(cx: &Cx, mut parts: Split<'_, char>, values: &[String]) -> anyhow::Result<()> {
    let picked = match parts.next() {
        Some("o") => parts
            .next()
            .and_then(|p| p.parse::<i64>().ok())
            .zip(parts.next().and_then(|t| t.parse::<usize>().ok())),
        Some("s") => parts
            .next()
            .and_then(|p| p.parse::<i64>().ok())
            .zip(values.first().and_then(|v| v.parse::<usize>().ok())),
        _ => None,
    };
    let Some((panel, index)) = picked else {
        return tell(cx, t!(cx.lang(), "tickets.expired")).await;
    };
    let Some((panel, topic)) = usable(cx, panel, index).await? else {
        return Ok(());
    };
    if !panel.questions.is_empty() {
        return ask(cx, &panel, index, &panel.questions).await;
    }
    open(cx, &panel, topic, Vec::new()).await
}

/// The form a panel asks before a ticket opens, as `tf:<panel>:<topic>`.
pub(crate) async fn on_form(cx: &Cx, mut parts: Split<'_, char>, fields: &[(String, String)]) -> anyhow::Result<()> {
    let picked = parts
        .next()
        .and_then(|p| p.parse::<i64>().ok())
        .zip(parts.next().and_then(|t| t.parse::<usize>().ok()));
    let Some((panel, topic)) = picked else {
        return tell(cx, t!(cx.lang(), "tickets.expired")).await;
    };
    let Some((panel, topic)) = usable(cx, panel, topic).await? else {
        return Ok(());
    };
    let answers: Vec<Answer> = panel
        .questions
        .iter()
        .enumerate()
        .filter_map(|(n, question)| {
            let answer = fields
                .iter()
                .find(|(id, _)| *id == question_id(n))
                .map(|(_, value)| value.trim().chars().take(ANSWER_MAX as usize).collect::<String>())?;
            (!answer.is_empty()).then(|| Answer {
                question: question.label.clone(),
                answer,
            })
        })
        .collect();
    open(cx, &panel, topic, answers).await
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

#[allow(deprecated)] // TextInput::label: the Label around it carries the label now
async fn ask(cx: &Cx, panel: &TicketPanel, topic: usize, questions: &[Question]) -> anyhow::Result<()> {
    let fields = questions
        .iter()
        .take(pinchora_core::tickets::MAX_QUESTIONS)
        .enumerate()
        .map(|(n, question)| {
            Component::Label(Label {
                component: Box::new(Component::TextInput(TextInput {
                    custom_id: question_id(n),
                    id: None,
                    label: None,
                    max_length: Some(ANSWER_MAX),
                    min_length: question.required.then_some(1),
                    placeholder: Some(question.placeholder.clone()).filter(|p| !p.trim().is_empty()),
                    required: Some(question.required),
                    style: if question.long {
                        TextInputStyle::Paragraph
                    } else {
                        TextInputStyle::Short
                    },
                    value: None,
                })),
                description: None,
                id: None,
                label: question
                    .label
                    .chars()
                    .take(pinchora_core::tickets::QUESTION_MAX)
                    .collect(),
            })
        })
        .collect();
    let title: String = single_line(&panel.title).chars().take(45).collect();
    cx.modal(format!("tf:{}:{topic}", panel.id), &title, fields).await
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
    let key = cooldown_key(guild, opener);
    if let Some(left) = app
        .cache
        .try_cooldown(&key, Duration::from_secs(OPEN_COOLDOWN_SECS))
        .await?
    {
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
    let place = opened.place().unwrap_or_default();
    let server_lang = guild_lang(app, guild).await;
    welcome(cx, &opened, panel, &topic, &answers, server_lang).await;
    let opener_name = scrub_value(&cx.author_display_name());
    log(
        app,
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
    let opened = app
        .db
        .open_ticket(&NewTicket {
            guild_id: guild,
            panel_id: Some(panel.id),
            opener,
            topic: &topic.label,
            answers,
            max_open: settings.max_open_per_user,
            at: app.now(),
        })
        .await?;
    let ticket = match opened {
        TicketOpen::Opened(ticket) => *ticket,
        TicketOpen::TooMany { open } => {
            return Ok(Err(t!(lang, "tickets.too-many", count = open)));
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
    let text = fill_welcome(
        &template,
        &format!("<@{opener}>"),
        &scrub_value(&server),
        ticket.number,
        &scrub_value(&topic.label),
    );
    let embed = welcome_embed(ticket, &text, answers, lang);
    let roles: Vec<u64> = panel.support_roles.iter().map(|r| *r as u64).collect();
    let mut content = format!("<@{opener}>");
    for role in &roles {
        content.push_str(&format!(" <@&{role}>"));
    }
    let embeds = [embed];
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
