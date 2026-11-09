//! typed member operations. Configuration lives under /safety.
mod reports;
pub use reports::review;
pub(crate) use reports::{appeal_hint, submit_report};
use reports::{notes, report};
mod configuration;
pub use configuration::check_config;
use configuration::policy;
mod member_roles;
use member_roles::{names, roles};
mod sanctions;
use sanctions::{execute, fresh_ranks, permission, restrict};
mod voice;
use voice::voice;

use crate::commands::moderation::*;
use pinchora_core::admin::settings::Grantor;
use pinchora_core::admin::settings::RoleFacts;
use pinchora_core::moderation::policy::Config;
use pinchora_core::moderation::policy::nickname;
use pinchora_core::moderation::policy::parse_ids;
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
        CaseKind::Purge => Some(pinchora_core::admin::logging::LogEvent::AutoPurge),
        CaseKind::RoleAdd | CaseKind::RoleRemove => Some(pinchora_core::admin::logging::LogEvent::RoleRule),
        CaseKind::Report => Some(pinchora_core::admin::logging::LogEvent::Report),
        CaseKind::Appeal => Some(pinchora_core::admin::logging::LogEvent::Appeal),
        CaseKind::Mute | CaseKind::Unmute | CaseKind::Jail | CaseKind::Unjail => {
            Some(pinchora_core::admin::logging::LogEvent::Restriction)
        }
        _ => None,
    };
    if let Some(event) = event
        && let Some((channel, lang)) = crate::moderation::server_log::route(
            app,
            g,
            event,
            pinchora_core::admin::logging::Subject {
                user: Some(user),
                ..Default::default()
            },
            None,
        )
        .await
    {
        let embed = EmbedBuilder::new()
            .title(crate::moderation::server_log::title(lang, event))
            .description(format!("<@{user}> · <@{actor}>"))
            .build();
        crate::moderation::server_log::post(app, g, channel, embed).await;
    }
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
        let pattern = match c
            .regex
            .as_deref()
            .map(pinchora_core::moderation::automod::compile)
            .transpose()
        {
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
