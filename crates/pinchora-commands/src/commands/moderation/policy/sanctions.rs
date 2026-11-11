use super::*;

/// Mutations must not use the cached hierarchy: roles can move while a preview is open.
pub(super) async fn fresh_ranks(
    app: &App,
    g: u64,
    actor: &Moderator,
    needed: Permissions,
) -> anyhow::Result<(Ranks, bool)> {
    let table = app.http.roles(Id::new(g)).await?.models().await?;
    let member = app
        .http
        .guild_member(Id::new(g), Id::new(actor.id))
        .await?
        .model()
        .await?;
    let bot = app
        .http
        .guild_member(Id::new(g), Id::new(app.application_id.get()))
        .await?
        .model()
        .await?;
    let owner = app.http.guild(Id::new(g)).await?.model().await?.owner_id.get();
    let perms = table
        .iter()
        .filter(|r| r.id.get() == g || member.roles.contains(&r.id))
        .fold(Permissions::empty(), |p, r| p | r.permissions);
    anyhow::ensure!(
        actor.id == owner || perms.contains(Permissions::ADMINISTRATOR) || perms.contains(needed),
        "permission"
    );
    let table: HashMap<_, _> = table
        .into_iter()
        .map(|r| {
            (
                r.id.get(),
                RoleInfo {
                    position: r.position,
                    permissions: r.permissions,
                },
            )
        })
        .collect();
    Ok((
        Ranks {
            owner,
            actor: Rank {
                id: actor.id,
                top: top(&table, member.roles.iter().map(|r| r.get())),
            },
            bot: Rank {
                id: app.application_id.get(),
                top: top(&table, bot.roles.iter().map(|r| r.get())),
            },
            table: Arc::new(table),
        },
        perms.contains(Permissions::ADMINISTRATOR),
    ))
}

pub(super) fn permission(action: &str) -> Option<(Permissions, &'static str)> {
    Some(match action {
        "ban" | "softban" => (Permissions::BAN_MEMBERS, "ban-members"),
        "kick" => (Permissions::KICK_MEMBERS, "kick-members"),
        "warn" | "mute" | "unmute" | "jail" | "unjail" => (Permissions::MODERATE_MEMBERS, "timeout-members"),
        "role-add" | "role-remove" => (Permissions::MANAGE_ROLES, "manage-roles"),
        _ => return None,
    })
}

pub(super) async fn execute(
    cx: &Cx,
    g: u64,
    action: &str,
    user: u64,
    role: Option<u64>,
    reason: Option<String>,
) -> anyhow::Result<()> {
    let (p, _) = permission(action).ok_or_else(|| anyhow::anyhow!("action"))?;
    anyhow::ensure!(allowed(cx, p), "permission");
    let actor = Moderator::of(cx)?;
    let needed = if matches!(action, "mute" | "jail" | "unmute" | "unjail") {
        p | Permissions::MANAGE_ROLES
    } else {
        p
    };
    let (ranks, admin) = fresh_ranks(&cx.app, g, &actor, needed).await?;
    let rank = ranks
        .of(&cx.app, g, user)
        .await
        .ok_or_else(|| anyhow::anyhow!("member unavailable"))?;
    anyhow::ensure!(may_act(ranks.owner, ranks.actor, ranks.bot, rank).is_ok(), "hierarchy");
    anyhow::ensure!(
        crate::moderation::anti_nuke::vip_refusal(&cx.app, g, user)
            .await?
            .is_none(),
        "protected member"
    );
    let reason = pinchora_core::moderation::moderation::check_reason(reason.as_deref())
        .map_err(|_| anyhow::anyhow!("reason"))?;
    let audit = members::audit_reason(cx.lang(), &actor, reason.as_deref());
    let sanction = Sanction {
        cx,
        guild: g,
        user,
        actor,
        ranks,
        administrator: admin,
        audit,
        reason,
        action,
    };
    let kind = match action {
        "ban" => return sanction.ban().await,
        "softban" => sanction.softban().await?,
        "kick" => sanction.kick().await?,
        "warn" => return sanction.warn().await,
        "role-add" | "role-remove" => sanction.change_role(role).await?,
        "mute" | "jail" | "unmute" | "unjail" => sanction.restrict_roles().await?,
        _ => unreachable!(),
    };
    log(
        &cx.app,
        g,
        (sanction.actor.id, &sanction.actor.name),
        kind,
        user,
        sanction.reason,
        None,
    )
    .await
}

