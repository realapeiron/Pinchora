use super::*;

pub(super) async fn notes(cx: &Cx, c: Notes) -> anyhow::Result<()> {
    let Some(g) = permitted(cx, Permissions::MODERATE_MEMBERS, "timeout-members").await? else {
        return Ok(());
    };
    let uid = c.user.resolved.id.get() as i64;
    let db = cx.app.db.pool();
    if c.action == "list" {
        let rows: Vec<(i64, String)> =
            sqlx::query_as("SELECT id,body FROM mod_notes WHERE guild_id=$1 AND user_id=$2 ORDER BY id DESC LIMIT 10")
                .bind(g as i64)
                .bind(uid)
                .fetch_all(db)
                .await?;
        let text = rows
            .iter()
            .map(|(id, s)| format!("#{id}: {s}"))
            .collect::<Vec<_>>()
            .join("\n");
        return cx
            .reply_ephemeral(if text.is_empty() {
                t!(cx.lang(), "parity.empty")
            } else {
                text.chars().take(1900).collect()
            })
            .await;
    }
    let body = c.text.as_deref().unwrap_or("").trim();
    match c.action.as_str() {
        "add" if !body.is_empty() => {
            sqlx::query("INSERT INTO mod_notes(guild_id,user_id,author_id,body) VALUES($1,$2,$3,$4)")
                .bind(g as i64)
                .bind(uid)
                .bind(cx.author()?.id.get() as i64)
                .bind(body)
                .execute(db)
                .await?;
        }
        "edit" if c.id.is_some() && !body.is_empty() => {
            let changed = sqlx::query("UPDATE mod_notes SET body=$4 WHERE guild_id=$1 AND user_id=$2 AND id=$3")
                .bind(g as i64)
                .bind(uid)
                .bind(c.id)
                .bind(body)
                .execute(db)
                .await?;
            if changed.rows_affected() == 0 {
                return invalid(cx).await;
            }
        }
        "delete" if c.id.is_some() => {
            let changed = sqlx::query("DELETE FROM mod_notes WHERE guild_id=$1 AND user_id=$2 AND id=$3")
                .bind(g as i64)
                .bind(uid)
                .bind(c.id)
                .execute(db)
                .await?;
            if changed.rows_affected() == 0 {
                return invalid(cx).await;
            }
        }
        _ => return invalid(cx).await,
    }
    log(
        &cx.app,
        g,
        (cx.author()?.id.get(), &cx.author_display_name()),
        CaseKind::Note,
        uid as u64,
        None,
        Some(c.action),
    )
    .await?;
    done(cx).await
}

pub(super) async fn report(cx: &Cx, c: Report) -> anyhow::Result<()> {
    let Some(g) = cx.guild_id() else {
        return invalid(cx).await;
    };
    submit_report(cx, g, c.user.resolved.id.get(), None, c.reason).await
}

pub(crate) async fn submit_report(
    cx: &Cx,
    g: u64,
    user: u64,
    message: Option<u64>,
    body: String,
) -> anyhow::Result<()> {
    let config = cx.app.db.moderation_policy_config(g).await?;
    let Some(channel) = config
        .report_channel
        .as_deref()
        .and_then(pinchora_core::moderation::policy::id)
    else {
        return cx.reply_ephemeral(t!(cx.lang(), "parity.no-inbox")).await;
    };
    let id = cx
        .app
        .db
        .submit_mod_report(
            g,
            user,
            cx.author()?.id.get(),
            cx.interaction.channel.as_ref().map(|c| c.id.get()),
            message,
            &body,
        )
        .await?;
    let Some(id) = id else {
        return cx.reply_ephemeral(t!(cx.lang(), "parity.report-limit")).await;
    };
    let mut text = t!(cx.lang(), "parity.report-post", id = id, user = user, body = body);
    if let (Some(message), Some(source)) = (message, cx.interaction.channel.as_ref()) {
        text.push_str(&format!("\nhttps://discord.com/channels/{g}/{}/{message}", source.id));
    }
    let _ = cx
        .app
        .http
        .create_message(Id::new(channel))
        .content(&text)
        .allowed_mentions(Some(&AllowedMentions::default()))
        .await;
    log(
        &cx.app,
        g,
        (cx.app.application_id.get(), BOT_NAME),
        CaseKind::Report,
        user,
        None,
        Some(format!("#{id}")),
    )
    .await?;
    done(cx).await
}

