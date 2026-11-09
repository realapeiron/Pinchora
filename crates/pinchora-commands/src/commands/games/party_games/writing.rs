//! Acronym Tide and Caption the Postcard: both are written games. Everyone types one line for what the
//! round shows (a phrase for the letters, a caption for the postcard), the lines go up anonymously and the
//! table votes, and then the names and the votes are shown. After the last round the scores decide.
//! A round is `write`, `vote`, `reveal`; each wait has a deadline, and the first player to finish all
//! their steps early (everyone has written, everyone has voted) moves it on sooner.

mod view;
use view::*;

use std::{str::Split, sync::Arc, time::Duration};

use pinchora_core::infra::i18n::Lang;
use pinchora_core::t;
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

use crate::App;
use crate::Cx;
use crate::commands::games::games::channel_lang;
use crate::commands::games::games::quiet;
use crate::commands::games::party_games::after;
use crate::commands::games::party_games::card;
use crate::commands::games::party_games::count_games;
use crate::commands::games::party_games::day;
use crate::commands::games::party_games::edit_card;
use crate::commands::games::party_games::game_lang;
use crate::commands::games::party_games::later;
use crate::commands::games::party_games::lobby;
use crate::commands::games::party_games::name_of;
use crate::commands::games::party_games::paid_note;
use crate::commands::games::party_games::postcard;
use crate::commands::games::party_games::push_button;
use crate::commands::games::party_games::remember;
use crate::commands::games::party_games::round_label;
use crate::commands::games::party_games::row;
use crate::commands::games::party_games::snapshot;
use crate::commands::games::party_games::standings_png;
use crate::commands::games::party_games::strict;
use crate::commands::games::party_games::text;
use crate::commands::games::party_games::theme;
use crate::infra::util::BRAND;
use crate::infra::util::SUCCESS;
use crate::infra::util::WARN;
use crate::infra::util::relative;

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
    #[serde(default)]
    tally: Tally,
}

/// What a round's tally came to. It is kept as what it was, not as a sentence, so the reveal is worded in the
/// language of the table whenever it is drawn.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum Tally {
    /// Nothing counted yet.
    #[default]
    Open,
    /// Nobody wrote a line.
    NoEntries,
    /// One line came in: nothing to vote on.
    OneEntry,
    NoVotes,
    Winner {
        name: String,
        text: String,
    },
    Tie {
        names: Vec<String>,
    },
}

