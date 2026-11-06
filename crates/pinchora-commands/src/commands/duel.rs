use std::{str::Split, sync::Arc, time::Duration};

use pinchora_db::{ACTIVE, DuelStart, GameSession, NewGame, PENDING, Payout};
use pinchora_game::play::{Game as Play, MAX_BET, MIN_BET};
use twilight_interactions::command::{CommandModel, CommandOption, CreateCommand, CreateOption, ResolvedUser};
use twilight_model::{
    channel::message::{
        AllowedMentions, Component,
        component::{ActionRow, Button, ButtonStyle},
    },
    http::interaction::InteractionResponseData,
};

use super::{
    board::{self, checkers, chess, fleet},
    connect4,
    games::{button, channel_of, edit_message, location, pack, record, track_message, unpack},
    rps,
};
use crate::{
    App, Cx,
    util::{mention, relative, resolved_display_name, thousands},
};

/// Shells for a win; paid at most once per pair of players per hour.
const WIN_REWARD: i64 = 30;

#[derive(CommandModel, CreateCommand)]
#[command(name = "duel", desc = "Challenge someone to a quick game", contexts = "guild")]
pub enum DuelCommand {
    #[command(name = "tictactoe")]
    TicTacToe(DuelTicTacToe),
    #[command(name = "connect4")]
    Connect4(DuelConnect4),
    #[command(name = "rps")]
    Rps(DuelRps),
    #[command(name = "seafleet")]
    SeaFleet(DuelSeaFleet),
    #[command(name = "checkers")]
    Checkers(DuelCheckers),
    #[command(name = "chess")]
    Chess(DuelChess),
}

#[derive(CommandModel, CreateCommand)]
#[command(
    name = "seafleet",
    desc = "Sea Fleet: hide your ships, sink theirs; the winner earns shells, or the pot"
)]
pub struct DuelSeaFleet {
    /// Your opponent
    user: ResolvedUser,
    /// Shells each of you puts up (10-500); leave empty to play for fun
    #[command(min_value = 10, max_value = 500)]
    bet: Option<i64>,
}

#[derive(CommandModel, CreateCommand)]
#[command(
    name = "checkers",
    desc = "Pearl Checkers on a pixel board; the winner earns shells, or the pot"
)]
pub struct DuelCheckers {
    /// Your opponent
    user: ResolvedUser,
    /// Shells each of you puts up (10-500); leave empty to play for fun
    #[command(min_value = 10, max_value = 500)]
    bet: Option<i64>,
}

#[derive(CommandModel, CreateCommand)]
#[command(
    name = "chess",
    desc = "Reef Chess on a pixel board; the winner earns shells, or the pot"
)]
pub struct DuelChess {
    /// Your opponent
    user: ResolvedUser,
    /// Shells each of you puts up (10-500); leave empty to play for fun
    #[command(min_value = 10, max_value = 500)]
    bet: Option<i64>,
}

#[derive(CommandModel, CreateCommand)]
#[command(
    name = "connect4",
    desc = "Connect four on a pixel board; the winner earns shells, or the pot"
)]
pub struct DuelConnect4 {
    /// Your opponent
    user: ResolvedUser,
    /// Shells each of you puts up (10-500); leave empty to play for fun
    #[command(min_value = 10, max_value = 500)]
    bet: Option<i64>,
}

#[derive(CommandModel, CreateCommand)]
#[command(
    name = "rps",
    desc = "Rock paper scissors, picked in secret; the winner earns shells, or the pot"
)]
pub struct DuelRps {
    /// Your opponent
    user: ResolvedUser,
    /// How many rounds the match is (default: best of 3)
    rounds: Option<Rounds>,
    /// Shells each of you puts up (10-500); leave empty to play for fun
    #[command(min_value = 10, max_value = 500)]
    bet: Option<i64>,
}