/// Review queues share the actual sanction permission and hierarchy, including the original moderator.
pub async fn review(
    app: &App,
    g: u64,
    actor: (u64, &str),
    queue: &str,
    id: i64,
    accept: bool,
    reason: &str,
) -> anyhow::Result<()> {
    let (actor, name) = actor;
    anyhow::ensure!(
        reason.chars().count() <= 512 && !reason.trim().is_empty(),
        "decision required"
    );
    let member = app.http.guild_member(Id::new(g), Id::new(actor)).await?.model().await?;
    let moderator = Moderator {
        id: actor,
        name: name.into(),
        roles: member.roles.iter().map(|r| r.get()).collect(),
    };
    let (ranks, _) = fresh_ranks(app, g, &moderator, Permissions::empty()).await?;
    let perms = member
        .roles
        .iter()
        .map(|id| id.get())
        .chain([g])
        .filter_map(|id| ranks.table.get(&id))
        .fold(Permissions::empty(), |p, r| p | r.permissions);
    let mut tx = app.db.pool().begin().await?;
    anyhow::ensure!(matches!(queue, "reports" | "appeals"), "queue");
    let row: Option<(i64, String)> = sqlx::query_as(if queue == "reports" {
        "SELECT user_id,status FROM mod_reports WHERE guild_id=$1 AND id=$2 FOR UPDATE"
    } else {
        "SELECT user_id,status FROM mod_appeals WHERE guild_id=$1 AND id=$2 FOR UPDATE"
    })
    .bind(g as i64)
    .bind(id)
    .fetch_optional(&mut *tx)
    .await?;
    let (user, status) = row.ok_or_else(|| anyhow::anyhow!("not found"))?;
    anyhow::ensure!(status == "pending", "already reviewed");
    let mut ban_guard = None;
    let needed = if queue == "appeals" {
        AppealReview {
            app,
            guild: g,
            actor,
            ranks: &ranks,
            permissions: perms,
            moderator: &moderator,
            id,
            user,
            accept,
            reason,
        }
        .resolve(&mut tx, &mut ban_guard)
        .await?
    } else {
        Permissions::MODERATE_MEMBERS
    };
    anyhow::ensure!(
        actor == ranks.owner || perms.contains(Permissions::ADMINISTRATOR) || perms.contains(needed),
        "permission"
    );
    sqlx::query(if queue == "reports" {
        "UPDATE mod_reports SET status=$3,reviewer_id=$4 WHERE guild_id=$1 AND id=$2"
    } else {
        "UPDATE mod_appeals SET status=$3,reviewer_id=$4 WHERE guild_id=$1 AND id=$2"
    })
    .bind(g as i64)
    .bind(id)
    .bind(if accept { "accepted" } else { "rejected" })
    .bind(actor as i64)
    .execute(&mut *tx)
    .await?;
    if queue == "appeals" {
        sqlx::query("UPDATE mod_appeals SET decision=$3 WHERE guild_id=$1 AND id=$2")
            .bind(g as i64)
            .bind(id)
            .bind(reason)
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;
    drop(ban_guard);
    log(
        app,
        g,
        (actor, name),
        if queue == "appeals" {
            CaseKind::Appeal
        } else {
            CaseKind::Report
        },
        user as u64,
        Some(reason.into()),
        Some(format!("{id}: {}", if accept { "accepted" } else { "rejected" })),
    )
    .await
}

pub(crate) async fn appeal_hint(app: &App, g: u64, case: &CaseRow) {
    if !matches!(case.kind.as_str(), "ban" | "warn") {
        return;
    }
    if !app.db.moderation_policy_config(g).await.is_ok_and(|c| c.appeals)
        || !app.db.mod_settings(g).await.is_ok_and(|c| c.dm_members)
    {
        return;
    }
    let Some(channel) = private_channel(app, case.target_id as u64).await else {
        return;
    };
    let lang = member_lang(app, g, case.target_id as u64).await;
    let url = format!("{}/appeals/{g}/{}", app.public_url.trim_end_matches('/'), case.number);
    let content = t!(lang, "parity.appeal-link", number = case.number, url = url);
    let _ = app
        .http
        .create_message(channel)
        .content(&content)
        .allowed_mentions(Some(&AllowedMentions::default()))
        .await;
}

struct AppealReview<'a> {
    app: &'a App,
    guild: u64,
    actor: u64,
    ranks: &'a Ranks,
    permissions: Permissions,
    moderator: &'a Moderator,
    id: i64,
    user: i64,
    accept: bool,
    reason: &'a str,
}
impl<'a> AppealReview<'a> {
    async fn resolve(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        ban_guard: &mut Option<sqlx::Transaction<'a, sqlx::Postgres>>,
    ) -> anyhow::Result<Permissions> {
        let (app, g, actor, ranks) = (self.app, self.guild, self.actor, self.ranks);
        let (perms, moderator) = (self.permissions, self.moderator);
        let (id, user, accept, reason) = (self.id, self.user, self.accept, self.reason);
        let number: i32 = sqlx::query_scalar("SELECT case_number FROM mod_appeals WHERE guild_id=$1 AND id=$2")
            .bind(g as i64)
            .bind(id)
            .fetch_one(&mut **tx)
            .await?;
        let case = app
            .db
            .case(g, number)
            .await?
            .ok_or_else(|| anyhow::anyhow!("case unavailable"))?;
        anyhow::ensure!(case.active && case.target_id == user, "case inactive");
        let needed = if case.kind == "ban" {
            Permissions::BAN_MEMBERS
        } else {
            Permissions::MODERATE_MEMBERS
        };
        anyhow::ensure!(
            actor == ranks.owner || perms.contains(Permissions::ADMINISTRATOR) || perms.contains(needed),
            "permission"
        );
        let target = ranks
            .of(app, g, user as u64)
            .await
            .ok_or_else(|| anyhow::anyhow!("target unavailable"))?;
        anyhow::ensure!(
            may_act(ranks.owner, ranks.actor, ranks.bot, target).is_ok(),
            "hierarchy"
        );
        if case.moderator_id as u64 != actor
            && case.moderator_id != 0
            && case.moderator_id as u64 != app.application_id.get()
        {
            let staff = ranks
                .of(app, g, case.moderator_id as u64)
                .await
                .ok_or_else(|| anyhow::anyhow!("moderator unavailable"))?;
            anyhow::ensure!(
                pinchora_core::moderation::moderation::outranks(ranks.owner, ranks.actor, staff),
                "moderator hierarchy"
            );
        }
        if accept {
            if case.kind == "ban" {
                *ban_guard = Some(app.db.lock_ban(g, user as u64).await?);
                let newest: Option<i32> = sqlx::query_scalar(
                    "SELECT max(number) FROM mod_cases WHERE guild_id=$1 AND target_id=$2 AND kind='ban' AND active",
                )
                .bind(g as i64)
                .bind(user)
                .fetch_one(&mut **tx)
                .await?;
                anyhow::ensure!(newest == Some(number), "newer ban exists");
                app.http
                    .delete_ban(Id::new(g), Id::new(user as u64))
                    .reason(&members::audit_reason(app.guild_lang(g).await, moderator, Some(reason)))
                    .await?;
            }
            sqlx::query("UPDATE mod_cases SET active=false,updated_at=now() WHERE guild_id=$1 AND number=$2")
                .bind(g as i64)
                .bind(number)
                .execute(&mut **tx)
                .await?;
            // Record after releasing the transaction: open_case may update the same case rows.
        }
        Ok(needed)
    }
}
