use super::*;

// /Picking

/// What changing a member's roles did.
pub(crate) struct Applied {
    pub added: Vec<u64>,
    pub removed: Vec<u64>,
    /// The first change Discord refused; the ones after it were not tried.
    pub failed: Option<Failed>,
}

pub(crate) struct Failed {
    pub role: u64,
    pub giving: bool,
    pub error: anyhow::Error,
    /// The role no longer exists, and was dropped from everything that named it.
    pub gone: bool,
}

/// Takes and gives the roles of `changes` (takes first, so a swap frees its place), keeping the timers
/// of roles that expire: `temp` is how long each role lasts, in seconds.
pub(crate) async fn apply(
    app: &App,
    guild: u64,
    user: u64,
    changes: &Changes,
    temp: &HashMap<u64, i32>,
    reason: &str,
) -> Applied {
    let mut done = Applied {
        added: Vec::new(),
        removed: Vec::new(),
        failed: None,
    };
    // Reject a role whose current permissions or position changed before touching a held role.
    for &role in &changes.add {
        if let Err(error) = give::validate(app, guild, role).await {
            done.failed = Some(failed(app, guild, role, true, error).await);
            return done;
        }
    }
    let mut scheduled = Vec::new();
    for &role in &changes.add {
        if let Some(secs) = temp.get(&role) {
            let at = app.now() + chrono::Duration::seconds(i64::from(*secs));
            if let Err(error) = app.db.schedule_removal(guild, user, role, at).await {
                for prior in scheduled {
                    if let Err(cancel) = app.db.cancel_removal(guild, user, prior).await {
                        tracing::warn!(guild, user, role = prior, %cancel, "could not clear a prepared timer");
                    }
                }
                done.failed = Some(failed(app, guild, role, true, error).await);
                return done;
            }
            scheduled.push(role);
        }
    }
    for &role in &changes.remove {
        match give::take(app, guild, user, role, reason).await {
            Ok(()) => {
                done.removed.push(role);
                if let Err(error) = app.db.cancel_removal(guild, user, role).await {
                    tracing::warn!(guild, user, role, %error, "could not cancel a role's timer");
                }
            }
            Err(error) => {
                for &pending in &scheduled {
                    if let Err(cancel) = app.db.cancel_removal(guild, user, pending).await {
                        tracing::warn!(guild, user, role = pending, %cancel, "could not clear a prepared timer");
                    }
                }
                done.failed = Some(failed(app, guild, role, false, error.into()).await);
                return done;
            }
        }
    }
    for &role in &changes.add {
        match give::give(app, guild, user, role, reason).await {
            Ok(()) => {
                done.added.push(role);
            }
            Err(error) => {
                for &pending in &scheduled {
                    if !done.added.contains(&pending)
                        && let Err(cancel) = app.db.cancel_removal(guild, user, pending).await
                    {
                        tracing::warn!(guild, user, role = pending, %cancel, "could not clear a prepared timer");
                    }
                }
                done.failed = Some(failed(app, guild, role, true, error).await);
                return done;
            }
        }
    }
    done
}

async fn failed(app: &App, guild: u64, role: u64, giving: bool, error: anyhow::Error) -> Failed {
    tracing::warn!(guild, role, %error, "could not change a role from a panel");
    let gone = error.downcast_ref::<Error>().is_some_and(roles::role_is_gone);
    if gone {
        give::forget(app, guild, role).await;
    }
    Failed {
        role,
        giving,
        error,
        gone,
    }
}

fn mentions(roles: &[u64]) -> String {
    roles.iter().map(|r| format!("<@&{r}>")).collect::<Vec<_>>().join(", ")
}

/// What to tell a member their pick did.
pub(crate) fn applied_text(lang: Lang, done: &Applied, temp: &HashMap<u64, i32>) -> String {
    let mut parts: Vec<String> = Vec::new();
    if !done.added.is_empty() {
        parts.push(t!(lang, "roles.panel.added", roles = mentions(&done.added)));
    }
    if !done.removed.is_empty() {
        parts.push(t!(lang, "roles.panel.removed", roles = mentions(&done.removed)));
    }
    for role in &done.added {
        if let Some(secs) = temp.get(role) {
            parts.push(t!(
                lang,
                "roles.panel.lasts",
                role = format!("<@&{role}>"),
                span = format_span(i64::from(*secs))
            ));
        }
    }
    if let Some(fail) = &done.failed {
        let role = format!("<@&{}>", fail.role);
        let why = give::give_refusal_text(lang, &fail.error);
        parts.push(if fail.giving {
            t!(lang, "roles.panel.cant-give", role = role, why = why)
        } else {
            t!(lang, "roles.panel.cant-take", role = role, why = why)
        });
        if fail.gone {
            parts.push(t!(lang, "roles.panel.button-removed"));
        }
    }
    if parts.is_empty() {
        t!(lang, "roles.panel.unchanged")
    } else {
        parts.join(" ")
    }
}

