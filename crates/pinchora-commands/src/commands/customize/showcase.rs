//! `/customize showcase`: a favourite fish, a pet, a piece of furniture and a thumbnail of the
//! house, shown in a row on the profile card.

use std::{str::Split, sync::Arc};

use pinchora_core::{i18n::Lang, t};
use pinchora_db::UserRow;
use pinchora_game::{Content, house};
use pinchora_render::{ShowcaseKind, ShowcaseSlot, SpriteArt};
use twilight_model::{channel::message::component::ButtonStyle, http::interaction::InteractionResponseData};
use twilight_util::builder::embed::EmbedBuilder;

use super::{PER_PAGE, button, expired, menu, option, profile, row, worn_badges};
use crate::{
    App, Cx,
    game::scene::{RoomView, ScaledArt, Sprites},
    util::BRAND,
};

// /Showcase

/// Which of the three picks the editor is on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Slot {
    Fish,
    Pet,
    Furniture,
}

impl Slot {
    const ALL: [Self; 3] = [Self::Fish, Self::Pet, Self::Furniture];

    /// The letter custom ids carry.
    fn letter(self) -> &'static str {
        match self {
            Self::Fish => "f",
            Self::Pet => "p",
            Self::Furniture => "i",
        }
    }

    fn from_letter(letter: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|s| s.letter() == letter)
    }

    fn label(self, lang: Lang) -> String {
        match self {
            Self::Fish => t!(lang, "customize.showcase.slot.fish"),
            Self::Pet => t!(lang, "customize.showcase.slot.pet"),
            Self::Furniture => t!(lang, "customize.showcase.slot.furniture"),
        }
    }

    /// The key suffix of the slot's menu texts.
    fn key(self) -> &'static str {
        match self {
            Self::Fish => "fish",
            Self::Pet => "pet",
            Self::Furniture => "furniture",
        }
    }
}

/// What a showcase tile is drawn from. The art is made (or fetched from the cache) on the render thread.
enum Source {
    Fish(String),
    Pet(String),
    Item(String),
    House(Box<RoomView>),
}

struct Tile {
    kind: ShowcaseKind,
    name: String,
    source: Source,
}

/// The tiles of a player's showcase, as far as they still hold: a fish that is not in the book,
/// a pet that left or furniture that was sold is skipped. Owned, so it can move to the render thread.
pub(crate) struct Showcase {
    tiles: Vec<Tile>,
}

impl Showcase {
    /// Reads what each pick needs to be checked against: one small query for each slot that has
    /// a pick (is the fish in the book, is the pet theirs, is the furniture theirs) and, for the
    /// house, its room. Nothing else is read.
    pub(crate) async fn load(app: &App, user: u64, row: &UserRow, lang: Lang) -> anyhow::Result<Self> {
        let content = Content::get();
        let mut tiles = Vec::new();
        if let Some(fish) = row.showcase_fish.as_deref().and_then(|id| content.fish(id))
            && app.db.has_fish(user, &fish.id).await?
        {
            tiles.push(Tile {
                kind: ShowcaseKind::Fish,
                name: content.name(&fish.id, lang).into_owned(),
                source: Source::Fish(fish.id.clone()),
            });
        }
        if let Some(pet) = row.showcase_pet
            && let Some((name, species)) = app.db.pet_name_species(user, pet).await?
        {
            tiles.push(Tile {
                kind: ShowcaseKind::Pet,
                name,
                source: Source::Pet(species),
            });
        }
        if let Some(item) = row.showcase_item.as_deref().and_then(|id| content.item(id))
            && app.db.owns_item(user, &item.id()).await?
        {
            tiles.push(Tile {
                kind: ShowcaseKind::Furniture,
                name: item.name_in(lang),
                source: Source::Item(item.id()),
            });
        }
        if row.showcase_house
            && let Some(preview) = app.db.house_preview(user).await?
        {
            let tier = house::tier(preview.house.tier);
            let name = match preview.house.name.trim() {
                "" => tier.name_in(lang).to_owned(),
                name => name.to_owned(),
            };
            tiles.push(Tile {
                kind: ShowcaseKind::House,
                name,
                source: Source::House(Box::new(RoomView {
                    width: tier.width,
                    depth: tier.depth,
                    room: preview.room,
                    placements: preview.placements,
                    cursor: None,
                    selected: None,
                    edit: false,
                    pets: Vec::new(),
                    characters: Vec::new(),
                    seed: 0,
                })),
            });
        }
        Ok(Self { tiles })
    }

