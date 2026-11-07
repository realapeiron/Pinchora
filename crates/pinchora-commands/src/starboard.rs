//! Reaction-driven starboards and vote channels.

use std::{sync::Arc, time::Duration};

use pinchora_core::{modules::Module, t};
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

use crate::{App, hooks::ReactionEvent};

fn emoji_matches(configured: &str, reaction: &EmojiReactionType) -> bool {
    match reaction {
        EmojiReactionType::Unicode { name } => configured.replace('\u{fe0f}', "") == name.replace('\u{fe0f}', ""),
        EmojiReactionType::Custom { id, .. } => configured.contains(&format!(":{}>", id.get())),
    }
}

fn reaction_type(emoji: &str) -> RequestReactionType<'_> {
    let custom = emoji
        .trim_end_matches('>')
        .rsplit_once(':')
        .and_then(|(_, id)| id.parse::<u64>().ok());
    match custom {
        Some(id) if emoji.starts_with('<') && id > 0 => RequestReactionType::Custom {
            id: Id::new(id),
            name: None,
        },
        _ => RequestReactionType::Unicode { name: emoji },
    }
}

fn eligible(board: &Starboard, channel: u64) -> bool {
    board.enabled
        && board.channel_id as u64 != channel
        && (board.only_channels.is_empty() || board.only_channels.contains(&(channel as i64)))
        && !board.except_channels.contains(&(channel as i64))
}

fn everyone_visible(guild: u64, roles: &[twilight_model::guild::Role], channel: &Channel) -> bool {
    let base = roles
        .iter()
        .find(|r| r.id.get() == guild)
        .map_or(Permissions::empty(), |r| r.permissions);
    crate::text::access::channel_permissions(
        base,
        guild,
        guild,
        &[],
        channel.permission_overwrites.as_deref().unwrap_or(&[]),
    )
    .contains(Permissions::VIEW_CHANNEL)
}

fn nsfw_compatible(source: Option<bool>, parent: Option<bool>, target: Option<bool>) -> bool {
    let is_nsfw = if source.is_none() && parent.is_none() {
        true
    } else {
        source.unwrap_or(false) || parent.unwrap_or(false)
    };
    !is_nsfw || target.unwrap_or(false)
}

async fn can_copy(app: &App, board: &Starboard, source: &Channel) -> anyhow::Result<bool> {
    let guild = board.guild_id as u64;
    let target = app
        .http
        .channel(Id::new(board.channel_id as u64))
        .await?
        .model()
        .await?;
    if source.guild_id.is_none_or(|v| v.get() != guild) || target.guild_id.is_none_or(|v| v.get() != guild) {
        return Ok(false);
    }
    let parent = if source.kind.is_thread() {
        match source.parent_id {
            Some(id) => Some(app.http.channel(id).await?.model().await?),
            None => return Ok(false),
        }
    } else {
        None
    };
    if !nsfw_compatible(source.nsfw, parent.as_ref().and_then(|p| p.nsfw), target.nsfw) {
        return Ok(false);
    }
    let roles = app.http.roles(Id::new(guild)).await?.models().await?;
    let source_public = if source.kind == ChannelType::PrivateThread {
        false
    } else if let Some(parent) = &parent {
        everyone_visible(guild, &roles, parent)
    } else {
        everyone_visible(guild, &roles, source)
    };
    if source_public || !everyone_visible(guild, &roles, &target) {
        return Ok(true);
    }
    Ok(board.only_channels.contains(&(source.id.get() as i64)))
}

fn has_image(message: &Message) -> bool {
    message
        .attachments
        .iter()
        .any(|a| a.content_type.as_deref().is_some_and(|t| t.starts_with("image/")))
        || !message
            .embeds
            .iter()
            .all(|e| e.image.is_none() && e.thumbnail.is_none())
}

fn weight(board: &Starboard, roles: &[Id<twilight_model::id::marker::RoleMarker>]) -> i32 {
    roles
        .iter()
        .filter_map(|r| {
            board
                .role_weights
                .get(r.get().to_string())
                .and_then(serde_json::Value::as_i64)
        })
        .max()
        .unwrap_or(1)
        .clamp(1, 50) as i32
}

