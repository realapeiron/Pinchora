//! Voice XP: members sitting in a voice channel earn server XP once a minute.
//!
//! Who is in voice lives in Redis, written from the voice-state events, so a restart loses nothing
//! and any pod can pay. One ticker for all guilds takes a per-minute lock (a restart or a second pod
//! in the same minute pays nobody twice), works out who earns and writes each guild's batch in a
//! single statement.

use std::{sync::Arc, time::Duration};

use pinchora_core::leveling::{VoicePresence, level_of, scale, voice_earners, week_start};
use pinchora_db::GuildSettings;
use twilight_model::{id::Id, voice::VoiceState};

use crate::{
    App,
    leveling::{self, LevelUp},
};

/// The guilds that have somebody in voice, as a set; each guild's members are a hash.
const GUILDS: &str = "vc:guilds";
/// A record not refreshed by any voice event for this long is dropped: the bot was probably away
/// when the member left, and nobody earns forever on a guess.
const MAX_AGE_SECS: i64 = 8 * 3600;
/// How long a guild's hash survives with no voice event at all.
const KEEP: Duration = Duration::from_secs(2 * 86_400);
/// The ticker wakes this often and the first wake of each minute does the work.
const WAKE: Duration = Duration::from_secs(20);
/// Weekly XP older than this many weeks is pruned, hourly.
const KEEP_WEEKS: i64 = 6;

fn members_key(guild: u64) -> String {
    format!("vc:{guild}")
}

/// What is remembered about one member in voice.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Record {
    channel: u64,
    muted: bool,
    deafened: bool,
    /// Unix seconds of the last voice event for this member.
    seen: i64,
    roles: Vec<u64>,
}

impl Record {
    /// `channel|flags|seen|role,role` with flags 1 for muted and 2 for deafened.
    fn encode(&self) -> String {
        let flags = u8::from(self.muted) | (u8::from(self.deafened) << 1);
        let roles: Vec<String> = self.roles.iter().map(u64::to_string).collect();
        format!("{}|{flags}|{}|{}", self.channel, self.seen, roles.join(","))
    }

    fn decode(raw: &str) -> Option<Self> {
        let mut parts = raw.splitn(4, '|');
        let channel = parts.next()?.parse().ok()?;
        let flags: u8 = parts.next()?.parse().ok()?;
        let seen = parts.next()?.parse().ok()?;
        let roles = parts
            .next()?
            .split(',')
            .filter(|r| !r.is_empty())
            .map(|r| r.parse().ok())
            .collect::<Option<Vec<u64>>>()?;
        Some(Self {
            channel,
            muted: flags & 1 != 0,
            deafened: flags & 2 != 0,
            seen,
            roles,
        })
    }
}

/// Remembers where `state`'s member is now, or forgets them when they left voice. Bots are never
/// tracked. Never fails the caller: a miss only means a minute of XP.
pub(crate) async fn track(app: &App, guild: u64, state: &VoiceState) {
    if let Err(error) = try_track(app, guild, state).await {
        tracing::debug!(guild, %error, "voice state not remembered");
    }
}

async fn try_track(app: &App, guild: u64, state: &VoiceState) -> anyhow::Result<()> {
    let user = state.user_id.get();
    let key = members_key(guild);
    let human = !state.member.as_ref().is_some_and(|m| m.user.bot);
    let Some(channel) = state.channel_id.filter(|_| human) else {
        return app.cache.map_remove(&key, &user.to_string()).await;
    };
    let record = Record {
        channel: channel.get(),
        muted: state.mute || state.self_mute,
        deafened: state.deaf || state.self_deaf,
        seen: app.now().timestamp(),
        roles: state
            .member
            .as_ref()
            .map(|m| m.roles.iter().map(|r| r.get()).collect())
            .unwrap_or_default(),
    };
    app.cache
        .map_set(&key, &user.to_string(), &record.encode(), KEEP)
        .await?;
    app.cache.set_add(GUILDS, &guild.to_string()).await
}

/// The server's AFK voice channel, remembered for ten minutes. `None` when it has none or Discord
/// cannot be asked.
async fn afk_channel(app: &App, guild: u64) -> Option<u64> {
    let key = format!("voice-afk:{guild}");
    if let Ok(Some(known)) = app.cache.get(&key).await {
        return known.parse::<u64>().ok().filter(|id| *id != 0);
    }
    let found = async {
        anyhow::Ok(
            app.http
                .guild(Id::new(guild.max(1)))
                .await?
                .model()
                .await?
                .afk_channel_id,
        )
    }
    .await
    .map_err(|error| tracing::debug!(guild, %error, "could not look up the AFK channel"))
    .ok()?;
    let id = found.map_or(0, |id| id.get());
    if let Err(error) = app.cache.put(&key, &id.to_string(), Duration::from_secs(600)).await {
        tracing::debug!(guild, %error, "AFK channel not remembered");
    }
    Some(id).filter(|id| *id != 0)
}

/// Pays this minute's voice XP, unless another tick (a second pod, or this process before a restart)
/// already did. Returns how many members were paid.
pub async fn tick(app: &Arc<App>) -> anyhow::Result<usize> {
    let now = app.now();
    let minute = now.timestamp().div_euclid(60);
    if !app
        .cache
        .once(&format!("voice-tick:{minute}"), Duration::from_secs(150))
        .await?
    {
        return Ok(0);
    }
    let mut paid = 0;
    for guild in app.cache.set_members(GUILDS).await? {
        let Ok(guild) = guild.parse::<u64>() else {
            continue;
        };
        match pay_guild(app, guild).await {
            Ok(n) => paid += n,
            Err(error) => tracing::warn!(guild, %error, "voice XP failed for a server"),
        }
    }
    Ok(paid)
}

