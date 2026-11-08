use super::{Intent, ReefGame, ReefGamePlayer, ReefGameStep, Session};

pub(super) fn transition(
    g: &ReefGame,
    players: &[ReefGamePlayer],
    user: u64,
    revision: i64,
    intent: &Intent,
    now: i64,
) -> anyhow::Result<ReefGameStep> {
    let mut state: Session = serde_json::from_value(g.state.clone())?;
    anyhow::ensure!(!g.finished, "game_finished");
    let mut next = ReefGameStep {
        state: serde_json::Value::Null,
        finished: false,
        reward: 0,
        cap: state.kind().daily_cap(),
        seconds: 0,
        join: false,
        leave: false,
        answer: None,
        winners: vec![],
    };
    if matches!(intent, Intent::Refresh) {
        anyhow::ensure!(now >= g.deadline.timestamp(), "invalid_action");
        expire(&mut state);
    } else {
        // Timers are internal transitions. A late spectator or stale action never advances a game.
        anyhow::ensure!(revision == g.revision, "stale_action");
        if !matches!(intent, Intent::Join) {
            anyhow::ensure!(players.iter().any(|p| p.user_id == user as i64), "spectator");
        }
        anyhow::ensure!(now < g.deadline.timestamp(), "game_expired");
        apply_intent(g, players, user, intent, &mut state, &mut next)?;
    }
    next.finished = state.phase == "done";
    if next.finished {
        next.winners = winners(&state, players, user, next.answer);
        next.reward = state.reward(g.room_key.starts_with("plaza:"), players.len());
    }
    next.state = serde_json::to_value(state)?;
    Ok(next)
}

fn expire(state: &mut Session) {
    if state.phase == "active" {
        if let Some(turn) = state.turn() {
            state.finish(Some(1 - turn), "timeout");
        } else {
            state.finish(None, "reveal");
        }
    } else {
        state.finish(None, "expired");
    }
}

fn apply_intent(
    g: &ReefGame,
    players: &[ReefGamePlayer],
    user: u64,
    intent: &Intent,
    state: &mut Session,
    next: &mut ReefGameStep,
) -> anyhow::Result<()> {
    let me = players.iter().find(|p| p.user_id == user as i64);
    match intent {
        Intent::Join => {
            anyhow::ensure!(state.phase == "waiting" && me.is_none(), "invalid_action");
            let max = if g.kind == "trivia" { 32 } else { 2 };
            anyhow::ensure!(players.len() < max, "invalid_action");
            next.join = true;
            if max == 2 {
                state.phase = "active".into();
                next.seconds = state.seconds();
            }
        }
        Intent::Start => {
            anyhow::ensure!(
                g.kind == "trivia" && state.phase == "waiting" && g.host_id == user as i64 && players.len() >= 2,
                "invalid_action"
            );
            state.phase = "active".into();
            next.seconds = state.seconds();
        }
        Intent::Leave => {
            let me = me.ok_or_else(|| anyhow::anyhow!("spectator"))?;
            if g.kind == "trivia" && players.len() > 1 && (state.phase == "active" || g.host_id != user as i64) {
                next.leave = true;
            } else {
                state.finish(
                    if state.phase == "active" && g.kind != "trivia" {
                        Some(1 - me.side)
                    } else {
                        None
                    },
                    "resigned",
                );
            }
        }
        Intent::Move { code } => {
            state.play(me.ok_or_else(|| anyhow::anyhow!("spectator"))?.side, code)?;
            next.seconds = state.seconds();
        }
        Intent::Answer { choice } => {
            let me = me.ok_or_else(|| anyhow::anyhow!("spectator"))?;
            anyhow::ensure!(
                g.kind == "trivia" && state.phase == "active" && *choice < 4 && me.answer.is_none(),
                "invalid_action"
            );
            next.answer = Some(i16::from(*choice));
            if players.iter().all(|p| p.user_id == user as i64 || p.answer.is_some()) {
                state.finish(None, "reveal");
            }
        }
        Intent::Refresh => anyhow::bail!("invalid_action"),
    }
    Ok(())
}

fn winners(state: &Session, players: &[ReefGamePlayer], user: u64, answer: Option<i16>) -> Vec<i16> {
    if state.kind() != pinchora_game::play::Game::Trivia {
        return state.winner.into_iter().collect();
    }
    if state.reason.as_deref() != Some("reveal") {
        return vec![];
    }
    let correct = state
        .question()
        .and_then(|(q, order)| q.correct_slot(order))
        .map(|n| n as i16);
    players
        .iter()
        .filter_map(|p| {
            let choice = if p.user_id == user as i64 {
                answer.or(p.answer)
            } else {
                p.answer
            };
            (choice.is_some() && choice == correct).then_some(p.side)
        })
        .collect()
}
