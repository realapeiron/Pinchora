use super::*;

fn shown_letters(state: &State) -> String {
    letters_of(state)
        .iter()
        .map(char::to_string)
        .collect::<Vec<_>>()
        .join(" · ")
}

/// The file the round's picture goes up as.
pub(super) fn write_file(party: Party) -> &'static str {
    if party == Party::Postcard {
        "postcard.png"
    } else {
        "letters.png"
    }
}

/// `embed` with the attached picture `file` as its image.
pub(super) fn pictured(embed: EmbedBuilder, file: &str) -> EmbedBuilder {
    match ImageSource::attachment(file) {
        Ok(source) => embed.image(source),
        Err(_) => embed,
    }
}

/// A message that changes only its embed, so the picture and the buttons stay as they are.
pub(super) fn embed_only(embed: EmbedBuilder) -> InteractionResponseData {
    InteractionResponseData {
        embeds: Some(vec![embed.build()]),
        allowed_mentions: quiet(),
        ..Default::default()
    }
}

/// The words under the round's picture: what to write, when it closes, and how many have.
pub(super) fn write_embed(
    party: Party,
    game: &PartyGame,
    players: &[PartyPlayer],
    state: &State,
    written: usize,
) -> EmbedBuilder {
    let lang = state.lang;
    let ask = if party == Party::Postcard {
        t!(lang, "postcard.ask")
    } else {
        t!(lang, "acronym.ask", letters = shown_letters(state))
    };
    EmbedBuilder::new()
        .color(BRAND)
        .title(format!(
            "{} {} · {}",
            party.emoji(),
            party.label_in(lang),
            round_label(lang, game.round, game.rounds)
        ))
        .description(t!(
            lang,
            &format!("{}.write-desc", party.key()),
            ask = ask,
            when = relative(game.deadline),
            written = written,
            players = players.len()
        ))
}

/// The round's prompt: the letters or the postcard, who is in, and the button to write. The picture
/// does not say how many have written (the embed does), so a new line only changes the embed.
pub(super) async fn write_view(
    app: &App,
    party: Party,
    game: &PartyGame,
    players: &[PartyPlayer],
    state: &State,
    written: usize,
) -> anyhow::Result<InteractionResponseData> {
    let lang = state.lang;
    // The cards draw their title, place, stamp and footer in capitals, which Turkish spells with its own İ.
    let round = lang.upper(&round_label(lang, game.round, game.rounds));
    let png = if party == Party::Postcard {
        let scene = scenes::scene(&state.scene).ok_or_else(|| anyhow::anyhow!("unknown scene {}", state.scene))?;
        let picture = postcard::scene_png(app, scene)?;
        let place = lang.upper(&t!(lang, &format!("postcard.place.{}", scene.id)));
        app.render(move |r| {
            r.postcard_card(&PostcardCard {
                lang,
                scene: &picture,
                place: &place,
                stamp: &round,
                theme: theme(),
            })
        })
        .await?
    } else {
        let letters = letters_of(state);
        let title = lang.upper(&party.label_in(lang));
        let hint = t!(lang, "acronym.card-hint");
        let status = lang.upper(&t!(lang, "acronym.card-status"));
        app.render(move |r| {
            r.letters_card(&LettersCard {
                title: &title,
                round: &round,
                letters: &letters,
                hint: &hint,
                status: &status,
                theme: theme(),
            })
        })
        .await?
    };
    let file = write_file(party);
    let button = push_button(
        party,
        &[&"w", &game.id, &game.round],
        &t!(lang, &format!("{}.btn-write", party.key())),
        ButtonStyle::Primary,
    );
    Ok(card(
        None,
        write_embed(party, game, players, state, written),
        file,
        png,
        vec![row(vec![button])],
    ))
}

/// The words over the ballot: what to vote for, when it closes, and how many have.
pub(super) fn vote_embed(party: Party, game: &PartyGame, lang: Lang, players: usize, voted: usize) -> EmbedBuilder {
    EmbedBuilder::new()
        .color(BRAND)
        .title(t!(
            lang,
            "party.vote-title",
            emoji = party.emoji(),
            label = party.label_in(lang)
        ))
        .description(t!(
            lang,
            &format!("{}.vote-desc", party.key()),
            when = relative(game.deadline),
            voted = voted,
            players = players
        ))
}

/// The entries of the round up for a vote, numbered, with the voting buttons.
pub(super) async fn vote_view(
    app: &App,
    party: Party,
    game: &PartyGame,
    players: &[PartyPlayer],
    state: &State,
    entries: &[PartyEntry],
    voted: usize,
) -> anyhow::Result<InteractionResponseData> {
    let lang = state.lang;
    let picture = if party == Party::Postcard {
        Some(postcard::scene_png(
            app,
            scenes::scene(&state.scene).ok_or_else(|| anyhow::anyhow!("unknown scene {}", state.scene))?,
        )?)
    } else {
        None
    };
    let rows: Vec<(String, Option<String>, Option<u32>, bool)> =
        entries.iter().map(|e| (e.text.clone(), None, None, false)).collect();
    let subtitle = if party == Party::Postcard {
        t!(lang, "postcard.vote-sub")
    } else {
        t!(lang, "acronym.vote-sub", letters = shown_letters(state))
    };
    let png = entries_png(
        app,
        party,
        game,
        &subtitle,
        picture,
        rows,
        &t!(lang, "party.no-own-vote"),
    )
    .await?;
    let buttons: Vec<Component> = (0..entries.len())
        .map(|i| {
            push_button(
                party,
                &[&"v", &game.id, &game.round, &i],
                &(i + 1).to_string(),
                ButtonStyle::Secondary,
            )
        })
        .collect();
    let rows: Vec<Component> = buttons.chunks(5).map(|chunk| row(chunk.to_vec())).collect();
    Ok(card(
        None,
        vote_embed(party, game, lang, players.len(), voted),
        "entries.png",
        png,
        rows,
    ))
}

