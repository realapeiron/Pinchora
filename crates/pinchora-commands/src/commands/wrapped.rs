//! `/wrapped`: a card with the year in numbers, open from 1 December to 31 January.

use pinchora_core::wrapped::{self, YearTotals};
use pinchora_db::Activity;
use pinchora_game::Content;
use pinchora_render::{SpriteArt, WrappedCard, WrappedFish, WrappedHouse, WrappedTotals};
use twilight_interactions::command::{CommandModel, CreateCommand};

use crate::{
    App, Cx,
    util::{display_name, relative},
};

#[derive(CommandModel, CreateCommand)]
#[command(name = "wrapped", desc = "Your year in numbers (open from 1 December to 31 January)")]
pub struct Wrapped {
    /// Which year (default: the one that just ended)
    #[command(min_value = 2024, max_value = 2200)]
    year: Option<i64>,
}

impl Wrapped {
    pub async fn run(self, cx: &Cx) -> anyhow::Result<()> {
        let now = cx.app.now();
        let Some(open) = wrapped::open_year(now) else {
            let opens = wrapped::opens_at(now);
            return cx
                .reply_ephemeral(format!(
                    "Wrapped isn't open yet. It opens on <t:{}:D> ({}) and stays open until the end of January.\n\
                     Until then Pinchora keeps counting: messages, XP, shells, fish, visits, likes and pet care.",
                    opens.timestamp(),
                    relative(opens)
                ))
                .await;
        };
        let year = self.year.map_or(open, |y| y as i32);
        if year > open {
            return cx
                .reply_ephemeral(format!(
                    "{year} hasn't been wrapped up yet. The latest Wrapped is {open}."
                ))
                .await;
        }
        let user = cx.author()?;
        let id = user.id.get();
        let months = cx.app.db.activity_year(id, year).await?;
        // The year just ended always gets a card, even a quiet one; an earlier year only when it has
        // something to show.
        if months.is_empty() && year != open {
            return cx
                .reply_ephemeral(format!("Nothing was counted for you in {year}."))
                .await;
        }
        cx.responder.defer(false).await?;

        let mut per_month = [0i64; 12];
        let mut total = Activity::default();
        for (month, numbers) in &months {
            per_month[*month as usize - 1] = numbers.actions();
            total = total.plus(*numbers);
        }
        let db = &cx.app.db;
        let content = Content::get();
        let favourite = db.favourite_fish(id).await?;
        let fish = favourite
            .as_ref()
            .and_then(|(fish_id, caught)| content.fish(fish_id).map(|f| (f, *caught)));
        let fish_art = fish.and_then(|(f, _)| cx.app.sprites.showcase_fish(&f.id));
        let house = match db.most_visited_house(id, year).await? {
            Some((host, visits, friend)) => Some((house_name(&cx.app, host).await, visits, friend)),
            None => None,
        };
        let player = db.user(id).await?;
        let background = player.background.as_deref().and_then(|b| content.background(b));
        let cosmetics = super::customize::Cosmetics::new(&cx.app.sprites, background, &[]);
        let theme = super::customize::theme(&player, background);
        let avatar = cx.app.avatar(user, WrappedCard::AVATAR_SIZE, Some(32)).await;
        let name = cx.author_display_name();
        let title = wrapped::title(YearTotals {
            messages: total.messages,
            shells: total.shells,
            fish: total.fish,
            visits: total.visits,
            likes: total.likes_given + total.likes_received,
            pet_care: total.pet_care,
        });
        let totals = WrappedTotals {
            messages: total.messages,
            xp: total.xp,
            shells: total.shells,
            fish: total.fish,
            visits: total.visits,
            likes_given: total.likes_given,
            likes_received: total.likes_received,
            pet_care: total.pet_care,
        };
        let fish_name = fish.map(|(f, caught)| (f.name.clone(), caught));
        let png = cx
            .app
            .render(move |r| {
                r.wrapped_card(&WrappedCard {
                    year,
                    display_name: &name,
                    avatar_png: &avatar,
                    title,
                    totals,
                    months: per_month,
                    fish: fish_name.as_ref().map(|(name, caught)| WrappedFish {
                        name,
                        caught: *caught,
                        art: fish_art.as_ref().map(|a| SpriteArt {
                            png: &a.png,
                            w: a.w,
                            h: a.h,
                        }),
                    }),
                    house: house.as_ref().map(|(name, visits, friend)| WrappedHouse {
                        name,
                        visits: *visits,
                        friend: *friend,
                    }),
                    backdrop: cosmetics.backdrop(),
                    theme,
                })
            })
            .await?;
        cx.reply_png("wrapped.png", png).await
    }
}

/// The name of a house's owner: the one remembered for them, else Discord's, else a stand-in.
async fn house_name(app: &App, host: u64) -> String {
    if let Ok(Some(name)) = app.db.remembered_name(host).await
        && !name.is_empty()
    {
        return name;
    }
    match app.user(host).await {
        Some(user) => display_name(&user, None),
        None => "A friendly crab".to_owned(),
    }
}
