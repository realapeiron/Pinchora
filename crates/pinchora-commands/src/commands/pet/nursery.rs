//! The nursery: every pet at a glance, eight to a page, with a menu to open one pet's card.

use std::str::Split;

use pinchora_db::PetRow;
use pinchora_game::{Content, pets};
use pinchora_render::{NurseryCard, NurseryPet, PetStatus, Theme};
use twilight_model::{
    channel::message::component::{ButtonStyle, SelectMenuOption},
    http::interaction::InteractionResponseData,
};
use twilight_util::builder::embed::EmbedBuilder;

use super::{button_if, current_needs, pet_card, row, scaled_sprite, select, status};
use crate::{App, Cx, util::BRAND};

/// Pets on one page of the grid.
const PER_PAGE: usize = 8;

/// A pet as the grid draws it, owned so it can move to the render thread.
struct Cell {
    species: String,
    name: String,
    level: i32,
    xp: f32,
    needs: [i16; 4],
    frame: u32,
    active: bool,
    status: PetStatus,
    badge: String,
}

/// "2H5M", "25M": short enough for the corner of a grid cell.
fn short_span(secs: i64) -> String {
    let minutes = (secs.max(0) + 59) / 60;
    match (minutes / 60, minutes % 60) {
        (0, m) => format!("{m}M"),
        (h, 0) => format!("{h}H"),
        (h, m) => format!("{h}H{m}M"),
    }
}

/// The nursery page `page` (clamped) of `user`'s pets; `None` for a player without any.
async fn nursery_card(app: &App, user: u64, page: usize) -> anyhow::Result<Option<InteractionResponseData>> {
    let all = app.db.pets(user).await?;
    if all.is_empty() {
        return Ok(None);
    }
    let content = Content::get();
    let now = app.now();
    let pages = all.len().div_ceil(PER_PAGE);
    let page = page.min(pages - 1);
    let shown: &[PetRow] = &all[page * PER_PAGE..((page + 1) * PER_PAGE).min(all.len())];

    let mut cells = Vec::new();
    let mut options = Vec::new();
    for pet in shown {
        let species = content.pet(&pet.species);
        let state = status(pet, now);
        let badge = match state {
            PetStatus::Awake => String::new(),
            PetStatus::Asleep => "ASLEEP".to_owned(),
            PetStatus::Away => format!(
                "AWAY {}",
                short_span(pet.trip_ends_at.map_or(0, |ends| (ends - now).num_seconds()))
            ),
            PetStatus::Back => "BACK!".to_owned(),
        };
        let needs = current_needs(pet, now);
        cells.push(Cell {
            species: pet.species.clone(),
            name: pet.name.clone(),
            level: pet.level,
            xp: match pets::xp_to_next(pet.level) {
                0 => 1.0,
                need => pet.xp as f32 / need as f32,
            },
            needs: [needs.hunger, needs.fun, needs.energy, needs.hygiene],
            frame: species.map_or(0x8fa0d8, |s| s.rarity.color()),
            active: pet.active,
            status: state,
            badge,
        });
        let feeling = match state {
            PetStatus::Awake => needs.mood(),
            PetStatus::Asleep => "asleep",
            PetStatus::Away => "away on a trip",
            PetStatus::Back => "souvenir waiting",
        };
        options.push(SelectMenuOption {
            default: pet.active,
            description: Some(format!(
                "Lv {} · {} · {feeling}",
                pet.level,
                species.map_or(pet.species.as_str(), |s| s.name.as_str())
            )),
            emoji: None,
            label: pet.name.clone(),
            value: pet.id.to_string(),
        });
    }

    let count = |wanted: PetStatus| all.iter().filter(|p| status(p, now) == wanted).count();
    let (away, asleep, back) = (count(PetStatus::Away), count(PetStatus::Asleep), count(PetStatus::Back));
    let mut footer = vec![format!("{} PETS", all.len())];
    for (n, what) in [(away, "AWAY"), (asleep, "ASLEEP"), (back, "WITH A SOUVENIR")] {
        if n > 0 {
            footer.push(format!("{n} {what}"));
        }
    }
    let sprites = app.sprites.clone();
    let card = NurseryCard {
        title: "Nursery".to_owned(),
        subtitle: format!("PAGE {}/{pages}", page + 1),
        pets: Vec::new(),
        footer: footer.join(" · "),
        theme: Theme::reef(),
    };
    let png = app
        .render(move |r| {
            let (bw, bh) = NurseryCard::SPRITE_BOX;
            let mut pets = Vec::with_capacity(cells.len());
            for cell in cells {
                let (sprite_png, sprite_w, sprite_h) = scaled_sprite(&sprites, &cell.species, bw, bh)?;
                pets.push(NurseryPet {
                    sprite_png,
                    sprite_w,
                    sprite_h,
                    name: cell.name,
                    level: cell.level,
                    xp: cell.xp,
                    needs: cell.needs,
                    frame: cell.frame,
                    active: cell.active,
                    status: cell.status,
                    badge: cell.badge,
                });
            }
            r.nursery_card(&NurseryCard { pets, ..card })
        })
        .await?;

    let mut description = format!("**{}** pets", all.len());
    if away > 0 {
        description.push_str(&format!(" · {away} away"));
    }
    if asleep > 0 {
        description.push_str(&format!(" · {asleep} asleep"));
    }
    if back > 0 {
        description.push_str(&format!(" · {back} back with a souvenir"));
    }
    description.push_str(".\nPick one below to open its card. ★ marks your active pet.");
    let embed = EmbedBuilder::new()
        .color(BRAND)
        .title("Your nursery")
        .description(description);
    let mut components = vec![select(format!("pn:o:{page}"), "Open a pet…", options)];
    if pages > 1 {
        components.push(row(vec![
            button_if(
                format!("pn:p:{}", page.saturating_sub(1)),
                "◀ Page",
                ButtonStyle::Secondary,
                page == 0,
            ),
            button_if(
                format!("pn:x:{page}"),
                &format!("{}/{pages}", page + 1),
                ButtonStyle::Secondary,
                true,
            ),
            button_if(
                format!("pn:p:{}", page + 1),
                "Page ▶",
                ButtonStyle::Secondary,
                page + 1 >= pages,
            ),
        ]));
    }
    Ok(Some(Cx::card(embed, "nursery.png", png, components, true)))
}

