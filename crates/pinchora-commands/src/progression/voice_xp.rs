//! Voice XP: members sitting in a voice channel earn server XP once a minute.
//!
//! Who is in voice lives in Redis, written from the voice-state events (and rebuilt from the voice
//! states of GUILD_CREATE, so a restart or a missed leave heals itself), so any pod can pay. Servers
//! with voice XP off are not tracked at all. One ticker for all guilds takes a per-minute lock (a
//! restart or a second pod in the same minute pays nobody twice), works out who earns and writes each
//! guild's batch in a single statement.

mod announcement;
use announcement::*;
mod payment;
pub use payment::*;
#[cfg(test)]
mod tests;

use std::{
    collections::{HashMap, hash_map::Entry},
    sync::Arc,
    time::Duration,
};

use pinchora_core::admin::modules::Module;
use pinchora_core::progression::leveling::LevelReward;
use pinchora_core::progression::leveling::LevelUpMode;
use pinchora_core::progression::leveling::VoicePresence;
use pinchora_core::progression::leveling::scale;
use pinchora_core::progression::leveling::voice_earners;
use pinchora_core::progression::leveling::week_start;
use pinchora_core::t;
use pinchora_db::GuildSettings;
use tokio::{
    sync::{OwnedSemaphorePermit, Semaphore},
    task::JoinSet,
};
use twilight_model::{
    guild::{Guild, Member},
    id::Id,
    voice::VoiceState,
};

use crate::App;
use crate::infra::app::ChannelPlace;
use crate::progression::leveling;
use crate::progression::leveling::LevelUp;
use crate::roles::roles;

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
/// How long a member who left voice stays written down as having left. A rebuild from the voice states of an
/// older GUILD_CREATE must not bring them back, and it is merged within seconds of the snapshot, so this
/// only has to outlast the longest queue of rebuilds; the minute tick then sweeps the note away.
const LEFT_KEPT: i64 = 15 * 60;
/// A snapshot older than this is not used any more: the notes of leaves it could be wrong about may be gone
/// (see [`LEFT_KEPT`]), and the next voice event of the server puts things right anyway.
const MAX_SNAPSHOT_AGE: i64 = LEFT_KEPT / 3;
/// How long a server that switched voice XP off keeps refusing to write who is in voice, in Redis, where
/// every pod sees it at once. The settings a pod holds are at most a minute old, so this is plenty.
const OFF_KEPT: Duration = Duration::from_secs(10 * 60);
/// How many rounds of paying a ticker runs at once. A round that is still going when the next minute comes
/// is not run over by it: that minute is skipped, so a slow database is never given a pile of rounds.
const ROUNDS_AT_ONCE: usize = 1;

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
    /// A member who left voice at `seen`: channel 0, which nobody is paid in.
    fn left(seen: i64) -> Self {
        Self {
            channel: 0,
            muted: false,
            deafened: false,
            seen,
            roles: Vec::new(),
        }
    }

    fn has_left(&self) -> bool {
        self.channel == 0
    }

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
    // The settings this pod holds can be a minute old. A server that just switched voice XP off said so in
    // Redis, so nothing is written for it here after that, whatever the settings say.
    if is_switched_off(app, guild).await? {
        return Ok(());
    }
    let user = state.user_id.get().to_string();
    let key = members_key(guild);
    let now = app.now().timestamp();
    let Some(record) = record_of(state, state.member.as_ref(), now) else {
        // A leave is written down rather than erased: a rebuild that started from an older GUILD_CREATE and
        // is still waiting for its settings would otherwise put the member back for a day.
        return app.cache.map_set(&key, &user, &Record::left(now).encode(), KEEP).await;
    };
    app.cache.map_set(&key, &user, &record.encode(), KEEP).await?;
    app.cache.set_add(GUILDS, &guild.to_string()).await
}

fn off_key(guild: u64) -> String {
    format!("vc-off:{guild}")
}

/// Whether the server switched voice XP off recently (see [`switched_off`]).
async fn is_switched_off(app: &App, guild: u64) -> anyhow::Result<bool> {
    Ok(app.cache.get(&off_key(guild)).await?.is_some())
}

/// A server turned voice XP off: nobody in it is remembered, and for a few minutes no pod writes anyone down
/// for it, even one whose settings still say voice XP is on.
pub async fn switched_off(app: &App, guild: u64) -> anyhow::Result<()> {
    app.cache.put(&off_key(guild), "1", OFF_KEPT).await?;
    forget(app, guild).await
}

