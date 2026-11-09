use super::*;

pub(super) async fn show_user(cx: &Cx, guild: u64, c: InfoUser) -> anyhow::Result<()> {
    let gid = Id::<GuildMarker>::new(guild);
    let user = c.user.map_or_else(|| cx.author().cloned(), |r| Ok(r.resolved))?;
    let member = cx.app.http.guild_member(gid, user.id).await?.model().await?;
    user_card(cx, guild, user, serde_json::to_value(member)?, 0, false).await
}

pub(super) async fn show_server(cx: &Cx, guild: u64, members: bool) -> anyhow::Result<()> {
    let gid = Id::<GuildMarker>::new(guild);
    let data = serde_json::to_value(cx.app.http.guild(gid).with_counts(true).await?.model().await?)?;
    let rows = if members {
        vec![
            row(t!(cx.lang(), "info.members"), field(&data, "approximate_member_count")),
            row(
                t!(cx.lang(), "info.pinchora-members"),
                cx.app.db.guild_member_count(guild).await?.to_string(),
            ),
        ]
    } else {
        vec![
            row(t!(cx.lang(), "info.owner"), field(&data, "owner_id")),
            row(t!(cx.lang(), "info.boost"), field(&data, "premium_subscription_count")),
            row(
                t!(cx.lang(), "info.pinchora-members"),
                cx.app.db.guild_member_count(guild).await?.to_string(),
            ),
            row(t!(cx.lang(), "info.members"), field(&data, "approximate_member_count")),
            row(
                t!(cx.lang(), "info.roles"),
                data.get("roles")
                    .and_then(Value::as_array)
                    .map_or(0, Vec::len)
                    .to_string(),
            ),
            row(
                t!(cx.lang(), "info.created"),
                chrono::DateTime::<chrono::Utc>::from_timestamp_millis(((guild >> 22) + 1_420_070_400_000) as i64)
                    .map_or_else(|| "—".into(), |d| d.format("%Y-%m-%d").to_string()),
            ),
        ]
    };
    let icon = match data.get("icon").and_then(Value::as_str) {
        Some(hash) => {
            thumbnail(
                cx,
                &format!("https://cdn.discordapp.com/icons/{guild}/{hash}.png?size=256"),
            )
            .await
        }
        None => None,
    }
    .unwrap_or_else(|| pinchora_render::placeholder_avatar(guild, 208));
    send(
        cx,
        field(&data, "name"),
        if members {
            t!(cx.lang(), "info.members")
        } else {
            t!(cx.lang(), "info.server")
        },
        rows,
        Some(icon),
        None,
        false,
    )
    .await
}

pub(super) async fn show_avatar(cx: &Cx, guild: u64, c: InfoAvatar) -> anyhow::Result<()> {
    let gid = Id::<GuildMarker>::new(guild);
    let user = c.user.map_or_else(|| cx.author().cloned(), |r| Ok(r.resolved))?;
    let mut avatar = user
        .avatar
        .map(|h| format!("https://cdn.discordapp.com/avatars/{}/{}.png?size=1024", user.id, h));
    if c.server == Some(true) {
        let member = serde_json::to_value(cx.app.http.guild_member(gid, user.id).await?.model().await?)?;
        if let Some(hash) = member.get("avatar").and_then(Value::as_str) {
            avatar = Some(format!(
                "https://cdn.discordapp.com/guilds/{guild}/users/{}/avatars/{hash}.png?size=1024",
                user.id
            ));
        }
    }
    let picture = if c.server == Some(true) {
        match avatar.as_deref() {
            Some(url) if url.contains("/guilds/") => thumbnail(cx, url).await,
            _ => None,
        }
    } else {
        None
    };
    let picture = match picture {
        Some(png) => png,
        None => (*cx.app.avatar(&user, 160, Some(40)).await).clone(),
    };
    send(
        cx,
        user.name,
        t!(cx.lang(), "info.avatar"),
        vec![row(
            t!(cx.lang(), "info.image-url"),
            avatar.unwrap_or_else(|| t!(cx.lang(), "info.default-avatar")),
        )],
        Some(picture),
        None,
        false,
    )
    .await
}

pub(super) async fn show_banner(cx: &Cx, c: InfoBanner) -> anyhow::Result<()> {
    let user = c.user.map_or_else(|| cx.author().cloned(), |r| Ok(r.resolved))?;
    let data = serde_json::to_value(cx.app.http.user(user.id).await?.model().await?)?;
    let url = data
        .get("banner")
        .and_then(Value::as_str)
        .map(|hash| format!("https://cdn.discordapp.com/banners/{}/{hash}.png?size=1024", user.id));
    let picture = match url.as_deref() {
        Some(url) => thumbnail(cx, url).await,
        None => None,
    };
    send(
        cx,
        user.name,
        t!(cx.lang(), "info.banner"),
        vec![row(
            t!(cx.lang(), "info.image-url"),
            url.unwrap_or_else(|| t!(cx.lang(), "info.no-banner")),
        )],
        picture,
        None,
        false,
    )
    .await
}

pub(super) async fn show_role(cx: &Cx, c: InfoRole) -> anyhow::Result<()> {
    let data = serde_json::to_value(&c.role)?;
    send(
        cx,
        c.role.name.clone(),
        t!(cx.lang(), "info.role"),
        vec![
            row(t!(cx.lang(), "info.id"), c.role.id.to_string()),
            row(
                t!(cx.lang(), "info.color"),
                format!(
                    "#{:06X}",
                    data.get("colors")
                        .and_then(|v| v.get("primary_color"))
                        .and_then(Value::as_u64)
                        .or_else(|| data.get("color").and_then(Value::as_u64))
                        .unwrap_or(0)
                ),
            ),
            row(t!(cx.lang(), "info.position"), field(&data, "position")),
            row(
                t!(cx.lang(), "info.mentionable"),
                if c.role.mentionable {
                    t!(cx.lang(), "info.yes")
                } else {
                    t!(cx.lang(), "info.no")
                },
            ),
            row(
                t!(cx.lang(), "info.hoisted"),
                if c.role.hoist {
                    t!(cx.lang(), "info.yes")
                } else {
                    t!(cx.lang(), "info.no")
                },
            ),
        ],
        None,
        None,
        false,
    )
    .await
}

