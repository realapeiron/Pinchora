//! `/config prefix`: what this server's text commands start with (`p!profile`).

use pinchora_core::settings::{self, DEFAULT_PREFIX};
use pinchora_db::{AuditSource, NewAudit};
use serde_json::json;
use twilight_interactions::command::{CommandModel, CreateCommand};

use crate::{Cx, commands::modules::announce_guild_change};

#[derive(CommandModel, CreateCommand)]
#[command(name = "prefix", desc = "What text commands start with, e.g. p!profile")]
pub enum ConfigPrefix {
    #[command(name = "set")]
    Set(PrefixSet),
    #[command(name = "reset")]
    Reset(PrefixReset),
    #[command(name = "show")]
    Show(PrefixShow),
}

#[derive(CommandModel, CreateCommand)]
#[command(name = "set", desc = "Pick the prefix (1-5 characters, no spaces)")]
pub struct PrefixSet {
    /// The new prefix, e.g. ! or z!
    #[command(min_length = 1, max_length = 5)]
    prefix: String,
}

#[derive(CommandModel, CreateCommand)]
#[command(name = "reset", desc = "Go back to the default prefix, p!")]
pub struct PrefixReset;

#[derive(CommandModel, CreateCommand)]
#[command(name = "show", desc = "Show the prefix")]
pub struct PrefixShow;

/// How the settings and the answers word the prefix, with what members need to know to use it.
pub(crate) fn prefix_summary(prefix: &str, message_content: bool) -> String {
    if message_content {
        format!("`{prefix}`, e.g. `{prefix}profile` · `@Pinchora profile` works too")
    } else {
        format!(
            "`{prefix}` · text commands need `@Pinchora profile` until the bot owner turns on the Message Content intent"
        )
    }
}

impl ConfigPrefix {
    pub async fn run(self, cx: &Cx, guild: u64) -> anyhow::Result<()> {
        let app = &cx.app;
        let before = app.db.guild_settings(guild).await?;
        let wanted = match self {
            Self::Show(_) => {
                let text = format!(
                    "Text commands start with {}",
                    prefix_summary(before.prefix(), app.message_content)
                );
                return cx.reply_ephemeral(text).await;
            }
            Self::Reset(_) => None,
            Self::Set(c) => match settings::check_prefix(&c.prefix) {
                Ok(prefix) => Some(prefix).filter(|p| p != DEFAULT_PREFIX),
                Err(error) => return cx.reply_ephemeral(error.message).await,
            },
        };
        let after = app.db.set_prefix(guild, wanted.as_deref()).await?;
        if after.prefix != before.prefix {
            announce_guild_change(app, guild).await;
            let actor = cx.author()?;
            let (old, new) = (
                json!({ "prefix": before.prefix() }).to_string(),
                json!({ "prefix": after.prefix() }).to_string(),
            );
            if let Err(error) = app
                .db
                .audit(&NewAudit {
                    guild,
                    actor: actor.id.get(),
                    actor_name: &cx.author_display_name(),
                    source: AuditSource::Discord,
                    action: "settings.patch",
                    target: None,
                    before: Some(&old),
                    after: Some(&new),
                })
                .await
            {
                tracing::error!(error = ?error, guild, "a prefix change was made but not written to the audit log");
            }
        }
        let text = format!(
            "Text commands now start with {}",
            prefix_summary(after.prefix(), app.message_content)
        );
        cx.reply_ephemeral(text).await
    }
}
