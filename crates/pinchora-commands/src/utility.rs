//! Message hooks and the lease-backed sender for sticky and scheduled messages.

use std::{sync::Arc, time::Duration};

use chrono::{DateTime, Utc};
use pinchora_core::{
    modules::Module,
    t,
    utility::{Schedule, next_run},
};
use pinchora_db::ScheduledRow;
use twilight_model::{
    channel::{
        Message,
        message::{AllowedMentions, MentionType},
    },
    guild::Permissions,
    id::Id,
};

use crate::{App, custom::scrub_value, text::access::channel_permissions};

async fn post_channel(app: &App, guild: u64, channel: u64) -> anyhow::Result<twilight_model::channel::Channel> {
    if let Ok(response) = app.http.channel(Id::new(channel)).await {
        return Ok(response.model().await?);
    }
    app.http
        .guild_channels(Id::new(guild))
        .await?
        .models()
        .await?
        .into_iter()
        .find(|item| item.id.get() == channel)
        .ok_or_else(|| anyhow::anyhow!("channel not found"))
}

pub(crate) async fn after_message(app: &Arc<App>, msg: &Message) {
    let Some(guild) = msg.guild_id.map(|id| id.get()) else {
        return;
    };
    let app = Arc::clone(app);
    let message = msg.clone();
    app.clone().spawn(async move {
        if let Err(error) = handle_message(&app, guild, &message).await {
            tracing::warn!(%error,"utility message hook failed");
        }
    });
}

async fn handle_message(app: &App, guild: u64, msg: &Message) -> anyhow::Result<()> {
    let channel = msg.channel_id.get();
    if !app.module_allowed(guild, Some(channel), Module::Utility).await {
        return Ok(());
    }
    let author = msg.author.id.get();
    let sent = DateTime::<Utc>::from_timestamp_micros(msg.timestamp.as_micros()).unwrap_or_else(Utc::now);
    let cleared = app.db.clear_afk(guild, author, sent).await?;
    if !cleared.is_empty() {
        for _row in cleared.iter().filter(|r| r.nick_prefixed && r.guild_id != 0) {
            if let Ok(member) = app.http.guild_member(Id::new(guild), Id::new(author)).await
                && let Ok(member) = member.model().await
                && let Some(nick) = member.nick.and_then(|n| n.strip_prefix("[AFK] ").map(str::to_owned))
            {
                let _ = app
                    .http
                    .update_guild_member(Id::new(guild), Id::new(author))
                    .nick(Some(&nick))
                    .await;
            }
        }
        let lang = app.guild_lang(guild).await;
        let none = AllowedMentions::default();
        if let Ok(response) = app
            .http
            .create_message(Id::new(channel))
            .content(&t!(lang, "utility.afk-back"))
            .allowed_mentions(Some(&none))
            .await
            && let Ok(copy) = response.model().await
        {
            app.delete_later(Id::new(channel), copy.id, Duration::from_secs(10));
        }
    }
    let mentioned: Vec<i64> = msg
        .mentions
        .iter()
        .map(|u| u.id.get() as i64)
        .filter(|u| *u != author as i64)
        .collect();
    if !mentioned.is_empty() {
        let away = app.db.afk_for(guild, &mentioned).await?;
        if !away.is_empty() && app.db.claim_afk_reply(channel).await? {
            let lang = app.guild_lang(guild).await;
            let names = away
                .iter()
                .map(|r| {
                    scrub_value(
                        &msg.mentions
                            .iter()
                            .find(|m| m.id.get() as i64 == r.user_id)
                            .map_or_else(|| r.user_id.to_string(), |m| m.name.clone()),
                    )
                })
                .collect::<Vec<_>>()
                .join(", ");
            let reasons = away.iter().map(|r| r.message.clone()).collect::<Vec<_>>().join(" · ");
            let text = t!(
                lang,
                "utility.afk-mention",
                names = names,
                verb = t!(
                    lang,
                    if away.len() == 1 {
                        "utility.afk-is"
                    } else {
                        "utility.afk-are"
                    }
                ),
                reasons = reasons
            );
            let none = AllowedMentions::default();
            let _ = app
                .http
                .create_message(Id::new(channel))
                .content(&text)
                .allowed_mentions(Some(&none))
                .await;
        }
    }
    app.db.count_sticky_message(channel).await?;
    Ok(())
}

async fn live_permissions(app: &App, guild: u64, channel: u64, user: u64) -> anyhow::Result<Permissions> {
    let roles = app.http.roles(Id::new(guild)).await?.models().await?;
    let member = app
        .http
        .guild_member(Id::new(guild), Id::new(user))
        .await?
        .model()
        .await?;
    let held: Vec<u64> = member.roles.iter().map(|r| r.get()).collect();
    let base = roles
        .iter()
        .filter(|r| r.id.get() == guild || held.contains(&r.id.get()))
        .fold(Permissions::empty(), |p, r| p | r.permissions);
    let target = post_channel(app, guild, channel).await?;
    Ok(channel_permissions(
        base,
        guild,
        user,
        &held,
        target.permission_overwrites.as_deref().unwrap_or(&[]),
    ))
}

