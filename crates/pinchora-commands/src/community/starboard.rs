//! Reaction-driven starboards and vote channels.

mod eligibility;
use eligibility::*;
mod events;
pub(crate) use events::{on_delete, on_delete_bulk};
pub use events::{on_message, on_update};
mod ticker;
pub use ticker::{cleanup_one, spawn_ticker, tick};
#[cfg(test)]
mod tests;

use std::{
    sync::{Arc, LazyLock},
    time::Duration,
};

use tokio::sync::Semaphore;

use pinchora_core::admin::modules::Module;
use pinchora_core::admin::server_stats::grouped_in;
use pinchora_core::t;
use pinchora_db::{StarPost, Starboard};
use twilight_http::request::channel::reaction::RequestReactionType;
use twilight_model::{
    channel::{
        Channel, ChannelType, Message,
        message::{AllowedMentions, EmojiReactionType},
    },
    gateway::payload::incoming::{MessageDelete, MessageDeleteBulk, MessageUpdate},
    guild::Permissions,
    id::Id,
};
use twilight_util::builder::embed::EmbedBuilder;

use crate::App;
use crate::infra::hooks::ReactionEvent;

pub async fn on_reaction(app: &Arc<App>, event: &ReactionEvent) {
    let (guild, channel, message, user, emoji) = match event {
        ReactionEvent::Add(e) => (
            e.guild_id.map(|v| v.get()),
            e.channel_id.get(),
            e.message_id.get(),
            e.user_id.get(),
            &e.emoji,
        ),
        ReactionEvent::Remove(e) => (
            e.guild_id.map(|v| v.get()),
            e.channel_id.get(),
            e.message_id.get(),
            e.user_id.get(),
            &e.emoji,
        ),
        ReactionEvent::RemoveAll(e) => {
            if let Some(guild) = e.guild_id {
                cleared(app, guild.get(), e.channel_id.get(), e.message_id.get(), None).await;
            }
            return;
        }
        ReactionEvent::RemoveEmoji(e) => {
            cleared(
                app,
                e.guild_id.get(),
                e.channel_id.get(),
                e.message_id.get(),
                Some(&e.emoji),
            )
            .await;
            return;
        }
    };
    let Some(guild) = guild else { return };
    if user == app.application_id.get() {
        return;
    }
    let app = app.clone();
    let who = Who {
        guild,
        channel,
        message,
    };
    let emoji = emoji.clone();
    app.clone().spawn(async move {
        if let Err(error) = process_reaction(&app, who, &emoji).await {
            tracing::warn!(guild, message, %error, "starboard reaction failed");
        }
    });
}

async fn cleared(app: &Arc<App>, guild: u64, channel: u64, message: u64, emoji: Option<&EmojiReactionType>) {
    let app = app.clone();
    let emoji = emoji.cloned();
    app.clone().spawn(async move {
        let Ok(boards) = app.db.starboards(guild).await else {
            return;
        };
        for board in boards
            .into_iter()
            .filter(|b| eligible(b, channel) && emoji.as_ref().is_none_or(|e| emoji_matches(&b.emoji, e)))
        {
            if let Ok(Some(post)) = app.db.star_post(board.id, message).await
                && let Err(error) = app
                    .db
                    .touch_star_post(board.id, message, channel, post.author_id as u64)
                    .await
            {
                tracing::warn!(%error, "starboard reaction clear failed");
            }
        }
    });
}

async fn process_reaction(app: &App, who: Who, emoji: &EmojiReactionType) -> anyhow::Result<()> {
    let Who {
        guild,
        channel,
        message,
    } = who;
    let boards = app.db.starboards(guild).await?;
    let matched: Vec<_> = boards
        .into_iter()
        .filter(|b| eligible(b, channel) && emoji_matches(&b.emoji, emoji))
        .collect();
    if matched.is_empty() {
        return Ok(());
    }
    let original = app
        .http
        .message(Id::new(channel), Id::new(message))
        .await?
        .model()
        .await?;
    for board in matched {
        app.db
            .touch_star_post(board.id, message, channel, original.author.id.get())
            .await?;
    }
    Ok(())
}

fn unknown_message(error: &twilight_http::Error) -> bool {
    matches!(error.kind(), twilight_http::error::ErrorType::Response { status, .. } if status.get() == 404)
}

