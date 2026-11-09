//! The editor message: the preview card with the open tab's choices, and its menus and buttons.

use image::{RgbaImage, imageops};
use pinchora_core::infra::i18n::Lang;
use pinchora_core::t;
use pinchora_db::OutfitRow;
use pinchora_game::{
    Content,
    character::{Access, Character, OUTFIT_SLOTS, Shelf, accessories_in, accessory, presets},
};
use pinchora_render::{
    CharacterCard, EditorCell, EditorSection, Theme,
    character::{
        ACCESSORY_TINTS, Channel, OUTLINE, Paint, SHOE_KEY, Slot, accessory_icon, compose, ramp_of, render_character,
    },
};
use twilight_model::{
    channel::message::{
        Component, EmojiReactionType,
        component::{ActionRow, Button, ButtonStyle, SelectMenu, SelectMenuOption, SelectMenuType},
    },
    http::interaction::InteractionResponseData,
};
use twilight_util::builder::embed::EmbedBuilder;

use crate::Cx;
use crate::commands::social::character::Draft;
use crate::commands::social::character::Tab;
use crate::commands::social::character::access_note;
use crate::commands::social::character::load_shelf;
use crate::infra::util::BRAND;

fn row(components: Vec<Component>) -> Component {
    Component::ActionRow(ActionRow { id: None, components })
}

fn button(custom_id: String, label: &str, style: ButtonStyle, disabled: bool) -> Component {
    Component::Button(Button {
        custom_id: Some(custom_id),
        disabled,
        emoji: None,
        id: None,
        label: Some(label.to_owned()),
        sku_id: None,
        style,
        url: None,
    })
}

fn menu(custom_id: String, placeholder: &str, options: Vec<SelectMenuOption>) -> Component {
    Component::SelectMenu(SelectMenu {
        channel_types: None,
        custom_id,
        default_values: None,
        disabled: false,
        id: None,
        kind: SelectMenuType::Text,
        max_values: Some(1),
        min_values: Some(1),
        options: Some(options),
        placeholder: Some(placeholder.to_owned()),
        required: None,
    })
}

fn option(label: &str, value: &str, description: Option<String>, default: bool) -> SelectMenuOption {
    SelectMenuOption {
        default,
        description: description.map(|d| d.chars().take(100).collect()),
        emoji: None,
        label: label.chars().take(100).collect(),
        value: value.to_owned(),
    }
}

fn locked(mut o: SelectMenuOption) -> SelectMenuOption {
    o.emoji = Some(EmojiReactionType::Unicode {
        name: "🔒".to_owned()
    });
    o
}

/// `0xrrggbb` of a colour.
fn hex([r, g, b]: [u8; 3]) -> u32 {
    (u32::from(r) << 16) | (u32::from(g) << 8) | u32::from(b)
}

/// The shades a swatch shows on the card; the art's own for "as drawn".
fn swatch_ramp(channel: Channel, paint: Paint) -> Vec<u32> {
    match ramp_of(channel.swatches(), paint, 4, 2) {
        Some(r) => r.into_iter().map(hex).collect(),
        None if channel == Channel::Shoes => SHOE_KEY.iter().copied().map(hex).collect(),
        None => [OUTLINE, [0x3b, 0x2a, 0x24], SHOE_KEY[2], [0xff, 0xf1, 0xe8]]
            .into_iter()
            .map(hex)
            .collect(),
    }
}

/// The editor: card, menus and buttons for the draft.
pub(super) async fn editor(cx: &Cx, draft: &Draft, note: &str) -> anyhow::Result<InteractionResponseData> {
    let user = cx.author()?.id.get();
    let shelf = load_shelf(&cx.app, user).await?;
    let outfits = cx.app.db.outfits(user).await?;
    let character = draft.character();
    let components = controls(draft, &character, &shelf, &outfits, cx.lang());
    let png = {
        let (draft, name, lang) = (draft.clone(), cx.author_display_name(), cx.lang());
        let accesses: Vec<(String, Access)> = pinchora_game::character::accessories()
            .iter()
            .map(|a| (a.id.clone(), shelf.access(a)))
            .collect();
        cx.app
            .render(move |r| r.character_card(&card(&draft, &character, name, &accesses, &outfits, lang)))
            .await?
    };
    let mut text = note.to_owned();
    if !draft.is_saved() {
        if !text.is_empty() {
            text.push('\n');
        }
        text.push_str(&t!(cx.lang(), "character.draft"));
    }
    let embed = EmbedBuilder::new()
        .color(BRAND)
        .title(t!(cx.lang(), "character.title"))
        .description(text);
    Ok(Cx::card(embed, "character.png", png, components, true))
}

