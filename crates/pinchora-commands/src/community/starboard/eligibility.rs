use super::*;

pub(super) fn emoji_matches(configured: &str, reaction: &EmojiReactionType) -> bool {
    match reaction {
        EmojiReactionType::Unicode { name } => configured.replace('\u{fe0f}', "") == name.replace('\u{fe0f}', ""),
        EmojiReactionType::Custom { id, .. } => configured.contains(&format!(":{}>", id.get())),
    }
}

pub(super) fn reaction_type(emoji: &str) -> RequestReactionType<'_> {
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

pub(super) fn eligible(board: &Starboard, channel: u64) -> bool {
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

pub(super) fn nsfw_compatible(source: Option<bool>, parent: Option<bool>, target: Option<bool>) -> bool {
    let is_nsfw = if source.is_none() && parent.is_none() {
        true
    } else {
        source.unwrap_or(false) || parent.unwrap_or(false)
    };
    !is_nsfw || target.unwrap_or(false)
}

pub(super) async fn can_copy(app: &App, board: &Starboard, source: &Channel) -> anyhow::Result<bool> {
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

pub(super) fn has_image(message: &Message) -> bool {
    message
        .attachments
        .iter()
        .any(|a| a.content_type.as_deref().is_some_and(|t| t.starts_with("image/")))
        || !message
            .embeds
            .iter()
            .all(|e| e.image.is_none() && e.thumbnail.is_none())
}

pub(super) fn weight(board: &Starboard, roles: &[Id<twilight_model::id::marker::RoleMarker>]) -> i32 {
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

pub(super) struct Who {
    pub(super) guild: u64,
    pub(super) channel: u64,
    pub(super) message: u64,
}