    /// The art of each tile with its kind and name. Slow the first time (a room is drawn), so
    /// call it on the render thread.
    pub(crate) fn art(&self, sprites: &Sprites) -> Vec<(ShowcaseKind, &str, Arc<ScaledArt>)> {
        let content = Content::get();
        self.tiles
            .iter()
            .filter_map(|tile| {
                let art = match &tile.source {
                    Source::Fish(id) => sprites.showcase_fish(id),
                    Source::Pet(species) => sprites.showcase_pet(species),
                    Source::Item(id) => sprites.showcase_item(content, id),
                    Source::House(view) => sprites.house_thumbnail(view),
                }?;
                Some((tile.kind, tile.name.as_str(), art))
            })
            .collect()
    }

    /// The tiles of `art` as the card draws them.
    pub(crate) fn slots<'a>(art: &'a [(ShowcaseKind, &'a str, Arc<ScaledArt>)]) -> Vec<ShowcaseSlot<'a>> {
        art.iter()
            .map(|(kind, name, art)| ShowcaseSlot {
                kind: *kind,
                art: SpriteArt {
                    png: &art.png,
                    w: art.w,
                    h: art.h,
                },
                name,
            })
            .collect()
    }

    fn is_empty(&self) -> bool {
        self.tiles.is_empty()
    }

    fn name_of(&self, kind: ShowcaseKind) -> Option<&str> {
        self.tiles.iter().find(|t| t.kind == kind).map(|t| t.name.as_str())
    }
}

/// One thing the player may pick for a slot.
struct Choice {
    value: String,
    label: String,
    description: String,
}

/// The line under a furniture piece in the menu: its rarity, and "Soulbound" for a piece bought with
/// fragments, so it is not mistaken for its unbound twin, which has the same name.
fn furniture_note(rarity: &str, soulbound: bool, lang: Lang) -> String {
    if soulbound {
        t!(lang, "customize.showcase.soulbound", rarity = rarity)
    } else {
        rarity.to_owned()
    }
}

/// What the player may pick for `slot`, by name.
async fn choices(app: &App, user: u64, slot: Slot, lang: Lang) -> anyhow::Result<Vec<Choice>> {
    let content = Content::get();
    let mut choices: Vec<Choice> = match slot {
        Slot::Fish => app
            .db
            .fish_book(user)
            .await?
            .into_iter()
            .filter_map(|b| {
                let fish = content.fish(&b.fish_id)?;
                Some(Choice {
                    value: fish.id.clone(),
                    label: content.name(&fish.id, lang).into_owned(),
                    description: t!(
                        lang,
                        "customize.showcase.fish-description",
                        rarity = fish.rarity.label_in(lang),
                        best = pinchora_core::server_stats::decimal_in(lang, b.best_cm, 1),
                        caught = b.caught
                    ),
                })
            })
            .collect(),
        Slot::Pet => app
            .db
            .pets(user)
            .await?
            .into_iter()
            .map(|p| Choice {
                value: p.id.to_string(),
                description: t!(
                    lang,
                    "customize.showcase.pet-description",
                    species = content.name(&p.species, lang),
                    level = p.level
                ),
                label: p.name,
            })
            .collect(),
        Slot::Furniture => app
            .db
            .owned_item_ids(user)
            .await?
            .iter()
            .filter_map(|id| {
                let item = content.item(id)?;
                Some(Choice {
                    value: item.id(),
                    label: item.name_in(lang),
                    description: furniture_note(item.base.rarity.label_in(lang), item.bound, lang),
                })
            })
            .collect(),
    };
    choices.sort_by(|a, b| {
        a.label
            .to_lowercase()
            .cmp(&b.label.to_lowercase())
            .then(a.value.cmp(&b.value))
    });
    Ok(choices)
}

/// The value of the player's current pick for `slot`, in the form `choices` uses.
fn current(row: &UserRow, slot: Slot) -> Option<String> {
    match slot {
        Slot::Fish => row.showcase_fish.clone(),
        Slot::Pet => row.showcase_pet.map(|p| p.to_string()),
        Slot::Furniture => row.showcase_item.clone(),
    }
}

/// The ephemeral showcase editor: the profile card with the showcase as it stands, a menu for
/// the slot being edited, a menu of what the player may put in it, and buttons to clear a slot,
/// show the house or empty everything.
pub(super) async fn panel(cx: &Cx, slot: Slot, page: usize, note: &str) -> anyhow::Result<InteractionResponseData> {
    let lang = cx.lang();
    let content = Content::get();
    let user = cx.author()?;
    let id = user.id.get();
    let player = cx.app.db.user(id).await?;
    let showcase = Showcase::load(&cx.app, id, &player, lang).await?;
    // A house on the card has a room; without one on it, a single question tells.
    let has_house = showcase.name_of(ShowcaseKind::House).is_some() || cx.app.db.has_house_room(id).await?;
    let picked = current(&player, slot);
    let choices = choices(&cx.app, id, slot, lang).await?;
    let pages = choices.len().div_ceil(PER_PAGE).max(1);
    let page = page.min(pages - 1);

    let slot_options = Slot::ALL
        .iter()
        .map(|s| {
            let kind = match s {
                Slot::Fish => ShowcaseKind::Fish,
                Slot::Pet => ShowcaseKind::Pet,
                Slot::Furniture => ShowcaseKind::Furniture,
            };
            option(
                &s.label(lang),
                s.letter(),
                showcase
                    .name_of(kind)
                    .map_or_else(|| t!(lang, "customize.showcase.empty"), str::to_owned),
                *s == slot,
            )
        })
        .collect();
    let options = choices
        .iter()
        .skip(page * PER_PAGE)
        .take(PER_PAGE)
        .map(|c| {
            option(
                &c.label,
                &c.value,
                c.description.clone(),
                picked.as_deref() == Some(c.value.as_str()),
            )
        })
        .collect();
    let placeholder = t!(lang, &format!("customize.showcase.placeholder.{}", slot.key()));
    let empty = t!(lang, &format!("customize.showcase.nothing.{}", slot.key()));

    let (house_on, empty_showcase) = (player.showcase_house, showcase.is_empty());
    let slot_filled = picked.is_some();
    let worn = worn_badges(&cx.app, id, &player).await?;
    let background = player.background.as_deref().and_then(|e| content.background(e));

    // The text first: the card takes the showcase over, so that it is read once for both.
    let mut text = String::new();
    if !note.is_empty() {
        text.push_str(note);
        text.push_str("\n\n");
    }
    let named = |kind| showcase.name_of(kind).unwrap_or("-").to_owned();
    text.push_str(&t!(
        lang,
        "customize.showcase.summary",
        fish = named(ShowcaseKind::Fish),
        pet = named(ShowcaseKind::Pet),
        furniture = named(ShowcaseKind::Furniture),
        house = if showcase.name_of(ShowcaseKind::House).is_some() {
            t!(lang, "customize.showcase.house-shown")
        } else {
            t!(lang, "customize.showcase.house-hidden")
        }
    ));
    if empty_showcase {
        text.push_str(&t!(lang, "customize.showcase.empty-showcase"));
    }
    let png = Box::pin(profile::card_with(
        cx,
        user,
        cx.author_display_name(),
        player,
        background,
        &worn,
        showcase,
    ))
    .await?;

    let letter = slot.letter();
    let mut buttons = Vec::new();
    if pages > 1 {
        buttons.push(button(
            format!("sw:p:{letter}:{}", page.saturating_sub(1)),
            &t!(lang, "customize.prev-page"),
            ButtonStyle::Secondary,
            page == 0,
        ));
        buttons.push(button(
            format!("sw:p:{letter}:{}", page + 1),
            &t!(lang, "customize.next-page"),
            ButtonStyle::Secondary,
            page + 1 >= pages,
        ));
    }
    buttons.push(button(
        format!("sw:c:{letter}"),
        &t!(lang, "customize.showcase.clear-slot"),
        ButtonStyle::Secondary,
        !slot_filled,
    ));
    buttons.push(button(
        format!("sw:h:{letter}"),
        &if house_on {
            t!(lang, "customize.showcase.hide-house")
        } else {
            t!(lang, "customize.showcase.show-house")
        },
        if house_on {
            ButtonStyle::Primary
        } else {
            ButtonStyle::Secondary
        },
        !house_on && !has_house,
    ));
    buttons.push(button(
        format!("sw:x:{letter}"),
        &t!(lang, "customize.showcase.clear-all"),
        ButtonStyle::Danger,
        empty_showcase && !house_on,
    ));
    let components = vec![
        row(vec![menu(
            "sw:k".to_owned(),
            &t!(lang, "customize.showcase.slot-menu"),
            slot_options,
            1,
            1,
            &t!(lang, "customize.nothing-to-show"),
        )]),
        row(vec![menu(
            format!("sw:s:{letter}:{page}"),
            &placeholder,
            options,
            1,
            1,
            &empty,
        )]),
        row(buttons),
    ];
    let embed = EmbedBuilder::new()
        .color(BRAND)
        .title(t!(lang, "customize.showcase.title"))
        .description(text);
    Ok(Cx::card(embed, "profile.png", png, components, true))
}

/// Saves `value` as the pick for `slot` if it is something the player may pick; the name of
/// what was saved, or `None` for a refused pick. Only the one pick is checked, not everything
/// the player could have picked.
async fn pick(app: &App, user: u64, slot: Slot, value: &str, lang: Lang) -> anyhow::Result<Option<String>> {
    let content = Content::get();
    Ok(match slot {
        Slot::Fish => match content.fish(value) {
            Some(fish) if app.db.has_fish(user, &fish.id).await? => {
                app.db.set_showcase_fish(user, Some(&fish.id)).await?;
                Some(content.name(&fish.id, lang).into_owned())
            }
            _ => None,
        },
        Slot::Pet => match value.parse::<i64>() {
            Ok(id) => match app.db.pet_name_species(user, id).await? {
                Some((name, _)) => {
                    app.db.set_showcase_pet(user, Some(id)).await?;
                    Some(name)
                }
                None => None,
            },
            Err(_) => None,
        },
        Slot::Furniture => match content.item(value) {
            Some(item) if app.db.owns_item(user, &item.id()).await? => {
                app.db.set_showcase_item(user, Some(&item.id())).await?;
                Some(item.name_in(lang))
            }
            _ => None,
        },
    })
}

/// `sw:<what>:...` selects and buttons of the showcase editor.
pub(crate) async fn on_showcase(cx: &Cx, mut args: Split<'_, char>, values: &[String]) -> anyhow::Result<()> {
    let lang = cx.lang();
    let id = cx.author()?.id.get();
    let value = values.first().map(String::as_str);
    let slot_arg = |arg: Option<&str>| arg.and_then(Slot::from_letter).unwrap_or(Slot::Fish);
    match args.next() {
        Some("k") => {
            let Some(slot) = value.and_then(Slot::from_letter) else {
                return expired(cx, "showcase").await;
            };
            let data = panel(cx, slot, 0, "").await?;
            cx.update(data).await
        }
        Some("s") => {
            let slot = slot_arg(args.next());
            let page = args.next().and_then(|p| p.parse().ok()).unwrap_or(0);
            let Some(name) = (match value {
                Some(value) => pick(&cx.app, id, slot, value, lang).await?,
                None => None,
            }) else {
                return expired(cx, "showcase").await;
            };
            let data = panel(cx, slot, page, &t!(lang, "customize.showcase.saved", name = name)).await?;
            cx.update(data).await
        }
        Some("p") => {
            let slot = slot_arg(args.next());
            let page = args.next().and_then(|p| p.parse().ok()).unwrap_or(0);
            let data = panel(cx, slot, page, "").await?;
            cx.update(data).await
        }
        Some("c") => {
            let slot = slot_arg(args.next());
            match slot {
                Slot::Fish => cx.app.db.set_showcase_fish(id, None).await?,
                Slot::Pet => cx.app.db.set_showcase_pet(id, None).await?,
                Slot::Furniture => cx.app.db.set_showcase_item(id, None).await?,
            }
            let note = t!(lang, "customize.showcase.cleared-slot", slot = slot.label(lang));
            let data = panel(cx, slot, 0, &note).await?;
            cx.update(data).await
        }
        Some("h") => {
            let slot = slot_arg(args.next());
            let shown = cx.app.db.user(id).await?.showcase_house;
            let note = if !shown && !cx.app.db.has_house_room(id).await? {
                t!(lang, "customize.showcase.no-house")
            } else {
                cx.app.db.set_showcase_house(id, !shown).await?;
                if shown {
                    t!(lang, "customize.showcase.house-off")
                } else {
                    t!(lang, "customize.showcase.house-on")
                }
            };
            let data = panel(cx, slot, 0, &note).await?;
            cx.update(data).await
        }
        Some("x") => {
            let slot = slot_arg(args.next());
            cx.app.db.clear_showcase(id).await?;
            let data = panel(cx, slot, 0, &t!(lang, "customize.showcase.cleared-all")).await?;
            cx.update(data).await
        }
        _ => cx.reply_ephemeral(t!(lang, "customize.showcase.expired")).await,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slots_round_trip_through_their_letters() {
        for slot in Slot::ALL {
            assert_eq!(Slot::from_letter(slot.letter()), Some(slot));
        }
        assert_eq!(Slot::from_letter("z"), None);
        assert_eq!(Slot::from_letter(""), None);
    }

    #[test]
    fn a_soulbound_piece_is_told_from_its_unbound_twin() {
        assert_eq!(furniture_note("Rare", false, Lang::En), "Rare");
        assert_eq!(furniture_note("Rare", true, Lang::En), "Rare · Soulbound");
        assert_eq!(furniture_note("Nadir", true, Lang::Tr), "Nadir · Ruh bağlı");
    }

    #[test]
    fn custom_ids_fit_discords_limit_for_every_item() {
        let content = Content::get();
        for item in content.items() {
            let value = item.id();
            assert!(
                format!("sw:s:i:999:{value}").len() <= 100 && value.len() <= 100,
                "{value}"
            );
            assert!(item.name().chars().count() <= 100, "{value}");
        }
    }
}
