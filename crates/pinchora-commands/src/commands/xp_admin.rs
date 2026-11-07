//! `/xp`: server XP edits and confirmed imports.

use pinchora_core::t;
use pinchora_db::{AuditSource, NewAudit, XpEdit};
use twilight_interactions::command::{CommandModel, CommandOption, CreateCommand, CreateOption, ResolvedUser};
use twilight_model::{
    channel::message::{
        Component, MessageFlags,
        component::{ActionRow, Button, ButtonStyle},
    },
    guild::Permissions,
    http::interaction::InteractionResponseData,
};

use crate::{
    Cx,
    xp_admin::{self, ImportMode},
};

async fn confirmation_id(cx: &Cx, payload: String) -> anyhow::Result<String> {
    let nonce = format!("{:032x}", rand::random::<u128>());
    let key = format!(
        "xp:confirm:{}:{}:{nonce}",
        cx.guild_id().unwrap_or_default(),
        cx.author()?.id
    );
    cx.app
        .cache
        .put(&key, &payload, std::time::Duration::from_secs(300))
        .await?;
    Ok(format!("xi:confirm:{nonce}"))
}
fn manage_guild() -> Permissions {
    Permissions::MANAGE_GUILD
}

#[derive(CommandModel, CreateCommand)]
#[command(
    name = "xp",
    desc = "Manage server XP and import levels",
    default_permissions = "manage_guild",
    contexts = "guild"
)]
pub enum XpAdmin {
    #[command(name = "add")]
    Add(XpAmount),
    #[command(name = "remove")]
    Remove(XpRemove),
    #[command(name = "set")]
    Set(XpSet),
    #[command(name = "reset")]
    Reset(XpReset),
    #[command(name = "reset-all")]
    ResetAll(XpResetAll),
    #[command(name = "import")]
    Import(XpImport),
}

#[derive(CommandModel, CreateCommand)]
#[command(name = "add", desc = "Add server XP to a member")]
pub struct XpAmount {
    /// Member to edit
    user: ResolvedUser,
    /// XP amount
    #[command(min_value = 0, max_value = 1_000_000_000)]
    amount: i64,
}

#[derive(CommandModel, CreateCommand)]
#[command(name = "remove", desc = "Remove server XP from a member")]
pub struct XpRemove {
    /// Member to edit
    user: ResolvedUser,
    /// XP amount
    #[command(min_value = 0, max_value = 1_000_000_000)]
    amount: i64,
}

#[derive(CommandModel, CreateCommand)]
#[command(name = "set", desc = "Set a member's server XP")]
pub struct XpSet {
    /// Member to edit
    user: ResolvedUser,
    /// XP amount
    #[command(min_value = 0, max_value = 1_000_000_000)]
    amount: i64,
}

#[derive(CommandModel, CreateCommand)]
#[command(name = "reset", desc = "Reset one member's server XP")]
pub struct XpReset {
    /// Member to reset
    user: ResolvedUser,
}

#[derive(CommandModel, CreateCommand)]
#[command(name = "reset-all", desc = "Reset everyone's server XP; requires two confirmations")]
pub struct XpResetAll;

#[derive(CommandModel, CreateCommand)]
#[command(name = "import", desc = "Import levels from another bot")]
pub enum XpImport {
    #[command(name = "mee6")]
    Mee6(XpMee6),
}

#[derive(CommandOption, CreateOption, Clone, Copy)]
pub enum ModeOption {
    #[option(name = "Merge: keep the higher XP", value = "merge")]
    Merge,
    #[option(name = "Replace: use MEE6 XP", value = "replace")]
    Replace,
}

#[derive(CommandModel, CreateCommand)]
#[command(name = "mee6", desc = "Import MEE6 XP (once per server per day)")]
pub struct XpMee6 {
    /// How imported XP combines with existing XP
    mode: ModeOption,
}

fn buttons(options: &[(&str, String, ButtonStyle)]) -> Vec<Component> {
    vec![Component::ActionRow(ActionRow {
        id: None,
        components: options
            .iter()
            .map(|(label, id, style)| {
                Component::Button(Button {
                    custom_id: Some(id.clone()),
                    disabled: false,
                    emoji: None,
                    id: None,
                    label: Some((*label).to_owned()),
                    sku_id: None,
                    style: *style,
                    url: None,
                })
            })
            .collect(),
    })]
}

