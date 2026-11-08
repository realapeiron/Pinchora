mod invalidate;
mod queue;
mod reef;
mod stats;

use std::time::Duration;

use redis::{AsyncCommands, aio::ConnectionManager};
use serde::{Deserialize, Serialize};

pub use invalidate::Invalidation;
pub use queue::RedisQueue;
pub use stats::StatsBatch;

const PREFIX: &str = "pinchora";

/// Handle to Redis. One multiplexed, auto-reconnecting connection; cheap to clone.
#[derive(Clone)]
pub struct Cache {
    conn: ConnectionManager,
    /// For pub/sub, which needs a connection of its own.
    client: redis::Client,
    namespace: String,
}

/// What a shard needs to resume its gateway session after a restart.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredSession {
    pub session_id: String,
    pub sequence: u64,
    pub resume_url: Option<String>,
    /// The intents the session was identified with; a resumed session keeps them, so a
    /// release that changes intents must identify afresh. Sessions saved before this field
    /// read as 0 and are not resumed.
    #[serde(default)]
    pub intents: u64,
}

impl Cache {
    /// Reserve one Riot request in both shared application windows. All pods use the same Redis clock.
    pub async fn reserve_riot(&self, region: &str) -> anyhow::Result<bool> {
        let key = format!("{}:riot:{}", self.namespace, region);
        let accepted: i64 = redis::Script::new(
            "if redis.call('EXISTS',KEYS[4])==1 then return 0 end; \
             local now=redis.call('TIME'); local ms=now[1]*1000+math.floor(now[2]/1000); \
             redis.call('ZREMRANGEBYSCORE',KEYS[1],0,ms-1000); \
             redis.call('ZREMRANGEBYSCORE',KEYS[2],0,ms-120000); \
             if redis.call('ZCARD',KEYS[1])>=20 or redis.call('ZCARD',KEYS[2])>=100 then return 0 end; \
             local n=redis.call('INCR',KEYS[3]); local member=tostring(ms)..':'..tostring(n); \
             redis.call('ZADD',KEYS[1],ms,member); redis.call('ZADD',KEYS[2],ms,member); \
             redis.call('PEXPIRE',KEYS[1],1000); redis.call('PEXPIRE',KEYS[2],120000); redis.call('PEXPIRE',KEYS[3],120000); return 1",
        )
        .key(format!("{key}:second"))
        .key(format!("{key}:two_minute"))
        .key(format!("{key}:nonce"))
        .key(format!("{key}:blocked"))
        .invoke_async(&mut self.conn())
        .await?;
        Ok(accepted == 1)
    }

    pub async fn block_riot(&self, region: &str, secs: u64) -> anyhow::Result<()> {
        let key = format!("{}:riot:{}:blocked", self.namespace, region);
        // Concurrent 429s must never shorten an existing shared backoff.
        let millis = secs.max(1).saturating_mul(1000).min(i64::MAX as u64 / 2);
        let _: i64 = redis::Script::new(
            "local old=redis.call('PTTL',KEYS[1]); if old<tonumber(ARGV[1]) then redis.call('PSETEX',KEYS[1],ARGV[1],1) end; return redis.call('PTTL',KEYS[1])",
        ).key(key).arg(millis).invoke_async(&mut self.conn()).await?;
        Ok(())
    }
    pub async fn connect(url: &str) -> anyhow::Result<Self> {
        let environment = std::env::var("PINCHORA_ENV").unwrap_or_else(|_| "default".into());
        Self::connect_scoped(url, &environment).await
    }

    /// Pub/Sub ignores SELECT. Include both deployment environment and database in every channel.
    pub async fn connect_scoped(url: &str, environment: &str) -> anyhow::Result<Self> {
        anyhow::ensure!(
            !environment.is_empty()
                && environment
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-'),
            "invalid PINCHORA_ENV"
        );
        let client = redis::Client::open(url)?;
        let namespace = format!(
            "pinchora:{environment}:db{}",
            client.get_connection_info().redis_settings().db()
        );
        let config = redis::aio::ConnectionManagerConfig::new()
            .set_connection_timeout(Some(Duration::from_secs(5)))
            .set_response_timeout(Some(Duration::from_secs(5)));
        let conn = ConnectionManager::new_with_config(client.clone(), config).await?;
        Ok(Self {
            conn,
            client,
            namespace,
        })
    }

