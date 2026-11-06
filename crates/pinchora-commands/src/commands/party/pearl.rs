//! Hot Pearl: the pearl is with one player, a syllable on it, and they have a few seconds to type a word
//! that holds it. A word passes the pearl on with a new syllable; running out of time pops it, which costs
//! a life. The last player with lives wins.

use std::{str::Split, sync::Arc};

use pinchora_db::{Commit, NewEntry, PartyGame, PartyPlayer, Pay, PlayerChange};
use pinchora_game::party::{
    Dice, Party,
    filter::is_profane,
    hotpearl::{self, check_word, dictionary, next_player, turn_secs},
    pearl_reward,
};
use pinchora_render::{PearlCard, PearlSeat};
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
    after, card, count_games, day, edit_card, lobby, name_of, paid_note, push_button, remember, row, snapshot,
    standings_png, strict, text, theme,
};
use crate::{
    App, Cx,
    util::{BRAND, SUCCESS, mention, relative},
};

/// Custom id of the word input in the form.
pub(crate) const WORD_FIELD: &str = "word";

/// A game's own state, kept as JSON in `party_games.state`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct State {
    /// Lives everyone started with.
    lives: i32,
    /// The syllable on the pearl, lowercase.
    syl: String,
    /// Who holds the pearl.
    cur: u64,
    turn: u32,
    seed: u64,
    /// What just happened, for the line above the board.
    note: String,
    last: Option<String>,
}

fn decode(game: &PartyGame) -> anyhow::Result<State> {
    Ok(serde_json::from_str(&game.state)?)
}

fn encode(state: &State) -> anyhow::Result<String> {
    Ok(serde_json::to_string(state)?)
}

/// The syllable for turn `turn`: fixed by the game's seed, and never the one before it.
fn syllable(seed: u64, turn: u32, avoid: Option<&str>) -> String {
    let mut dice = Dice::new(seed ^ u64::from(turn).wrapping_mul(0x9e37_79b9_7f4a_7c15));
    dictionary().syllable(turn, &mut dice, avoid).to_owned()
}

pub(crate) async fn open(cx: &Cx, lives: i32) -> anyhow::Result<()> {
    let lives = lives.clamp(*hotpearl::LIVES.start(), *hotpearl::LIVES.end());
    let state = encode(&State {
        lives,
        ..Default::default()
    })?;
    lobby::open(cx, Party::HotPearl, lives, state).await
}

/// Seats the players in a random order and gives the first one the pearl.
pub(crate) async fn start(cx: &Cx, game: &PartyGame, players: &[PartyPlayer]) -> anyhow::Result<lobby::Started> {
    let app = &cx.app;
    let mut state = decode(game)?;
    state.seed = rand::random();
    let mut order: Vec<&PartyPlayer> = players.iter().collect();
    Dice::new(state.seed).shuffle(&mut order);
    let first = order[0];
    state.cur = first.id();
    state.turn = 0;
    state.syl = syllable(state.seed, 0, None);
    state.note = format!("{} goes first", first.name);
    let seats: Vec<PlayerChange> = order
        .iter()
        .enumerate()
        .map(|(seat, p)| PlayerChange {
            user: p.id(),
            seat: Some(seat as i32),
            lives: Some(state.lives),
            ..Default::default()
        })
        .collect();
    let commit = Commit {
        phase: "turn",
        round: 0,
        state: &encode(&state)?,
        deadline: after(app, turn_secs(0)),
        status: "playing",
        players: &seats,
        entries: &[],
        pay: None,
    };
    let Some(done) = app.db.commit_party(game.id, game.seq, commit).await? else {
        return Ok(lobby::Started::Stale);
    };
    let players = app.db.party_players(game.id).await?;
    cx.update(view(app, &done.game, &players, &state, None).await?).await?;
    Ok(lobby::Started::Yes)
}

