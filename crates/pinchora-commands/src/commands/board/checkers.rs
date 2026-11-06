//! `/duel checkers`: Pearl Checkers, English draughts. A move is two picks from selects under the board: the
//! piece, then the move (a step, or a jump, which is compulsory).

use std::str::Split;

use chrono::{DateTime, Duration, Utc};
use pinchora_db::GameSession;
use pinchora_game::play::{
    Game as Play,
    checkers::{self as rules, Game, Move, Outcome},
};
use pinchora_render::{CheckersCard, Theme};
use twilight_model::{channel::message::Component, http::interaction::InteractionResponseData};
use twilight_util::builder::embed::EmbedBuilder;

use super::{
    CHECKERS_SECS, conclude, confirm_resign, edit_card, option, players, resign_button, resigned, select_row,
    select_rows,
};
use crate::{
    App, Cx,
    commands::{
        duel::{Kind as DuelKind, answer, expire_here},
        games::{location, pack, quiet, unpack},
    },
    util::{BRAND, SUCCESS, mention, relative, thousands},
};

/// The state's own fields: which player (0 host, 1 guest) has the pearls and moves first, the position,
/// plies played and the squares the last move visited.
struct State {
    pearl: usize,
    game: Game,
    plies: usize,
    last: Vec<u8>,
}

impl State {
    fn encode(&self) -> String {
        let last = if self.last.is_empty() {
            "-".to_owned()
        } else {
            self.last.iter().map(u8::to_string).collect::<Vec<_>>().join(".")
        };
        format!(
            "{}:{}:{}:{}:{}:{last}",
            self.pearl,
            self.game.cells_text(),
            self.game.turn,
            self.game.quiet,
            self.plies
        )
    }

    fn decode(fields: &str) -> Option<Self> {
        let mut parts = fields.split(':');
        let pearl = parts.next()?.parse().ok().filter(|p| *p < 2)?;
        let cells = parts.next()?;
        let turn = parts.next()?.parse().ok()?;
        let quiet = parts.next()?.parse().ok()?;
        let plies = parts.next()?.parse().ok()?;
        let last = match parts.next()? {
            "-" => Vec::new(),
            text => text
                .split('.')
                .map(|s| s.parse().ok().filter(|sq| *sq < 32))
                .collect::<Option<_>>()?,
        };
        Some(Self {
            pearl,
            game: Game::from_parts(cells, turn, quiet)?,
            plies,
            last,
        })
    }

    /// The player (0 or 1) who plays side `side` (1 or 2).
    fn player(&self, side: u8) -> usize {
        if side == 1 { self.pearl } else { 1 - self.pearl }
    }

    fn mover(&self) -> usize {
        self.player(self.game.turn)
    }
}

pub(crate) fn initial_fields() -> String {
    State {
        pearl: rand::random_range(0..2),
        game: Game::new(),
        plies: 0,
        last: Vec::new(),
    }
    .encode()
}

/// The player to move when the game timed out.
pub(crate) fn slow_players(session: &GameSession) -> Vec<u64> {
    let (fields, _) = unpack(&session.state);
    State::decode(fields)
        .map(|state| vec![players(session)[state.mover()]])
        .unwrap_or_default()
}

/// How a game ended: the winner (0 host, 1 guest) or `None` for a draw, and why.
struct End {
    winner: Option<usize>,
    how: String,
}

impl End {
    fn of(state: &State, outcome: Outcome) -> Self {
        match outcome {
            Outcome::Won(side) => Self {
                winner: Some(state.player(side)),
                how: "taking or blocking every piece".to_owned(),
            },
            Outcome::Draw => Self {
                winner: None,
                how: "the 40-move rule".to_owned(),
            },
        }
    }
}

/// How a board is shown: the end of the game if it is over, when the move is due, the piece picked up and a note to add.
#[derive(Default)]
struct Show<'a> {
    end: Option<&'a End>,
    deadline: DateTime<Utc>,
    selected: Option<u8>,
    note: &'a str,
}