impl Tally {
    fn say(&self, lang: Lang, party: Party) -> String {
        match self {
            Self::Open => String::new(),
            Self::NoEntries => t!(lang, &format!("{}.no-entries", party.key())),
            Self::OneEntry => t!(lang, &format!("{}.one-entry", party.key())),
            Self::NoVotes => t!(lang, "party.no-votes"),
            Self::Winner { name, text } => t!(lang, "party.round-winner", name = name, text = text),
            Self::Tie { names } => t!(lang, "party.round-tie", names = names.join(", ")),
        }
    }
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

pub(crate) async fn open(cx: &Cx, party: Party, rounds: i32) -> anyhow::Result<()> {
    let range = if party == Party::Postcard {
        scenes::ROUNDS
    } else {
        acronym::ROUNDS
    };
    let rounds = rounds.clamp(*range.start(), *range.end());
    let state = State {
        // The table is public and keeps playing in the server's language, whoever opened it.
        lang: channel_lang(cx).await,
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
    state.tally = Tally::Open;
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

/// `ac:` or `cn:` and the action: the lobby's buttons, `w:<id>:<round>` to write and `v:<id>:<round>:<n>`
/// to vote.
pub(crate) async fn on_button(cx: &Cx, party: Party, mut parts: Split<'_, char>) -> anyhow::Result<()> {
    let action = parts.next().unwrap_or_default();
    let mut number = |_: ()| parts.next().and_then(|v| v.parse::<i64>().ok());
    let lang = cx.lang();
    let Some(id) = number(()) else {
        return cx.reply_ephemeral(t!(lang, "party.expired-button")).await;
    };
    if lobby::on_button(cx, party, action, id).await? {
        return Ok(());
    }
    let (Some(round), me) = (number(()), cx.author()?.id.get()) else {
        return cx.reply_ephemeral(t!(lang, "party.expired-button")).await;
    };
    let Some((game, players)) = snapshot(&cx.app, party, id).await? else {
        return cx.reply_ephemeral(t!(lang, "party.expired-game")).await;
    };
    let round = round as i32;
    remember(cx, &game).await;
    let phase = if action == "w" { "write" } else { "vote" };
    if !game.is_playing() || game.round != round || game.phase != phase {
        return cx.reply_ephemeral(t!(lang, "party.step-over")).await;
    }
    if game.late(cx.app.now()) {
        if action == "w" {
            // The button that opens a form has to be answered in time: the step is taken after the answer.
            later(&cx.app, id, move |app| async move { on_due(&app, game).await });
        } else {
            on_due(&cx.app, game).await?;
        }
        return cx.reply_ephemeral(t!(lang, "party.step-late")).await;
    }
    if !players.iter().any(|p| p.id() == me) {
        return cx.reply_ephemeral(t!(lang, "party.not-in-game")).await;
    }
    match action {
        "w" => {
            let state = decode(&game)?;
            cx.modal(
                format!("{}:m:{id}:{round}", party.prefix()),
                &t!(lang, &format!("{}.modal-title", party.key())),
                vec![field(lang, party, &state)],
            )
            .await
        }
        "v" => {
            let Some(index) = number(()).map(|n| n as usize) else {
                return cx.reply_ephemeral(t!(lang, "party.expired-button")).await;
            };
            vote(cx, party, &game, &players, index).await
        }
        _ => cx.reply_ephemeral(t!(lang, "party.expired-button")).await,
    }
}

async fn vote(cx: &Cx, party: Party, game: &PartyGame, players: &[PartyPlayer], index: usize) -> anyhow::Result<()> {
    let app = &cx.app;
    let me = cx.author()?.id.get();
    let entries = app.db.party_entries(game.id, game.round).await?;
    let lang = cx.lang();
    let Some(entry) = entries.get(index) else {
        return cx.reply_ephemeral(t!(lang, "party.no-entry")).await;
    };
    let number = index + 1;
    let (said, new_vote) = match app
        .db
        .put_vote(game.id, game.round, me, entry.author(), app.now())
        .await?
    {
        PartyVote::Counted => (t!(lang, "party.vote.counted", number = number), true),
        PartyVote::Changed => (t!(lang, "party.vote.changed", number = number), false),
        PartyVote::Same => (t!(lang, "party.vote.same", number = number), false),
        PartyVote::OwnEntry => (t!(lang, "party.vote.own"), false),
        PartyVote::NotPlayer => (t!(lang, "party.vote.not-player"), false),
        PartyVote::NoEntry => (t!(lang, "party.no-entry"), false),
        PartyVote::Closed => (t!(lang, "party.vote.closed"), false),
    };
    cx.reply_ephemeral(said).await?;
    // Only a first vote changes the count. Everybody has voted: no reason to wait out the clock; otherwise
    // the count on the message follows, now or in a moment.
    if new_vote {
        let votes = app.db.party_votes(game.id, game.round).await?.len();
        if votes >= players.len() {
            to_reveal(app, party, game.id).await?;
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
        let Some((game, players)) = snapshot(app, party, id).await? else {
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
                pictured(
                    vote_embed(party, &game, game_lang(&game), players.len(), voted),
                    "entries.png",
                )
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
    let lang = cx.lang();
    let (Some(id), Some(round)) = (
        parts.next().and_then(|v| v.parse::<i64>().ok()),
        parts.next().and_then(|v| v.parse::<i32>().ok()),
    ) else {
        return cx.reply_ephemeral(t!(lang, "party.expired-form")).await;
    };
    let app = &cx.app;
    let me = cx.author()?.id.get();
    let Some((game, players)) = snapshot(app, party, id).await? else {
        return cx.reply_ephemeral(t!(lang, "party.expired-game")).await;
    };
    if !game.is_playing() || game.phase != "write" || game.round != round || game.late(app.now()) {
        if game.is_playing() && game.late(app.now()) {
            on_due(app, game).await?;
        }
        return cx.reply_ephemeral(t!(lang, &format!("{}.too-late", party.key()))).await;
    }
    let state = decode(&game)?;
    let typed = fields
        .iter()
        .find(|(field, _)| field == ENTRY_FIELD)
        .map_or("", |(_, value)| value.as_str());
    let cleaned = match text::clean_for(typed, strict(app, game.guild_id as u64).await, state.lang) {
        Ok(cleaned) => cleaned,
        Err(refused) => return cx.reply_ephemeral(refused.say(lang)).await,
    };
    let line = if party == Party::Postcard {
        scenes::check_caption(&cleaned).map_err(|e| e.say_in(lang))
    } else {
        acronym::check_phrase(state.lang, &cleaned, &letters_of(&state))
            .map_err(|e| e.say_in(lang, &letters_of(&state)))
    };
    let line = match line {
        Ok(line) => line,
        Err(why) => return cx.reply_ephemeral(why).await,
    };
    let fresh = match app.db.put_entry(id, round, "write", me, &line, app.now()).await? {
        None => {
            return cx
                .reply_ephemeral(t!(lang, &format!("{}.not-counted", party.key())))
                .await;
        }
        Some(fresh) => {
            let key = if fresh {
                "party.entry.locked"
            } else {
                "party.entry.rewritten"
            };
            cx.reply_ephemeral(t!(lang, key, line = line)).await?;
            fresh
        }
    };
    // A rewrite changes no count. A first line may be the last one, which moves the game on at once;
    // otherwise the count on the message follows, now or in a moment.
    if fresh {
        let written = app.db.party_entries(id, round).await?.len();
        if written >= players.len() {
            to_vote(app, party, id).await?;
        } else {
            count_changed(app, party, &game).await;
        }
    }
    Ok(())
}

// /Phases

/// Writing is over: up for a vote, or, with fewer than two lines, straight to a reveal that says so.
async fn to_vote(app: &Arc<App>, party: Party, id: i64) -> anyhow::Result<bool> {
    let Some((game, players)) = snapshot(app, party, id).await? else {
        return Ok(false);
    };
    if !game.is_playing() || game.phase != "write" {
        return Ok(false);
    }
    let mut state = decode(&game)?;
    let entries = app.db.party_entries(id, game.round).await?;
    if entries.len() < 2 {
        state.tally = if entries.is_empty() {
            Tally::NoEntries
        } else {
            Tally::OneEntry
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
async fn to_reveal(app: &Arc<App>, party: Party, id: i64) -> anyhow::Result<bool> {
    let Some((game, players)) = snapshot(app, party, id).await? else {
        return Ok(false);
    };
    if !game.is_playing() || game.phase != "vote" {
        return Ok(false);
    }
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
    state.tally = match winners.as_slice() {
        [] => Tally::NoVotes,
        [one] => Tally::Winner {
            name: name_of(state.lang, &players, *one),
            text: entries
                .iter()
                .find(|e| e.author() == *one)
                .map_or("", |e| e.text.as_str())
                .to_owned(),
        },
        many => Tally::Tie {
            names: many.iter().map(|w| name_of(state.lang, &players, *w)).collect(),
        },
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

/// The reveal has been up long enough: the next round, or the final scores.
async fn after_reveal(app: &Arc<App>, party: Party, id: i64) -> anyhow::Result<bool> {
    let Some((game, players)) = snapshot(app, party, id).await? else {
        return Ok(false);
    };
    if !game.is_playing() || game.phase != "reveal" {
        return Ok(false);
    }
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
    let lang = game_lang(&game);
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
                t!(lang, "party.points", count = p.score),
                paid_note(lang, &done.paid, p.id()),
                winners.contains(&p.id()),
            )
        })
        .collect();
    let png = standings_png(
        app,
        lang,
        &party.label_in(lang),
        &t!(lang, "party.final-after", count = game.rounds),
        rows,
        &t!(lang, "party.thanks"),
    )
    .await?;
    let line = match winners.as_slice() {
        [] => t!(lang, "party.nobody-scored"),
        [one] => t!(lang, "party.wins", name = name_of(lang, &players, *one)),
        many => t!(
            lang,
            "party.tie-between",
            names = many
                .iter()
                .map(|w| name_of(lang, &players, *w))
                .collect::<Vec<_>>()
                .join(", ")
        ),
    };
    let embed = EmbedBuilder::new()
        .color(SUCCESS)
        .title(t!(
            lang,
            "party.over-title",
            emoji = party.emoji(),
            label = party.label_in(lang)
        ))
        .description(line);
    edit_card(app, &done.game, &card(None, embed, "standings.png", png, Vec::new())).await;
    Ok(true)
}

/// The clock: a writing wait that ran out goes to the vote, a vote to the reveal, a reveal to what comes next.
pub(crate) async fn on_due(app: &Arc<App>, game: PartyGame) -> anyhow::Result<bool> {
    if !game.late(app.now()) {
        return Ok(false);
    }
    let party = party_of(&game)?;
    match game.phase.as_str() {
        "write" => to_vote(app, party, game.id).await,
        "vote" => to_reveal(app, party, game.id).await,
        "reveal" => after_reveal(app, party, game.id).await,
        _ => Ok(false),
    }
}