/// The table: the pearl and the players round it, and who has to type.
async fn view(
    app: &App,
    game: &PartyGame,
    players: &[PartyPlayer],
    state: &State,
    headline: Option<&str>,
) -> anyhow::Result<InteractionResponseData> {
    let holder = players.iter().find(|p| p.id() == state.cur);
    let banner = if state.note.is_empty() {
        format!("{} holds the pearl", holder.map_or("Somebody", |p| p.name.as_str()))
    } else {
        state.note.clone()
    };
    let png = {
        let (syllable, banner, last) = (state.syl.clone(), banner.clone(), state.last.clone());
        let seats: Vec<(String, i32, bool, bool)> = players
            .iter()
            .map(|p| {
                (
                    p.name.clone(),
                    p.lives,
                    p.eliminated,
                    p.id() == state.cur && !p.eliminated,
                )
            })
            .collect();
        let max_lives = state.lives;
        app.render(move |r| {
            let seats: Vec<PearlSeat> = seats
                .iter()
                .map(|(name, lives, out, current)| PearlSeat {
                    name,
                    lives: *lives,
                    max_lives,
                    out: *out,
                    current: *current,
                })
                .collect();
            r.pearl_card(&PearlCard {
                syllable: &syllable,
                seats: &seats,
                banner: &banner,
                last_word: last.as_deref(),
                theme: theme(),
            })
        })
        .await?
    };
    let secs = turn_secs(state.turn);
    let embed = EmbedBuilder::new()
        .color(BRAND)
        .title(format!("{} Hot Pearl", Party::HotPearl.emoji()))
        .description(format!(
            "{}\n{}, type a word with **{}** before the pearl pops {}! ({secs} seconds a turn)",
            headline.unwrap_or(&banner),
            mention(state.cur),
            state.syl.to_uppercase(),
            relative(game.deadline)
        ));
    let button = push_button(
        Party::HotPearl,
        &[&"w", &game.id, &state.turn],
        "Type a word",
        ButtonStyle::Primary,
    );
    Ok(card(None, embed, "pearl.png", png, vec![row(vec![button])]))
}

#[allow(deprecated)] // the label belongs on the Label component, so the input's own stays empty
fn word_field(syllable: &str) -> Component {
    Component::Label(Label {
        component: Box::new(Component::TextInput(TextInput {
            custom_id: WORD_FIELD.to_owned(),
            id: None,
            label: None,
            max_length: Some(hotpearl::MAX_WORD as u16),
            min_length: Some(1),
            placeholder: Some("pearl".to_owned()),
            required: Some(true),
            style: TextInputStyle::Short,
            value: None,
        })),
        description: None,
        id: None,
        label: format!("A word with {}", syllable.to_uppercase()),
    })
}

/// What the table is in, after the checks every click and form shares: the game, its players, its
/// state, and the player holding the pearl; or the private answer to give instead.
enum Turn {
    Live(Box<(PartyGame, Vec<PartyPlayer>, State)>),
    Refused(String),
}

/// Whether `user` can play now on turn `turn` of game `id`. A pearl that ran out of time is popped here,
/// so a click never plays on after the clock.
async fn check(cx: &Cx, id: i64, turn: u32, user: u64) -> anyhow::Result<Turn> {
    let Some((game, players)) = snapshot(&cx.app, id).await? else {
        return Ok(Turn::Refused("This game has expired.".to_owned()));
    };
    let state = decode(&game)?;
    if !game.is_playing() || game.phase != "turn" || state.turn != turn {
        return Ok(Turn::Refused(
            "The pearl has moved on. Use the buttons on the newest message!".to_owned(),
        ));
    }
    if !players.iter().any(|p| p.id() == user) {
        return Ok(Turn::Refused("You are not playing this game.".to_owned()));
    }
    if user != state.cur {
        return Ok(Turn::Refused(format!(
            "It is {}'s turn, not yours.",
            name_of(&players, state.cur)
        )));
    }
    if game.late(cx.app.now()) {
        pop(&cx.app, game).await?;
        return Ok(Turn::Refused("Too slow: the pearl popped!".to_owned()));
    }
    Ok(Turn::Live(Box::new((game, players, state))))
}

fn ids(mut parts: Split<'_, char>) -> Option<(i64, u32)> {
    let id = parts.next()?.parse().ok()?;
    let turn = parts.next()?.parse().ok()?;
    Some((id, turn))
}

