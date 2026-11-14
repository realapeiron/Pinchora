use super::*;

/// The role is only handed out to a verified owner and only while it passes the shared role rule.
pub async fn change_live_role(app: &App, feed: &FeedRow, add: bool) -> bool {
    let user = match app.db.linked_streamer(&feed.kind, &feed.source).await {
        Ok(Some(user)) => user,
        Ok(None) => return true,
        Err(_) => return false,
    };
    let Ok(_guard) = app.db.lock_feed_link(user).await else {
        return false;
    };
    if app.db.linked_streamer(&feed.kind, &feed.source).await.ok().flatten() != Some(user) {
        return true;
    }
    change_live_role_for(app, feed, user, add).await
}

/// Caller holds the member's link lock, so unlink cannot race a role grant.
pub async fn change_live_role_for(app: &App, feed: &FeedRow, user: u64, add: bool) -> bool {
    if add
        && (!app.module_enabled(feed.guild_id as u64, Module::Feeds).await
            || !app
                .db
                .feed_channel_is_live(&feed.kind, &feed.source)
                .await
                .unwrap_or(false)
            || app
                .db
                .feed(feed.guild_id as u64, feed.id)
                .await
                .ok()
                .flatten()
                .is_none_or(|current| !current.enabled || current.live_role_id != feed.live_role_id))
    {
        return false;
    }
    let Some(role) = feed.live_role_id.map(|id| id as u64) else {
        return true;
    };
    let guild = feed.guild_id as u64;
    let lang = app.guild_lang(guild).await;
    if !add {
        return match app
            .http
            .remove_guild_member_role(Id::new(guild), Id::new(user), Id::new(role))
            .reason(&t!(lang, "feeds.live.role-removed"))
            .await
        {
            Ok(_) => true,
            Err(error) => matches!(error_code(&error), Some(10007 | 10011 | 10013)),
        };
    }
    let Ok(roles) = app.http.roles(Id::new(guild)).await else {
        return false;
    };
    let Ok(roles) = roles.models().await else {
        return false;
    };
    let Some(target) = roles.iter().find(|r| r.id.get() == role) else {
        return false;
    };
    let Ok(me) = app
        .http
        .guild_member(Id::new(guild), Id::new(app.application_id.get()))
        .await
    else {
        return false;
    };
    let Ok(me) = me.model().await else {
        return false;
    };
    let top = roles
        .iter()
        .filter(|r| me.roles.contains(&r.id))
        .map(|r| r.position)
        .max()
        .unwrap_or(0);
    let facts = pinchora_core::admin::settings::RoleFacts {
        everyone: role == guild,
        managed: target.managed,
        position: target.position,
        permissions: target.permissions.bits(),
    };
    if pinchora_core::admin::settings::role_refusal(facts, Some(top), "Live now").is_some()
        || pinchora_core::admin::settings::run_time_refusal(facts.permissions, "Live now").is_some()
    {
        return false;
    }
    let request = app
        .http
        .add_guild_member_role(Id::new(guild), Id::new(user), Id::new(role))
        .reason(&t!(lang, "feeds.live.role-added"))
        .await;
    if let Err(error) = request {
        tracing::warn!(%error, guild, user, role, "live role update failed");
        return false;
    }
    true
}

/// Remove a granted role before its configuration is erased; retain the member lock through the write.
pub async fn before_live_change(
    app: &App,
    feed: Option<&FeedRow>,
    edit: Option<&pinchora_db::FeedEdit>,
) -> anyhow::Result<impl Send> {
    let Some(feed) = feed.filter(|f| f.live_message_id.is_some()) else {
        return Ok(None);
    };
    let changes_role = edit.is_none_or(|edit| {
        edit.enabled == Some(false)
            || edit.channel.is_some_and(|id| id as i64 != feed.channel_id)
            || edit
                .live_role
                .is_some_and(|id| id.map(|id| id as i64) != feed.live_role_id)
    });
    if !changes_role {
        return Ok(None);
    }
    let Some(user) = app.db.linked_streamer(&feed.kind, &feed.source).await? else {
        return Ok(None);
    };
    let guard = app.db.lock_feed_link(user).await?;
    if app.db.linked_streamer(&feed.kind, &feed.source).await? != Some(user) {
        anyhow::bail!("channel owner changed during cleanup");
    }
    let current = app.db.feed(feed.guild_id as u64, feed.id).await?;
    if let Some(current) = current.as_ref()
        && !change_live_role_for(app, current, user, false).await
    {
        anyhow::bail!("live role cleanup failed");
    }
    Ok(Some(guard))
}