async fn reconcile_post(app: &App, board: &Starboard, post: &StarPost, force: bool) -> anyhow::Result<()> {
    let guild = board.guild_id as u64;
    let channel = post.channel_id as u64;
    let source = match app.http.channel(Id::new(channel)).await {
        Ok(response) => response.model().await?,
        Err(error) if unknown_message(&error) => {
            if let Some(copy) = post.board_message_id.filter(|id| *id > 0) {
                app.db
                    .clear_star_copy(board.id, post.message_id as u64, copy as u64)
                    .await?;
            }
            return Ok(());
        }
        Err(error) => return Err(error.into()),
    };
    let policy = board.enabled
        && eligible(board, channel)
        && app.module_allowed(guild, Some(channel), Module::Starboard).await
        && can_copy(app, board, &source).await?;
    let original = if policy {
        match app
            .http
            .message(Id::new(channel), Id::new(post.message_id as u64))
            .await
        {
            Ok(response) => Some(response.model().await?),
            Err(error) if unknown_message(&error) => None,
            Err(error) => return Err(error.into()),
        }
    } else {
        None
    };
    let allowed = original.as_ref().is_some_and(|message| {
        (!board.require_image || app.message_content && has_image(message)) && (board.allow_bots || !message.author.bot)
    });
    if !allowed {
        if let Some(copy) = post.board_message_id.filter(|id| *id > 0) {
            app.db
                .clear_star_copy(board.id, post.message_id as u64, copy as u64)
                .await?;
        }
        return Ok(());
    }
    let original = original.expect("allowed original");
    let roles = app.http.roles(Id::new(guild)).await?.models().await?;
    let base = roles
        .iter()
        .find(|role| role.id.get() == guild)
        .map_or(Permissions::empty(), |role| role.permissions);
    let mut votes = Vec::new();
    let mut after = None;
    let emoji = reaction_type(&board.emoji);
    loop {
        let request = app
            .http
            .reactions(Id::new(channel), Id::new(post.message_id as u64), &emoji)
            .limit(100);
        let users = match after {
            Some(id) => request.after(Id::new(id)).await?.models().await?,
            None => request.await?.models().await?,
        };
        for user in &users {
            if user.id.get() == app.application_id.get()
                || (!board.allow_bots && user.bot)
                || (!board.self_star && user.id == original.author.id)
            {
                continue;
            }
            let member = match app.http.guild_member(Id::new(guild), user.id).await {
                Ok(response) => response.model().await?,
                Err(error) if unknown_message(&error) => continue,
                Err(error) => return Err(error.into()),
            };
            let held: Vec<u64> = member.roles.iter().map(|role| role.get()).collect();
            let permissions = crate::infra::text::access::channel_permissions(
                roles
                    .iter()
                    .filter(|role| held.contains(&role.id.get()))
                    .fold(base, |permissions, role| permissions | role.permissions),
                guild,
                user.id.get(),
                &held,
                source.permission_overwrites.as_deref().unwrap_or(&[]),
            );
            if permissions.contains(Permissions::VIEW_CHANNEL) {
                votes.push((user.id.get(), weight(board, &member.roles)));
            }
        }
        if users.len() < 100 {
            break;
        }
        after = users.last().map(|user| user.id.get());
    }
    if let Some((current, changed)) = app
        .db
        .replace_star_votes(board.id, post.message_id as u64, &votes)
        .await?
        && (force || changed || current.board_message_id.is_none())
    {
        sync_post(app, board, &current, Some(&original)).await?;
    }
    Ok(())
}

pub async fn reconcile_one(app: &App, board_id: i64, message: u64) -> anyhow::Result<()> {
    let Some(board) = app.db.starboard(board_id).await? else {
        return Ok(());
    };
    let Some(post) = app.db.star_post(board_id, message).await? else {
        return Ok(());
    };
    reconcile_post(app, &board, &post, true).await
}

