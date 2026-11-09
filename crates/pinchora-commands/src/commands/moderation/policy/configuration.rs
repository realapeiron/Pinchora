use super::*;

/// Fresh role safety checks, shared by slash and dashboard policy writes.
pub async fn check_config(app: &App, g: u64, actor: u64, c: &Config) -> anyhow::Result<()> {
    anyhow::ensure!(c.valid(), "invalid moderation policy");
    let member = app.http.guild_member(Id::new(g), Id::new(actor)).await?.model().await?;
    let table = app.http.roles(Id::new(g)).await?.models().await?;
    let bot = app
        .http
        .guild_member(Id::new(g), Id::new(app.application_id.get()))
        .await?
        .model()
        .await?;
    let owner = app.http.guild(Id::new(g)).await?.model().await?.owner_id.get();
    let top = |held: &[Id<twilight_model::id::marker::RoleMarker>]| {
        table
            .iter()
            .filter(|r| held.contains(&r.id))
            .map(|r| r.position)
            .max()
            .unwrap_or(0)
    };
    let permissions = table
        .iter()
        .filter(|r| r.id.get() == g || member.roles.contains(&r.id))
        .fold(Permissions::empty(), |p, r| p | r.permissions);
    let admin = permissions.contains(Permissions::ADMINISTRATOR);
    let mut needed = Permissions::MANAGE_GUILD;
    let previous = app.db.moderation_policy_config(g).await?;
    for policy in [c, &previous] {
        if policy.mute_role.is_some()
            || policy.jail_role.is_some()
            || policy.booster_role.is_some()
            || policy.booster_personal
            || !policy.conditions.is_empty()
        {
            needed |= Permissions::MANAGE_ROLES;
        }
        if !policy.autopurge.is_empty() {
            needed |= Permissions::MANAGE_MESSAGES;
        }
    }
    anyhow::ensure!(owner == actor || admin || permissions.contains(needed), "permission");
    let channels = app.http.guild_channels(Id::new(g)).await?.models().await?;
    ConfigAccess {
        guild: g,
        actor,
        owner,
        permissions,
        member: &member,
    }
    .check_channels(c, &channels)?;
    check_condition_references(c, &table, &channels)?;
    let targets = c
        .mute_role
        .iter()
        .chain(&c.jail_role)
        .chain(&c.booster_role)
        .chain(c.conditions.iter().map(|r| &r.role));
    for target in targets {
        let role = table
            .iter()
            .find(|r| r.id.to_string() == *target)
            .ok_or_else(|| anyhow::anyhow!("role unavailable"))?;
        let facts = RoleFacts {
            everyone: role.id.get() == g,
            managed: role.managed,
            position: role.position,
            permissions: role.permissions.bits(),
        };
        let grantor = Grantor {
            owner: owner == actor,
            administrator: admin,
            top_role: Some(top(&member.roles)),
        };
        anyhow::ensure!(
            pinchora_core::moderation::moderation::role_refusal_for(facts, grantor, Some(top(&bot.roles)), &role.name)
                .is_none(),
            "unsafe role"
        );
        anyhow::ensure!(
            pinchora_core::admin::settings::dangerous_permissions(role.permissions.bits()).is_empty(),
            "unsafe automated role"
        );
    }
    Ok(())
}

