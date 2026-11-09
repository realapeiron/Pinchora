//! The editor message: the preview card with the open tab's choices, and its menus and buttons.
mod preview;
use preview::card;
#[cfg(test)]
use preview::compact_price;

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
    let tabs = Tab::ALL
        .iter()
        .map(|t| option(&t.name(lang), t.key(), None, *t == draft.tab))
        .collect();
    let jump = selection_options(draft, character, shelf, outfits, lang);
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
    let step_row = navigation_buttons(draft, character, lang);
    let worn = character.worn(draft.slot);
    let mut rows = vec![
        row(vec![menu(format!("ca:tab:{s}"), &button_label(lang, "category"), tabs)]),
        row(vec![menu(format!("ca:pick:{s}"), &placeholder, jump)]),
        row(step_row),
    ];
    category_buttons(draft, outfits, lang, &mut rows);
    let mut last = vec![button(
        format!("ca:save:{s}"),
        &button_label(lang, "save"),
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
        &button_label(lang, "cancel"),
        ButtonStyle::Danger,
        false,
    ));
    rows.push(row(last));
    rows
}

fn selection_options(
    draft: &Draft,
    character: &Character,
    shelf: &Shelf,
    outfits: &[OutfitRow],
    lang: Lang,
) -> Vec<SelectMenuOption> {
    let content = Content::get();
    match draft.tab {
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
    }
}

fn navigation_buttons(draft: &Draft, character: &Character, lang: Lang) -> Vec<Component> {
    let s = draft.session;
    let mut step_row = vec![
        button(format!("ca:prev:{s}"), "◀", ButtonStyle::Secondary, false),
        button(format!("ca:next:{s}"), "▶", ButtonStyle::Secondary, false),
        button(
            format!("ca:undo:{s}"),
            &button_label(lang, "undo"),
            ButtonStyle::Secondary,
            draft.undo.is_empty(),
        ),
        button(
            format!("ca:rand:{s}"),
            &button_label(lang, "randomise"),
            ButtonStyle::Secondary,
            false,
        ),
    ];
    let worn = character.worn(draft.slot);
    match draft.tab {
        Tab::Paint(_) => step_row.push(button(
            format!("ca:hex:{s}"),
            &button_label(lang, "hex"),
            ButtonStyle::Secondary,
            false,
        )),
        Tab::Gear => {
            let dyeable = worn.and_then(|w| accessory(&w.id)).is_some_and(|a| a.tintable);
            step_row.push(button(
                format!("ca:tint:{s}"),
                &button_label(lang, "dye"),
                ButtonStyle::Secondary,
                !dyeable,
            ));
        }
        _ => {}
    }
    step_row
}

fn category_buttons(draft: &Draft, outfits: &[OutfitRow], lang: Lang, rows: &mut Vec<Component>) {
    let s = draft.session;
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
                &button_label(lang, "rename"),
                ButtonStyle::Secondary,
                outfits.is_empty(),
            ));
            rows.push(row(keep));
        }
        _ => {}
    }
}

fn button_label(lang: Lang, key: &str) -> String {
    t!(lang, &format!("character.button.{key}"))
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
