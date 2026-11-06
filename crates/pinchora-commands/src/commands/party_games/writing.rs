//! Acronym Tide and Caption the Postcard: both are written games. Everyone types one line for what the
//! round shows (a phrase for the letters, a caption for the postcard), the lines go up anonymously and the
//! table votes, and then the names and the votes are shown. After the last round the scores decide.
//! A round is `write`, `vote`, `reveal`; each wait has a deadline, and the first player to finish all
//! their steps early (everyone has written, everyone has voted) moves it on sooner.

use std::{str::Split, sync::Arc, time::Duration};

use pinchora_core::i18n::Lang;
use pinchora_db::{Commit, PartyEntry, PartyGame, PartyPlayer, PartyVote, Pay, PlayerChange};
use pinchora_game::party::{
    Dice, Party, VOTE_POINTS, WIN_BONUS, acronym, postcard as scenes, round_points, round_winners, writing_reward,
};
use pinchora_render::{EntryHeader, EntryRow, LettersCard, PostcardCard};
use serde::{Deserialize, Serialize};
use twilight_model::{
    channel::message::{
        Component,
        component::{ButtonStyle, Label, TextInput, TextInputStyle},
    },
    http::interaction::InteractionResponseData,
};
use twilight_util::builder::embed::{EmbedBuilder, ImageSource};

use super::{
    after, card, count_games, day, edit_card, lang_of, later, lobby, name_of, paid_note, postcard, push_button,
    remember, row, snapshot, standings_png, strict, text, theme,
};
use crate::{
    App, Cx,
    commands::games::quiet,
    util::{BRAND, SUCCESS, WARN, relative},
};

/// The shortest wait between two edits that only change how many have written or voted.
const COUNT_EVERY: Duration = Duration::from_secs(3);

/// Custom id of the text input of the form.
pub(crate) const ENTRY_FIELD: &str = "entry";

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct State {
    /// The language of the game: Acronym Tide draws its letters from it.
    #[serde(default)]
    lang: Lang,
    seed: u64,
    /// Acronym Tide: the letters of this round.
    letters: String,
    /// Caption the Postcard: this round's scene, and the ones already shown.
    scene: String,
    seen: Vec<String>,
    /// What the last tally said, for the reveal.
    note: String,
}

fn decode(game: &PartyGame) -> anyhow::Result<State> {
    Ok(serde_json::from_str(&game.state)?)
}

fn encode(state: &State) -> anyhow::Result<String> {
    Ok(serde_json::to_string(state)?)
}

fn party_of(game: &PartyGame) -> anyhow::Result<Party> {
    match Party::from_key(&game.kind) {
        Some(party @ (Party::Acronym | Party::Postcard)) => Ok(party),
        _ => anyhow::bail!("not a written game: {}", game.kind),
    }
}

fn write_secs(party: Party) -> i64 {
    match party {
        Party::Postcard => scenes::WRITE_SECS,
        _ => acronym::WRITE_SECS,
    }
}

fn vote_secs(party: Party) -> i64 {
    match party {
        Party::Postcard => scenes::VOTE_SECS,
        _ => acronym::VOTE_SECS,
    }
}

fn reveal_secs(party: Party) -> i64 {
    match party {
        Party::Postcard => scenes::REVEAL_SECS,
        _ => acronym::REVEAL_SECS,
    }
}

/// What a line is called in this game.
fn noun(party: Party) -> &'static str {
    if party == Party::Postcard { "caption" } else { "phrase" }
}

pub(crate) async fn open(cx: &Cx, party: Party, rounds: i32) -> anyhow::Result<()> {
    let range = if party == Party::Postcard {
        scenes::ROUNDS
    } else {
        acronym::ROUNDS
    };
    let rounds = rounds.clamp(*range.start(), *range.end());
    let state = State {
        lang: lang_of(cx),
        ..Default::default()
    };
    lobby::open(cx, party, rounds, encode(&state)?).await
}

/// Sets up what round `round` (from 1) shows: its letters, or its postcard.
fn begin_round(party: Party, state: &mut State, round: i32) {
    let mut dice = Dice::new(state.seed ^ (round as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15));
    if party == Party::Postcard {
        let seen: Vec<&str> = state.seen.iter().map(String::as_str).collect();
        let scene = scenes::pick_scene(&mut dice, &seen).id;
        state.scene = scene.to_owned();
        state.seen.push(scene.to_owned());
    } else {
        let letters = acronym::draw_letters(state.lang, &mut dice, acronym::letter_count(round - 1));
        state.letters = letters.into_iter().collect();
    }
    state.note.clear();
}

