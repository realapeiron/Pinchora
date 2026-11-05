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
}
