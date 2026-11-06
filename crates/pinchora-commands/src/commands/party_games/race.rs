//! Wordle Race: two to four players look for the same word, each on a private board. The table sees a
//! progress card with the colours of everyone's guesses and no letters; the first to solve it wins.

use std::{str::Split, sync::Arc};

use pinchora_db::{Commit, PartyGame, PartyPlayer, Pay};
use pinchora_game::{
    party::{
        Party,
        race::{self, Board, RACE_SECS},
        race_reward,
    },
    play::{PlayContent, wordle},
};
use pinchora_render::{ABSENT, CORRECT, PRESENT, RaceCard, Racer, RacerState, Theme, WordleCard, WordleRow};
use serde::{Deserialize, Serialize};
use twilight_model::{
    channel::message::{
        Component,
        component::{ButtonStyle, Label, TextInput, TextInputStyle},
    },
    http::interaction::InteractionResponseData,
};
use twilight_util::builder::embed::EmbedBuilder;

use super::{
    after, card, count_games, day, edit_card, later, lobby, name_of, paid_note, push_button, remember, row, snapshot,
    standings_png, theme,
};
use crate::{
    App, Cx,
    util::{BRAND, SUCCESS, WARN, relative},
};

/// Custom id of the guess input in the form.
pub(crate) const GUESS_FIELD: &str = "guess";

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct State {
    /// The word everybody is after.
    word: String,
}

fn decode(game: &PartyGame) -> anyhow::Result<State> {
    Ok(serde_json::from_str(&game.state)?)
}

fn mark(tile: wordle::Tile) -> u8 {
    match tile {
        wordle::Tile::Absent => ABSENT,
        wordle::Tile::Present => PRESENT,
        wordle::Tile::Correct => CORRECT,
    }
}

pub(crate) async fn open(cx: &Cx) -> anyhow::Result<()> {
    lobby::open(cx, Party::WordRace, 1, serde_json::to_string(&State::default())?).await
}

/// Picks the word and starts the clock.
pub(crate) async fn start(cx: &Cx, game: &PartyGame, players: &[PartyPlayer]) -> anyhow::Result<lobby::Started> {
    let app = &cx.app;
    let answers = &PlayContent::get().words.answers;
    let state = State {
        word: answers[rand::random_range(0..answers.len())].clone(),
    };
    let commit = Commit {
        phase: "race",
        round: 1,
        state: &serde_json::to_string(&state)?,
        deadline: after(app, RACE_SECS),
        status: "playing",
        players: &[],
        entries: &[],
        pay: None,
    };
    let Some(done) = app.db.commit_party(game.id, game.seq, commit).await? else {
        return Ok(lobby::Started::Stale);
    };
    cx.update(view(app, &done.game, players, &state).await?).await?;
    Ok(lobby::Started::Yes)
}

fn racer_state(board: Board) -> RacerState {
    match board {
        Board::Playing => RacerState::Playing,
        Board::Solved { tries } => RacerState::Solved(tries),
        Board::Failed => RacerState::Failed,
    }
}

/// The public progress card: a small board for each racer, colours only.
async fn view(
    app: &App,
    game: &PartyGame,
    players: &[PartyPlayer],
    state: &State,
) -> anyhow::Result<InteractionResponseData> {
    let boards: Vec<(String, Vec<[u8; 5]>, RacerState)> = players
        .iter()
        .map(|p| {
            (
                p.name.clone(),
                p.guesses.iter().map(|g| race::marks(g, &state.word)).collect(),
                racer_state(race::board(&p.guesses, &state.word)),
            )
        })
        .collect();
    let png = app
        .render(move |r| {
            let racers: Vec<Racer> = boards
                .iter()
                .map(|(name, rows, state)| Racer {
                    name,
                    rows,
                    state: *state,
                })
                .collect();
            r.race_card(&RaceCard {
                title: "Wordle Race",
                racers: &racers,
                footer: "First to find the word wins",
                theme: theme(),
            })
        })
        .await?;
    let embed = EmbedBuilder::new()
        .color(BRAND)
        .title(format!("{} Wordle Race", Party::WordRace.emoji()))
        .description(format!(
            "Find the same five-letter word, first! Your own board is private: press **Guess** and the board comes back only to you. The race ends {}.",
            relative(game.deadline)
        ));
    let button = push_button(Party::WordRace, &[&"g", &game.id], "Guess", ButtonStyle::Primary);
    Ok(card(None, embed, "race.png", png, vec![row(vec![button])]))
}

