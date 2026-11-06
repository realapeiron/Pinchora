//! `/customize showcase`: a favourite fish, a pet, a piece of furniture and a thumbnail of the
//! house, shown in a row on the profile card.

use std::{str::Split, sync::Arc};

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

    fn label(self) -> &'static str {
        match self {
            Self::Fish => "Favourite fish",
            Self::Pet => "Pet",
            Self::Furniture => "Furniture",
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
    pub(crate) async fn load(app: &App, user: u64, row: &UserRow) -> anyhow::Result<Self> {
        let content = Content::get();
        let mut tiles = Vec::new();
        if let Some(fish) = row.showcase_fish.as_deref().and_then(|id| content.fish(id))
            && app.db.has_fish(user, &fish.id).await?
        {
            tiles.push(Tile {
                kind: ShowcaseKind::Fish,
                name: fish.name.clone(),
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
                name: item.name(),
                source: Source::Item(item.id()),
            });
        }
        if row.showcase_house
            && let Some(preview) = app.db.house_preview(user).await?
        {
            let tier = house::tier(preview.house.tier);
            let name = match preview.house.name.trim() {
                "" => tier.name.to_owned(),
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

/// What the player may pick for `slot`, by name.
async fn choices(app: &App, user: u64, slot: Slot) -> anyhow::Result<Vec<Choice>> {
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
                    label: fish.name.clone(),
                    description: format!(
                        "{} · best {:.1} cm · caught {}",
                        fish.rarity.label(),
                        b.best_cm,
                        b.caught
                    ),
                })
            })
            .collect(),
        Slot::Pet => app
            .db
            .pets(user)
            .await?
            .into_iter()
            .map(|p| {
                let species = content.pet(&p.species).map_or(p.species.as_str(), |s| &s.name);
                Choice {
                    value: p.id.to_string(),
                    description: format!("{species} · level {}", p.level),
                    label: p.name,
                }
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
                    label: item.name(),
                    description: item.base.rarity.label().to_owned(),
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
    let content = Content::get();
    let user = cx.author()?;
    let id = user.id.get();
    let player = cx.app.db.user(id).await?;
    let showcase = Showcase::load(&cx.app, id, &player).await?;
    // A house on the card has a room; without one on it, a single question tells.
    let has_house = showcase.name_of(ShowcaseKind::House).is_some() || cx.app.db.has_house_room(id).await?;
    let picked = current(&player, slot);
    let choices = choices(&cx.app, id, slot).await?;
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
                s.label(),
                s.letter(),
                showcase.name_of(kind).unwrap_or("Empty").to_owned(),
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
    let (placeholder, empty) = match slot {
        Slot::Fish => ("Pick a favourite fish…", "Catch a fish with /fish first"),
        Slot::Pet => ("Pick a pet…", "Adopt a pet with /pet first"),
        Slot::Furniture => ("Pick a furniture item…", "Buy furniture with /shop first"),
    };

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
    let line = |kind, label: &str| format!("**{label}** {}", showcase.name_of(kind).unwrap_or("-"));
    text.push_str(&format!(
        "{} · {} · {} · **House** {}",
        line(ShowcaseKind::Fish, "Fish"),
        line(ShowcaseKind::Pet, "Pet"),
        line(ShowcaseKind::Furniture, "Furniture"),
        if showcase.name_of(ShowcaseKind::House).is_some() {
            "shown"
        } else {
            "hidden"
        },
    ));
    if empty_showcase {
        text.push_str("\nNothing is on your showcase, so your card looks as it always did.");
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
            "◀ Page",
            ButtonStyle::Secondary,
            page == 0,
        ));
        buttons.push(button(
            format!("sw:p:{letter}:{}", page + 1),
            "Page ▶",
            ButtonStyle::Secondary,
            page + 1 >= pages,
        ));
    }
    buttons.push(button(
        format!("sw:c:{letter}"),
        "Clear slot",
        ButtonStyle::Secondary,
        !slot_filled,
    ));
    buttons.push(button(
        format!("sw:h:{letter}"),
        if house_on { "Hide house" } else { "Show house" },
        if house_on {
            ButtonStyle::Primary
        } else {
            ButtonStyle::Secondary
        },
        !house_on && !has_house,
    ));
    buttons.push(button(
        format!("sw:x:{letter}"),
        "Clear all",
        ButtonStyle::Danger,
        empty_showcase && !house_on,
    ));
    let components = vec![
        row(vec![menu(
            "sw:k".to_owned(),
            "Slot to edit…",
            slot_options,
            1,
            1,
            "Nothing to show",
        )]),
        row(vec![menu(
            format!("sw:s:{letter}:{page}"),
            placeholder,
            options,
            1,
            1,
            empty,
        )]),
        row(buttons),
    ];
    let embed = EmbedBuilder::new()
        .color(BRAND)
        .title("Profile showcase")
        .description(text);
    Ok(Cx::card(embed, "profile.png", png, components, true))
}

/// Saves `value` as the pick for `slot` if it is something the player may pick; the name of
/// what was saved, or `None` for a refused pick. Only the one pick is checked, not everything
/// the player could have picked.
async fn pick(app: &App, user: u64, slot: Slot, value: &str) -> anyhow::Result<Option<String>> {
    let content = Content::get();
    Ok(match slot {
        Slot::Fish => match content.fish(value) {
            Some(fish) if app.db.has_fish(user, &fish.id).await? => {
                app.db.set_showcase_fish(user, Some(&fish.id)).await?;
                Some(fish.name.clone())
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
                Some(item.name())
            }
            _ => None,
        },
    })
}

/// `sw:<what>:...` selects and buttons of the showcase editor.
pub(crate) async fn on_showcase(cx: &Cx, mut args: Split<'_, char>, values: &[String]) -> anyhow::Result<()> {
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
                Some(value) => pick(&cx.app, id, slot, value).await?,
                None => None,
            }) else {
                return expired(cx, "showcase").await;
            };
            let data = panel(cx, slot, page, &format!("Saved. **{name}** is on your profile card.")).await?;
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
            let data = panel(cx, slot, 0, &format!("Cleared. {} is off your card.", slot.label())).await?;
            cx.update(data).await
        }
        Some("h") => {
            let slot = slot_arg(args.next());
            let shown = cx.app.db.user(id).await?.showcase_house;
            let note = if !shown && !cx.app.db.has_house_room(id).await? {
                "You have no house yet. Open it with `/house edit` first.".to_owned()
            } else {
                cx.app.db.set_showcase_house(id, !shown).await?;
                if shown {
                    "Your house is off your card.".to_owned()
                } else {
                    "Your house is on your card.".to_owned()
                }
            };
            let data = panel(cx, slot, 0, &note).await?;
            cx.update(data).await
        }
        Some("x") => {
            let slot = slot_arg(args.next());
            cx.app.db.clear_showcase(id).await?;
            let data = panel(cx, slot, 0, "Cleared. Your card looks as it always did.").await?;
            cx.update(data).await
        }
        _ => cx.reply_ephemeral("This button has expired.").await,
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
