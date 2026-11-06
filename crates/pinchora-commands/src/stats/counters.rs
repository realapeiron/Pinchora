//! Counter channels: a channel whose name shows a live number ("Members: 1,234"). Every ten minutes
//! each server's counters are worked out again with one look at the server (Discord's approximate
//! member and online counts) and only the channels whose name changes are renamed, never twice within
//! [`RENAME_GAP`](pinchora_core::server_stats::RENAME_GAP).

use std::{collections::BTreeMap, sync::Arc, time::Duration};

use pinchora_core::{
    leveling::level_of,
    modules::Module,
    server_stats::{CounterKind, MEMBER_LIST_MAX, counter_name, rename_due},
};
use pinchora_db::CounterRow;
use twilight_model::{
    channel::ChannelType,
    guild::{Guild, Permissions},
    id::Id,
};

use pinchora_core::{i18n::Lang, t};

use crate::{App, roles::error_code, text::access::channel_permissions};

const UNKNOWN_CHANNEL: u64 = 10003;
const MISSING_ACCESS: u64 = 50001;
const MISSING_PERMISSIONS: u64 = 50013;

/// Why a counter could not show its number, as stored and as the dashboard and `/counters list` word it.
pub(crate) mod problem {
    pub const VIEW_CHANNEL: &str = "view_channel";
    pub const MANAGE_CHANNELS: &str = "manage_channels";
    pub const CONNECT: &str = "connect";
    pub const TOO_MANY_MEMBERS: &str = "too_many_members";
    pub const ROLE_GONE: &str = "role_gone";
}

/// What a counter's number came to: the number, a lasting problem to show, or nothing this time
/// (Discord did not answer), in which case the counter keeps its name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Value {
    Number(i64),
    Problem(&'static str),
    Unknown,
}

/// One member as the member-list counters need them.
struct Listed {
    bot: bool,
    roles: Vec<u64>,
}

/// Works out the counters of up to `guilds` servers that are due and renames what changed; returns
/// how many channels were renamed.
pub(crate) async fn tick(app: &Arc<App>, guilds: i64) -> anyhow::Result<usize> {
    let rows = app.db.claim_counters(app.now(), guilds).await?;
    let mut by_guild: BTreeMap<i64, Vec<CounterRow>> = BTreeMap::new();
    for row in rows {
        by_guild.entry(row.guild_id).or_default().push(row);
    }
    let mut renamed = 0;
    for (guild, rows) in by_guild {
        let guild = guild as u64;
        // Switched off, a counter keeps the last number it showed.
        if !app.module_allowed(guild, None, Module::Stats).await {
            continue;
        }
        renamed += refresh(app, guild, &rows).await;
    }
    Ok(renamed)
}

/// Works out `rows` (all of one server) and renames the ones whose name changed; returns how many.
pub(crate) async fn refresh(app: &App, guild: u64, rows: &[CounterRow]) -> usize {
    let wanted: Vec<(CounterKind, Option<u64>)> = rows
        .iter()
        .filter_map(|r| CounterKind::parse(&r.kind).map(|k| (k, r.role_id.map(|id| id as u64))))
        .collect();
    let values = values(app, guild, &wanted).await;
    let mut renamed = 0;
    for row in rows {
        let Some(kind) = CounterKind::parse(&row.kind) else {
            continue;
        };
        let value = values
            .get(&(kind, row.role_id.map(|id| id as u64)))
            .copied()
            .unwrap_or(Value::Unknown);
        match apply(app, guild, row, value).await {
            Ok(true) => renamed += 1,
            Ok(false) => {}
            Err(error) => tracing::warn!(%error, guild, counter = row.id, "counter not updated"),
        }
    }
    renamed
}

