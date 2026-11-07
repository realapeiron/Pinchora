//! Public bot health snapshot. Gateway pods publish expiring shard records; readers never trust
//! a stale pod and never expose configuration, host names, tokens or connection strings.

use std::{
    collections::{BTreeMap, HashMap},
    sync::LazyLock,
    time::Duration,
};

use pinchora_cache::Cache;
use pinchora_db::Db;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::sync::Mutex;
use twilight_http::{
    Client,
    request::{Method, RequestBuilder},
    response::marker::ListBody,
};

const SHARD_PREFIX: &str = "pinchora:status:shard:";
const SNAPSHOT_KEY: &str = "public:status:v1";
const TTL: Duration = Duration::from_secs(90);
static REFRESH: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct ShardHealth {
    pub id: u32,
    pub total: u32,
    pub latency_ms: Option<u64>,
    pub guilds: u64,
    pub members: u64,
    pub rss_mb: u64,
    pub uptime_seconds: u64,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct PublicStatus {
    pub servers: u64,
    pub approximate_members: u64,
    pub players: u64,
    pub shards: Vec<ShardHealth>,
    pub uptime_seconds: u64,
    pub version: String,
    pub rust_version: String,
    pub rss_mb: u64,
    pub postgres_mb: u64,
    pub redis_mb: u64,
    pub commands_24h: u64,
    pub commands_total: u64,
    pub interactions_per_minute: u64,
    pub active_games: u64,
    pub open_tickets: u64,
    pub invite_url: String,
    pub support_url: String,
    pub website_url: String,
    pub status_url: String,
}

pub fn format_number(n: u64, turkish: bool) -> String {
    let separator = if turkish { '.' } else { ',' };
    let digits = n.to_string();
    let mut out = String::new();
    for (i, ch) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(separator);
        }
        out.push(ch);
    }
    out
}

/// Enumerate the bot's guilds through Twilight's rate-limited client. The typed endpoint does
/// not expose `with_counts`, so use its raw request path and decode just the fields we need.
pub async fn fetch_guild_counts(http: &Client, total: u32) -> anyhow::Result<HashMap<u32, HashMap<u64, u64>>> {
    let mut guilds: HashMap<u32, HashMap<u64, u64>> = HashMap::new();
    let mut after = None;
    loop {
        let path = match after {
            Some(id) => format!("users/@me/guilds?with_counts=true&limit=200&after={id}"),
            None => "users/@me/guilds?with_counts=true&limit=200".to_owned(),
        };
        let request = RequestBuilder::raw(Method::Get, path).build()?;
        let page: Vec<Value> = http.request::<ListBody<Value>>(request).await?.models().await?;
        let count = page.len();
        if count == 0 {
            break;
        }
        for guild in page {
            let id: u64 = guild["id"]
                .as_str()
                .ok_or_else(|| anyhow::anyhow!("guild list missing id"))?
                .parse()?;
            let members = guild["approximate_member_count"]
                .as_u64()
                .ok_or_else(|| anyhow::anyhow!("guild {id} missing approximate_member_count"))?;
            let shard = ((id >> 22) % u64::from(total.max(1))) as u32;
            guilds.entry(shard).or_default().insert(id, members);
            after = Some(after.map_or(id, |previous: u64| previous.max(id)));
        }
        if count < 200 {
            break;
        }
    }
    Ok(guilds)
}

/// Linux procfs reports kB. On other platforms or inaccessible procfs the measurement is absent.
pub fn parse_rss_kb(status: &str) -> Option<u64> {
    status.lines().find_map(|line| {
        let value = line.strip_prefix("VmRSS:")?;
        let mut words = value.split_whitespace();
        let amount = words.next()?.parse::<u64>().ok()?;
        (words.next() == Some("kB")).then_some(amount)
    })
}

pub fn process_rss_mb() -> u64 {
    let linux = std::fs::read_to_string("/proc/self/status")
        .ok()
        .and_then(|s| parse_rss_kb(&s));
    #[cfg(target_os = "macos")]
    let kb = linux.or_else(|| {
        let output = std::process::Command::new("ps")
            .args(["-o", "rss=", "-p", &std::process::id().to_string()])
            .output()
            .ok()?;
        String::from_utf8(output.stdout).ok()?.trim().parse::<u64>().ok()
    });
    #[cfg(not(target_os = "macos"))]
    let kb = linux;
    kb.unwrap_or(0) / 1024
}

pub async fn publish_shard(cache: &Cache, shard: &ShardHealth) -> anyhow::Result<()> {
    let key = format!("{SHARD_PREFIX}{}", shard.id);
    let _: () = redis::cmd("SET")
        .arg(key)
        .arg(serde_json::to_string(shard)?)
        .arg("EX")
        .arg(TTL.as_secs())
        .query_async(&mut cache.conn())
        .await?;
    Ok(())
}