    pub fn conn(&self) -> ConnectionManager {
        self.conn.clone()
    }

    pub async fn ping(&self) -> anyhow::Result<()> {
        let _: String = redis::cmd("PING").query_async(&mut self.conn()).await?;
        Ok(())
    }

    /// Starts a cooldown unless one is running.
    /// Returns `None` when it started now, or the time left on the running one.
    pub async fn try_cooldown(&self, key: &str, ttl: Duration) -> anyhow::Result<Option<Duration>> {
        let key = format!("{PREFIX}:cd:{key}");
        let mut conn = self.conn();
        let set: Option<String> = redis::cmd("SET")
            .arg(&key)
            .arg(1)
            .arg("NX")
            .arg("PX")
            .arg(ttl.as_millis() as u64)
            .query_async(&mut conn)
            .await?;
        if set.is_some() {
            return Ok(None);
        }
        let left: i64 = conn.pttl(&key).await?;
        Ok(Some(Duration::from_millis(left.max(0) as u64)))
    }

    /// Ends a cooldown started with [`Cache::try_cooldown`] early, for a step that turned out not to count.
    pub async fn release_cooldown(&self, key: &str) -> anyhow::Result<()> {
        let _: () = self.conn().del(format!("{PREFIX}:cd:{key}")).await?;
        Ok(())
    }

    /// Reserves a cooldown for one request. A failing request can release only its own reservation.
    pub async fn try_reserve(&self, key: &str, token: &str, ttl: Duration) -> anyhow::Result<Option<Duration>> {
        let key = format!("{PREFIX}:reserve:{key}");
        let mut conn = self.conn();
        let set: Option<String> = redis::cmd("SET")
            .arg(&key)
            .arg(token)
            .arg("NX")
            .arg("PX")
            .arg(ttl.as_millis() as u64)
            .query_async(&mut conn)
            .await?;
        if set.is_some() {
            return Ok(None);
        }
        let left: i64 = conn.pttl(&key).await?;
        Ok(Some(Duration::from_millis(left.max(0) as u64)))
    }

    pub async fn release_reservation(&self, key: &str, token: &str) -> anyhow::Result<bool> {
        let key = format!("{PREFIX}:reserve:{key}");
        let deleted: i64 = redis::Script::new(
            "if redis.call('GET', KEYS[1]) == ARGV[1] then return redis.call('DEL', KEYS[1]) else return 0 end",
        )
        .key(key)
        .arg(token)
        .invoke_async(&mut self.conn())
        .await?;
        Ok(deleted > 0)
    }

    pub async fn store_session(&self, shard: u32, session: &StoredSession, ttl: Duration) -> anyhow::Result<()> {
        let _: () = self
            .conn()
            .set_ex(
                format!("{PREFIX}:session:{shard}"),
                serde_json::to_string(session)?,
                ttl.as_secs(),
            )
            .await?;
        Ok(())
    }

    /// Returns and deletes the stored session, so it is only resumed once.
    pub async fn take_session(&self, shard: u32) -> anyhow::Result<Option<StoredSession>> {
        let raw: Option<String> = self.conn().get_del(format!("{PREFIX}:session:{shard}")).await?;
        Ok(raw.and_then(|r| serde_json::from_str(&r).ok()))
    }

    /// An identify queue shared by every gateway pod of this bot.
    pub fn identify_queue(&self, max_concurrency: u16) -> RedisQueue {
        RedisQueue::new(self.conn(), max_concurrency)
    }
}

impl Cache {
    /// Stores a short-lived value.
    pub async fn put(&self, key: &str, value: &str, ttl: Duration) -> anyhow::Result<()> {
        let _: () = self
            .conn()
            .set_ex(format!("{PREFIX}:kv:{key}"), value, ttl.as_secs().max(1))
            .await?;
        Ok(())
    }

    /// Overwrites a value stored with [`Cache::put`] only while it still exists; `false` when it is gone
    /// (nothing is written). For a session being updated: one that was logged out in the meantime must
    /// not come back.
    pub async fn put_if_present(&self, key: &str, value: &str, ttl: Duration) -> anyhow::Result<bool> {
        let set: Option<String> = redis::cmd("SET")
            .arg(format!("{PREFIX}:kv:{key}"))
            .arg(value)
            .arg("XX")
            .arg("EX")
            .arg(ttl.as_secs().max(1))
            .query_async(&mut self.conn())
            .await?;
        Ok(set.is_some())
    }

