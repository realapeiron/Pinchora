//! `/duel connect4`: seven columns of buttons under a pixel-art board.

use std::str::Split;

use chrono::{DateTime, Duration, Utc};
use pinchora_db::{GameSession, Payout};
use pinchora_game::play::{
    Game,
    connect4::{Board, COLS},
};
use pinchora_render::{Connect4Card, Theme};
use twilight_model::{
    channel::message::{
        Component,
        component::{ActionRow, ButtonStyle},
    },
    http::interaction::InteractionResponseData,
};
use twilight_util::builder::embed::EmbedBuilder;

use super::{
    duel::{Kind, TURN_SECS, answer, expire_here},
    games::{button, pack, pay_win, quiet, record, unpack},
};
use crate::{
    App, Cx,
    util::{BRAND, mention, relative, thousands},
};

/// The state's own fields: the board, whose turn it is (1 or 2) and the cell dropped last.
struct State {
    board: Board,
    turn: u8,
    last: Option<usize>,
}

impl State {
    fn new() -> Self {
        Self {
            board: Board::new(),
            turn: 1,
            last: None,
        }
    }

    fn encode(&self) -> String {
        let last = self.last.map_or_else(|| "-".to_owned(), |l| l.to_string());
        format!("{}:{}:{last}", self.board.encode(), self.turn)
    }

    fn decode(fields: &str) -> Option<Self> {
        let mut parts = fields.split(':');
        let board = Board::decode(parts.next()?)?;
        let turn = parts.next()?.parse().ok().filter(|t| (1..=2).contains(t))?;
        let last = parts.next()?.parse().ok();
        Some(Self { board, turn, last })
    }
}

pub(crate) fn initial_fields() -> String {
    State::new().encode()
}

/// Who still had to move when the game timed out.
pub(crate) fn slow_players(session: &GameSession) -> Vec<u64> {
    let (fields, _) = unpack(&session.state);
    match State::decode(fields) {
        Some(state) if state.turn == 1 => vec![session.host_id as u64],
        Some(_) => session.guest_id.map(|g| g as u64).into_iter().collect(),
        None => Vec::new(),
    }
}

enum End {
    Won(u8, [usize; 4]),
    Draw,
}

/// The board message: the picture, a line on whose turn it is and the column buttons.
async fn message(
    app: &App,
    session: &GameSession,
    names: &[String; 2],
    state: &State,
    end: Option<&End>,
    deadline: DateTime<Utc>,
    note: &str,
) -> anyhow::Result<InteractionResponseData> {
    let (host, guest) = (session.host_id as u64, session.guest_id.unwrap_or(0) as u64);
    let players = [host, guest];
    let name_of = |n: u8| names[usize::from(n) - 1].clone();
    let (content, status, turn, winner, win_cells) = match end {
        None => (
            format!(
                "🔴 {} vs 🔵 {}\n{}'s turn, move {}",
                mention(host),
                mention(guest),
                mention(players[usize::from(state.turn) - 1]),
                relative(deadline)
            ),
            format!("{}'s move", name_of(state.turn)),
            Some(state.turn),
            None,
            Vec::new(),
        ),
        Some(End::Won(who, line)) => (
            format!("🏆 {} wins! {note}", mention(players[usize::from(*who) - 1])),
            format!("{} wins!", name_of(*who)),
            None,
            Some(*who),
            line.to_vec(),
        ),
        Some(End::Draw) => (
            format!("🤝 It's a draw! {note}"),
            "A draw!".to_owned(),
            None,
            None,
            Vec::new(),
        ),
    };
    let pot = if session.bet > 0 { session.bet * 2 } else { 0 };
    let cells = state.board.cells().to_vec();
    let (names, last) = (names.clone(), state.last);
    let png = app
        .render(move |r| {
            r.connect4_card(&Connect4Card {
                cells: &cells,
                names: [&names[0], &names[1]],
                turn,
                winner,
                win_cells: &win_cells,
                last,
                status: &status,
                pot,
                theme: Theme::reef(),
            })
        })
        .await?;
    let mut embed = EmbedBuilder::new().color(BRAND).title("Connect four");
    if session.bet > 0 {
        embed = embed.description(format!(
            "Pot: **{}** shells ({} each)",
            thousands(pot),
            thousands(session.bet)
        ));
    }
    let components = if end.is_some() {
        Vec::new()
    } else {
        column_rows(session.id, &state.board)
    };
    let mut data = Cx::card(embed, "connect4.png", png, components, false);
    data.content = Some(content);
    data.allowed_mentions = quiet();
    Ok(data)
}

/// Buttons 1 to 4 on one row and 5 to 7 on the next; a full column is disabled.
fn column_rows(id: i64, board: &Board) -> Vec<Component> {
    (0..COLS)
        .collect::<Vec<_>>()
        .chunks(4)
        .map(|cols| {
            Component::ActionRow(ActionRow {
                id: None,
                components: cols
                    .iter()
                    .map(|&c| {
                        button(
                            format!("c4:m:{id}:{c}"),
                            &(c + 1).to_string(),
                            ButtonStyle::Secondary,
                            board.column_full(c),
                        )
                    })
                    .collect(),
            })
        })
        .collect()
}