struct Who {
    guild: u64,
    channel: u64,
    message: u64,
    user: u64,
}

pub(crate) async fn on_reaction(app: &Arc<App>, event: &ReactionEvent) {
    let (guild, channel, message, user, emoji, adding, roles) = match event {
        ReactionEvent::Add(e) => (
            e.guild_id.map(|v| v.get()),
            e.channel_id.get(),
            e.message_id.get(),
            e.user_id.get(),
            &e.emoji,
            true,
            e.member.as_ref().map(|m| m.roles.as_slice()),
        ),
        ReactionEvent::Remove(e) => (
            e.guild_id.map(|v| v.get()),
            e.channel_id.get(),
            e.message_id.get(),
            e.user_id.get(),
            &e.emoji,
            false,
            None,
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
    if user == app.application_id.get() || !app.module_allowed(guild, Some(channel), Module::Starboard).await {
        return;
    }
    let app = app.clone();
    let who = Who {
        guild,
        channel,
        message,
        user,
    };
    let emoji = emoji.clone();
    let roles = roles.map(<[_]>::to_vec);
    app.clone().spawn(async move {
        if let Err(error) = process_reaction(&app, who, &emoji, adding, roles.as_deref()).await {
            tracing::warn!(guild, message, %error, "starboard reaction failed");
        }
    });
}

async fn cleared(app: &Arc<App>, guild: u64, channel: u64, message: u64, emoji: Option<&EmojiReactionType>) {
    if !app.module_allowed(guild, Some(channel), Module::Starboard).await {
        return;
    }
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
            match app.db.clear_stars(board.id, message).await {
                Ok(Some(post)) => {
                    if let Err(error) = sync_post(&app, &board, &post, None).await {
                        tracing::warn!(%error, "starboard reaction clear failed");
                    }
                }
                Ok(None) => {}
                Err(error) => tracing::warn!(%error, "starboard reaction clear failed"),
            }
        }
    });
}

async fn process_reaction(
    app: &App,
    who: Who,
    emoji: &EmojiReactionType,
    adding: bool,
    roles: Option<&[Id<twilight_model::id::marker::RoleMarker>]>,
) -> anyhow::Result<()> {
    let Who {
        guild,
        channel,
        message,
        user,
    } = who;
    let boards = app.db.starboards(guild).await?;
    let matched: Vec<_> = boards
        .into_iter()
        .filter(|b| eligible(b, channel) && emoji_matches(&b.emoji, emoji))
        .collect();
    if matched.is_empty() {
        return Ok(());
    }
    let source = app.http.channel(Id::new(channel)).await?.model().await?;
    let original = app
        .http
        .message(Id::new(channel), Id::new(message))
        .await?
        .model()
        .await?;
    for board in matched {
        if !can_copy(app, &board, &source).await.unwrap_or(false)
            || board.require_image && !has_image(&original)
            || !board.allow_bots && original.author.bot
            || !board.self_star && original.author.id.get() == user
        {
            continue;
        }
        let held = if adding {
            match roles {
                Some(held) => held.to_vec(),
                None => {
                    app.http
                        .guild_member(Id::new(guild), Id::new(user))
                        .await?
                        .model()
                        .await?
                        .roles
                }
            }
        } else {
            Vec::new()
        };
        let count = app
            .db
            .star_change(
                board.id,
                message,
                channel,
                original.author.id.get(),
                user,
                adding.then(|| weight(&board, &held)),
            )
            .await?;
        sync_post(app, &board, &count, Some(&original)).await?;
    }
    Ok(())
}

