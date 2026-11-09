use std::{str::Split, sync::Arc, time::Duration};

use pinchora_core::infra::i18n::Lang;
use pinchora_core::t;
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

use crate::App;
use crate::Cx;
use crate::commands::games::board;
use crate::commands::games::board::checkers;
use crate::commands::games::board::chess;
use crate::commands::games::board::fleet;
use crate::commands::games::board::othello;
use crate::commands::games::connect4;
use crate::commands::games::games::button;
use crate::commands::games::games::channel_lang;
use crate::commands::games::games::channel_of;
use crate::commands::games::games::edit_message;
use crate::commands::games::games::location;
use crate::commands::games::games::pack;
use crate::commands::games::games::record;
use crate::commands::games::games::track_message;
use crate::commands::games::games::unpack;
use crate::commands::games::rps;
use crate::commands::social::share;
use crate::infra::util::mention;
use crate::infra::util::relative;
use crate::infra::util::resolved_display_name;
use crate::infra::util::thousands_in;

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
    #[command(name = "share")]
    Share(DuelShare),
    #[command(name = "seafleet")]
    SeaFleet(DuelSeaFleet),
    #[command(name = "checkers")]
    Checkers(DuelCheckers),
    #[command(name = "chess")]
    Chess(DuelChess),
    #[command(name = "othello")]
    Othello(DuelOthello),
}

#[derive(CommandModel, CreateCommand)]
#[command(
    name = "othello",
    desc = "Pearl Flip on an 8 by 8 board; flip the other player's pearls"
)]
pub struct DuelOthello {
    /// Your opponent
    user: ResolvedUser,
    /// Shells each of you puts up (10-500); leave empty to play for fun
    #[command(min_value = 10, max_value = 500)]
    bet: Option<i64>,
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

#[derive(CommandModel, CreateCommand)]
#[command(
    name = "share",
    desc = "Share or Grab: split the pool, or grab it all and hope they share"
)]
pub struct DuelShare {
    /// Your opponent
    user: ResolvedUser,
    /// Shells each of you puts up (10-500); leave empty to play for the sea's pool
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
            Self::Share(c) => challenge(cx, Kind::ShareGrab, &c.user, 0, c.bet).await,
            Self::SeaFleet(c) => challenge(cx, Kind::SeaFleet, &c.user, 0, c.bet).await,
            Self::Checkers(c) => challenge(cx, Kind::Checkers, &c.user, 0, c.bet).await,
            Self::Chess(c) => challenge(cx, Kind::Chess, &c.user, 0, c.bet).await,
            Self::Othello(c) => challenge(cx, Kind::Othello, &c.user, 0, c.bet).await,
        }
    }
}

impl DuelTicTacToe {
    async fn run(self, cx: &Cx) -> anyhow::Result<()> {
        let c = self;
        let me = cx.author()?.id.get();
        let them = &c.user.resolved;
        if them.bot || them.id.get() == me {
            return cx.reply_ephemeral(t!(cx.lang(), "duel.pick-human")).await;
        }
        let game = Game {
            x: me,
            o: them.id.get(),
            board: [b'.'; 9],
            x_turn: true,
        };
        // The board is public: the server's language.
        let text = t!(
            channel_lang(cx).await,
            "tictactoe.start",
            x = mention(me),
            o = mention(them.id),
            first = mention(me)
        );
        cx.reply(game.message(&text)).await
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

/// How long a challenge waits to be accepted.
const CHALLENGE_SECS: i64 = 180;
/// How long a player has for one turn or one pick.
pub(crate) const TURN_SECS: i64 = 90;

/// The duels that keep their state in the database, with a challenge to accept first.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Kind {
    Connect4,
    Rps,
    ShareGrab,
    SeaFleet,
    Checkers,
    Chess,
    Othello,
}

impl Kind {
    pub(crate) fn key(self) -> &'static str {
        match self {
            Self::Connect4 => "connect4",
            Self::Rps => "rps",
            Self::ShareGrab => "sharegrab",
            Self::SeaFleet => "seafleet",
            Self::Checkers => "checkers",
            Self::Chess => "chess",
            Self::Othello => "othello",
        }
    }

    pub(crate) fn from_key(key: &str) -> Option<Self> {
        match key {
            "connect4" => Some(Self::Connect4),
            "rps" => Some(Self::Rps),
            "sharegrab" => Some(Self::ShareGrab),
            "seafleet" => Some(Self::SeaFleet),
            "checkers" => Some(Self::Checkers),
            "chess" => Some(Self::Chess),
            "othello" => Some(Self::Othello),
            _ => None,
        }
    }

    /// The game it keeps the records of, which also names it.
    fn game(self) -> Play {
        match self {
            Self::Connect4 => Play::Connect4,
            Self::Rps => Play::Rps,
            Self::ShareGrab => Play::ShareGrab,
            Self::SeaFleet => Play::SeaFleet,
            Self::Checkers => Play::Checkers,
            Self::Chess => Play::Chess,
            Self::Othello => Play::Othello,
        }
    }