async fn live_shards(cache: &Cache) -> anyhow::Result<Vec<ShardHealth>> {
    let mut cursor = 0_u64;
    let mut keys = Vec::new();
    loop {
        let (next, page): (u64, Vec<String>) = redis::cmd("SCAN")
            .arg(cursor)
            .arg("MATCH")
            .arg(format!("{SHARD_PREFIX}*"))
            .arg("COUNT")
            .arg(100)
            .query_async(&mut cache.conn())
            .await?;
        keys.extend(page);
        cursor = next;
        if cursor == 0 {
            break;
        }
    }
    let mut by_id = BTreeMap::new();
    for key in keys {
        let raw: Option<String> = redis::cmd("GET").arg(key).query_async(&mut cache.conn()).await?;
        if let Some(shard) = raw.and_then(|s| serde_json::from_str::<ShardHealth>(&s).ok()) {
            by_id.insert(shard.id, shard);
        }
    }
    Ok(by_id.into_values().collect())
}

async fn query_count(db: &Db, sql: &'static str) -> u64 {
    sqlx::query_scalar::<_, i64>(sql)
        .fetch_one(db.pool())
        .await
        .unwrap_or(0)
        .max(0) as u64
}

pub async fn record_interaction(cache: &Cache) {
    let minute = chrono::Utc::now().timestamp() / 60;
    let key = format!("pinchora:status:interactions:{minute}");
    let mut conn = cache.conn();
    let _: Result<i64, _> = redis::cmd("INCR").arg(&key).query_async(&mut conn).await;
    let _: Result<i64, _> = redis::cmd("EXPIRE").arg(&key).arg(120).query_async(&mut conn).await;
}

async fn interactions_per_minute(cache: &Cache) -> u64 {
    let minute = chrono::Utc::now().timestamp() / 60;
    let previous: i64 = redis::cmd("GET")
        .arg(format!("pinchora:status:interactions:{}", minute - 1))
        .query_async::<Option<i64>>(&mut cache.conn())
        .await
        .ok()
        .flatten()
        .unwrap_or(0);
    previous.max(0) as u64
}

pub async fn snapshot(db: &Db, cache: &Cache, application_id: u64, public_url: &str) -> PublicStatus {
    if let Ok(Some(raw)) = cache.get(SNAPSHOT_KEY).await
        && let Ok(value) = serde_json::from_str(&raw)
    {
        return value;
    }
    let _guard = REFRESH.lock().await;
    if let Ok(Some(raw)) = cache.get(SNAPSHOT_KEY).await
        && let Ok(value) = serde_json::from_str(&raw)
    {
        return value;
    }
    let shards = live_shards(cache).await.unwrap_or_default();
    let players = query_count(db, "SELECT count(*) FROM users").await;
    let postgres_mb = query_count(db, "SELECT pg_database_size(current_database()) / 1048576").await;
    let commands_24h = query_count(
        db,
        "SELECT coalesce(sum(commands),0)::bigint FROM guild_activity_hourly WHERE hour >= now() - interval '24 hours'",
    )
    .await;
    let commands_total = query_count(
        db,
        "SELECT coalesce(sum(commands),0)::bigint FROM guild_activity_hourly",
    )
    .await;
    let active_games = query_count(
        db,
        "SELECT count(*) FROM game_sessions WHERE status = 'active' AND expires_at > now()",
    )
    .await;
    let open_tickets = query_count(db, "SELECT count(*) FROM tickets WHERE status = 'open'").await;
    let redis_info: String = redis::cmd("INFO")
        .arg("memory")
        .query_async(&mut cache.conn())
        .await
        .unwrap_or_default();
    let redis_mb = redis_info
        .lines()
        .find_map(|line| line.strip_prefix("used_memory:")?.trim().parse::<u64>().ok())
        .unwrap_or(0)
        / 1_048_576;
    let website = public_url.trim_end_matches('/').to_owned();
    let status = PublicStatus {
        servers: shards.iter().map(|s| s.guilds).sum(),
        approximate_members: shards.iter().map(|s| s.members).sum(),
        players,
        uptime_seconds: shards.iter().map(|s| s.uptime_seconds).max().unwrap_or(0),
        rss_mb: shards.iter().map(|s| s.rss_mb).sum(),
        shards,
        version: format!("{} · {}", env!("PINCHORA_BUILD_COMMIT"), env!("PINCHORA_BUILD_DATE")),
        rust_version: env!("PINCHORA_RUST_VERSION")
            .split_whitespace()
            .take(2)
            .collect::<Vec<_>>()
            .join(" "),
        postgres_mb,
        redis_mb,
        commands_24h,
        commands_total,
        interactions_per_minute: interactions_per_minute(cache).await,
        active_games,
        open_tickets,
        invite_url: format!("{website}/invite?client_id={application_id}"),
        support_url: std::env::var("PINCHORA_SUPPORT_URL")
            .ok()
            .filter(|url| url.starts_with("https://discord.gg/") || url.starts_with("https://discord.com/invite/"))
            .unwrap_or_default(),
        website_url: website.clone(),
        status_url: format!("{website}/status"),
    };
    let _ = cache
        .put(
            SNAPSHOT_KEY,
            &serde_json::to_string(&status).unwrap_or_default(),
            Duration::from_secs(30),
        )
        .await;
    status
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rss_parser() {
        assert_eq!(parse_rss_kb("Name:\tbot\nVmRSS:\t12345 kB\n"), Some(12345));
        assert_eq!(parse_rss_kb("VmRSS: bogus kB"), None);
        assert_eq!(parse_rss_kb("VmRSS: 13 MB"), None);
    }
    #[test]
    fn localized_numbers() {
        assert_eq!(format_number(1_234, true), "1.234");
        assert_eq!(format_number(1_234_567, false), "1,234,567");
    }
}
