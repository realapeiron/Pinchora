use super::*;

/// The card: the draft on its tile and the open tab's choices.
pub(super) fn card(
    draft: &Draft,
    character: &Character,
    name: String,
    accesses: &[(String, Access)],
    outfits: &[OutfitRow],
    lang: Lang,
) -> CharacterCard {
    let tab_index = Tab::ALL.iter().position(|t| *t == draft.tab).unwrap_or(0);
    let (caption, sections) = match draft.tab {
        Tab::Look => look_sections(character, lang),
        Tab::Paint(channel) => paint_sections(character, lang, channel),
        Tab::Gear => gear_sections(draft, character, accesses, lang),
        Tab::Presets => presets_sections(draft, lang),
        Tab::Outfits => outfits_sections(draft, outfits, lang),
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

/// `0xrrggbb` of a colour.
fn hex([r, g, b]: [u8; 3]) -> u32 {
    (u32::from(r) << 16) | (u32::from(g) << 8) | u32::from(b)
}

/// A price short enough for a tile's corner: `900`, `1.2K`.
pub(super) fn compact_price(lang: Lang, p: i64) -> String {
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

fn look_sections(character: &Character, lang: Lang) -> (String, Vec<EditorSection>) {
    let content = Content::get();

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

fn paint_sections(character: &Character, lang: Lang, channel: Channel) -> (String, Vec<EditorSection>) {
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

fn gear_sections(
    draft: &Draft,
    character: &Character,
    accesses: &[(String, Access)],
    lang: Lang,
) -> (String, Vec<EditorSection>) {
    let access_of = |id: &str| {
        accesses
            .iter()
            .find(|(a, _)| a == id)
            .map_or(Access::Owned, |(_, x)| x.clone())
    };

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

fn presets_sections(draft: &Draft, lang: Lang) -> (String, Vec<EditorSection>) {
    let content = Content::get();

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

fn outfits_sections(draft: &Draft, outfits: &[OutfitRow], lang: Lang) -> (String, Vec<EditorSection>) {
    let content = Content::get();

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
