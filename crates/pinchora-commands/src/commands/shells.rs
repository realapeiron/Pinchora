//! `/play shells`: three shells, one pearl, one guess. A gambling-style game, so it is off until a
//! manager switches it on with `/config gambling`, with low stakes and daily limits on plays, wins and
//! losses. The sea keeps a sixth of every stake on average.

use std::{str::Split, sync::Arc};

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
    games::{channel_of, edit_message, location, quiet},
    party::{count, edit_card, rest, resting, track_message, wait_text},
};
use crate::{
    App, Cx,
    server_events::{button, row},
    util::{BRAND, DANGER, SUCCESS, mention, relative, thousands},
};

/// What a game that has not been guessed yet shows, and what a finished one shows.
struct Table<'a> {
    picked: Option<usize>,
    pearl: Option<usize>,
    status: &'a str,
}

/// The table as a message: the card, and the three shells to pick from while the game runs.
async fn message(
    app: &App,
    session: &GameSession,
    table: Table<'_>,
    content: String,
) -> anyhow::Result<InteractionResponseData> {
    let (stake, picked, pearl) = (session.bet, table.picked, table.pearl);
    let status = table.status.to_owned();
    let png = app
        .render(move |r| {
            r.shell_card(&ShellCard {
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
        .title("Shell game");
    let components = if over {
        Vec::new()
    } else {
        vec![row((0..SHELLS)
            .map(|n| {
                button(
                    format!("sz:p:{}:{n}", session.id),
                    &format!("Shell {}", n + 1),
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
    let Some(guild) = cx.guild_id() else {
        return cx.reply_ephemeral("The shell table is inside a server.").await;
    };
    if !cx.app.db.gambling_enabled(guild).await? {
        return cx
            .reply_ephemeral(
                "🎲 Gambling-style games are **off** on this server. A manager can switch them on with `/config gambling`.",
            )
            .await;
    }
    if !(MIN_STAKE..=MAX_STAKE).contains(&stake) {
        return cx
            .reply_ephemeral(format!(
                "A stake at the shell table is {MIN_STAKE} to {MAX_STAKE} shells."
            ))
            .await;
    }
    if let Some(left) = resting(&cx.app, "shellsstart", me).await? {
        return cx
            .reply_ephemeral(format!(
                "The crab is still shuffling. Sit down again in {}.",
                wait_text(left)
            ))
            .await;
    }
    let seed: u64 = rand::random();
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
                state: &pearl(seed).to_string(),
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
                .reply_ephemeral(format!(
                    "You only have **{}** shells, so you can't stake {}. Try `/daily`!",
                    thousands(balance),
                    thousands(stake)
                ))
                .await;
        }
        ShellStart::TooMany => {
            return cx
                .reply_ephemeral(format!(
                    "You've played {DAILY_PLAYS} games at the table today. The crab needs a nap; come back tomorrow."
                ))
                .await;
        }
        ShellStart::Down { net } => {
            return cx
                .reply_ephemeral(format!(
                    "That stake would take you past today's loss limit of **{DAILY_LOSS_LIMIT}** shells \
                     (you're at {}). Try a smaller one, or come back tomorrow.",
                    thousands(net)
                ))
                .await;
        }
        ShellStart::Up { net } => {
            return cx
                .reply_ephemeral(format!(
                    "You're up **{}** shells today, as far as the table lets anyone go. Come back tomorrow!",
                    thousands(net)
                ))
                .await;
        }
    };
    let content = format!(
        "🐚 {} sits down at the shell table with **{}** shells. The pearl is under one shell: pick one {}! \
         A right guess pays **{}** shells.",
        mention(me),
        thousands(stake),
        relative(session.expires_at),
        thousands(payout(stake))
    );
    let data = message(
        &cx.app,
        &session,
        Table {
            picked: None,
            pearl: None,
            status: "Where is the pearl?",
        },
        content,
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
        return cx.reply_ephemeral("This button has expired.").await;
    };
    let me = cx.author()?.id.get();
    match cx.app.db.shell_pick(id, me, shell, payout).await? {
        ShellPick::Done(done) => {
            count(&cx.app, Game::ShellGame, &[(me, done.paid > 0)]).await;
            let (table, content) = ending(&done);
            let data = message(&cx.app, &done.session, table, content).await?;
            cx.update(data).await
        }
        ShellPick::NotYours => {
            cx.reply_ephemeral("This isn't your table. Sit down at your own with `/play shells`!")
                .await
        }
        ShellPick::Gone => {
            // Out of time: give the stake back right here instead of waiting for the janitor.
            match cx.app.db.shell_refund(id).await? {
                Some(session) => cx.update(timeout_message(&cx.app, &session).await?).await,
                None => cx.reply_ephemeral("This game is over.").await,
            }
        }
    }
}

/// What the table shows once the guess is in, and the line under it.
fn ending(done: &ShellDone) -> (Table<'static>, String) {
    let player = mention(done.session.host_id);
    let stake = thousands(done.session.bet);
    if done.paid > 0 {
        (
            Table {
                picked: Some(usize::from(done.picked)),
                pearl: Some(usize::from(done.pearl)),
                status: "You found the pearl!",
            },
            format!(
                "🎉 {player} found the pearl under shell {} and takes **{}** shells for the **{stake}** staked!",
                done.pearl + 1,
                thousands(done.paid)
            ),
        )
    } else {
        (
            Table {
                picked: Some(usize::from(done.picked)),
                pearl: Some(usize::from(done.pearl)),
                status: "Not under that one",
            },
            format!(
                "🐚 {player} picked shell {}, but the pearl was under shell {}. The sea keeps the **{stake}** shells.",
                done.picked + 1,
                done.pearl + 1
            ),
        )
    }
}

async fn timeout_message(app: &App, session: &GameSession) -> anyhow::Result<InteractionResponseData> {
    let content = format!(
        "⌛ {} never picked a shell, so the **{}** shells staked went back.",
        mention(session.host_id),
        thousands(session.bet)
    );
    // The pearl stays hidden: nothing was guessed.
    let mut data = message(
        app,
        session,
        Table {
            picked: None,
            pearl: None,
            status: "Stake returned",
        },
        content,
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
        match timeout_message(app, &before).await {
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
        let (table, line) = ending(&done(1, 1, 50));
        assert_eq!((table.picked, table.pearl), (Some(1), Some(1)));
        assert!(line.contains("found the pearl under shell 2") && line.contains("**50** shells"));
        let (table, line) = ending(&done(0, 2, 0));
        assert_eq!((table.picked, table.pearl), (Some(0), Some(2)));
        assert!(line.contains("picked shell 1") && line.contains("under shell 3") && line.contains("keeps"));
    }
}