#[derive(CommandOption, CreateOption, Clone, Copy, Debug, PartialEq, Eq)]
pub enum Rounds {
    #[option(name = "Best of 1", value = 1)]
    One,
    #[option(name = "Best of 3", value = 3)]
    Three,
    #[option(name = "Best of 5", value = 5)]
    Five,
}

impl Rounds {
    fn count(self) -> u8 {
        match self {
            Self::One => 1,
            Self::Three => 3,
            Self::Five => 5,
        }
    }
}

#[derive(CommandModel, CreateCommand)]
#[command(name = "tictactoe", desc = "Tic-tac-toe with buttons; the winner earns shells")]
pub struct DuelTicTacToe {
    /// Your opponent
    user: ResolvedUser,
}

impl DuelCommand {
    pub async fn run(self, cx: &Cx) -> anyhow::Result<()> {
        match self {
            Self::TicTacToe(c) => c.run(cx).await,
            Self::Connect4(c) => challenge(cx, Kind::Connect4, &c.user, 0, c.bet).await,
            Self::Rps(c) => challenge(cx, Kind::Rps, &c.user, c.rounds.map_or(3, Rounds::count), c.bet).await,
            Self::SeaFleet(c) => challenge(cx, Kind::SeaFleet, &c.user, 0, c.bet).await,
            Self::Checkers(c) => challenge(cx, Kind::Checkers, &c.user, 0, c.bet).await,
            Self::Chess(c) => challenge(cx, Kind::Chess, &c.user, 0, c.bet).await,
        }
    }
}

impl DuelTicTacToe {
    async fn run(self, cx: &Cx) -> anyhow::Result<()> {
        let c = self;
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
            format!("🏆 {} wins and earns **{WIN_REWARD}** shells!", mention(winner))
        } else {
            format!("🏆 {} wins! (Rewards for this pair reset hourly.)", mention(winner))
        }
    } else if game.full() {
        finished = Some(None);
        "🤝 It's a draw!".to_owned()
    } else {
        game.x_turn = !game.x_turn;
        let next = if game.x_turn { game.x } else { game.o };
        format!("{}'s turn ({})", mention(next), if game.x_turn { "❌" } else { "⭕" })
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

/// How long a challenge waits to be accepted.
const CHALLENGE_SECS: i64 = 180;
/// How long a player has for one turn or one pick.
pub(crate) const TURN_SECS: i64 = 90;

/// The duels that keep their state in the database, with a challenge to accept first.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Kind {
    Connect4,
    Rps,
    SeaFleet,
    Checkers,
    Chess,
}

impl Kind {
    pub(crate) fn key(self) -> &'static str {
        match self {
            Self::Connect4 => "connect4",
            Self::Rps => "rps",
            Self::SeaFleet => "seafleet",
            Self::Checkers => "checkers",
            Self::Chess => "chess",
        }
    }

    pub(crate) fn from_key(key: &str) -> Option<Self> {
        match key {
            "connect4" => Some(Self::Connect4),
            "rps" => Some(Self::Rps),
            "seafleet" => Some(Self::SeaFleet),
            "checkers" => Some(Self::Checkers),
            "chess" => Some(Self::Chess),
            _ => None,
        }
    }

    /// The custom id prefix of its buttons.
    fn prefix(self) -> &'static str {
        match self {
            Self::Connect4 => "c4",
            Self::Rps => "rp",
            Self::SeaFleet => "bs",
            Self::Checkers => "ck",
            Self::Chess => "cc",
        }
    }

    /// Whether the player who runs out of time loses a wagered game. The older duels give both bets back.
    fn forfeits(self) -> bool {
        matches!(self, Self::SeaFleet | Self::Checkers | Self::Chess)
    }

    /// Seconds the first turn lasts once the challenge is accepted.
    fn first_secs(self) -> i64 {
        match self {
            Self::Connect4 | Self::Rps => TURN_SECS,
            Self::SeaFleet => board::FLEET_PLACE_SECS,
            Self::Checkers => board::CHECKERS_SECS,
            Self::Chess => board::CHESS_SECS,
        }
    }
}

