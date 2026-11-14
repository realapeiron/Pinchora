use super::*;

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
fn timeout_text(session: &GameSession, lang: Lang) -> String {
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
