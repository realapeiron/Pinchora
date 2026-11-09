//! Reaction-driven starboards and vote channels.

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

static VOTE_SLOTS: LazyLock<Arc<Semaphore>> = LazyLock::new(|| Arc::new(Semaphore::new(32)));

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
    let view = crate::infra::text::access::channel_permissions(
        base,
        guild,
        guild,
        &[],
        channel.permission_overwrites.as_deref().unwrap_or(&[]),
    )
    .contains(Permissions::VIEW_CHANNEL);
    view && !channel
        .permission_overwrites
        .as_deref()
        .unwrap_or(&[])
        .iter()
        .any(|overwrite| overwrite.deny.contains(Permissions::VIEW_CHANNEL))
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
        everyone_visible(guild, &roles, parent) && everyone_visible(guild, &roles, source)
    } else {
        everyone_visible(guild, &roles, source)
    };
    Ok(source_public)
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
}

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
    let Ok(permit) = VOTE_SLOTS.clone().try_acquire_owned() else {
        tracing::warn!(guild, "vote channel queue full");
        return;
    };
    let app = app.clone();
    let msg = message.clone();
    app.clone().spawn(async move {
        let _permit = permit;
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
        if app
            .cache
            .try_cooldown(
                &format!("vote-thread-user:{}:{}", vote.channel_id, message.author.id),
                Duration::from_secs(30),
            )
            .await?
            .is_some()
            || app
                .cache
                .try_cooldown(
                    &format!("vote-thread-channel:{}", vote.channel_id),
                    Duration::from_secs(6),
                )
                .await?
                .is_some()
        {
            return Ok(());
        }
        let lang = app.guild_lang(vote.guild_id as u64).await;
        app.http
            .create_thread_from_message(channel, id, &t!(lang, "starboard.vote.thread"))
            .await?;
    }
    Ok(())
}

pub async fn on_update(app: &Arc<App>, event: &MessageUpdate) {
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
    for post in posts {
        app.db
            .touch_star_post(
                post.starboard_id,
                message,
                post.channel_id as u64,
                post.author_id as u64,
            )
            .await?;
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
            Ok(_) => {}
            Err(error) => tracing::warn!(%error, "starboard source delete failed"),
        }
    });
}

pub fn spawn_ticker(app: Arc<App>) {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(2));
        let mut sweeps = 0_u64;
        loop {
            interval.tick().await;
            if let Err(error) = tick_with_periodic(&app, sweeps.is_multiple_of(30)).await {
                tracing::warn!(%error, "starboard reconciliation failed");
            }
            sweeps += 1;
            if sweeps.is_multiple_of(43_200)
                && let Err(error) = app.db.sweep_star_posts().await
            {
                tracing::warn!(%error, "starboard sweep failed");
            }
        }
    });
}

pub async fn tick(app: &App) -> anyhow::Result<()> {
    tick_with_periodic(app, true).await
}

async fn tick_with_periodic(app: &App, periodic: bool) -> anyhow::Result<()> {
    for (channel, copy) in app.db.due_star_cleanups().await? {
        cleanup_one(app, channel, copy).await?;
    }
    for (board_id, message, token, force) in app.db.due_star_posts(100, periodic).await? {
        let version = app.db.star_sync_version(board_id, message as u64).await?;
        let result = async {
            let Some(board) = app.db.starboard(board_id).await? else {
                return Ok(());
            };
            let Some(post) = app.db.star_post(board_id, message as u64).await? else {
                return Ok(());
            };
            reconcile_post(app, &board, &post, force).await
        }
        .await;
        if let Some(version) = version {
            app.db
                .finish_star_sync(board_id, message as u64, token, version, result.is_ok())
                .await?;
        }
        if let Err(error) = result {
            tracing::warn!(board_id, message, %error, "starboard post sync will retry");
        }
    }
    Ok(())
}

pub async fn cleanup_one(app: &App, channel: i64, copy: i64) -> anyhow::Result<()> {
    if copy < 0 {
        let source = (-copy) as u64;
        let destination = Id::new(channel as u64);
        let mut before = None;
        for _ in 0..20 {
            let response = match before {
                Some(id) => app.http.channel_messages(destination).before(id).limit(100).await,
                None => app.http.channel_messages(destination).limit(100).await,
            };
            let messages = match response {
                Ok(response) => response.models().await?,
                Err(error) if unknown_message(&error) => {
                    app.db.finish_star_cleanup(channel, copy).await?;
                    return Ok(());
                }
                Err(error) => return Err(error.into()),
            };
            if let Some(found) = messages.iter().find(|message| {
                message.author.id.get() == app.application_id.get()
                    && message.embeds.iter().any(|embed| {
                        embed
                            .description
                            .as_deref()
                            .is_some_and(|description| description.ends_with(&format!("/{source}")))
                    })
            }) {
                if !app
                    .db
                    .recover_ambiguous_star_post(channel as u64, source, found.id.get())
                    .await?
                {
                    app.db.queue_star_cleanup(channel as u64, found.id.get()).await?;
                }
                app.db.finish_star_cleanup(channel, copy).await?;
                return Ok(());
            }
            if messages.len() < 100 {
                break;
            }
            before = messages.last().map(|message| message.id);
        }
        return Ok(());
    }
    match app
        .http
        .delete_message(Id::new(channel as u64), Id::new(copy as u64))
        .await
    {
        Ok(_) => app.db.finish_star_cleanup(channel, copy).await?,
        Err(error) if unknown_message(&error) => app.db.finish_star_cleanup(channel, copy).await?,
        Err(error) => tracing::warn!(channel, copy, %error, "starboard copy delete will retry"),
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::community::starboard::*;

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