/// Keep role grants stopped until privacy deletion has erased the member's links.
pub(crate) async fn before_delete(app: &App, user: u64) -> anyhow::Result<impl Send> {
    let guard = app.db.lock_feed_link(user).await?;
    for link in app.db.channel_links(user).await? {
        if link.verified_at.is_none() {
            continue;
        }
        for feed in app.db.live_feeds_for_channel(&link.kind, &link.source).await? {
            if !change_live_role_for(app, &feed, user, false).await {
                anyhow::bail!("live role cleanup failed");
            }
        }
    }
    Ok(guard)
}

pub async fn reconcile_live_edit(app: &App, before: &FeedRow, after: &FeedRow) {
    if before.live_message_id.is_none() {
        return;
    }
    if !after.enabled || before.channel_id != after.channel_id {
        end_live(app, before).await;
    } else if before.live_role_id != after.live_role_id {
        change_live_role(app, before, false).await;
        change_live_role(app, after, true).await;
    }
}

pub async fn end_live(app: &App, feed: &FeedRow) {
    let Some(id) = feed.live_message_id.map(|id| id as u64) else {
        return;
    };
    let lang = app.guild_lang(feed.guild_id as u64).await;
    let kind = Kind::parse(&feed.kind).unwrap_or(Kind::Twitch);
    let vod = format!(
        "{}/videos",
        pinchora_core::community::feeds::page_url(kind, &feed.source).unwrap_or_default()
    );
    let embed = twilight_util::builder::embed::EmbedBuilder::new()
        .title(t!(lang, "feeds.live.ended"))
        .url(vod)
        .color(0x6b7280)
        .build();
    let embed = app.brand_embed(feed.guild_id as u64, embed).await;
    let body = serde_json::to_vec(&serde_json::json!({
        "content": "", "embeds": [embed], "components": [], "allowed_mentions": {"parse": []}
    }))
    .expect("ended payload");
    let edited = app
        .http
        .update_message(Id::new(feed.channel_id as u64), Id::new(id))
        .payload_json(&body)
        .await;
    let removed = change_live_role(app, feed, false).await;
    if !removed {
        return;
    }
    if let Err(error) = edited {
        tracing::warn!(%error, feed = feed.id, "live message edit failed");
        return;
    }
    if let Err(error) = app.db.clear_live_message(feed.id, id).await {
        tracing::warn!(%error, feed = feed.id, "live message id was not cleared");
    }
}

pub(super) fn delivery_nonce(feed: i64, item: &str) -> u64 {
    let mut hash = Sha256::new();
    hash.update(feed.to_be_bytes());
    hash.update(item.as_bytes());
    u64::from_be_bytes(hash.finalize()[..8].try_into().expect("SHA-256 has eight bytes"))
}

pub(super) fn delivery_payload(
    message: &FeedMessage,
    embeds: &[twilight_model::channel::message::Embed],
    mentions: &AllowedMentions,
    nonce: Option<u64>,
) -> Vec<u8> {
    let mut payload = serde_json::json!({
        "content": message.content,
        "embeds": embeds,
        "allowed_mentions": mentions,
    });
    if let Some(nonce) = nonce {
        payload["nonce"] = serde_json::json!(nonce.to_string());
        payload["enforce_nonce"] = serde_json::json!(true);
    }
    serde_json::to_vec(&payload).expect("message payload is serializable")
}