/// The message once the editor is closed: the saved character, no controls. `lost`: the draft
/// held changes that were not saved.
pub(super) async fn closed(cx: &Cx, saved: Option<&str>, lost: bool) -> anyhow::Result<InteractionResponseData> {
    let content = Content::get();
    let character = saved
        .and_then(|c| Character::decode(content, c))
        .unwrap_or_else(|| Character::starter(content));
    let name = cx.author_display_name();
    let lang = cx.lang();
    let png = cx
        .app
        .render(move |r| {
            r.character_card(&CharacterCard {
                character: render_character(&character.spec(), 1),
                name,
                status: (t!(lang, "character.closed-status"), true),
                tabs: Vec::new(),
                tab: usize::MAX,
                caption: t!(
                    lang,
                    if lost {
                        "character.closed-lost"
                    } else {
                        "character.closed"
                    }
                ),
                sections: Vec::new(),
                hint: t!(lang, "character.closed-hint"),
                theme: Theme::reef(),
            })
        })
        .await?;
    let embed = EmbedBuilder::new()
        .color(BRAND)
        .title(t!(lang, "character.title"))
        .description(t!(
            lang,
            if lost {
                "character.closed-lost-text"
            } else {
                "character.closed-text"
            }
        ));
    Ok(Cx::card(embed, "character.png", png, Vec::new(), true))
}