/// Shows `value` on the counter `row`: renames its channel when the name changes and the last rename
/// is ten minutes old, and notes what came of it. Returns whether the channel was renamed.
pub(crate) async fn apply(app: &App, guild: u64, row: &CounterRow, value: Value) -> anyhow::Result<bool> {
    let channel = Id::new((row.channel_id as u64).max(1));
    // Reconcile deleted channels even when their calculated name did not change.
    if let Err(error) = app.http.channel(channel).await
        && error_code(&error) == Some(UNKNOWN_CHANNEL)
    {
        app.db.remove_counter(guild, row.channel_id as u64).await?;
        return Ok(false);
    }
    let number = match value {
        Value::Number(n) => n,
        Value::Problem(problem) => {
            app.db.record_counter(row.id, None, None, Some(problem)).await?;
            return Ok(false);
        }
        Value::Unknown => return Ok(false),
    };
    let now = app.now();
    let name = counter_name(&row.template, number);
    if !rename_due(row.last_name.as_deref(), row.last_rename_at, &name, now) {
        // Already showing it, nothing to complain about; or the last rename is too recent to tell.
        let problem = if row.last_name.as_deref() == Some(name.as_str()) {
            None
        } else {
            row.problem.as_deref()
        };
        app.db.record_counter(row.id, Some(number), None, problem).await?;
        return Ok(false);
    }
    if !app
        .cache
        .once(
            &format!("counter-rename:{}", row.channel_id),
            Duration::from_secs(pinchora_core::server_stats::RENAME_GAP.num_seconds() as u64),
        )
        .await?
    {
        return Ok(false);
    }
    match app.http.update_channel(channel).name(&name).await {
        Ok(_) => {
            app.db
                .record_counter(row.id, Some(number), Some((&name, now)), None)
                .await?;
            metrics::counter!("pinchora_counter_renames_total").increment(1);
            Ok(true)
        }
        Err(error) => {
            match error_code(&error) {
                Some(UNKNOWN_CHANNEL) => {
                    // The channel is gone: so is its counter.
                    app.db.remove_counter(guild, row.channel_id as u64).await?;
                }
                Some(MISSING_PERMISSIONS) => {
                    app.db
                        .record_counter(row.id, Some(number), None, Some(problem::MANAGE_CHANNELS))
                        .await?;
                }
                Some(MISSING_ACCESS) => {
                    app.db
                        .record_counter(row.id, Some(number), None, Some(problem::VIEW_CHANNEL))
                        .await?;
                }
                _ => {
                    // Discord had a bad moment: the next round tries again.
                    tracing::debug!(%error, guild, counter = row.id, "counter channel not renamed");
                    app.db
                        .record_counter(row.id, Some(number), None, row.problem.as_deref())
                        .await?;
                }
            }
            Ok(false)
        }
    }
}

/// The numbers of the `wanted` counters of a server, each asked of Discord or the database at most once.
pub(crate) async fn values(
    app: &App,
    guild: u64,
    wanted: &[(CounterKind, Option<u64>)],
) -> BTreeMap<(CounterKind, Option<u64>), Value> {
    let needs = |test: fn(CounterKind) -> bool| wanted.iter().any(|(kind, _)| test(*kind));
    let id = Id::new(guild.max(1));
    let info: Option<Guild> = if needs(|k| {
        matches!(
            k,
            CounterKind::Members | CounterKind::Online | CounterKind::Boosts | CounterKind::Roles
        ) || k.lists_members()
    }) {
        match app.http.guild(id).with_counts(true).await {
            Ok(response) => response.model().await.ok(),
            Err(error) => {
                tracing::debug!(%error, guild, "server counts unavailable");
                None
            }
        }
    } else {
        None
    };
    let channels = if needs(|k| k == CounterKind::Channels) {
        match app.http.guild_channels(id).await {
            Ok(response) => response.models().await.ok().map(|list| {
                list.iter()
                    .filter(|c| !c.kind.is_thread() && c.kind != ChannelType::GuildCategory)
                    .count() as i64
            }),
            Err(_) => None,
        }
    } else {
        None
    };
    let members = if needs(CounterKind::lists_members) {
        list_members(app, guild, info.as_ref()).await
    } else {
        Err(Value::Unknown)
    };
    let mut out = BTreeMap::new();
    for &(kind, role) in wanted {
        let value = match kind {
            CounterKind::Members => count(info.as_ref().and_then(|g| g.approximate_member_count)),
            CounterKind::Online => count(info.as_ref().and_then(|g| g.approximate_presence_count)),
            CounterKind::Boosts => count(info.as_ref().and_then(|g| g.premium_subscription_count)),
            // Without @everyone, which every server has.
            CounterKind::Roles => count(info.as_ref().map(|g| g.roles.len().saturating_sub(1) as u64)),
            CounterKind::Channels => channels.map_or(Value::Unknown, Value::Number),
            CounterKind::Humans | CounterKind::Bots | CounterKind::RoleMembers => match &members {
                Err(why) => *why,
                Ok(list) => match (kind, role) {
                    (CounterKind::Humans, _) => Value::Number(list.iter().filter(|m| !m.bot).count() as i64),
                    (CounterKind::Bots, _) => Value::Number(list.iter().filter(|m| m.bot).count() as i64),
                    (_, Some(role))
                        if info
                            .as_ref()
                            .is_some_and(|g| !g.roles.iter().any(|r| r.id.get() == role)) =>
                    {
                        Value::Problem(problem::ROLE_GONE)
                    }
                    (_, Some(role)) => Value::Number(list.iter().filter(|m| m.roles.contains(&role)).count() as i64),
                    (_, None) => Value::Problem(problem::ROLE_GONE),
                },
            },
            CounterKind::Houses => app.db.guild_houses(guild).await.map_or(Value::Unknown, Value::Number),
            CounterKind::Fish => app.db.guild_fish(guild).await.map_or(Value::Unknown, Value::Number),
            CounterKind::LevelAvg => match app.db.guild_xp(guild).await {
                Ok(xp) if xp.is_empty() => Value::Number(0),
                Ok(xp) => {
                    let total: f64 = xp.iter().map(|x| f64::from(level_of(*x))).sum();
                    Value::Number((total / xp.len() as f64).round() as i64)
                }
                Err(_) => Value::Unknown,
            },
        };
        out.insert((kind, role), value);
    }
    out
}