/// `/duel connect4` and `/duel rps`: posts a challenge the opponent can accept or decline. Nothing is
/// held until they accept.
async fn challenge(cx: &Cx, kind: Kind, opponent: &ResolvedUser, rounds: u8, bet: Option<i64>) -> anyhow::Result<()> {
    let me = cx.author()?.id.get();
    let them = &opponent.resolved;
    if them.bot || them.id.get() == me {
        return cx.reply_ephemeral("Pick a human opponent!").await;
    }
    let bet = bet.unwrap_or(0);
    if bet != 0 && !(MIN_BET..=MAX_BET).contains(&bet) {
        return cx
            .reply_ephemeral(format!("A wager is {MIN_BET} to {MAX_BET} shells."))
            .await;
    }
    if bet > 0 {
        let balance = cx.app.db.user(me).await?.credits;
        if balance < bet {
            return cx
                .reply_ephemeral(format!(
                    "You only have **{}** shells, so you can't wager {}. Try `/daily`!",
                    thousands(balance),
                    thousands(bet)
                ))
                .await;
        }
    }
    if let Some(left) = cx
        .app
        .cache
        .try_cooldown(&format!("duelchal:{me}"), Duration::from_secs(10))
        .await?
    {
        return cx
            .reply_ephemeral(format!(
                "Easy there! You can send another challenge in {} s.",
                left.as_secs() + 1
            ))
            .await;
    }
    let (host_name, guest_name) = (cx.author_display_name(), resolved_display_name(opponent));
    let fields = if kind == Kind::Rps {
        rounds.to_string()
    } else {
        String::new()
    };
    let session = cx
        .app
        .db
        .create_game(NewGame {
            kind: kind.key(),
            guild_id: cx.guild_id().unwrap_or(0),
            channel_id: channel_of(cx),
            host: me,
            guest: Some(them.id.get()),
            bet,
            status: PENDING,
            state: &pack(&fields, [&host_name, &guest_name]),
            ttl_secs: CHALLENGE_SECS,
        })
        .await?;
    let title = match kind {
        Kind::Connect4 => "Connect four".to_owned(),
        Kind::Rps => format!("rock paper scissors, best of {rounds}"),
        Kind::SeaFleet => "Sea Fleet".to_owned(),
        Kind::Checkers => "Pearl Checkers".to_owned(),
        Kind::Chess => "Reef Chess".to_owned(),
    };
    let wager = if bet > 0 && kind.forfeits() {
        format!(
            "💰 Wager: **{}** shells each. The winner takes **{}**; a draw or a decline gives the shells back, and a player who runs out of time forfeits.",
            thousands(bet),
            thousands(bet * 2)
        )
    } else if bet > 0 {
        format!(
            "💰 Wager: **{}** shells each. The winner takes **{}**; a draw, a decline or a no-show gives the shells back.",
            thousands(bet),
            thousands(bet * 2)
        )
    } else {
        "No wager, just for fun.".to_owned()
    };
    let prefix = kind.prefix();
    cx.reply(InteractionResponseData {
        content: Some(format!(
            "⚔️ {} challenges {} to **{title}**!\n{wager}\n{}, answer {}.",
            mention(me),
            mention(them.id),
            mention(them.id),
            relative(session.expires_at)
        )),
        components: Some(vec![Component::ActionRow(ActionRow {
            id: None,
            components: vec![
                button(
                    format!("{prefix}:y:{}", session.id),
                    "Accept",
                    ButtonStyle::Success,
                    false,
                ),
                button(
                    format!("{prefix}:n:{}", session.id),
                    "Decline",
                    ButtonStyle::Danger,
                    false,
                ),
            ],
        })]),
        // Only the challenged player is pinged.
        allowed_mentions: Some(AllowedMentions {
            users: vec![them.id],
            ..Default::default()
        }),
        ..Default::default()
    })
    .await?;
    track_message(cx, session.id);
    Ok(())
}

