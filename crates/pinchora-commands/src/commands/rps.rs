//! `/duel rps`: both players pick in secret, the public message reveals each round.

use std::str::Split;

use chrono::{DateTime, Duration, Utc};
use pinchora_core::{i18n::Lang, t};
use pinchora_db::{GameSession, Payout};
use pinchora_game::play::{
    Game,
    rps::{Move, Round, round, wins_needed},
};
use twilight_model::{
    channel::message::{
        Component, Embed,
        component::{ActionRow, ButtonStyle},
    },
    http::interaction::InteractionResponseData,
};
use twilight_util::builder::embed::EmbedBuilder;

use super::{
    duel::{Kind, TURN_SECS, answer, expire_here},
    games::{button, button_row, edit_message, location, pack, pay_win, quiet, record, unpack},
};
use crate::{
    Cx,
    util::{BRAND, SUCCESS, mention, relative, thousands},
};

/// Rounds shown in the history under the score.
const SHOWN_ROUNDS: usize = 5;
/// Rounds kept in the state; ties can make a match run long.
const KEPT_ROUNDS: usize = 12;

/// The state's own fields: `rounds:score_host:score_guest:round:pick_host:pick_guest:history`. A pick
/// is `r`, `p`, `s` or `-`; the history is two letters (host, guest) per finished round.
struct State {
    rounds: u8,
    score: [u8; 2],
    round: u8,
    picks: [Option<Move>; 2],
    history: Vec<(Move, Move)>,
}

impl State {
    fn new(rounds: u8) -> Self {
        Self {
            rounds,
            score: [0, 0],
            round: 1,
            picks: [None, None],
            history: Vec::new(),
        }
    }

    fn encode(&self) -> String {
        let pick = |p: Option<Move>| p.map_or('-', Move::key);
        let history: String = self.history.iter().flat_map(|(a, b)| [a.key(), b.key()]).collect();
        format!(
            "{}:{}:{}:{}:{}:{}:{history}",
            self.rounds,
            self.score[0],
            self.score[1],
            self.round,
            pick(self.picks[0]),
            pick(self.picks[1])
        )
    }

    fn decode(fields: &str) -> Option<Self> {
        let mut parts = fields.split(':');
        let rounds: u8 = parts.next()?.parse().ok().filter(|r| [1, 3, 5].contains(r))?;
        let score = [parts.next()?.parse().ok()?, parts.next()?.parse().ok()?];
        let round = parts.next()?.parse().ok()?;
        let pick = |p: &str| match p {
            "-" => Some(None),
            other => other.chars().next().and_then(Move::from_key).map(Some),
        };
        let picks = [pick(parts.next()?)?, pick(parts.next()?)?];
        let letters: Vec<Move> = parts.next()?.chars().map(Move::from_key).collect::<Option<_>>()?;
        if !letters.len().is_multiple_of(2) {
            return None;
        }
        let history = letters.chunks(2).map(|c| (c[0], c[1])).collect();
        Some(Self {
            rounds,
            score,
            round,
            picks,
            history,
        })
    }
}

pub(crate) fn initial_fields(rounds: u8) -> String {
    State::new(if [1, 3, 5].contains(&rounds) { rounds } else { 3 }).encode()
}

/// Who had not picked yet when the match timed out.
pub(crate) fn slow_players(session: &GameSession) -> Vec<u64> {
    let (fields, _) = unpack(&session.state);
    let Some(state) = State::decode(fields) else {
        return Vec::new();
    };
    let players = [Some(session.host_id as u64), session.guest_id.map(|g| g as u64)];
    (0..2)
        .filter(|&i| state.picks[i].is_none())
        .filter_map(|i| players[i])
        .collect()
}

/// What the winner got for the match.
#[derive(Clone, Copy)]
enum Reward {
    /// The pot of a wagered match.
    Pot,
    /// Shells the sea paid.
    Paid(i64),
    /// The game's daily limit was reached.
    Limit,
}

/// How a match ended: who won (0 host, 1 guest).
struct End {
    winner: usize,
    reward: Reward,
}

/// The public message: scores, the rounds so far, who has locked in, and the button to pick.
struct View {
    content: String,
    embed: Embed,
    components: Vec<Component>,
}

impl View {
    fn response(self) -> InteractionResponseData {
        InteractionResponseData {
            content: Some(self.content),
            embeds: Some(vec![self.embed]),
            components: Some(self.components),
            allowed_mentions: quiet(),
            ..Default::default()
        }
    }
}