async fn sync_post(app: &App, board: &Starboard, post: &StarPost, original: Option<&Message>) -> anyhow::Result<()> {
    let dest = Id::new(board.channel_id as u64);
    if post.star_count < board.threshold && board.remove_below {
        if let Some(copy) = post.board_message_id.filter(|v| *v > 0) {
            let _ = app.http.delete_message(dest, Id::new(copy as u64)).await;
            app.db
                .clear_star_copy(board.id, post.message_id as u64, copy as u64)
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
        count = post.star_count,
        channel = format!("<#{}>", post.channel_id)
    );
    let mut embed = EmbedBuilder::new()
        .title(t!(
            lang,
            "starboard.post.title",
            board = crate::custom::scrub_value(&board.name)
        ))
        .description(format!(
            "{heading}\n{}\n{jump}",
            t!(lang, "starboard.post.author", author = format!("<@{}>", post.author_id))
        ))
        .color(board.color as u32);
    if app.message_content && !original.content.is_empty() {
        embed = embed.field(twilight_util::builder::embed::EmbedFieldBuilder::new(
            t!(lang, "starboard.post.message"),
            crate::custom::scrub_value(&original.content.chars().take(1000).collect::<String>()),
        ));
    }
    if !app.message_content {
        embed = embed.footer(twilight_util::builder::embed::EmbedFooterBuilder::new(t!(
            lang,
            "starboard.post.locked"
        )));
    }
    let files: Vec<_> = original.attachments.iter().take(5).map(|a| a.url.as_str()).collect();
    if !files.is_empty() {
        embed = embed.field(twilight_util::builder::embed::EmbedFieldBuilder::new(
            t!(lang, "starboard.post.attachments"),
            files.join("\n"),
        ));
    }
    let embeds = [embed.build()];
    if let Some(copy) = post.board_message_id.filter(|v| *v > 0) {
        if app
            .cache
            .try_cooldown(
                &format!("star-edit:{}:{}", board.id, post.message_id),
                Duration::from_secs(2),
            )
            .await?
            .is_none()
        {
            app.http
                .update_message(dest, Id::new(copy as u64))
                .embeds(Some(&embeds))
                .await?;
        } else if app
            .cache
            .try_cooldown(
                &format!("star-edit-pending:{}:{}", board.id, post.message_id),
                Duration::from_secs(2),
            )
            .await?
            .is_none()
        {
            tokio::time::sleep(Duration::from_millis(2_100)).await;
            if let Some(latest) = app.db.star_post(board.id, post.message_id as u64).await? {
                Box::pin(sync_post(app, board, &latest, None)).await?;
            }
        }
    } else if app
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
                app.db
                    .finish_star_post(board.id, post.message_id as u64, Some(copy))
                    .await?;
                if let Some(latest) = app.db.star_post(board.id, post.message_id as u64).await?
                    && latest.star_count != post.star_count
                {
                    Box::pin(sync_post(app, board, &latest, Some(original))).await?;
                }
            }
            Err(error) => {
                app.db.finish_star_post(board.id, post.message_id as u64, None).await?;
                return Err(error.into());
            }
        }
    }
    Ok(())
}

pub async fn on_message(app: &Arc<App>, message: &Message) {
    let Some(guild) = message.guild_id.map(|g| g.get()) else {
        return;
    };
    if !app
        .module_allowed(guild, Some(message.channel_id.get()), Module::Starboard)
        .await
    {
        return;
    }
    let app = app.clone();
    let msg = message.clone();
    app.clone().spawn(async move {
        if let Err(error) = react_vote(&app, &msg).await {
            tracing::warn!(guild, %error, "vote channel reaction failed");
        }
    });
}

async fn react_vote(app: &App, message: &Message) -> anyhow::Result<()> {
    let Some(vote) = app.db.vote(message.channel_id.get()).await? else {
        return Ok(());
    };
    if vote.only_media && (!app.message_content || !has_image(message) && message.attachments.is_empty()) {
        return Ok(());
    }
    let channel = message.channel_id;
    let id = message.id;
    app.http
        .create_reaction(channel, id, &reaction_type(&vote.up_emoji))
        .await?;
    app.http
        .create_reaction(channel, id, &reaction_type(&vote.down_emoji))
        .await?;
    if vote.create_thread {
        let lang = app.guild_lang(vote.guild_id as u64).await;
        app.http
            .create_thread_from_message(channel, id, &t!(lang, "starboard.vote.thread"))
            .await?;
    }
    Ok(())
}

pub(crate) async fn on_update(app: &Arc<App>, event: &MessageUpdate) {
    let Some(guild) = event.0.guild_id.map(|g| g.get()) else {
        return;
    };
    let app = app.clone();
    let message = event.0.clone();
    app.clone().spawn(async move {
        if let Err(error) = refresh(&app, guild, message.id.get()).await {
            tracing::warn!(%error, "starboard edit failed");
        }
    });
}