/// The board message: the picture, whose move it is, and the menus to make it.
async fn message(
    app: &App,
    session: &GameSession,
    names: &[String; 2],
    state: &State,
    show: &Show<'_>,
) -> anyhow::Result<InteractionResponseData> {
    let Show {
        end,
        deadline,
        selected,
        note,
    } = *show;
    let ids = players(session);
    let moves = state.game.legal_moves();
    let targets: Vec<u8> = selected
        .map(|from| moves.iter().filter(|m| m.from() == from).map(Move::to).collect())
        .unwrap_or_default();
    let cells: [u8; 32] = std::array::from_fn(|sq| state.game.cell(sq as u8));
    let pearl_name = names[state.pearl].clone();
    let black_name = names[1 - state.pearl].clone();
    let turn = end.is_none().then(|| state.game.turn - 1);
    let winner = end.and_then(|e| e.winner).map(|w| u8::from(w != state.pearl));
    let mover = state.mover();
    let status = match end {
        Some(End { winner: Some(w), how }) => format!("{} wins by {how}", names[*w]),
        Some(End { winner: None, how }) => format!("A draw by {how}"),
        None if moves.iter().any(|m| !m.captured.is_empty()) => format!("{} must jump!", names[mover]),
        None => format!("{} to move", names[mover]),
    };
    let content = match end {
        None => format!(
            "⚪ {} (pearls) vs 🟣 {} (black pearls)\n{}'s move, {}{note}",
            mention(ids[state.pearl]),
            mention(ids[1 - state.pearl]),
            mention(ids[mover]),
            relative(deadline)
        ),
        Some(End { winner: Some(w), how }) => format!("🏆 {} wins by {how}! {note}", mention(ids[*w])),
        Some(End { winner: None, how }) => format!("🤝 It's a draw by {how}! {note}"),
    };
    let pot = if session.bet > 0 { session.bet * 2 } else { 0 };
    let png = {
        let (status, targets, last) = (status.clone(), targets.clone(), state.last.clone());
        app.render(move |r| {
            r.checkers_card(&CheckersCard {
                cells: &cells,
                names: [&pearl_name, &black_name],
                turn,
                winner,
                last: &last,
                selected,
                targets: &targets,
                status: &status,
                pot,
                theme: Theme::reef(),
            })
        })
        .await?
    };
    let mut description = if state.plies == 0 {
        "Pearls move first. Pick a piece, then its move. Jumps are compulsory.".to_owned()
    } else {
        format!("Move {}", state.plies / 2 + 1)
    };
    if pot > 0 {
        description.push_str(&format!(
            "\nPot: **{}** shells ({} each)",
            thousands(pot),
            thousands(session.bet)
        ));
    }
    let embed = EmbedBuilder::new()
        .color(if end.is_some() { SUCCESS } else { BRAND })
        .title("Pearl Checkers")
        .description(description);
    let mut components = Vec::new();
    if end.is_none() {
        components = menus(session.id, state, &moves, selected);
    }
    let mut data = Cx::card(embed, "checkers.png", png, components, false);
    data.content = Some(content);
    data.allowed_mentions = quiet();
    Ok(data)
}

/// The piece menu, the move menu once a piece is picked, and Resign.
fn menus(id: i64, state: &State, moves: &[Move], selected: Option<u8>) -> Vec<Component> {
    let mut froms: Vec<u8> = moves.iter().map(Move::from).collect();
    froms.sort_unstable();
    froms.dedup();
    let piece_options = froms
        .iter()
        .map(|&sq| {
            let count = moves.iter().filter(|m| m.from() == sq).count();
            let jumps = moves.iter().any(|m| m.from() == sq && !m.captured.is_empty());
            option(
                format!(
                    "{} on {}",
                    if state.game.is_king(sq) { "King" } else { "Man" },
                    rules::name(sq)
                ),
                sq.to_string(),
                Some(format!(
                    "{count} move{}{}",
                    if count == 1 { "" } else { "s" },
                    if jumps { ", jumps" } else { "" }
                )),
                selected == Some(sq),
            )
        })
        .collect();
    let mut rows = vec![select_row(
        format!("ck:s:{id}:{}", state.plies),
        "Pick a piece to move",
        piece_options,
        (1, 1),
    )];
    if let Some(from) = selected {
        let options = moves
            .iter()
            .filter(|m| m.from() == from)
            .map(|m| {
                let takes = m.captured.len();
                option(m.text(), m.code(), (takes > 0).then(|| format!("takes {takes}")), false)
            })
            .collect();
        rows.extend(select_rows(
            &format!("ck:t:{id}:{}:{from}", state.plies),
            &format!("Where should the piece on {} go?", rules::name(from)),
            options,
            (1, 1),
        ));
    }
    rows.push(resign_button("ck", id));
    rows
}

