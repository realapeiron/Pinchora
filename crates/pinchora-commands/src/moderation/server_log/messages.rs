use super::*;

fn kept_key(id: u64) -> String {
    format!("logmsg:{id}")
}

/// Keeps a message for a day, so its edit or delete can show what it said and who sent it. Only a
/// message with text or files (its text only with message content on, and Discord sends no files
/// without it either), and only where the log reports message edits or deletes.
pub(crate) async fn keep_message(app: &App, guild: u64, message: &Message) {
    let content = read_text(app, message);
    if content.is_empty() && message.attachments.is_empty() {
        return;
    }
    let logged = match app.log_config(guild).await {
        Ok(config) => [
            LogEvent::MessageEdit,
            LogEvent::MessageDelete,
            LogEvent::MessageBulkDelete,
        ]
        .into_iter()
        .any(|e| config.channel(e).is_some()),
        Err(_) => false,
    };
    if !logged {
        return;
    }
    let kept = KeptMessage {
        author: message.author.id.get(),
        name: display_name(&message.author, message.member.as_ref().and_then(|m| m.nick.as_deref())),
        content: clip(content, MESSAGE_TEXT_MAX),
        attachments: message.attachments.iter().map(|a| a.filename.clone()).collect(),
    };
    if let Ok(json) = serde_json::to_string(&kept)
        && let Err(error) = app.cache.put(&kept_key(message.id.get()), &json, MESSAGE_KEPT).await
    {
        tracing::debug!(%error, "message not kept for the log");
    }
}

/// What a message says, as far as the log may read it: nothing without message content on (Discord
/// still sends the text of a message that mentions the bot, for text commands; the log leaves it).
fn read_text<'a>(app: &App, message: &'a Message) -> &'a str {
    if app.message_content { &message.content } else { "" }
}

async fn kept(app: &App, id: u64) -> Option<KeptMessage> {
    // Redis outlives a restart that switches Message Content off. Do not expose older text either.
    if !app.message_content {
        return None;
    }
    let json = app.cache.get(&kept_key(id)).await.ok().flatten()?;
    serde_json::from_str(&json).ok()
}

fn jump(guild: u64, channel: u64, message: u64) -> String {
    format!("https://discord.com/channels/{guild}/{channel}/{message}")
}

/// Message text for a field: clipped, and something when it is empty.
pub(super) fn shown(lang: Lang, text: &str) -> String {
    if text.trim().is_empty() {
        t!(lang, "log.entry.no-text")
    } else {
        clip(text, 1_000)
    }
}

/// Logs an edit: who, where, a link, and the text before and after when it is known.
pub async fn on_message_update(app: Arc<App>, event: MessageUpdate) -> anyhow::Result<()> {
    crate::community::starboard::on_update(&app, &event).await;
    let mut message = event.0;
    if !app.message_content {
        message.content.clear();
    }
    let Some(guild) = message.guild_id.map(|g| g.get()) else {
        return Ok(());
    };
    // Discord also sends an update when a link unfurls; only a real edit has an edit time.
    if message.author.bot || message.webhook_id.is_some() || message.edited_timestamp.is_none() {
        return Ok(());
    }
    let before = kept(&app, message.id.get()).await;
    if before.as_ref().is_some_and(|b| b.content == message.content) {
        return Ok(());
    }
    let roles: Vec<u64> = message
        .member
        .as_ref()
        .map(|m| m.roles.iter().map(|r| r.get()).collect())
        .unwrap_or_default();
    let subject = Subject {
        roles: &roles,
        ..user_subject(&message.author)
    };
    let channel_id = message.channel_id.get();
    if let Some((to, lang)) = route(&app, guild, LogEvent::MessageEdit, subject, Some(channel_id)).await {
        let text = t!(
            lang,
            "log.entry.edited",
            member = mention(message.author.id),
            link = jump(guild, channel_id, message.id.get()),
            channel = format!("<#{channel_id}>")
        );
        let mut embed = entry(
            lang,
            title(lang, LogEvent::MessageEdit),
            WARN,
            message.author.id.get(),
            Some(&message.author),
            text,
        );
        if let Some(before) = &before {
            embed = embed.field(EmbedFieldBuilder::new(
                t!(lang, "log.entry.before"),
                shown(lang, &before.content),
            ));
        }
        // Without the Message Content intent the new text is not known either.
        if before.is_some() || !message.content.is_empty() {
            embed = embed.field(EmbedFieldBuilder::new(
                t!(lang, "log.entry.after"),
                shown(lang, &message.content),
            ));
        }
        post(&app, guild, to, embed.build()).await;
    }
    keep_message(&app, guild, &message).await;
    Ok(())
}