pub(super) async fn restrict(cx: &Cx, c: Restrict) -> anyhow::Result<()> {
    let Some((p, label)) = permission(&c.action) else {
        return invalid(cx).await;
    };
    let Some(g) = permitted(cx, p, label).await? else {
        return Ok(());
    };
    if execute(cx, g, &c.action, c.user.resolved.id.get(), None, c.reason)
        .await
        .is_err()
    {
        return cx.reply_ephemeral(t!(cx.lang(), "parity.refused")).await;
    }
    done(cx).await
}

struct Sanction<'a> {
    cx: &'a Cx,
    guild: u64,
    user: u64,
    actor: Moderator,
    ranks: Ranks,
    administrator: bool,
    audit: String,
    reason: Option<String>,
    action: &'a str,
}

impl Sanction<'_> {
    async fn ban(&self) -> anyhow::Result<()> {
        let cx = self.cx;
        let g = self.guild;
        let user = self.user;
        let audit = &self.audit;
        let http = &self.cx.app.http;
        let gid = Id::new(self.guild);
        let uid = Id::new(self.user);
        let guard = cx.app.db.lock_ban(g, user).await?;
        http.create_ban(gid, uid).reason(audit).await?;
        let act = Act {
            kind: CaseKind::Ban,
            target: user,
            target_name: user.to_string(),
            reason: self.reason.clone(),
            duration: None,
            details: None,
        };
        super::super::revoke::record_locked(&cx.app, guard, g, (self.actor.id, &self.actor.name), &act, None).await?;
        extra_log(&cx.app, g, self.actor.id, CaseKind::Ban, user).await;
        Ok(())
    }

    async fn softban(&self) -> anyhow::Result<CaseKind> {
        let cx = self.cx;
        let g = self.guild;
        let user = self.user;
        let actor = &self.actor;
        let audit = &self.audit;
        let reason = &self.reason;
        let http = &self.cx.app.http;
        let gid = Id::new(self.guild);
        let uid = Id::new(self.user);
        let _guard = cx.app.db.lock_ban(g, user).await?;
        anyhow::ensure!(
            http.ban(gid, uid)
                .await
                .is_err_and(|e| crate::roles::roles::error_code(&e) == Some(10026)),
            "already banned or unavailable"
        );
        http.create_ban(gid, uid)
            .delete_message_seconds(86400)
            .reason(audit)
            .await?;
        // Record the real ban first: a failed unban must remain visible and actionable.
        log(
            &cx.app,
            g,
            (actor.id, &actor.name),
            CaseKind::Ban,
            user,
            reason.clone(),
            Some("softban".into()),
        )
        .await?;
        http.delete_ban(gid, uid).reason(audit).await?;
        log(
            &cx.app,
            g,
            (actor.id, &actor.name),
            CaseKind::Unban,
            user,
            reason.clone(),
            Some("softban".into()),
        )
        .await?;
        Ok(CaseKind::Softban)
    }

    async fn kick(&self) -> anyhow::Result<CaseKind> {
        let audit = &self.audit;
        let http = &self.cx.app.http;
        let gid = Id::new(self.guild);
        let uid = Id::new(self.user);
        http.remove_guild_member(gid, uid).reason(audit).await?;
        Ok(CaseKind::Kick)
    }

    async fn warn(self) -> anyhow::Result<()> {
        let cx = self.cx;
        let g = self.guild;
        let actor = &self.actor;
        let ranks = &self.ranks;
        let http = &self.cx.app.http;
        let gid = Id::new(self.guild);
        let uid = Id::new(self.user);
        anyhow::ensure!(self.reason.is_some(), "a warning needs a reason");
        let member = http.guild_member(gid, uid).await?.model().await?;
        let permissions = member
            .roles
            .iter()
            .map(|r| r.get())
            .chain([g])
            .filter_map(|id| ranks.table.get(&id))
            .fold(Permissions::empty(), |p, r| p | r.permissions);
        let mut data = serde_json::to_value(&member)?;
        data["permissions"] = serde_json::json!(permissions.bits().to_string());
        let resolved = ResolvedUser {
            resolved: member.user.clone(),
            member: Some(serde_json::from_value(data)?),
        };
        let act = Act::on(CaseKind::Warn, &resolved, self.reason);
        let opened = record(&cx.app, g, (actor.id, &actor.name), &act, None).await?;
        let ladder = cx.app.db.warn_thresholds(g).await?;
        if let Some(rung) = pinchora_core::moderation::moderation::reached(&ladder, opened.active_warns) {
            crate::commands::moderation::warns::climb(cx, g, ranks, &resolved, rung, opened.active_warns).await?;
        }
        Ok(())
    }

    async fn change_role(&self, role: Option<u64>) -> anyhow::Result<CaseKind> {
        let g = self.guild;
        let actor = &self.actor;
        let ranks = &self.ranks;
        let admin = self.administrator;
        let audit = &self.audit;
        let action = self.action;
        let http = &self.cx.app.http;
        let gid = Id::new(self.guild);
        let uid = Id::new(self.user);
        let rid = role.ok_or_else(|| anyhow::anyhow!("role"))?;
        let roles = http.roles(gid).await?.models().await?;
        let r = roles
            .iter()
            .find(|r| r.id.get() == rid)
            .ok_or_else(|| anyhow::anyhow!("role"))?;
        let facts = RoleFacts {
            everyone: rid == g,
            managed: r.managed,
            position: r.position,
            permissions: r.permissions.bits(),
        };
        anyhow::ensure!(
            pinchora_core::moderation::moderation::role_refusal_for(
                facts,
                Grantor {
                    owner: actor.id == ranks.owner,
                    administrator: admin,
                    top_role: Some(ranks.actor.top)
                },
                Some(ranks.bot.top),
                &r.name
            )
            .is_none(),
            "unsafe role"
        );
        Ok(if action == "role-add" {
            http.add_guild_member_role(gid, uid, r.id).reason(audit).await?;
            CaseKind::RoleAdd
        } else {
            http.remove_guild_member_role(gid, uid, r.id).reason(audit).await?;
            CaseKind::RoleRemove
        })
    }

    async fn apply_restriction(
        &self,
        member: twilight_model::guild::Member,
        roles: Vec<Role>,
    ) -> anyhow::Result<CaseKind> {
        let cx = self.cx;
        let g = self.guild;
        let user = self.user;
        let ranks = &self.ranks;
        let audit = &self.audit;
        let action = self.action;
        let http = &self.cx.app.http;
        let gid = Id::new(self.guild);
        let uid = Id::new(self.user);
        let c = cx.app.db.moderation_policy_config(g).await?;
        let rid = if action == "mute" { c.mute_role } else { c.jail_role }
            .as_deref()
            .and_then(pinchora_core::moderation::policy::id)
            .ok_or_else(|| anyhow::anyhow!("restriction role not configured"))?;
        anyhow::ensure!(
            roles.iter().filter(|r| self.safe_role(r)).any(|r| r.id.get() == rid),
            "unsafe role"
        );
        // Reject before stripping anything if the same role cannot be safely restored.
        anyhow::ensure!(
            member.roles.iter().all(|id| roles.iter().any(|r| r.id == *id
                && if r.managed {
                    !r.permissions.contains(Permissions::ADMINISTRATOR)
                } else {
                    self.safe_role(r)
                })),
            "unrestorable roles"
        );
        let managed: Vec<_> = roles
            .iter()
            .filter(|r| r.managed && member.roles.contains(&r.id))
            .map(|r| r.id.get())
            .collect();
        crate::moderation::policy::restriction_overwrites(&cx.app, g, user, rid, &managed, audit).await?;
        let saved: Vec<i64> = member.roles.iter().map(|r| r.get() as i64).collect();
        let inserted=sqlx::query("INSERT INTO mod_restrictions(guild_id,user_id,role_id,role_ids,kind) VALUES($1,$2,$3,$4,$5) ON CONFLICT DO NOTHING").bind(g as i64).bind(user as i64).bind(rid as i64).bind(&saved).bind(action).execute(cx.app.db.pool()).await?.rows_affected();
        anyhow::ensure!(inserted == 1, "already restricted; release first");
        // Retain managed roles; reject roles beyond the bot instead of silently under-restricting.
        anyhow::ensure!(
            member.roles.iter().all(|id| roles
                .iter()
                .any(|r| r.id == *id && (r.managed || r.position < ranks.bot.top))),
            "unmanageable roles"
        );
        let mut desired: Vec<_> = roles
            .iter()
            .filter(|r| r.managed && member.roles.contains(&r.id))
            .map(|r| r.id)
            .collect();
        desired.push(Id::new(rid));
        http.update_guild_member(gid, uid).roles(&desired).reason(audit).await?;
        Ok(if action == "mute" {
            CaseKind::Mute
        } else {
            CaseKind::Jail
        })
    }

    async fn release_restriction(
        &self,
        member: twilight_model::guild::Member,
        roles: Vec<Role>,
    ) -> anyhow::Result<CaseKind> {
        let cx = self.cx;
        let g = self.guild;
        let user = self.user;
        let actor = &self.actor;
        let ranks = &self.ranks;
        let audit = &self.audit;
        let action = self.action;
        let http = &self.cx.app.http;
        let gid = Id::new(self.guild);
        let uid = Id::new(self.user);
        let saved: Option<(i64, Vec<i64>, String)> =
            sqlx::query_as("SELECT role_id,role_ids,kind FROM mod_restrictions WHERE guild_id=$1 AND user_id=$2")
                .bind(g as i64)
                .bind(user as i64)
                .fetch_optional(cx.app.db.pool())
                .await?;
        let (rid, saved, kind) = saved.ok_or_else(|| anyhow::anyhow!("not restricted"))?;
        anyhow::ensure!(
            (action == "unmute" && kind == "mute") || (action == "unjail" && kind == "jail"),
            "wrong restriction"
        );
        anyhow::ensure!(
            roles
                .iter()
                .filter(|r| saved.contains(&(r.id.get() as i64)) && !r.managed)
                .all(|r| self.safe_role(r)),
            "saved roles changed; keep the snapshot for staff resolution"
        );
        if let Some(r) = roles.iter().find(|r| r.id.get() == rid as u64) {
            anyhow::ensure!(
                !r.managed && r.position < ranks.bot.top && (actor.id == ranks.owner || r.position < ranks.actor.top),
                "restriction role moved"
            );
        }
        let mut desired: Vec<_> = member.roles.into_iter().filter(|id| id.get() != rid as u64).collect();
        for r in roles
            .iter()
            .filter(|r| self.safe_role(r))
            .filter(|r| r.id.get() != rid as u64 && saved.contains(&(r.id.get() as i64)))
        {
            if !desired.contains(&r.id) {
                desired.push(r.id)
            }
        }
        http.update_guild_member(gid, uid).roles(&desired).reason(audit).await?;
        sqlx::query("DELETE FROM mod_restrictions WHERE guild_id=$1 AND user_id=$2")
            .bind(g as i64)
            .bind(user as i64)
            .execute(cx.app.db.pool())
            .await?;
        Ok(if action == "unmute" {
            CaseKind::Unmute
        } else {
            CaseKind::Unjail
        })
    }

    async fn restrict_roles(&self) -> anyhow::Result<CaseKind> {
        let cx = self.cx;
        let g = self.guild;
        let user = self.user;
        let action = self.action;
        let http = &self.cx.app.http;
        let gid = Id::new(self.guild);
        let uid = Id::new(self.user);
        anyhow::ensure!(allowed(cx, Permissions::MANAGE_ROLES), "Manage Roles");
        let _guard = cx.app.db.lock_ban(g, user).await?;
        let member = http.guild_member(gid, uid).await?.model().await?;
        let roles = http.roles(gid).await?.models().await?;
        if action == "mute" || action == "jail" {
            self.apply_restriction(member, roles).await
        } else {
            self.release_restriction(member, roles).await
        }
    }

    fn safe_role(&self, r: &Role) -> bool {
        !r.managed
            && r.id.get() != self.guild
            && r.position < self.ranks.bot.top
            && (self.actor.id == self.ranks.owner || r.position < self.ranks.actor.top)
            && pinchora_core::admin::settings::dangerous_permissions(r.permissions.bits()).is_empty()
    }
}