pub async fn bot_permissions(app: &App, guild: u64, channel: u64) -> anyhow::Result<Permissions> {
    live_permissions(app, guild, channel, app.application_id.get()).await
}

pub async fn creator_may_ping(app: &App, guild: u64, channel: u64, user: u64) -> bool {
    let result = async {
        if app.http.guild(Id::new(guild)).await?.model().await?.owner_id.get() == user {
            return anyhow::Ok(true);
        }
        let perms = live_permissions(app, guild, channel, user).await?;
        anyhow::Ok(perms.contains(Permissions::MENTION_EVERYONE))
    }
    .await;
    result.unwrap_or(false)
}

async fn post(
    app: &App,
    guild: u64,
    channel: u64,
    content: &str,
    embed: Option<&serde_json::Value>,
    ping: bool,
) -> anyhow::Result<u64> {
    let permissions = bot_permissions(app, guild, channel).await?;
    anyhow::ensure!(
        permissions.contains(Permissions::SEND_MESSAGES),
        "missing Send Messages"
    );
    let broad = ping && permissions.contains(Permissions::MENTION_EVERYONE);
    let mentions = AllowedMentions {
        parse: if broad {
            vec![MentionType::Everyone, MentionType::Roles]
        } else {
            Vec::new()
        },
        ..Default::default()
    };
    let text = if broad {
        content.to_owned()
    } else {
        content
            .replace("@everyone", "@\u{200B}everyone")
            .replace("@here", "@\u{200B}here")
    };
    let embeds = embed
        .and_then(|v| serde_json::from_value::<pinchora_core::tags::EmbedSpec>(v.clone()).ok())
        .map(|e| vec![crate::custom::embed_of(&e)]);
    let mut request = app
        .http
        .create_message(Id::new(channel))
        .content(&text)
        .allowed_mentions(Some(&mentions));
    if let Some(ref embeds) = embeds {
        request = request.embeds(embeds);
    }
    Ok(request.await?.model().await?.id.get())
}

pub async fn send_scheduled(app: &App, row: &ScheduledRow) -> anyhow::Result<u64> {
    let guild = row.guild_id as u64;
    let channel = row.channel_id as u64;
    let ping = row.ping_allowed
        && if let Some(actor) = row.created_by {
            creator_may_ping(app, guild, channel, actor as u64).await
        } else {
            false
        };
    post(app, guild, channel, &row.content, row.embed.as_ref(), ping).await
}

pub fn spawn_ticker(app: Arc<App>) {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(5));
        loop {
            interval.tick().await;
            if let Err(error) = tick(&app).await {
                tracing::warn!(%error,"utility ticker failed");
            }
        }
    });
}

pub async fn tick(app: &App) -> anyhow::Result<()> {
    for (channel, copy) in app.db.sticky_cleanups(30).await? {
        match app
            .http
            .delete_message(Id::new(channel as u64), Id::new(copy as u64))
            .await
        {
            Ok(_) => app.db.finish_sticky_cleanup(channel as u64, copy as u64).await?,
            Err(error) if matches!(crate::roles::error_code(&error), Some(10008 | 10003)) => {
                app.db.finish_sticky_cleanup(channel as u64, copy as u64).await?
            }
            Err(error) => tracing::warn!(%error,channel,copy,"sticky cleanup will retry"),
        }
    }
    for row in app.db.claim_stickies(20).await? {
        let channel = row.channel_id as u64;
        if !app
            .module_allowed(row.guild_id as u64, Some(channel), Module::Utility)
            .await
        {
            app.db.release_sticky(channel).await?;
            continue;
        }
        let result = async {
            let permissions = bot_permissions(app, row.guild_id as u64, channel).await?;
            anyhow::ensure!(
                permissions.contains(Permissions::MANAGE_MESSAGES),
                "missing Manage Messages"
            );
            let copy = post(
                app,
                row.guild_id as u64,
                channel,
                &row.content,
                row.embed.as_ref(),
                false,
            )
            .await?;
            app.db.finish_sticky(channel, copy).await?;
            if let Some(old) = row.last_message_id {
                app.db.queue_sticky_cleanup(channel, old as u64).await?;
            }
            anyhow::Ok(())
        }
        .await;
        if let Err(error) = result {
            tracing::warn!(%error,channel,"sticky post failed");
            app.db.release_sticky(channel).await?;
        }
    }
    for row in app.db.claim_schedules(20).await? {
        if !app
            .module_allowed(row.guild_id as u64, Some(row.channel_id as u64), Module::Utility)
            .await
        {
            app.db.release_schedule(row.id).await?;
            continue;
        }
        match send_scheduled(app, &row).await {
            Ok(_) => {
                let rule: Schedule = serde_json::from_value(row.schedule.clone())?;
                let next = next_run(&rule, &row.tz, Utc::now());
                app.db.finish_schedule(row.id, next).await?;
            }
            Err(error) => {
                tracing::warn!(%error,id=row.id,"scheduled message failed");
                app.db.release_schedule(row.id).await?;
            }
        }
    }
    Ok(())
}
