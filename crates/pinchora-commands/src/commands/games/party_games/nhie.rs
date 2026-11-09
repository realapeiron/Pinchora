//! Family-friendly Never Have I Ever: a timed group poll with private votes.
use crate::App;
use crate::Cx;
use crate::commands::games::games::button;
use crate::commands::games::games::channel_lang;
use crate::commands::games::games::channel_of;
use crate::commands::games::games::pay_win;
use crate::commands::games::games::record;
use crate::commands::games::party_games::after;
use crate::commands::games::party_games::card;
use crate::commands::games::party_games::edit_card;
use crate::commands::games::party_games::remember;
use crate::commands::games::party_games::row;
use crate::commands::games::party_games::track_message;
use crate::infra::util::BRAND;
use crate::infra::util::SUCCESS;
use crate::infra::util::relative;
use pinchora_core::infra::i18n::Lang;
use pinchora_core::t;
use pinchora_db::{Commit, NewParty, PartyGame, PollChoice};
use pinchora_game::{party::Party, play::Game};
use pinchora_render::{QuickCard, QuickDetails, QuickKind, Theme};
use serde::{Deserialize, Serialize};
use std::{str::Split, sync::Arc, time::Duration};
use twilight_model::{channel::message::component::ButtonStyle, http::interaction::InteractionResponseData};
use twilight_util::builder::embed::EmbedBuilder;

