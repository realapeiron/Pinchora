//! Voice XP: members sitting in a voice channel earn server XP once a minute.
//!
//! Who is in voice lives in Redis, written from the voice-state events (and rebuilt from the voice
//! states of GUILD_CREATE, so a restart or a missed leave heals itself), so any pod can pay. Servers
//! with voice XP off are not tracked at all. One ticker for all guilds takes a per-minute lock (a
//! restart or a second pod in the same minute pays nobody twice), works out who earns and writes each
//! guild's batch in a single statement.

use std::{
    collections::{HashMap, hash_map::Entry},
    sync::Arc,
    time::Duration,
};

use pinchora_core::leveling::{VoicePresence, level_of, scale, voice_earners, week_start};
use pinchora_db::GuildSettings;
use tokio::{sync::Semaphore, task::JoinSet};
use twilight_model::{
    guild::{Guild, Member},
    id::Id,
    voice::VoiceState,
};

use crate::{
    App,
    leveling::{self, LevelUp},
};

/// The guilds that have somebody in voice, as a set; each guild's members are a hash.
const GUILDS: &str = "vc:guilds";
/// A record not refreshed by any voice event for this long is dropped. GUILD_CREATE and the leave
/// events keep the records right; this only stops a leave that was lost anyway (an event dropped under
/// load, Redis down at the wrong moment) from paying for days.
const MAX_AGE_SECS: i64 = 24 * 3600;
/// How long a guild's hash survives with no voice event at all.
const KEEP: Duration = Duration::from_secs(2 * 86_400);
/// The ticker wakes this often and the first wake of each minute does the work.
const WAKE: Duration = Duration::from_secs(20);
/// Guilds paid at the same time in one tick.
const PARALLEL: usize = 8;
/// The most minutes a stalled ticker pays back once it is running again.
const MAX_CATCH_UP: i64 = 3;
/// How long a paid minute stays locked: past the oldest minute a late ticker still pays, so a pod that
/// catches up never pays a minute another pod already paid.
const MINUTE_LOCK: Duration = Duration::from_secs(60 * (MAX_CATCH_UP as u64 + 3));
const _: () = assert!(MINUTE_LOCK.as_secs() > (MAX_CATCH_UP as u64 + 1) * 60 + WAKE.as_secs());
/// Weekly XP older than this many weeks is pruned, hourly.
const KEEP_WEEKS: i64 = 6;
/// How long the AFK channel lookup is trusted, and how long its last answer is kept for when Discord
/// cannot be asked.
const AFK_FRESH: Duration = Duration::from_secs(600);
const AFK_KEPT: Duration = Duration::from_secs(7 * 86_400);

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
/// tracked, and neither is anyone in a server with voice XP off. Never fails the caller: a miss only
/// means a minute of XP.
pub(crate) async fn track(app: &App, guild: u64, state: &VoiceState, settings: &GuildSettings) {
    if !settings.voice_xp_enabled {
        return;
    }
    if let Err(error) = try_track(app, guild, state).await {
        tracing::warn!(guild, user = state.user_id.get(), %error, "voice state not remembered");
    }
}

async fn try_track(app: &App, guild: u64, state: &VoiceState) -> anyhow::Result<()> {
    let user = state.user_id.get().to_string();
    let key = members_key(guild);
    let Some(record) = record_of(state, state.member.as_ref(), app.now().timestamp()) else {
        return app.cache.map_remove(&key, &user).await;
    };
    app.cache.map_set(&key, &user, &record.encode(), KEEP).await?;
    app.cache.set_add(GUILDS, &guild.to_string()).await
}

/// What `state` says about a human sitting in a voice channel; `None` for a bot or a member in none.
fn record_of(state: &VoiceState, member: Option<&Member>, seen: i64) -> Option<Record> {
    let channel = state.channel_id?;
    if member.is_some_and(|m| m.user.bot) {
        return None;
    }
    Some(Record {
        channel: channel.get(),
        muted: state.mute || state.self_mute,
        deafened: state.deaf || state.self_deaf,
        seen,
        roles: member
            .map(|m| m.roles.iter().map(|r| r.get()).collect())
            .unwrap_or_default(),
    })
}

/// The members a GUILD_CREATE puts in voice. Its voice states carry no member, so roles and the bot
/// flag come from the payload's member list; someone it has no member for is left out until their next
/// voice event rather than guessed at (a bot, or a role that mutes XP, could be missed).
fn snapshot(states: &[VoiceState], members: &[Member], seen: i64) -> Vec<(u64, Record)> {
    let known: HashMap<u64, &Member> = members.iter().map(|m| (m.user.id.get(), m)).collect();
    states
        .iter()
        .filter_map(|state| {
            let user = state.user_id.get();
            let member = state.member.as_ref().or_else(|| known.get(&user).copied())?;
            Some((user, record_of(state, Some(member), seen)?))
        })
        .collect()
}

