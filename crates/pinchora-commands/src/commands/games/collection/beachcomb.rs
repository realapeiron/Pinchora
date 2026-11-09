//! `/beachcomb`: a walk along the shore every twenty minutes, turning up a find (or now and then a
//! critter) for the dex.

use chrono::{Duration, Timelike};
use pinchora_core::infra::i18n::Lang;
use pinchora_core::t;
use pinchora_db::Comb;
use pinchora_game::collection;
use pinchora_game::collection::COMB_COOLDOWN_MINUTES;
use pinchora_game::fishing;
use pinchora_game::lang as letters;
use pinchora_game::play::Game;
use pinchora_render::{HaulCard, Theme};
use twilight_interactions::command::{CommandModel, CreateCommand};
use twilight_util::builder::embed::EmbedBuilder;

use crate::Cx;
use crate::commands::games::collection::award;
use crate::commands::games::collection::daily;
use crate::commands::games::collection::dice;
use crate::commands::games::collection::haul_tile;
use crate::commands::games::collection::money;
use crate::commands::games::games::record;
use crate::infra::util::BRAND;
use crate::infra::util::SUCCESS;

#[derive(CommandModel, CreateCommand)]
#[command(
    name = "beachcomb",
    desc = "Walk the shore for shells, sea glass and critters (once every 20 minutes)"
)]
pub struct Beachcomb;

/// How the shore looks at a part of the day, for the card's corner.
fn shore(lang: Lang, part: &str) -> String {
    match part {
        "dawn" => t!(lang, "beach.shore-dawn"),
        "day" => t!(lang, "beach.shore-day"),
        "dusk" => t!(lang, "beach.shore-dusk"),
        _ => t!(lang, "beach.shore-night"),
    }
}

impl Beachcomb {
    pub async fn run(self, cx: &Cx) -> anyhow::Result<()> {
        let app = &cx.app;
        let user = cx.author()?.id.get();
        let lang = cx.lang();
        let now = app.now();
        let Some(entry) = collection::comb(now.hour(), chrono::Datelike::month(&now), &mut dice()) else {
            return cx.reply_ephemeral(t!(lang, "beach.nothing")).await;
        };
        let wait = Duration::minutes(COMB_COOLDOWN_MINUTES);
        let outcome = app
            .db
            .beachcomb(user, now, wait, award(entry), daily(app, Game::Beachcomb))
            .await?;
        let (granted, paid) = match outcome {
            Comb::Wait { ready_at } => {
                let when = format!("<t:{}:R>", ready_at.timestamp());
                return cx.reply_ephemeral(t!(lang, "beach.wait", when = when)).await;
            }
            Comb::Found { granted, paid } => (granted, paid),
        };
        record(app, Game::Beachcomb, user, true).await;
        let tile = haul_tile(app, lang, entry, Some(&granted), HaulCard::SINGLE_ART_BOX);
        let footer = if paid {
            t!(lang, "beach.next", minutes = COMB_COOLDOWN_MINUTES)
        } else {
            t!(lang, "beach.limit")
        };
        let part = fishing::time_of_day(now.hour());
        let card = HaulCard {
            title: Game::Beachcomb.name(lang),
            subtitle: lang.upper(&shore(lang, part)),
            tiles: vec![tile],
            footer,
            theme: Theme::reef(),
        };
        let png = app.render(move |r| r.haul_card(&card)).await?;
        let pay = money(lang, &granted);
        let summary = t!(
            lang,
            "beach.summary",
            new = if granted.new { "🆕 " } else { "" },
            name = entry.name_in(lang),
            rarity = letters::lower(lang, entry.rarity.label_in(lang)),
            pay = if pay.is_empty() {
                String::new()
            } else {
                format!(" · {pay}")
            }
        );
        let embed = EmbedBuilder::new()
            .color(if granted.new { SUCCESS } else { BRAND })
            .description(summary);
        cx.reply(Cx::card(embed, "beachcomb.png", png, Vec::new(), true)).await
    }
}
