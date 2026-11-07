//! Family-friendly Never Have I Ever: a timed group poll with private votes.
use super::{after, card, edit_card, remember, row, track_message};
use crate::{
    App, Cx,
    commands::games::{button, channel_lang, channel_of, pay_win, record},
    util::{BRAND, SUCCESS, relative},
};
use pinchora_core::{i18n::Lang, t};
use pinchora_db::{Commit, NewParty, PartyGame, PollChoice};
use pinchora_game::{party::Party, play::Game};
use pinchora_render::{QuickCard, QuickDetails, QuickKind, Theme};
use serde::{Deserialize, Serialize};
use std::{str::Split, sync::Arc, time::Duration};
use twilight_model::{channel::message::component::ButtonStyle, http::interaction::InteractionResponseData};
use twilight_util::builder::embed::EmbedBuilder;

const SECS: i64 = 300;
const EN: [&str; 16] = [
    "I have never built a sandcastle.",
    "I have never named a cloud.",
    "I have never worn mismatched socks.",
    "I have never tried a new food and loved it.",
    "I have never gotten lost in a book.",
    "I have never danced in the rain.",
    "I have never seen a shooting star.",
    "I have never made a gift by hand.",
    "I have never learned a magic trick.",
    "I have never stayed up to watch a sunrise.",
    "I have never found a four-leaf clover.",
    "I have never taught a pet a trick.",
    "I have never sailed on a boat.",
    "I have never written a poem.",
    "I have never planted a seed.",
    "I have never spotted a rainbow at sea.",
];
const TR: [&str; 16] = [
    "Ben hiç kumdan kale yapmadım.",
    "Ben hiç bir buluta ad vermedim.",
    "Ben hiç farklı çoraplar giymedim.",
    "Ben hiç yeni bir yiyecek deneyip sevmedim.",
    "Ben hiç bir kitaba dalıp gitmedim.",
    "Ben hiç yağmurda dans etmedim.",
    "Ben hiç kayan yıldız görmedim.",
    "Ben hiç el yapımı hediye vermedim.",
    "Ben hiç sihirbazlık numarası öğrenmedim.",
    "Ben hiç güneşin doğuşunu izlemek için uyanık kalmadım.",
    "Ben hiç dört yapraklı yonca bulmadım.",
    "Ben hiç bir evcil hayvana numara öğretmedim.",
    "Ben hiç tekneyle gezmedim.",
    "Ben hiç şiir yazmadım.",
    "Ben hiç tohum ekmedim.",
    "Ben hiç denizde gökkuşağı görmedim.",
];
#[derive(Serialize, Deserialize)]
struct State {
    lang: Lang,
    q: usize,
    nonce: u64,
}
fn question(state: &State) -> &'static str {
    if state.lang == Lang::Tr {
        TR[state.q % TR.len()]
    } else {
        EN[state.q % EN.len()]
    }
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
        q: rand::random_range(0..EN.len()),
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
    let prompt = question(&s).to_owned();
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
    use super::*;
    #[test]
    fn packs_are_family_friendly_and_equal_length() {
        assert_eq!(EN.len(), TR.len());
        assert!(EN.iter().all(|q| q.starts_with("I have never ") && q.len() < 100));
        assert!(TR.iter().all(|q| q.starts_with("Ben hiç ") && q.len() < 120));
    }
}
