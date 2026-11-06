//! `/play shells`: three shells, one pearl, one guess. A gambling-style game, so it is off until a
//! manager switches it on with `/config gambling`, with low stakes and daily limits on plays, wins and
//! losses. The sea keeps a sixth of every stake on average.

use std::{str::Split, sync::Arc};

use pinchora_core::{i18n::Lang, t};
use pinchora_db::{GameSession, NewGame, ShellDone, ShellLimits, ShellPick, ShellStart};
use pinchora_game::{
    party::shells::{
        COOLDOWN_SECS, DAILY_LOSS_LIMIT, DAILY_PLAYS, DAILY_WIN_LIMIT, GUESS_SECS, MAX_STAKE, MIN_STAKE, SHELLS,
        payout, pearl,
    },
    play::Game,
};
use pinchora_render::{ShellCard, Theme};
use twilight_model::{
    channel::message::{Component, component::ButtonStyle},
    http::interaction::InteractionResponseData,
};
use twilight_util::builder::embed::EmbedBuilder;

use super::{
    games::{channel_of, edit_message, location, quiet, session_lang, state_with_lang},
    party::{count, edit_card, rest, resting, track_message, wait_text_in},
};
use crate::{
    App, Cx,
    server_events::{button, row},
    util::{BRAND, DANGER, SUCCESS, mention, relative, thousands},
};

/// What a game that has not been guessed yet shows, and what a finished one shows.
struct Table {
    picked: Option<usize>,
    pearl: Option<usize>,
    /// The line on the card, translated.
    status: String,
}

/// The table as a message: the card, and the three shells to pick from while the game runs.
async fn message(
    app: &App,
    session: &GameSession,
    table: Table,
    content: String,
    lang: Lang,
) -> anyhow::Result<InteractionResponseData> {
    let (stake, picked, pearl) = (session.bet, table.picked, table.pearl);
    let status = lang.upper(&table.status);
    let png = app
        .render(move |r| {
            r.shell_card(&ShellCard {
                lang,
                stake,
                picked,
                pearl,
                status: &status,
                payout: payout(stake),
                theme: Theme::reef(),
            })
        })
        .await?;
    let over = table.pearl.is_some();
    let embed = EmbedBuilder::new()
        .color(match (over, picked == pearl) {
            (false, _) => BRAND,
            (true, true) => SUCCESS,
            (true, false) => DANGER,
        })
        .title(t!(lang, "shells.title"));
    let components = if over {
        Vec::new()
    } else {
        vec![row((0..SHELLS)
            .map(|n| {
                button(
                    format!("sz:p:{}:{n}", session.id),
                    &t!(lang, "shells.shell-button", n = n + 1),
                    Some("🐚"),
                    ButtonStyle::Secondary,
                    false,
                )
            })
            .collect())]
    };
    let mut data = Cx::card(embed, "shells.png", png, components, false);
    data.content = Some(content);
    data.allowed_mentions = quiet();
    Ok(data)
}

fn limits() -> ShellLimits {
    ShellLimits {
        plays: DAILY_PLAYS,
        loss: DAILY_LOSS_LIMIT,
        win: DAILY_WIN_LIMIT,
    }
}

/// `/play shells`: sits the player down at the table.
pub(crate) async fn start(cx: &Cx, stake: i64) -> anyhow::Result<()> {
    let me = cx.author()?.id.get();
    let lang = cx.lang();
    let Some(guild) = cx.guild_id() else {
        return cx.reply_ephemeral(t!(lang, "shells.server-only")).await;
    };
    if !cx.app.db.gambling_enabled(guild).await? {
        return cx.reply_ephemeral(t!(lang, "shells.gambling-off")).await;
    }
    if !(MIN_STAKE..=MAX_STAKE).contains(&stake) {
        return cx
            .reply_ephemeral(t!(lang, "shells.stake-range", min = MIN_STAKE, max = MAX_STAKE))
            .await;
    }
    if let Some(left) = resting(&cx.app, "shellsstart", me).await? {
        return cx
            .reply_ephemeral(t!(lang, "shells.shuffling", wait = wait_text_in(lang, left)))
            .await;
    }
    let seed: u64 = rand::random();
    // The table is public and keeps speaking the server's language to its end; `lang` is for the player's refusals.
    let board_lang = cx.app.guild_lang(guild).await;
    let started = cx
        .app
        .db
        .shell_start(
            NewGame {
                kind: "shellgame",
                guild_id: guild,
                channel_id: channel_of(cx),
                host: me,
                guest: None,
                bet: stake,
                status: pinchora_db::ACTIVE,
                state: &state_with_lang(&pearl(seed).to_string(), board_lang),
                ttl_secs: GUESS_SECS,
            },
            limits(),
        )
        .await?;
    let session = match started {
        ShellStart::Started(session) => {
            rest(&cx.app, "shellsstart", me, COOLDOWN_SECS).await;
            session
        }
        ShellStart::Broke { balance } => {
            return cx
                .reply_ephemeral(t!(
                    lang,
                    "shells.broke",
                    balance = thousands(balance),
                    stake = thousands(stake)
                ))
                .await;
        }
        ShellStart::TooMany => {
            return cx
                .reply_ephemeral(t!(lang, "shells.too-many", plays = DAILY_PLAYS))
                .await;
        }
        ShellStart::Down { net } => {
            return cx
                .reply_ephemeral(t!(lang, "shells.down", limit = DAILY_LOSS_LIMIT, net = thousands(net)))
                .await;
        }
        ShellStart::Up { net } => {
            return cx.reply_ephemeral(t!(lang, "shells.up", net = thousands(net))).await;
        }
    };
    let content = t!(
        board_lang,
        "shells.sits",
        player = mention(me),
        stake = thousands(stake),
        when = relative(session.expires_at),
        payout = thousands(payout(stake))
    );
    let data = message(
        &cx.app,
        &session,
        Table {
            picked: None,
            pearl: None,
            status: t!(board_lang, "shells.status-start"),
        },
        content,
        board_lang,
    )
    .await?;
    cx.reply(data).await?;
    track_message(cx, session.id);
    Ok(())
}