/// The list of entries as a picture, anonymous or with names and votes.
async fn entries_png(
    app: &App,
    party: Party,
    game: &PartyGame,
    subtitle: &str,
    picture: Option<Arc<Vec<u8>>>,
    rows: Vec<(String, Option<String>, Option<u32>, bool)>,
    footer: &str,
) -> anyhow::Result<Vec<u8>> {
    let lang = game_lang(game);
    // The card draws its title, chip and footer in capitals, which Turkish spells with its own İ.
    let (title, subtitle, footer) = (
        lang.upper(&party.label_in(lang)),
        subtitle.to_owned(),
        lang.upper(footer),
    );
    let chip = lang.upper(&round_label(lang, game.round, game.rounds));
    app.render(move |r| {
        let rows: Vec<EntryRow> = rows
            .iter()
            .map(|(text, author, votes, winner)| EntryRow {
                text,
                author: author.as_deref(),
                votes: *votes,
                winner: *winner,
            })
            .collect();
        r.entries_card(&pinchora_render::EntriesCard {
            lang,
            title: &title,
            subtitle: &subtitle,
            chip: &chip,
            header: picture.as_ref().map(|png| EntryHeader { png, w: 960, h: 540 }),
            rows: &rows,
            footer: &footer,
            theme: theme(),
        })
    })
    .await
}

/// The form's one field: `ui` is the language of the player who opens it, the table's own (`state.lang`)
/// words the example in it.
#[allow(deprecated)] // the label belongs on the Label component, so the input's own stays empty
pub(super) fn field(ui: Lang, party: Party, state: &State) -> Component {
    let (label, max) = if party == Party::Postcard {
        (t!(ui, "postcard.field-label"), scenes::MAX_CAPTION)
    } else {
        (
            t!(
                ui,
                "acronym.field-label",
                letters = letters_of(state)
                    .iter()
                    .map(char::to_string)
                    .collect::<Vec<_>>()
                    .join(" ")
            ),
            acronym::MAX_PHRASE,
        )
    };
    let placeholder = t!(state.lang, &format!("{}.placeholder", party.key()));
    Component::Label(Label {
        component: Box::new(Component::TextInput(TextInput {
            custom_id: ENTRY_FIELD.to_owned(),
            id: None,
            label: None,
            max_length: Some(max as u16),
            min_length: Some(3),
            placeholder: Some(placeholder),
            required: Some(true),
            style: TextInputStyle::Short,
            value: None,
        })),
        description: None,
        id: None,
        label: label.chars().take(45).collect(),
    })
}

/// The results of a round: every entry with its author and votes, the winner marked.
pub(super) async fn reveal_view(
    app: &App,
    party: Party,
    game: &PartyGame,
    players: &[PartyPlayer],
    state: &State,
    entries: &[PartyEntry],
    tally: &[(u64, u32)],
) -> anyhow::Result<InteractionResponseData> {
    let lang = state.lang;
    let winners = round_winners(tally);
    let mut ranked: Vec<&PartyEntry> = entries.iter().collect();
    let votes_of = |user: u64| tally.iter().find(|(who, _)| *who == user).map(|(_, n)| *n);
    ranked.sort_by_key(|e| std::cmp::Reverse(votes_of(e.author()).unwrap_or(0)));
    let rows: Vec<(String, Option<String>, Option<u32>, bool)> = ranked
        .iter()
        .map(|e| {
            (
                e.text.clone(),
                Some(name_of(lang, players, e.author())),
                votes_of(e.author()),
                winners.contains(&e.author()),
            )
        })
        .collect();
    let picture = if party == Party::Postcard {
        scenes::scene(&state.scene)
            .map(|scene| postcard::scene_png(app, scene))
            .transpose()?
    } else {
        None
    };
    let subtitle = if party == Party::Postcard {
        t!(lang, "postcard.reveal-sub")
    } else {
        t!(lang, "acronym.reveal-sub", letters = shown_letters(state))
    };
    let png = entries_png(
        app,
        party,
        game,
        &subtitle,
        picture,
        rows,
        &t!(lang, "party.score-rule", points = VOTE_POINTS, bonus = WIN_BONUS),
    )
    .await?;
    let last = game.round >= game.rounds;
    let embed = EmbedBuilder::new()
        .color(WARN)
        .title(t!(
            lang,
            "party.results-title",
            emoji = party.emoji(),
            label = party.label_in(lang),
            round = game.round
        ))
        .description(t!(
            lang,
            if last {
                "party.reveal-final"
            } else {
                "party.reveal-next"
            },
            note = state.tally.say(lang, party),
            when = relative(game.deadline)
        ));
    Ok(card(None, embed, "results.png", png, Vec::new()))
}
