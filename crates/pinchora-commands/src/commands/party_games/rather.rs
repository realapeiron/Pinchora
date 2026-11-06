//! Would You Rather: one question with two sides for the whole channel. Everybody votes with a button and
//! may change their mind; the split stays hidden until the poll closes, so nobody just follows the crowd.

use std::{str::Split, sync::Arc, time::Duration};

use pinchora_core::{i18n::Lang, t};
use pinchora_db::{Commit, NewParty, PartyGame, PollChoice};
use pinchora_game::{
    lang::upper,
    party::{
        POLL_VOTE_REWARD, Party,
        prompts::{Category, PartyContent, Rather},
    },
    play::Game,
};
use pinchora_render::RatherCard;
use serde::{Deserialize, Serialize};
use twilight_model::channel::message::component::ButtonStyle;
use twilight_util::builder::embed::EmbedBuilder;

use super::{after, card, edit_card, game_lang, remember, row, theme, track_message};
use crate::{
    App, Cx,
    commands::games::{button, channel_of, pay_win, record},
    util::{BRAND, SUCCESS, relative},
};

/// How long a poll stays open.
const POLL_SECS: i64 = 600;
/// The shortest wait between polls in one channel.
const COOLDOWN_SECS: u64 = 20;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct State {
    /// The language the poll is shown in: the question is from its pool.
    #[serde(default)]
    lang: Lang,
    /// The question's id in `content/party.json`.
    q: String,
}

fn question(game: &PartyGame) -> anyhow::Result<&'static Rather> {
    let state: State = serde_json::from_str(&game.state)?;
    PartyContent::find_rather(&state.q).ok_or_else(|| anyhow::anyhow!("unknown would-you-rather question {}", state.q))
}

/// A button label: `A · the first thing`, cut to what a button holds.
fn label(side: &str, text: &str) -> String {
    format!("{side} · {text}").chars().take(80).collect()
}

pub(crate) async fn open(cx: &Cx, topic: Option<Category>) -> anyhow::Result<()> {
    let app = &cx.app;
    let channel = channel_of(cx);
    if let Some(left) = app
        .cache
        .try_cooldown(&format!("wyr:{channel}"), Duration::from_secs(COOLDOWN_SECS))
        .await?
    {
        return cx
            .reply_ephemeral(t!(cx.lang(), "rather.cooldown", secs = left.as_secs() + 1))
            .await;
    }
    let pool = PartyContent::for_lang(cx.lang()).questions(topic);
    anyhow::ensure!(!pool.is_empty(), "no questions in this category");
    // Skip questions this channel saw lately, a few tries deep.
    let mut chosen = pool[rand::random_range(0..pool.len())];
    for _ in 0..6 {
        let candidate = pool[rand::random_range(0..pool.len())];
        chosen = candidate;
        if app
            .cache
            .once(
                &format!("wyrseen:{channel}:{}", candidate.id),
                Duration::from_secs(6 * 3600),
            )
            .await?
        {
            break;
        }
    }
    let host = cx.author()?.id.get();
    let game = app
        .db
        .create_party(NewParty {
            kind: Party::WouldYouRather.key(),
            guild_id: cx.guild_id().unwrap_or(0),
            channel_id: channel,
            host,
            host_name: &super::text::name(&cx.author_display_name(), cx.lang()),
            rounds: 1,
            status: "playing",
            phase: "vote",
            state: &serde_json::to_string(&State {
                lang: cx.lang(),
                q: chosen.id.clone(),
            })?,
            deadline: after(app, POLL_SECS),
        })
        .await?;
    cx.reply(view(app, &game, chosen, None).await?).await?;
    track_message(cx, game.id);
    Ok(())
}

