use super::*;

/// The message of an open critter drop: its embed, the picture of the three cards, and the buttons.
pub(super) async fn critter_message(app: &App, lang: Lang, row: &DropRow) -> anyhow::Result<(Embed, Vec<u8>, Vec<Component>)> {
    let card = DropCard {
        title: Game::CritterDrop.name(lang),
        subtitle: if row.mode == "word" {
            row.challenge_word.clone().unwrap_or_default()
        } else {
            t!(lang, "drop.card.subtitle")
        },
        cards: cards(row)
            .into_iter()
            .enumerate()
            .map(|(i, e)| DropCritter {
                art: if row.mode == "hidden" {
                    pinchora_render::EntryPicture {
                        png: pinchora_render::encode_png(&image::RgbaImage::new(1, 1)).unwrap_or_default(),
                        w: 1,
                        h: 1,
                    }
                } else if row.cards[i].starts_with("g:") {
                    crate::commands::games::collection::glimmer_picture(app, e, DropCard::ART_BOX)
                } else {
                    picture(app, e, DropCard::ART_BOX)
                },
                name: if row.mode == "hidden" {
                    "???".to_owned()
                } else if row.cards[i].starts_with("g:") {
                    format!("✦ {}", e.name_in(lang))
                } else {
                    e.name_in(lang)
                },
                rarity: if row.mode == "hidden" {
                    "?".to_owned()
                } else {
                    e.rarity.label_in(lang).to_owned()
                },
                frame: if row.mode == "hidden" {
                    0x71829c
                } else if row.cards[i].starts_with("g:") {
                    0x9af5ec
                } else {
                    e.rarity.color()
                },
                habitat: if row.mode == "hidden" {
                    "???".to_owned()
                } else {
                    e.place_in(lang).to_owned()
                },
                sparkles: if row.mode == "hidden" {
                    0
                } else if row.cards[i].starts_with("g:") {
                    4
                } else {
                    sparkles(e.rarity)
                },
            })
            .collect(),
        footer: t!(lang, "drop.card.footer", secs = DROP_SECS),
        theme: Theme::reef(),
    };
    let png = app.render(move |r| r.drop_card(&card)).await?;
    Ok((critter_embed(row, false, lang), png, critter_buttons(row, false)))
}

/// The text of a drop: what each card is and who has it. `over`: its time is up.
pub(super) fn critter_embed(row: &DropRow, over: bool, lang: Lang) -> Embed {
    let lines: Vec<String> = cards(row)
        .iter()
        .zip(&row.takers)
        .enumerate()
        .map(|(i, (entry, taker))| {
            let who = match (*taker, over) {
                (0, false) if row.mode == "vote" && row.vote_winner.is_some_and(|winner| winner != i as i16) => {
                    t!(lang, "collection-depth.vote-lost")
                }
                (0, false) => t!(lang, "drop.free"),
                (0, true) => t!(lang, "drop.drifted"),
                (taker, _) if taker < 0 => t!(lang, "collection-depth.anonymous-claim"),
                (taker, _) => t!(lang, "drop.taken-by", user = mention(taker)),
            };
            t!(
                lang,
                "drop.line",
                number = i + 1,
                name = if row.mode == "hidden" && !over && *taker == 0 {
                    "???".to_owned()
                } else if row.cards[i].starts_with("g:") {
                    format!("✦ {}", entry.name_in(lang))
                } else {
                    entry.name_in(lang)
                },
                rarity = if row.mode == "hidden" && !over && *taker == 0 {
                    "?".to_owned()
                } else {
                    letters::lower(lang, entry.rarity.label_in(lang))
                },
                who = who
            )
        })
        .collect();
    let all_gone = if row.mode == "vote" {
        row.vote_winner
            .and_then(|slot| row.takers.get(slot as usize))
            .is_some_and(|t| *t != 0)
    } else {
        row.takers.iter().all(|t| *t != 0)
    };
    let (title, intro, color) = if over && all_gone {
        (t!(lang, "drop.title-over"), t!(lang, "drop.all-home-over"), SUCCESS)
    } else if over {
        (t!(lang, "drop.title-over"), t!(lang, "drop.tide-took"), WARN)
    } else if all_gone {
        (t!(lang, "drop.title-gone"), t!(lang, "drop.all-home"), SUCCESS)
    } else {
        (
            t!(lang, "drop.title-open"),
            t!(
                lang,
                "drop.intro",
                when = format!("<t:{}:R>", row.expires_at.timestamp())
            ),
            BRAND,
        )
    };
    let called = row.host_id.map_or(String::new(), |host| {
        format!("\n{}", t!(lang, "drop.called-by", user = mention(host)))
    });
    let instruction = if row.mode == "word" && !over {
        format!("\n\n{}", t!(lang, "collection-depth.type-word"))
    } else {
        String::new()
    };
    let instruction = if row.mode == "vote" && !over {
        format!(
            "\n\n{}",
            if let Some(winner) = row.vote_winner {
                t!(lang, "collection-depth.vote-winner", number = winner + 1)
            } else {
                t!(lang, "collection-depth.vote-open")
            }
        )
    } else {
        instruction
    };
    let mut embed = EmbedBuilder::new()
        .color(color)
        .title(title)
        .description(format!("{intro}{called}{instruction}\n\n{}", lines.join("\n")));
    if let Ok(source) = ImageSource::attachment("drop.png") {
        embed = embed.image(source);
    }
    embed.build()
}

pub(super) fn critter_buttons(row: &DropRow, over: bool) -> Vec<Component> {
    let buttons = row
        .takers
        .iter()
        .enumerate()
        .map(|(slot, taker)| {
            let free = *taker == 0
                && !over
                && (row.mode != "vote" || row.vote_winner.is_none() || row.vote_winner == Some(slot as i16));
            button(
                format!(
                    "cd:{}:{}:{slot}",
                    if row.mode == "word" {
                        "w"
                    } else if row.mode == "vote" && row.vote_winner.is_none() {
                        "v"
                    } else {
                        "g"
                    },
                    row.id
                ),
                &(slot + 1).to_string(),
                None,
                if free {
                    ButtonStyle::Primary
                } else {
                    ButtonStyle::Secondary
                },
                !free,
            )
        })
        .collect();
    vec![button_row(buttons)]
}