/// What a refused pick says.
fn refusal_line(lang: Lang, refusal: Refusal) -> String {
    match refusal {
        Refusal::NotOnPanel => t!(lang, "roles.panel.not-on-panel"),
        Refusal::TooMany(max) => t!(lang, "roles.panel.too-many", max = max),
        Refusal::TooFew(min) => t!(lang, "roles.panel.too-few", min = min),
    }
}

/// The roles of a panel and how long each lasts, as a pick needs them.
async fn panel_facts(cx: &Cx, panel: &PanelRow) -> anyhow::Result<(Vec<u64>, HashMap<u64, i32>)> {
    let entries = cx.app.db.panel_roles(panel.id).await?;
    let temp = entries
        .iter()
        .filter_map(|e| e.temp_secs.map(|s| (e.role_id as u64, s)))
        .collect();
    Ok((entries.iter().map(|e| e.role_id as u64).collect(), temp))
}

/// Interaction member roles can be older than another click. Read Discord under the member lock.
pub(crate) async fn current_roles(app: &App, guild: u64, user: u64) -> anyhow::Result<Vec<u64>> {
    let member = app
        .http
        .guild_member(Id::new(guild.max(1)), Id::new(user.max(1)))
        .await?
        .model()
        .await?;
    Ok(member.roles.iter().map(|role| role.get()).collect())
}

/// `rr` and `rb` buttons: give the member the role, or take it if they have it, as the panel's rules allow.
pub(crate) async fn on_toggle<'a>(cx: &Cx, parts: impl Iterator<Item = &'a str>) -> anyhow::Result<()> {
    let lang = cx.lang();
    let Some(guild) = cx.guild_id() else {
        return cx.reply_ephemeral(t!(lang, "roles.panel.guild-only-buttons")).await;
    };
    let Some((panel_id, role)) = parse_button(parts) else {
        return cx.reply_ephemeral(t!(lang, "roles.panel.button-expired")).await;
    };
    // The id came from the client; it counts only if that role still has a button on a panel of this server.
    let db = &cx.app.db;
    let Some(panel) = db.panel_by_id(guild, panel_id).await? else {
        return cx.reply_ephemeral(t!(lang, "roles.panel.role-gone")).await;
    };
    if db.panel_role(guild, panel_id, role).await?.is_none() {
        return cx.reply_ephemeral(t!(lang, "roles.panel.role-gone")).await;
    }
    let user = cx.author()?.id.get();
    let _lock = cx.app.db.lock_role_member(guild, user).await?;
    let held = current_roles(&cx.app, guild, user).await?;
    if let Some(needed) = panel.required_role.filter(|r| !held.contains(&(*r as u64))) {
        return cx
            .reply_ephemeral(t!(lang, "roles.panel.needs-role", role = format!("<@&{needed}>")))
            .await;
    }
    let (ids, temp) = panel_facts(cx, &panel).await?;
    let changes = match toggle(&rules_of(&panel), &ids, &held, role) {
        Ok(changes) => changes,
        Err(refusal) => return cx.reply_ephemeral(refusal_line(lang, refusal)).await,
    };
    let done = apply(&cx.app, guild, user, &changes, &temp, "Role panel").await;
    cx.reply_ephemeral(applied_text(lang, &done, &temp)).await
}

/// `rs` menus: what the member picked is what they hold of the panel afterwards.
pub(crate) async fn on_select<'a>(
    cx: &Cx,
    mut parts: impl Iterator<Item = &'a str>,
    values: &[String],
) -> anyhow::Result<()> {
    let lang = cx.lang();
    let Some(guild) = cx.guild_id() else {
        return cx.reply_ephemeral(t!(lang, "roles.panel.guild-only-menus")).await;
    };
    let panel_id = parts.next().and_then(|p| p.parse::<i64>().ok());
    let chosen: Option<Vec<u64>> = values.iter().map(|v| v.parse().ok()).collect();
    let (Some(panel_id), Some(chosen), None) = (panel_id, chosen, parts.next()) else {
        return cx.reply_ephemeral(t!(lang, "roles.panel.menu-expired")).await;
    };
    let Some(panel) = cx.app.db.panel_by_id(guild, panel_id).await? else {
        return cx.reply_ephemeral(t!(lang, "roles.panel.menu-gone")).await;
    };
    let user = cx.author()?.id.get();
    let _lock = cx.app.db.lock_role_member(guild, user).await?;
    let held = current_roles(&cx.app, guild, user).await?;
    if let Some(needed) = panel.required_role.filter(|r| !held.contains(&(*r as u64))) {
        return cx
            .reply_ephemeral(t!(lang, "roles.panel.needs-role", role = format!("<@&{needed}>")))
            .await;
    }
    let (ids, temp) = panel_facts(cx, &panel).await?;
    let changes = match choose(&rules_of(&panel), &ids, &held, &chosen) {
        Ok(changes) => changes,
        Err(refusal) => return cx.reply_ephemeral(refusal_line(lang, refusal)).await,
    };
    let done = apply(&cx.app, guild, user, &changes, &temp, "Role panel").await;
    cx.reply_ephemeral(applied_text(lang, &done, &temp)).await
}