/// Replaces who is remembered in `guild` with the voice states of its GUILD_CREATE. Servers with voice
/// XP off, or with nobody in voice, are simply forgotten: whatever was remembered from before is wrong.
pub(crate) async fn rebuild(app: &App, guild: &Guild) -> anyhow::Result<()> {
    let id = guild.id.get();
    let records = snapshot(&guild.voice_states, &guild.members, app.now().timestamp());
    if records.is_empty() || !app.guild_settings(id).await?.voice_xp_enabled {
        return forget(app, id).await;
    }
    let entries: Vec<(String, String)> = records.iter().map(|(user, r)| (user.to_string(), r.encode())).collect();
    app.cache.map_replace(&members_key(id), &entries, KEEP).await?;
    app.cache.set_add(GUILDS, &id.to_string()).await?;
    remember_afk(app, id, guild.afk_channel_id.map(|c| c.get())).await;
    tracing::debug!(guild = id, members = entries.len(), "voice presence rebuilt");
    Ok(())
}

/// Forgets everyone in voice in `guild`.
pub(crate) async fn forget(app: &App, guild: u64) -> anyhow::Result<()> {
    app.cache.map_replace(&members_key(guild), &[], KEEP).await?;
    app.cache.set_remove(GUILDS, &guild.to_string()).await
}

/// The server's AFK voice channel: `None` when it has none. Remembered for ten minutes; when Discord
/// cannot be asked the last answer is used for a minute at a time, and without one this is an error:
/// guessing "no AFK channel" would pay everybody sleeping in it.
async fn afk_channel(app: &App, guild: u64) -> anyhow::Result<Option<u64>> {
    let key = format!("voice-afk:{guild}");
    if let Ok(Some(known)) = app.cache.get(&key).await {
        return Ok(known.parse::<u64>().ok().filter(|id| *id != 0));
    }
    let asked = async {
        anyhow::Ok(
            app.http
                .guild(Id::new(guild.max(1)))
                .await?
                .model()
                .await?
                .afk_channel_id,
        )
    }
    .await;
    match asked {
        Ok(found) => {
            let id = found.map(|id| id.get());
            remember_afk(app, guild, id).await;
            Ok(id)
        }
        Err(error) => {
            let Ok(Some(last)) = app.cache.get(&format!("voice-afk-last:{guild}")).await else {
                return Err(error);
            };
            tracing::warn!(guild, %error, "could not look up the AFK channel; using the last known one");
            if let Err(error) = app.cache.put(&key, &last, Duration::from_secs(60)).await {
                tracing::debug!(guild, %error, "AFK channel not remembered");
            }
            Ok(last.parse::<u64>().ok().filter(|id| *id != 0))
        }
    }
}

/// Keeps the AFK channel (0 for none) for [`afk_channel`], fresh and as the last known answer.
async fn remember_afk(app: &App, guild: u64, afk: Option<u64>) {
    let id = afk.unwrap_or(0).to_string();
    for (key, ttl) in [
        (format!("voice-afk:{guild}"), AFK_FRESH),
        (format!("voice-afk-last:{guild}"), AFK_KEPT),
    ] {
        if let Err(error) = app.cache.put(&key, &id, ttl).await {
            tracing::debug!(guild, %error, "AFK channel not remembered");
        }
    }
}

/// Pays this minute's voice XP, unless another tick (a second pod, or this process before a restart)
/// already did. Returns how many members were paid.
pub async fn tick(app: &Arc<App>) -> anyhow::Result<usize> {
    pay_minute(app, app.now().timestamp().div_euclid(60)).await
}

/// Pays `minute` to every guild with somebody in voice, a few guilds at a time.
async fn pay_minute(app: &Arc<App>, minute: i64) -> anyhow::Result<usize> {
    if !app.cache.once(&format!("voice-tick:{minute}"), MINUTE_LOCK).await? {
        return Ok(0);
    }
    let guilds: Vec<u64> = app
        .cache
        .set_members(GUILDS)
        .await?
        .iter()
        .filter_map(|guild| guild.parse().ok())
        .collect();
    let slots = Arc::new(Semaphore::new(PARALLEL));
    let mut jobs = JoinSet::new();
    for guild in guilds {
        let slot = slots.clone().acquire_owned().await?;
        let app = app.clone();
        jobs.spawn(async move {
            let paid = pay_guild(&app, guild).await;
            drop(slot);
            (guild, paid)
        });
    }
    let mut paid = 0;
    while let Some(done) = jobs.join_next().await {
        match done {
            Ok((_, Ok(n))) => paid += n,
            Ok((guild, Err(error))) => tracing::warn!(guild, %error, "voice XP failed for a server"),
            Err(error) => tracing::error!(%error, "voice XP task failed"),
        }
    }
    Ok(paid)
}