/// The first board, shown when the challenge is accepted.
pub(crate) async fn begin(cx: &Cx, session: &GameSession) -> anyhow::Result<()> {
    let (fields, names) = unpack(&session.state);
    let state = State::decode(fields).ok_or_else(|| anyhow::anyhow!("bad connect four state {fields:?}"))?;
    let data = message(&cx.app, session, &names, &state, None, session.expires_at, "").await?;
    cx.update(data).await
}

/// `c4:y:<id>` and `c4:n:<id>` answer a challenge, `c4:m:<id>:<column>` drops a disc.
pub(crate) async fn on_button(cx: &Cx, mut args: Split<'_, char>) -> anyhow::Result<()> {
    let action = args.next();
    let Some(id) = args.next().and_then(|v| v.parse::<i64>().ok()) else {
        return cx.reply_ephemeral("This game has expired.").await;
    };
    match action {
        Some("y") => answer(cx, Kind::Connect4, id, true).await,
        Some("n") => answer(cx, Kind::Connect4, id, false).await,
        Some("m") => match args.next().and_then(|v| v.parse::<usize>().ok()) {
            Some(col) => on_move(cx, id, col).await,
            None => cx.reply_ephemeral("This game has expired.").await,
        },
        _ => cx.reply_ephemeral("This button has expired.").await,
    }
}

async fn on_move(cx: &Cx, id: i64, col: usize) -> anyhow::Result<()> {
    let me = cx.author()?.id.get();
    // Another kind's id is not this game: its buttons pass another module's switch.
    let Some(session) = cx
        .app
        .db
        .game(id)
        .await?
        .filter(|s| s.is_active() && s.kind == Kind::Connect4.key())
    else {
        return cx.reply_ephemeral("This game is over.").await;
    };
    if !session.has_player(me) {
        return cx
            .reply_ephemeral("You're not in this game. Challenge someone with `/duel connect4`!")
            .await;
    }
    if session.expired() {
        return expire_here(cx, &session).await;
    }
    let (host, guest) = (session.host_id as u64, session.guest_id.unwrap_or(0) as u64);
    let (fields, names) = unpack(&session.state);
    let Some(mut state) = State::decode(fields) else {
        anyhow::bail!("bad connect four state {fields:?}");
    };
    let mine: u8 = if me == host { 1 } else { 2 };
    if state.turn != mine {
        return cx.reply_ephemeral("Not your turn yet.").await;
    }
    let Some(cell) = state.board.drop_disc(col, mine) else {
        return cx.reply_ephemeral("That column is full.").await;
    };
    state.last = Some(cell);
    let end = match state.board.winner() {
        Some((who, line)) => Some(End::Won(who, line)),
        None if state.board.is_full() => Some(End::Draw),
        None => None,
    };
    let Some(end) = end else {
        state.turn = 3 - mine;
        let next = pack(&state.encode(), [&names[0], &names[1]]);
        if !cx.app.db.advance_game(id, &session.state, &next, TURN_SECS).await? {
            return cx
                .reply_ephemeral("The board changed under you. Look again and pick a column.")
                .await;
        }
        let deadline = Utc::now() + Duration::seconds(TURN_SECS);
        let data = message(&cx.app, &session, &names, &state, None, deadline, "").await?;
        return cx.update(data).await;
    };

    // The last disc: end the game and pay, once.
    let payout = match end {
        End::Won(who, _) => Payout::Winner(if who == 1 { host } else { guest }),
        End::Draw => Payout::Refund,
    };
    let finished = pack(&state.encode(), [&names[0], &names[1]]);
    if cx
        .app
        .db
        .settle_game(id, Some(&session.state), &finished, payout, false)
        .await?
        .is_none()
    {
        return cx.reply_ephemeral("The board changed under you. Look again.").await;
    }
    let winner = match end {
        End::Won(who, _) => Some(if who == 1 { host } else { guest }),
        End::Draw => None,
    };
    for player in [host, guest] {
        record(&cx.app, Game::Connect4, player, winner == Some(player)).await;
    }
    let note = match winner {
        Some(_) if session.bet > 0 => format!("They take the pot of **{}** shells.", thousands(session.bet * 2)),
        Some(w) => {
            let paid = pay_win(&cx.app, w, Game::Connect4, Game::Connect4.win_reward(), "connect4_win").await?;
            if paid > 0 {
                format!("+**{paid}** shells!")
            } else {
                "(Today's reward limit for this game is reached.)".to_owned()
            }
        }
        None if session.bet > 0 => "Both wagers were given back.".to_owned(),
        None => String::new(),
    };
    let data = message(&cx.app, &session, &names, &state, Some(&end), Utc::now(), &note).await?;
    cx.update(data).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn state_round_trips() {
        let mut state = State::new();
        state.board.drop_disc(3, 1);
        state.turn = 2;
        state.last = Some(38);
        let again = State::decode(&state.encode()).expect("decodes");
        assert_eq!((again.board, again.turn, again.last), (state.board, 2, Some(38)));
        assert!(State::decode("short:1:-").is_none());
        assert!(State::decode(&format!("{}:3:-", Board::new().encode())).is_none());
        assert_eq!(State::decode(&initial_fields()).map(|s| s.turn), Some(1));
    }
}