/// `hl:<action>:...`: the lobby's buttons, and `w:<id>:<turn>` to open the word form.
pub(crate) async fn on_button(cx: &Cx, mut parts: Split<'_, char>) -> anyhow::Result<()> {
    let action = parts.next().unwrap_or_default();
    if matches!(action, "j" | "l" | "s" | "x") {
        let Some(id) = parts.next().and_then(|v| v.parse().ok()) else {
            return cx.reply_ephemeral("This button has expired.").await;
        };
        lobby::on_button(cx, Party::HotPearl, action, id).await?;
        return Ok(());
    }
    let (Some((id, turn)), "w") = (ids(parts), action) else {
        return cx.reply_ephemeral("This button has expired.").await;
    };
    let me = cx.author()?.id.get();
    match check(cx, id, turn, me).await? {
        Turn::Refused(why) => cx.reply_ephemeral(why).await,
        Turn::Live(live) => {
            let (game, _, state) = *live;
            remember(cx, &game).await;
            cx.modal(format!("hl:m:{id}:{turn}"), "Hot Pearl", vec![word_field(&state.syl)])
                .await
        }
    }
}

/// The word form was submitted: `hl:m:<id>:<turn>`.
pub(crate) async fn on_modal(cx: &Cx, mut parts: Split<'_, char>, fields: &[(String, String)]) -> anyhow::Result<()> {
    parts.next();
    let Some((id, turn)) = ids(parts) else {
        return cx.reply_ephemeral("This form has expired.").await;
    };
    let me = cx.author()?.id.get();
    let typed = fields
        .iter()
        .find(|(field, _)| field == WORD_FIELD)
        .map_or("", |(_, value)| value.as_str());
    let (game, players, state) = match check(cx, id, turn, me).await? {
        Turn::Refused(why) => return cx.reply_ephemeral(why).await,
        Turn::Live(live) => *live,
    };
    let app = &cx.app;
    let used: Vec<String> = app
        .db
        .party_all_entries(id)
        .await?
        .into_iter()
        .map(|e| e.text)
        .collect();
    let word = match check_word(dictionary(), &state.syl, typed, &used) {
        Ok(word) => word,
        Err(refusal) => return cx.reply_ephemeral(refusal.say(&state.syl)).await,
    };
    // The list holds mild words ("damn" is out, "hell" is not); a strict server turns those away too.
    if is_profane(&word, true) && strict(app, game.guild_id as u64).await {
        return cx.reply_ephemeral(text::Refused::Profane.say()).await;
    }
    let order: Vec<(u64, bool)> = players.iter().map(|p| (p.id(), !p.eliminated)).collect();
    let Some(next) = next_player(&order, state.cur) else {
        return cx.reply_ephemeral("Nobody is left to pass the pearl to.").await;
    };
    let mut after_state = state.clone();
    after_state.cur = next;
    after_state.turn = state.turn + 1;
    after_state.syl = syllable(state.seed, after_state.turn, Some(&state.syl));
    after_state.note = format!(
        "{} played {}  ·  {} is up",
        name_of(&players, me),
        word.to_uppercase(),
        name_of(&players, next)
    );
    after_state.last = Some(word.clone());
    let commit = Commit {
        phase: "turn",
        round: after_state.turn as i32,
        state: &encode(&after_state)?,
        deadline: after(app, turn_secs(after_state.turn)),
        status: "playing",
        players: &[],
        entries: &[NewEntry {
            round: state.turn as i32,
            user: me,
            text: &word,
        }],
        pay: None,
    };
    let Some(done) = app.db.commit_party(id, game.seq, commit).await? else {
        return cx.reply_ephemeral("The pearl moved on before your word landed.").await;
    };
    cx.update(view(app, &done.game, &players, &after_state, None).await?)
        .await
}