async fn sync_post(app: &App, board: &Starboard, post: &StarPost, original: Option<&Message>) -> anyhow::Result<()> {
    let dest = Id::new(board.channel_id as u64);
    if post.star_count < board.threshold && board.remove_below {
        if let Some(copy) = post.board_message_id.filter(|v| *v > 0) {
            app.db
                .clear_star_copy_if_below(board.id, post.message_id as u64, copy as u64, board.threshold)
                .await?;
        }
        return Ok(());
    }
    if post.star_count < board.threshold && post.board_message_id.is_none_or(|copy| copy <= 0) {
        return Ok(());
    }
    let fetched;
    let original = match original {
        Some(v) => v,
        None => {
            fetched = app
                .http
                .message(Id::new(post.channel_id as u64), Id::new(post.message_id as u64))
                .await?
                .model()
                .await?;
            &fetched
        }
    };
    let lang = app.guild_lang(board.guild_id as u64).await;
    let jump = format!(
        "https://discord.com/channels/{}/{}/{}",
        board.guild_id, post.channel_id, post.message_id
    );
    let heading = t!(
        lang,
        "starboard.post.heading",
        count = grouped_in(lang, i64::from(post.star_count)),
        channel = format!("<#{}>", post.channel_id)
    );
    let mut embed = EmbedBuilder::new()
        .title(t!(
            lang,
            "starboard.post.title",
            board = crate::admin::custom::scrub_value(&board.name)
        ))
        .description(format!(
            "{heading}\n{}\n{jump}",
            t!(lang, "starboard.post.author", author = format!("<@{}>", post.author_id))
        ))
        .color(board.color as u32);
    if app.message_content && !original.content.is_empty() {
        embed = embed.field(twilight_util::builder::embed::EmbedFieldBuilder::new(
            t!(lang, "starboard.post.message"),
            crate::admin::custom::scrub_value(&original.content.chars().take(1000).collect::<String>()),
        ));
    }
    if !app.message_content {
        embed = embed.footer(twilight_util::builder::embed::EmbedFooterBuilder::new(t!(
            lang,
            "starboard.post.locked"
        )));
    }
    let files: Vec<_> = if app.message_content {
        original.attachments.iter().take(5).map(|a| a.url.as_str()).collect()
    } else {
        Vec::new()
    };
    if !files.is_empty() {
        embed = embed.field(twilight_util::builder::embed::EmbedFieldBuilder::new(
            t!(lang, "starboard.post.attachments"),
            files.join("\n"),
        ));
    }
    let embeds = [app.brand_embed(board.guild_id as u64, embed.build()).await];
    if let Some(copy) = post.board_message_id.filter(|v| *v > 0) {
        match app
            .http
            .update_message(dest, Id::new(copy as u64))
            .embeds(Some(&embeds))
            .await
        {
            Ok(_) => {}
            Err(error) if unknown_message(&error) => {
                app.db
                    .clear_star_copy(board.id, post.message_id as u64, copy as u64)
                    .await?;
            }
            Err(error) => return Err(error.into()),
        }
    } else if let Some(token) = app
        .db
        .claim_star_post(board.id, post.message_id as u64, board.threshold)
        .await?
    {
        let none = AllowedMentions::default();
        let sent = app
            .http
            .create_message(dest)
            .embeds(&embeds)
            .allowed_mentions(Some(&none))
            .await;
        match sent {
            Ok(response) => {
                let copy = response.model().await?.id.get();
                crate::admin::workflows::observe(
                    app,
                    board.guild_id as u64,
                    post.author_id as u64,
                    "starboard_post",
                    Some(board.channel_id as u64),
                )
                .await;
                if !app
                    .db
                    .finish_star_post(board.id, post.message_id as u64, token, Some(copy))
                    .await?
                {
                    app.db.queue_star_cleanup(board.channel_id as u64, copy).await?;
                }
            }
            Err(error) => {
                if !matches!(
                    error.kind(),
                    twilight_http::error::ErrorType::RequestTimedOut
                        | twilight_http::error::ErrorType::RequestError
                        | twilight_http::error::ErrorType::RequestCanceled
                ) {
                    app.db
                        .finish_star_post(board.id, post.message_id as u64, token, None)
                        .await?;
                } else {
                    app.db
                        .mark_star_post_ambiguous(board.id, post.message_id as u64, token, board.channel_id as u64)
                        .await?;
                    tracing::error!(
                        board = board.id,
                        message = post.message_id,
                        "ambiguous starboard send requires reconciliation"
                    );
                }
                return Err(error.into());
            }
        }
    }
    Ok(())
}
