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

use pinchora_core::{
    leveling::{LevelReward, LevelUpMode, VoicePresence, scale, voice_earners, week_start},
    modules::Module,
    t,
};
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

use crate::{
    App,
    app::ChannelPlace,
    leveling::{self, LevelUp},
    roles,
};

async fn announce_voice_level(
    app: &Arc<App>,
    settings: &GuildSettings,
    guild: u64,
    user: u64,
    level: u32,
    record: &Record,
) {
    let result = async {
        let rewards: Vec<LevelReward> = app
            .db
            .voice_level_roles(guild)
            .await?
            .into_iter()
            .map(|row| LevelReward {
                level: row.level as u32,
                role_id: row.role_id as u64,
            })
            .collect();
        let (given, _) = roles::apply_level_roles(
            app,
            guild,
            user,
            level,
            &record.roles,
            &rewards,
            settings.level_roles_stack,
        )
        .await;
        let role = rewards
            .iter()
            .filter(|row| given.contains(&row.role_id))
            .max_by_key(|row| row.level)
            .map(|row| row.role_id);
        let mode = LevelUpMode::parse(&settings.levelup_mode);
        let here = Id::new(record.channel.max(1));
        let channel = match mode {
            LevelUpMode::Off => return anyhow::Ok(()),
            LevelUpMode::Current => here,
            LevelUpMode::Channel => match settings.levelup_channel_id {
                Some(ch) if app.module_allowed(guild, Some(ch as u64), Module::Leveling).await => Id::new(ch as u64),
                _ => here,
            },
            LevelUpMode::Dm => match leveling::private_channel(app, user).await {
                Some(ch) => ch,
                None => return Ok(()),
            },
        };
        let lang = if mode == LevelUpMode::Dm {
            crate::server_events::dm_lang(app, user, guild).await
        } else {
            app.guild_lang(guild).await
        };
        let content = if let Some(role) = role {
            t!(lang, "xp.voice-levelup-role", user = user, level = level, role = role)
        } else {
            t!(lang, "xp.voice-levelup", user = user, level = level)
        };
        let payload = serde_json::json!({"content": content, "allowed_mentions": {"parse": []}});
        app.http
            .create_message(channel)
            .payload_json(&serde_json::to_vec(&payload)?)
            .await?;
        anyhow::Ok(())
    }
    .await;
    if let Err(error) = result {
        tracing::warn!(%error, guild, user, "voice level-up failed");
    }
}

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
    crate::commands::tempvoice::reconcile(app, id, seen, inside, places.iter().map(|(channel, _)| *channel)).await;
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
    let parity = app.leveling_parity(guild).await?;
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
    crate::stats::count_voice(app, guild, &present, afk).await;
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
        crate::commands::achievements::on_message(app, guild, *user).await;
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

/// Stops ticking `guild`, unless a voice event refilled its hash between the empty read and the
/// removal: that member would otherwise sit unpaid until the next event in the server.
async fn forget_guild(app: &App, guild: u64, key: &str) -> anyhow::Result<()> {
    app.cache.set_remove(GUILDS, &guild.to_string()).await?;
    // Only a member who is in voice counts: the notes of those who left are no reason to keep ticking.
    let someone = app
        .cache
        .map_all(key)
        .await?
        .iter()
        .any(|(_, value)| Record::decode(value).is_some_and(|r| !r.has_left()));
    if someone {
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
    Ok(Some(xp + catches))
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
    fn a_member_who_left_is_a_record_nobody_is_paid_by() {
        let left = Record::left(1_700_000_000);
        assert!(left.has_left() && !left.muted && !left.deafened && left.roles.is_empty());
        assert_eq!(Record::decode(&left.encode()), Some(left));
        assert_eq!(Record::left(5).encode(), "0|0|5|");
        let here = Record {
            channel: 7,
            muted: false,
            deafened: false,
            seen: 1,
            roles: Vec::new(),
        };
        assert!(!here.has_left());
    }

    #[test]
    fn rounds_do_not_stack() {
        let rounds = Arc::new(Semaphore::new(ROUNDS_AT_ONCE));
        let mut running = Vec::new();
        for _ in 0..ROUNDS_AT_ONCE {
            running.push(start_round(&rounds).expect("a round may start"));
        }
        assert!(
            start_round(&rounds).is_none(),
            "the next minute is skipped while they pay"
        );
        running.pop();
        assert!(start_round(&rounds).is_some(), "once one is done another may start");
    }

    #[test]
    fn a_snapshot_keeps_only_what_the_rebuild_needs() {
        let guild: Guild = serde_json::from_str(
            r#"{
                "id": "5", "name": "g", "afk_channel_id": "99", "afk_timeout": 300, "application_id": null,
                "banner": null, "default_message_notifications": 0, "description": null,
                "discovery_splash": null, "emojis": [], "explicit_content_filter": 0, "features": [],
                "icon": null, "large": false, "mfa_level": 0, "nsfw_level": 0, "owner_id": "1",
                "preferred_locale": "en-US", "premium_progress_bar_enabled": false,
                "public_updates_channel_id": null, "roles": [], "rules_channel_id": null, "splash": null,
                "system_channel_flags": 0, "system_channel_id": null, "vanity_url_code": null,
                "verification_level": 0,
                "channels": [
                    { "id": "10", "type": 2, "name": "lounge", "parent_id": "11" },
                    { "id": "11", "type": 4, "name": "Voice" }
                ],
                "threads": [
                    { "id": "12", "type": 11, "name": "chat", "parent_id": "13", "guild_id": "5" }
                ],
                "members": [
                    { "user": { "id": "1", "username": "a", "discriminator": "0", "avatar": null },
                      "roles": ["7"], "joined_at": "2024-01-01T00:00:00.000000+00:00", "deaf": false,
                      "mute": false, "flags": 0 },
                    { "user": { "id": "2", "username": "b", "discriminator": "0", "avatar": null },
                      "roles": [], "joined_at": "2024-01-01T00:00:00.000000+00:00", "deaf": false,
                      "mute": false, "flags": 0 }
                ],
                "voice_states": [
                    { "guild_id": "5", "channel_id": "10", "user_id": "1", "session_id": "s", "deaf": false,
                      "mute": false, "self_deaf": false, "self_mute": true, "self_video": false,
                      "suppress": false, "request_to_speak_timestamp": null }
                ]
            }"#,
        )
        .expect("guild JSON");
        let snapshot = VoiceSnapshot::of(&guild, 77);
        assert_eq!((snapshot.guild(), snapshot.seen, snapshot.afk), (5, 77, Some(99)));
        let users: Vec<u64> = snapshot.records.iter().map(|(user, _)| *user).collect();
        assert_eq!(users, vec![1], "only the member in voice is kept, with their roles");
        assert_eq!(snapshot.records[0].1.roles, vec![7]);
        assert!(snapshot.records[0].1.muted);
        let mut places = snapshot.places.clone();
        places.sort_by_key(|(id, _)| *id);
        let seen: Vec<(u64, Option<u64>, bool)> = places.iter().map(|(id, p)| (*id, p.parent, p.thread)).collect();
        assert_eq!(
            seen,
            vec![(10, Some(11), false), (11, None, false), (12, Some(13), true)],
            "channels and threads, where they sit"
        );
    }

    #[test]
    fn garbage_is_not_a_record() {
        for raw in ["", "x", "1|2|3", "a|0|0|", "1|b|0|", "1|0|c|", "1|0|0|9,x"] {
            assert_eq!(Record::decode(raw), None, "{raw:?}");
        }
    }
}
