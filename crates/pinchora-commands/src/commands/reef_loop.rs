//! D7 member actions under the existing fish, house, pet and partner roots.
use crate::Cx;
use chrono::{Datelike, Duration};
use pinchora_core::{i18n::Lang, reef_loop as rules, t};
use pinchora_db::{ReefAction, ReefOutcome};
use pinchora_game::{Content, Rarity};
use pinchora_render::{ReefCard, ReefScene, ReefTile, encode_png};
use std::str::Split;
use twilight_interactions::command::{CommandModel, CommandOption, CreateCommand, CreateOption};
use twilight_model::channel::message::{
    Component,
    component::{ActionRow, Button, ButtonStyle},
};
use twilight_util::builder::embed::EmbedBuilder;

#[derive(CommandModel, CreateCommand)]
#[command(
    name = "dock",
    desc = "Visit Mallowfin: two requested fish for two seaweed; rotates every 8 hours"
)]
pub struct Dock {
    /// Deliver two of the requested fish from your bucket
    deliver: Option<bool>,
}
#[derive(CommandModel, CreateCommand)]
#[command(
    name = "contest",
    desc = "Daily fishing contest: your best size relative to its species, UTC"
)]
pub struct Contest {
    /// Show yesterday's final results
    yesterday: Option<bool>,
}
#[derive(CommandModel, CreateCommand)]
#[command(
    name = "season",
    desc = "Seasonal fish, standings and last season's furniture keepsake"
)]
pub struct Season {
    /// Show the previous season's final results
    previous: Option<bool>,
    /// Claim the previous season's keepsake (three fishing days)
    claim: Option<bool>,
}
#[derive(CommandModel, CreateCommand)]
#[command(
    name = "dive",
    desc = "Lantern Descent: 300 shells, once daily, up to three pearls at risk"
)]
pub struct Dive {
    /// Pay 300 shells and start a 90-second dive
    start: Option<bool>,
}
#[derive(CommandOption, CreateOption, Clone, Copy)]
pub enum TankAction {
    #[option(name = "View", value = "view")]
    View,
    #[option(name = "Rename", value = "rename")]
    Rename,
    #[option(name = "Add fish", value = "add")]
    Add,
    #[option(name = "Return fish", value = "remove")]
    Remove,
}
#[derive(CommandModel, CreateCommand)]
#[command(
    name = "aquarium",
    desc = "Your named house aquarium: display up to six fish from your bucket"
)]
pub struct Aquarium {
    /// What to do (default: view)
    action: Option<TankAction>,
    /// Aquarium name
    #[command(min_length = 1, max_length = 24)]
    name: Option<String>,
    /// Fish number shown by the aquarium's bucket list
    #[command(min_value = 1)]
    fish: Option<i64>,
    /// Display slot, 1 to 6
    #[command(min_value = 1, max_value = 6)]
    slot: Option<i64>,
    /// Bucket page, 1 to 5
    #[command(min_value = 1, max_value = 5)]
    page: Option<i64>,
}
#[derive(CommandOption, CreateOption, Clone, Copy)]
pub enum GardenAction {
    #[option(name = "View", value = "view")]
    View,
    #[option(name = "Plant (40 shells)", value = "plant")]
    Plant,
    #[option(name = "Water", value = "water")]
    Water,
    #[option(name = "Harvest", value = "harvest")]
    Harvest,
}
#[derive(CommandModel, CreateCommand)]
#[command(
    name = "garden",
    desc = "Brinebud Garden: plant, water after one hour, harvest after eight hours"
)]
pub struct Garden {
    /// What to do (default: view)
    action: Option<GardenAction>,
}
#[derive(CommandOption, CreateOption, Clone, Copy)]
pub enum RecipeChoice {
    #[option(name = "Sprigstep planter", value = "sprig")]
    Sprig,
    #[option(name = "Reefbubble lamp", value = "glow")]
    Glow,
    #[option(name = "Blushpetal planter", value = "bloom")]
    Bloom,
}
#[derive(CommandModel, CreateCommand)]
#[command(
    name = "craft",
    desc = "Craft bound furniture with seaweed, common fish, pearls and shells"
)]
pub struct Craft {
    /// Craft this recipe; leave empty to browse the costs
    recipe: Option<RecipeChoice>,
}
#[derive(CommandModel, CreateCommand)]
#[command(name = "streak", desc = "See your daily pet-care streak and gentle seaweed reward")]
pub struct PetStreak;
#[derive(CommandModel, CreateCommand)]
#[command(
    name = "streak",
    desc = "Check in together: both partners visit daily for one seaweed each"
)]
pub struct PartnerStreak;