#[allow(deprecated)] // the label belongs on the Label component, so the input's own stays empty
fn guess_field() -> Component {
    Component::Label(Label {
        component: Box::new(Component::TextInput(TextInput {
            custom_id: GUESS_FIELD.to_owned(),
            id: None,
            label: None,
            max_length: Some(5),
            min_length: Some(5),
            placeholder: Some("pearl".to_owned()),
            required: Some(true),
            style: TextInputStyle::Short,
            value: None,
        })),
        description: None,
        id: None,
        label: "Your five-letter guess".to_owned(),
    })
}

/// `wx:<action>:<id>`: the lobby's buttons, and `g` to open the guess form.
pub(crate) async fn on_button(cx: &Cx, mut parts: Split<'_, char>) -> anyhow::Result<()> {
    let action = parts.next().unwrap_or_default();
    let Some(id) = parts.next().and_then(|v| v.parse::<i64>().ok()) else {
        return cx.reply_ephemeral("This button has expired.").await;
    };
    if lobby::on_button(cx, Party::WordRace, action, id).await? {
        return Ok(());
    }
    if action != "g" {
        return cx.reply_ephemeral("This button has expired.").await;
    }
    let me = cx.author()?.id.get();
    let Some((game, players)) = snapshot(&cx.app, Party::WordRace, id).await? else {
        return cx.reply_ephemeral("This race has expired.").await;
    };
    remember(cx, &game).await;
    if !game.is_playing() || game.phase != "race" {
        return cx.reply_ephemeral("This race is over.").await;
    }
    if game.late(cx.app.now()) {
        // The button that opens the form has to be answered in time: the race is ended after the answer.
        later(&cx.app, id, move |app| async move { finish(&app, id, None).await });
        return cx.reply_ephemeral("Time is up: the race is over!").await;
    }
    let Some(player) = players.iter().find(|p| p.id() == me) else {
        return cx
            .reply_ephemeral("You are not in this race: only its players guess.")
            .await;
    };
    let state = decode(&game)?;
    if race::board(&player.guesses, &state.word) != Board::Playing {
        return cx.reply_ephemeral("Your board is finished. Wait for the others!").await;
    }
    cx.modal(format!("wx:m:{id}"), "Wordle Race guess", vec![guess_field()])
        .await
}

/// Your own board: tiles, keyboard, and where you stand, shown only to you.
async fn private_board(
    app: &App,
    guesses: &[String],
    word: &str,
    note: &str,
) -> anyhow::Result<InteractionResponseData> {
    let scored: Vec<(&str, [wordle::Tile; 5])> = guesses.iter().map(|g| (g.as_str(), wordle::score(g, word))).collect();
    let rows: Vec<WordleRow> = scored
        .iter()
        .map(|(guess, tiles)| WordleRow {
            letters: guess.chars().collect::<Vec<_>>().try_into().unwrap_or(['?'; 5]),
            marks: tiles.map(mark),
        })
        .collect();
    let mut keys = [0u8; 26];
    for (i, key) in wordle::keyboard(scored.iter().copied()).iter().enumerate() {
        keys[i] = key.map_or(0, mark);
    }
    let tries = guesses.len();
    let footer = format!("Try {} of {}", (tries + 1).min(wordle::MAX_TRIES), wordle::MAX_TRIES);
    let png = app
        .render(move |r| {
            r.wordle_card(&WordleCard {
                title: "Your race board",
                rows: &rows,
                keys,
                footer: &footer,
                theme: Theme::reef(),
            })
        })
        .await?;
    Ok(Cx::card(
        EmbedBuilder::new().color(SUCCESS).title("Your board").description(note),
        "board.png",
        png,
        Vec::new(),
        true,
    ))
}

