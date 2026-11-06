//! `/duel chess`: Reef Chess. A move is two picks from selects under the board: the piece, then where it
//! goes. The game is stored as its list of moves, so any pod can pick it up.

use std::str::Split;

use chrono::{DateTime, Duration, Utc};
use pinchora_db::GameSession;
use pinchora_game::play::{
    Game as Play,
    chess::{self as rules, Color, Game, Kind, Move, Outcome},
};
use pinchora_render::{ChessCard, Theme};
use twilight_model::{channel::message::Component, http::interaction::InteractionResponseData};
use twilight_util::builder::embed::EmbedBuilder;

use super::{
    CHESS_SECS, conclude, confirm_resign, edit_card, option, players, resign_button, resigned, select_row, select_rows,
    tag,
};
use crate::{
    App, Cx,
    commands::{
        duel::{Kind as DuelKind, answer, expire_here},
        games::{pack, quiet, unpack},
    },
    util::{BRAND, SUCCESS, mention, relative, thousands},
};

/// The state's own fields: which player (0 host, 1 guest) has the first side, coral, then the moves played.
struct State {
    coral: usize,
    game: Game,
}

impl State {
    fn encode(&self) -> String {
        format!("{}:{}", self.coral, self.game.moves_text())
    }

    fn decode(fields: &str) -> Option<Self> {
        let (coral, moves) = fields.split_once(':')?;
        Some(Self {
            coral: coral.parse().ok().filter(|c| *c < 2)?,
            game: Game::from_moves(moves)?,
        })
    }

    /// The player (0 or 1) who plays `color`; coral is white.
    fn player(&self, color: Color) -> usize {
        if color == Color::White {
            self.coral
        } else {
            1 - self.coral
        }
    }

    /// The player to move.
    fn mover(&self) -> usize {
        self.player(self.game.position.turn)
    }
}

pub(crate) fn initial_fields() -> String {
    State {
        coral: rand::random_range(0..2),
        game: Game::new(),
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
        Self {
            winner: outcome.winner().map(|color| state.player(color)),
            how: outcome.describe().to_owned(),
        }
    }
}

/// What the strongest-first order of the piece menu goes by.
fn weight(kind: Kind) -> u8 {
    match kind {
        Kind::King => 5,
        Kind::Queen => 4,
        Kind::Rook => 3,
        Kind::Bishop => 2,
        Kind::Knight => 1,
        Kind::Pawn => 0,
    }
}

/// The sea creature a piece is drawn as.
fn creature(kind: Kind) -> &'static str {
    match kind {
        Kind::Pawn => "Pearl",
        Kind::Knight => "Seahorse",
        Kind::Bishop => "Conch",
        Kind::Rook => "Lighthouse",
        Kind::Queen => "Octopus queen",
        Kind::King => "Crab king",
    }
}

/// The card's code for a piece: 1 to 6 for coral's, 7 to 12 for sky's.
fn code(piece: rules::Piece) -> u8 {
    piece.kind as u8 + 1 + if piece.color == Color::White { 0 } else { 6 }
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
    let position = state.game.position;
    let moves = position.legal_moves();
    let targets: Vec<u8> = selected
        .map(|from| moves.iter().filter(|m| m.from == from).map(|m| m.to).collect())
        .unwrap_or_default();
    let pieces: [u8; 64] = std::array::from_fn(|sq| position.piece_at(sq as u8).map_or(0, code));
    let coral_name = names[state.coral].clone();
    let sky_name = names[1 - state.coral].clone();
    let in_check = position.in_check();
    let check = in_check.then(|| position.king_square(position.turn)).flatten();
    let last = state.game.moves().last().map(|m| (m.from, m.to));
    let turn = end.is_none().then(|| u8::from(position.turn == Color::Black));
    let winner = end.and_then(|e| e.winner).map(|w| u8::from(w != state.coral));
    let mover = state.mover();
    let status = match end {
        Some(End { winner: Some(w), how }) => format!("{} wins by {how}", tag(&names[*w])),
        Some(End { winner: None, how }) => format!("A draw by {how}"),
        None if in_check => format!("{} to move, in check!", tag(&names[mover])),
        None => format!("{} to move", tag(&names[mover])),
    };
    let content = match end {
        None => format!(
            "♟️ {} (coral) vs {} (sky)\n{}'s move, {}{note}",
            mention(ids[state.coral]),
            mention(ids[1 - state.coral]),
            mention(ids[mover]),
            relative(deadline)
        ),
        Some(End { winner: Some(w), how }) => format!("🏆 {} wins by {how}! {note}", mention(ids[*w])),
        Some(End { winner: None, how }) => format!("🤝 It's a draw by {how}! {note}"),
    };
    let pot = if session.bet > 0 { session.bet * 2 } else { 0 };
    let png = {
        let (status, targets) = (status.clone(), targets.clone());
        app.render(move |r| {
            r.chess_card(&ChessCard {
                pieces: &pieces,
                names: [&coral_name, &sky_name],
                turn,
                winner,
                last,
                selected,
                targets: &targets,
                check,
                status: &status,
                pot,
                theme: Theme::reef(),
            })
        })
        .await?
    };
    let mut description = format!("`{}`", state.game.recent_san(6));
    if state.game.plies() == 0 {
        description = "Coral moves first. Pick a piece, then where it goes.".to_owned();
    }
    if pot > 0 {
        description.push_str(&format!(
            "\nPot: **{}** shells ({} each)",
            thousands(pot),
            thousands(session.bet)
        ));
    }
    let embed = EmbedBuilder::new()
        .color(if end.is_some() { SUCCESS } else { BRAND })
        .title("Reef Chess")
        .description(description);
    let mut components = Vec::new();
    if end.is_none() {
        components = menus(session.id, state, &moves, selected);
    }
    let mut data = Cx::card(embed, "chess.png", png, components, false);
    data.content = Some(content);
    data.allowed_mentions = quiet();
    Ok(data)
}