/// Menus and buttons for the open tab.
fn controls(draft: &Draft, character: &Character, shelf: &Shelf, outfits: &[OutfitRow], lang: Lang) -> Vec<Component> {
    let s = draft.session;
    let content = Content::get();
    let tabs = Tab::ALL
        .iter()
        .map(|t| option(&t.name(lang), t.key(), None, *t == draft.tab))
        .collect();
    let jump: Vec<SelectMenuOption> = match draft.tab {
        Tab::Look => content
            .looks
            .iter()
            .map(|l| option(&l.name, &l.id, None, l.id == character.look))
            .collect(),
        Tab::Paint(channel) => {
            let now = character.colors.get(channel);
            let mut options: Vec<SelectMenuOption> = channel
                .swatches()
                .iter()
                .enumerate()
                .map(|(i, sw)| {
                    let note = if sw.is_natural() {
                        t!(lang, "character.natural")
                    } else {
                        format!("#{:06x}", sw.rgb)
                    };
                    option(sw.name, &i.to_string(), Some(note), now == Paint::Swatch(i as u8))
                })
                .collect();
            if let Paint::Custom(v) = now {
                options.push(option(
                    &t!(lang, "character.custom", hex = format!("#{v:06x}")),
                    "custom",
                    Some(t!(lang, "character.typed")),
                    true,
                ));
            }
            options
        }
        Tab::Gear => {
            let worn = character.worn(draft.slot).map(|w| w.id.clone());
            std::iter::once(option(&t!(lang, "character.nothing"), "none", None, worn.is_none()))
                .chain(accessories_in(draft.slot).map(|a| {
                    let access = shelf.access(a);
                    let o = option(
                        &a.name,
                        &a.id,
                        Some(access_note(&access, lang)),
                        worn.as_deref() == Some(&a.id),
                    );
                    if access == Access::Owned { o } else { locked(o) }
                }))
                .collect()
        }
        Tab::Presets => presets()
            .iter()
            .map(|p| option(&p.name, &p.id, None, p.code == draft.code))
            .collect(),
        Tab::Outfits => {
            // Two slots can keep the same code; a one-value menu marks only the first.
            let worn = outfits.iter().find(|o| o.code == draft.code).map(|o| o.slot);
            (1..=OUTFIT_SLOTS)
                .map(|slot| {
                    let saved = outfits.iter().find(|o| o.slot == slot);
                    let label = saved.map_or_else(
                        || t!(lang, "character.slot-empty", slot = slot),
                        |o| format!("{slot} · {}", o.name),
                    );
                    let worn = worn == Some(slot);
                    option(
                        &label,
                        &slot.to_string(),
                        Some(t!(
                            lang,
                            if saved.is_some() {
                                "character.put-on"
                            } else {
                                "character.nothing-kept"
                            }
                        )),
                        worn,
                    )
                })
                .collect()
        }
    };
    let placeholder = t!(
        lang,
        match draft.tab {
            Tab::Look => "character.jump-look",
            Tab::Paint(_) => "character.jump-colour",
            Tab::Gear => "character.jump-gear",
            Tab::Presets => "character.jump-preset",
            Tab::Outfits => "character.jump-outfit",
        }
    );
    let label = |key: &str| t!(lang, &format!("character.button.{key}"));
    let mut step_row = vec![
        button(format!("ca:prev:{s}"), "◀", ButtonStyle::Secondary, false),
        button(format!("ca:next:{s}"), "▶", ButtonStyle::Secondary, false),
        button(
            format!("ca:undo:{s}"),
            &label("undo"),
            ButtonStyle::Secondary,
            draft.undo.is_empty(),
        ),
        button(
            format!("ca:rand:{s}"),
            &label("randomise"),
            ButtonStyle::Secondary,
            false,
        ),
    ];
    let worn = character.worn(draft.slot);
    match draft.tab {
        Tab::Paint(_) => step_row.push(button(
            format!("ca:hex:{s}"),
            &label("hex"),
            ButtonStyle::Secondary,
            false,
        )),
        Tab::Gear => {
            let dyeable = worn.and_then(|w| accessory(&w.id)).is_some_and(|a| a.tintable);
            step_row.push(button(
                format!("ca:tint:{s}"),
                &label("dye"),
                ButtonStyle::Secondary,
                !dyeable,
            ));
        }
        _ => {}
    }
    let mut rows = vec![
        row(vec![menu(format!("ca:tab:{s}"), &label("category"), tabs)]),
        row(vec![menu(format!("ca:pick:{s}"), &placeholder, jump)]),
        row(step_row),
    ];
    match draft.tab {
        Tab::Gear => rows.push(row(Slot::ALL
            .iter()
            .map(|slot| {
                let style = if *slot == draft.slot {
                    ButtonStyle::Primary
                } else {
                    ButtonStyle::Secondary
                };
                button(format!("ca:slot:{s}:{}", slot.key()), slot.label(), style, false)
            })
            .collect())),
        Tab::Outfits => {
            let mut keep: Vec<Component> = (1..=OUTFIT_SLOTS)
                .map(|n| {
                    button(
                        format!("ca:keep:{s}:{n}"),
                        &t!(lang, "character.button.keep", slot = n),
                        ButtonStyle::Secondary,
                        false,
                    )
                })
                .collect();
            keep.push(button(
                format!("ca:name:{s}"),
                &label("rename"),
                ButtonStyle::Secondary,
                outfits.is_empty(),
            ));
            rows.push(row(keep));
        }
        _ => {}
    }
    let mut last = vec![button(
        format!("ca:save:{s}"),
        &label("save"),
        ButtonStyle::Success,
        draft.is_saved(),
    )];
    if draft.tab == Tab::Gear
        && let Some(item) = worn.and_then(|w| accessory(&w.id))
        && let Access::Buy(price) = shelf.access(item)
    {
        let buy = t!(
            lang,
            "character.button.buy",
            item = item.name,
            price = crate::infra::util::thousands_in(lang, price)
        );
        last.push(button(format!("ca:buy:{s}"), &buy, ButtonStyle::Primary, false));
    }
    last.push(button(
        format!("ca:cancel:{s}"),
        &label("cancel"),
        ButtonStyle::Danger,
        false,
    ));
    rows.push(row(last));
    rows
}

/// The top of a character down to its collar, for the look strip's smaller tiles.
fn trimmed(img: &RgbaImage) -> RgbaImage {
    let (mut x0, mut y0, mut x1, mut y1) = (u32::MAX, u32::MAX, 0, 0);
    for (x, y, p) in img.enumerate_pixels() {
        if p[3] > 0 {
            (x0, y0, x1, y1) = (x0.min(x), y0.min(y), x1.max(x), y1.max(y));
        }
    }
    if x0 > x1 {
        return img.clone();
    }
    imageops::crop_imm(img, x0, y0, x1 - x0 + 1, y1 - y0 + 1).to_image()
}