    /// Reads and deletes a value stored with [`Cache::put`].
    pub async fn take(&self, key: &str) -> anyhow::Result<Option<String>> {
        Ok(self.conn().get_del(format!("{PREFIX}:kv:{key}")).await?)
    }

    /// Reads a value stored with [`Cache::put`] and leaves it there.
    pub async fn get(&self, key: &str) -> anyhow::Result<Option<String>> {
        Ok(self.conn().get(format!("{PREFIX}:kv:{key}")).await?)
    }

    /// Atomically consumes one observed use from the same value namespace read by `get`.
    /// A missing baseline is initialized without crediting an old use.
    pub async fn claim_observed_value(&self, key: &str, observed: u64, ttl: Duration) -> anyhow::Result<bool> {
        let script = redis::Script::new(
            "local old=redis.call('GET',KEYS[1]); if not old then redis.call('SET',KEYS[1],ARGV[1],'EX',ARGV[2]); return 0 end; if tonumber(old)<tonumber(ARGV[1]) then redis.call('SET',KEYS[1],tonumber(old)+1,'EX',ARGV[2]); return 1 end; return 0",
        );
        let claimed: i32 = script
            .key(format!("{PREFIX}:kv:{key}"))
            .arg(observed)
            .arg(ttl.as_secs().max(1))
            .invoke_async(&mut self.conn())
            .await?;
        Ok(claimed == 1)
    }

    /// Pushes back the expiry of a value stored with [`Cache::put`]; `false` when it is gone.
    pub async fn extend(&self, key: &str, ttl: Duration) -> anyhow::Result<bool> {
        let set: i64 = self
            .conn()
            .expire(format!("{PREFIX}:kv:{key}"), ttl.as_secs().max(1) as i64)
            .await?;
        Ok(set == 1)
    }

    /// Deletes a value stored with [`Cache::put`].
    pub async fn forget(&self, key: &str) -> anyhow::Result<()> {
        let _: () = self.conn().del(format!("{PREFIX}:kv:{key}")).await?;
        Ok(())
    }
}

impl Cache {
    /// Increments a counter that expires after `ttl`; returns the new value.
    pub async fn incr(&self, key: &str, ttl: Duration) -> anyhow::Result<i64> {
        self.incr_by(key, 1, ttl).await
    }

    /// Adds `by` to a counter that expires after `ttl`; returns the new value. Concurrent adds each see
    /// a different total, so a cap can be shared out exactly.
    pub async fn incr_by(&self, key: &str, by: i64, ttl: Duration) -> anyhow::Result<i64> {
        let key = format!("{PREFIX}:n:{key}");
        let mut conn = self.conn();
        let (value, _): (i64, i64) = redis::pipe()
            .atomic()
            .incr(&key, by)
            .expire(&key, ttl.as_secs() as i64)
            .query_async(&mut conn)
            .await?;
        Ok(value)
    }

    pub async fn counter(&self, key: &str) -> anyhow::Result<i64> {
        let value: Option<i64> = self.conn().get(format!("{PREFIX}:n:{key}")).await?;
        Ok(value.unwrap_or(0))
    }

    /// True only the first time it is called for `key` within `ttl`.
    pub async fn once(&self, key: &str, ttl: Duration) -> anyhow::Result<bool> {
        let set: Option<String> = redis::cmd("SET")
            .arg(format!("{PREFIX}:once:{key}"))
            .arg(1)
            .arg("NX")
            .arg("EX")
            .arg(ttl.as_secs().max(1))
            .query_async(&mut self.conn())
            .await?;
        Ok(set.is_some())
    }
}

impl Cache {
    /// Sets one field of a hash. The whole hash expires `ttl` after its last write.
    pub async fn map_set(&self, key: &str, field: &str, value: &str, ttl: Duration) -> anyhow::Result<()> {
        let key = format!("{PREFIX}:h:{key}");
        let _: () = redis::pipe()
            .atomic()
            .hset(&key, field, value)
            .ignore()
            .expire(&key, ttl.as_secs().max(1) as i64)
            .ignore()
            .query_async(&mut self.conn())
            .await?;
        Ok(())
    }