/// Works out who earns in one guild and pays them in one batch.
async fn pay_guild(app: &Arc<App>, guild: u64) -> anyhow::Result<usize> {
    let now = app.now();
    let key = members_key(guild);
    let mut view = app.voice_guild(guild).await?;
    if !view.settings.voice_xp_enabled {
        // The view can lag a change by minutes: ask again before giving up on the guild.
        app.forget_voice_guild(guild).await;
        view = app.voice_guild(guild).await?;
        if !view.settings.voice_xp_enabled {
            forget(app, guild).await?;
            return Ok(0);
        }
    }
    if !view.settings.leveling_enabled {
        return Ok(0);
    }
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
    let present: Vec<VoicePresence> = here
        .iter()
        .map(|(user, r)| VoicePresence {
            user: *user,
            channel: r.channel,
            muted: r.muted,
            deafened: r.deafened,
        })
        .collect();
    let afk = match afk_channel(app, guild).await {
        Ok(afk) => afk,
        Err(error) => {
            tracing::warn!(guild, %error, "AFK channel unknown; nobody is paid in this server this minute");
            return Ok(0);
        }
    };
    let earners = voice_earners(&present, afk);
    if earners.is_empty() {
        return Ok(0);
    }
    let rules = &view.rules;
    let base = i64::from(view.settings.voice_xp_amount);
    let mut lineages: HashMap<u64, Vec<u64>> = HashMap::new();
    let mut grants: Vec<(u64, i64)> = Vec::with_capacity(earners.len());
    for (user, record) in here.iter().filter(|(user, _)| earners.contains(user)) {
        let lineage = match lineages.entry(record.channel) {
            Entry::Occupied(known) => known.into_mut(),
            Entry::Vacant(free) => free.insert(app.xp_lineage(rules, record.channel).await),
        };
        let xp = scale(base, rules.multiplier_in(lineage, &record.roles));
        if xp > 0 {
            grants.push((*user, xp));
        }
    }
    let gains = app.db.add_voice_xp(guild, &grants, week_start(now)).await?;
    metrics::counter!("pinchora_voice_xp_granted_total").increment(grants.iter().map(|(_, xp)| *xp as u64).sum());
    let climbs: Vec<(u64, u32, u32)> = gains
        .iter()
        .map(|(user, gain)| (*user, level_of(gain.old_xp), level_of(gain.new_xp)))
        .filter(|(_, before, after)| after > before)
        .collect();
    if !climbs.is_empty() {
        // Level-ups are the rare case: the freshest settings are worth a lookup only then.
        match app.guild_settings(guild).await {
            Ok(settings) => {
                for (user, before, after) in climbs {
                    if let Some((_, record)) = here.iter().find(|(u, _)| *u == user) {
                        announce(app, &settings, guild, user, (before, after), record).await;
                    }
                }
            }
            Err(error) => tracing::warn!(guild, %error, "voice level-ups not announced"),
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

/// Deletes weekly XP older than [`KEEP_WEEKS`] weeks, once an hour across every gateway pod: the pod
/// that takes the hour's lock does it and returns how many rows went, the others get `None`.
pub async fn prune(app: &App) -> anyhow::Result<Option<u64>> {
    let now = app.now();
    let hour = now.timestamp().div_euclid(3600);
    if !app
        .cache
        .once(&format!("weekly-prune:{hour}"), Duration::from_secs(2 * 3600))
        .await?
    {
        return Ok(None);
    }
    let before = week_start(now) - chrono::Duration::weeks(KEEP_WEEKS);
    Ok(Some(app.db.forget_weeks_before(before).await?))
}

/// The minutes a ticker whose last round paid up to `next - 1` has to pay now that it is `now`: every
/// one since, oldest first, but no more than [`MAX_CATCH_UP`] back.
fn due_minutes(next: i64, now: i64) -> std::ops::RangeInclusive<i64> {
    next.max(now - MAX_CATCH_UP)..=now
}

/// Runs [`tick`] for the life of the process, and prunes old weekly XP once an hour. Each minute is
/// paid in a task of its own, so a slow round never holds up the next one, and a round that was
/// late catches up on the minutes it missed.
pub fn spawn_ticker(app: Arc<App>) {
    tokio::spawn(async move {
        let mut wake = tokio::time::interval(WAKE);
        wake.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        let mut next = app.now().timestamp().div_euclid(60);
        let mut pruned_hour = None;
        loop {
            wake.tick().await;
            let now = app.now().timestamp();
            let minute = now.div_euclid(60);
            for due in due_minutes(next, minute) {
                let paying = app.clone();
                app.spawn(async move {
                    if let Err(error) = pay_minute(&paying, due).await {
                        tracing::warn!(%error, "voice XP tick failed");
                    }
                });
            }
            next = next.max(minute + 1);
            let hour = now.div_euclid(3600);
            if pruned_hour != Some(hour) {
                pruned_hour = Some(hour);
                let pruning = app.clone();
                app.spawn(async move {
                    match prune(&pruning).await {
                        Ok(Some(n)) if n > 0 => tracing::debug!(n, "old weekly XP forgotten"),
                        Ok(_) => {}
                        Err(error) => tracing::warn!(%error, "old weekly XP not forgotten"),
                    }
                });
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

    fn member(user: u64, bot: bool, roles: &[u64]) -> Member {
        serde_json::from_value(serde_json::json!({
            "user": { "id": user.to_string(), "username": "u", "discriminator": "0", "avatar": null, "bot": bot },
            "roles": roles.iter().map(u64::to_string).collect::<Vec<_>>(),
            "joined_at": "2024-01-01T00:00:00.000000+00:00",
            "deaf": false,
            "mute": false,
            "flags": 0,
        }))
        .expect("member JSON")
    }

    /// A voice state as GUILD_CREATE sends it: no member.
    fn state(user: u64, channel: Option<u64>, self_mute: bool, self_deaf: bool) -> VoiceState {
        serde_json::from_value(serde_json::json!({
            "guild_id": "1",
            "channel_id": channel.map(|c| c.to_string()),
            "user_id": user.to_string(),
            "session_id": "s",
            "deaf": false,
            "mute": false,
            "self_deaf": self_deaf,
            "self_mute": self_mute,
            "self_video": false,
            "suppress": false,
            "request_to_speak_timestamp": null,
        }))
        .expect("voice state JSON")
    }

    #[test]
    fn a_snapshot_takes_roles_and_bots_from_the_member_list() {
        let states = [
            state(1, Some(10), false, false),
            state(2, Some(10), true, true),
            state(3, Some(10), false, false),
            state(4, None, false, false),
            state(5, Some(20), false, false),
        ];
        let members = [
            member(1, false, &[7, 8]),
            member(2, false, &[]),
            member(3, true, &[]),
            member(4, false, &[]),
        ];
        let records = snapshot(&states, &members, 99);
        let users: Vec<u64> = records.iter().map(|(user, _)| *user).collect();
        assert_eq!(
            users,
            vec![1, 2],
            "3 is a bot, 4 is in no channel, 5 has no member to vouch for them"
        );
        assert_eq!(
            records[0].1,
            Record {
                channel: 10,
                muted: false,
                deafened: false,
                seen: 99,
                roles: vec![7, 8],
            }
        );
        assert!(records[1].1.muted && records[1].1.deafened, "self mute and deaf count");
        assert!(snapshot(&[], &members, 0).is_empty());
    }

    #[test]
    fn an_event_without_a_member_is_still_a_human() {
        let record = record_of(&state(9, Some(3), false, false), None, 5).expect("a record");
        assert_eq!((record.channel, record.roles), (3, Vec::new()));
        assert!(
            record_of(&state(9, None, false, false), None, 5).is_none(),
            "left voice"
        );
        assert!(record_of(&state(9, Some(3), false, false), Some(&member(9, true, &[])), 5).is_none());
    }

    #[test]
    fn a_late_ticker_catches_up_a_few_minutes_and_no_more() {
        assert_eq!(due_minutes(100, 100).collect::<Vec<_>>(), vec![100]);
        assert_eq!(due_minutes(101, 100).count(), 0, "nothing is due yet");
        assert_eq!(due_minutes(100, 102).collect::<Vec<_>>(), vec![100, 101, 102]);
        assert_eq!(
            due_minutes(100, 500).collect::<Vec<_>>(),
            vec![497, 498, 499, 500],
            "a long stall pays only the latest few"
        );
    }

    #[test]
    fn garbage_is_not_a_record() {
        for raw in ["", "x", "1|2|3", "a|0|0|", "1|b|0|", "1|0|c|", "1|0|0|9,x"] {
            assert_eq!(Record::decode(raw), None, "{raw:?}");
        }
    }
}