/// `/pet list`: the nursery, privately.
pub(super) async fn open(cx: &Cx, user: u64) -> anyhow::Result<()> {
    match nursery_card(&cx.app, user, 0).await? {
        Some(data) => cx.reply(data).await,
        None => cx.reply_ephemeral("No pets yet. Start with `/pet adopt`.").await,
    }
}

/// `pn:<action>:<arg>`: `p` a page, `o` a pet picked from the menu, `f` back to the page a pet
/// is on, `a` make a pet the active one.
pub(crate) async fn on_nursery(cx: &Cx, mut args: Split<'_, char>, value: Option<&str>) -> anyhow::Result<()> {
    let user = cx.author()?.id.get();
    let action = args.next();
    let arg: i64 = args.next().and_then(|v| v.parse().ok()).unwrap_or(0);
    match action {
        Some("p") => show_page(cx, user, usize::try_from(arg).unwrap_or(0)).await,
        Some("f") => {
            let at = cx.app.db.pets(user).await?.iter().position(|p| p.id == arg);
            show_page(cx, user, at.unwrap_or(0) / PER_PAGE).await
        }
        Some("o") => {
            let id: i64 = value.and_then(|v| v.parse().ok()).unwrap_or(0);
            open_pet(cx, user, id, None).await
        }
        Some("a") => {
            if !cx.app.db.set_active_pet(user, arg).await? {
                return cx.reply_ephemeral("That pet isn't yours.").await;
            }
            let note = "Active pet changed. It will greet visitors in your house.";
            open_pet(cx, user, arg, Some(note.to_owned())).await
        }
        _ => cx.reply_ephemeral("This button has expired.").await,
    }
}

async fn show_page(cx: &Cx, user: u64, page: usize) -> anyhow::Result<()> {
    match nursery_card(&cx.app, user, page).await? {
        Some(data) => cx.update(data).await,
        None => cx.reply_ephemeral("No pets yet. Start with `/pet adopt`.").await,
    }
}

/// Replaces the nursery with one pet's card.
async fn open_pet(cx: &Cx, user: u64, pet_id: i64, note: Option<String>) -> anyhow::Result<()> {
    let Some(pet) = cx.app.db.pets(user).await?.into_iter().find(|p| p.id == pet_id) else {
        return cx.reply_ephemeral("That pet isn't yours.").await;
    };
    let data = pet_card(&cx.app, &pet, note).await?;
    cx.update(data).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn corner_spans_are_short() {
        assert_eq!(short_span(0), "0M");
        assert_eq!(short_span(25 * 60), "25M");
        assert_eq!(short_span(2 * 3600), "2H");
        assert_eq!(short_span(2 * 3600 + 5 * 60), "2H5M");
    }
}
