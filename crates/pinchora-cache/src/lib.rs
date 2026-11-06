mod queue;

use std::time::Duration;

use redis::{AsyncCommands, aio::ConnectionManager};
use serde::{Deserialize, Serialize};

pub use queue::RedisQueue;

const PREFIX: &str = "pinchora";

/// Handle to Redis. One multiplexed, auto-reconnecting connection; cheap to clone.
#[derive(Clone)]
pub struct Cache {
    conn: ConnectionManager,
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
    pub async fn connect(url: &str) -> anyhow::Result<Self> {
        let client = redis::Client::open(url)?;
        let config = redis::aio::ConnectionManagerConfig::new()
            .set_connection_timeout(Some(Duration::from_secs(5)))
            .set_response_timeout(Some(Duration::from_secs(5)));
        let conn = ConnectionManager::new_with_config(client, config).await?;
        Ok(Self { conn })
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

    /// Reads and deletes a value stored with [`Cache::put`].
    pub async fn take(&self, key: &str) -> anyhow::Result<Option<String>> {
        Ok(self.conn().get_del(format!("{PREFIX}:kv:{key}")).await?)
    }

    /// Reads a value stored with [`Cache::put`] and leaves it there.
    pub async fn get(&self, key: &str) -> anyhow::Result<Option<String>> {
        Ok(self.conn().get(format!("{PREFIX}:kv:{key}")).await?)
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
        let key = format!("{PREFIX}:n:{key}");
        let mut conn = self.conn();
        let (value, _): (i64, i64) = redis::pipe()
            .atomic()
            .incr(&key, 1)
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
