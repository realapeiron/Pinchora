use super::*;

/// The whole game rides in the buttons: `tt:<x>:<o>:<board>:<turn>:<cell>`.
pub(super) struct Game {
    pub(super) x: u64,
    pub(super) o: u64,
    pub(super) board: [u8; 9],
    pub(super) x_turn: bool,
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
    pub(super) fn winner(&self) -> Option<(u8, [usize; 3])> {
        LINES
            .iter()
            .find(|l| self.board[l[0]] != b'.' && l.iter().all(|&i| self.board[i] == self.board[l[0]]))
            .map(|l| (self.board[l[0]], *l))
    }

    pub(super) fn full(&self) -> bool {
        self.board.iter().all(|&c| c != b'.')
    }

    pub(super) fn message(&self, text: &str) -> InteractionResponseData {
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
    let lang = cx.lang();
    // `lang` is for what only the clicker sees; the board stays in the server's language.
    let board_lang = channel_lang(cx).await;
    let Some((mut game, cell)) = parsed else {
        return cx.reply_ephemeral(t!(lang, "duel.game-expired")).await;
    };
    let player = cx.author()?.id.get();
    let expected = if game.x_turn { game.x } else { game.o };
    if player != game.x && player != game.o {
        return cx.reply_ephemeral(t!(lang, "tictactoe.not-in-game")).await;
    }
    if player != expected {
        return cx.reply_ephemeral(t!(lang, "duel.not-your-turn")).await;
    }
    if cell > 8 || game.board[cell] != b'.' || game.winner().is_some() {
        return cx.reply_ephemeral(t!(lang, "tictactoe.taken")).await;
    }
    game.board[cell] = if game.x_turn { b'x' } else { b'o' };
    // Who won, once the game is over: `Some(None)` is a draw.
    let mut finished = None;
    let text = if let Some((mark, _)) = game.winner() {
        let (winner, loser) = if mark == b'x' {
            (game.x, game.o)
        } else {
            (game.o, game.x)
        };
        finished = Some(Some(winner));
        let pair = format!("ttt:{}:{}", winner.min(loser), winner.max(loser));
        let paid = cx
            .app
            .cache
            .try_cooldown(&pair, Duration::from_secs(3600))
            .await?
            .is_none();
        if paid {
            cx.app.db.earn_shells(winner, WIN_REWARD, "duel_win").await?;
            t!(
                board_lang,
                "tictactoe.win-paid",
                winner = mention(winner),
                reward = WIN_REWARD
            )
        } else {
            t!(board_lang, "tictactoe.win-resting", winner = mention(winner))
        }
    } else if game.full() {
        finished = Some(None);
        t!(board_lang, "tictactoe.draw")
    } else {
        game.x_turn = !game.x_turn;
        let next = if game.x_turn { game.x } else { game.o };
        t!(
            board_lang,
            "tictactoe.turn",
            next = mention(next),
            mark = if game.x_turn { "❌" } else { "⭕" }
        )
    };
    if let Some(winner) = finished {
        // The buttons carry the whole game, so a double click finishes it twice; the message it
        // was played on counts it once.
        let key = format!("tttstat:{}", cx.clicked_message().map_or(0, |m| m.1));
        if cx.app.cache.once(&key, Duration::from_secs(86_400)).await? {
            for player in [game.x, game.o] {
                record(&cx.app, Play::TicTacToe, player, winner == Some(player)).await;
            }
        }
    }
    cx.update(game.message(&text)).await
}