fn line(lang: Lang, key: &str) -> String {
    t!(lang, key)
}
fn card(cx: &Cx, key: &str, scene: ReefScene) -> ReefCard {
    ReefCard {
        title: line(cx.lang(), key),
        subtitle: t!(cx.lang(), "reef.subtitle"),
        scene,
        tiles: vec![],
        lines: vec![],
        footer: t!(cx.lang(), "reef.footer"),
        progress: 0.,
    }
}
async fn reply(cx: &Cx, card: ReefCard, body: String, buttons: Vec<Component>, update: bool) -> anyhow::Result<()> {
    let title = card.title.clone();
    let png = cx.app.render(move |r| r.svg_to_png(&card.to_svg())).await?;
    let color = match cx.guild_id() {
        Some(g) => cx.app.brand_color(g).await?,
        None => crate::util::BRAND,
    };
    let embed = EmbedBuilder::new().color(color).title(title).description(body);
    let data = Cx::card(embed, "reef-loop.png", png, buttons, true);
    if update {
        cx.update(data).await
    } else {
        cx.reply(data).await
    }
}
fn fish_tile(cx: &Cx, id: &str, label: String) -> ReefTile {
    let image = cx.app.sprites.fish(id);
    ReefTile {
        png: encode_png(&image).unwrap_or_default(),
        width: image.width(),
        height: image.height(),
        label,
    }
}
fn request_fish(timestamp: i64, month: u32) -> &'static pinchora_game::Fish {
    let eligible: Vec<_> = Content::get()
        .fish
        .iter()
        .filter(|f| {
            f.rarity == Rarity::Common
                && f.time == "any"
                && !f.junk
                && f.spots.contains_key("shore")
                && (f.season == "any" || f.season == pinchora_game::fishing::season(month))
        })
        .collect();
    eligible[timestamp
        .div_euclid(rules::REQUEST_SECS)
        .rem_euclid(eligible.len() as i64) as usize]
}
impl Dock {
    pub async fn run(self, cx: &Cx) -> anyhow::Result<()> {
        let now = cx.app.now();
        let fish = request_fish(now.timestamp(), now.month());
        let mut c = card(cx, "reef.dock-title", ReefScene::Dock);
        let result = if self.deliver.unwrap_or(false) {
            Some(
                cx.app
                    .db
                    .reef_action(cx.author()?.id.get(), now, ReefAction::Request { fish: &fish.id })
                    .await?,
            )
        } else {
            None
        };
        let name = Content::get().name(&fish.id, cx.lang());
        let until = (now.timestamp().div_euclid(rules::REQUEST_SECS) + 1) * rules::REQUEST_SECS;
        let body = t!(cx.lang(), "reef.dock-body", fish = name, until = until);
        c.lines = vec![
            t!(cx.lang(), "reef.request"),
            name.into_owned(),
            t!(cx.lang(), "reef.request-cost"),
            t!(cx.lang(), "reef.request-reward"),
        ];
        if let Some(r) = result {
            c.lines.push(line(cx.lang(), r.key()));
        }
        reply(cx, c, with_result(cx, result, body), vec![], false).await
    }
}
fn with_result(cx: &Cx, result: Option<ReefOutcome>, body: String) -> String {
    match result {
        Some(r) => format!("{}\n\n{body}", line(cx.lang(), r.key())),
        None => body,
    }
}
impl Aquarium {
    pub async fn run(self, cx: &Cx) -> anyhow::Result<()> {
        let user = cx.author()?.id.get();
        super::house::ensure_house(&cx.app, user).await?;
        let action = match self.action.unwrap_or(TankAction::View) {
            TankAction::View => None,
            TankAction::Rename => Some(ReefAction::Rename(self.name.as_deref().unwrap_or(""))),
            TankAction::Add => Some(ReefAction::AddFish {
                bucket: self.fish.unwrap_or(0),
                slot: self.slot.unwrap_or(1) as i16,
            }),
            TankAction::Remove => Some(ReefAction::RemoveFish(self.slot.unwrap_or(1) as i16)),
        };
        let result = if let Some(action) = action {
            Some(cx.app.db.reef_action(user, cx.app.now(), action).await?)
        } else {
            None
        };
        let s = cx.app.db.reef_state(user).await?;
        let fish = cx.app.db.reef_aquarium(user).await?;
        let mut c = card(cx, "reef.aquarium-title", ReefScene::Aquarium);
        c.subtitle = s.aquarium_name;
        c.tiles = fish
            .iter()
            .map(|f| {
                fish_tile(
                    cx,
                    &f.fish_id,
                    format!("{} · {}", f.slot, Content::get().name(&f.fish_id, cx.lang())),
                )
            })
            .collect();
        c.lines = vec![
            t!(cx.lang(), "reef.tank-count", count = fish.len()),
            t!(cx.lang(), "reef.tank-safe"),
        ];
        for f in &fish {
            c.lines.push(format!("{} · {:.1} cm", f.slot, f.size_cm));
        }
        c.progress = fish.len() as f32 / 6.;
        let bucket = cx.app.db.bucket(user).await?;
        let picks = bucket
            .iter()
            .skip((self.page.unwrap_or(1).clamp(1, 5) as usize - 1) * 12)
            .take(12)
            .map(|f| {
                format!(
                    "`{}` · {} · {:.1} cm",
                    f.id,
                    Content::get().name(&f.fish_id, cx.lang()),
                    f.size_cm
                )
            })
            .collect::<Vec<_>>()
            .join("\n");
        let body = t!(cx.lang(), "reef.tank-body", fish = picks);
        reply(cx, c, with_result(cx, result, body), vec![], false).await
    }
}
impl Garden {
    pub async fn run(self, cx: &Cx) -> anyhow::Result<()> {
        let user = cx.author()?.id.get();
        super::house::ensure_house(&cx.app, user).await?;
        let action = match self.action.unwrap_or(GardenAction::View) {
            GardenAction::View => None,
            GardenAction::Plant => Some(ReefAction::Plant),
            GardenAction::Water => Some(ReefAction::Water),
            GardenAction::Harvest => Some(ReefAction::Harvest),
        };
        let now = cx.app.now();
        let result = if let Some(a) = action {
            Some(cx.app.db.reef_action(user, now, a).await?)
        } else {
            None
        };
        let s = cx.app.db.reef_state(user).await?;
        let mut c = card(cx, "reef.garden-title", ReefScene::Garden);
        c.lines = vec![
            t!(cx.lang(), "reef.stock", count = s.seaweed),
            t!(cx.lang(), "reef.seed-cost"),
            t!(cx.lang(), "reef.harvest-yield"),
        ];
        let mut body = t!(cx.lang(), "reef.garden-body");
        if let Some(planted) = s.planted_at {
            c.progress = ((now - planted).num_seconds() as f32 / rules::GROW_SECS as f32).clamp(0., 1.);
            let status = if s.watered_at.is_some() {
                "reef.watered"
            } else {
                "reef.needs-water"
            };
            c.lines.push(line(cx.lang(), status));
            body.push_str(&t!(
                cx.lang(),
                "reef.garden-times",
                water = (planted + Duration::seconds(rules::WATER_SECS)).timestamp(),
                harvest = (planted + Duration::seconds(rules::GROW_SECS)).timestamp()
            ));
        } else {
            c.progress = -1.;
            c.lines.push(t!(cx.lang(), "reef.empty-bed"));
        }
        reply(cx, c, with_result(cx, result, body), vec![], false).await
    }
}
impl Craft {
    pub async fn run(self, cx: &Cx) -> anyhow::Result<()> {
        let user = cx.author()?.id.get();
        super::house::ensure_house(&cx.app, user).await?;
        let eligible: Vec<_> = Content::get()
            .fish
            .iter()
            .filter(|f| f.rarity == Rarity::Common && !f.junk)
            .map(|f| f.id.as_str())
            .collect();
        let result = if let Some(choice) = self.recipe {
            let recipe = match choice {
                RecipeChoice::Sprig => "sprig",
                RecipeChoice::Glow => "glow",
                RecipeChoice::Bloom => "bloom",
            };
            Some(
                cx.app
                    .db
                    .reef_action(
                        user,
                        cx.app.now(),
                        ReefAction::Craft {
                            recipe,
                            eligible_fish: &eligible,
                        },
                    )
                    .await?,
            )
        } else {
            None
        };
        let mut c = card(cx, "reef.craft-title", ReefScene::Craft);
        let mut body = t!(cx.lang(), "reef.craft-body");
        for r in rules::RECIPES {
            let name = Content::get().name(r.item, cx.lang()).into_owned();
            if let Some(img) = cx.app.sprites.item(Content::get(), r.item) {
                c.tiles.push(ReefTile {
                    png: encode_png(&img)?,
                    width: img.width(),
                    height: img.height(),
                    label: name.clone(),
                });
            }
            body.push_str(&t!(
                cx.lang(),
                "reef.recipe",
                name = name,
                weed = r.seaweed,
                fish = r.fish,
                pearls = r.pearls,
                shells = r.shells
            ));
        }
        let s = cx.app.db.reef_state(user).await?;
        c.lines = vec![
            t!(cx.lang(), "reef.stock", count = s.seaweed),
            t!(cx.lang(), "reef.bound"),
            t!(cx.lang(), "reef.common-only"),
            t!(cx.lang(), "reef.craft-hint"),
        ];
        reply(cx, c, with_result(cx, result, body), vec![], false).await
    }
}
impl Dive {
    pub async fn run(self, cx: &Cx) -> anyhow::Result<()> {
        let result = if self.start.unwrap_or(false) {
            Some(
                cx.app
                    .db
                    .reef_action(cx.author()?.id.get(), cx.app.now(), ReefAction::DiveStart)
                    .await?,
            )
        } else {
            None
        };
        dive_panel(cx, result, false).await
    }
}
async fn dive_panel(cx: &Cx, result: Option<ReefOutcome>, update: bool) -> anyhow::Result<()> {
    let user = cx.author()?.id.get();
    let s = cx.app.db.reef_state(user).await?;
    let mut c = card(cx, "reef.dive-title", ReefScene::Dive);
    let active = s.dive_until.is_some_and(|until| until > cx.app.now());
    let mut buttons = vec![];
    c.lines = vec![
        t!(cx.lang(), "reef.dive-fee"),
        t!(cx.lang(), "reef.held", count = if active { s.dive_depth } else { 0 }),
        if s.dive_depth == 3 {
            t!(cx.lang(), "reef.surface-ready")
        } else {
            t!(cx.lang(), "reef.risk", risk = rules::dive_risk(s.dive_depth))
        },
    ];
    c.progress = s.dive_depth as f32 / 3.;
    if active {
        for (action, key) in [("push", "reef.push"), ("surface", "reef.surface")] {
            if action == "push" && s.dive_depth == 3 {
                continue;
            }
            buttons.push(Component::Button(Button {
                custom_id: Some(format!("fz:loop:{user}:{}:{}:{action}", s.dive_nonce, s.dive_depth)),
                disabled: false,
                emoji: None,
                id: None,
                label: Some(line(cx.lang(), key)),
                sku_id: None,
                style: if action == "surface" {
                    ButtonStyle::Success
                } else {
                    ButtonStyle::Primary
                },
                url: None,
            }));
        }
    }
    let rows = if buttons.is_empty() {
        vec![]
    } else {
        vec![Component::ActionRow(ActionRow {
            id: None,
            components: buttons,
        })]
    };
    let mut body = with_result(cx, result, t!(cx.lang(), "reef.dive-body"));
    if update && result == Some(ReefOutcome::Done) && !active {
        let banked = t!(cx.lang(), "reef.banked", count = s.dive_depth);
        c.lines.push(banked.clone());
        body = format!("{banked}\n{body}");
    }
    reply(cx, c, body, rows, update).await
}
pub(crate) async fn on_dive(cx: &Cx, mut args: Split<'_, char>) -> anyhow::Result<()> {
    let user = args.next().and_then(|v| v.parse::<u64>().ok()).unwrap_or(0);
    let nonce = args.next().and_then(|v| v.parse::<i64>().ok()).unwrap_or(-1);
    let depth = args.next().and_then(|v| v.parse::<i32>().ok()).unwrap_or(-1);
    let action = args.next().unwrap_or("");
    if user != cx.author()?.id.get() || !matches!(action, "push" | "surface") {
        return cx.reply_ephemeral(t!(cx.lang(), "reef.stale")).await;
    }
    let roll = rand::random_range(0..100);
    let result = cx
        .app
        .db
        .reef_action(
            user,
            cx.app.now(),
            ReefAction::DiveMove {
                nonce,
                depth,
                surface: action == "surface",
                roll,
            },
        )
        .await?;
    dive_panel(cx, Some(result), true).await
}
impl Contest {
    pub async fn run(self, cx: &Cx) -> anyhow::Result<()> {
        let day = cx.app.now().date_naive() - Duration::days(i64::from(self.yesterday.unwrap_or(false)));
        board(cx, day, day + Duration::days(1), false, None).await
    }
}
impl Season {
    pub async fn run(self, cx: &Cx) -> anyhow::Result<()> {
        let now = cx.app.now();
        let current = rules::season_start(now.date_naive());
        let previous = self.previous.unwrap_or(false) || self.claim.unwrap_or(false);
        let start = if previous {
            rules::season_start(current - Duration::days(1))
        } else {
            current
        };
        let result = if self.claim.unwrap_or(false) {
            let Some(guild) = cx.guild_id() else {
                return cx.reply_ephemeral(t!(cx.lang(), "reef.guild-only")).await;
            };
            Some(
                cx.app
                    .db
                    .reef_action(
                        cx.author()?.id.get(),
                        now,
                        ReefAction::SeasonClaim { guild, season: start },
                    )
                    .await?,
            )
        } else {
            None
        };
        board(cx, start, rules::next_season(start), true, result).await
    }
}
async fn board(
    cx: &Cx,
    start: chrono::NaiveDate,
    end: chrono::NaiveDate,
    season: bool,
    result: Option<ReefOutcome>,
) -> anyhow::Result<()> {
    let Some(guild) = cx.guild_id() else {
        return cx.reply_ephemeral(t!(cx.lang(), "reef.guild-only")).await;
    };
    let rows = cx.app.db.reef_board(guild, start, end).await?;
    let mut c = card(
        cx,
        if season {
            "reef.season-title"
        } else {
            "reef.contest-title"
        },
        ReefScene::Season,
    );
    c.subtitle = format!("{start} / {end} · UTC");
    c.lines.push(t!(
        cx.lang(),
        if end <= cx.app.now().date_naive() {
            "reef.final"
        } else {
            "reef.live"
        }
    ));
    for (i, row) in rows.iter().take(8).enumerate() {
        let name = cx
            .app
            .db
            .remembered_name(row.user_id as u64)
            .await?
            .unwrap_or_else(|| t!(cx.lang(), "reef.angler"));
        c.lines.push(format!("{}. {} · {:.2}", i + 1, name, row.score));
    }
    if rows.is_empty() {
        c.lines.push(t!(cx.lang(), "reef.no-entries"));
    }
    let mut body = t!(cx.lang(), "reef.contest-body");
    if season {
        body.push_str(&t!(cx.lang(), "reef.season-body"));
        let key = rules::season_key(start);
        let fish: Vec<_> = Content::get().fish.iter().filter(|f| f.season == key).collect();
        body.push_str(&format!(
            "\n{}",
            fish.iter()
                .map(|f| Content::get().name(&f.id, cx.lang()).into_owned())
                .collect::<Vec<_>>()
                .join(" · ")
        ));
        c.scene = ReefScene::Aquarium;
        c.tiles = fish
            .iter()
            .take(6)
            .map(|f| fish_tile(cx, &f.id, Content::get().name(&f.id, cx.lang()).into_owned()))
            .collect();
    }
    reply(cx, c, with_result(cx, result, body), vec![], false).await
}
impl PetStreak {
    pub async fn run(self, cx: &Cx) -> anyhow::Result<()> {
        let s = cx.app.db.reef_state(cx.author()?.id.get()).await?;
        let mut c = card(cx, "reef.pet-title", ReefScene::Care);
        c.lines = vec![
            t!(cx.lang(), "reef.streak-days", count = s.pet_streak),
            t!(cx.lang(), "reef.stock", count = s.seaweed),
        ];
        reply(cx, c, t!(cx.lang(), "reef.pet-body"), vec![], false).await
    }
}
impl PartnerStreak {
    pub async fn run(self, cx: &Cx) -> anyhow::Result<()> {
        let (result, streak) = cx
            .app
            .db
            .reef_partner_checkin(cx.author()?.id.get(), cx.app.now())
            .await?;
        let mut c = card(cx, "reef.partner-title", ReefScene::Care);
        c.lines = vec![
            t!(cx.lang(), "reef.streak-days", count = streak),
            line(cx.lang(), result.key()),
        ];
        reply(
            cx,
            c,
            with_result(cx, Some(result), t!(cx.lang(), "reef.partner-body")),
            vec![],
            false,
        )
        .await
    }
}