async fn confirm(cx: &Cx, text: String, id: String) -> anyhow::Result<()> {
    let id = confirmation_id(cx, id).await?;
    cx.reply(InteractionResponseData {
        content: Some(text),
        flags: Some(MessageFlags::EPHEMERAL),
        components: Some(buttons(&[(&t!(cx.lang(), "xp.confirm"), id, ButtonStyle::Danger)])),
        ..Default::default()
    })
    .await
}

impl XpAdmin {
    pub async fn run(self, cx: &Cx) -> anyhow::Result<()> {
        let Some(guild) = cx.guild_id() else {
            return cx.reply_ephemeral(t!(cx.lang(), "xp.guild-only")).await;
        };
        if !cx.can_manage_guild() {
            return cx.reply_ephemeral(t!(cx.lang(), "xp.permission")).await;
        }
        let actor = cx.author()?.id.get();
        match self {
            Self::Add(c) => edit(cx, guild, c.user, c.amount, XpEdit::Add, "xp.add").await,
            Self::Remove(c) => edit(cx, guild, c.user, c.amount, XpEdit::Remove, "xp.remove").await,
            Self::Set(c) => edit(cx, guild, c.user, c.amount, XpEdit::Set, "xp.set").await,
            Self::Reset(c) => {
                confirm(
                    cx,
                    t!(cx.lang(), "xp.reset-confirm", user = c.user.resolved.id),
                    format!("xi:reset:{guild}:{actor}:{}", c.user.resolved.id),
                )
                .await
            }
            Self::ResetAll(_) => {
                confirm(
                    cx,
                    t!(cx.lang(), "xp.all-confirm-first"),
                    format!("xi:all:1:{guild}:{actor}"),
                )
                .await
            }
            Self::Import(XpImport::Mee6(c)) => {
                if let Some(left) = xp_admin::mee6_resting(&cx.app, guild).await? {
                    return cx
                        .reply_ephemeral(t!(cx.lang(), "xp.import-rest", hours = left.as_secs().div_ceil(3600)))
                        .await;
                }
                let (mode, label) = match c.mode {
                    ModeOption::Merge => ("merge", t!(cx.lang(), "xp.mode-merge")),
                    ModeOption::Replace => ("replace", t!(cx.lang(), "xp.mode-replace")),
                };
                confirm(
                    cx,
                    t!(cx.lang(), "xp.import-confirm", mode = label),
                    format!("xi:go:{guild}:{actor}:{mode}"),
                )
                .await
            }
        }
    }
}

async fn edit(
    cx: &Cx,
    guild: u64,
    target: ResolvedUser,
    amount: i64,
    kind: XpEdit,
    action: &str,
) -> anyhow::Result<()> {
    if !(0..=1_000_000_000).contains(&amount) {
        return cx.reply_ephemeral(t!(cx.lang(), "xp.invalid-amount")).await;
    }
    let user = target.resolved.id.get();
    let (old, new) = cx.app.db.edit_member_xp(guild, user, kind, amount).await?;
    audit(cx, guild, action, Some(user.to_string()), old, new).await?;
    xp_admin::sync_member_roles(&cx.app, guild, user).await?;
    cx.reply_ephemeral(t!(cx.lang(), "xp.changed", user = user, old = old, new = new))
        .await
}

async fn audit(cx: &Cx, guild: u64, action: &str, target: Option<String>, old: i64, new: i64) -> anyhow::Result<()> {
    let before = serde_json::json!({ "xp": old }).to_string();
    let after = serde_json::json!({ "xp": new }).to_string();
    cx.app
        .db
        .audit(&NewAudit {
            guild,
            actor: cx.author()?.id.get(),
            actor_name: &cx.author_display_name(),
            source: AuditSource::Discord,
            action,
            target: target.as_deref(),
            before: Some(&before),
            after: Some(&after),
        })
        .await?;
    Ok(())
}