/// A server turned voice XP on again: writes are taken again.
pub async fn switched_on(app: &App, guild: u64) -> anyhow::Result<()> {
    app.cache.forget(&off_key(guild)).await
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

/// What the rebuild of a server keeps of its GUILD_CREATE: who is in voice (with the roles and the bot flag
/// their member entries carry), the AFK channel, where the channels and threads sit, and when the snapshot
/// was taken. It is taken before anything is awaited, so the guild itself (members, roles, emojis) is dropped
/// at once and whatever waits for its turn to be rebuilt holds only this.
#[derive(Debug)]
pub struct VoiceSnapshot {
    guild: u64,
    seen: i64,
    records: Vec<(u64, Record)>,
    afk: Option<u64>,
    places: Vec<(u64, ChannelPlace)>,
}

impl VoiceSnapshot {
    /// What the rebuild needs of `guild`, as of `seen` (Unix seconds).
    pub(crate) fn of(guild: &Guild, seen: i64) -> Self {
        Self {
            guild: guild.id.get(),
            seen,
            records: snapshot(&guild.voice_states, &guild.members, seen),
            afk: guild.afk_channel_id.map(|c| c.get()),
            places: guild
                .channels
                .iter()
                .chain(&guild.threads)
                .map(|channel| {
                    (
                        channel.id.get(),
                        ChannelPlace {
                            parent: channel.parent_id.map(|p| p.get()),
                            thread: channel.kind.is_thread(),
                        },
                    )
                })
                .collect(),
        }
    }

    /// The server this is about.
    pub fn guild(&self) -> u64 {
        self.guild
    }
}

/// Puts who is remembered in a server right with the voice states of its GUILD_CREATE, which stand for the
/// moment of the snapshot: whoever a voice event wrote down since (joined, moved, or left) stays as that
/// event left it, so a rebuild that waited for its settings never undoes an event that came in meanwhile.
/// Servers with voice XP off, or with nobody in voice, end up with nobody remembered.
pub async fn rebuild(app: &App, snapshot: VoiceSnapshot) -> anyhow::Result<()> {
    let VoiceSnapshot {
        guild: id,
        seen,
        records,
        afk,
        places,
    } = snapshot;
    // Temp voice puts its rooms right with the same snapshot, whether voice XP is on or not.
    let inside = records.iter().map(|(user, record)| (*user, record.channel));
    crate::commands::admin::tempvoice::reconcile(app, id, seen, inside, places.iter().map(|(channel, _)| *channel))
        .await;
    // Where the channels sit is learned either way: it costs no round trip, and saves many.
    app.prefill_channel_places(places).await;
    if app.now().timestamp() - seen > MAX_SNAPSHOT_AGE {
        tracing::debug!(
            guild = id,
            "voice snapshot too old to use; the next voice event puts things right"
        );
        return Ok(());
    }
    if !app.guild_settings(id).await?.voice_xp_enabled || is_switched_off(app, id).await? {
        return forget(app, id).await;
    }
    let key = members_key(id);
    let entries: Vec<(String, String)> = records.iter().map(|(user, r)| (user.to_string(), r.encode())).collect();
    app.cache.map_merge(&key, &entries, seen, KEEP).await?;
    if entries.is_empty() {
        forget_guild(app, id, &key).await?;
    } else {
        app.cache.set_add(GUILDS, &id.to_string()).await?;
    }
    remember_afk(app, id, afk).await;
    tracing::debug!(guild = id, members = entries.len(), "voice presence rebuilt");
    Ok(())
}

/// Forgets everyone in voice in `guild`.
pub async fn forget(app: &App, guild: u64) -> anyhow::Result<()> {
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

/// Works out who earns in one guild and pays them in one batch.
async fn pay_guild(app: &Arc<App>, guild: u64) -> anyhow::Result<usize> {
    let now = app.now();
    let key = members_key(guild);
    // The switch in Redis is as fresh as the toggle itself; the view below can be minutes old.
    if is_switched_off(app, guild).await? {
        forget(app, guild).await?;
        return Ok(0);
    }
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
    let parity = app.leveling_policy(guild).await?;
    if !parity.voice {
        return Ok(0);
    }
    let stored = app.cache.map_all(&key).await?;
    if stored.is_empty() {
        forget_guild(app, guild, &key).await?;
        return Ok(0);
    }
    let mut here: Vec<(u64, Record)> = Vec::with_capacity(stored.len());
    for (field, value) in stored {
        let record = field.parse::<u64>().ok().zip(Record::decode(&value));
        match record {
            // A member who left stays written down for a while (see `LEFT_KEPT`), then the note goes.
            Some((_, r)) if r.has_left() => {
                if now.timestamp() - r.seen > LEFT_KEPT {
                    app.cache.map_remove(&key, &field).await?;
                }
            }
            Some(entry) if now.timestamp() - entry.1.seen <= MAX_AGE_SECS => here.push(entry),
            _ => app.cache.map_remove(&key, &field).await?,
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
    crate::admin::stats::count_voice(app, guild, &present, afk).await;
    if earners.is_empty() {
        return Ok(0);
    }
    let rules = &view.rules;
    let base = i64::from(view.settings.voice_xp_amount);
    let mut lineages: HashMap<u64, Vec<u64>> = HashMap::new();
    // Leveling can be switched off in some voice channels (`/config modules`); nobody sitting in one earns.
    let mut allowed: HashMap<u64, bool> = HashMap::new();
    let mut grants: Vec<(u64, i64)> = Vec::with_capacity(earners.len());
    for (user, record) in here.iter().filter(|(user, _)| earners.contains(user)) {
        let channel_on = match allowed.entry(record.channel) {
            Entry::Occupied(known) => *known.get(),
            Entry::Vacant(free) => {
                *free.insert(app.module_allowed(guild, Some(record.channel), Module::Leveling).await)
            }
        };
        if !channel_on {
            continue;
        }
        let lineage = match lineages.entry(record.channel) {
            Entry::Occupied(known) => known.into_mut(),
            Entry::Vacant(free) => free.insert(app.xp_lineage(rules, record.channel).await),
        };
        let xp = scale(base, rules.multiplier_in(lineage, &record.roles));
        if xp > 0 {
            grants.push((*user, xp));
        }
    }
    let gains = if parity.separate_voice_level {
        app.db.add_separate_voice_xp(guild, &grants, week_start(now)).await?
    } else {
        app.db.add_voice_xp(guild, &grants, week_start(now)).await?
    };
    for (user, _) in &gains {
        crate::commands::progression::achievements::on_message(app, guild, *user).await;
    }
    metrics::counter!("pinchora_voice_xp_granted_total").increment(grants.iter().map(|(_, xp)| *xp as u64).sum());
    let climbs: Vec<(u64, u32, u32)> = gains
        .iter()
        .map(|(user, gain)| (*user, parity.level_of(gain.old_xp), parity.level_of(gain.new_xp)))
        .filter(|(_, before, after)| after > before)
        .collect();
    if !climbs.is_empty() {
        // Level-ups are the rare case: the freshest settings are worth a lookup only then.
        match app.guild_settings(guild).await {
            Ok(settings) => {
                for (user, before, after) in climbs {
                    if let Some((_, record)) = here.iter().find(|(u, _)| *u == user) {
                        if parity.separate_voice_level {
                            announce_voice_level(app, &settings, guild, user, after, record).await;
                        } else {
                            announce(app, &settings, guild, user, (before, after), record).await;
                        }
                    }
                }
            }
            Err(error) => tracing::warn!(guild, %error, "voice level-ups not announced"),
        }
    }
    Ok(gains.len())
}

/// Deletes weekly XP and weekly catches older than [`KEEP_WEEKS`] weeks, once an hour across every
/// gateway pod: the pod that takes the hour's lock does it and returns how many rows went, the others
/// get `None`.
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
    let xp = app.db.forget_weeks_before(before).await?;
    let catches = app.db.forget_catch_weeks_before(before).await?;
    let reef = app.db.prune_reef_loop(now).await?;
    Ok(Some(xp + catches + reef))
}

/// The minutes a ticker whose last round paid up to `next - 1` has to pay now that it is `now`: every
/// one since, oldest first, but no more than [`MAX_CATCH_UP`] back.
fn due_minutes(next: i64, now: i64) -> std::ops::RangeInclusive<i64> {
    next.max(now - MAX_CATCH_UP)..=now
}

/// Starts a round of paying if fewer than [`ROUNDS_AT_ONCE`] are running; the permit is the round's, and
/// ends it when dropped.
fn start_round(rounds: &Arc<Semaphore>) -> Option<OwnedSemaphorePermit> {
    rounds.clone().try_acquire_owned().ok()
}

/// Runs [`tick`] for the life of the process, and prunes old weekly XP and weekly catches once an
/// hour. Paying runs in a task of its own, so a slow round never holds up the clock, and a ticker that
/// woke late catches up on the minutes it missed, one after the other. Rounds never stack: when the last
/// one is still paying as a minute comes, that minute is skipped.
pub fn spawn_ticker(app: Arc<App>) {
    tokio::spawn(async move {
        let mut wake = tokio::time::interval(WAKE);
        wake.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        let rounds = Arc::new(Semaphore::new(ROUNDS_AT_ONCE));
        let mut next = app.now().timestamp().div_euclid(60);
        let mut pruned_hour = None;
        loop {
            wake.tick().await;
            let now = app.now().timestamp();
            let minute = now.div_euclid(60);
            let due: Vec<i64> = due_minutes(next, minute).collect();
            if !due.is_empty() {
                if let Some(round) = start_round(&rounds) {
                    let paying = app.clone();
                    app.spawn(async move {
                        for minute in due {
                            if let Err(error) = pay_minute(&paying, minute).await {
                                tracing::warn!(%error, "voice XP tick failed");
                            }
                        }
                        drop(round);
                    });
                } else {
                    tracing::warn!(
                        minutes = due.len(),
                        "voice XP minute skipped: the last round is still paying"
                    );
                    metrics::counter!("pinchora_voice_rounds_skipped_total").increment(due.len() as u64);
                }
            }
            next = next.max(minute + 1);
            let hour = now.div_euclid(3600);
            if pruned_hour != Some(hour) {
                pruned_hour = Some(hour);
                let pruning = app.clone();
                app.spawn(async move {
                    match prune(&pruning).await {
                        Ok(Some(n)) if n > 0 => tracing::debug!(n, "old weekly XP and catches forgotten"),
                        Ok(_) => {}
                        Err(error) => tracing::warn!(%error, "old weekly XP and catches not forgotten"),
                    }
                });
            }
        }
    });
}