/// The card: the draft on its tile and the open tab's choices.
fn card(
    draft: &Draft,
    character: &Character,
    name: String,
    accesses: &[(String, Access)],
    outfits: &[OutfitRow],
    lang: Lang,
) -> CharacterCard {
    let content = Content::get();
    let access_of = |id: &str| {
        accesses
            .iter()
            .find(|(a, _)| a == id)
            .map_or(Access::Owned, |(_, x)| x.clone())
    };
    let tab_index = Tab::ALL.iter().position(|t| *t == draft.tab).unwrap_or(0);
    let (caption, sections) = match draft.tab {
        Tab::Look => {
            let n = content.looks.len();
            let at = content.looks.iter().position(|l| l.id == character.look).unwrap_or(0);
            let cells = (0..4)
                .map(|k| {
                    let look = &content.looks[(at + n + k - 1) % n];
                    let mut c = character.clone();
                    c.look = look.id.clone();
                    c.accessories.clear();
                    EditorCell {
                        art: Some(trimmed(&compose(&c.spec()).image)),
                        label: look.name.clone(),
                        current: look.id == character.look,
                        ..Default::default()
                    }
                })
                .collect();
            let name = content.look(&character.look).map_or("?", |l| l.name.as_str());
            (
                t!(lang, "character.card.look", name = name, at = at + 1, count = n),
                vec![EditorSection {
                    title: t!(lang, "character.card.look-hint", count = n),
                    cols: 4,
                    cells,
                }],
            )
        }
        Tab::Paint(channel) => {
            let now = character.colors.get(channel);
            let cells = (0..channel.swatches().len())
                .map(|i| {
                    let paint = Paint::Swatch(i as u8);
                    EditorCell {
                        swatch: Some(swatch_ramp(channel, paint)),
                        label: channel.swatches()[i].name.to_owned(),
                        current: now == paint,
                        ..Default::default()
                    }
                })
                .collect();
            (
                format!(
                    "{} · {}",
                    crate::commands::social::character::channel_name(channel, lang),
                    channel.name_of(now)
                ),
                vec![EditorSection {
                    title: t!(
                        lang,
                        "character.card.colour-hint",
                        channel = crate::commands::social::character::channel_name(channel, lang)
                    ),
                    cols: 5,
                    cells,
                }],
            )
        }
        Tab::Gear => {
            let wearing = Slot::ALL
                .iter()
                .map(|slot| {
                    let worn = character.worn(*slot);
                    EditorCell {
                        art: worn.and_then(|w| accessory_icon(&w.id, w.tint, 1)),
                        label: slot.label().to_owned(),
                        current: *slot == draft.slot,
                        ..Default::default()
                    }
                })
                .collect();
            let worn = character.worn(draft.slot);
            let items = std::iter::once(EditorCell {
                label: t!(lang, "character.nothing"),
                current: worn.is_none(),
                ..Default::default()
            })
            .chain(accessories_in(draft.slot).map(|a| {
                let access = access_of(&a.id);
                let tint = worn.filter(|w| w.id == a.id).and_then(|w| w.tint);
                EditorCell {
                    art: accessory_icon(&a.id, tint, 1),
                    label: a.name.clone(),
                    note: match &access {
                        Access::Owned => String::new(),
                        Access::Buy(p) => compact_price(lang, *p),
                        Access::Level(at) => t!(lang, "character.card.level", level = at),
                        Access::Badge(_) => t!(lang, "character.card.badge"),
                    },
                    current: worn.is_some_and(|w| w.id == a.id),
                    locked: !matches!(access, Access::Owned | Access::Buy(_)),
                    ..Default::default()
                }
            }))
            .collect();
            let worn_name = worn
                .and_then(|w| accessory(&w.id))
                .map_or_else(|| t!(lang, "house.editor.nothing"), |a| a.name.clone());
            let dye = worn
                .and_then(|w| w.tint)
                .and_then(|t| match t {
                    Paint::Swatch(i) => ACCESSORY_TINTS.get(usize::from(i)).map(|s| format!(" · {}", s.name)),
                    Paint::Custom(v) => Some(format!(" · #{v:06x}")),
                })
                .unwrap_or_default();
            (
                format!("{} · {worn_name}{dye}", draft.slot.label()),
                vec![
                    EditorSection {
                        title: t!(lang, "character.card.wearing"),
                        cols: 5,
                        cells: wearing,
                    },
                    EditorSection {
                        title: t!(lang, "character.card.try-on", slot = draft.slot.label()),
                        cols: 5,
                        cells: items,
                    },
                ],
            )
        }
        Tab::Presets => {
            let cells = presets()
                .iter()
                .map(|p| EditorCell {
                    art: p.character(content).map(|c| render_character(&c.spec(), 1)),
                    label: p.name.clone(),
                    current: p.code == draft.code,
                    ..Default::default()
                })
                .collect();
            (
                t!(lang, "character.card.presets"),
                vec![EditorSection {
                    title: t!(lang, "character.card.presets-hint"),
                    cols: 5,
                    cells,
                }],
            )
        }
        Tab::Outfits => {
            let cells = (1..=OUTFIT_SLOTS)
                .map(|slot| {
                    let saved = outfits.iter().find(|o| o.slot == slot);
                    let art = saved
                        .and_then(|o| Character::decode(content, &o.code))
                        .map(|c| render_character(&c.spec(), 1));
                    EditorCell {
                        art,
                        label: saved.map_or_else(|| t!(lang, "character.card.empty"), |o| o.name.clone()),
                        note: slot.to_string(),
                        current: saved.is_some_and(|o| o.code == draft.code),
                        ..Default::default()
                    }
                })
                .collect();
            (
                t!(
                    lang,
                    "character.card.outfits",
                    kept = outfits.len(),
                    slots = OUTFIT_SLOTS
                ),
                vec![EditorSection {
                    title: t!(lang, "character.card.outfits-hint"),
                    cols: 3,
                    cells,
                }],
            )
        }
    };
    CharacterCard {
        character: render_character(&character.spec(), 1),
        name,
        status: if draft.is_saved() {
            (t!(lang, "character.card.saved"), true)
        } else {
            (t!(lang, "character.card.draft"), false)
        },
        tabs: Tab::ALL.iter().map(|t| t.name(lang)).collect(),
        tab: tab_index,
        caption,
        sections,
        hint: t!(lang, "character.card.hint"),
        theme: Theme::reef(),
    }
}

