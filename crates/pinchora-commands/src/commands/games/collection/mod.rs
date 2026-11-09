//! The collection games: Critter Drop, Message in a Bottle, Beachcomb with the Tide Pool Dex, and the
//! Crab Pot. They share one catalogue of critters and finds, one dex per player, and what spares leave
//! behind (shells and fragments).

pub(crate) mod beachcomb;
pub(crate) mod crabpot;
pub(crate) mod depth;
pub(crate) mod dex;
pub(crate) mod drops;
pub(crate) mod settings;
pub(crate) mod spawn;

pub use crate::commands::games::collection::drops::post_bottle;
pub use crate::commands::games::collection::drops::post_critter_drop;

use std::{
    collections::HashMap,
    sync::{Arc, LazyLock, Mutex},
    time::Duration,
};

use chrono::Duration as Span;
use image::{RgbaImage, imageops};
use pinchora_core::games::items::is_soulbound;
use pinchora_core::infra::i18n::Lang;
use pinchora_core::t;
use pinchora_db::{Award, Daily, Granted, InventoryRow};
use pinchora_game::{
    collection::{Art, Entry},
    play::Game,
};
use pinchora_render::{EntryPicture, HaulTile, critters, encode_png};

use crate::App;

/// Ticks closes drops whose time is up, and forgets old ones once an hour.
const WAKE: Duration = Duration::from_secs(5);
const CLEANUP_EVERY: u32 = 720;
/// Closed drops are forgotten after this long.
const KEEP: Span = Span::days(2);

/// What soulbound means, for every place that has to tell a player so.
pub(crate) fn soulbound_rule(lang: Lang) -> String {
    t!(lang, "collect.soulbound-rule")
}

/// The most options a select menu holds.
const MENU: usize = 25;

/// The pieces of an inventory a menu of things to give away can offer (the soulbound ones stay out), and
/// whether any soulbound piece was left out.
pub(crate) fn giveable(rows: Vec<InventoryRow>) -> (Vec<InventoryRow>, bool) {
    let (bound, free): (Vec<_>, Vec<_>) = rows.into_iter().partition(|row| is_soulbound(&row.item_id));
    (free.into_iter().take(MENU).collect(), !bound.is_empty())
}

/// What the rules say `entry` pays, for the database to apply.
pub(crate) fn award(entry: &Entry) -> Award<'_> {
    let pay = entry.pay();
    Award {
        entry: &entry.id,
        new_shells: pay.new_shells,
        dup_shells: pay.dup_shells,
        dup_fragments: pay.dup_fragments,
        bonus_fragments: i64::from(entry.salvage),
    }
}

/// The UTC day (days since 1970) as the rules see it, which tests can move with `App::shift_clock`.
pub(crate) fn today(app: &App) -> i32 {
    app.now().timestamp().div_euclid(86_400) as i32
}

/// The daily limit of a game's paying plays.
pub(crate) fn daily(app: &App, game: Game) -> Daily<'static> {
    Daily {
        game: game.key(),
        day: today(app),
        cap: game.daily_cap() as i32,
    }
}

/// Dice for the rules: numbers in `0..1`.
pub(crate) fn dice() -> impl FnMut() -> f64 {
    || rand::random::<f64>()
}

/// The sprite of an entry: the atlas's `cr_<id>` once it is drawn, else the sprite it borrows, else its
/// drawn look.
fn sprite(sprites: &crate::games::game::scene::Sprites, entry: &Entry) -> Arc<RgbaImage> {
    if let Some(drawn) = sprites.atlas(&entry.sprite_name()) {
        return drawn;
    }
    let made = match entry.art() {
        Some(Art::Fish(id)) => Some(sprites.fish(id)),
        Some(Art::Pet(id)) => Some(sprites.pet(id)),
        Some(Art::Furniture(id)) => sprites.atlas(&format!("f_{id}")),
        Some(Art::Drawn {
            shape,
            body,
            accent,
            pattern,
        }) => critters::draw(shape, body, accent, pattern).map(Arc::new),
        None => None,
    };
    made.unwrap_or_else(|| Arc::new(RgbaImage::from_pixel(8, 8, image::Rgba([0x8f, 0xa0, 0xd8, 255]))))
}