/// The aquarium accompanies an authorized house view, including visits; empty tanks add no card.
pub(crate) async fn attach_aquarium(
    cx: &Cx,
    owner: u64,
    data: &mut twilight_model::http::interaction::InteractionResponseData,
) -> anyhow::Result<()> {
    let fish = cx.app.db.reef_aquarium(owner).await?;
    if fish.is_empty() {
        return Ok(());
    }
    let Some(s) = cx.app.db.reef_state_if_any(owner).await? else {
        return Ok(());
    };
    let mut c = card(cx, "reef.aquarium-title", ReefScene::Aquarium);
    c.subtitle = s.aquarium_name.clone();
    c.tiles = fish
        .iter()
        .map(|f| fish_tile(cx, &f.fish_id, Content::get().name(&f.fish_id, cx.lang()).into_owned()))
        .collect();
    c.lines = vec![
        t!(cx.lang(), "reef.tank-count", count = fish.len()),
        t!(cx.lang(), "reef.tank-safe"),
    ];
    c.progress = fish.len() as f32 / 6.;
    let png = cx.app.render(move |r| r.svg_to_png(&c.to_svg())).await?;
    let color = match cx.guild_id() {
        Some(g) => cx.app.brand_color(g).await?,
        None => crate::util::BRAND,
    };
    data.embeds.get_or_insert_default().push(
        EmbedBuilder::new()
            .color(color)
            .title(s.aquarium_name)
            .image(twilight_util::builder::embed::ImageSource::attachment("aquarium.png")?)
            .build(),
    );
    data.attachments
        .get_or_insert_default()
        .push(twilight_model::http::attachment::Attachment::from_bytes(
            "aquarium.png".into(),
            png,
            2,
        ));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn recipes_resolve_to_bound_placeable_catalog_items() {
        for r in rules::RECIPES {
            let id = pinchora_core::items::soulbound(r.item);
            let item = Content::get().item(&id).expect("a real furniture variant");
            assert!(item.bound);
            assert!(matches!(
                item.base.layer,
                pinchora_game::Layer::Floor | pinchora_game::Layer::Surface
            ));
        }
    }
    #[test]
    fn dock_requests_are_catchable_all_day_at_the_starter_spot() {
        let mut seen = std::collections::BTreeSet::new();
        for month in 1..=12 {
            for rotation in 0..20 {
                let f = request_fish(rotation * rules::REQUEST_SECS, month);
                assert!(!f.junk && f.rarity == Rarity::Common && f.spots.contains_key("shore"));
                for hour in 0..24 {
                    assert!(f.bites_at(hour, month));
                }
                seen.insert(&f.id);
            }
        }
        assert!(seen.len() >= 3);
    }
}
