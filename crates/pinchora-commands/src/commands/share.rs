//! `/duel share`: two players secretly pick Share or Grab, and the pool goes by what they picked.
//!
//! With no wager the pool is the sea's (a few shells, paid up to a daily limit); with a wager it is the
//! two stakes. Both share: split it (stakes come back). One grabs: they take all. Both grab: nobody
//! gets anything, and a wagered pot sinks into the sea. A player who never picks forfeits.

use std::{str::Split, sync::Arc};

use pinchora_db::{GameSession, PENDING, Payout};
use pinchora_game::{
    party::share::{DAILY_SHELLS, Outcome, PAIR_COOLDOWN_SECS, PICK_SECS, POOL, Pick, Picks},
    play::Game,
};
use pinchora_render::{Mood, PickView, ShareGrabCard, Theme};
use twilight_model::{
    channel::message::{MessageFlags, component::ButtonStyle},
    http::interaction::InteractionResponseData,
};
use twilight_util::builder::embed::EmbedBuilder;

use super::{
    duel::{Kind, answer},
    games::{button, button_row, location, pack, quiet, unpack},
    party::{count, edit_card, rest, resting},
};
use crate::{
    App, Cx,
    server_events::row,
    util::{BRAND, SUCCESS, mention, relative, thousands},
};

pub(crate) fn initial_fields() -> String {
    Picks::default().encode()
}

/// What the public message says after the last pick, or after a forfeit.
struct Ending {
    banner: String,
    mood: Mood,
    note: String,
}

/// The pool of a game: the stakes of a wagered one, the sea's otherwise.
fn pool_of(session: &GameSession) -> i64 {
    if session.bet > 0 { session.bet * 2 } else { POOL }
}

/// The picks as the card shows them: hidden while the game runs, shown once it is over.
fn shown(picks: Picks, over: bool) -> [PickView; 2] {
    std::array::from_fn(|i| match (picks.0[i], over) {
        (Some(Pick::Share), true) => PickView::Share,
        (Some(Pick::Grab), true) => PickView::Grab,
        (Some(_), false) => PickView::Locked,
        (None, _) => PickView::Thinking,
    })
}

/// The public message: the card, who has locked in, and the button that opens the private picker.
async fn message(
    app: &App,
    session: &GameSession,
    names: &[String; 2],
    picks: Picks,
    end: Option<&Ending>,
    deadline: chrono::DateTime<chrono::Utc>,
) -> anyhow::Result<InteractionResponseData> {
    let players = [session.host_id as u64, session.guest_id.unwrap_or(0) as u64];
    let (banner, mood) = match end {
        Some(end) => (end.banner.clone(), end.mood),
        None => {
            let waiting = picks.waiting();
            let text = match waiting.as_slice() {
                [one] => format!("Waiting for {}", names[*one]),
                _ => "Waiting for both of you".to_owned(),
            };
            (text, Mood::Wait)
        }
    };
    let (card_names, pool, wager) = (names.clone(), pool_of(session), session.bet > 0);
    let views = shown(picks, end.is_some());
    let png = app
        .render(move |r| {
            r.share_grab_card(&ShareGrabCard {
                names: [&card_names[0], &card_names[1]],
                picks: views,
                pool,
                wager,
                banner: &banner,
                mood,
                theme: Theme::reef(),
            })
        })
        .await?;
    let embed = EmbedBuilder::new()
        .color(if end.is_some() { SUCCESS } else { BRAND })
        .title("Share or Grab");
    let status = |i: usize| {
        if picks.0[i].is_some() {
            format!("✅ **{}** locked in", names[i])
        } else {
            format!("⏳ **{}** is choosing", names[i])
        }
    };
    let (content, components) = match end {
        None => (
            format!(
                "{} vs {}\nPick in secret, {}. The pool is **{}** shells.\n{} · {}",
                mention(players[0]),
                mention(players[1]),
                relative(deadline),
                thousands(pool_of(session)),
                status(0),
                status(1)
            ),
            vec![button_row(
                format!("sh:o:{}", session.id),
                "Choose in secret",
                ButtonStyle::Primary,
            )],
        ),
        Some(end) => (
            format!("{} vs {}\n{}", mention(players[0]), mention(players[1]), end.note),
            Vec::new(),
        ),
    };
    let mut data = Cx::card(embed, "share.png", png, components, false);
    data.content = Some(content);
    data.allowed_mentions = quiet();
    Ok(data)
}

