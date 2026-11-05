use std::{fmt, time::Duration};

use redis::aio::ConnectionManager;
use tokio::sync::oneshot;
use twilight_gateway_queue::{IDENTIFY_DELAY, Queue};

/// Gateway identify queue backed by Redis.
///
/// Discord allows `max_concurrency` identifies per 5 seconds, one per bucket
/// (`shard_id % max_concurrency`). Shards on different pods share that limit, so each
/// bucket is a Redis key that lives for 5 seconds: whoever sets it may identify.
#[derive(Clone)]
pub struct RedisQueue {
    conn: ConnectionManager,
    max_concurrency: u16,
}

impl RedisQueue {
    pub fn new(conn: ConnectionManager, max_concurrency: u16) -> Self {
        Self {
            conn,
            max_concurrency: max_concurrency.max(1),
        }
    }
}

impl fmt::Debug for RedisQueue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RedisQueue")
            .field("max_concurrency", &self.max_concurrency)
            .finish_non_exhaustive()
    }
}

impl Queue for RedisQueue {
    fn enqueue(&self, shard: u32) -> oneshot::Receiver<()> {
        let (tx, rx) = oneshot::channel();
        let mut conn = self.conn.clone();
        let key = format!("pinchora:identify:{}", shard % u32::from(self.max_concurrency));
        tokio::spawn(async move {
            while !tx.is_closed() {
                let won: redis::RedisResult<Option<String>> = redis::cmd("SET")
                    .arg(&key)
                    .arg(shard)
                    .arg("NX")
                    .arg("PX")
                    .arg(IDENTIFY_DELAY.as_millis() as u64)
                    .query_async(&mut conn)
                    .await;
                match won {
                    Ok(Some(_)) => {
                        tracing::debug!(shard, "identify slot acquired");
                        let _ = tx.send(());
                        return;
                    }
                    Ok(None) => {}
                    Err(error) => tracing::warn!(shard, %error, "identify queue redis error"),
                }
                tokio::time::sleep(Duration::from_millis(250)).await;
            }
        });
        rx
    }
}
