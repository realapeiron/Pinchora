//! Manage Server command controls and configuration recovery.
use crate::Cx;
use pinchora_core::{
    config_backup::{Import, Policy},
    t,
};
use twilight_interactions::command::{CommandModel, CreateCommand};
use twilight_model::{
    channel::message::{
        Component, MessageFlags,
        component::{ActionRow, Button, ButtonStyle},
    },
    guild::Permissions,
    http::{attachment::Attachment, interaction::InteractionResponseData},
};
fn manage_guild() -> Permissions {
    Permissions::MANAGE_GUILD
}
#[derive(CommandModel, CreateCommand)]
#[command(
    name = "commandcontrol",
    desc = "Command permissions, cooldowns and prefix aliases",
    default_permissions = "manage_guild",
    contexts = "guild"
)]
pub enum CommandControl {
    #[command(name = "show")]
    Show(Show),
    #[command(name = "set")]
    Set(Box<Set>),
    #[command(name = "reset")]
    Reset(Reset),
    #[command(name = "alias")]
    Alias(Alias),
}
#[derive(CommandModel, CreateCommand)]
#[command(name = "show", desc = "Show command control settings")]
pub struct Show;
#[derive(CommandModel, CreateCommand)]
#[command(name = "set", desc = "Set additional restrictions for a command or subcommand")]
pub struct Set {
    /// Command path, for example play wordle
    command: String,
    /// Enable or disable this command
    enabled: Option<bool>,
    /// Additional cooldown in seconds (0 resets it)
    #[command(min_value = 0, max_value = 86400)]
    cooldown: Option<i64>,
    /// Delete bot replies after this many seconds (0 disables deletion)
    #[command(min_value = 0, max_value = 840)]
    delete_reply: Option<i64>,
    /// Delete prefix invocations after this many seconds (0 disables deletion)
    #[command(min_value = 0, max_value = 86400)]
    delete_invocation: Option<i64>,
    /// Reply privately by default where supported
    ephemeral: Option<bool>,
    /// Add a role allowed to use the command
    allow_role: Option<twilight_model::guild::Role>,
    /// Add a role denied access to the command
    deny_role: Option<twilight_model::guild::Role>,
    /// Add an allowed channel or category
    allow_channel: Option<twilight_model::application::interaction::InteractionChannel>,
    /// Add a denied channel or category
    deny_channel: Option<twilight_model::application::interaction::InteractionChannel>,
}
#[derive(CommandModel, CreateCommand)]
#[command(name = "reset", desc = "Remove a command override")]
pub struct Reset {
    /// Command path
    command: String,
}
#[derive(CommandModel, CreateCommand)]
#[command(name = "alias", desc = "Add a prefix alias, or remove it by omitting the command")]
pub struct Alias {
    /// Shortcut name
    name: String,
    /// Target command path
    command: Option<String>,
}
async fn managed(cx: &Cx) -> anyhow::Result<Option<u64>> {
    if !cx.can_manage_guild() || cx.guild_id().is_none() {
        cx.reply_ephemeral(t!(cx.lang(), "command_control.manage")).await?;
        return Ok(None);
    }
    Ok(cx.guild_id())
}
impl CommandControl {
    pub async fn run(self, cx: &Cx) -> anyhow::Result<()> {
        let Some(guild) = managed(cx).await? else {
            return Ok(());
        };
        let mut config = cx.app.db.command_control(guild).await?;
        let before = serde_json::to_string(&config)?;
        match self {
            Self::Show(_) => {
                return cx
                    .reply(InteractionResponseData {
                        content: Some(t!(
                            cx.lang(),
                            "command_control.show",
                            rules = config.rules.len(),
                            aliases = config.aliases.len()
                        )),
                        flags: Some(MessageFlags::EPHEMERAL),
                        components: Some(vec![Component::ActionRow(ActionRow {
                            id: None,
                            components: vec![Component::Button(Button {
                                id: None,
                                custom_id: Some("kc:show".into()),
                                disabled: false,
                                label: Some(t!(cx.lang(), "command_control.ui.refresh")),
                                style: ButtonStyle::Secondary,
                                emoji: None,
                                url: None,
                                sku_id: None,
                            })],
                        })]),
                        ..Default::default()
                    })
                    .await;
            }
            Self::Set(c) => {
                let rule = config
                    .rules
                    .entry(
                        crate::routing::canonical(&c.command.trim().trim_start_matches('/').to_lowercase()).to_owned(),
                    )
                    .or_default();
                if let Some(v) = c.enabled {
                    rule.enabled = v;
                }
                if let Some(v) = c.cooldown {
                    rule.cooldown_secs = v as u32;
                }
                if let Some(v) = c.delete_reply {
                    rule.delete_reply_secs = v as u32;
                }
                if let Some(v) = c.delete_invocation {
                    rule.delete_invocation_secs = v as u32;
                }
                if let Some(v) = c.ephemeral {
                    rule.ephemeral = v;
                }
                for (list, id) in [
                    (&mut rule.allowed_roles, c.allow_role.map(|r| r.id.to_string())),
                    (&mut rule.denied_roles, c.deny_role.map(|r| r.id.to_string())),
                    (&mut rule.allowed_channels, c.allow_channel.map(|c| c.id.to_string())),
                    (&mut rule.denied_channels, c.deny_channel.map(|c| c.id.to_string())),
                ] {
                    if let Some(id) = id
                        && !list.contains(&id)
                    {
                        list.push(id);
                    }
                }
            }
            Self::Reset(c) => {
                let path = c.command.trim().trim_start_matches('/').to_lowercase();
                config.rules.remove(crate::routing::canonical(&path));
                config.rules.remove(crate::routing::legacy(&path));
            }
            Self::Alias(c) => {
                let name = c.name.trim().to_lowercase();
                if let Some(path) = c.command {
                    config.aliases.insert(
                        name,
                        crate::routing::canonical(&path.trim().trim_start_matches('/').to_lowercase()).to_owned(),
                    );
                } else {
                    config.aliases.remove(&name);
                }
            }
        }
        if let Err(error) = config.validate_update(
            &cx.app.db.command_control(guild).await?,
            &crate::control_catalog(),
            &crate::command_reserved_aliases(),
            cx.lang(),
        ) {
            return cx.reply_ephemeral(error).await;
        }
        cx.app.db.set_command_control(guild, &config).await?;
        cx.app.announce_guild_change(guild).await;
        let after = serde_json::to_string(&config)?;
        if let Err(error) = cx
            .app
            .db
            .audit(&pinchora_db::NewAudit {
                guild,
                actor: cx.author()?.id.get(),
                actor_name: &cx.author_display_name(),
                source: pinchora_db::AuditSource::Discord,
                action: "command_control.set",
                target: None,
                before: Some(&before),
                after: Some(&after),
            })
            .await
        {
            tracing::error!(%error,"command control audit failed");
        }
        cx.reply_ephemeral(t!(cx.lang(), "command_control.saved")).await
    }
}
#[derive(CommandModel, CreateCommand)]
#[command(
    name = "backup",
    desc = "Export configuration and preview recovery",
    default_permissions = "manage_guild",
    contexts = "guild"
)]
pub enum BackupCommand {
    #[command(name = "export")]
    Export(Export),
    #[command(name = "list")]
    List(List),
    #[command(name = "restore")]
    Restore(Restore),
    #[command(name = "import")]
    Import(ImportHelp),
}
#[derive(CommandModel, CreateCommand)]
#[command(name = "export", desc = "Download a versioned JSON configuration backup")]
pub struct Export;
#[derive(CommandModel, CreateCommand)]
#[command(name = "list", desc = "List the last ten automatic restore points")]
pub struct List;
#[derive(CommandModel, CreateCommand)]
#[command(name = "restore", desc = "Preview a restore point before applying it")]
pub struct Restore {
    /// Restore point ID
    id: String,
}
#[derive(CommandModel, CreateCommand)]
#[command(name = "import", desc = "Open the dashboard to preview and import a JSON backup")]
pub struct ImportHelp;
impl BackupCommand {
    pub async fn run(self, cx: &Cx) -> anyhow::Result<()> {
        let Some(guild) = managed(cx).await? else {
            return Ok(());
        };
        match self {
            Self::Export(_) => {
                crate::backup_rate_limit(&cx.app, guild, "export", 10, cx.lang()).await?;
                let names = crate::backup_names(&cx.app, guild).await?;
                let backup = cx.app.db.export_config(guild, names).await?;
                let bytes = serde_json::to_vec_pretty(&backup)?;
                if bytes.len() as u64 > cx.interaction.attachment_size_limit {
                    return cx
                        .reply_ephemeral(t!(
                            cx.lang(),
                            "config_backup.too_large_discord",
                            url = format!("{}/g/{guild}/backup", cx.app.public_url)
                        ))
                        .await;
                }
                cx.reply(InteractionResponseData {
                    content: Some(t!(cx.lang(), "config_backup.ui.export_hint")),
                    flags: Some(MessageFlags::EPHEMERAL),
                    attachments: Some(vec![Attachment::from_bytes(
                        format!("pinchora-{guild}-v1.json"),
                        bytes,
                        1,
                    )]),
                    ..Default::default()
                })
                .await
            }
            Self::List(_) => {
                let points = cx.app.db.config_restore_points(guild).await?;
                let body = if points.is_empty() {
                    t!(cx.lang(), "config_backup.ui.no_points")
                } else {
                    points
                        .iter()
                        .map(|p| {
                            format!(
                                "`{}` · {}",
                                p["id"].as_str().unwrap_or_default(),
                                p["created_at"].as_str().unwrap_or_default()
                            )
                        })
                        .collect::<Vec<_>>()
                        .join("\n")
                };
                cx.reply_ephemeral(body).await
            }
            Self::Import(_) => {
                cx.reply_ephemeral(t!(
                    cx.lang(),
                    "config_backup.show",
                    url = format!("{}/g/{guild}/backup", cx.app.public_url)
                ))
                .await
            }
            Self::Restore(c) => {
                let Some(point) = c.id.parse::<i64>().ok() else {
                    return cx.reply_ephemeral(t!(cx.lang(), "config_backup.invalid")).await;
                };
                let Some(backup) = cx.app.db.config_restore_point(guild, point).await? else {
                    return cx.reply_ephemeral(t!(cx.lang(), "config_backup.invalid")).await;
                };
                let (_, preview) = crate::backup_preview(
                    &cx.app,
                    guild,
                    &Import {
                        backup,
                        policy: Policy::Replace,
                    },
                    cx.lang(),
                    cx.author()?.id.get(),
                )
                .await?;
                cx.reply(InteractionResponseData {
                    content: Some(t!(cx.lang(), "config_backup.ui.ready")),
                    flags: Some(MessageFlags::EPHEMERAL),
                    attachments: Some(vec![Attachment::from_bytes(
                        "preview.json".into(),
                        serde_json::to_vec_pretty(&preview)?,
                        1,
                    )]),
                    components: Some(vec![Component::ActionRow(ActionRow {
                        id: None,
                        components: vec![Component::Button(Button {
                            id: None,
                            custom_id: Some(format!("cf:restore:{point}")),
                            disabled: !preview.can_import,
                            label: Some(t!(cx.lang(), "config_backup.ui.apply")),
                            style: ButtonStyle::Danger,
                            emoji: None,
                            url: None,
                            sku_id: None,
                        })],
                    })]),
                    ..Default::default()
                })
                .await
            }
        }
    }
}
pub(crate) async fn on_component(cx: &Cx, mut parts: std::str::Split<'_, char>) -> anyhow::Result<()> {
    let Some(guild) = managed(cx).await? else {
        return Ok(());
    };
    let Some(id) = parts
        .next()
        .filter(|a| *a == "restore")
        .and_then(|_| parts.next())
        .and_then(|s| s.parse::<i64>().ok())
    else {
        return cx.reply_ephemeral(t!(cx.lang(), "config_backup.invalid")).await;
    };
    let Some(backup) = cx.app.db.config_restore_point(guild, id).await? else {
        return cx.reply_ephemeral(t!(cx.lang(), "config_backup.invalid")).await;
    };
    let saved = crate::backup_apply(
        &cx.app,
        guild,
        &Import {
            backup,
            policy: Policy::Replace,
        },
        cx.lang(),
        cx.author()?.id.get(),
    )
    .await?;
    let after = serde_json::json!({"restore_point": saved.to_string(), "restored_from": id.to_string()}).to_string();
    cx.app
        .db
        .audit(&pinchora_db::NewAudit {
            guild,
            actor: cx.author()?.id.get(),
            actor_name: &cx.author_display_name(),
            source: pinchora_db::AuditSource::Discord,
            action: "backup.restore",
            target: None,
            before: None,
            after: Some(&after),
        })
        .await?;
    cx.update(InteractionResponseData {
        content: Some(t!(cx.lang(), "config_backup.saved")),
        components: Some(vec![]),
        attachments: Some(vec![]),
        ..Default::default()
    })
    .await
}

pub(crate) async fn on_control_component(cx: &Cx) -> anyhow::Result<()> {
    CommandControl::Show(Show).run(cx).await
}
