//! `/verify`, `/verification` and `/raid` use the same service as the Safety dashboard.
use crate::{Cx, verification};
use pinchora_core::t;
use twilight_interactions::command::{CommandModel, CreateCommand};
use twilight_model::{
    application::interaction::InteractionChannel,
    guild::{Permissions, Role},
};
fn manage_guild() -> Permissions {
    Permissions::MANAGE_GUILD
}
#[derive(CommandModel, CreateCommand)]
#[command(name = "verify", desc = "Pass this server's join gate", contexts = "guild")]
pub struct Verify;
impl Verify {
    pub async fn run(self, cx: &Cx) -> anyhow::Result<()> {
        verification::begin(cx).await
    }
}
#[derive(CommandModel, CreateCommand)]
#[command(
    name = "verification",
    desc = "Configure the join gate and verification message",
    default_permissions = "manage_guild",
    contexts = "guild"
)]
#[allow(clippy::large_enum_variant)] // Discord command derives require concrete subcommand models.
pub enum Verification {
    #[command(name = "setup")]
    Setup(Setup),
    #[command(name = "mode")]
    Mode(Mode),
    #[command(name = "message")]
    Message(Message),
    #[command(name = "post")]
    Post(Post),
}
#[derive(CommandModel, CreateCommand)]
#[command(name = "setup", desc = "Enable or disable the gate and choose safe roles")]
pub struct Setup {
    /// Enable the join gate
    enabled: bool,
    /// Role granted after verification (keep onboarding permissions restricted until then)
    verified_role: Option<Role>,
    /// Role removed after verification
    unverified_role: Option<Role>,
    /// Channel for the verification panel
    channel: Option<InteractionChannel>,
    /// Optional kick timeout in minutes; zero disables it
    #[command(min_value = 0, max_value = 10080)]
    kick_after: Option<i64>,
}
#[derive(CommandModel, CreateCommand)]
#[command(name = "mode", desc = "Choose button, captcha or web verification")]
pub struct Mode {
    /// button, captcha or web
    mode: String,
}
#[derive(CommandModel, CreateCommand)]
#[command(name = "message", desc = "Set the message displayed above the verification button")]
pub struct Message {
    /// Message to display
    #[command(max_length = 1500)]
    text: String,
}
#[derive(CommandModel, CreateCommand)]
#[command(name = "post", desc = "Post the verification panel to the configured channel")]
pub struct Post;
impl Verification {
    pub async fn run(self, cx: &Cx) -> anyhow::Result<()> {
        if !cx.can_manage_guild() {
            return cx.reply_ephemeral(t!(cx.lang(), "verification.manage")).await;
        }
        let Some(guild) = cx.guild_id() else {
            return cx.reply_ephemeral(t!(cx.lang(), "verification.guild_only")).await;
        };
        let mut c = cx.app.db.safety_config(guild).await?;
        match self {
            Self::Post(_) => {
                verification::post(&cx.app, guild).await?;
                return cx.reply_ephemeral(t!(cx.lang(), "verification.posted")).await;
            }
            Self::Setup(s) => {
                c.verification.enabled = s.enabled;
                if let Some(r) = s.verified_role {
                    c.verification.verified_role_id = Some(r.id.to_string())
                }
                if let Some(r) = s.unverified_role {
                    c.verification.unverified_role_id = Some(r.id.to_string())
                }
                if let Some(ch) = s.channel {
                    c.verification.channel_id = Some(ch.id.to_string())
                }
                if let Some(m) = s.kick_after {
                    c.verification.kick_after_mins = m as u32
                }
            }
            Self::Mode(m) => c.verification.mode = m.mode,
            Self::Message(m) => c.verification.message = m.text,
        }
        if let Err(error) = verification::check_roles(&cx.app, guild, cx.author()?.id.get(), &c).await {
            return cx.reply_ephemeral(error.to_string()).await;
        }
        cx.app.db.save_safety(guild, &c).await?;
        cx.reply_ephemeral(t!(cx.lang(), "verification.saved")).await
    }
}
#[derive(CommandModel, CreateCommand)]
#[command(
    name = "raid",
    desc = "Start, end or inspect raid protection",
    default_permissions = "manage_guild",
    contexts = "guild"
)]
pub enum Raid {
    #[command(name = "lockdown")]
    Lockdown(Lockdown),
    #[command(name = "unlock")]
    Unlock(Unlock),
    #[command(name = "status")]
    Status(Status),
}
#[derive(CommandModel, CreateCommand)]
#[command(name = "lockdown", desc = "Force captcha and temporarily restrict the server")]
pub struct Lockdown {
    /// Minutes before restoring settings automatically
    #[command(min_value = 1, max_value = 1440)]
    minutes: Option<i64>,
}
#[derive(CommandModel, CreateCommand)]
#[command(name = "unlock", desc = "End the raid and restore the settings it changed")]
pub struct Unlock;
#[derive(CommandModel, CreateCommand)]
#[command(name = "status", desc = "Show the live raid state")]
pub struct Status;
impl Raid {
    pub async fn run(self, cx: &Cx) -> anyhow::Result<()> {
        if !cx.can_manage_guild() {
            return cx.reply_ephemeral(t!(cx.lang(), "verification.manage")).await;
        }
        let Some(guild) = cx.guild_id() else {
            return cx.reply_ephemeral(t!(cx.lang(), "verification.guild_only")).await;
        };
        let actor = cx.author()?.id.get();
        let result = match self {
            Self::Lockdown(l) => {
                verification::raid::start(
                    &cx.app,
                    guild,
                    Some(actor),
                    l.minutes.unwrap_or(10) as u32,
                    0,
                    "lockdown",
                )
                .await
            }
            Self::Unlock(_) => verification::raid::unlock(&cx.app, guild, actor, None).await,
            Self::Status(_) => {
                let text = match cx.app.db.active_raid(guild).await? {
                    Some(e) => t!(
                        cx.lang(),
                        "anti_raid.status",
                        id = e.id,
                        until = e.expires_at.timestamp(),
                        error = e.error.unwrap_or_default()
                    ),
                    None => t!(cx.lang(), "anti_raid.calm"),
                };
                return cx.reply_ephemeral(text).await;
            }
        };
        cx.reply_ephemeral(match result {
            Ok(()) => t!(cx.lang(), "verification.done"),
            Err(e) => e.to_string(),
        })
        .await
    }
}