/// A price short enough for a tile's corner: `900`, `1.2K`.
fn compact_price(lang: Lang, p: i64) -> String {
    if p >= 1000 {
        let k = p as f32 / 1000.0;
        if (k - k.round()).abs() < 0.05 {
            format!("{}K", k.round())
        } else {
            format!("{}K", pinchora_core::admin::server_stats::decimal_in(lang, k, 1))
        }
    } else {
        p.to_string()
    }
}

#[cfg(test)]
mod tests {
    use crate::commands::social::character::view::*;

    #[test]
    fn prices_fit_a_corner() {
        assert_eq!(compact_price(Lang::En, 900), "900");
        assert_eq!(compact_price(Lang::En, 1200), "1.2K");
        assert_eq!(compact_price(Lang::En, 2000), "2K");
        assert_eq!(compact_price(Lang::Tr, 1200), "1,2K");
    }

    #[test]
    fn every_tab_has_valid_discord_controls() {
        let content = Content::get();
        let shelf = Shelf {
            bought: Vec::new(),
            level: 1,
            badges: Vec::new(),
        };
        let mut character = Character::starter(content);
        character.colors.hair = Paint::Custom(0x12ab3f);
        character.wear(Slot::Head, Some("top_hat"), None);
        // The same look kept in two slots.
        let outfits: Vec<OutfitRow> = [1, 2]
            .map(|slot| OutfitRow {
                slot,
                name: format!("Outfit {slot}"),
                code: character.encode(),
            })
            .into();
        for tab in Tab::ALL {
            for slot in Slot::ALL {
                let draft = Draft {
                    session: u32::MAX,
                    code: character.encode(),
                    saved: String::new(),
                    undo: vec!["x".to_owned()],
                    tab,
                    slot,
                };
                let rows = controls(&draft, &character, &shelf, &outfits, Lang::Tr);
                assert!(rows.len() <= 5, "{tab:?}: {} rows", rows.len());
                for r in &rows {
                    let Component::ActionRow(r) = r else {
                        panic!("rows only")
                    };
                    assert!(!r.components.is_empty() && r.components.len() <= 5, "{tab:?}");
                    for c in &r.components {
                        match c {
                            Component::Button(b) => {
                                let id = b.custom_id.as_deref().expect("an id");
                                assert!(id.len() <= 100 && id.starts_with("ca:"), "{id}");
                                assert!(b.label.as_deref().is_some_and(|l| l.chars().count() <= 80));
                            }
                            Component::SelectMenu(m) => {
                                assert!(r.components.len() == 1, "a select takes a whole row");
                                let options = m.options.as_deref().unwrap_or_default();
                                assert!(!options.is_empty() && options.len() <= 25, "{tab:?}: {}", options.len());
                                assert!(options.iter().filter(|o| o.default).count() <= 1, "{tab:?}");
                                assert!(m.custom_id.len() <= 100);
                            }
                            other => panic!("unexpected {other:?}"),
                        }
                    }
                }
            }
        }
    }
}