pub(super) async fn show_channel(cx: &Cx, guild: u64, c: InfoChannel) -> anyhow::Result<()> {
    let channel = c
        .channel
        .map(|c| c.id)
        .or_else(|| cx.interaction.channel.as_ref().map(|c| c.id));
    let Some(channel) = channel else {
        return cx.reply_ephemeral(t!(cx.lang(), "info.unavailable")).await;
    };
    let found = cx.app.http.channel(channel).await?.model().await?;
    if !visible_channel(cx, guild, &found).await? {
        return cx.reply_ephemeral(t!(cx.lang(), "info.unavailable")).await;
    }
    let mut data = serde_json::to_value(&found)?;
    // Even the category ID is withheld when its own overwrites hide it.
    if let Some(parent) = found.parent_id {
        let parent = cx.app.http.channel(parent).await?.model().await?;
        if !visible_channel(cx, guild, &parent).await? {
            data["parent_id"] = Value::Null;
        } else {
            data["parent_id"] = parent.name.map(Value::String).unwrap_or(Value::Null);
        }
    }
    send(
        cx,
        field(&data, "name"),
        t!(cx.lang(), "info.channel"),
        vec![
            row(t!(cx.lang(), "info.id"), channel.to_string()),
            row(t!(cx.lang(), "info.type"), channel_kind(cx.lang(), found.kind)),
            row(t!(cx.lang(), "info.parent"), field(&data, "parent_id")),
            row(t!(cx.lang(), "info.topic"), field(&data, "topic")),
        ],
        None,
        None,
        false,
    )
    .await
}

pub(super) async fn show_emoji(cx: &Cx, guild: u64, c: InfoEmoji) -> anyhow::Result<()> {
    let gid = Id::<GuildMarker>::new(guild);
    let Some(emoji) = c.emoji else {
        let server = cx.app.http.guild(gid).await?.model().await?;
        let animated = server.emojis.iter().filter(|e| e.animated).count();
        return send(
            cx,
            server.name,
            t!(cx.lang(), "info.emoji"),
            vec![
                row(t!(cx.lang(), "info.emoji"), server.emojis.len().to_string()),
                row(t!(cx.lang(), "info.animated"), animated.to_string()),
            ],
            None,
            None,
            false,
        )
        .await;
    };
    let id = emoji
        .split(':')
        .next_back()
        .unwrap_or("")
        .trim_end_matches('>')
        .parse::<u64>();
    let Ok(id) = id else {
        return send(
            cx,
            emoji.clone(),
            t!(cx.lang(), "info.emoji"),
            vec![row(t!(cx.lang(), "info.unicode"), emoji)],
            None,
            None,
            false,
        )
        .await;
    };
    if id == 0 {
        return cx.reply_ephemeral(t!(cx.lang(), "info.unavailable")).await;
    }
    let data = serde_json::to_value(
        cx.app
            .http
            .emoji(gid, Id::<EmojiMarker>::new(id))
            .await?
            .model()
            .await?,
    )?;
    send(
        cx,
        field(&data, "name"),
        t!(cx.lang(), "info.emoji"),
        vec![
            row(t!(cx.lang(), "info.id"), id.to_string()),
            row(
                t!(cx.lang(), "info.animated"),
                if data.get("animated").and_then(Value::as_bool).unwrap_or(false) {
                    t!(cx.lang(), "info.yes")
                } else {
                    t!(cx.lang(), "info.no")
                },
            ),
        ],
        None,
        None,
        false,
    )
    .await
}

pub(super) async fn show_invite(cx: &Cx, guild: u64, c: InfoInvite) -> anyhow::Result<()> {
    let data = serde_json::to_value(cx.app.http.invite(&c.code).await?.model().await?)?;
    if data.get("guild").and_then(|v| v.get("id")).and_then(Value::as_str) != Some(&guild.to_string()) {
        return cx.reply_ephemeral(t!(cx.lang(), "info.invite-other")).await;
    }
    let channel = data
        .get("channel")
        .and_then(|v| v.get("id"))
        .and_then(Value::as_str)
        .and_then(|v| v.parse::<u64>().ok())
        .filter(|id| *id > 0);
    let Some(channel) = channel else {
        return cx.reply_ephemeral(t!(cx.lang(), "info.unavailable")).await;
    };
    let channel = cx
        .app
        .http
        .channel(Id::<ChannelMarker>::new(channel))
        .await?
        .model()
        .await?;
    if !visible_channel(cx, guild, &channel).await? {
        return cx.reply_ephemeral(t!(cx.lang(), "info.unavailable")).await;
    }
    send(
        cx,
        c.code,
        t!(cx.lang(), "info.invite"),
        vec![
            row(
                t!(cx.lang(), "info.channel"),
                data.get("channel").map_or("—".into(), |v| field(v, "name")),
            ),
            row(t!(cx.lang(), "info.uses"), field(&data, "uses")),
            row(t!(cx.lang(), "info.max-uses"), field(&data, "max_uses")),
            row(t!(cx.lang(), "info.expires"), field(&data, "expires_at")),
        ],
        None,
        None,
        false,
    )
    .await
}
