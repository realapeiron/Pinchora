//! `/config partygames`: how strict the word filter is for what members type into the party games.

use std::{sync::LazyLock, time::Duration};

use moka::future::Cache as Moka;
use pinchora_core::{i18n::Lang, t};
use twilight_interactions::command::{CommandModel, CreateCommand};

use crate::{App, Cx, server_events::tell};

#[derive(CommandModel, CreateCommand)]
#[command(
    name = "partygames",
    desc = "How strict the word filter is in Hot Pearl, acronyms and captions"
)]
pub struct ConfigPartyGames {
    /// Also block mild words and name-calling (strong words and slurs are always blocked)
    strict: Option<bool>,
}

/// Settings kept for a minute: every phrase and caption looks them up.
static STRICT: LazyLock<Moka<u64, bool>> = LazyLock::new(|| {
    Moka::builder()
        .max_capacity(50_000)
        .time_to_live(Duration::from_secs(60))
        .build()
});

/// Whether a server wants the strict filter. A failed lookup counts as not strict.
pub(crate) async fn strict(app: &App, guild: u64) -> bool {
    STRICT
        .try_get_with(guild, app.db.party_strict(guild))
        .await
        .unwrap_or(false)
}

/// How `/config show` and `/config partygames` word the setting.
pub(crate) fn party_summary(lang: Lang, strict: bool) -> String {
    t!(lang, if strict { "cfg.party-strict" } else { "cfg.party-normal" })
}

impl ConfigPartyGames {
    pub async fn run(self, cx: &Cx, guild: u64) -> anyhow::Result<()> {
        if let Some(strict) = self.strict {
            cx.app.db.set_party_strict(guild, strict).await?;
            STRICT.invalidate(&guild).await;
        }
        let strict = cx.app.db.party_strict(guild).await?;
        tell(
            cx,
            t!(
                cx.lang(),
                "cfg.party-current",
                summary = party_summary(cx.lang(), strict)
            ),
        )
        .await
    }
}
