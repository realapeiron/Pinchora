use pinchora_core::t;
use pinchora_db::{AuditSource, GuildBranding, NewAudit};
use twilight_interactions::command::{CommandModel, CreateCommand};
use twilight_model::guild::Permissions;

use crate::{
    Cx,
    branding::{self, ApplyResult},
};

fn manage_guild() -> Permissions {
    Permissions::MANAGE_GUILD
}

#[derive(CommandModel, CreateCommand)]
#[command(
    name = "branding",
    desc = "Server profile and embed colour",
    default_permissions = "manage_guild",
    contexts = "guild"
)]
pub enum BrandingCommand {
    #[command(name = "show")]
    Show(Show),
    #[command(name = "apply")]
    Apply(Apply),
    #[command(name = "reset")]
    Reset(Reset),
}

#[derive(CommandModel, CreateCommand)]
#[command(name = "show", desc = "Show this server's bot profile and colour")]
pub struct Show;

#[derive(CommandModel, CreateCommand)]
#[command(name = "apply", desc = "Apply the server profile and embed colour")]
pub struct Apply {
    /// The server nickname (up to 32 characters)
    pub nickname: Option<String>,
    /// Avatar upload ID from the dashboard
    pub avatar: Option<String>,
    /// Banner upload ID from the dashboard
    pub banner: Option<String>,
    /// Profile bio (up to 190 characters)
    pub bio: Option<String>,
    /// Embed colour as #RRGGBB
    pub colour: Option<String>,
}

#[derive(CommandModel, CreateCommand)]
#[command(name = "reset", desc = "Clear this server's branding")]
pub struct Reset;

impl BrandingCommand {
    pub async fn run(self, cx: &Cx) -> anyhow::Result<()> {
        let Some(guild) = cx.guild_id().filter(|_| cx.can_manage_guild()) else {
            return cx.reply_ephemeral(t!(cx.lang(), "config.needs-manage-server")).await;
        };
        let before = cx.app.db.guild_branding(guild).await?;
        match self {
            Self::Show(_) => {
                let color = format!(
                    "#{:06X}",
                    before.active_embed_color.unwrap_or(crate::util::BRAND as i32)
                );
                cx.reply_ephemeral(t!(
                    cx.lang(),
                    "branding.show",
                    nick = before.active_nickname.as_deref().unwrap_or("—"),
                    color = color,
                    bio = before.active_bio.as_deref().unwrap_or("—")
                ))
                .await
            }
            Self::Apply(input) => {
                let mut row = before.clone();
                if let Some(v) = input.nickname {
                    row.nickname = Some(v);
                }
                if let Some(v) = input.avatar {
                    row.avatar_upload_id = Some(v);
                }
                if let Some(v) = input.banner {
                    row.banner_upload_id = Some(v);
                }
                if let Some(v) = input.bio {
                    row.bio = Some(v);
                }
                if let Some(v) = input.colour {
                    let Some(color) = branding::parse_color(&v) else {
                        return cx.reply_ephemeral(t!(cx.lang(), "branding.error.colour")).await;
                    };
                    row.embed_color = Some(color);
                }
                if let Err(error) = branding::validate(&row) {
                    return cx
                        .reply_ephemeral(t!(cx.lang(), &format!("branding.error.{error}")))
                        .await;
                }
                for upload in [&row.avatar_upload_id, &row.banner_upload_id].into_iter().flatten() {
                    if cx.app.db.card_upload(guild, upload).await?.is_none() {
                        return cx.reply_ephemeral(t!(cx.lang(), "branding.error.upload")).await;
                    }
                }
                cx.app.db.save_guild_branding(guild, &row).await?;
                audit(cx, guild, "branding.save", &before, &row).await?;
                let outcome = branding::apply(&cx.app, guild, &row).await?;
                if !matches!(outcome, ApplyResult::Cooldown) {
                    audit(
                        cx,
                        guild,
                        "branding.apply",
                        &before,
                        &cx.app.db.guild_branding(guild).await?,
                    )
                    .await?;
                }
                let message = match outcome {
                    ApplyResult::Applied => t!(cx.lang(), "branding.applied"),
                    ApplyResult::Cooldown => t!(cx.lang(), "branding.cooldown"),
                    ApplyResult::Partial(fields) => t!(
                        cx.lang(),
                        "branding.partial",
                        fields = fields
                            .into_iter()
                            .map(|field| t!(cx.lang(), &format!("branding.field.{field}")))
                            .collect::<Vec<_>>()
                            .join(", ")
                    ),
                };
                cx.reply_ephemeral(message).await
            }
            Self::Reset(_) => {
                let result = branding::reset(&cx.app, guild).await?;
                if matches!(result, ApplyResult::Cooldown) {
                    return cx.reply_ephemeral(t!(cx.lang(), "branding.cooldown")).await;
                }
                audit(
                    cx,
                    guild,
                    "branding.reset",
                    &before,
                    &cx.app.db.guild_branding(guild).await?,
                )
                .await?;
                cx.reply_ephemeral(if matches!(result, ApplyResult::Partial(_)) {
                    t!(cx.lang(), "branding.reset-partial")
                } else {
                    t!(cx.lang(), "branding.reset")
                })
                .await
            }
        }
    }
}

async fn audit(cx: &Cx, guild: u64, action: &str, before: &GuildBranding, after: &GuildBranding) -> anyhow::Result<()> {
    let previous = serde_json::json!({"nickname": before.nickname, "avatar_upload_id": before.avatar_upload_id, "banner_upload_id": before.banner_upload_id, "bio": before.bio, "embed_color": before.embed_color, "active_nickname": before.active_nickname, "active_avatar_upload_id": before.active_avatar_upload_id, "active_banner_upload_id": before.active_banner_upload_id, "active_bio": before.active_bio, "last_error": before.last_error, "active_embed_color": before.active_embed_color}).to_string();
    let next = serde_json::json!({"nickname": after.nickname, "avatar_upload_id": after.avatar_upload_id, "banner_upload_id": after.banner_upload_id, "bio": after.bio, "embed_color": after.embed_color, "active_nickname": after.active_nickname, "active_avatar_upload_id": after.active_avatar_upload_id, "active_banner_upload_id": after.active_banner_upload_id, "active_bio": after.active_bio, "last_error": after.last_error, "active_embed_color": after.active_embed_color}).to_string();
    cx.app
        .db
        .audit(&NewAudit {
            guild,
            actor: cx.author()?.id.get(),
            actor_name: &cx.author()?.name,
            source: AuditSource::Discord,
            action,
            target: None,
            before: Some(&previous),
            after: Some(&next),
        })
        .await?;
    Ok(())
}