/// The sprite without the transparent rows and columns around it, so it sits in the middle of a box.
pub(crate) fn trimmed(sprite: &RgbaImage) -> RgbaImage {
    let (mut x0, mut y0, mut x1, mut y1) = (sprite.width(), sprite.height(), 0, 0);
    for (x, y, px) in sprite.enumerate_pixels() {
        if px[3] > 0 {
            (x0, y0, x1, y1) = (x0.min(x), y0.min(y), x1.max(x), y1.max(y));
        }
    }
    if x1 < x0 {
        return sprite.clone();
    }
    imageops::crop_imm(sprite, x0, y0, x1 - x0 + 1, y1 - y0 + 1).to_image()
}

/// Entry pictures made so far by entry and box; there are only a hundred or so entries.
type Pictures = Mutex<HashMap<(String, u32, u32), EntryPicture>>;
static PICTURES: LazyLock<Pictures> = LazyLock::new(Mutex::default);

/// The picture of `entry` for a box of `boxed` pixels: scaled up by the largest whole factor that fits (up to
/// 12×), or down by a whole divisor when it is bigger than the box, so every pixel stays a crisp block.
pub(crate) fn picture(app: &App, entry: &Entry, boxed: (u32, u32)) -> EntryPicture {
    let key = (entry.id.clone(), boxed.0, boxed.1);
    if let Some(done) = PICTURES.lock().expect("picture cache").get(&key) {
        return done.clone();
    }
    let sprite = trimmed(&sprite(&app.sprites, entry));
    let (w, h) = (sprite.width().max(1), sprite.height().max(1));
    let fitted = if w <= boxed.0 && h <= boxed.1 {
        let factor = (boxed.0 / w).min(boxed.1 / h).clamp(1, 12);
        imageops::resize(&sprite, w * factor, h * factor, imageops::FilterType::Nearest)
    } else {
        let k = w.div_ceil(boxed.0).max(h.div_ceil(boxed.1));
        imageops::resize(&sprite, (w / k).max(1), (h / k).max(1), imageops::FilterType::Nearest)
    };
    let done = EntryPicture {
        w: fitted.width(),
        h: fitted.height(),
        png: encode_png(&fitted).unwrap_or_default(),
    };
    PICTURES.lock().expect("picture cache").insert(key, done.clone());
    done
}

/// A deterministic palette shift of the original atlas/drawn sprite, with no new art file.
pub(crate) fn glimmer_picture(app: &App, entry: &Entry, boxed: (u32, u32)) -> EntryPicture {
    let mut art = picture(app, entry, boxed);
    if let Ok(bitmap) = image::load_from_memory(&art.png) {
        art.png = encode_png(&pinchora_render::sprites::glimmer(&bitmap.to_rgba8())).unwrap_or(art.png);
    }
    art
}

/// What a play paid, as a short phrase: `+14 shells +2 frag`; empty when nothing was.
pub(crate) fn money(lang: Lang, granted: &Granted) -> String {
    let mut parts = Vec::new();
    if granted.shells > 0 {
        parts.push(t!(lang, "collect.money-shells", shells = granted.shells));
    }
    if granted.fragments > 0 {
        parts.push(t!(lang, "collect.money-fragments", fragments = granted.fragments));
    }
    parts.join(" ")
}

/// The tag a haul tile carries: `NEW` for a discovery, `SPARE` for one the player already had.
pub(crate) fn tag(lang: Lang, granted: &Granted) -> String {
    if granted.new {
        t!(lang, "collect.tag-new")
    } else {
        t!(lang, "collect.tag-spare")
    }
}