/// The first message, shown when the challenge is accepted.
pub(crate) async fn begin(cx: &Cx, session: &GameSession) -> anyhow::Result<()> {
    let (fields, names) = unpack(&session.state);
    let picks = Picks::decode(fields).ok_or_else(|| anyhow::anyhow!("bad share or grab state {fields:?}"))?;
    cx.update(message(&cx.app, session, &names, picks, None, session.expires_at).await?)
        .await
}

/// `sh:y:<id>` and `sh:n:<id>` answer the challenge, `sh:o:<id>` opens a player's private picker and
/// `sh:m:<id>:<s|g>` locks a pick in.
pub(crate) async fn on_button(cx: &Cx, mut args: Split<'_, char>) -> anyhow::Result<()> {
    let action = args.next();
    let Some(id) = args.next().and_then(|v| v.parse::<i64>().ok()) else {
        return cx.reply_ephemeral("This game has expired.").await;
    };
    match action {
        Some("y") => answer(cx, Kind::ShareGrab, id, true).await,
        Some("n") => answer(cx, Kind::ShareGrab, id, false).await,
        Some("o") => on_open(cx, id).await,
        Some("m") => match args.next().and_then(|v| v.chars().next()).and_then(Pick::from_key) {
            Some(pick) => on_pick(cx, id, pick).await,
            None => cx.reply_ephemeral("This button has expired.").await,
        },
        _ => cx.reply_ephemeral("This button has expired.").await,
    }
}

