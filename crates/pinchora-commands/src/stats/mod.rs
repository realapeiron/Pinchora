//! Server stats: counting what members do (messages, voice minutes, joins, leaves, commands) without
//! reading any message text, flushing the counts from Redis to Postgres once a minute, forgetting
//! them after 90 days, and keeping the counter channels' names up to date. The slash commands are in
//! `commands/stats.rs`; the rules in `pinchora_core::server_stats`.
//!
//! Counting is one Redis round trip per message and never fails its caller. The ticker runs on every
//! pod; a per-minute claim keeps the flush to one pod at a time, and a flush cut short is finished by
//! the next one without counting anything twice (see `pinchora_cache::StatsBatch`).

pub(crate) mod cards;
pub(crate) mod counters;

use std::{collections::HashMap, sync::Arc, time::Duration};

use chrono::Duration as Span;
use pinchora_core::{
    leveling::VoicePresence,
    modules::Module,
    server_stats::{self, HourTally, KEEP_DAYS, field},
};
use twilight_model::channel::Message;

use crate::{App, Cx};

/// The ticker wakes this often; the first wake of each minute flushes.
const WAKE: Duration = Duration::from_secs(20);
/// How long one minute's flush stays claimed by the pod that took it.
const FLUSH_CLAIM: Duration = Duration::from_secs(90);
/// Hashes sealed per flush, and batches written per flush, at most: a backlog drains over a few minutes.
const SEAL_LIMIT: usize = 2_000;
const WRITE_LIMIT: usize = 2_000;
/// How many servers' counters one tick works out at most.
const COUNTER_GUILDS: i64 = 25;

/// Counts a message from a member in a server, in the channel it was written in (a thread's in the
/// channel the thread belongs to). Nothing happens where the stats module is switched off.
pub(crate) async fn count_message(app: &Arc<App>, msg: &Message) {
    let Some(guild) = msg.guild_id.map(|g| g.get()) else {
        return;
    };
    let channel = home(app, msg.channel_id.get()).await;
    if !app.module_allowed(guild, Some(channel), Module::Stats).await {
        return;
    }
    let fields = server_stats::message_fields(channel, msg.author.id.get());
    add(app, guild, &fields).await;
}

/// Counts a member joining (or, with `joined` false, leaving) a server.
pub(crate) async fn count_member(app: &Arc<App>, guild: u64, joined: bool) {
    if !app.module_allowed(guild, None, Module::Stats).await {
        return;
    }
    let name = if joined { field::JOINS } else { field::LEAVES };
    add(app, guild, &[(name.to_owned(), 1)]).await;
}

/// Counts a slash or text command that passed its module's gate.
pub(crate) async fn count_command(cx: &Cx) {
    let Some(guild) = cx.guild_id() else { return };
    let channel = cx.interaction.channel.as_ref().map(|c| c.id.get());
    if !cx.app.module_allowed(guild, channel, Module::Stats).await {
        return;
    }
    add(&cx.app, guild, &[(field::COMMANDS.to_owned(), 1)]).await;
}

/// Counts one minute of everyone sitting in voice in a server, outside its AFK channel. The voice XP
/// ticker calls this once a minute for each server where it follows who is in voice.
pub(crate) async fn count_voice(app: &Arc<App>, guild: u64, present: &[VoicePresence], afk: Option<u64>) {
    let mut allowed: HashMap<u64, bool> = HashMap::new();
    let mut here = Vec::with_capacity(present.len());
    for member in present.iter().filter(|m| m.channel != 0 && Some(m.channel) != afk) {
        let on = match allowed.get(&member.channel) {
            Some(on) => *on,
            None => {
                let on = app.module_allowed(guild, Some(member.channel), Module::Stats).await;
                allowed.insert(member.channel, on);
                on
            }
        };
        if on {
            here.push((member.user, member.channel));
        }
    }
    add(app, guild, &server_stats::voice_fields(&here)).await;
}

/// The channel a message counts in: a thread's own channel, else the channel itself. Where a channel sits
/// is known from GUILD_CREATE (threads included); counting never asks Discord, so a thread the bot has
/// not heard of yet counts on its own.
async fn home(app: &App, channel: u64) -> u64 {
    match app.known_channel_place(channel).await {
        Some(place) if place.thread => place.parent.unwrap_or(channel),
        _ => channel,
    }
}