/// The piece menu, the destination menu once a piece is picked, and Resign.
fn menus(id: i64, state: &State, moves: &[Move], selected: Option<u8>) -> Vec<Component> {
    let position = &state.game.position;
    let mut froms: Vec<u8> = moves.iter().map(|m| m.from).collect();
    froms.sort_unstable();
    froms.dedup();
    froms.sort_by_key(|&sq| std::cmp::Reverse(position.piece_at(sq).map_or(0, |p| weight(p.kind))));
    let plies = state.game.plies();
    let piece_options = froms
        .iter()
        .filter_map(|&sq| {
            let piece = position.piece_at(sq)?;
            let count = moves.iter().filter(|m| m.from == sq).count();
            Some(option(
                format!("{} {}", piece.kind.name(), rules::square_name(sq)),
                rules::square_name(sq),
                Some(format!(
                    "{}, {count} move{}",
                    creature(piece.kind),
                    if count == 1 { "" } else { "s" }
                )),
                selected == Some(sq),
            ))
        })
        .collect();
    let mut rows = vec![select_row(
        format!("cc:s:{id}:{plies}"),
        "Pick a piece to move",
        piece_options,
        (1, 1),
    )];
    if let Some(from) = selected
        && let Some(piece) = position.piece_at(from)
    {
        let mut here: Vec<&Move> = moves.iter().filter(|m| m.from == from).collect();
        // A pawn that reaches the last rank is offered the queen first, then the rook, bishop and knight.
        here.sort_by_key(|m| (m.to, std::cmp::Reverse(m.promotion.map_or(0, weight))));
        let options = here
            .iter()
            .map(|m| {
                let target = rules::square_name(m.to);
                let taken = position.piece_at(m.to);
                let en_passant = piece.kind == Kind::Pawn && m.from % 8 != m.to % 8 && taken.is_none();
                let label = if piece.kind == Kind::King && m.to.abs_diff(m.from) == 2 {
                    format!(
                        "{target} (castle {})",
                        if m.to > m.from { "kingside" } else { "queenside" }
                    )
                } else if let Some(promotion) = m.promotion {
                    format!("{target} (becomes a {})", promotion.name().to_lowercase())
                } else if let Some(victim) = taken {
                    format!("{target} (takes {})", victim.kind.name().to_lowercase())
                } else if en_passant {
                    format!("{target} (takes en passant)")
                } else {
                    target
                };
                option(label, m.uci(), None, false)
            })
            .collect();
        rows.extend(select_rows(
            &format!("cc:t:{id}:{plies}:{}", rules::square_name(from)),
            &format!(
                "Where should the {} on {} go?",
                piece.kind.name().to_lowercase(),
                rules::square_name(from)
            ),
            options,
            (1, 1),
        ));
    }
    rows.push(resign_button("cc", id));
    rows
}

