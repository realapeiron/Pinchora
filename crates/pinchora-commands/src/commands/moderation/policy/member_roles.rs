use super::*;

pub(super) async fn names(cx: &Cx, c: Names) -> anyhow::Result<()> {
    let Some(g) = permitted(cx, Permissions::MANAGE_NICKNAMES, "manage-nicknames").await? else {
        return Ok(());
    };
    let actor = Moderator::of(cx)?;
    let Ok((ranks, _)) = fresh_ranks(&cx.app, g, &actor, Permissions::MANAGE_NICKNAMES).await else {
        return invalid(cx).await;
    };
    let Some(rank) = ranks.of(&cx.app, g, c.user.resolved.id.get()).await else {
        return invalid(cx).await;
    };
    if may_act(ranks.owner, ranks.actor, ranks.bot, rank).is_err() {
        return cx.reply_ephemeral(t!(cx.lang(), "parity.refused")).await;
    }
    let name = nickname(
        &crate::infra::util::resolved_display_name(&c.user),
        c.normalize.unwrap_or(false),
    );
    cx.app
        .http
        .update_guild_member(Id::new(g), c.user.resolved.id)
        .nick(Some(&name))
        .reason(&members::audit_reason(cx.lang(), &actor, None))
        .await?;
    log(
        &cx.app,
        g,
        (actor.id, &actor.name),
        CaseKind::Nick,
        c.user.resolved.id.get(),
        None,
        Some(name),
    )
    .await?;
    done(cx).await
}

pub(super) async fn roles(cx: &Cx, c: Roles) -> anyhow::Result<()> {
    if c.action == "booster" {
        return booster_role(cx, c).await;
    }
    let Some(g) = permitted(cx, Permissions::MANAGE_ROLES, "manage-roles").await? else {
        return Ok(());
    };
    let actor = Moderator::of(cx)?;
    let Ok((ranks, admin)) = fresh_ranks(&cx.app, g, &actor, Permissions::MANAGE_ROLES).await else {
        return invalid(cx).await;
    };
    let audit = members::audit_reason(cx.lang(), &actor, None);
    let color = match c
        .color
        .as_deref()
        .map(|s| u32::from_str_radix(s.trim_start_matches('#'), 16))
        .transpose()
    {
        Ok(v) if v.is_none_or(|v| v <= 0xffffff) => v,
        _ => return invalid(cx).await,
    };
    let id = if c.action == "create" {
        let Some(name) = c.name else { return invalid(cx).await };
        cx.app
            .http
            .create_role(Id::new(g))
            .name(&name)
            .permissions(Permissions::empty())
            .color(color.unwrap_or(0))
            .mentionable(c.mentionable.unwrap_or(false))
            .reason(&audit)
            .await?
            .model()
            .await?
            .id
    } else {
        let Some(selected) = c.role else {
            return invalid(cx).await;
        };
        let current = cx.app.http.roles(Id::new(g)).await?.models().await?;
        let Some(r) = current.iter().find(|r| r.id == selected.id) else {
            return invalid(cx).await;
        };
        let facts = RoleFacts {
            everyone: r.id.get() == g,
            managed: r.managed,
            position: r.position,
            permissions: r.permissions.bits(),
        };
        if pinchora_core::moderation::moderation::role_refusal_for(
            facts,
            Grantor {
                owner: actor.id == ranks.owner,
                administrator: admin,
                top_role: Some(ranks.actor.top),
            },
            Some(ranks.bot.top),
            &r.name,
        )
        .is_some()
        {
            return invalid(cx).await;
        }
        if c.action == "delete" && c.confirm == Some(true) {
            cx.app.http.delete_role(Id::new(g), r.id).reason(&audit).await?;
        } else if c.action == "edit" {
            let mut req = cx.app.http.update_role(Id::new(g), r.id);
            if let Some(ref n) = c.name {
                req = req.name(Some(n))
            }
            if let Some(v) = color {
                req = req.color(Some(v))
            }
            if let Some(v) = c.mentionable {
                req = req.mentionable(v)
            }
            req.reason(&audit).await?;
        } else {
            return invalid(cx).await;
        }
        r.id
    };
    log(
        &cx.app,
        g,
        (actor.id, &actor.name),
        CaseKind::RoleEdit,
        id.get(),
        None,
        Some(c.action),
    )
    .await?;
    done(cx).await
}

