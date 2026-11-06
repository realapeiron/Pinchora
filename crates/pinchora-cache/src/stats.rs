//! Server stats counted in Redis between two flushes to Postgres.
//!
//! Each server has one hash per hour, bumped once per message (and per voice minute, join, leave and
//! command). A flush seals the hashes that changed: each is renamed to a batch key of its own, in one
//! step on the server, so counts that arrive while it is written start a fresh hash. A batch stays in
//! Redis until Postgres has it; Postgres remembers which batches it already took, so a flush cut short
//! and run again never counts anything twice.

use std::{
    collections::{HashMap, hash_map::RandomState},
    hash::{BuildHasher, Hasher},
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use redis::AsyncCommands;

use crate::{Cache, PREFIX};

/// How long counts wait for a flush at most: well past a stopped bot's restart, short enough that
/// Redis never fills up with them.
const KEEP: Duration = Duration::from_secs(3 * 86_400);
static SEAL_NONCE: AtomicU64 = AtomicU64::new(0);

fn seal_nonce() -> String {
    let time = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let random = RandomState::new().build_hasher().finish();
    let local = SEAL_NONCE.fetch_add(1, Ordering::Relaxed);
    format!("{time:x}-{random:x}-{:x}-{local:x}", std::process::id())
}

fn dirty() -> String {
    format!("{PREFIX}:stats:dirty")
}

fn pending() -> String {
    format!("{PREFIX}:stats:pending")
}

fn hash_prefix() -> String {
    format!("{PREFIX}:stats:h:")
}

fn batch_prefix() -> String {
    format!("{PREFIX}:stats:p:")
}

/// The counts of one server in one hour, sealed for a flush.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatsBatch {
    /// Unique to this batch: Postgres keeps it to know it was written.
    pub key: String,
    pub guild: u64,
    /// Hours since the Unix epoch.
    pub hour: i64,
    pub fields: Vec<(String, i64)>,
}

impl StatsBatch {
    /// The server and hour a batch key names: `…:p:{guild}:{hour}:{seq}`.
    fn parse(key: &str) -> Option<(u64, i64)> {
        let mut parts = key.strip_prefix(&batch_prefix())?.split(':');
        Some((parts.next()?.parse().ok()?, parts.next()?.parse().ok()?))
    }
}

impl Cache {
    /// One pod owns the stats flush until it releases this token or the lease expires.
    pub async fn stats_claim_flush(&self) -> anyhow::Result<Option<String>> {
        let token = seal_nonce();
        let set: Option<String> = redis::cmd("SET")
            .arg(format!("{PREFIX}:stats:flush-owner"))
            .arg(&token)
            .arg("NX")
            .arg("EX")
            .arg(90)
            .query_async(&mut self.conn())
            .await?;
        Ok(set.map(|_| token))
    }

    pub async fn stats_release_flush(&self, token: &str) -> anyhow::Result<()> {
        const SCRIPT: &str =
            "if redis.call('GET', KEYS[1]) == ARGV[1] then return redis.call('DEL', KEYS[1]) else return 0 end";
        let _: i64 = redis::Script::new(SCRIPT)
            .key(format!("{PREFIX}:stats:flush-owner"))
            .arg(token)
            .invoke_async(&mut self.conn())
            .await?;
        Ok(())
    }

    pub async fn stats_extend_flush(&self, token: &str) -> anyhow::Result<bool> {
        const SCRIPT: &str =
            "if redis.call('GET', KEYS[1]) == ARGV[1] then return redis.call('EXPIRE', KEYS[1], 90) else return 0 end";
        let renewed: i64 = redis::Script::new(SCRIPT)
            .key(format!("{PREFIX}:stats:flush-owner"))
            .arg(token)
            .invoke_async(&mut self.conn())
            .await?;
        Ok(renewed == 1)
    }
    /// Adds `fields` to what `guild` did in `hour` (hours since the Unix epoch), in one round trip.
    pub async fn stats_add(&self, guild: u64, hour: i64, fields: &[(String, i64)]) -> anyhow::Result<()> {
        if fields.is_empty() {
            return Ok(());
        }
        let member = format!("{guild}:{hour}");
        let key = format!("{}{member}", hash_prefix());
        let mut pipe = redis::pipe();
        pipe.atomic();
        for (field, by) in fields {
            pipe.hincr(&key, field, *by).ignore();
        }
        pipe.expire(&key, KEEP.as_secs() as i64)
            .ignore()
            .sadd(dirty(), &member)
            .ignore();
        let _: () = pipe.query_async(&mut self.conn()).await?;
        Ok(())
    }

    /// Seals up to `limit` hashes that changed since the last flush into batches; returns how many.
    pub async fn stats_seal(&self, limit: usize) -> anyhow::Result<usize> {
        const SCRIPT: &str = r#"
            local members = redis.call('SPOP', KEYS[1], tonumber(ARGV[1]))
            local sealed = 0
            for _, member in ipairs(members) do
                local source = ARGV[2] .. member
                if redis.call('EXISTS', source) == 1 then
                    local seq = redis.call('INCR', KEYS[3])
                    local target = ARGV[3] .. member .. ':' .. ARGV[5] .. ':' .. seq
                    redis.call('RENAME', source, target)
                    redis.call('EXPIRE', target, tonumber(ARGV[4]))
                    redis.call('SADD', KEYS[2], target)
                    sealed = sealed + 1
                end
            end
            return sealed
        "#;
        let sealed: usize = redis::Script::new(SCRIPT)
            .key(dirty())
            .key(pending())
            .key(format!("{PREFIX}:stats:seq"))
            .arg(limit.max(1))
            .arg(hash_prefix())
            .arg(batch_prefix())
            .arg(KEEP.as_secs())
            .arg(seal_nonce())
            .invoke_async(&mut self.conn())
            .await?;
        Ok(sealed)
    }