const SECS: i64 = 300;
const QUESTION_COUNT: usize = 16;
#[derive(Serialize, Deserialize)]
struct State {
    lang: Lang,
    q: usize,
    nonce: u64,
}
fn question(state: &State) -> String {
    t!(state.lang, &format!("nhie.questions.item-{}", state.q % QUESTION_COUNT))
}
pub(crate) async fn open(cx: &Cx) -> anyhow::Result<()> {
    let app = &cx.app;
    let channel = channel_of(cx);
    if app
        .cache
        .try_cooldown(&format!("nhie:{channel}"), Duration::from_secs(20))
        .await?
        .is_some()
    {
        return cx.reply_ephemeral(t!(cx.lang(), "nhie.cooldown")).await;
    }
    let lang = channel_lang(cx).await;
    let state = State {
        lang,
        q: rand::random_range(0..QUESTION_COUNT),
        nonce: rand::random(),
    };
    let game = app
        .db
        .create_party(NewParty {
            kind: Party::NeverHaveIEver.key(),
            guild_id: cx.guild_id().unwrap_or(0),
            channel_id: channel,
            host: cx.author()?.id.get(),
            host_name: &cx.author_display_name(),
            rounds: 1,
            status: "playing",
            phase: "vote",
            state: &serde_json::to_string(&state)?,
            deadline: after(app, SECS),
        })
        .await?;
    cx.reply(view(app, &game, None).await?).await?;
    track_message(cx, game.id);
    Ok(())
}
async fn view(app: &App, game: &PartyGame, counts: Option<(i64, i64)>) -> anyhow::Result<InteractionResponseData> {
    let s: State = serde_json::from_str(&game.state)?;
    let lang = s.lang;
    let title = Game::NeverHaveIEver.name(lang);
    let subtitle = t!(lang, "nhie.subtitle");
    let prompt = question(&s);
    let status = match counts {
        Some((yes, no)) => t!(lang, "nhie.result", yes = yes, no = no),
        None => t!(lang, "nhie.open"),
    };
    let png = app
        .render(move |r| {
            r.quick_card(&QuickCard {
                kind: QuickKind::Nhie,
                details: QuickDetails {
                    finished: counts.is_some(),
                    ..Default::default()
                },
                lang,
                title: &title,
                subtitle: &subtitle,
                cells: &[],
                columns: 0,
                prompt: &prompt,
                status: &status,
                theme: Theme::reef(),
                distort: false,
            })
        })
        .await?;
    let embed = EmbedBuilder::new()
        .color(if counts.is_some() { SUCCESS } else { BRAND })
        .title(Game::NeverHaveIEver.name(lang))
        .description(if counts.is_some() {
            t!(lang, "nhie.closed")
        } else {
            t!(lang, "nhie.until", when = relative(game.deadline))
        });
    let buttons = if counts.is_some() {
        vec![]
    } else {
        vec![row(vec![
            button(
                format!("ni:y:{}:{}", game.id, s.nonce),
                &t!(lang, "nhie.yes"),
                ButtonStyle::Success,
                false,
            ),
            button(
                format!("ni:n:{}:{}", game.id, s.nonce),
                &t!(lang, "nhie.no"),
                ButtonStyle::Secondary,
                false,
            ),
        ])]
    };
    Ok(card(None, embed, "nhie.png", png, buttons))
}
pub(crate) async fn on_button(cx: &Cx, mut parts: Split<'_, char>) -> anyhow::Result<()> {
    let side = match parts.next() {
        Some("y") => 0,
        Some("n") => 1,
        _ => return cx.reply_ephemeral(t!(cx.lang(), "party.expired-button")).await,
    };
    let (id, nonce) = (
        parts.next().and_then(|v| v.parse::<i64>().ok()),
        parts.next().and_then(|v| v.parse::<u64>().ok()),
    );
    let (Some(id), Some(nonce)) = (id, nonce) else {
        return cx.reply_ephemeral(t!(cx.lang(), "party.expired-button")).await;
    };
    let Some(game) = cx
        .app
        .db
        .party(id)
        .await?
        .filter(|g| g.kind == Party::NeverHaveIEver.key())
    else {
        return cx.reply_ephemeral(t!(cx.lang(), "party.expired-button")).await;
    };
    let state: State = serde_json::from_str(&game.state)?;
    if state.nonce != nonce {
        return cx.reply_ephemeral(t!(cx.lang(), "party.expired-button")).await;
    }
    remember(cx, &game).await;
    let me = cx.author()?.id.get();
    let result = cx.app.db.put_poll_vote(id, me, side, cx.app.now()).await?;
    match result {
        PollChoice::Closed => cx.reply_ephemeral(t!(cx.lang(), "nhie.closed")).await,
        PollChoice::First => {
            let _ = pay_win(&cx.app, me, Game::NeverHaveIEver, 1, "nhie_vote").await?;
            cx.reply_ephemeral(t!(cx.lang(), "nhie.saved")).await
        }
        PollChoice::Changed | PollChoice::Same => cx.reply_ephemeral(t!(cx.lang(), "nhie.saved")).await,
    }
}
pub(crate) async fn on_due(app: &Arc<App>, game: PartyGame) -> anyhow::Result<bool> {
    if !game.is_playing() || !game.late(app.now()) {
        return Ok(false);
    }
    let commit = Commit {
        phase: "done",
        round: game.round,
        state: &game.state,
        deadline: game.deadline,
        status: "done",
        players: &[],
        entries: &[],
        pay: None,
    };
    let Some(done) = app.db.commit_party(game.id, game.seq, commit).await? else {
        return Ok(false);
    };
    let counts = app.db.poll_counts(game.id).await?;
    for (user, _) in app.db.poll_voters(game.id).await? {
        record(app, Game::NeverHaveIEver, user, true).await;
    }
    edit_card(app, &done.game, &view(app, &done.game, Some(counts)).await?).await;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use crate::commands::games::party_games::nhie::*;
    #[test]
    fn packs_are_family_friendly_and_equal_length() {
        for (lang, prefix, limit) in [(Lang::En, "I have never ", 100), (Lang::Tr, "Ben hiç ", 120)] {
            for q in 0..QUESTION_COUNT {
                let text = question(&State { lang, q, nonce: 0 });
                assert!(text.starts_with(prefix) && text.len() < limit, "{lang:?}: {text}");
            }
        }
    }
}
