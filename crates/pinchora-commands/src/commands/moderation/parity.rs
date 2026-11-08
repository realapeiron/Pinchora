//! D11 typed member operations. Configuration lives under /safety.
use super::*;
use pinchora_core::{
    mod_parity::{Config, nickname, parse_ids},
    settings::{Grantor, RoleFacts},
};
use twilight_interactions::command::{CommandModel, CreateCommand};
use twilight_model::{application::interaction::InteractionChannel, guild::Role};

#[derive(CommandModel, CreateCommand)]
#[command(name = "modtools", desc = "Moderation tools", contexts = "guild")]
pub enum Parity {
    #[command(name = "notes")]
    Notes(Notes),
    #[command(name = "report")]
    Report(Report),
    #[command(name = "restrict")]
    Restrict(Restrict),
    #[command(name = "mass")]
    Mass(Mass),
    #[command(name = "voice")]
    Voice(Voice),
    #[command(name = "roles")]
    Roles(Roles),
    #[command(name = "names")]
    Names(Names),
    #[command(name = "policy")]
    Policy(Box<Policy>),
}
#[derive(CommandModel, CreateCommand)]
#[command(name = "notes", desc = "Staff notes")]
pub struct Notes {
    /// add, list, edit or delete
    action: String,
    /// Member
    user: ResolvedUser,
    /// Note text
    #[command(max_length = 1000)]
    text: Option<String>,
    /// Note number
    id: Option<i64>,
}
#[derive(CommandModel, CreateCommand)]
#[command(name = "report", desc = "Report a member to staff")]
pub struct Report {
    /// Member
    user: ResolvedUser,
    /// What happened
    #[command(min_length = 1, max_length = 1000)]
    reason: String,
}
#[derive(CommandModel, CreateCommand)]
#[command(name = "restrict", desc = "Mute, jail, release or softban")]
pub struct Restrict {
    /// mute, unmute, jail, unjail or softban
    action: String,
    /// Member
    user: ResolvedUser,
    /// Why
    #[command(max_length = 512)]
    reason: Option<String>,
}
#[derive(CommandModel, CreateCommand)]
#[command(name = "mass", desc = "Preview then confirm a bulk action")]
pub struct Mass {
    /// ban, kick, warn, mute, role-add or role-remove
    action: String,
    /// Space-separated member IDs
    #[command(max_length = 2000)]
    ids: Option<String>,
    /// Username regular expression
    #[command(max_length = 100)]
    regex: Option<String>,
    /// Joined after this UTC date (RFC3339)
    after: Option<String>,
    /// Joined before this UTC date (RFC3339)
    before: Option<String>,
    /// Role for bulk role actions
    role: Option<Role>,
    /// Why
    #[command(max_length = 512)]
    reason: Option<String>,
    /// Confirmation token from the preview
    confirm: Option<String>,
}
#[derive(CommandModel, CreateCommand)]
#[command(name = "voice", desc = "Move, disconnect, mute or ban voice members")]
pub struct Voice {
    /// move, disconnect, ban, unban, mute, unmute, deafen or undeafen
    action: String,
    /// Member (omit for everyone in source)
    user: Option<ResolvedUser>,
    /// Source voice channel for all members
    source: Option<InteractionChannel>,
    /// Destination for move
    destination: Option<InteractionChannel>,
    /// Confirm acting on all source members
    confirm: Option<bool>,
    /// Why
    #[command(max_length = 512)]
    reason: Option<String>,
}
#[derive(CommandModel, CreateCommand)]
#[command(name = "roles", desc = "Create, edit or delete a role")]
pub struct Roles {
    /// create, edit or delete
    action: String,
    /// Role to edit or delete
    role: Option<Role>,
    /// Role name
    #[command(min_length = 1, max_length = 100)]
    name: Option<String>,
    /// Hex color, such as ff8844
    color: Option<String>,
    /// Allow mentions
    mentionable: Option<bool>,
    /// Confirm deleting the role
    confirm: Option<bool>,
}
#[derive(CommandModel, CreateCommand)]
#[command(name = "names", desc = "Remove nickname hoisting or decoration")]
pub struct Names {
    /// Member
    user: ResolvedUser,
    /// Normalize decorations as well as hoisting
    normalize: Option<bool>,
}
#[derive(CommandModel, CreateCommand)]
#[command(name = "policy", desc = "Moderation and role policy")]
pub struct Policy {
    /// Report inbox
    reports: Option<InteractionChannel>,
    /// Mute role
    mute: Option<Role>,
    /// Jail role
    jail: Option<Role>,
    /// Enable member appeals
    appeals: Option<bool>,
    /// Booster reward role
    booster: Option<Role>,
    /// Allow personal booster roles
    personal: Option<bool>,
    /// Member report access
    report_user: Option<ResolvedUser>,
    /// Block reports from this member
    blocked: Option<bool>,
}
impl Parity {
    pub async fn run(self, cx: &Cx) -> anyhow::Result<()> {
        match self {
            Self::Notes(c) => notes(cx, c).await,
            Self::Report(c) => report(cx, c).await,
            Self::Restrict(c) => restrict(cx, c).await,
            Self::Mass(c) => mass(cx, c).await,
            Self::Voice(c) => voice(cx, c).await,
            Self::Roles(c) => roles(cx, c).await,
            Self::Names(c) => names(cx, c).await,
            Self::Policy(c) => policy(cx, *c).await,
        }
    }
}
async fn invalid(cx: &Cx) -> anyhow::Result<()> {
    cx.reply_ephemeral(t!(cx.lang(), "parity.invalid")).await
}
async fn done(cx: &Cx) -> anyhow::Result<()> {
    cx.reply_ephemeral(t!(cx.lang(), "parity.saved")).await
}
async fn log(
    app: &App,
    g: u64,
    actor: (u64, &str),
    kind: CaseKind,
    user: u64,
    reason: Option<String>,
    details: Option<String>,
) -> anyhow::Result<()> {
    let act = Act {
        kind,
        target: user,
        target_name: user.to_string(),
        reason,
        duration: None,
        details,
    };
    record(app, g, actor, &act, None).await?;
    extra_log(app, g, actor.0, kind, user).await;
    Ok(())
}
pub(crate) async fn extra_log(app: &App, g: u64, actor: u64, kind: CaseKind, user: u64) {
    let event = match kind {
        CaseKind::Purge => Some(pinchora_core::logging::LogEvent::AutoPurge),
        CaseKind::RoleAdd | CaseKind::RoleRemove => Some(pinchora_core::logging::LogEvent::RoleRule),
        CaseKind::Report => Some(pinchora_core::logging::LogEvent::Report),
        CaseKind::Appeal => Some(pinchora_core::logging::LogEvent::Appeal),
        CaseKind::Mute | CaseKind::Unmute | CaseKind::Jail | CaseKind::Unjail => {
            Some(pinchora_core::logging::LogEvent::Restriction)
        }
        _ => None,
    };
    if let Some(event) = event
        && let Some((channel, lang)) = crate::server_log::route(
            app,
            g,
            event,
            pinchora_core::logging::Subject {
                user: Some(user),
                ..Default::default()
            },
            None,
        )
        .await
    {
        let embed = EmbedBuilder::new()
            .title(crate::server_log::title(lang, event))
            .description(format!("<@{user}> · <@{actor}>"))
            .build();
        crate::server_log::post(app, g, channel, embed).await;
    }
}
async fn notes(cx: &Cx, c: Notes) -> anyhow::Result<()> {
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
async fn report(cx: &Cx, c: Report) -> anyhow::Result<()> {
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
    let config = cx.app.db.mod_parity_config(g).await?;
    let Some(channel) = config.report_channel.as_deref().and_then(pinchora_core::mod_parity::id) else {
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
/// Mutations must not use the cached hierarchy: roles can move while a preview is open.
async fn fresh_ranks(app: &App, g: u64, actor: &Moderator, needed: Permissions) -> anyhow::Result<(Ranks, bool)> {
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
    let previous = app.db.mod_parity_config(g).await?;
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
        let effective = crate::text::access::channel_permissions(
            base,
            g,
            actor,
            &member.roles.iter().map(|r| r.get()).collect::<Vec<_>>(),
            channel.permission_overwrites.as_deref().unwrap_or_default(),
        );
        anyhow::ensure!(effective.contains(permission), "channel permission");
    }
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
            pinchora_core::moderation::role_refusal_for(facts, grantor, Some(top(&bot.roles)), &role.name).is_none(),
            "unsafe role"
        );
        anyhow::ensure!(
            pinchora_core::settings::dangerous_permissions(role.permissions.bits()).is_empty(),
            "unsafe automated role"
        );
    }
    Ok(())
}
async fn policy(cx: &Cx, c: Policy) -> anyhow::Result<()> {
    let Some(g) = permitted(cx, Permissions::MANAGE_GUILD, "manage-server").await? else {
        return Ok(());
    };
    let mut p = cx.app.db.mod_parity_config(g).await?;
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
    cx.app.db.save_mod_parity_config(g, &p).await?;
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
fn permission(action: &str) -> Option<(Permissions, &'static str)> {
    Some(match action {
        "ban" | "softban" => (Permissions::BAN_MEMBERS, "ban-members"),
        "kick" => (Permissions::KICK_MEMBERS, "kick-members"),
        "warn" | "mute" | "unmute" | "jail" | "unjail" => (Permissions::MODERATE_MEMBERS, "timeout-members"),
        "role-add" | "role-remove" => (Permissions::MANAGE_ROLES, "manage-roles"),
        _ => return None,
    })
}
async fn execute(
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
        crate::anti_nuke::vip_refusal(&cx.app, g, user).await?.is_none(),
        "protected member"
    );
    let reason = pinchora_core::moderation::check_reason(reason.as_deref()).map_err(|_| anyhow::anyhow!("reason"))?;
    let audit = members::audit_reason(cx.lang(), &actor, reason.as_deref());
    let http = &cx.app.http;
    let gid = Id::new(g);
    let uid = Id::new(user);
    let kind = match action {
        "ban" => {
            let _guard = cx.app.db.lock_ban(g, user).await?;
            http.create_ban(gid, uid).reason(&audit).await?;
            CaseKind::Ban
        }
        "softban" => {
            let _guard = cx.app.db.lock_ban(g, user).await?;
            anyhow::ensure!(
                http.ban(gid, uid)
                    .await
                    .is_err_and(|e| crate::roles::error_code(&e) == Some(10026)),
                "already banned or unavailable"
            );
            http.create_ban(gid, uid)
                .delete_message_seconds(86400)
                .reason(&audit)
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
            http.delete_ban(gid, uid).reason(&audit).await?;
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
            CaseKind::Softban
        }
        "kick" => {
            http.remove_guild_member(gid, uid).reason(&audit).await?;
            CaseKind::Kick
        }
        "warn" => {
            anyhow::ensure!(reason.is_some(), "a warning needs a reason");
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
            let act = Act::on(CaseKind::Warn, &resolved, reason);
            let opened = record(&cx.app, g, (actor.id, &actor.name), &act, None).await?;
            let ladder = cx.app.db.warn_thresholds(g).await?;
            if let Some(rung) = pinchora_core::moderation::reached(&ladder, opened.active_warns) {
                super::warns::climb(cx, g, &ranks, &resolved, rung, opened.active_warns).await?;
            }
            return Ok(());
        }
        "role-add" | "role-remove" => {
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
                pinchora_core::moderation::role_refusal_for(
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
            if action == "role-add" {
                http.add_guild_member_role(gid, uid, r.id).reason(&audit).await?;
                CaseKind::RoleAdd
            } else {
                http.remove_guild_member_role(gid, uid, r.id).reason(&audit).await?;
                CaseKind::RoleRemove
            }
        }
        "mute" | "jail" | "unmute" | "unjail" => {
            anyhow::ensure!(allowed(cx, Permissions::MANAGE_ROLES), "Manage Roles");
            let _guard = cx.app.db.lock_ban(g, user).await?;
            let member = http.guild_member(gid, uid).await?.model().await?;
            let roles = http.roles(gid).await?.models().await?;
            let safe = |r: &&Role| {
                !r.managed
                    && r.id.get() != g
                    && r.position < ranks.bot.top
                    && (actor.id == ranks.owner || r.position < ranks.actor.top)
                    && pinchora_core::settings::dangerous_permissions(r.permissions.bits()).is_empty()
            };
            if action == "mute" || action == "jail" {
                let c = cx.app.db.mod_parity_config(g).await?;
                let rid = if action == "mute" { c.mute_role } else { c.jail_role }
                    .as_deref()
                    .and_then(pinchora_core::mod_parity::id)
                    .ok_or_else(|| anyhow::anyhow!("restriction role not configured"))?;
                anyhow::ensure!(roles.iter().filter(safe).any(|r| r.id.get() == rid), "unsafe role");
                // Reject before stripping anything if the same role cannot be safely restored.
                anyhow::ensure!(
                    member.roles.iter().all(|id| roles.iter().any(|r| r.id == *id
                        && if r.managed {
                            !r.permissions.contains(Permissions::ADMINISTRATOR)
                        } else {
                            safe(&r)
                        })),
                    "unrestorable roles"
                );
                let managed: Vec<_> = roles
                    .iter()
                    .filter(|r| r.managed && member.roles.contains(&r.id))
                    .map(|r| r.id.get())
                    .collect();
                crate::mod_parity::restriction_overwrites(&cx.app, g, user, rid, &managed, &audit).await?;
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
                http.update_guild_member(gid, uid)
                    .roles(&desired)
                    .reason(&audit)
                    .await?;
                if action == "mute" {
                    CaseKind::Mute
                } else {
                    CaseKind::Jail
                }
            } else {
                let saved: Option<(i64, Vec<i64>, String)> = sqlx::query_as(
                    "SELECT role_id,role_ids,kind FROM mod_restrictions WHERE guild_id=$1 AND user_id=$2",
                )
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
                        .all(|r| safe(&r)),
                    "saved roles changed; keep the snapshot for staff resolution"
                );
                if let Some(r) = roles.iter().find(|r| r.id.get() == rid as u64) {
                    anyhow::ensure!(
                        !r.managed
                            && r.position < ranks.bot.top
                            && (actor.id == ranks.owner || r.position < ranks.actor.top),
                        "restriction role moved"
                    );
                }
                let mut desired: Vec<_> = member.roles.into_iter().filter(|id| id.get() != rid as u64).collect();
                for r in roles
                    .iter()
                    .filter(safe)
                    .filter(|r| r.id.get() != rid as u64 && saved.contains(&(r.id.get() as i64)))
                {
                    if !desired.contains(&r.id) {
                        desired.push(r.id)
                    }
                }
                http.update_guild_member(gid, uid)
                    .roles(&desired)
                    .reason(&audit)
                    .await?;
                sqlx::query("DELETE FROM mod_restrictions WHERE guild_id=$1 AND user_id=$2")
                    .bind(g as i64)
                    .bind(user as i64)
                    .execute(cx.app.db.pool())
                    .await?;
                if action == "unmute" {
                    CaseKind::Unmute
                } else {
                    CaseKind::Unjail
                }
            }
        }
        _ => unreachable!(),
    };
    log(&cx.app, g, (actor.id, &actor.name), kind, user, reason, None).await
}
async fn restrict(cx: &Cx, c: Restrict) -> anyhow::Result<()> {
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
#[derive(serde::Serialize, serde::Deserialize)]
struct Batch {
    action: String,
    ids: Vec<u64>,
    role: Option<u64>,
    reason: Option<String>,
}
async fn batch_slot(cx: &Cx, g: u64) -> anyhow::Result<bool> {
    if cx
        .app
        .cache
        .try_cooldown(&format!("modparity:batch:{g}"), StdDuration::from_secs(60))
        .await?
        .is_some()
    {
        cx.reply_ephemeral(t!(cx.lang(), "parity.batch-limit")).await?;
        return Ok(false);
    }
    Ok(true)
}
async fn batch_reply(cx: &Cx, ok: usize, total: usize, failed: &[u64]) -> anyhow::Result<()> {
    let mut text = t!(cx.lang(), "parity.batch-result", ok = ok, total = total);
    if !failed.is_empty() {
        text.push_str(&format!(
            "\n{}",
            t!(
                cx.lang(),
                "parity.batch-failed",
                ids = failed.iter().map(u64::to_string).collect::<Vec<_>>().join(", ")
            )
        ));
    }
    if text.chars().count() <= 1900 {
        cx.reply_ephemeral(text).await
    } else {
        cx.reply(members::private(EmbedBuilder::new().description(text).build()))
            .await
    }
}
async fn mass(cx: &Cx, c: Mass) -> anyhow::Result<()> {
    let Some((p, label)) = permission(&c.action) else {
        return invalid(cx).await;
    };
    let Some(g) = permitted(cx, p, label).await? else {
        return Ok(());
    };
    let key = |token: &str| {
        format!(
            "modparity:confirm:{g}:{}:{token}",
            cx.author().map(|u| u.id.get()).unwrap_or(0)
        )
    };
    if let Some(token) = c.confirm {
        let Some(raw) = cx.app.cache.take(&key(&token)).await? else {
            return invalid(cx).await;
        };
        let batch: Batch = serde_json::from_str(&raw)?;
        if batch.action != c.action {
            return invalid(cx).await;
        }
        if !batch_slot(cx, g).await? {
            return Ok(());
        }
        let mut ok = 0;
        let mut failed = Vec::new();
        let total = batch.ids.len();
        for user in batch.ids {
            if execute(cx, g, &batch.action, user, batch.role, batch.reason.clone())
                .await
                .is_ok()
            {
                ok += 1
            } else {
                failed.push(user);
            }
        }
        return batch_reply(cx, ok, total, &failed).await;
    }
    let ids = if let Some(s) = c.ids {
        let Some(ids) = parse_ids(&s) else {
            return invalid(cx).await;
        };
        ids
    } else {
        if c.regex.is_none() && c.after.is_none() && c.before.is_none() {
            return invalid(cx).await;
        }
        let pattern = match c.regex.as_deref().map(pinchora_core::automod::compile).transpose() {
            Ok(r) => r,
            Err(_) => return invalid(cx).await,
        };
        let after = match c.after.as_deref().map(DateTime::parse_from_rfc3339).transpose() {
            Ok(d) => d,
            Err(_) => return invalid(cx).await,
        };
        let before = match c.before.as_deref().map(DateTime::parse_from_rfc3339).transpose() {
            Ok(d) => d,
            Err(_) => return invalid(cx).await,
        };
        let mut ids = Vec::new();
        let mut cursor = None;
        loop {
            let mut req = cx.app.http.guild_members(Id::new(g)).limit(1000);
            if let Some(id) = cursor {
                req = req.after(id)
            }
            let page = req.await?.models().await?;
            let full = page.len() == 1000;
            for m in &page {
                if pattern.as_ref().is_none_or(|p| p.is_match(&m.user.name))
                    && after.is_none_or(|a| m.joined_at.is_some_and(|d| d.as_secs() >= a.timestamp()))
                    && before.is_none_or(|a| m.joined_at.is_some_and(|d| d.as_secs() <= a.timestamp()))
                {
                    ids.push(m.user.id.get())
                }
            }
            if ids.len() > 100 {
                return cx.reply_ephemeral(t!(cx.lang(), "parity.too-many")).await;
            }
            cursor = page.last().map(|m| m.user.id);
            if !full {
                break;
            }
        }
        ids
    };
    if ids.is_empty() {
        return cx.reply_ephemeral(t!(cx.lang(), "parity.empty")).await;
    }
    let batch = Batch {
        action: c.action,
        ids,
        role: c.role.map(|r| r.id.get()),
        reason: c.reason,
    };
    let token = format!("{:016x}", rand::random::<u64>());
    cx.app
        .cache
        .put(
            &key(&token),
            &serde_json::to_string(&batch)?,
            StdDuration::from_secs(120),
        )
        .await?;
    let preview = t!(
        cx.lang(),
        "parity.confirm",
        count = batch.ids.len(),
        ids = batch.ids.iter().map(u64::to_string).collect::<Vec<_>>().join(", "),
        action = batch.action,
        token = token
    );
    if preview.chars().count() > 1900 {
        cx.reply(members::private(EmbedBuilder::new().description(preview).build()))
            .await
    } else {
        cx.reply_ephemeral(preview).await
    }
}
async fn names(cx: &Cx, c: Names) -> anyhow::Result<()> {
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
        &crate::util::resolved_display_name(&c.user),
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
async fn roles(cx: &Cx, c: Roles) -> anyhow::Result<()> {
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
        if pinchora_core::moderation::role_refusal_for(
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
async fn voice(cx: &Cx, c: Voice) -> anyhow::Result<()> {
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
    let current = cx
        .app
        .http
        .guild_member(Id::new(g), Id::new(actor.id))
        .await?
        .model()
        .await?;
    let held: Vec<_> = current.roles.iter().map(|r| r.get()).collect();
    let base = if actor.id == ranks.owner {
        Permissions::ADMINISTRATOR
    } else {
        held.iter()
            .copied()
            .chain([g])
            .filter_map(|id| ranks.table.get(&id))
            .fold(Permissions::empty(), |p, r| p | r.permissions)
    };
    let channels = cx.app.http.guild_channels(Id::new(g)).await?.models().await?;
    let can_use = |id| {
        channels.iter().find(|ch| ch.id == id).is_some_and(|ch| {
            crate::text::access::channel_permissions(
                base,
                g,
                actor.id,
                &held,
                ch.permission_overwrites.as_deref().unwrap_or_default(),
            )
            .contains(p | Permissions::VIEW_CHANNEL)
        })
    };
    if [&c.source, &c.destination]
        .into_iter()
        .flatten()
        .any(|ch| !can_use(ch.id))
    {
        return invalid(cx).await;
    }
    let presence = cx.app.cache.map_all(&format!("modparity:voice-v2:{g}")).await?;
    let users = if let Some(u) = c.user {
        vec![u.resolved.id.get()]
    } else {
        let Some(source) = c.source else {
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
                crate::mod_parity::voice_channel(value).as_deref() == Some(source.id.to_string().as_str())
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
    let mut ok = 0;
    let mut failed = Vec::new();
    let total = users.len();
    for uid in users {
        if let Some(source) = presence
            .iter()
            .find(|(id, _)| id == &uid.to_string())
            .and_then(|(_, value)| crate::mod_parity::voice_channel(value))
            .and_then(|id| id.parse::<u64>().ok())
            && !can_use(Id::new(source))
        {
            failed.push(uid);
            continue;
        }
        let Some(rank) = ranks.of(&cx.app, g, uid).await else {
            failed.push(uid);
            continue;
        };
        if may_act(ranks.owner, ranks.actor, ranks.bot, rank).is_err()
            || crate::anti_nuke::vip_refusal(&cx.app, g, uid).await?.is_some()
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
                .reason(&members::audit_reason(cx.lang(), &actor, c.reason.as_deref()))
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
        let number: i32 = sqlx::query_scalar("SELECT case_number FROM mod_appeals WHERE guild_id=$1 AND id=$2")
            .bind(g as i64)
            .bind(id)
            .fetch_one(&mut *tx)
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
                pinchora_core::moderation::outranks(ranks.owner, ranks.actor, staff),
                "moderator hierarchy"
            );
        }
        if accept {
            if case.kind == "ban" {
                ban_guard = Some(app.db.lock_ban(g, user as u64).await?);
                let newest: Option<i32> = sqlx::query_scalar(
                    "SELECT max(number) FROM mod_cases WHERE guild_id=$1 AND target_id=$2 AND kind='ban' AND active",
                )
                .bind(g as i64)
                .bind(user)
                .fetch_one(&mut *tx)
                .await?;
                anyhow::ensure!(newest == Some(number), "newer ban exists");
                app.http
                    .delete_ban(Id::new(g), Id::new(user as u64))
                    .reason(&members::audit_reason(
                        app.guild_lang(g).await,
                        &moderator,
                        Some(reason),
                    ))
                    .await?;
            }
            sqlx::query("UPDATE mod_cases SET active=false,updated_at=now() WHERE guild_id=$1 AND number=$2")
                .bind(g as i64)
                .bind(number)
                .execute(&mut *tx)
                .await?;
            // Record after releasing the transaction: open_case may update the same case rows.
        }
        needed
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
async fn booster_role(cx: &Cx, c: Roles) -> anyhow::Result<()> {
    let Some(g) = cx.guild_id() else {
        return invalid(cx).await;
    };
    let user = cx.author()?.id;
    if !cx.app.db.mod_parity_config(g).await?.booster_personal {
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
            return invalid(cx).await;
        };
        cx.app
            .http
            .update_role(Id::new(g), role.id)
            .name(Some(&name))
            .color(color)
            .reason(&audit)
            .await?;
        role.id
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
pub(crate) async fn appeal_hint(app: &App, g: u64, case: &CaseRow) {
    if !matches!(case.kind.as_str(), "ban" | "warn") {
        return;
    }
    if !app.db.mod_parity_config(g).await.is_ok_and(|c| c.appeals)
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