async fn refresh(app: &App, guild: u64, message: u64) -> anyhow::Result<()> {
    let posts = app.db.star_posts_for_source(guild, message).await?;
    let Some(first) = posts.first() else {
        return Ok(());
    };
    let original = app
        .http
        .message(Id::new(first.channel_id as u64), Id::new(message))
        .await?
        .model()
        .await?;
    for post in posts {
        if let Some(board) = app.db.starboard(post.starboard_id).await? {
            if board.require_image && !has_image(&original) {
                if let Some(copy) = post.board_message_id.filter(|v| *v > 0) {
                    let _ = app
                        .http
                        .delete_message(Id::new(board.channel_id as u64), Id::new(copy as u64))
                        .await;
                    app.db.clear_star_copy(board.id, message, copy as u64).await?;
                }
            } else {
                sync_post(app, &board, &post, Some(&original)).await?;
            }
        }
    }
    Ok(())
}

pub(crate) async fn on_delete(app: &Arc<App>, event: &MessageDelete) {
    if let Some(guild) = event.guild_id {
        delete_source(app, guild.get(), event.id.get()).await;
    }
}

pub(crate) async fn on_delete_bulk(app: &Arc<App>, event: &MessageDeleteBulk) {
    if let Some(guild) = event.guild_id {
        for message in &event.ids {
            delete_source(app, guild.get(), message.get()).await;
        }
    }
}

async fn delete_source(app: &Arc<App>, guild: u64, message: u64) {
    let app = app.clone();
    app.clone().spawn(async move {
        match app.db.delete_star_source(guild, message).await {
            Ok(posts) => {
                for post in posts {
                    if let Some(copy) = post.board_message_id.filter(|v| *v > 0)
                        && let Ok(Some(board)) = app.db.starboard(post.starboard_id).await
                    {
                        let _ = app
                            .http
                            .delete_message(Id::new(board.channel_id as u64), Id::new(copy as u64))
                            .await;
                    }
                }
            }
            Err(error) => tracing::warn!(%error, "starboard source delete failed"),
        }
    });
}

pub fn spawn_ticker(app: Arc<App>) {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(86_400));
        loop {
            interval.tick().await;
            if let Err(error) = app.db.sweep_star_posts().await {
                tracing::warn!(%error, "starboard sweep failed");
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn board() -> Starboard {
        Starboard {
            id: 1,
            guild_id: 1,
            name: "Highlights".into(),
            channel_id: 2,
            emoji: "⭐".into(),
            threshold: 3,
            self_star: false,
            allow_bots: false,
            only_channels: vec![3],
            except_channels: vec![4],
            require_image: false,
            color: 0xffc857,
            role_weights: serde_json::json!({"5": 2, "6": 4}),
            remove_below: true,
            enabled: true,
        }
    }

    #[test]
    fn emoji_and_channel_rules_match_exactly() {
        let board = board();
        assert!(emoji_matches(
            &board.emoji,
            &EmojiReactionType::Unicode { name: "⭐".into() }
        ));
        assert!(!emoji_matches(
            &board.emoji,
            &EmojiReactionType::Unicode { name: "🌟".into() }
        ));
        assert!(eligible(&board, 3));
        assert!(!eligible(&board, 4));
        assert!(!eligible(&board, 2));
    }

    #[test]
    fn nsfw_sources_only_enter_nsfw_boards() {
        assert!(!nsfw_compatible(Some(true), None, Some(false)));
        assert!(nsfw_compatible(Some(true), None, Some(true)));
        assert!(!nsfw_compatible(None, Some(true), Some(false)));
        assert!(!nsfw_compatible(Some(false), Some(true), Some(false)));
        assert!(!nsfw_compatible(None, None, Some(false)));
    }

    #[test]
    fn highest_held_role_weight_wins() {
        let board = board();
        assert_eq!(weight(&board, &[]), 1);
        assert_eq!(weight(&board, &[Id::new(5)]), 2);
        assert_eq!(weight(&board, &[Id::new(5), Id::new(6)]), 4);
    }
}