fn view(
    lang: Lang,
    session: &GameSession,
    names: &[String; 2],
    state: &State,
    end: Option<&End>,
    deadline: DateTime<Utc>,
) -> View {
    let players = [session.host_id as u64, session.guest_id.unwrap_or(0) as u64];
    let mut lines = vec![format!(
        "**{}** {} - {} **{}**",
        names[0], state.score[0], state.score[1], names[1]
    )];
    let skip = state.history.len().saturating_sub(SHOWN_ROUNDS);
    for (i, (a, b)) in state.history.iter().enumerate().skip(skip) {
        let result = match round(*a, *b) {
            Round::First => t!(lang, "rps.takes-it", name = names[0]),
            Round::Second => t!(lang, "rps.takes-it", name = names[1]),
            Round::Tie => t!(lang, "rps.tie-again"),
        };
        lines.push(t!(
            lang,
            "rps.round-line",
            n = i + 1,
            first = a.emoji(),
            second = b.emoji(),
            result = result
        ));
    }
    let embed = {
        let mut embed = EmbedBuilder::new()
            .color(if end.is_some() { SUCCESS } else { BRAND })
            .title(t!(lang, "rps.title", rounds = state.rounds))
            .description(lines.join("\n"));
        if session.bet > 0 {
            embed = embed.footer(twilight_util::builder::embed::EmbedFooterBuilder::new(t!(
                lang,
                "rps.pot-footer",
                pot = thousands(session.bet * 2)
            )));
        }
        embed.build()
    };
    match end {
        None => {
            let status = |i: usize| {
                if state.picks[i].is_some() {
                    t!(lang, "rps.locked-in", name = names[i])
                } else {
                    t!(lang, "rps.choosing", name = names[i])
                }
            };
            View {
                content: t!(
                    lang,
                    "rps.round",
                    host = mention(players[0]),
                    guest = mention(players[1]),
                    round = state.round,
                    when = relative(deadline),
                    host_status = status(0),
                    guest_status = status(1)
                ),
                embed,
                components: vec![button_row(
                    format!("rp:o:{}", session.id),
                    &t!(lang, "rps.choose"),
                    ButtonStyle::Primary,
                )],
            }
        }
        Some(end) => {
            let note = match end.reward {
                Reward::Pot => t!(lang, "duel.pot-won", pot = thousands(session.bet * 2)),
                Reward::Paid(paid) => t!(lang, "duel.paid", paid = paid),
                Reward::Limit => t!(lang, "duel.limit-reached"),
            };
            View {
                content: t!(
                    lang,
                    "rps.won",
                    winner = mention(players[end.winner]),
                    host_score = state.score[0],
                    guest_score = state.score[1],
                    note = note
                ),
                embed,
                components: Vec::new(),
            }
        }
    }
}

/// The first round, shown when the challenge is accepted.
pub(crate) async fn begin(cx: &Cx, session: &GameSession) -> anyhow::Result<()> {
    let (fields, names) = unpack(&session.state);
    let state = State::decode(fields).ok_or_else(|| anyhow::anyhow!("bad rock paper scissors state {fields:?}"))?;
    cx.update(view(cx.lang(), session, &names, &state, None, session.expires_at).response())
        .await
}

/// `rp:y:<id>` and `rp:n:<id>` answer a challenge, `rp:o:<id>` opens a player's private picker and
/// `rp:m:<id>:<round>:<move>` locks a pick in.
pub(crate) async fn on_button(cx: &Cx, mut args: Split<'_, char>) -> anyhow::Result<()> {
    let action = args.next();
    let Some(id) = args.next().and_then(|v| v.parse::<i64>().ok()) else {
        return cx.reply_ephemeral(t!(cx.lang(), "duel.game-expired")).await;
    };
    match action {
        Some("y") => answer(cx, Kind::Rps, id, true).await,
        Some("n") => answer(cx, Kind::Rps, id, false).await,
        Some("o") => on_open(cx, id).await,
        Some("m") => {
            let round = args.next().and_then(|v| v.parse::<u8>().ok());
            let mv = args.next().and_then(|v| v.chars().next()).and_then(Move::from_key);
            match (round, mv) {
                (Some(round), Some(mv)) => on_pick(cx, id, round, mv).await,
                _ => cx.reply_ephemeral(t!(cx.lang(), "duel.button-expired")).await,
            }
        }
        _ => cx.reply_ephemeral(t!(cx.lang(), "duel.button-expired")).await,
    }
}