fn count(n: Option<u64>) -> Value {
    n.map_or(Value::Unknown, |n| Value::Number(n.min(i64::MAX as u64) as i64))
}

/// Every member of a server, a thousand per request, for servers of up to [`MEMBER_LIST_MAX`] members.
async fn list_members(app: &App, guild: u64, info: Option<&Guild>) -> Result<Vec<Listed>, Value> {
    let size = info.and_then(|g| g.approximate_member_count).ok_or(Value::Unknown)?;
    if size > MEMBER_LIST_MAX {
        return Err(Value::Problem(problem::TOO_MANY_MEMBERS));
    }
    let id = Id::new(guild.max(1));
    let mut out = Vec::with_capacity(size as usize);
    let mut after = None;
    for _ in 0..MEMBER_LIST_MAX / 1000 + 1 {
        let mut request = app.http.guild_members(id).limit(1000);
        if let Some(after) = after {
            request = request.after(after);
        }
        let page = match request.await {
            Ok(response) => response.models().await.map_err(|_| Value::Unknown)?,
            Err(error) => {
                tracing::debug!(%error, guild, "members not listed");
                return Err(Value::Unknown);
            }
        };
        let full = page.len() == 1000;
        after = page.last().map(|m| m.user.id);
        out.extend(page.into_iter().map(|m| Listed {
            bot: m.user.bot,
            roles: m.roles.iter().map(|r| r.get()).collect(),
        }));
        if !full {
            break;
        }
    }
    Ok(out)
}

/// What the bot lacks to rename `channel` (of `kind`), worded as a [`problem`]; `None` when it can, or
/// when Discord cannot tell us (the rename itself will say).
pub async fn missing_permission(app: &App, guild: u64, channel: u64, kind: ChannelType) -> Option<&'static str> {
    let facts = app.permission_facts(guild).await.ok()?;
    let rules = app.channel_rules(channel).await.ok()?;
    let bot = app.application_id.get();
    let held: Vec<u64> = if facts.owner == bot {
        Vec::new()
    } else {
        let member = app
            .http
            .guild_member(Id::new(guild.max(1)), Id::new(bot.max(1)))
            .await
            .ok()?;
        member.model().await.ok()?.roles.iter().map(|r| r.get()).collect()
    };
    let base = if facts.owner == bot {
        Permissions::all()
    } else {
        let granted = facts
            .roles
            .iter()
            .filter(|r| r.id.get() == guild || held.contains(&r.id.get()))
            .fold(Permissions::empty(), |p, r| p | r.permissions);
        if granted.contains(Permissions::ADMINISTRATOR) {
            Permissions::all()
        } else {
            granted
        }
    };
    let here = channel_permissions(base, guild, bot, &held, &rules.overwrites);
    let voice = matches!(kind, ChannelType::GuildVoice | ChannelType::GuildStageVoice);
    if !here.contains(Permissions::VIEW_CHANNEL) {
        Some(problem::VIEW_CHANNEL)
    } else if voice && !here.contains(Permissions::CONNECT) {
        Some(problem::CONNECT)
    } else if !here.contains(Permissions::MANAGE_CHANNELS) {
        Some(problem::MANAGE_CHANNELS)
    } else {
        None
    }
}

/// A permission's name as Discord's settings show it, for a [`problem`] key.
pub(crate) fn permission_name(key: &str, lang: Lang) -> String {
    t!(lang, &format!("stats.permission.{key}"))
}

/// What a stored problem says, in `lang`.
pub fn problem_text(key: &str, lang: Lang) -> String {
    match key {
        problem::VIEW_CHANNEL | problem::MANAGE_CHANNELS | problem::CONNECT => {
            t!(lang, "stats.problem.missing", permission = permission_name(key, lang))
        }
        problem::TOO_MANY_MEMBERS => t!(lang, "stats.problem.too-many-members"),
        problem::ROLE_GONE => t!(lang, "stats.problem.role-gone"),
        _ => t!(lang, "stats.problem.other"),
    }
}