    /// Replaces a whole hash with `entries` in one step; no entries deletes it. The hash expires `ttl`
    /// after this write.
    pub async fn map_replace(&self, key: &str, entries: &[(String, String)], ttl: Duration) -> anyhow::Result<()> {
        let key = format!("{PREFIX}:h:{key}");
        let mut pipe = redis::pipe();
        pipe.atomic().del(&key).ignore();
        if !entries.is_empty() {
            pipe.hset_multiple(&key, entries)
                .ignore()
                .expire(&key, ttl.as_secs().max(1) as i64)
                .ignore();
        }
        let _: () = pipe.query_async(&mut self.conn()).await?;
        Ok(())
    }

    /// Replaces a hash of voice presence records with `entries`, except for the fields written at or after
    /// `newer_than` (Unix seconds): those, whoever wrote them, stay exactly as they are, and a field of
    /// `entries` they cover is not written. A record is `channel|flags|seen|...`, and its third part is
    /// when it was written. Done in one step on the server, so an event that lands in between is not lost.
    /// The hash expires `ttl` after this write; no fields left deletes it.
    pub async fn map_merge(
        &self,
        key: &str,
        entries: &[(String, String)],
        newer_than: i64,
        ttl: Duration,
    ) -> anyhow::Result<()> {
        const SCRIPT: &str = r#"
            local newer = tonumber(ARGV[1])
            local kept = {}
            local fields = redis.call('HGETALL', KEYS[1])
            for i = 1, #fields, 2 do
                local seen = tonumber(string.match(fields[i + 1], '^[^|]*|[^|]*|(%-?%d+)'))
                if seen and seen >= newer then kept[fields[i]] = fields[i + 1] end
            end
            redis.call('DEL', KEYS[1])
            local written = 0
            for i = 3, #ARGV, 2 do
                if not kept[ARGV[i]] then
                    redis.call('HSET', KEYS[1], ARGV[i], ARGV[i + 1])
                    written = written + 1
                end
            end
            for field, value in pairs(kept) do
                redis.call('HSET', KEYS[1], field, value)
                written = written + 1
            end
            if written > 0 then redis.call('EXPIRE', KEYS[1], tonumber(ARGV[2])) end
            return written
        "#;
        let key = format!("{PREFIX}:h:{key}");
        let script = redis::Script::new(SCRIPT);
        let mut call = script.key(&key);
        call.arg(newer_than).arg(ttl.as_secs().max(1));
        for (field, value) in entries {
            call.arg(field).arg(value);
        }
        let _: i64 = call.invoke_async(&mut self.conn()).await?;
        Ok(())
    }

    /// Removes one field of a hash.
    pub async fn map_remove(&self, key: &str, field: &str) -> anyhow::Result<()> {
        let _: () = self.conn().hdel(format!("{PREFIX}:h:{key}"), field).await?;
        Ok(())
    }

    /// Every field and value of a hash; empty when there is none.
    pub async fn map_all(&self, key: &str) -> anyhow::Result<Vec<(String, String)>> {
        let all: std::collections::HashMap<String, String> = self.conn().hgetall(format!("{PREFIX}:h:{key}")).await?;
        Ok(all.into_iter().collect())
    }

    /// How many fields a hash has, without reading them; 0 when there is none.
    pub async fn map_len(&self, key: &str) -> anyhow::Result<usize> {
        Ok(self.conn().hlen(format!("{PREFIX}:h:{key}")).await?)
    }

    /// Adds a member to a set.
    pub async fn set_add(&self, key: &str, member: &str) -> anyhow::Result<()> {
        let _: () = self.conn().sadd(format!("{PREFIX}:s:{key}"), member).await?;
        Ok(())
    }

    /// Removes a member from a set.
    pub async fn set_remove(&self, key: &str, member: &str) -> anyhow::Result<()> {
        let _: () = self.conn().srem(format!("{PREFIX}:s:{key}"), member).await?;
        Ok(())
    }

    /// Every member of a set; empty when there is none.
    pub async fn set_members(&self, key: &str) -> anyhow::Result<Vec<String>> {
        Ok(self.conn().smembers(format!("{PREFIX}:s:{key}")).await?)
    }
}

mod automod;
mod verification;
pub use verification::VerificationLease;

mod anti_nuke;