/// An open, running match the clicker plays in.
async fn running(cx: &Cx, id: i64) -> anyhow::Result<Option<GameSession>> {
    let me = cx.author()?.id.get();
    // Another kind's id is not this match: its buttons pass another module's switch.
    let Some(session) = cx
        .app
        .db
        .game(id)
        .await?
        .filter(|s| s.is_active() && s.kind == Kind::Rps.key())
    else {
        cx.reply_ephemeral(t!(cx.lang(), "rps.match-over")).await?;
        return Ok(None);
    };
    if !session.has_player(me) {
        cx.reply_ephemeral(t!(cx.lang(), "rps.not-in-match")).await?;
        return Ok(None);
    }
    if session.expired() {
        expire_here(cx, &session).await?;
        return Ok(None);
    }
    Ok(Some(session))
}

async fn on_open(cx: &Cx, id: i64) -> anyhow::Result<()> {
    let Some(session) = running(cx, id).await? else {
        return Ok(());
    };
    let me = cx.author()?.id.get();
    let (fields, names) = unpack(&session.state);
    let state = State::decode(fields).ok_or_else(|| anyhow::anyhow!("bad rock paper scissors state {fields:?}"))?;
    let mine = usize::from(me != session.host_id as u64);
    let lang = cx.lang();
    if let Some(mv) = state.picks[mine] {
        return cx
            .reply_ephemeral(t!(
                lang,
                "rps.already-locked",
                pick = format!("{} {}", mv.emoji(), mv.label(lang)),
                name = names[1 - mine]
            ))
            .await;
    }
    let buttons = Move::ALL
        .iter()
        .map(|mv| {
            button(
                format!("rp:m:{id}:{}:{}", state.round, mv.key()),
                &format!("{} {}", mv.emoji(), mv.label(lang)),
                ButtonStyle::Primary,
                false,
            )
        })
        .collect();
    cx.reply(InteractionResponseData {
        content: Some(t!(lang, "rps.pick-prompt", round = state.round, name = names[1 - mine])),
        components: Some(vec![Component::ActionRow(ActionRow {
            id: None,
            components: buttons,
        })]),
        flags: Some(twilight_model::channel::message::MessageFlags::EPHEMERAL),
        ..Default::default()
    })
    .await
}

/// Answers the picker's own (private) message and drops its buttons.
async fn private(cx: &Cx, text: String) -> anyhow::Result<()> {
    cx.update(InteractionResponseData {
        content: Some(text),
        components: Some(Vec::new()),
        ..Default::default()
    })
    .await
}