/// Works out who earns in one guild and pays them in one batch.
async fn pay_guild(app: &Arc<App>, guild: u64) -> anyhow::Result<usize> {
    let now = app.now();
    let key = members_key(guild);
    let stored = app.cache.map_all(&key).await?;
    if stored.is_empty() {
        forget_guild(app, guild, &key).await?;
        return Ok(0);
    }
    let mut here: Vec<(u64, Record)> = Vec::with_capacity(stored.len());
    for (field, value) in stored {
        let record = field
            .parse::<u64>()
            .ok()
            .zip(Record::decode(&value))
            .filter(|(_, r)| now.timestamp() - r.seen <= MAX_AGE_SECS);
        match record {
            Some(entry) => here.push(entry),
            None => app.cache.map_remove(&key, &field).await?,
        }
    }
    if here.is_empty() {
        forget_guild(app, guild, &key).await?;
        return Ok(0);
    }
    let settings = app.guild_settings(guild).await?;
    if !settings.voice_xp_enabled || !settings.leveling_enabled {
        return Ok(0);
    }
    let present: Vec<VoicePresence> = here
        .iter()
        .map(|(user, r)| VoicePresence {
            user: *user,
            channel: r.channel,
            muted: r.muted,
            deafened: r.deafened,
        })
        .collect();
    let afk = afk_channel(app, guild).await;
    let earners = voice_earners(&present, afk);
    if earners.is_empty() {
        return Ok(0);
    }
    let rules = app.xp_rules(guild).await?;
    let base = i64::from(settings.voice_xp_amount);
    let grants: Vec<(u64, i64)> = here
        .iter()
        .filter(|(user, _)| earners.contains(user))
        .map(|(user, r)| (*user, scale(base, rules.multiplier(r.channel, &r.roles))))
        .filter(|(_, xp)| *xp > 0)
        .collect();
    let gains = app.db.add_voice_xp(guild, &grants, week_start(now)).await?;
    metrics::counter!("pinchora_voice_xp_granted_total").increment(grants.iter().map(|(_, xp)| *xp as u64).sum());
    for (user, gain) in &gains {
        let (before, after) = (level_of(gain.old_xp), level_of(gain.new_xp));
        if after > before
            && let Some((_, record)) = here.iter().find(|(u, _)| u == user)
        {
            announce(app, &settings, guild, *user, (before, after), record).await;
        }
    }
    Ok(gains.len())
}

/// Stops ticking `guild`, unless a voice event refilled its hash between the empty read and the
/// removal: that member would otherwise sit unpaid until the next event in the server.
async fn forget_guild(app: &App, guild: u64, key: &str) -> anyhow::Result<()> {
    app.cache.set_remove(GUILDS, &guild.to_string()).await?;
    if !app.cache.map_all(key).await?.is_empty() {
        app.cache.set_add(GUILDS, &guild.to_string()).await?;
    }
    Ok(())
}

/// A level-up earned in voice. Failures are logged: the XP is already paid.
async fn announce(
    app: &Arc<App>,
    settings: &GuildSettings,
    guild: u64,
    user: u64,
    levels: (u32, u32),
    record: &Record,
) {
    let author = app.user(user).await;
    let result = leveling::level_up(
        app,
        settings,
        LevelUp {
            guild,
            user,
            before: levels.0,
            after: levels.1,
            author: author.as_deref(),
            nick: None,
            roles: &record.roles,
            here: Id::new(record.channel.max(1)),
        },
    )
    .await;
    if let Err(error) = result {
        tracing::warn!(guild, user, %error, "voice level-up not announced");
    }
}

/// Runs [`tick`] for the life of the process, and prunes old weekly XP once an hour.
pub fn spawn_ticker(app: Arc<App>) {
    tokio::spawn(async move {
        let mut wake = tokio::time::interval(WAKE);
        wake.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        let mut rounds = 0u32;
        loop {
            wake.tick().await;
            if let Err(error) = tick(&app).await {
                tracing::warn!(%error, "voice XP tick failed");
            }
            rounds = rounds.wrapping_add(1);
            if rounds.is_multiple_of(180) {
                let before = week_start(app.now()) - chrono::Duration::weeks(KEEP_WEEKS);
                match app.db.forget_weeks_before(before).await {
                    Ok(n) if n > 0 => tracing::debug!(n, "old weekly XP forgotten"),
                    Ok(_) => {}
                    Err(error) => tracing::warn!(%error, "old weekly XP not forgotten"),
                }
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn records_survive_the_trip_through_redis() {
        let record = Record {
            channel: 123,
            muted: true,
            deafened: false,
            seen: 1_700_000_000,
            roles: vec![5, 6, 7],
        };
        assert_eq!(Record::decode(&record.encode()), Some(record));
        let bare = Record {
            channel: 1,
            muted: false,
            deafened: true,
            seen: 0,
            roles: Vec::new(),
        };
        assert_eq!(bare.encode(), "1|2|0|");
        assert_eq!(Record::decode(&bare.encode()), Some(bare));
    }

    #[test]
    fn garbage_is_not_a_record() {
        for raw in ["", "x", "1|2|3", "a|0|0|", "1|b|0|", "1|0|c|", "1|0|0|9,x"] {
            assert_eq!(Record::decode(raw), None, "{raw:?}");
        }
    }
}