/// The guess form was submitted: `wx:m:<id>`.
pub(crate) async fn on_modal(cx: &Cx, mut parts: Split<'_, char>, fields: &[(String, String)]) -> anyhow::Result<()> {
    parts.next();
    let Some(id) = parts.next().and_then(|v| v.parse::<i64>().ok()) else {
        return cx.reply_ephemeral("This form has expired.").await;
    };
    let app = &cx.app;
    let me = cx.author()?.id.get();
    let Some((game, players)) = snapshot(app, Party::WordRace, id).await? else {
        return cx.reply_ephemeral("This race has expired.").await;
    };
    if !game.is_playing() || game.phase != "race" {
        return cx.reply_ephemeral("This race is over.").await;
    }
    if game.late(app.now()) {
        finish(app, id, None).await?;
        return cx.reply_ephemeral("Time is up: the race is over!").await;
    }
    if !players.iter().any(|p| p.id() == me) {
        return cx
            .reply_ephemeral("You are not in this race: only its players guess.")
            .await;
    }
    let typed = fields
        .iter()
        .find(|(field, _)| field == GUESS_FIELD)
        .map_or("", |(_, value)| value.as_str());
    let Some(guess) = wordle::normalize(typed) else {
        return cx.reply_ephemeral("A guess is exactly five letters, a to z.").await;
    };
    if !PlayContent::get().words.is_allowed(&guess) {
        return cx
            .reply_ephemeral(format!(
                "**{}** isn't in my word list. Try another word!",
                guess.to_uppercase()
            ))
            .await;
    }
    let state = decode(&game)?;
    let Some(board) = app.db.race_guess(id, me, &guess, &state.word, app.now()).await? else {
        return cx
            .reply_ephemeral("Your board is finished, so that guess did not count.")
            .await;
    };
    let outcome = race::board(&board.guesses, &state.word);
    let note = match outcome {
        Board::Solved { tries } => format!("You found it in **{tries}**! Everyone else sees the result now."),
        Board::Failed => "Out of tries. The word stays secret until the race is over.".to_owned(),
        Board::Playing => "🟩 right spot · 🟨 wrong spot · ⬛ not in the word.".to_owned(),
    };
    cx.reply(private_board(app, &board.guesses, &state.word, &note).await?)
        .await?;

    match outcome {
        Board::Solved { tries } => {
            finish(app, id, Some((me, tries))).await?;
        }
        Board::Failed | Board::Playing => {
            let Some((game, players)) = snapshot(app, Party::WordRace, id).await? else {
                return Ok(());
            };
            let boards: Vec<Board> = players.iter().map(|p| race::board(&p.guesses, &state.word)).collect();
            if race::everyone_failed(&boards) {
                finish(app, id, None).await?;
            } else if game.is_playing() {
                edit_card(app, &game, &view(app, &game, &players, &state).await?).await;
            }
        }
    }
    Ok(())
}

/// Ends the race: `winner` is who solved it and in how many tries, or nobody. Shows the word and the
/// standings. False when it was already over.
pub(crate) async fn finish(app: &Arc<App>, id: i64, winner: Option<(u64, usize)>) -> anyhow::Result<bool> {
    let Some((game, players)) = snapshot(app, Party::WordRace, id).await? else {
        return Ok(false);
    };
    if !game.is_playing() {
        return Ok(false);
    }
    let state = decode(&game)?;
    let shells: Vec<(u64, i64)> = winner
        .map(|(user, tries)| (user, race_reward(tries)))
        .into_iter()
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
            game: Party::WordRace.game().key(),
            day: day(app),
            cap: Party::WordRace.game().daily_cap() as i32,
            reason: "wordrace_win",
            shells: &shells,
        }),
    };
    let Some(done) = app.db.commit_party(id, game.seq, commit).await? else {
        return Ok(false);
    };
    let winner_id = winner.map(|(user, _)| user);
    count_games(app, Party::WordRace, &players, winner_id.as_slice()).await;
    let mut ranked: Vec<&PartyPlayer> = players.iter().collect();
    ranked.sort_by_key(|p| (Some(p.id()) != winner_id, p.guesses.len(), p.seat));
    let rows = ranked
        .iter()
        .map(|p| {
            let won = Some(p.id()) == winner_id;
            let score = match race::board(&p.guesses, &state.word) {
                Board::Solved { tries } => format!("SOLVED IN {tries}"),
                Board::Failed => "OUT OF TRIES".to_owned(),
                Board::Playing => format!("{} GUESSES", p.guesses.len()),
            };
            (
                p.name.clone(),
                score,
                if won {
                    paid_note(&done.paid, p.id())
                } else {
                    String::new()
                },
                won,
            )
        })
        .collect();
    let subtitle = format!("The word was {}", state.word.to_uppercase());
    let png = standings_png(app, "Wordle Race", &subtitle, rows, "Thanks for racing").await?;
    let line = match winner_id {
        Some(user) => format!("🏆 {} found the word first!", name_of(&players, user)),
        None => "Nobody found the word this time.".to_owned(),
    };
    let embed = EmbedBuilder::new()
        .color(if winner_id.is_some() { SUCCESS } else { WARN })
        .title(format!("{} Wordle Race is over", Party::WordRace.emoji()))
        .description(format!("{line}\nThe word was **{}**.", state.word.to_uppercase()));
    edit_card(app, &done.game, &card(None, embed, "standings.png", png, Vec::new())).await;
    Ok(true)
}

/// The clock: a race that ran out of time ends with no winner.
pub(crate) async fn on_due(app: &Arc<App>, game: PartyGame) -> anyhow::Result<bool> {
    if game.phase == "race" && game.late(app.now()) {
        return finish(app, game.id, None).await;
    }
    Ok(false)
}