async fn on_pick(cx: &Cx, id: i64, round_no: u8, mv: Move) -> anyhow::Result<()> {
    let me = cx.author()?.id.get();
    let lang = cx.lang();
    // Two picks can land at once; the one that loses the race for the state tries again.
    for _ in 0..4 {
        let Some(session) = running(cx, id).await? else {
            return Ok(());
        };
        let (host, guest) = (session.host_id as u64, session.guest_id.unwrap_or(0) as u64);
        let (fields, names) = unpack(&session.state);
        let Some(mut state) = State::decode(fields) else {
            anyhow::bail!("bad rock paper scissors state {fields:?}");
        };
        if state.round != round_no {
            return private(cx, t!(lang, "rps.round-over")).await;
        }
        let mine = usize::from(me != host);
        if let Some(locked) = state.picks[mine] {
            let pick = format!("{} {}", locked.emoji(), locked.label(lang));
            return private(cx, t!(lang, "rps.locked", pick = pick)).await;
        }
        state.picks[mine] = Some(mv);
        let deadline = Utc::now() + Duration::seconds(TURN_SECS);

        let [Some(first), Some(second)] = state.picks else {
            // The other player is still choosing.
            let next = pack(&state.encode(), [&names[0], &names[1]]);
            if !cx.app.db.advance_game(id, &session.state, &next, TURN_SECS).await? {
                continue;
            }
            refresh_public(cx, &session, &names, &state, None, deadline).await;
            let pick = format!("{} {}", mv.emoji(), mv.label(lang));
            return private(cx, t!(lang, "rps.locked-waiting", pick = pick, name = names[1 - mine])).await;
        };

        // Both have picked: reveal.
        let outcome = round(first, second);
        let mut winner = None;
        match outcome {
            Round::First => state.score[0] += 1,
            Round::Second => state.score[1] += 1,
            Round::Tie => {}
        }
        state.history.push((first, second));
        if state.history.len() > KEPT_ROUNDS {
            state.history.remove(0);
        }
        for i in 0..2 {
            if state.score[i] >= wins_needed(state.rounds) {
                winner = Some(i);
            }
        }
        if outcome != Round::Tie {
            state.round += 1;
        }
        state.picks = [None, None];
        let result = || {
            let mine_move = if mine == 0 { first } else { second };
            let theirs = if mine == 0 { second } else { first };
            let verdict = match outcome {
                Round::Tie => t!(lang, "rps.result-tie"),
                _ if (outcome == Round::First) == (mine == 0) => t!(lang, "rps.result-win"),
                _ => t!(lang, "rps.result-lose"),
            };
            t!(
                lang,
                "rps.result",
                mine = format!("{} {}", mine_move.emoji(), mine_move.label(lang)),
                theirs = format!("{} {}", theirs.emoji(), theirs.label(lang)),
                verdict = verdict
            )
        };

        let Some(w) = winner else {
            let next = pack(&state.encode(), [&names[0], &names[1]]);
            if !cx.app.db.advance_game(id, &session.state, &next, TURN_SECS).await? {
                continue;
            }
            refresh_public(cx, &session, &names, &state, None, deadline).await;
            return private(cx, result()).await;
        };

        // The match is decided: end it and pay, once.
        let finished = pack(&state.encode(), [&names[0], &names[1]]);
        let champion = if w == 0 { host } else { guest };
        if cx
            .app
            .db
            .settle_game(id, Some(&session.state), &finished, Payout::Winner(champion), false)
            .await?
            .is_none()
        {
            continue;
        }
        for (i, player) in [host, guest].into_iter().enumerate() {
            record(&cx.app, Game::Rps, player, i == w).await;
        }
        let reward = if session.bet > 0 {
            Reward::Pot
        } else {
            match pay_win(&cx.app, champion, Game::Rps, Game::Rps.win_reward(), "rps_win").await? {
                0 => Reward::Limit,
                paid => Reward::Paid(paid),
            }
        };
        let end = End { winner: w, reward };
        refresh_public(cx, &session, &names, &state, Some(&end), deadline).await;
        return private(cx, result()).await;
    }
    private(cx, t!(lang, "rps.moved-on")).await
}

/// Edits the public message with the bot's token, because the click that changed it was on a
/// private one.
async fn refresh_public(
    cx: &Cx,
    session: &GameSession,
    names: &[String; 2],
    state: &State,
    end: Option<&End>,
    deadline: DateTime<Utc>,
) {
    let Some((channel, message)) = location(session) else {
        return;
    };
    // The bot's own edit is for everyone: the server's language.
    let lang = cx.app.guild_lang(session.guild_id as u64).await;
    let view = view(lang, session, names, state, end, deadline);
    edit_message(
        &cx.app,
        channel,
        message,
        Some(&view.content),
        Some(&[view.embed]),
        Some(&view.components),
    )
    .await;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn state_round_trips() {
        let mut state = State::new(5);
        state.score = [1, 2];
        state.round = 4;
        state.picks = [Some(Move::Rock), None];
        state.history = vec![(Move::Rock, Move::Scissors), (Move::Paper, Move::Paper)];
        let text = state.encode();
        assert_eq!(text, "5:1:2:4:r:-:rspp");
        let again = State::decode(&text).expect("decodes");
        assert_eq!(
            (again.rounds, again.score, again.round, again.picks, again.history),
            (
                5,
                [1, 2],
                4,
                [Some(Move::Rock), None],
                vec![(Move::Rock, Move::Scissors), (Move::Paper, Move::Paper)]
            )
        );
        for bad in ["", "2:0:0:1:-:-:", "3:0:0:1:x:-:", "3:0:0:1:-:-:r", "3:0:0:1:-:-:zz"] {
            assert!(State::decode(bad).is_none(), "{bad:?}");
        }
        assert_eq!(initial_fields(3), "3:0:0:1:-:-:");
        assert_eq!(
            initial_fields(4),
            "3:0:0:1:-:-:",
            "odd match lengths fall back to best of 3"
        );
    }
}