pub(super) async fn policy(cx: &Cx, c: Policy) -> anyhow::Result<()> {
    let Some(g) = permitted(cx, Permissions::MANAGE_GUILD, "manage-server").await? else {
        return Ok(());
    };
    let mut p = cx.app.db.moderation_policy_config(g).await?;
    if let Some(r) = c.reports {
        p.report_channel = Some(r.id.to_string())
    }
    if let Some(r) = c.mute {
        p.mute_role = Some(r.id.to_string())
    }
    if let Some(r) = c.jail {
        p.jail_role = Some(r.id.to_string())
    }
    if let Some(v) = c.appeals {
        p.appeals = v
    }
    if let Some(v) = c.personal {
        p.booster_personal = v;
    }
    if let Some(r) = c.booster {
        p.booster_role = Some(r.id.to_string())
    }
    if check_config(&cx.app, g, cx.author()?.id.get(), &p).await.is_err() {
        return invalid(cx).await;
    }
    if let Some(user) = c.report_user {
        if !allowed(cx, Permissions::MODERATE_MEMBERS) {
            return invalid(cx).await;
        }
        let actor = Moderator::of(cx)?;
        let Ok((ranks, _)) = fresh_ranks(
            &cx.app,
            g,
            &actor,
            Permissions::MANAGE_GUILD | Permissions::MODERATE_MEMBERS,
        )
        .await
        else {
            return invalid(cx).await;
        };
        let Some(target) = ranks.of(&cx.app, g, user.resolved.id.get()).await else {
            return invalid(cx).await;
        };
        if may_act(ranks.owner, ranks.actor, ranks.bot, target).is_err() {
            return cx.reply_ephemeral(t!(cx.lang(), "parity.refused")).await;
        }
        if c.blocked.unwrap_or(true) {
            sqlx::query("INSERT INTO mod_report_blocks(guild_id,user_id) VALUES($1,$2) ON CONFLICT DO NOTHING")
                .bind(g as i64)
                .bind(user.resolved.id.get() as i64)
                .execute(cx.app.db.pool())
                .await?;
        } else {
            sqlx::query("DELETE FROM mod_report_blocks WHERE guild_id=$1 AND user_id=$2")
                .bind(g as i64)
                .bind(user.resolved.id.get() as i64)
                .execute(cx.app.db.pool())
                .await?;
        }
    }
    cx.app.db.save_moderation_policy_config(g, &p).await?;
    log(
        &cx.app,
        g,
        (cx.author()?.id.get(), &cx.author_display_name()),
        CaseKind::RoleEdit,
        g,
        None,
        Some("policy".into()),
    )
    .await?;
    done(cx).await
}

struct ConfigAccess<'a> {
    guild: u64,
    actor: u64,
    owner: u64,
    permissions: Permissions,
    member: &'a twilight_model::guild::Member,
}
impl ConfigAccess<'_> {
    fn check_channels(&self, c: &Config, channels: &[twilight_model::channel::Channel]) -> anyhow::Result<()> {
        let (g, actor, owner) = (self.guild, self.actor, self.owner);
        let (permissions, member) = (self.permissions, self.member);
        for (id, permission) in c.report_channel.iter().map(|id| (id, Permissions::VIEW_CHANNEL)).chain(
            c.autopurge
                .iter()
                .map(|r| (&r.channel, Permissions::VIEW_CHANNEL | Permissions::MANAGE_MESSAGES)),
        ) {
            let channel = channels
                .iter()
                .find(|ch| {
                    ch.id.to_string() == *id
                        && matches!(
                            ch.kind,
                            twilight_model::channel::ChannelType::GuildText
                                | twilight_model::channel::ChannelType::GuildAnnouncement
                        )
                })
                .ok_or_else(|| anyhow::anyhow!("channel unavailable"))?;
            let base = if owner == actor {
                Permissions::ADMINISTRATOR
            } else {
                permissions
            };
            let effective = crate::infra::text::access::channel_permissions(
                base,
                g,
                actor,
                &member.roles.iter().map(|r| r.get()).collect::<Vec<_>>(),
                channel.permission_overwrites.as_deref().unwrap_or_default(),
            );
            anyhow::ensure!(effective.contains(permission), "channel permission");
        }
        Ok(())
    }
}

fn check_condition_references(
    c: &Config,
    table: &[Role],
    channels: &[twilight_model::channel::Channel],
) -> anyhow::Result<()> {
    for rule in &c.conditions {
        anyhow::ensure!(
            rule.requires
                .iter()
                .all(|id| table.iter().any(|r| r.id.to_string() == *id)),
            "required role unavailable"
        );
    }
    if c.conditions.iter().any(|r| r.voice_channel.is_some()) {
        for id in c.conditions.iter().filter_map(|r| r.voice_channel.as_ref()) {
            anyhow::ensure!(
                channels.iter().any(|ch| ch.id.to_string() == *id
                    && matches!(
                        ch.kind,
                        twilight_model::channel::ChannelType::GuildVoice
                            | twilight_model::channel::ChannelType::GuildStageVoice
                    )),
                "voice channel unavailable"
            );
        }
    }
    Ok(())
}
