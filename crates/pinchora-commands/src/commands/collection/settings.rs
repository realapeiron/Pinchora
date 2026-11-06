//! `/config critters`: whether chat in a server makes critter drops and bottles wash up, and where.

use std::{sync::LazyLock, time::Duration};

use moka::future::Cache as Moka;
use pinchora_db::CollectionSettings;
use twilight_interactions::command::{CommandModel, CreateCommand};
use twilight_model::application::interaction::InteractionChannel;

use crate::{App, Cx, server_events::tell};

#[derive(CommandModel, CreateCommand)]
#[command(name = "critters", desc = "Critter drops and bottles that wash up while people chat")]
pub struct ConfigCritters {
    /// Turn chat drops on or off (they start off)
    enabled: Option<bool>,
    /// Drops land here, and only chat here counts towards them
    #[command(channel_types = "guild_text guild_announcement")]
    channel: Option<InteractionChannel>,
    /// Let drops land in whichever channel is busy, instead of a set one
    anywhere: Option<bool>,
}

/// Settings kept for a minute, like a server's other settings: chat checks them on every message.
static SETTINGS: LazyLock<Moka<u64, CollectionSettings>> = LazyLock::new(|| {
    Moka::builder()
        .max_capacity(50_000)
        .time_to_live(Duration::from_secs(60))
        .build()
});

/// A server's settings, from the cache when it is fresh.
pub(crate) async fn settings(app: &App, guild: u64) -> anyhow::Result<CollectionSettings> {
    SETTINGS
        .try_get_with(guild, app.db.collection_settings(guild))
        .await
        .map_err(|e| anyhow::anyhow!("{e:#}"))
}

/// Drops this pod's copy of a server's settings (the dashboard announces its changes).
pub(crate) async fn forget(guild: u64) {
    SETTINGS.invalidate(&guild).await;
}

/// How `/config show` and `/config critters` word the setting.
pub(crate) fn critters_summary(s: &CollectionSettings) -> String {
    match (s.enabled, s.channel_id) {
        (false, _) => "**off** · turn it on with `/config critters enabled:true`".to_owned(),
        (true, Some(channel)) => format!("**on** · drops land in <#{channel}>, where chat counts"),
        (true, None) => "**on** · drops land in whichever channel is busy".to_owned(),
    }
}

impl ConfigCritters {
    pub async fn run(self, cx: &Cx, guild: u64) -> anyhow::Result<()> {
        if self.channel.is_some() && self.anywhere == Some(true) {
            return tell(cx, "Pick a channel or `anywhere`, not both.").await;
        }
        let channel = match (self.channel, self.anywhere) {
            (Some(channel), _) => Some(Some(channel.id.get())),
            (None, Some(true)) => Some(None),
            _ => None,
        };
        let settings = cx.app.db.set_collection_settings(guild, self.enabled, channel).await?;
        forget(guild).await;
        tell(cx, format!("Critter drops: {}", critters_summary(&settings))).await
    }
}
