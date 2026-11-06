//! `/play trivia`: a question in the channel, four buttons, 20 seconds, anyone can answer once.

use std::{str::Split, sync::Arc, time::Duration};

use chrono::{Duration as Span, Utc};
use pinchora_core::t;
use pinchora_db::{ACTIVE, NewGame, Payout};
use pinchora_game::play::{
    Game, PlayContent, Question, TriviaCategory, find_question,
    trivia::{decode_order, encode_order, permutation},
};
use twilight_model::{
    channel::message::{
        Component, Embed,
        component::{ActionRow, ButtonStyle},
    },
    http::interaction::InteractionResponseData,
};
use twilight_util::builder::embed::{EmbedBuilder, EmbedFieldBuilder};

use super::games::{button, channel_of, edit_message, location, pay_win, quiet, record, track_message};
use crate::{
    App, Cx, Responder,
    util::{BRAND, SUCCESS, mention, relative},
};

/// How long a round takes.
pub(crate) const ROUND_SECS: i64 = 20;
const LETTERS: [char; 4] = ['A', 'B', 'C', 'D'];
/// Winners named on the reveal; an embed field holds 1,024 characters.
const MAX_LISTED: usize = 25;

/// A session's state: the question id and the order its options are shown in.
fn decode(state: &str) -> Option<(&'static Question, [usize; 4])> {
    let (id, order) = state.split_once(':')?;
    Some((find_question(id)?, decode_order(order)?))
}

pub(crate) async fn start(cx: &Cx, category: Option<TriviaCategory>) -> anyhow::Result<()> {
    let me = cx.author()?.id.get();
    let channel = channel_of(cx);
    let lang = cx.lang();
    if let Some(left) = cx
        .app
        .cache
        .try_cooldown(
            &format!("trivia:{channel}"),
            Duration::from_secs(ROUND_SECS as u64 + 10),
        )
        .await?
    {
        return cx
            .reply_ephemeral(t!(lang, "trivia.cooldown", secs = left.as_secs() + 1))
            .await;
    }
    let pool: Vec<&Question> = PlayContent::for_lang(lang)
        .trivia
        .iter()
        .filter(|q| category.is_none_or(|c| q.category == c))
        .collect();
    anyhow::ensure!(!pool.is_empty(), "no trivia questions in this category");
    // Skip questions this channel saw recently, a few tries deep.
    let mut question = pool[rand::random_range(0..pool.len())];
    for _ in 0..6 {
        let candidate = pool[rand::random_range(0..pool.len())];
        question = candidate;
        if cx
            .app
            .cache
            .once(
                &format!("triviaseen:{channel}:{}", candidate.id),
                Duration::from_secs(6 * 3600),
            )
            .await?
        {
            break;
        }
    }
    let order = permutation(rand::random());
    let session = cx
        .app
        .db
        .create_game(NewGame {
            kind: "trivia",
            guild_id: cx.guild_id().unwrap_or(0),
            channel_id: channel,
            host: me,
            guest: None,
            bet: 0,
            status: ACTIVE,
            state: &format!("{}:{}", question.id, encode_order(order)),
            ttl_secs: ROUND_SECS,
        })
        .await?;
    let deadline = Utc::now() + Span::seconds(ROUND_SECS);
    cx.reply(InteractionResponseData {
        content: Some(t!(
            lang,
            "trivia.started",
            user = mention(me),
            when = relative(deadline)
        )),
        embeds: Some(vec![question_embed(question, order, None)]),
        components: Some(answer_row(session.id, question, order, None)),
        allowed_mentions: quiet(),
        ..Default::default()
    })
    .await?;
    track_message(cx, session.id);

    // The round ends by itself; the janitor does it if this pod is gone by then.
    let (app, responder, id) = (cx.app.clone(), cx.responder.clone(), session.id);
    cx.app.spawn(async move {
        tokio::time::sleep(Duration::from_secs(ROUND_SECS as u64)).await;
        if let Err(error) = finish(&app, id, Some(responder)).await {
            tracing::warn!(%error, session = id, "trivia round not finished");
        }
    });
    Ok(())
}

/// The question with its options; once revealed, the right one is marked.
fn question_embed(q: &Question, order: [usize; 4], reveal: Option<&Reveal>) -> Embed {
    let shown = q.shown(order);
    let right = q.correct_slot(order);
    let options: Vec<String> = shown
        .iter()
        .enumerate()
        .map(|(i, text)| match reveal {
            Some(_) if Some(i) == right => format!("✅ **{}.** **{text}**", LETTERS[i]),
            _ => format!("**{}.** {text}", LETTERS[i]),
        })
        .collect();
    let mut embed = EmbedBuilder::new()
        .color(if reveal.is_some() { SUCCESS } else { BRAND })
        .title(format!(
            "{} {} · {}",
            q.category.emoji(),
            q.category.label_in(q.lang),
            q.difficulty.label_in(q.lang)
        ))
        .description(format!("**{}**\n\n{}", q.question, options.join("\n")));
    if let Some(reveal) = reveal {
        embed = embed.field(EmbedFieldBuilder::new(
            t!(q.lang, "trivia.got-it"),
            reveal.winners.clone(),
        ));
    }
    embed.build()
}