/// The first board, shown when the challenge is accepted.
pub(crate) async fn begin(cx: &Cx, session: &GameSession) -> anyhow::Result<()> {
    let (fields, names) = unpack(&session.state);
    let state = State::decode(fields).ok_or_else(|| anyhow::anyhow!("bad checkers state {fields:?}"))?;
    let data = message(
        &cx.app,
        session,
        &names,
        &state,
        &Show {
            deadline: session.expires_at,
            ..Show::default()
        },
    )
    .await?;
    cx.update(data).await
}

/// `ck:y:<id>` and `ck:n:<id>` answer a challenge, `ck:s:<id>:<plies>` picks a piece (the value is its
/// square), `ck:t:<id>:<plies>:<from>` picks its move (the value is the path), `ck:q:<id>` asks to resign
/// and `ck:R:<id>` confirms.
pub(crate) async fn on_component(cx: &Cx, mut args: Split<'_, char>, values: &[String]) -> anyhow::Result<()> {
    let action = args.next();
    let Some(id) = args.next().and_then(|v| v.parse::<i64>().ok()) else {
        return cx.reply_ephemeral("This game has expired.").await;
    };
    let value = values.first().map(String::as_str);
    match action {
        Some("y") => answer(cx, DuelKind::Checkers, id, true).await,
        Some("n") => answer(cx, DuelKind::Checkers, id, false).await,
        Some("q") => confirm_resign(cx, "ck", id).await,
        Some("R") => on_resign(cx, id).await,
        Some("s") => match (
            args.next().and_then(|v| v.parse::<usize>().ok()),
            value.and_then(|v| v.parse::<u8>().ok()),
        ) {
            (Some(plies), Some(from)) => on_piece(cx, id, plies, from).await,
            _ => cx.reply_ephemeral("This menu has expired.").await,
        },
        Some("t") => match (args.next().and_then(|v| v.parse::<usize>().ok()), value) {
            (Some(plies), Some(path)) => on_move(cx, id, plies, path).await,
            _ => cx.reply_ephemeral("This menu has expired.").await,
        },
        _ => cx.reply_ephemeral("This button has expired.").await,
    }
}

/// A running game the clicker plays in, with the board as it stands; see the chess module's `running`.
async fn running(cx: &Cx, id: i64, plies: Option<usize>) -> anyhow::Result<Option<(GameSession, [String; 2], State)>> {
    let me = cx.author()?.id.get();
    let Some(session) = cx.app.db.game(id).await?.filter(|s| s.is_active()) else {
        cx.reply_ephemeral("This game is over.").await?;
        return Ok(None);
    };
    if !session.has_player(me) {
        cx.reply_ephemeral("You're not in this game. Challenge someone with `/duel checkers`!")
            .await?;
        return Ok(None);
    }
    if session.expired() {
        expire_here(cx, &session).await?;
        return Ok(None);
    }
    let (fields, names) = unpack(&session.state);
    let state = State::decode(fields).ok_or_else(|| anyhow::anyhow!("bad checkers state {fields:?}"))?;
    if let Some(plies) = plies {
        if state.plies != plies {
            let data = message(
                &cx.app,
                &session,
                &names,
                &state,
                &Show {
                    deadline: session.expires_at,
                    ..Show::default()
                },
            )
            .await?;
            cx.update(data).await?;
            return Ok(None);
        }
        if state.mover() != usize::from(me != players(&session)[0]) {
            cx.reply_ephemeral("Not your turn yet.").await?;
            return Ok(None);
        }
    }
    Ok(Some((session, names, state)))
}

async fn on_piece(cx: &Cx, id: i64, plies: usize, from: u8) -> anyhow::Result<()> {
    let Some((session, names, state)) = running(cx, id, Some(plies)).await? else {
        return Ok(());
    };
    if !state.game.legal_moves().iter().any(|m| m.from() == from) {
        return cx.reply_ephemeral("That piece can't move.").await;
    }
    let data = message(
        &cx.app,
        &session,
        &names,
        &state,
        &Show {
            deadline: session.expires_at,
            selected: Some(from),
            ..Show::default()
        },
    )
    .await?;
    cx.update(data).await
}