/// The poll: the question card, and a button for each side (switched off once it has closed). `result` is
/// the split to show on the card, once the poll has closed.
async fn view(
    app: &App,
    game: &PartyGame,
    q: &Rather,
    result: Option<(i64, i64)>,
) -> anyhow::Result<twilight_model::http::interaction::InteractionResponseData> {
    let lang = game_lang(game);
    // The card draws the category in capitals, which Turkish spells with its own İ.
    let (category, a, b) = (upper(lang, &q.category.label_in(lang)), q.a.clone(), q.b.clone());
    let closed = result.is_some();
    let png = app
        .render(move |r| {
            r.rather_card(&RatherCard {
                lang,
                category: &category,
                a: &a,
                b: &b,
                votes: result,
                closed,
                theme: theme(),
            })
        })
        .await?;
    let description = match result {
        Some((a, b)) => t!(lang, "rather.closed", count = a + b),
        None => t!(lang, "rather.open", when = relative(game.deadline)),
    };
    let embed = EmbedBuilder::new()
        .color(if closed { SUCCESS } else { BRAND })
        .title(t!(lang, "rather.title", emoji = Party::WouldYouRather.emoji()))
        .description(description);
    let buttons = vec![
        button(
            format!("wy:a:{}", game.id),
            &label("A", &q.a),
            ButtonStyle::Danger,
            closed,
        ),
        button(
            format!("wy:b:{}", game.id),
            &label("B", &q.b),
            ButtonStyle::Success,
            closed,
        ),
    ];
    Ok(card(None, embed, "rather.png", png, vec![row(buttons)]))
}

/// `wy:<a|b>:<id>`: a vote.
pub(crate) async fn on_button(cx: &Cx, mut parts: Split<'_, char>) -> anyhow::Result<()> {
    let lang = cx.lang();
    let side = match parts.next() {
        Some("a") => 0,
        Some("b") => 1,
        _ => return cx.reply_ephemeral(t!(lang, "party.expired-button")).await,
    };
    let Some(id) = parts.next().and_then(|v| v.parse::<i64>().ok()) else {
        return cx.reply_ephemeral(t!(lang, "party.expired-button")).await;
    };
    let app = &cx.app;
    let me = cx.author()?.id.get();
    let Some(game) = app.db.party(id).await? else {
        return cx.reply_ephemeral(t!(lang, "rather.expired")).await;
    };
    remember(cx, &game).await;
    let q = question(&game)?;
    let chosen = if side == 0 { &q.a } else { &q.b };
    let letter = if side == 0 { "A" } else { "B" };
    let mut said = match app.db.put_poll_vote(id, me, side, app.now()).await? {
        PollChoice::Closed => {
            if game.is_playing() && game.late(app.now()) {
                close(app, game).await?;
            }
            return cx.reply_ephemeral(t!(lang, "rather.poll-closed")).await;
        }
        PollChoice::First => {
            let paid = pay_win(app, me, Game::WouldYouRather, POLL_VOTE_REWARD, "wyr_vote").await?;
            let note = if paid > 0 {
                t!(lang, "rather.paid", paid = paid)
            } else {
                String::new()
            };
            format!("{}{note}", t!(lang, "rather.picked", letter = letter, chosen = chosen))
        }
        PollChoice::Changed => t!(lang, "rather.moved", letter = letter, chosen = chosen),
        PollChoice::Same => t!(lang, "rather.same", letter = letter, chosen = chosen),
    };
    let (a, b) = app.db.poll_counts(id).await?;
    let total = a + b;
    said.push_str(&format!(
        "\n{}",
        t!(
            lang,
            "rather.so-far",
            a = RatherCard::percent(a, total),
            b = RatherCard::percent(b, total),
            count = total
        )
    ));
    cx.reply_ephemeral(said).await
}

/// Closes a poll: shows the split, switches the buttons off and counts the game for everyone who voted
/// (a win for being on the bigger side, or on either side of a tie). False when it was already closed.
pub(crate) async fn close(app: &Arc<App>, game: PartyGame) -> anyhow::Result<bool> {
    let q = question(&game)?;
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
    let (a, b) = app.db.poll_counts(game.id).await?;
    for (voter, side) in app.db.poll_voters(game.id).await? {
        let majority = if side == 0 { a >= b } else { b >= a };
        record(app, Game::WouldYouRather, voter, majority).await;
    }
    edit_card(app, &done.game, &view(app, &done.game, q, Some((a, b))).await?).await;
    Ok(true)
}

/// The clock: a poll whose time is up closes.
pub(crate) async fn on_due(app: &Arc<App>, game: PartyGame) -> anyhow::Result<bool> {
    if game.is_playing() && game.late(app.now()) {
        return close(app, game).await;
    }
    Ok(false)
}