    /// The custom id prefix of its buttons.
    fn prefix(self) -> &'static str {
        match self {
            Self::Connect4 => "c4",
            Self::Rps => "rp",
            Self::ShareGrab => "sh",
            Self::SeaFleet => "bs",
            Self::Checkers => "ck",
            Self::Chess => "cc",
            Self::Othello => "of",
        }
    }

    /// Whether the player who runs out of time loses a wagered game (the other takes the pot), so letting the
    /// clock run cannot cancel a bet. Share or Grab settles its own no-shows.
    fn forfeits(self) -> bool {
        !matches!(self, Self::ShareGrab)
    }

    /// How long a player has for the first turn or pick once the challenge is accepted.
    fn first_secs(self) -> i64 {
        match self {
            Self::Connect4 | Self::Rps => TURN_SECS,
            Self::ShareGrab => pinchora_game::party::share::PICK_SECS,
            Self::SeaFleet => board::FLEET_PLACE_SECS,
            Self::Checkers | Self::Othello => board::CHECKERS_SECS,
            Self::Chess => board::CHESS_SECS,
        }
    }
}

/// `/duel connect4` and `/duel rps`: posts a challenge the opponent can accept or decline. Nothing is
/// held until they accept.
async fn challenge(cx: &Cx, kind: Kind, opponent: &ResolvedUser, rounds: u8, bet: Option<i64>) -> anyhow::Result<()> {
    let me = cx.author()?.id.get();
    let lang = cx.lang();
    let them = &opponent.resolved;
    if them.bot || them.id.get() == me {
        return cx.reply_ephemeral(t!(lang, "duel.pick-human")).await;
    }
    let bet = bet.unwrap_or(0);
    if bet != 0 && !(MIN_BET..=MAX_BET).contains(&bet) {
        return cx
            .reply_ephemeral(t!(lang, "duel.bet-range", min = MIN_BET, max = MAX_BET))
            .await;
    }
    if bet > 0 {
        let balance = cx.app.db.user(me).await?.credits;
        if balance < bet {
            return cx
                .reply_ephemeral(t!(
                    lang,
                    "duel.cant-wager",
                    balance = thousands_in(lang, balance),
                    bet = thousands_in(lang, bet)
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
            .reply_ephemeral(t!(lang, "duel.cooldown", secs = left.as_secs() + 1))
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
    // The challenge is public: the server's language, whoever reads or clicks it.
    let board_lang = channel_lang(cx).await;
    let title = match kind {
        Kind::Rps => t!(board_lang, "duel.title-rps", rounds = rounds),
        other => other.game().name(board_lang),
    };
    let wager = match (kind, bet > 0) {
        (Kind::ShareGrab, true) => t!(
            board_lang,
            "duel.wager-share",
            stake = thousands_in(lang, bet),
            pot = thousands_in(lang, bet * 2)
        ),
        (Kind::ShareGrab, false) => t!(
            board_lang,
            "duel.wager-share-free",
            pool = pinchora_game::party::share::POOL
        ),
        (_, true) => t!(
            board_lang,
            "duel.wager",
            bet = thousands_in(lang, bet),
            pot = thousands_in(lang, bet * 2)
        ),
        (_, false) => t!(board_lang, "duel.no-wager"),
    };
    let prefix = kind.prefix();
    cx.reply(InteractionResponseData {
        content: Some(t!(
            board_lang,
            "duel.challenge",
            host = mention(me),
            guest = mention(them.id),
            title = title,
            wager = wager,
            when = relative(session.expires_at)
        )),
        components: Some(vec![Component::ActionRow(ActionRow {
            id: None,
            components: vec![
                button(
                    format!("{prefix}:y:{}", session.id),
                    &t!(board_lang, "duel.accept"),
                    ButtonStyle::Success,
                    false,
                ),
                button(
                    format!("{prefix}:n:{}", session.id),
                    &t!(board_lang, "duel.decline"),
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
    let lang = cx.lang();
    // Another kind's id is not this challenge: its buttons pass another module's switch.
    let Some(session) = cx.app.db.game(id).await?.filter(|s| s.kind == kind.key()) else {
        return cx.reply_ephemeral(t!(lang, "duel.challenge-expired")).await;
    };
    let (host, guest) = (session.host_id as u64, session.guest_id.unwrap_or(0) as u64);
    if session.status != PENDING || session.expired() {
        return cx.reply_ephemeral(t!(lang, "duel.challenge-over")).await;
    }
    if !accept {
        if me != host && me != guest {
            return cx.reply_ephemeral(t!(lang, "duel.not-yours")).await;
        }
        let ended = cx
            .app
            .db
            // Only the pending state: an Accept that lands first holds the bets, which must not vanish.
            .settle_game(id, Some(&session.state), "declined", Payout::Refund, false)
            .await?
            .is_some();
        if !ended {
            return cx.reply_ephemeral(t!(lang, "duel.challenge-over")).await;
        }
        // The challenge message everybody reads: the server's language.
        let board_lang = channel_lang(cx).await;
        let text = if me == host {
            t!(board_lang, "duel.took-back", host = mention(host))
        } else {
            t!(board_lang, "duel.declined", guest = mention(guest))
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
                t!(lang, "duel.own-challenge")
            } else {
                t!(lang, "duel.not-for-you-start")
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
        Kind::ShareGrab => share::initial_fields(),
        Kind::SeaFleet => fleet::initial_fields(),
        Kind::Checkers => checkers::initial_fields(),
        Kind::Chess => chess::initial_fields(),
        Kind::Othello => othello::initial_fields(),
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
            Kind::ShareGrab => share::begin(cx, &started).await,
            Kind::SeaFleet => fleet::begin(cx, &started).await,
            Kind::Checkers => checkers::begin(cx, &started).await,
            Kind::Chess => chess::begin(cx, &started).await,
            Kind::Othello => othello::begin(cx, &started).await,
        },
        DuelStart::Gone => cx.reply_ephemeral(t!(lang, "duel.challenge-over")).await,
        DuelStart::NotInvited => cx.reply_ephemeral(t!(lang, "duel.not-for-you")).await,
        DuelStart::Broke { user, balance } => {
            let text = if user == me {
                t!(
                    lang,
                    "duel.broke-self",
                    balance = thousands_in(lang, balance),
                    bet = thousands_in(lang, session.bet)
                )
            } else {
                t!(
                    lang,
                    "duel.broke-other",
                    user = mention(user),
                    bet = thousands_in(lang, session.bet)
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
    record_othello_timeout(app, &before).await;
    // Nobody clicked: the message is in the server's language.
    let text = timeout_text(&before, app.guild_lang(before.guild_id as u64).await);
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
            record_othello_timeout(&cx.app, &before).await;
            let clicked = cx.interaction.message.as_ref().map(|m| m.id.get());
            // The public message is the server's language whoever clicked it; a private one is the clicker's.
            let board = timeout_text(&before, cx.app.guild_lang(before.guild_id as u64).await);
            let spot = location(&before);
            let text = if spot.is_some_and(|(_, message)| clicked == Some(message)) {
                board.clone()
            } else {
                timeout_text(&before, cx.lang())
            };
            if let Some((channel, message)) = spot
                && clicked != Some(message)
            {
                edit_message(&cx.app, channel, message, Some(&board), None, Some(&[])).await;
            }
            cx.update(InteractionResponseData {
                content: Some(text),
                components: Some(Vec::new()),
                allowed_mentions: Some(AllowedMentions::default()),
                ..Default::default()
            })
            .await
        }
        None => cx.reply_ephemeral(t!(cx.lang(), "duel.game-over")).await,
    }
}

async fn record_othello_timeout(app: &App, session: &GameSession) {
    if session.kind != Kind::Othello.key() || session.status != ACTIVE {
        return;
    }
    let slow = slow_players(session);
    for user in [session.host_id as u64, session.guest_id.unwrap_or(0) as u64] {
        record(app, Play::Othello, user, slow.len() == 1 && slow[0] != user).await;
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
        Some(Kind::ShareGrab) => share::slow_players(session),
        Some(Kind::SeaFleet) => fleet::slow_players(session),
        Some(Kind::Checkers) => checkers::slow_players(session),
        Some(Kind::Chess) => chess::slow_players(session),
        Some(Kind::Othello) => othello::slow_players(session),
        None => Vec::new(),
    }
}

/// What a timed-out duel's message says. `session` is the session as it was before it ended.
pub(crate) fn timeout_text(session: &GameSession, lang: Lang) -> String {
    let guest = session.guest_id.unwrap_or(0);
    if session.status == PENDING {
        return t!(lang, "duel.timeout-challenge", guest = mention(guest));
    }
    let slow = slow_players(session);
    let who = match slow.as_slice() {
        [] => t!(lang, "duel.nobody"),
        [one] => mention(one),
        many => many.iter().map(mention).collect::<Vec<_>>().join(&t!(lang, "duel.and")),
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
        return t!(
            lang,
            "duel.timeout-forfeit",
            who = who,
            winner = mention(other),
            pot = thousands_in(lang, session.bet * 2)
        );
    }
    if session.bet > 0 {
        t!(lang, "duel.timeout-over-refund", who = who)
    } else {
        t!(lang, "duel.timeout-over", who = who)
    }
}

#[cfg(test)]
mod tests {
    use crate::commands::games::duel::*;

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

    #[test]
    fn the_wager_limits_are_told_in_both_languages() {
        assert_eq!(
            t!(Lang::En, "duel.bet-range", min = MIN_BET, max = MAX_BET),
            "A wager is 10 to 500 shells."
        );
        assert_eq!(
            t!(Lang::Tr, "duel.bet-range", min = MIN_BET, max = MAX_BET),
            "Bahis 10 ile 500 kabuk arasında olmalı."
        );
    }
}