/// The four answer buttons. Revealed, they are disabled and the right one is green.
fn answer_row(id: i64, q: &Question, order: [usize; 4], reveal: Option<&Reveal>) -> Vec<Component> {
    let right = q.correct_slot(order);
    vec![Component::ActionRow(ActionRow {
        id: None,
        components: q
            .shown(order)
            .iter()
            .enumerate()
            .map(|(i, text)| {
                let label: String = format!("{} · {text}", LETTERS[i]).chars().take(40).collect();
                let style = match reveal {
                    None => ButtonStyle::Primary,
                    Some(_) if Some(i) == right => ButtonStyle::Success,
                    Some(_) => ButtonStyle::Secondary,
                };
                button(format!("tv:{id}:{i}"), &label, style, reveal.is_some())
            })
            .collect(),
    })]
}

/// What the round turned out like.
struct Reveal {
    /// The mentions of those who were right, with what they earned.
    winners: String,
    answered: usize,
}

/// `tv:<id>:<slot>`: an answer.
pub(crate) async fn on_answer(cx: &Cx, mut args: Split<'_, char>) -> anyhow::Result<()> {
    let me = cx.author()?.id.get();
    let (Some(id), Some(slot)) = (
        args.next().and_then(|v| v.parse::<i64>().ok()),
        args.next()
            .and_then(|v| v.parse::<i16>().ok())
            .filter(|s| (0..4).contains(s)),
    ) else {
        return cx.reply_ephemeral(t!(cx.lang(), "trivia.expired")).await;
    };
    // Only a trivia round: another game's id would pass another module's switch.
    let Some(session) = cx
        .app
        .db
        .game(id)
        .await?
        .filter(|s| s.kind == "trivia" && s.is_active() && !s.expired())
    else {
        return cx.reply_ephemeral(t!(cx.lang(), "trivia.time-up")).await;
    };
    if session.message_id.is_none()
        && let Some((channel, message)) = cx.clicked_message()
    {
        cx.app.db.set_game_message(id, channel, message).await?;
    }
    if !cx.app.db.add_game_answer(id, me, slot).await? {
        return cx.reply_ephemeral(t!(cx.lang(), "trivia.already-answered")).await;
    }
    cx.reply_ephemeral(t!(cx.lang(), "trivia.locked", letter = LETTERS[slot as usize]))
        .await
}

/// Ends a round: shows the answer, pays the right answers (up to the daily limit) and counts
/// everyone's game. Exactly one caller gets to do it; `fallback` edits the round's own message if
/// the bot cannot reach it by id. False when someone else ended the round first.
pub(crate) async fn finish(app: &Arc<App>, id: i64, fallback: Option<Arc<Responder>>) -> anyhow::Result<bool> {
    let Some(session) = app.db.settle_game(id, None, "done", Payout::None, false).await? else {
        return Ok(false);
    };
    let Some((q, order)) = decode(&session.state) else {
        anyhow::bail!("bad trivia state {:?}", session.state);
    };
    let lang = q.lang;
    let right = q.correct_slot(order).map(|s| s as i16);
    let answers = app.db.game_answers(id).await?;
    let mut winners = Vec::new();
    for (user, slot) in &answers {
        let correct = Some(*slot) == right;
        record(app, Game::Trivia, *user, correct).await;
        if correct {
            let paid = pay_win(app, *user, Game::Trivia, q.difficulty.reward(), "trivia").await?;
            winners.push(if paid > 0 {
                format!("{} +{paid}", mention(user))
            } else {
                mention(user)
            });
        }
    }
    let reveal = Reveal {
        winners: if winners.len() > MAX_LISTED {
            let more = winners.len() - MAX_LISTED;
            winners.truncate(MAX_LISTED);
            format!("{}\n{}", winners.join("\n"), t!(lang, "trivia.and-more", more = more))
        } else if winners.is_empty() {
            if answers.is_empty() {
                t!(lang, "trivia.nobody-answered")
            } else {
                t!(lang, "trivia.nobody-right")
            }
        } else {
            winners.join("\n")
        },
        answered: answers.len(),
    };
    let content = t!(lang, "trivia.reveal", answered = reveal.answered, answer = q.answer);
    let embeds = [question_embed(q, order, Some(&reveal))];
    let components = answer_row(id, q, order, Some(&reveal));
    let mut edited = false;
    if let Some(responder) = &fallback {
        let data = InteractionResponseData {
            content: Some(content.clone()),
            embeds: Some(embeds.to_vec()),
            components: Some(components.clone()),
            ..Default::default()
        };
        edited = responder.edit(&data).await.is_ok();
    }
    if !edited && let Some((channel, message)) = location(&session) {
        edit_message(app, channel, message, Some(&content), Some(&embeds), Some(&components)).await;
    }
    Ok(true)
}