/// A running game the clicker plays in. A late one is ended here and now.
async fn running(cx: &Cx, id: i64) -> anyhow::Result<Option<GameSession>> {
    let me = cx.author()?.id.get();
    let Some(session) = cx
        .app
        .db
        .game(id)
        .await?
        .filter(|s| s.is_active() && s.kind == Kind::ShareGrab.key())
    else {
        cx.reply_ephemeral("This game is over.").await?;
        return Ok(None);
    };
    if !session.has_player(me) {
        cx.reply_ephemeral("You're not in this game. Challenge someone with `/duel share`!")
            .await?;
        return Ok(None);
    }
    if session.expired() {
        match forfeit(&cx.app, &session).await? {
            Some(card) => cx.update(card).await?,
            None => cx.reply_ephemeral("This game is over.").await?,
        }
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
    let picks = Picks::decode(fields).ok_or_else(|| anyhow::anyhow!("bad share or grab state {fields:?}"))?;
    let mine = usize::from(me != session.host_id as u64);
    if let Some(pick) = picks.0[mine] {
        return cx
            .reply_ephemeral(format!(
                "You locked in {} {}. Waiting for **{}**.",
                pick.emoji(),
                pick.label(),
                names[1 - mine]
            ))
            .await;
    }
    let stakes = if session.bet > 0 {
        format!(
            "Both grab and the **{}** shells sink into the sea.",
            thousands(session.bet * 2)
        )
    } else {
        "Both grab and nobody gets anything.".to_owned()
    };
    let buttons = Pick::ALL
        .iter()
        .map(|pick| {
            button(
                format!("sh:m:{id}:{}", pick.key()),
                &format!("{} {}", pick.emoji(), pick.label()),
                if *pick == Pick::Share {
                    ButtonStyle::Success
                } else {
                    ButtonStyle::Danger
                },
                false,
            )
        })
        .collect();
    cx.reply(InteractionResponseData {
        content: Some(format!(
            "Choose in secret. **{}** can't see it until you both have.\n\
             🤝 **Share** splits the pool. 🦀 **Grab** takes it all if they share. {stakes}",
            names[1 - mine]
        )),
        components: Some(vec![row(buttons)]),
        flags: Some(MessageFlags::EPHEMERAL),
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

async fn on_pick(cx: &Cx, id: i64, pick: Pick) -> anyhow::Result<()> {
    let me = cx.author()?.id.get();
    // Two picks can land at once; the one that loses the race for the state tries again.
    for _ in 0..4 {
        let Some(session) = running(cx, id).await? else {
            return Ok(());
        };
        let (fields, names) = unpack(&session.state);
        let Some(mut picks) = Picks::decode(fields) else {
            anyhow::bail!("bad share or grab state {fields:?}");
        };
        let mine = usize::from(me != session.host_id as u64);
        if let Some(locked) = picks.0[mine] {
            return private(cx, format!("You locked in {} {}.", locked.emoji(), locked.label())).await;
        }
        picks.0[mine] = Some(pick);
        let next = pack(&picks.encode(), [&names[0], &names[1]]);
        let Some(outcome) = picks.outcome() else {
            // The other player is still choosing.
            if !cx.app.db.advance_game(id, &session.state, &next, PICK_SECS).await? {
                continue;
            }
            refresh_public(&cx.app, &session, &names, picks, None).await;
            return private(
                cx,
                format!(
                    "🔒 Locked in {} {}. Waiting for **{}**.",
                    pick.emoji(),
                    pick.label(),
                    names[1 - mine]
                ),
            )
            .await;
        };
        // Both have picked: end the game and pay, once.
        let Some(end) = conclude(&cx.app, &session, &names, outcome).await? else {
            continue;
        };
        let theirs = picks.0[1 - mine].unwrap_or(pick);
        let line = format!(
            "{} {} against {} {}. {}",
            pick.emoji(),
            pick.label(),
            theirs.emoji(),
            theirs.label(),
            end.banner
        );
        refresh_public(&cx.app, &session, &names, picks, Some(&end)).await;
        return private(cx, line).await;
    }
    private(
        cx,
        "The game moved on while you picked. Press **Choose in secret** again.".to_owned(),
    )
    .await
}

/// Ends a game both players have picked in: pays by the outcome, counts it, and says how it went.
/// `None` when somebody else moved first and the pick has to be tried again.
async fn conclude(
    app: &App,
    session: &GameSession,
    names: &[String; 2],
    outcome: Outcome,
) -> anyhow::Result<Option<Ending>> {
    let players = [session.host_id as u64, session.guest_id.unwrap_or(0) as u64];
    let finished = pack("done", [&names[0], &names[1]]);
    let pot = pool_of(session);
    let mut note = String::new();
    if session.bet > 0 {
        let payout = match outcome {
            Outcome::BothShare => Payout::Refund,
            Outcome::Grabbed(who) => Payout::Winner(players[who]),
            Outcome::BothGrab => Payout::None,
        };
        let ended = app
            .db
            .settle_game(session.id, Some(&session.state), &finished, payout, false)
            .await?;
        if ended.is_none() {
            return Ok(None);
        }
    } else {
        let mut grants = outcome.split(POOL);
        let (lo, hi) = (players[0].min(players[1]), players[0].max(players[1]));
        // A pair is paid from the pool once an hour, so two accounts cannot trade it back and forth. The
        // hour starts when a payout has gone through, not when one is tried: a click that loses the race to
        // end the game must not leave the winner's payout facing a pair that looks paid already.
        let pair = format!("sharepair:{lo}");
        let paying = grants.iter().any(|g| *g > 0) && resting(app, &pair, hi).await?.is_none();
        if !paying {
            grants = [0, 0];
        }
        let Some((_, paid)) = app
            .db
            .settle_game_granting(session.id, Some(&session.state), &finished, grants, DAILY_SHELLS, false)
            .await?
        else {
            return Ok(None);
        };
        if paid.iter().sum::<i64>() > 0 {
            rest(app, &pair, hi, PAIR_COOLDOWN_SECS).await;
        }
        note = if !paying && outcome != Outcome::BothGrab {
            "(This pair was paid from the sea's pool within the hour, so no shells this time.)".to_owned()
        } else if paid.iter().sum::<i64>() < grants.iter().sum::<i64>() {
            "(Today's limit for this game is reached, so some shells were not paid.)".to_owned()
        } else {
            String::new()
        };
    }
    let (banner, mood, line) = match outcome {
        Outcome::BothShare if session.bet > 0 => (
            "Both shared, stakes back".to_owned(),
            Mood::Good,
            "🤝 Both shared! Each gets their stake back.".to_owned(),
        ),
        Outcome::BothShare => (
            format!("Both shared, {} shells each", POOL / 2),
            Mood::Good,
            format!("🤝 Both shared! The pool is split: **{}** shells each.", POOL / 2),
        ),
        Outcome::Grabbed(who) => (
            format!("{} grabbed it all", names[who]),
            Mood::Bad,
            format!(
                "🦀 {} grabbed the whole pool of **{}** shells while {} shared!",
                mention(players[who]),
                thousands(pot),
                mention(players[1 - who])
            ),
        ),
        Outcome::BothGrab => (
            "Both grabbed, nobody wins".to_owned(),
            Mood::Bad,
            if session.bet > 0 {
                format!("🦀 Both grabbed! The **{}** shells sink into the sea.", thousands(pot))
            } else {
                "🦀 Both grabbed! The pool washes away.".to_owned()
            },
        ),
    };
    let won = |i: usize| match outcome {
        Outcome::BothShare => true,
        Outcome::Grabbed(who) => who == i,
        Outcome::BothGrab => false,
    };
    count(app, Game::ShareGrab, &[(players[0], won(0)), (players[1], won(1))]).await;
    Ok(Some(Ending {
        banner,
        mood,
        note: if note.is_empty() {
            line
        } else {
            format!("{line}\n{note}")
        },
    }))
}

/// Edits the public message with the bot's token, because the click that changed it was on a private one.
async fn refresh_public(app: &App, session: &GameSession, names: &[String; 2], picks: Picks, end: Option<&Ending>) {
    let Some(at) = location(session) else {
        return;
    };
    let deadline = chrono::Utc::now() + chrono::Duration::seconds(PICK_SECS);
    let data = match message(app, session, names, picks, end, deadline).await {
        Ok(data) => data,
        Err(error) => return tracing::warn!(%error, "share or grab card not drawn"),
    };
    publish(app, at, data).await;
}

/// Puts a built message on an existing one with the bot's token.
async fn publish(app: &App, at: (u64, u64), data: InteractionResponseData) {
    let (Some(embeds), Some(attachments)) = (data.embeds, data.attachments) else {
        return;
    };
    let (Some(embed), Some(file)) = (embeds.into_iter().next(), attachments.into_iter().next()) else {
        return;
    };
    edit_card(
        app,
        at,
        data.content.as_deref(),
        embed,
        (&file.filename, file.file),
        &data.components.unwrap_or_default(),
    )
    .await;
}

/// Ends a game whose clock ran out: the player who never picked forfeits. With a wager the other
/// player takes the pot; when nobody picked the stakes go back; the sea's pool is simply not paid.
/// Returns the message to show, or `None` when somebody else ended it first.
async fn forfeit(app: &App, session: &GameSession) -> anyhow::Result<Option<InteractionResponseData>> {
    let (fields, names) = unpack(&session.state);
    let picks = Picks::decode(fields).unwrap_or_default();
    let players = [session.host_id as u64, session.guest_id.unwrap_or(0) as u64];
    let waiting = picks.waiting();
    let (payout, banner, note) = match waiting.as_slice() {
        [slow] if session.bet > 0 => {
            let winner = 1 - slow;
            (
                Payout::Winner(players[winner]),
                format!("{} ran out of time", names[*slow]),
                format!(
                    "⌛ {} ran out of time and forfeits: {} takes the pot of **{}** shells.",
                    mention(players[*slow]),
                    mention(players[winner]),
                    thousands(session.bet * 2)
                ),
            )
        }
        [slow] => (
            Payout::None,
            format!("{} ran out of time", names[*slow]),
            format!(
                "⌛ {} ran out of time, so the pool washes away.",
                mention(players[*slow])
            ),
        ),
        _ => (
            Payout::Refund,
            "Nobody picked in time".to_owned(),
            if session.bet > 0 {
                "⌛ Nobody picked in time. Both stakes went back.".to_owned()
            } else {
                "⌛ Nobody picked in time, so the pool washes away.".to_owned()
            },
        ),
    };
    let ended = app
        .db
        .settle_game(session.id, Some(&session.state), "timeout", payout, true)
        .await?;
    if ended.is_none() {
        return Ok(None);
    }
    // Only a forfeit by one of two counts as a loss; stakes returned for nobody picking are not a game.
    if let [slow] = waiting.as_slice() {
        count(
            app,
            Game::ShareGrab,
            &[(players[1 - slow], session.bet > 0), (players[*slow], false)],
        )
        .await;
    }
    let ending = Ending {
        banner,
        mood: Mood::Bad,
        note,
    };
    let data = message(app, session, &names, picks, Some(&ending), chrono::Utc::now()).await?;
    Ok(Some(data))
}

/// The janitor's end for a game whose clock ran out.
pub(crate) async fn time_out(app: &Arc<App>, session: &GameSession) -> anyhow::Result<bool> {
    if session.status == PENDING {
        // An unanswered challenge: nothing was held, the duel code closes it.
        return super::duel::time_out(app, session).await;
    }
    let Some(data) = forfeit(app, session).await? else {
        return Ok(false);
    };
    if let Some(at) = location(session) {
        publish(app, at, data).await;
    }
    Ok(true)
}

/// Who still had to pick when the game timed out, for the generic timeout text of a challenge.
pub(crate) fn slow_players(session: &GameSession) -> Vec<u64> {
    let (fields, _) = unpack(&session.state);
    let players = [Some(session.host_id as u64), session.guest_id.map(|g| g as u64)];
    Picks::decode(fields)
        .map(|picks| picks.waiting().into_iter().filter_map(|i| players[i]).collect())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_card_hides_picks_until_the_game_is_over() {
        let picks = Picks([Some(Pick::Share), None]);
        assert_eq!(shown(picks, false), [PickView::Locked, PickView::Thinking]);
        let both = Picks([Some(Pick::Share), Some(Pick::Grab)]);
        assert_eq!(shown(both, false), [PickView::Locked, PickView::Locked]);
        assert_eq!(shown(both, true), [PickView::Share, PickView::Grab]);
    }

    #[test]
    fn the_pool_is_the_stakes_or_the_seas() {
        let mut session = GameSession {
            id: 1,
            kind: "sharegrab".into(),
            guild_id: 1,
            channel_id: 1,
            message_id: None,
            host_id: 1,
            guest_id: Some(2),
            bet: 0,
            status: "active".into(),
            state: "-:-|A|B".into(),
            expires_at: chrono::Utc::now(),
        };
        assert_eq!(pool_of(&session), POOL);
        session.bet = 75;
        assert_eq!(pool_of(&session), 150);
        assert_eq!(slow_players(&session), vec![1, 2]);
        session.state = "s:-|A|B".into();
        assert_eq!(slow_players(&session), vec![2]);
    }
}