/// `sz:p:<id>:<shell>` picks a shell.
pub(crate) async fn on_button(cx: &Cx, mut args: Split<'_, char>) -> anyhow::Result<()> {
    let (Some("p"), Some(id), Some(shell)) = (
        args.next(),
        args.next().and_then(|v| v.parse::<i64>().ok()),
        args.next()
            .and_then(|v| v.parse::<u8>().ok())
            .filter(|s| usize::from(*s) < SHELLS),
    ) else {
        return cx.reply_ephemeral(t!(cx.lang(), "arcade.button-expired")).await;
    };
    let me = cx.author()?.id.get();
    let lang = cx.lang();
    match cx.app.db.shell_pick(id, me, shell, payout).await? {
        ShellPick::Done(done) => {
            count(&cx.app, Game::ShellGame, &[(me, done.paid > 0)]).await;
            // The table is the server's language, whoever clicks.
            let table_lang = session_lang(&cx.app, &done.session).await;
            let (table, content) = ending(&done, table_lang);
            let data = message(&cx.app, &done.session, table, content, table_lang).await?;
            cx.update(data).await
        }
        ShellPick::NotYours => cx.reply_ephemeral(t!(lang, "shells.not-yours")).await,
        ShellPick::Gone => {
            // Out of time: give the stake back right here instead of waiting for the janitor.
            match cx.app.db.shell_refund(id).await? {
                Some(session) => {
                    let table_lang = session_lang(&cx.app, &session).await;
                    cx.update(timeout_message(&cx.app, &session, table_lang).await?).await
                }
                None => cx.reply_ephemeral(t!(lang, "shells.over")).await,
            }
        }
    }
}

/// What the table shows once the guess is in, and the line under it.
fn ending(done: &ShellDone, lang: Lang) -> (Table, String) {
    let player = mention(done.session.host_id);
    let stake = thousands(done.session.bet);
    if done.paid > 0 {
        (
            Table {
                picked: Some(usize::from(done.picked)),
                pearl: Some(usize::from(done.pearl)),
                status: t!(lang, "shells.status-found"),
            },
            t!(
                lang,
                "shells.found",
                player = player,
                pearl = done.pearl + 1,
                paid = thousands(done.paid),
                stake = stake
            ),
        )
    } else {
        (
            Table {
                picked: Some(usize::from(done.picked)),
                pearl: Some(usize::from(done.pearl)),
                status: t!(lang, "shells.status-missed"),
            },
            t!(
                lang,
                "shells.missed",
                player = player,
                picked = done.picked + 1,
                pearl = done.pearl + 1,
                stake = stake
            ),
        )
    }
}

async fn timeout_message(app: &App, session: &GameSession, lang: Lang) -> anyhow::Result<InteractionResponseData> {
    let content = t!(
        lang,
        "shells.timeout",
        player = mention(session.host_id),
        stake = thousands(session.bet)
    );
    // The pearl stays hidden: nothing was guessed.
    let mut data = message(
        app,
        session,
        Table {
            picked: None,
            pearl: None,
            status: t!(lang, "shells.status-returned"),
        },
        content,
        lang,
    )
    .await?;
    data.components = Some(Vec::<Component>::new());
    Ok(data)
}

/// Gives the stake back for a table nobody guessed at; the janitor's end for a late game.
pub(crate) async fn time_out(app: &Arc<App>, session: &GameSession) -> anyhow::Result<bool> {
    let Some(before) = app.db.shell_refund(session.id).await? else {
        return Ok(false);
    };
    if let Some(at) = location(session) {
        let lang = session_lang(app, &before).await;
        match timeout_message(app, &before, lang).await {
            Ok(data) => {
                if let (Some(embeds), Some(files)) = (data.embeds, data.attachments)
                    && let (Some(embed), Some(file)) = (embeds.into_iter().next(), files.into_iter().next())
                {
                    edit_card(
                        app,
                        at,
                        data.content.as_deref(),
                        embed,
                        (&file.filename, file.file),
                        &[],
                    )
                    .await;
                } else {
                    edit_message(app, at.0, at.1, data.content.as_deref(), None, Some(&[])).await;
                }
            }
            Err(error) => tracing::warn!(%error, "shell game card not drawn"),
        }
    }
    Ok(true)
}

#[cfg(test)]
mod tests {
    use chrono::Utc;

    use super::*;

    fn done(picked: u8, pearl: u8, paid: i64) -> ShellDone {
        ShellDone {
            session: GameSession {
                id: 1,
                kind: "shellgame".into(),
                guild_id: 1,
                channel_id: 1,
                message_id: None,
                host_id: 7,
                guest_id: None,
                bet: 20,
                status: "done".into(),
                state: String::new(),
                expires_at: Utc::now(),
            },
            pearl,
            picked,
            paid,
        }
    }

    #[test]
    fn a_win_and_a_loss_say_where_the_pearl_was() {
        let (table, line) = ending(&done(1, 1, 50), Lang::En);
        assert_eq!((table.picked, table.pearl), (Some(1), Some(1)));
        assert!(line.contains("found the pearl under shell 2") && line.contains("**50** shells"));
        let (table, line) = ending(&done(0, 2, 0), Lang::En);
        assert_eq!((table.picked, table.pearl), (Some(0), Some(2)));
        assert!(line.contains("picked shell 1") && line.contains("under shell 3") && line.contains("keeps"));
    }
}