/// The first board, shown when the challenge is accepted.
pub(crate) async fn begin(cx: &Cx, session: &GameSession) -> anyhow::Result<()> {
    let (fields, names) = unpack(&session.state);
    let state = State::decode(fields).ok_or_else(|| anyhow::anyhow!("bad chess state {fields:?}"))?;
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

/// `cc:y:<id>` and `cc:n:<id>` answer a challenge, `cc:s:<id>:<plies>` picks a piece (the value is its
/// square), `cc:t:<id>:<plies>:<from>` picks where it goes (the value is the move as `e2e4`), `cc:q:<id>`
/// asks to resign and `cc:R:<id>` confirms.
pub(crate) async fn on_component(cx: &Cx, mut args: Split<'_, char>, values: &[String]) -> anyhow::Result<()> {
    let action = args.next();
    let Some(id) = args.next().and_then(|v| v.parse::<i64>().ok()) else {
        return cx.reply_ephemeral("This game has expired.").await;
    };
    let value = values.first().map(String::as_str);
    match action {
        Some("y") => answer(cx, DuelKind::Chess, id, true).await,
        Some("n") => answer(cx, DuelKind::Chess, id, false).await,
        Some("q") => confirm_resign(cx, "cc", id).await,
        Some("R") => on_resign(cx, id).await,
        Some("s") => match (args.next().and_then(|v| v.parse::<usize>().ok()), value) {
            (Some(plies), Some(from)) => on_piece(cx, id, plies, from).await,
            _ => cx.reply_ephemeral("This menu has expired.").await,
        },
        Some("t") => match (args.next().and_then(|v| v.parse::<usize>().ok()), value) {
            (Some(plies), Some(mv)) => on_move(cx, id, plies, mv).await,
            _ => cx.reply_ephemeral("This menu has expired.").await,
        },
        _ => cx.reply_ephemeral("This button has expired.").await,
    }
}

/// A running game the clicker plays in, with the board as it stands. `None` after answering the click
/// when there is nothing to do: the game is over, the clicker is not in it, it is late, or the click came
/// from a board that has moved on (which gets the current one).
async fn running(cx: &Cx, id: i64, plies: Option<usize>) -> anyhow::Result<Option<(GameSession, [String; 2], State)>> {
    let me = cx.author()?.id.get();
    // Another kind's id is not this game: its buttons pass another module's switch.
    let Some(session) = cx
        .app
        .db
        .game(id)
        .await?
        .filter(|s| s.is_active() && s.kind == DuelKind::Chess.key())
    else {
        cx.reply_ephemeral("This game is over.").await?;
        return Ok(None);
    };
    if !session.has_player(me) {
        cx.reply_ephemeral("You're not in this game. Challenge someone with `/duel chess`!")
            .await?;
        return Ok(None);
    }
    if session.expired() {
        expire_here(cx, &session).await?;
        return Ok(None);
    }
    let (fields, names) = unpack(&session.state);
    let state = State::decode(fields).ok_or_else(|| anyhow::anyhow!("bad chess state {fields:?}"))?;
    if let Some(plies) = plies {
        if state.game.plies() != plies {
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

async fn on_piece(cx: &Cx, id: i64, plies: usize, from: &str) -> anyhow::Result<()> {
    let Some((session, names, state)) = running(cx, id, Some(plies)).await? else {
        return Ok(());
    };
    let Some(from) =
        rules::parse_square(from).filter(|&sq| state.game.position.legal_moves().iter().any(|m| m.from == sq))
    else {
        return cx.reply_ephemeral("That piece can't move.").await;
    };
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

async fn on_move(cx: &Cx, id: i64, plies: usize, text: &str) -> anyhow::Result<()> {
    let Some((session, names, state)) = running(cx, id, Some(plies)).await? else {
        return Ok(());
    };
    let mut game = state.game.clone();
    if Move::from_uci(text).and_then(|mv| game.play(mv)).is_none() {
        return cx.reply_ephemeral("That move isn't legal.").await;
    }
    let next = State {
        coral: state.coral,
        game,
    };
    let packed = pack(&next.encode(), [&names[0], &names[1]]);
    let Some(outcome) = next.game.outcome() else {
        if !cx.app.db.advance_game(id, &session.state, &packed, CHESS_SECS).await? {
            return cx
                .reply_ephemeral("The board changed under you. Look again and pick a piece.")
                .await;
        }
        let deadline = Utc::now() + Duration::seconds(CHESS_SECS);
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
    let Some(note) = conclude(&cx.app, &session, &packed, Play::Chess, winner, next.game.plies()).await? else {
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
    let plies = state.game.plies();
    let Some(note) = conclude(&cx.app, &session, &session.state, Play::Chess, Some(ids[winner]), plies).await? else {
        return resigned(cx, "The game was over before you could resign.").await;
    };
    let end = End {
        winner: Some(winner),
        how: format!("resignation ({} gave up)", tag(&names[1 - winner])),
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
    if let Some((channel, message)) = crate::commands::games::location(&session) {
        edit_card(&cx.app, channel, message, &data).await;
    }
    resigned(cx, "You resigned.").await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn state_round_trips_and_refuses_illegal_moves() {
        let mut game = Game::new();
        for text in ["e2e4", "e7e5", "g1f3"] {
            game.play(Move::from_uci(text).unwrap()).expect("legal");
        }
        let state = State { coral: 1, game };
        let text = state.encode();
        assert_eq!(text, "1:e2e4 e7e5 g1f3");
        let again = State::decode(&text).expect("decodes");
        assert_eq!((again.coral, again.game.plies(), again.mover()), (1, 3, 0));
        assert_eq!(State::decode(&initial_fields()).map(|s| s.game.plies()), Some(0));
        for bad in ["", "2:", "x:e2e4", "0:e2e5", "0:zzzz"] {
            assert!(State::decode(bad).is_none(), "{bad:?}");
        }
    }

    #[test]
    fn coral_is_white_and_the_mover_follows_the_turn() {
        let state = State {
            coral: 1,
            game: Game::new(),
        };
        assert_eq!(state.player(Color::White), 1);
        assert_eq!(state.player(Color::Black), 0);
        assert_eq!(state.mover(), 1, "the guest has coral and moves first");
    }
}