/// Pops the pearl: the holder loses a life, and the game goes on with a fresh syllable or, with one player
/// left, ends. False when somebody else moved the game first.
pub(crate) async fn pop(app: &Arc<App>, game: PartyGame) -> anyhow::Result<bool> {
    let players = app.db.party_players(game.id).await?;
    let state = decode(&game)?;
    let Some(victim) = players.iter().find(|p| p.id() == state.cur) else {
        return Ok(false);
    };
    let lives = (victim.lives - 1).max(0);
    let out = lives == 0;
    let name = victim.name.clone();
    let change = PlayerChange {
        user: victim.id(),
        lives: Some(lives),
        eliminated: out.then_some(true),
        // Players who last longer rank higher: the turn they went out on is their score.
        score: if out { state.turn as i32 + 1 } else { 0 },
        ..Default::default()
    };
    let order: Vec<(u64, bool)> = players
        .iter()
        .map(|p| (p.id(), !p.eliminated && !(out && p.id() == victim.id())))
        .collect();
    let alive: Vec<u64> = order
        .iter()
        .filter(|(_, alive)| *alive)
        .map(|(user, _)| *user)
        .collect();
    let verdict = if out {
        format!("💥 The pearl popped on **{name}**, who is out!")
    } else {
        format!(
            "💥 The pearl popped on **{name}**! {lives} li{} left.",
            if lives == 1 { "fe" } else { "ves" }
        )
    };

    if alive.len() <= 1 {
        let winner = alive.first().copied();
        let mut next = state.clone();
        next.note = verdict.clone();
        let shells: Vec<(u64, i64)> = winner.map(|w| (w, pearl_reward(players.len()))).into_iter().collect();
        let commit = Commit {
            phase: "done",
            round: game.round,
            state: &encode(&next)?,
            deadline: game.deadline,
            status: "done",
            players: &[change],
            entries: &[],
            pay: Some(Pay {
                game: Party::HotPearl.game().key(),
                day: day(app),
                cap: Party::HotPearl.game().daily_cap() as i32,
                reason: "hotpearl_win",
                shells: &shells,
            }),
        };
        let Some(done) = app.db.commit_party(game.id, game.seq, commit).await? else {
            return Ok(false);
        };
        let players = app.db.party_players(game.id).await?;
        count_games(app, Party::HotPearl, &players, winner.as_slice()).await;
        let mut ranked: Vec<&PartyPlayer> = players.iter().collect();
        ranked.sort_by_key(|p| (Some(p.id()) != winner, std::cmp::Reverse(p.score), p.seat));
        let rows = ranked
            .iter()
            .map(|p| {
                let won = Some(p.id()) == winner;
                (
                    p.name.clone(),
                    if won {
                        "LAST ONE STANDING".to_owned()
                    } else {
                        format!("OUT ON TURN {}", p.score)
                    },
                    if won {
                        paid_note(&done.paid, p.id())
                    } else {
                        String::new()
                    },
                    won,
                )
            })
            .collect();
        let winner_name = winner.map_or_else(|| "Nobody".to_owned(), |w| name_of(&players, w));
        let subtitle = format!("{winner_name} wins after {} turns", state.turn + 1);
        let png = standings_png(app, "Hot Pearl", &subtitle, rows, "Thanks for playing").await?;
        let embed = EmbedBuilder::new()
            .color(SUCCESS)
            .title(format!("{} Hot Pearl is over", Party::HotPearl.emoji()))
            .description(format!(
                "{verdict}\n🏆 {} wins!",
                winner.map_or_else(|| "Nobody".to_owned(), mention)
            ));
        edit_card(app, &done.game, &card(None, embed, "standings.png", png, Vec::new())).await;
        return Ok(true);
    }

    let Some(next_holder) = next_player(&order, victim.id()) else {
        return Ok(false);
    };
    let mut next = state.clone();
    next.cur = next_holder;
    next.turn = state.turn + 1;
    next.syl = syllable(state.seed, next.turn, Some(&state.syl));
    next.note = format!("BOOM! {name} {}", if out { "is out" } else { "lost a life" });
    let commit = Commit {
        phase: "turn",
        round: next.turn as i32,
        state: &encode(&next)?,
        deadline: after(app, turn_secs(next.turn)),
        status: "playing",
        players: &[change],
        entries: &[],
        pay: None,
    };
    let Some(done) = app.db.commit_party(game.id, game.seq, commit).await? else {
        return Ok(false);
    };
    let players = app.db.party_players(game.id).await?;
    // The verdict goes above the new turn.
    let data = view(app, &done.game, &players, &next, Some(&verdict)).await?;
    edit_card(app, &done.game, &data).await;
    Ok(true)
}

/// The clock: a pearl whose time ran out pops.
pub(crate) async fn on_due(app: &Arc<App>, game: PartyGame) -> anyhow::Result<bool> {
    if game.phase == "turn" && game.late(app.now()) {
        return pop(app, game).await;
    }
    Ok(false)
}