/// A click on a challenge's Accept (`accept`) or Decline.
pub(crate) async fn answer(cx: &Cx, kind: Kind, id: i64, accept: bool) -> anyhow::Result<()> {
    let me = cx.author()?.id.get();
    let Some(session) = cx.app.db.game(id).await? else {
        return cx.reply_ephemeral("This challenge has expired.").await;
    };
    let (host, guest) = (session.host_id as u64, session.guest_id.unwrap_or(0) as u64);
    if session.status != PENDING || session.expired() {
        return cx.reply_ephemeral("This challenge is over.").await;
    }
    if !accept {
        if me != host && me != guest {
            return cx.reply_ephemeral("This challenge isn't yours.").await;
        }
        let ended = cx
            .app
            .db
            // Only the pending state: an Accept that lands first holds the bets, which must not vanish.
            .settle_game(id, Some(&session.state), "declined", Payout::Refund, false)
            .await?
            .is_some();
        if !ended {
            return cx.reply_ephemeral("This challenge is over.").await;
        }
        let text = if me == host {
            format!("🚫 {} took the challenge back.", mention(host))
        } else {
            format!("🚫 {} declined the challenge.", mention(guest))
        };
        return cx
            .update(InteractionResponseData {
                content: Some(text),
                components: Some(Vec::new()),
                allowed_mentions: Some(AllowedMentions::default()),
                ..Default::default()
            })
            .await;
    }
    if me != guest {
        return cx
            .reply_ephemeral(if me == host {
                "You sent this challenge. Wait for it to be accepted, or decline to take it back."
            } else {
                "This challenge isn't for you. Start your own with `/duel`!"
            })
            .await;
    }
    let Some((channel, message)) = cx.clicked_message() else {
        anyhow::bail!("accepted a challenge without a message");
    };
    let (fields, names) = unpack(&session.state);
    let machine = match kind {
        Kind::Connect4 => connect4::initial_fields(),
        Kind::Rps => rps::initial_fields(fields.parse().unwrap_or(3)),
        Kind::SeaFleet => fleet::initial_fields(),
        Kind::Checkers => checkers::initial_fields(),
        Kind::Chess => chess::initial_fields(),
    };
    let packed = pack(&machine, [&names[0], &names[1]]);
    match cx
        .app
        .db
        .start_duel(id, me, (channel, message), &packed, kind.first_secs())
        .await?
    {
        DuelStart::Started(started) => match kind {
            Kind::Connect4 => connect4::begin(cx, &started).await,
            Kind::Rps => rps::begin(cx, &started).await,
            Kind::SeaFleet => fleet::begin(cx, &started).await,
            Kind::Checkers => checkers::begin(cx, &started).await,
            Kind::Chess => chess::begin(cx, &started).await,
        },
        DuelStart::Gone => cx.reply_ephemeral("This challenge is over.").await,
        DuelStart::NotInvited => cx.reply_ephemeral("This challenge isn't for you.").await,
        DuelStart::Broke { user, balance } => {
            let text = if user == me {
                format!(
                    "You only have **{}** shells, but the wager is **{}** each.",
                    thousands(balance),
                    thousands(session.bet)
                )
            } else {
                format!(
                    "{} can't cover the **{}** shell wager any more. Ask for a smaller one.",
                    mention(user),
                    thousands(session.bet)
                )
            };
            cx.reply_ephemeral(text).await
        }
    }
}

/// Ends a duel (or an unanswered challenge) whose clock ran out: bets go back and the message
/// says so. False when somebody else ended it first.
pub(crate) async fn time_out(app: &Arc<App>, session: &GameSession) -> anyhow::Result<bool> {
    let (payout, from) = timeout_settlement(session);
    let Some(before) = app.db.settle_game(session.id, from, "timeout", payout, true).await? else {
        return Ok(false);
    };
    let text = timeout_text(&before);
    if let Some((channel, message)) = location(&before) {
        edit_message(app, channel, message, Some(&text), None, Some(&[])).await;
    }
    Ok(true)
}

