//! `/beachcomb`: a walk along the shore every twenty minutes, turning up a find (or now and then a
//! critter) for the dex.

use chrono::{Duration, Timelike};
use pinchora_db::Comb;
use pinchora_game::{
    collection::{self, COMB_COOLDOWN_MINUTES},
    fishing,
    play::Game,
};
use pinchora_render::{HaulCard, Theme};
use twilight_interactions::command::{CommandModel, CreateCommand};
use twilight_util::builder::embed::EmbedBuilder;

use super::{award, daily, dice, haul_tile, money};
use crate::{
    Cx,
    commands::games::record,
    util::{BRAND, SUCCESS},
};

#[derive(CommandModel, CreateCommand)]
#[command(
    name = "beachcomb",
    desc = "Walk the shore for shells, sea glass and critters (once every 20 minutes)"
)]
pub struct Beachcomb;

/// How the shore looks at a part of the day, for the card's corner.
fn shore(part: &str) -> &'static str {
    match part {
        "dawn" => "Sunrise shore",
        "day" => "Bright shore",
        "dusk" => "Sunset shore",
        _ => "Moonlit shore",
    }
}

impl Beachcomb {
    pub async fn run(self, cx: &Cx) -> anyhow::Result<()> {
        let app = &cx.app;
        let user = cx.author()?.id.get();
        let now = app.now();
        let Some(entry) = collection::comb(now.hour(), chrono::Datelike::month(&now), &mut dice()) else {
            return cx.reply_ephemeral("The tide is out of ideas. Try again soon.").await;
        };
        let wait = Duration::minutes(COMB_COOLDOWN_MINUTES);
        let outcome = app
            .db
            .beachcomb(user, now, wait, award(entry), daily(app, Game::Beachcomb))
            .await?;
        let (granted, paid) = match outcome {
            Comb::Wait { ready_at } => {
                return cx
                    .reply_ephemeral(format!(
                        "You walked the shore a moment ago. The tide brings more <t:{}:R>.",
                        ready_at.timestamp()
                    ))
                    .await;
            }
            Comb::Found { granted, paid } => (granted, paid),
        };
        record(app, Game::Beachcomb, user, true).await;
        let tile = haul_tile(app, entry, Some(&granted), HaulCard::SINGLE_ART_BOX);
        let footer = if paid {
            format!("Next walk in {COMB_COOLDOWN_MINUTES} minutes")
        } else {
            "Daily limit reached: finds still fill your dex, shells wait for tomorrow".to_owned()
        };
        let part = fishing::time_of_day(now.hour());
        let card = HaulCard {
            title: "Beachcomb".to_owned(),
            subtitle: shore(part).to_uppercase(),
            tiles: vec![tile],
            footer,
            theme: Theme::reef(),
        };
        let png = app.render(move |r| r.haul_card(&card)).await?;
        let pay = money(&granted);
        let summary = format!(
            "{}**{}** ({}){}",
            if granted.new { "🆕 " } else { "" },
            entry.name,
            entry.rarity.label().to_lowercase(),
            if pay.is_empty() {
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