pub(crate) async fn on_component<'a>(cx: &Cx, mut parts: impl Iterator<Item = &'a str>) -> anyhow::Result<()> {
    if !cx.can_manage_guild() || cx.guild_id().is_none() {
        return cx.reply_ephemeral(t!(cx.lang(), "xp.permission")).await;
    }
    if parts.next() != Some("confirm") {
        return cx.reply_ephemeral(t!(cx.lang(), "xp.expired")).await;
    }
    let nonce = parts.next().unwrap_or("");
    if nonce.len() != 32 || !nonce.bytes().all(|b| b.is_ascii_hexdigit()) || parts.next().is_some() {
        return cx.reply_ephemeral(t!(cx.lang(), "xp.expired")).await;
    }
    let key = format!(
        "xp:confirm:{}:{}:{nonce}",
        cx.guild_id().unwrap_or_default(),
        cx.author()?.id
    );
    // GETDEL makes every stage single use, even across pods. The next stage is only minted here.
    let Some(payload) = cx.app.cache.take(&key).await? else {
        return cx.reply_ephemeral(t!(cx.lang(), "xp.expired")).await;
    };
    let mut parts = payload.split(':').skip(1);
    let action = parts.next().unwrap_or("");
    let next = parts.next().unwrap_or("");
    let (guild, actor, extra) = if action == "all" {
        (parts.next(), parts.next(), None)
    } else {
        (Some(next), parts.next(), parts.next())
    };
    let guild = guild.and_then(|v| v.parse::<u64>().ok());
    let actor = actor.and_then(|v| v.parse::<u64>().ok());
    if guild != cx.guild_id() || actor != Some(cx.author()?.id.get()) || !cx.can_manage_guild() {
        return cx.reply_ephemeral(t!(cx.lang(), "xp.permission")).await;
    }
    let guild = guild.unwrap_or_default();
    if action == "all" && next == "1" {
        return cx
            .update(InteractionResponseData {
                content: Some(t!(cx.lang(), "xp.all-confirm-second")),
                components: Some(buttons(&[(
                    &t!(cx.lang(), "xp.confirm"),
                    confirmation_id(cx, format!("xi:all:2:{guild}:{}", actor.unwrap_or_default())).await?,
                    ButtonStyle::Danger,
                )])),
                ..Default::default()
            })
            .await;
    }
    let text = match action {
        "all" if next == "2" => {
            let users = cx.app.db.xp_member_ids(guild).await?;
            let changed = cx.app.db.reset_all_member_xp(guild).await?;
            audit(cx, guild, "xp.reset_all", None, changed as i64, 0).await?;
            xp_admin::sync_members_roles(&cx.app, guild, &users).await?;
            t!(cx.lang(), "xp.all-done", count = changed)
        }
        "reset" => {
            let Some(user) = extra.and_then(|v| v.parse::<u64>().ok()) else {
                return cx.reply_ephemeral(t!(cx.lang(), "xp.expired")).await;
            };
            let (old, new) = cx.app.db.edit_member_xp(guild, user, XpEdit::Set, 0).await?;
            audit(cx, guild, "xp.reset", Some(user.to_string()), old, new).await?;
            xp_admin::sync_member_roles(&cx.app, guild, user).await?;
            t!(cx.lang(), "xp.changed", user = user, old = old, new = new)
        }
        "go" => {
            let Some(mode) = extra.and_then(ImportMode::parse) else {
                return cx.reply_ephemeral(t!(cx.lang(), "xp.expired")).await;
            };
            cx.responder.defer(true).await?;
            let actor_name = cx.author_display_name();
            match xp_admin::import_mee6(
                &cx.app,
                xp_admin::ImportRequest {
                    guild,
                    actor: actor.unwrap_or_default(),
                    actor_name: &actor_name,
                    source: AuditSource::Discord,
                    mode,
                },
                |_| {},
            )
            .await
            {
                Ok(report) => t!(cx.lang(), "xp.import-done", count = report.updated),
                Err(error) => {
                    tracing::warn!(%error, guild, "MEE6 import failed");
                    t!(cx.lang(), "xp.import-error")
                }
            }
        }
        _ => return cx.reply_ephemeral(t!(cx.lang(), "xp.expired")).await,
    };
    cx.update(InteractionResponseData {
        content: Some(text),
        components: Some(vec![]),
        ..Default::default()
    })
    .await
}