/// A click on a game whose clock has run out: ends it (the pot goes to whoever was not slow, or bets go
/// back) and shows that on the message the player clicked and, when the click came from a private message
/// (a placement, a confirmation), on the public board too, so its menus do not stay live.
pub(crate) async fn expire_here(cx: &Cx, session: &GameSession) -> anyhow::Result<()> {
    let (payout, from) = timeout_settlement(session);
    match cx.app.db.settle_game(session.id, from, "timeout", payout, true).await? {
        Some(before) => {
            let text = timeout_text(&before);
            let clicked = cx.interaction.message.as_ref().map(|m| m.id.get());
            if let Some((channel, message)) = location(&before)
                && clicked != Some(message)
            {
                edit_message(&cx.app, channel, message, Some(&text), None, Some(&[])).await;
            }
            cx.update(InteractionResponseData {
                content: Some(text),
                components: Some(Vec::new()),
                allowed_mentions: Some(AllowedMentions::default()),
                ..Default::default()
            })
            .await
        }
        None => cx.reply_ephemeral("This game is over.").await,
    }
}

/// What a late session pays out, and the state it must still be in for that to be right. A board game
/// with a wager goes to the player who was not slow, so it only settles if nobody has moved since
/// (otherwise the janitor looks again with the new state); every older duel gives both bets back.
fn timeout_settlement(session: &GameSession) -> (Payout, Option<&str>) {
    let kind = Kind::from_key(&session.kind);
    if session.status != ACTIVE || session.bet == 0 || !kind.is_some_and(Kind::forfeits) {
        return (Payout::Refund, None);
    }
    match slow_players(session).as_slice() {
        [slow] => {
            let other = if *slow == session.host_id as u64 {
                session.guest_id.unwrap_or(0) as u64
            } else {
                session.host_id as u64
            };
            (Payout::Winner(other), Some(session.state.as_str()))
        }
        _ => (Payout::Refund, Some(session.state.as_str())),
    }
}

/// Who still had to act when a running game timed out.
fn slow_players(session: &GameSession) -> Vec<u64> {
    match Kind::from_key(&session.kind) {
        Some(Kind::Connect4) => connect4::slow_players(session),
        Some(Kind::Rps) => rps::slow_players(session),
        Some(Kind::SeaFleet) => fleet::slow_players(session),
        Some(Kind::Checkers) => checkers::slow_players(session),
        Some(Kind::Chess) => chess::slow_players(session),
        None => Vec::new(),
    }
}

/// What a timed-out duel's message says. `session` is the session as it was before it ended.
pub(crate) fn timeout_text(session: &GameSession) -> String {
    let guest = session.guest_id.unwrap_or(0);
    if session.status == PENDING {
        return format!("⌛ {} didn't answer in time, so the challenge expired.", mention(guest));
    }
    let slow = slow_players(session);
    let who = match slow.as_slice() {
        [] => "Nobody".to_owned(),
        [one] => mention(one),
        many => many.iter().map(mention).collect::<Vec<_>>().join(" and "),
    };
    if session.bet > 0
        && Kind::from_key(&session.kind).is_some_and(Kind::forfeits)
        && let [slow] = slow.as_slice()
    {
        let other = if *slow == session.host_id as u64 {
            guest as u64
        } else {
            session.host_id as u64
        };
        return format!(
            "⌛ {who} ran out of time and forfeits, so {} takes the pot of **{}** shells.",
            mention(other),
            thousands(session.bet * 2)
        );
    }
    let refund = if session.bet > 0 {
        " Both wagers were given back."
    } else {
        ""
    };
    format!("⌛ {who} ran out of time, so the game is over.{refund}")
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