/// A tile for `entry` in a haul; `granted` is what it paid, if it paid anything to speak of.
pub(crate) fn haul_tile(
    app: &App,
    lang: Lang,
    entry: &Entry,
    granted: Option<&Granted>,
    boxed: (u32, u32),
) -> HaulTile {
    HaulTile {
        art: picture(app, entry, boxed),
        name: entry.name_in(lang),
        rarity: entry.rarity.label_in(lang).to_owned(),
        frame: entry.rarity.color(),
        tag: granted.map(|g| tag(lang, g)).unwrap_or_default(),
        line: granted.map(|g| money(lang, g)).unwrap_or_default(),
        flavor: entry.flavor_in(lang),
    }
}

/// Ends the drops whose time is up: each is closed in the database exactly once, and the pod that
/// closed it tells the channel. Returns how many it closed.
pub async fn tick(app: &Arc<App>) -> anyhow::Result<usize> {
    for row in app.db.reveal_due_votes(app.now(), 50).await? {
        crate::commands::games::collection::drops::reveal_vote(app, &row).await;
    }
    let due = app.db.close_due_drops(app.now(), 50).await?;
    let mut closed = 0;
    for row in due {
        crate::commands::games::collection::drops::close(app, &row).await;
        closed += 1;
    }
    Ok(closed)
}

/// Runs [`tick`] for the life of the process, and forgets old drops once an hour.
pub fn spawn_ticker(app: Arc<App>) {
    tokio::spawn(async move {
        let mut wake = tokio::time::interval(WAKE);
        wake.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        let mut rounds = 0u32;
        loop {
            wake.tick().await;
            if let Err(error) = Box::pin(tick(&app)).await {
                tracing::warn!(%error, "collection tick failed");
            }
            rounds = rounds.wrapping_add(1);
            if rounds.is_multiple_of(CLEANUP_EVERY) {
                match app.db.forget_old_collection(app.now() - KEEP, today(&app) - 3).await {
                    Ok(n) if n > 0 => tracing::debug!(n, "old collection rows forgotten"),
                    Ok(_) => {}
                    Err(error) => tracing::warn!(%error, "old collection rows not forgotten"),
                }
            }
        }
    });
}

#[cfg(test)]
mod depth_art_tests {
    use crate::commands::games::collection::*;
    use pinchora_render::{DropCard, DropCritter, Renderer, Theme};
    #[test]
    fn every_collection_critter_renders_original_glimmer_art_and_sparkle_frame() {
        let sprites = crate::games::game::scene::Sprites::default();
        let renderer = Renderer::new();
        let catalogue = pinchora_game::collection::Collection::get();
        for entry in &catalogue.critters {
            let original = sprite(&sprites, entry);
            assert!(
                original.width() > 8 || original.height() > 8,
                "placeholder: {}",
                entry.id
            );
            let variant = pinchora_render::sprites::glimmer(&original);
            assert_ne!(*original, variant, "{}", entry.id);
            let card = DropCard {
                title: "Glimmer".into(),
                subtitle: entry.id.clone(),
                cards: vec![DropCritter {
                    art: EntryPicture {
                        png: encode_png(&variant).unwrap(),
                        w: variant.width(),
                        h: variant.height(),
                    },
                    name: entry.name_in(Lang::En),
                    rarity: entry.rarity.label_in(Lang::En).into(),
                    frame: 0x9af5ec,
                    habitat: entry.place_in(Lang::En).into(),
                    sparkles: 4,
                }],
                footer: String::new(),
                theme: Theme::reef(),
            };
            assert!(card.to_svg().contains("stroke-dasharray=\"12 8\""));
            let png = renderer.drop_card(&card).unwrap();
            let image = image::load_from_memory(&png).unwrap().to_rgba8();
            assert_eq!(image.dimensions(), (1000, 460));
            assert!(
                image.pixels().filter(|p| p.0[..3] == [0x9a, 0xf5, 0xec]).count() > 100,
                "visible frame: {}",
                entry.id
            );
        }
    }
}
