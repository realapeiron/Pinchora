//! `/config critters`: whether chat in a server makes critter drops and bottles wash up, and where.

use std::{sync::LazyLock, time::Duration};

use moka::future::Cache as Moka;
use pinchora_core::{i18n::Lang, t};
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
pub(crate) fn critters_summary(s: &CollectionSettings, lang: Lang) -> String {
    match (s.enabled, s.channel_id) {
        (false, _) => t!(lang, "collect.summary-off"),
        (true, Some(channel)) => t!(lang, "collect.summary-channel", channel = channel),
        (true, None) => t!(lang, "collect.summary-anywhere"),
    }
}

impl ConfigCritters {
    pub async fn run(self, cx: &Cx, guild: u64) -> anyhow::Result<()> {
        if self.channel.is_some() && self.anywhere == Some(true) {
            return tell(cx, t!(cx.lang(), "collect.both-set")).await;
        }
        let channel = match (self.channel, self.anywhere) {
            (Some(channel), _) => Some(Some(channel.id.get())),
            (None, Some(true)) => Some(None),
            _ => None,
        };
        let settings = cx.app.db.set_collection_settings(guild, self.enabled, channel).await?;
        forget(guild).await;
        let summary = critters_summary(&settings, cx.lang());
        tell(cx, t!(cx.lang(), "collect.setting", summary = summary)).await
    }
}