async fn booster_role(cx: &Cx, c: Roles) -> anyhow::Result<()> {
    let Some(g) = cx.guild_id() else {
        return invalid(cx).await;
    };
    let user = cx.author()?.id;
    if !cx.app.db.moderation_policy_config(g).await?.booster_personal {
        return invalid(cx).await;
    }
    let member = cx.app.http.guild_member(Id::new(g), user).await?.model().await?;
    if member.premium_since.is_none() {
        return invalid(cx).await;
    }
    let _guard = cx.app.db.lock_ban(g, g).await?;
    let _member_guard = cx.app.db.lock_ban(g, user.get()).await?;
    if sqlx::query_scalar::<_, bool>("SELECT EXISTS(SELECT 1 FROM mod_restrictions WHERE guild_id=$1 AND user_id=$2)")
        .bind(g as i64)
        .bind(user.get() as i64)
        .fetch_one(cx.app.db.pool())
        .await?
    {
        return invalid(cx).await;
    }
    let existing: Option<i64> =
        sqlx::query_scalar("SELECT role_id FROM mod_booster_roles WHERE guild_id=$1 AND user_id=$2")
            .bind(g as i64)
            .bind(user.get() as i64)
            .fetch_optional(cx.app.db.pool())
            .await?;
    let name = c.name.unwrap_or_else(|| cx.author_display_name());
    let color = match c
        .color
        .as_deref()
        .map(|s| u32::from_str_radix(s.trim_start_matches('#'), 16))
        .transpose()
    {
        Ok(v) if v.is_none_or(|v| v <= 0xffffff) => v,
        _ => return invalid(cx).await,
    };
    let audit = members::audit_reason(cx.lang(), &Moderator::of(cx)?, None);
    let rid = if let Some(rid) = existing {
        let Some(role) = editable_booster_role(cx, g, rid).await? else {
            return invalid(cx).await;
        };
        cx.app
            .http
            .update_role(Id::new(g), role)
            .name(Some(&name))
            .color(color)
            .reason(&audit)
            .await?;
        role
    } else {
        let count: i64 = sqlx::query_scalar("SELECT count(*) FROM mod_booster_roles WHERE guild_id=$1")
            .bind(g as i64)
            .fetch_one(cx.app.db.pool())
            .await?;
        if count >= 25 {
            return invalid(cx).await;
        }
        let role = cx
            .app
            .http
            .create_role(Id::new(g))
            .name(&name)
            .permissions(Permissions::empty())
            .color(color.unwrap_or(0))
            .mentionable(false)
            .reason(&audit)
            .await?
            .model()
            .await?;
        sqlx::query("INSERT INTO mod_booster_roles(guild_id,user_id,role_id) VALUES($1,$2,$3)")
            .bind(g as i64)
            .bind(user.get() as i64)
            .bind(role.id.get() as i64)
            .execute(cx.app.db.pool())
            .await?;
        role.id
    };
    cx.app
        .http
        .add_guild_member_role(Id::new(g), user, rid)
        .reason(&audit)
        .await?;
    log(
        &cx.app,
        g,
        (user.get(), &cx.author_display_name()),
        CaseKind::RoleEdit,
        user.get(),
        None,
        Some(rid.to_string()),
    )
    .await?;
    done(cx).await
}

async fn editable_booster_role(
    cx: &Cx,
    g: u64,
    rid: i64,
) -> anyhow::Result<Option<Id<twilight_model::id::marker::RoleMarker>>> {
    let table = cx.app.http.roles(Id::new(g)).await?.models().await?;
    let mine = cx.app.my_roles(g).await?;
    let top = table
        .iter()
        .filter(|r| mine.contains(&r.id.get()))
        .map(|r| r.position)
        .max()
        .unwrap_or(0);
    let Some(role) = table
        .iter()
        .find(|r| r.id.get() == rid as u64 && !r.managed && r.position < top && r.permissions.is_empty())
    else {
        return Ok(None);
    };
    Ok(Some(role.id))
}