/// Logs a deleted message: what it said and who sent it when it was kept, otherwise where and when.
pub async fn on_message_delete(app: Arc<App>, event: MessageDelete) -> anyhow::Result<()> {
    crate::community::starboard::on_delete(&app, &event).await;
    let Some(guild) = event.guild_id.map(|g| g.get()) else {
        return Ok(());
    };
    let (id, channel_id) = (event.id.get(), event.channel_id.get());
    let message = kept(&app, id).await;
    let subject = Subject {
        user: message.as_ref().map(|m| m.author),
        ..Subject::default()
    };
    let Some((to, lang)) = route(&app, guild, LogEvent::MessageDelete, subject, Some(channel_id)).await else {
        return Ok(());
    };
    let sent = relative(snowflake::timestamp(id));
    let channel = format!("<#{channel_id}>");
    let title = title(lang, LogEvent::MessageDelete);
    let embed = match &message {
        Some(message) => {
            let text = t!(
                lang,
                "log.entry.deleted-by",
                member = mention(message.author),
                channel = channel
            );
            let mut embed = entry(lang, title, DANGER, message.author, None, text)
                .field(EmbedFieldBuilder::new(
                    t!(lang, "log.entry.message"),
                    shown(lang, &message.content),
                ))
                .field(EmbedFieldBuilder::new(t!(lang, "log.entry.sent"), sent).inline());
            if !message.attachments.is_empty() {
                let files: Vec<String> = message.attachments.iter().map(|f| code(f)).collect();
                let files = clip(&files.join(" "), 1_000);
                embed = embed.field(EmbedFieldBuilder::new(t!(lang, "log.entry.attachments"), files));
            }
            embed
        }
        None => entry(
            lang,
            title,
            DANGER,
            id,
            None,
            t!(lang, "log.entry.deleted", channel = channel),
        )
        .field(EmbedFieldBuilder::new(t!(lang, "log.entry.sent"), sent).inline()),
    };
    post(&app, guild, to, embed.build()).await;
    Ok(())
}

/// Logs a bulk delete: how many, where, and the messages that were kept, oldest first.
pub async fn on_message_delete_bulk(app: Arc<App>, event: MessageDeleteBulk) -> anyhow::Result<()> {
    crate::community::starboard::on_delete_bulk(&app, &event).await;
    let Some(guild) = event.guild_id.map(|g| g.get()) else {
        return Ok(());
    };
    let channel_id = event.channel_id.get();
    let Some((to, lang)) = route(
        &app,
        guild,
        LogEvent::MessageBulkDelete,
        Subject::default(),
        Some(channel_id),
    )
    .await
    else {
        return Ok(());
    };
    let mut ids: Vec<u64> = event.ids.iter().map(|id| id.get()).collect();
    ids.sort_unstable();
    let mut lines = Vec::new();
    for id in &ids {
        if let Some(message) = kept(&app, *id).await {
            lines.push(format!(
                "**{}**: {}",
                message.name.replace('*', ""),
                clip(&message.content.replace('\n', " "), 200)
            ));
        }
    }
    let mut text = t!(
        lang,
        "log.entry.bulk",
        count = ids.len(),
        channel = format!("<#{channel_id}>")
    );
    if !lines.is_empty() {
        text.push_str("\n\n");
        text.push_str(&clip(&lines.join("\n"), 3_500));
    }
    let title = title(lang, LogEvent::MessageBulkDelete);
    let embed = entry(lang, title, DANGER, channel_id, None, text).build();
    post(&app, guild, to, embed).await;
    Ok(())
}
