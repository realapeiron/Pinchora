use super::*;

pub(super) async fn voice(cx: &Cx, c: Voice) -> anyhow::Result<()> {
    let (p, label) = match c.action.as_str() {
        "mute" | "unmute" => (Permissions::MUTE_MEMBERS, "mute-members"),
        "deafen" | "undeafen" => (Permissions::DEAFEN_MEMBERS, "deafen-members"),
        "move" | "disconnect" | "ban" | "unban" => (Permissions::MOVE_MEMBERS, "move-members"),
        _ => return invalid(cx).await,
    };
    let Some(g) = permitted(cx, p, label).await? else {
        return Ok(());
    };
    for channel in [&c.source, &c.destination].into_iter().flatten() {
        if !channel.permissions.contains(p) && !channel.permissions.contains(Permissions::ADMINISTRATOR) {
            return invalid(cx).await;
        }
        if !matches!(
            channel.kind,
            twilight_model::channel::ChannelType::GuildVoice | twilight_model::channel::ChannelType::GuildStageVoice
        ) {
            return invalid(cx).await;
        }
    }
    if c.action == "move" && c.destination.is_none() {
        return invalid(cx).await;
    }
    let actor = Moderator::of(cx)?;
    let Ok((ranks, _)) = fresh_ranks(&cx.app, g, &actor, p).await else {
        return invalid(cx).await;
    };
    let access = voice_access(cx, g, actor.id, &ranks, p).await?;
    if [&c.source, &c.destination]
        .into_iter()
        .flatten()
        .any(|ch| !access.can_use(ch.id))
    {
        return invalid(cx).await;
    }
    let presence = cx.app.cache.map_all(&format!("modparity:voice-v2:{g}")).await?;
    let users = if let Some(u) = c.user.as_ref() {
        vec![u.resolved.id.get()]
    } else {
        let Some(source) = c.source.as_ref() else {
            return invalid(cx).await;
        };
        if c.confirm != Some(true) {
            return invalid(cx).await;
        }
        cx.app
            .cache
            .map_all(&format!("modparity:voice-v2:{g}"))
            .await?
            .iter()
            .filter(|(_, value)| {
                crate::moderation::policy::voice_channel(value).as_deref() == Some(source.id.to_string().as_str())
            })
            .filter_map(|(id, _)| id.parse().ok())
            .collect()
    };
    if users.len() > 100 {
        return invalid(cx).await;
    }
    if users.len() > 1 && !batch_slot(cx, g).await? {
        return Ok(());
    }
    apply_voice_actions(
        cx,
        g,
        &c,
        &actor,
        &ranks,
        VoiceSelection {
            users,
            presence,
            access,
        },
    )
    .await
}

struct VoiceAccess {
    guild: u64,
    actor: u64,
    held: Vec<u64>,
    base: Permissions,
    channels: Vec<twilight_model::channel::Channel>,
    permission: Permissions,
}
impl VoiceAccess {
    fn can_use(&self, id: Id<twilight_model::id::marker::ChannelMarker>) -> bool {
        let Self {
            guild: g,
            actor,
            held,
            base,
            channels,
            permission: p,
        } = self;

        channels.iter().find(|ch| ch.id == id).is_some_and(|ch| {
            crate::infra::text::access::channel_permissions(
                *base,
                *g,
                *actor,
                held,
                ch.permission_overwrites.as_deref().unwrap_or_default(),
            )
            .contains(*p | Permissions::VIEW_CHANNEL)
        })
    }
}
async fn voice_access(cx: &Cx, g: u64, actor: u64, ranks: &Ranks, p: Permissions) -> anyhow::Result<VoiceAccess> {
    let current = cx
        .app
        .http
        .guild_member(Id::new(g), Id::new(actor))
        .await?
        .model()
        .await?;
    let held: Vec<_> = current.roles.iter().map(|r| r.get()).collect();
    let base = if actor == ranks.owner {
        Permissions::ADMINISTRATOR
    } else {
        held.iter()
            .copied()
            .chain([g])
            .filter_map(|id| ranks.table.get(&id))
            .fold(Permissions::empty(), |p, r| p | r.permissions)
    };
    let channels = cx.app.http.guild_channels(Id::new(g)).await?.models().await?;
    Ok(VoiceAccess {
        guild: g,
        actor,
        held,
        base,
        channels,
        permission: p,
    })
}

struct VoiceSelection {
    users: Vec<u64>,
    presence: Vec<(String, String)>,
    access: VoiceAccess,
}
async fn apply_voice_actions(
    cx: &Cx,
    g: u64,
    c: &Voice,
    actor: &Moderator,
    ranks: &Ranks,
    selection: VoiceSelection,
) -> anyhow::Result<()> {
    let VoiceSelection {
        users,
        presence,
        access,
    } = selection;
    let mut ok = 0;
    let mut failed = Vec::new();
    let total = users.len();
    for uid in users {
        if let Some(source) = presence
            .iter()
            .find(|(id, _)| id == &uid.to_string())
            .and_then(|(_, value)| crate::moderation::policy::voice_channel(value))
            .and_then(|id| id.parse::<u64>().ok())
            && !access.can_use(Id::new(source))
        {
            failed.push(uid);
            continue;
        }
        let Some(rank) = ranks.of(&cx.app, g, uid).await else {
            failed.push(uid);
            continue;
        };
        if may_act(ranks.owner, ranks.actor, ranks.bot, rank).is_err()
            || crate::moderation::anti_nuke::vip_refusal(&cx.app, g, uid)
                .await?
                .is_some()
        {
            failed.push(uid);
            continue;
        }
        if c.action == "ban" {
            sqlx::query("INSERT INTO mod_voice_bans(guild_id,user_id) VALUES($1,$2) ON CONFLICT DO NOTHING")
                .bind(g as i64)
                .bind(uid as i64)
                .execute(cx.app.db.pool())
                .await?;
        }
        if c.action == "unban" {
            sqlx::query("DELETE FROM mod_voice_bans WHERE guild_id=$1 AND user_id=$2")
                .bind(g as i64)
                .bind(uid as i64)
                .execute(cx.app.db.pool())
                .await?;
        } else {
            let req = cx.app.http.update_guild_member(Id::new(g), Id::new(uid));
            let req = match c.action.as_str() {
                "move" => {
                    let Some(d) = c.destination.as_ref() else {
                        return invalid(cx).await;
                    };
                    req.channel_id(Some(d.id))
                }
                "disconnect" | "ban" => req.channel_id(None),
                "mute" => req.mute(true),
                "unmute" => req.mute(false),
                "deafen" => req.deaf(true),
                _ => req.deaf(false),
            };
            if req
                .reason(&members::audit_reason(cx.lang(), actor, c.reason.as_deref()))
                .await
                .is_err()
            {
                failed.push(uid);
                continue;
            }
        }
        log(
            &cx.app,
            g,
            (actor.id, &actor.name),
            CaseKind::Voice,
            uid,
            c.reason.clone(),
            Some(c.action.clone()),
        )
        .await?;
        ok += 1;
    }
    batch_reply(cx, ok, total, &failed).await
}