pub(crate) async fn start(cx: &Cx, game: &PartyGame, players: &[PartyPlayer]) -> anyhow::Result<lobby::Started> {
    let app = &cx.app;
    let party = party_of(game)?;
    let mut state = decode(game)?;
    state.seed = rand::random();
    begin_round(party, &mut state, 1);
    let commit = Commit {
        phase: "write",
        round: 1,
        state: &encode(&state)?,
        deadline: after(app, write_secs(party)),
        status: "playing",
        players: &[],
        entries: &[],
        pay: None,
    };
    let Some(done) = app.db.commit_party(game.id, game.seq, commit).await? else {
        return Ok(lobby::Started::Stale);
    };
    cx.update(write_view(app, party, &done.game, players, &state, 0).await?)
        .await?;
    Ok(lobby::Started::Yes)
}

fn letters_of(state: &State) -> Vec<char> {
    state.letters.chars().collect()
}

fn shown_letters(state: &State) -> String {
    letters_of(state)
        .iter()
        .map(char::to_string)
        .collect::<Vec<_>>()
        .join(" · ")
}

/// The file the round's picture goes up as.
fn write_file(party: Party) -> &'static str {
    if party == Party::Postcard {
        "postcard.png"
    } else {
        "letters.png"
    }
}

/// `embed` with the attached picture `file` as its image.
fn pictured(embed: EmbedBuilder, file: &str) -> EmbedBuilder {
    match ImageSource::attachment(file) {
        Ok(source) => embed.image(source),
        Err(_) => embed,
    }
}

/// A message that changes only its embed, so the picture and the buttons stay as they are.
fn embed_only(embed: EmbedBuilder) -> InteractionResponseData {
    InteractionResponseData {
        embeds: Some(vec![embed.build()]),
        allowed_mentions: quiet(),
        ..Default::default()
    }
}

/// The words under the round's picture: what to write, when it closes, and how many have.
fn write_embed(party: Party, game: &PartyGame, players: &[PartyPlayer], state: &State, written: usize) -> EmbedBuilder {
    let round_label = format!("Round {}/{}", game.round, game.rounds);
    let status = format!("{written} of {} {}s in", players.len(), noun(party));
    let ask = if party == Party::Postcard {
        "Write a caption for this postcard".to_owned()
    } else {
        format!("Build a phrase from **{}**", shown_letters(state))
    };
    EmbedBuilder::new()
        .color(BRAND)
        .title(format!("{} {} · {round_label}", party.emoji(), party.label()))
        .description(format!(
            "{ask}. Everyone writes one, and nobody sees who wrote what until the vote is over.\n{}s close {} · {status}",
            noun(party),
            relative(game.deadline)
        ))
}

/// The round's prompt: the letters or the postcard, who is in, and the button to write. The picture
/// does not say how many have written (the embed does), so a new line only changes the embed.
async fn write_view(
    app: &App,
    party: Party,
    game: &PartyGame,
    players: &[PartyPlayer],
    state: &State,
    written: usize,
) -> anyhow::Result<InteractionResponseData> {
    let round_label = format!("Round {}/{}", game.round, game.rounds);
    let png = if party == Party::Postcard {
        let scene = scenes::scene(&state.scene).ok_or_else(|| anyhow::anyhow!("unknown scene {}", state.scene))?;
        let picture = postcard::scene_png(app, scene)?;
        let (place, stamp) = (scene.place.to_owned(), round_label);
        app.render(move |r| {
            r.postcard_card(&PostcardCard {
                scene: &picture,
                place: &place,
                stamp: &stamp,
                theme: theme(),
            })
        })
        .await?
    } else {
        let letters = letters_of(state);
        let title = party.label().to_owned();
        let hint = "One word for each letter, in order. Press the button and type your phrase.".to_owned();
        let status = "Phrases stay hidden until the vote is over".to_owned();
        app.render(move |r| {
            r.letters_card(&LettersCard {
                title: &title,
                round: &round_label,
                letters: &letters,
                hint: &hint,
                status: &status,
                theme: theme(),
            })
        })
        .await?
    };
    let file = write_file(party);
    let label = if party == Party::Postcard {
        "Write a caption"
    } else {
        "Write a phrase"
    };
    let button = push_button(party, &[&"w", &game.id, &game.round], label, ButtonStyle::Primary);
    Ok(card(
        None,
        write_embed(party, game, players, state, written),
        file,
        png,
        vec![row(vec![button])],
    ))
}