    /// Up to `limit` sealed batches not yet written to Postgres, with their counts. A batch key that ran
    /// out (Postgres was down for days) is dropped from the list.
    pub async fn stats_pending(&self, limit: usize) -> anyhow::Result<Vec<StatsBatch>> {
        let mut conn = self.conn();
        let keys: Vec<String> = conn
            .srandmember_multiple(pending(), limit.clamp(1, 100_000) as isize)
            .await?;
        let mut batches = Vec::with_capacity(keys.len());
        for key in keys {
            let fields: HashMap<String, i64> = conn.hgetall(&key).await?;
            let fields: Vec<(String, i64)> = fields.into_iter().collect();
            match StatsBatch::parse(&key) {
                Some((guild, hour)) if !fields.is_empty() => batches.push(StatsBatch {
                    key,
                    guild,
                    hour,
                    fields,
                }),
                _ => {
                    let _: () = conn.srem(pending(), &key).await?;
                }
            }
        }
        Ok(batches)
    }

    /// Forgets a batch Postgres has.
    pub async fn stats_done(&self, key: &str) -> anyhow::Result<()> {
        let _: () = redis::pipe()
            .atomic()
            .del(key)
            .ignore()
            .srem(pending(), key)
            .ignore()
            .query_async(&mut self.conn())
            .await?;
        Ok(())
    }

    /// Remove a deleted member's pending counts from live and sealed hashes.
    pub async fn stats_forget_member(&self, user: u64) -> anyhow::Result<()> {
        const SCRIPT: &str = r#"
            for _, member in ipairs(redis.call('SMEMBERS', KEYS[1])) do
                redis.call('HDEL', ARGV[1] .. member, ARGV[2], ARGV[3])
            end
            for _, batch in ipairs(redis.call('SMEMBERS', KEYS[2])) do
                redis.call('HDEL', batch, ARGV[2], ARGV[3])
            end
            return 1
        "#;
        let _: i64 = redis::Script::new(SCRIPT)
            .key(dirty())
            .key(pending())
            .arg(hash_prefix())
            .arg(format!("u:{user}"))
            .arg(format!("uv:{user}"))
            .invoke_async(&mut self.conn())
            .await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn batch_keys_name_their_server_and_hour() {
        let key = format!("{}42:493000:7", batch_prefix());
        assert_eq!(StatsBatch::parse(&key), Some((42, 493_000)));
        assert_eq!(StatsBatch::parse("pinchora:stats:h:42:1"), None);
        assert_eq!(StatsBatch::parse(&format!("{}x:1:1", batch_prefix())), None);
    }

    #[test]
    fn seal_ids_have_a_process_independent_nonce() {
        assert_ne!(seal_nonce(), seal_nonce());
    }

    #[tokio::test]
    async fn deleting_a_member_clears_live_and_sealed_fields() {
        let Ok(url) = std::env::var("PINCHORA_TEST_REDIS_URL") else {
            return;
        };
        let cache = Cache::connect(&url).await.unwrap();
        let guild = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos() as u64;
        let user = guild + 1;
        cache
            .stats_add(guild, 123, &[(format!("u:{user}"), 1), (format!("uv:{user}"), 2)])
            .await
            .unwrap();
        cache.stats_seal(100_000).await.unwrap();
        cache.stats_add(guild, 123, &[(format!("u:{user}"), 3)]).await.unwrap();
        cache.stats_forget_member(user).await.unwrap();
        cache.stats_seal(100_000).await.unwrap();
        for batch in cache
            .stats_pending(100_000)
            .await
            .unwrap()
            .into_iter()
            .filter(|b| b.guild == guild)
        {
            assert!(
                batch
                    .fields
                    .iter()
                    .all(|(name, _)| name != &format!("u:{user}") && name != &format!("uv:{user}"))
            );
            cache.stats_done(&batch.key).await.unwrap();
        }
    }

    #[tokio::test]
    async fn channel_rename_claim_survives_a_configuration_reset() {
        let Ok(url) = std::env::var("PINCHORA_TEST_REDIS_URL") else {
            return;
        };
        let cache = Cache::connect(&url).await.unwrap();
        let channel = seal_nonce();
        let key = format!("counter-rename:{channel}");
        assert!(cache.once(&key, Duration::from_secs(600)).await.unwrap());
        assert!(!cache.once(&key, Duration::from_secs(600)).await.unwrap());
    }

    #[tokio::test]
    async fn overlapping_flush_claims_are_exclusive_and_releasable() {
        let Ok(url) = std::env::var("PINCHORA_TEST_REDIS_URL") else {
            return;
        };
        let cache = Cache::connect(&url).await.unwrap();
        let first = cache.stats_claim_flush().await.unwrap().expect("first claim");
        assert!(cache.stats_claim_flush().await.unwrap().is_none());
        assert!(cache.stats_extend_flush(&first).await.unwrap());
        assert!(!cache.stats_extend_flush("wrong-owner").await.unwrap());
        cache.stats_release_flush("wrong-owner").await.unwrap();
        assert!(cache.stats_claim_flush().await.unwrap().is_none());
        cache.stats_release_flush(&first).await.unwrap();
        let second = cache.stats_claim_flush().await.unwrap().expect("claim after release");
        cache.stats_release_flush(&second).await.unwrap();
    }
}