async fn on_move(cx: &Cx, id: i64, plies: usize, path: &str) -> anyhow::Result<()> {
    let Some((session, names, state)) = running(cx, id, Some(plies)).await? else {
        return Ok(());
    };
    let wanted: Option<Vec<u8>> = path.split('.').map(|s| s.parse().ok()).collect();
    let Some(mv) = state
        .game
        .legal_moves()
        .into_iter()
        .find(|m| wanted.as_deref() == Some(m.path.as_slice()))
    else {
        return cx.reply_ephemeral("That move isn't legal.").await;
    };
    let mut game = state.game.clone();
    game.play(&mv)
        .ok_or_else(|| anyhow::anyhow!("a legal move was refused"))?;
    let next = State {
        pearl: state.pearl,
        game,
        plies: state.plies + 1,
        last: mv.path.clone(),
    };
    let packed = pack(&next.encode(), [&names[0], &names[1]]);
    let Some(outcome) = next.game.outcome() else {
        if !cx
            .app
            .db
            .advance_game(id, &session.state, &packed, CHECKERS_SECS)
            .await?
        {
            return cx
                .reply_ephemeral("The board changed under you. Look again and pick a piece.")
                .await;
        }
        let deadline = Utc::now() + Duration::seconds(CHECKERS_SECS);
        let data = message(
            &cx.app,
            &session,
            &names,
            &next,
            &Show {
                deadline,
                ..Show::default()
            },
        )
        .await?;
        return cx.update(data).await;
    };
    let end = End::of(&next, outcome);
    let winner = end.winner.map(|w| players(&session)[w]);
    let Some(note) = conclude(&cx.app, &session, &packed, Play::Checkers, winner).await? else {
        return cx.reply_ephemeral("The board changed under you. Look again.").await;
    };
    let data = message(
        &cx.app,
        &session,
        &names,
        &next,
        &Show {
            deadline: Utc::now(),
            end: Some(&end),
            note: &note,
            ..Show::default()
        },
    )
    .await?;
    cx.update(data).await
}

/// The player who confirmed on the private message resigns; the public board shows it.
async fn on_resign(cx: &Cx, id: i64) -> anyhow::Result<()> {
    let Some((session, names, state)) = running(cx, id, None).await? else {
        return Ok(());
    };
    let me = cx.author()?.id.get();
    let ids = players(&session);
    let winner = usize::from(me == ids[0]);
    let Some(note) = conclude(&cx.app, &session, &session.state, Play::Checkers, Some(ids[winner])).await? else {
        return resigned(cx, "The game was over before you could resign.").await;
    };
    let end = End {
        winner: Some(winner),
        how: format!("resignation ({} gave up)", names[1 - winner]),
    };
    let data = message(
        &cx.app,
        &session,
        &names,
        &state,
        &Show {
            deadline: Utc::now(),
            end: Some(&end),
            note: &note,
            ..Show::default()
        },
    )
    .await?;
    if let Some((channel, message)) = location(&session) {
        edit_card(&cx.app, channel, message, &data).await;
    }
    resigned(cx, "You resigned.").await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn state_round_trips() {
        let mut game = Game::new();
        let mv = game.legal_moves().into_iter().next().expect("a move");
        game.play(&mv).expect("legal");
        let state = State {
            pearl: 1,
            game,
            plies: 1,
            last: mv.path.clone(),
        };
        let again = State::decode(&state.encode()).expect("decodes");
        assert_eq!(
            (again.pearl, again.plies, again.last.clone(), again.mover()),
            (1, 1, mv.path, 0),
            "after one ply it is player 2's turn, and the guest has the pearls"
        );
        assert_eq!(again.game, state.game);
        let fresh = State::decode(&initial_fields()).expect("the first state decodes");
        assert_eq!((fresh.plies, fresh.game.turn, fresh.last.is_empty()), (0, 1, true));
        let start = Game::new().cells_text();
        for bad in [
            "",
            "2:x",
            &format!("0:{start}:1:0:0:99"),
            &format!("0:{start}:9:0:0:-"),
            &format!("0:{start}:1:0:x:-"),
        ] {
            assert!(State::decode(bad).is_none(), "{bad:?}");
        }
    }

    #[test]
    fn pearls_are_player_one() {
        let state = State {
            pearl: 1,
            game: Game::new(),
            plies: 0,
            last: Vec::new(),
        };
        assert_eq!((state.player(1), state.player(2), state.mover()), (1, 0, 1));
    }
}