/// The words over the ballot: what to vote for, when it closes, and how many have.
fn vote_embed(party: Party, game: &PartyGame, players: usize, voted: usize) -> EmbedBuilder {
    EmbedBuilder::new()
        .color(BRAND)
        .title(format!("{} {} · vote", party.emoji(), party.label()))
        .description(format!(
            "Vote for the {} you like best, {}. {voted} of {players} voted.",
            noun(party),
            relative(game.deadline)
        ))
}

/// The entries of the round up for a vote, numbered, with the voting buttons.
async fn vote_view(
    app: &App,
    party: Party,
    game: &PartyGame,
    players: &[PartyPlayer],
    state: &State,
    entries: &[PartyEntry],
    voted: usize,
) -> anyhow::Result<InteractionResponseData> {
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
        "Which caption is the funniest?".to_owned()
    } else {
        format!("Which phrase is best for {}?", shown_letters(state))
    };
    let png = entries_png(
        app,
        party,
        game,
        &subtitle,
        picture,
        rows,
        "You can not vote for your own",
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
        vote_embed(party, game, players.len(), voted),
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
    let (title, subtitle, footer) = (party.label().to_owned(), subtitle.to_owned(), footer.to_owned());
    let chip = format!("Round {}/{}", game.round, game.rounds);
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

#[allow(deprecated)] // the label belongs on the Label component, so the input's own stays empty
fn field(party: Party, state: &State) -> Component {
    let (label, max, placeholder) = if party == Party::Postcard {
        ("Your caption".to_owned(), scenes::MAX_CAPTION, "What is going on here?")
    } else {
        (
            format!(
                "Your phrase for {}",
                letters_of(state)
                    .iter()
                    .map(char::to_string)
                    .collect::<Vec<_>>()
                    .join(" ")
            ),
            acronym::MAX_PHRASE,
            "Tiny whales ride sleds",
        )
    };
    Component::Label(Label {
        component: Box::new(Component::TextInput(TextInput {
            custom_id: ENTRY_FIELD.to_owned(),
            id: None,
            label: None,
            max_length: Some(max as u16),
            min_length: Some(3),
            placeholder: Some(placeholder.to_owned()),
            required: Some(true),
            style: TextInputStyle::Short,
            value: None,
        })),
        description: None,
        id: None,
        label: label.chars().take(45).collect(),
    })
}

/// `ac:` or `cn:` and the action: the lobby's buttons, `w:<id>:<round>` to write and `v:<id>:<round>:<n>`
/// to vote.
pub(crate) async fn on_button(cx: &Cx, party: Party, mut parts: Split<'_, char>) -> anyhow::Result<()> {
    let action = parts.next().unwrap_or_default();
    let mut number = |_: ()| parts.next().and_then(|v| v.parse::<i64>().ok());
    let Some(id) = number(()) else {
        return cx.reply_ephemeral("This button has expired.").await;
    };
    if lobby::on_button(cx, party, action, id).await? {
        return Ok(());
    }
    let (Some(round), me) = (number(()), cx.author()?.id.get()) else {
        return cx.reply_ephemeral("This button has expired.").await;
    };
    let Some((game, players)) = snapshot(&cx.app, id).await? else {
        return cx.reply_ephemeral("This game has expired.").await;
    };
    let round = round as i32;
    remember(cx, &game).await;
    let phase = if action == "w" { "write" } else { "vote" };
    if !game.is_playing() || game.round != round || game.phase != phase {
        return cx
            .reply_ephemeral("That step is over. Use the buttons on the newest message!")
            .await;
    }
    if game.late(cx.app.now()) {
        if action == "w" {
            // The button that opens a form has to be answered in time: the step is taken after the answer.
            later(&cx.app, id, move |app| async move { on_due(&app, game).await });
        } else {
            on_due(&cx.app, game).await?;
        }
        return cx.reply_ephemeral("Time is up for this step!").await;
    }
    if !players.iter().any(|p| p.id() == me) {
        return cx
            .reply_ephemeral("You are not in this game: only players write and vote.")
            .await;
    }
    match action {
        "w" => {
            let state = decode(&game)?;
            cx.modal(
                format!("{}:m:{id}:{round}", party.prefix()),
                if party == Party::Postcard {
                    "Caption the postcard"
                } else {
                    "Acronym Tide"
                },
                vec![field(party, &state)],
            )
            .await
        }
        "v" => {
            let Some(index) = number(()).map(|n| n as usize) else {
                return cx.reply_ephemeral("This button has expired.").await;
            };
            vote(cx, party, &game, &players, index).await
        }
        _ => cx.reply_ephemeral("This button has expired.").await,
    }
}

async fn vote(cx: &Cx, party: Party, game: &PartyGame, players: &[PartyPlayer], index: usize) -> anyhow::Result<()> {
    let app = &cx.app;
    let me = cx.author()?.id.get();
    let entries = app.db.party_entries(game.id, game.round).await?;
    let Some(entry) = entries.get(index) else {
        return cx.reply_ephemeral("There is no such entry.").await;
    };
    let number = index + 1;
    let (said, new_vote) = match app
        .db
        .put_vote(game.id, game.round, me, entry.author(), app.now())
        .await?
    {
        PartyVote::Counted => (
            format!("🗳️ Your vote for **#{number}** is in. You can change it until voting closes."),
            true,
        ),
        PartyVote::Changed => (format!("🗳️ Moved your vote to **#{number}**."), false),
        PartyVote::Same => (format!("You already voted for **#{number}**."), false),
        PartyVote::OwnEntry => ("That one is yours! Pick another.".to_owned(), false),
        PartyVote::NotPlayer => ("Only players can vote.".to_owned(), false),
        PartyVote::NoEntry => ("There is no such entry.".to_owned(), false),
        PartyVote::Closed => ("Voting is closed.".to_owned(), false),
    };
    cx.reply_ephemeral(said).await?;
    // Only a first vote changes the count. Everybody has voted: no reason to wait out the clock; otherwise
    // the count on the message follows, now or in a moment.
    if new_vote {
        let votes = app.db.party_votes(game.id, game.round).await?.len();
        if votes >= players.len() {
            to_reveal(app, game.id).await?;
        } else {
            count_changed(app, party, game).await;
        }
    }
    Ok(())
}

/// Puts the new count of lines in, or votes cast, on the game's message: at once if nothing was edited in
/// the last few seconds, otherwise once more when they are over, which then shows everything that came
/// in meanwhile. Only the embed is edited, so the picture is not drawn and sent again for a number.
async fn count_changed(app: &Arc<App>, party: Party, game: &PartyGame) {
    let id = game.id;
    let (phase, round) = (game.phase.clone(), game.round);
    // Each phase of each round has its own clock: the first vote is not held back by the last phrase.
    let key = format!("partycount:{id}:{phase}:{round}");
    let left = match app.cache.try_cooldown(&key, COUNT_EVERY).await {
        Ok(None) => return show_count(app, party, id, &phase, round).await,
        Ok(Some(left)) => left,
        Err(error) => {
            tracing::debug!(%error, game = id, "party count not throttled");
            return show_count(app, party, id, &phase, round).await;
        }
    };
    // One late edit is enough for everything that comes in until it goes out.
    let wait = left.max(Duration::from_millis(50));
    if !matches!(app.cache.try_cooldown(&format!("{key}:late"), wait).await, Ok(None)) {
        return;
    }
    let owned = app.clone();
    app.spawn(async move {
        tokio::time::sleep(wait).await;
        // The next edit waits its own turn from here.
        let _ = owned.cache.try_cooldown(&key, COUNT_EVERY).await;
        show_count(&owned, party, id, &phase, round).await;
    });
}

/// Edits the count into the game's message, if the game is still in `phase` of `round`.
async fn show_count(app: &Arc<App>, party: Party, id: i64, phase: &str, round: i32) {
    let result = async {
        let Some((game, players)) = snapshot(app, id).await? else {
            return anyhow::Ok(());
        };
        if !game.is_playing() || game.phase != phase || game.round != round {
            return Ok(());
        }
        let data = match phase {
            "write" => {
                let state = decode(&game)?;
                let written = app.db.party_entries(id, round).await?.len();
                pictured(write_embed(party, &game, &players, &state, written), write_file(party))
            }
            "vote" => {
                let voted = app.db.party_votes(id, round).await?.len();
                pictured(vote_embed(party, &game, players.len(), voted), "entries.png")
            }
            _ => return Ok(()),
        };
        edit_card(app, &game, &embed_only(data)).await;
        Ok(())
    }
    .await;
    if let Err(error) = result {
        tracing::debug!(%error, game = id, "party count not shown");
    }
}

/// The form was submitted: `ac:m:<id>:<round>` or `cn:m:<id>:<round>`.
pub(crate) async fn on_modal(
    cx: &Cx,
    party: Party,
    mut parts: Split<'_, char>,
    fields: &[(String, String)],
) -> anyhow::Result<()> {
    parts.next();
    let (Some(id), Some(round)) = (
        parts.next().and_then(|v| v.parse::<i64>().ok()),
        parts.next().and_then(|v| v.parse::<i32>().ok()),
    ) else {
        return cx.reply_ephemeral("This form has expired.").await;
    };
    let app = &cx.app;
    let me = cx.author()?.id.get();
    let Some((game, players)) = snapshot(app, id).await? else {
        return cx.reply_ephemeral("This game has expired.").await;
    };
    if !game.is_playing() || game.phase != "write" || game.round != round || game.late(app.now()) {
        if game.is_playing() && game.late(app.now()) {
            on_due(app, game).await?;
        }
        return cx
            .reply_ephemeral(format!("Time is up: this {} came too late.", noun(party)))
            .await;
    }
    let state = decode(&game)?;
    let typed = fields
        .iter()
        .find(|(field, _)| field == ENTRY_FIELD)
        .map_or("", |(_, value)| value.as_str());
    let cleaned = match text::clean_for(typed, strict(app, game.guild_id as u64).await, state.lang) {
        Ok(cleaned) => cleaned,
        Err(refused) => return cx.reply_ephemeral(refused.say()).await,
    };
    let line = if party == Party::Postcard {
        scenes::check_caption(&cleaned).map_err(|e| e.say())
    } else {
        acronym::check_phrase(state.lang, &cleaned, &letters_of(&state)).map_err(|e| e.say(&letters_of(&state)))
    };
    let line = match line {
        Ok(line) => line,
        Err(why) => return cx.reply_ephemeral(why).await,
    };
    let fresh = match app.db.put_entry(id, round, "write", me, &line, app.now()).await? {
        None => {
            return cx
                .reply_ephemeral(format!(
                    "Time is up, or you are not in this game: that {} did not count.",
                    noun(party)
                ))
                .await;
        }
        Some(fresh) => {
            cx.reply_ephemeral(format!(
                "✍️ {} in: “{line}”. Press the button again to rewrite it until time is up.",
                if fresh { "Locked" } else { "Rewritten and locked" }
            ))
            .await?;
            fresh
        }
    };
    // A rewrite changes no count. A first line may be the last one, which moves the game on at once;
    // otherwise the count on the message follows, now or in a moment.
    if fresh {
        let written = app.db.party_entries(id, round).await?.len();
        if written >= players.len() {
            to_vote(app, id).await?;
        } else {
            count_changed(app, party, &game).await;
        }
    }
    Ok(())
}

// /Phases

/// Writing is over: up for a vote, or, with fewer than two lines, straight to a reveal that says so.
async fn to_vote(app: &Arc<App>, id: i64) -> anyhow::Result<bool> {
    let Some((game, players)) = snapshot(app, id).await? else {
        return Ok(false);
    };
    if !game.is_playing() || game.phase != "write" {
        return Ok(false);
    }
    let party = party_of(&game)?;
    let mut state = decode(&game)?;
    let entries = app.db.party_entries(id, game.round).await?;
    if entries.len() < 2 {
        state.note = if entries.is_empty() {
            format!("Nobody wrote a {} this round.", noun(party))
        } else {
            format!("Only one {} came in, so there was nothing to vote on.", noun(party))
        };
        let commit = Commit {
            phase: "reveal",
            round: game.round,
            state: &encode(&state)?,
            deadline: after(app, reveal_secs(party)),
            status: "playing",
            players: &[],
            entries: &[],
            pay: None,
        };
        let Some(done) = app.db.commit_party(id, game.seq, commit).await? else {
            return Ok(false);
        };
        let view = reveal_view(app, party, &done.game, &players, &state, &entries, &[]).await?;
        edit_card(app, &done.game, &view).await;
        return Ok(true);
    }
    let commit = Commit {
        phase: "vote",
        round: game.round,
        state: &game.state,
        deadline: after(app, vote_secs(party)),
        status: "playing",
        players: &[],
        entries: &[],
        pay: None,
    };
    let Some(done) = app.db.commit_party(id, game.seq, commit).await? else {
        return Ok(false);
    };
    let view = vote_view(app, party, &done.game, &players, &state, &entries, 0).await?;
    edit_card(app, &done.game, &view).await;
    Ok(true)
}

/// Voting is over: score the round and show who wrote what.
async fn to_reveal(app: &Arc<App>, id: i64) -> anyhow::Result<bool> {
    let Some((game, players)) = snapshot(app, id).await? else {
        return Ok(false);
    };
    if !game.is_playing() || game.phase != "vote" {
        return Ok(false);
    }
    let party = party_of(&game)?;
    let mut state = decode(&game)?;
    let entries = app.db.party_entries(id, game.round).await?;
    let votes = app.db.party_votes(id, game.round).await?;
    let tally: Vec<(u64, u32)> = entries
        .iter()
        .map(|e| {
            (
                e.author(),
                votes.iter().filter(|(_, pick)| *pick == e.author()).count() as u32,
            )
        })
        .collect();
    let winners = round_winners(&tally);
    let changes: Vec<PlayerChange> = tally
        .iter()
        .map(|(author, count)| PlayerChange {
            user: *author,
            score: round_points(*count, winners.contains(author)),
            ..Default::default()
        })
        .collect();
    state.note = match winners.as_slice() {
        [] => "Nobody got a vote this round.".to_owned(),
        [one] => {
            let text = entries
                .iter()
                .find(|e| e.author() == *one)
                .map_or("", |e| e.text.as_str());
            format!("🏆 {} takes the round with “{text}”", name_of(&players, *one))
        }
        many => format!(
            "🏆 A tie! {} share the round.",
            many.iter()
                .map(|w| name_of(&players, *w))
                .collect::<Vec<_>>()
                .join(", ")
        ),
    };
    let commit = Commit {
        phase: "reveal",
        round: game.round,
        state: &encode(&state)?,
        deadline: after(app, reveal_secs(party)),
        status: "playing",
        players: &changes,
        entries: &[],
        pay: None,
    };
    let Some(done) = app.db.commit_party(id, game.seq, commit).await? else {
        return Ok(false);
    };
    let view = reveal_view(app, party, &done.game, &players, &state, &entries, &tally).await?;
    edit_card(app, &done.game, &view).await;
    Ok(true)
}

/// The results of a round: every entry with its author and votes, the winner marked.
async fn reveal_view(
    app: &App,
    party: Party,
    game: &PartyGame,
    players: &[PartyPlayer],
    state: &State,
    entries: &[PartyEntry],
    tally: &[(u64, u32)],
) -> anyhow::Result<InteractionResponseData> {
    let winners = round_winners(tally);
    let mut ranked: Vec<&PartyEntry> = entries.iter().collect();
    let votes_of = |user: u64| tally.iter().find(|(who, _)| *who == user).map(|(_, n)| *n);
    ranked.sort_by_key(|e| std::cmp::Reverse(votes_of(e.author()).unwrap_or(0)));
    let rows: Vec<(String, Option<String>, Option<u32>, bool)> = ranked
        .iter()
        .map(|e| {
            (
                e.text.clone(),
                Some(name_of(players, e.author())),
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
        "The votes are in".to_owned()
    } else {
        format!("Letters: {}", shown_letters(state))
    };
    let png = entries_png(
        app,
        party,
        game,
        &subtitle,
        picture,
        rows,
        &format!("{VOTE_POINTS} point a vote, {WIN_BONUS} more for the winner"),
    )
    .await?;
    let last = game.round >= game.rounds;
    let embed = EmbedBuilder::new()
        .color(WARN)
        .title(format!(
            "{} {} · round {} results",
            party.emoji(),
            party.label(),
            game.round
        ))
        .description(format!(
            "{}\n{} {}.",
            state.note,
            if last { "Final scores" } else { "Next round" },
            relative(game.deadline)
        ));
    Ok(card(None, embed, "results.png", png, Vec::new()))
}

/// The reveal has been up long enough: the next round, or the final scores.
async fn after_reveal(app: &Arc<App>, id: i64) -> anyhow::Result<bool> {
    let Some((game, players)) = snapshot(app, id).await? else {
        return Ok(false);
    };
    if !game.is_playing() || game.phase != "reveal" {
        return Ok(false);
    }
    let party = party_of(&game)?;
    if game.round >= game.rounds {
        return finish(app, game, players).await;
    }
    let mut state = decode(&game)?;
    let round = game.round + 1;
    begin_round(party, &mut state, round);
    let commit = Commit {
        phase: "write",
        round,
        state: &encode(&state)?,
        deadline: after(app, write_secs(party)),
        status: "playing",
        players: &[],
        entries: &[],
        pay: None,
    };
    let Some(done) = app.db.commit_party(id, game.seq, commit).await? else {
        return Ok(false);
    };
    edit_card(
        app,
        &done.game,
        &write_view(app, party, &done.game, &players, &state, 0).await?,
    )
    .await;
    Ok(true)
}

/// The last round is over: pay the players who wrote, count the games and show the final scores.
async fn finish(app: &Arc<App>, game: PartyGame, players: Vec<PartyPlayer>) -> anyhow::Result<bool> {
    let party = party_of(&game)?;
    let wrote: Vec<u64> = app
        .db
        .party_all_entries(game.id)
        .await?
        .iter()
        .map(PartyEntry::author)
        .collect();
    let top = players.iter().map(|p| p.score).max().unwrap_or(0);
    let winners: Vec<u64> = players
        .iter()
        .filter(|p| top > 0 && p.score == top)
        .map(PartyPlayer::id)
        .collect();
    let shells: Vec<(u64, i64)> = players
        .iter()
        .filter(|p| wrote.contains(&p.id()))
        .map(|p| (p.id(), writing_reward(p.score)))
        .collect();
    let commit = Commit {
        phase: "done",
        round: game.round,
        state: &game.state,
        deadline: game.deadline,
        status: "done",
        players: &[],
        entries: &[],
        pay: Some(Pay {
            game: party.game().key(),
            day: day(app),
            cap: party.game().daily_cap() as i32,
            reason: if party == Party::Postcard {
                "postcard_game"
            } else {
                "acronym_game"
            },
            shells: &shells,
        }),
    };
    let Some(done) = app.db.commit_party(game.id, game.seq, commit).await? else {
        return Ok(false);
    };
    count_games(app, party, &players, &winners).await;
    let mut ranked: Vec<&PartyPlayer> = players.iter().collect();
    ranked.sort_by_key(|p| (std::cmp::Reverse(p.score), p.seat));
    let rows = ranked
        .iter()
        .map(|p| {
            (
                p.name.clone(),
                format!("{} PT{}", p.score, if p.score == 1 { "" } else { "S" }),
                paid_note(&done.paid, p.id()),
                winners.contains(&p.id()),
            )
        })
        .collect();
    let png = standings_png(
        app,
        party.label(),
        &format!(
            "Final scores after {} round{}",
            game.rounds,
            if game.rounds == 1 { "" } else { "s" }
        ),
        rows,
        "Thanks for playing",
    )
    .await?;
    let line = match winners.as_slice() {
        [] => "Nobody scored this time.".to_owned(),
        [one] => format!("🏆 {} wins!", name_of(&players, *one)),
        many => format!(
            "🏆 A tie between {}!",
            many.iter()
                .map(|w| name_of(&players, *w))
                .collect::<Vec<_>>()
                .join(", ")
        ),
    };
    let embed = EmbedBuilder::new()
        .color(SUCCESS)
        .title(format!("{} {} is over", party.emoji(), party.label()))
        .description(line);
    edit_card(app, &done.game, &card(None, embed, "standings.png", png, Vec::new())).await;
    Ok(true)
}

/// The clock: a writing wait that ran out goes to the vote, a vote to the reveal, a reveal to what comes next.
pub(crate) async fn on_due(app: &Arc<App>, game: PartyGame) -> anyhow::Result<bool> {
    if !game.late(app.now()) {
        return Ok(false);
    }
    match game.phase.as_str() {
        "write" => to_vote(app, game.id).await,
        "vote" => to_reveal(app, game.id).await,
        "reveal" => after_reveal(app, game.id).await,
        _ => Ok(false),
    }
}