async fn add(app: &App, guild: u64, fields: &[(String, i64)]) {
    let hour = server_stats::hour_of(app.now());
    if let Err(error) = app.cache.stats_add(guild, hour, fields).await {
        tracing::debug!(%error, guild, "stats not counted");
    }
}

/// The number a counter of `kind` (counting `role`, for a role's members) shows in `guild` now: `Err` with
/// a [`counters::problem`] key when it cannot show one, `Err(None)` when Discord did not answer.
pub async fn counter_value(
    app: &App,
    guild: u64,
    kind: server_stats::CounterKind,
    role: Option<u64>,
) -> Result<i64, Option<&'static str>> {
    match counters::values(app, guild, &[(kind, role)])
        .await
        .remove(&(kind, role))
    {
        Some(counters::Value::Number(n)) => Ok(n),
        Some(counters::Value::Problem(key)) => Err(Some(key)),
        _ => Err(None),
    }
}

/// Works out every counter of `guild` now and renames the ones that changed (a new one at once);
/// returns how many were renamed.
pub async fn refresh_counters(app: &App, guild: u64) -> anyhow::Result<usize> {
    let rows = app.db.counters(guild).await?;
    Ok(counters::refresh(app, guild, &rows).await)
}

pub use counters::problem_text as counter_problem;

/// Writes what was counted since the last flush to Postgres; returns how many batches went in. Safe
/// to run on several pods at once and to run again after one was cut short.
pub async fn flush(app: &App) -> anyhow::Result<usize> {
    app.cache.stats_seal(SEAL_LIMIT).await?;
    let mut written = 0;
    for batch in app.cache.stats_pending(WRITE_LIMIT).await? {
        let tally = HourTally::from_fields(&batch.fields);
        let hour = server_stats::hour_start(batch.hour);
        if !tally.is_empty() && app.db.write_stats_batch(&batch.key, batch.guild, hour, &tally).await? {
            written += 1;
        }
        app.cache.stats_done(&batch.key).await?;
    }
    if written > 0 {
        metrics::counter!("pinchora_stats_batches_total").increment(written as u64);
    }
    Ok(written)
}

/// Forgets what is older than [`KEEP_DAYS`]; returns how many rows went.
pub async fn sweep(app: &App) -> anyhow::Result<u64> {
    let before = app.now().date_naive() - Span::days(KEEP_DAYS);
    let mut gone = 0;
    // A few thousand rows a table per statement; a big backlog goes in a few rounds.
    for _ in 0..20 {
        let round = app.db.sweep_stats(before).await?;
        gone += round;
        if round == 0 {
            break;
        }
    }
    Ok(gone)
}

/// Runs the stats work for the life of the process: the flush every minute, the counter channels as
/// they come due, and once an hour the sweep and the bot's daily totals.
pub fn spawn_ticker(app: Arc<App>) {
    tokio::spawn(async move {
        let mut wake = tokio::time::interval(WAKE);
        wake.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        let mut last_minute = 0;
        loop {
            wake.tick().await;
            let minute = app.now().timestamp().div_euclid(60);
            if minute == last_minute {
                continue;
            }
            last_minute = minute;
            match app.cache.once(&format!("stats-flush:{minute}"), FLUSH_CLAIM).await {
                Ok(true) => {
                    if let Err(error) = flush(&app).await {
                        tracing::warn!(%error, "stats flush failed");
                    }
                }
                Ok(false) => {}
                Err(error) => tracing::warn!(%error, "stats flush not claimed"),
            }
            if let Err(error) = Box::pin(counters::tick(&app, COUNTER_GUILDS)).await {
                tracing::warn!(%error, "counter channels not updated");
            }
            let hour = minute.div_euclid(60);
            if matches!(
                app.cache
                    .once(&format!("stats-hourly:{hour}"), Duration::from_secs(7_200))
                    .await,
                Ok(true)
            ) {
                match sweep(&app).await {
                    Ok(gone) if gone > 0 => tracing::debug!(rows = gone, "old stats forgotten"),
                    Ok(_) => {}
                    Err(error) => tracing::warn!(%error, "old stats not forgotten"),
                }
                if let Err(error) = app.db.snapshot_global_stats(app.now().date_naive()).await {
                    tracing::warn!(%error, "the day's totals were not written");
                }
            }
        }
    });
}
