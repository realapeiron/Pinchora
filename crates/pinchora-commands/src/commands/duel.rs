use std::{str::Split, time::Duration};

use twilight_interactions::command::{CommandModel, CreateCommand, ResolvedUser};
use twilight_model::{
    channel::message::{
        AllowedMentions, Component,
        component::{ActionRow, Button, ButtonStyle},
    },
    http::interaction::InteractionResponseData,
};

use crate::{Cx, util::mention};

/// Shells for a win; paid at most once per pair of players per hour.
const WIN_REWARD: i64 = 30;

#[derive(CommandModel, CreateCommand)]
#[command(name = "duel", desc = "Challenge someone to a quick game", contexts = "guild")]
pub enum DuelCommand {
    #[command(name = "tictactoe")]
    TicTacToe(DuelTicTacToe),
}

#[derive(CommandModel, CreateCommand)]
#[command(name = "tictactoe", desc = "Tic-tac-toe with buttons; the winner earns shells")]
pub struct DuelTicTacToe {
    /// Your opponent
    user: ResolvedUser,
}

impl DuelCommand {
    pub async fn run(self, cx: &Cx) -> anyhow::Result<()> {
        let Self::TicTacToe(c) = self;
        let me = cx.author()?.id.get();
        let them = &c.user.resolved;
        if them.bot || them.id.get() == me {
            return cx.reply_ephemeral("Pick a human opponent!").await;
        }
        let game = Game {
            x: me,
            o: them.id.get(),
            board: [b'.'; 9],
            x_turn: true,
        };
        cx.reply(game.message(&format!(
            "❌ {} vs ⭕ {}, {} goes first!",
            mention(me),
            mention(them.id),
            mention(me)
        )))
        .await
    }
}

/// The whole game rides in the buttons: `tt:<x>:<o>:<board>:<turn>:<cell>`.
struct Game {
    x: u64,
    o: u64,
    board: [u8; 9],
    x_turn: bool,
}

const LINES: [[usize; 3]; 8] = [
    [0, 1, 2],
    [3, 4, 5],
    [6, 7, 8],
    [0, 3, 6],
    [1, 4, 7],
    [2, 5, 8],
    [0, 4, 8],
    [2, 4, 6],
];

impl Game {
    fn winner(&self) -> Option<(u8, [usize; 3])> {
        LINES
            .iter()
            .find(|l| self.board[l[0]] != b'.' && l.iter().all(|&i| self.board[i] == self.board[l[0]]))
            .map(|l| (self.board[l[0]], *l))
    }

    fn full(&self) -> bool {
        self.board.iter().all(|&c| c != b'.')
    }

    fn message(&self, text: &str) -> InteractionResponseData {
        let over = self.winner().is_some() || self.full();
        let line = self.winner().map(|(_, l)| l);
        let board = String::from_utf8_lossy(&self.board).into_owned();
        let rows = (0..3)
            .map(|r| {
                Component::ActionRow(ActionRow {
                    id: None,
                    components: (0..3)
                        .map(|c| {
                            let i = r * 3 + c;
                            let (label, style) = match self.board[i] {
                                b'x' => ("❌", ButtonStyle::Danger),
                                b'o' => ("⭕", ButtonStyle::Primary),
                                _ => ("·", ButtonStyle::Secondary),
                            };
                            let style = if line.is_some_and(|l| l.contains(&i)) {
                                ButtonStyle::Success
                            } else {
                                style
                            };
                            Component::Button(Button {
                                custom_id: Some(format!(
                                    "tt:{}:{}:{board}:{}:{i}",
                                    self.x,
                                    self.o,
                                    if self.x_turn { 'x' } else { 'o' }
                                )),
                                disabled: over || self.board[i] != b'.',
                                emoji: None,
                                id: None,
                                label: Some(label.to_owned()),
                                sku_id: None,
                                style,
                                url: None,
                            })
                        })
                        .collect(),
                })
            })
            .collect();
        InteractionResponseData {
            content: Some(text.to_owned()),
            components: Some(rows),
            allowed_mentions: Some(AllowedMentions::default()),
            ..Default::default()
        }
    }
}

pub(crate) async fn on_move(cx: &Cx, mut args: Split<'_, char>) -> anyhow::Result<()> {
    let parsed = (|| {
        let x: u64 = args.next()?.parse().ok()?;
        let o: u64 = args.next()?.parse().ok()?;
        let board: [u8; 9] = args.next()?.as_bytes().try_into().ok()?;
        let x_turn = args.next()? == "x";
        let cell: usize = args.next()?.parse().ok()?;
        Some((Game { x, o, board, x_turn }, cell))
    })();
    let Some((mut game, cell)) = parsed else {
        return cx.reply_ephemeral("This game has expired.").await;
    };
    let player = cx.author()?.id.get();
    let expected = if game.x_turn { game.x } else { game.o };
    if player != game.x && player != game.o {
        return cx
            .reply_ephemeral("You're not in this game. Start your own with `/duel tictactoe`!")
            .await;
    }
    if player != expected {
        return cx.reply_ephemeral("Not your turn yet.").await;
    }
    if cell > 8 || game.board[cell] != b'.' || game.winner().is_some() {
        return cx.reply_ephemeral("That square is taken.").await;
    }
    game.board[cell] = if game.x_turn { b'x' } else { b'o' };
    let text = if let Some((mark, _)) = game.winner() {
        let (winner, loser) = if mark == b'x' {
            (game.x, game.o)
        } else {
            (game.o, game.x)
        };
        let pair = format!("ttt:{}:{}", winner.min(loser), winner.max(loser));
        let paid = cx
            .app
            .cache
            .try_cooldown(&pair, Duration::from_secs(3600))
            .await?
            .is_none();
        if paid {
            cx.app.db.earn_shells(winner, WIN_REWARD, "duel_win").await?;
            format!("🏆 {} wins and earns **{WIN_REWARD}** shells!", mention(winner))
        } else {
            format!("🏆 {} wins! (Rewards for this pair reset hourly.)", mention(winner))
        }
    } else if game.full() {
        "🤝 It's a draw!".to_owned()
    } else {
        game.x_turn = !game.x_turn;
        let next = if game.x_turn { game.x } else { game.o };
        format!("{}'s turn ({})", mention(next), if game.x_turn { "❌" } else { "⭕" })
    };
    cx.update(game.message(&text)).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_wins_and_draws() {
        let g = |b: &[u8; 9]| Game {
            x: 1,
            o: 2,
            board: *b,
            x_turn: true,
        };
        assert_eq!(g(b"xxx.o.o..").winner().map(|w| w.0), Some(b'x'));
        assert_eq!(g(b"o.xo.xo..").winner().map(|w| w.0), Some(b'o'));
        let draw = g(b"xoxxoooxx");
        assert!(draw.winner().is_none() && draw.full());
    }
}
