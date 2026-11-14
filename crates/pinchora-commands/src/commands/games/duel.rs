mod tic_tac_toe;
use tic_tac_toe::Game;
pub(crate) use tic_tac_toe::on_move;
mod kind;
pub(crate) use kind::Kind;
mod timeout;
pub(crate) use timeout::{expire_here, time_out};
#[cfg(test)]
mod tests;

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

/// How long a challenge waits to be accepted.
const CHALLENGE_SECS: i64 = 180;
/// How long a player has for one turn or one pick.
pub(crate) const TURN_SECS: i64 = 90;

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
