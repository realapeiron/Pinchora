//! Environment/database-isolated Reef transport and compare-and-renew leases.
use crate::Cache;
use futures_util::Stream;
use redis::AsyncCommands;
use std::time::Duration;
use tokio::sync::{mpsc, watch};

const SUBSCRIPTION_QUEUE: usize = 128;
const MAX_ROUTED_MESSAGE: usize = 256 * 1024;

impl Cache {
    pub fn namespace(&self) -> &str {
        &self.namespace
    }
    pub fn reef_key(&self, suffix: &str) -> String {
        format!("{}:reef:{suffix}", self.namespace)
    }
    pub async fn reef_subscribe(&self, channel: &str) -> anyhow::Result<impl Stream<Item = String> + Send + use<>> {
        // RESP2 PubSubStream hides an unbounded queue inside the Redis driver. RESP3's
        // synchronous push callback lets us bound the buffer at the transport boundary.
        let info = self.client.get_connection_info().clone();
        let settings = info
            .redis_settings()
            .clone()
            .set_protocol(redis::ProtocolVersion::RESP3);
        let client = redis::Client::open(info.set_redis_settings(settings))?;
        let (outgoing, incoming) = mpsc::channel(SUBSCRIPTION_QUEUE);
        let (failed, failure) = watch::channel(false);
        let config = redis::AsyncConnectionConfig::new()
            .set_connection_timeout(Some(Duration::from_secs(2)))
            .set_response_timeout(Some(Duration::from_secs(2)))
            .set_push_sender(move |push: redis::PushInfo| -> Result<(), redis::aio::SendError> {
                if *failed.borrow() {
                    return Err(redis::aio::SendError);
                }
                if push.kind == redis::PushKind::Disconnection {
                    failed.send_replace(true);
                } else if let Some(message) = redis::Msg::from_push_info(push) {
                    let accepted = match message.get_payload::<String>() {
                        Ok(body) if body.len() <= MAX_ROUTED_MESSAGE => outgoing.try_send(body).is_ok(),
                        _ => false,
                    };
                    if !accepted {
                        failed.send_replace(true);
                    }
                }
                Ok(())
            });
        let mut connection = client.get_multiplexed_async_connection_with_config(&config).await?;
        connection.subscribe(self.reef_key(channel)).await?;
        Ok(futures_util::stream::unfold(
            (connection, incoming, failure),
            |(connection, mut incoming, mut failure)| async move {
                if *failure.borrow() {
                    return None;
                }
                tokio::select! {
                    biased;
                    _ = failure.changed() => None,
                    body = incoming.recv() => body.map(|body| (body, (connection, incoming, failure))),
                }
            },
        ))
    }
    pub async fn reef_publish(&self, channel: &str, body: &str) -> anyhow::Result<()> {
        let _: usize = self.conn().publish(self.reef_key(channel), body).await?;
        Ok(())
    }
    pub async fn reef_get(&self, key: &str) -> anyhow::Result<Option<String>> {
        Ok(self.conn().get(self.reef_key(key)).await?)
    }
    pub async fn reef_claim(&self, key: &str, token: &str, ttl_ms: u64, replace: bool) -> anyhow::Result<bool> {
        let mut command = redis::cmd("SET");
        command.arg(self.reef_key(key)).arg(token).arg("PX").arg(ttl_ms);
        if !replace {
            command.arg("NX");
        }
        let set: Option<String> = command.query_async(&mut self.conn()).await?;
        Ok(set.is_some())
    }
    pub async fn reef_renew(&self, key: &str, token: &str, ttl_ms: u64) -> anyhow::Result<bool> {
        Ok(redis::Script::new("if redis.call('GET',KEYS[1]) == ARGV[1] then return redis.call('PEXPIRE',KEYS[1],ARGV[2]) else return 0 end")
            .key(self.reef_key(key)).arg(token).arg(ttl_ms).invoke_async::<i32>(&mut self.conn()).await? == 1)
    }
    pub async fn reef_release(&self, key: &str, token: &str) -> anyhow::Result<()> {
        let _: i32 = redis::Script::new(
            "if redis.call('GET',KEYS[1]) == ARGV[1] then return redis.call('DEL',KEYS[1]) else return 0 end",
        )
        .key(self.reef_key(key))
        .arg(token)
        .invoke_async(&mut self.conn())
        .await?;
        Ok(())
    }
    /// Publication and ownership validation are one Redis operation; a paused owner cannot publish.
    pub async fn reef_publish_fenced(
        &self,
        lease: &str,
        token: &str,
        channel: &str,
        body: &str,
    ) -> anyhow::Result<bool> {
        Ok(redis::Script::new(
            "if redis.call('GET',KEYS[1]) ~= ARGV[1] then return 0 end redis.call('PUBLISH',KEYS[2],ARGV[2]) return 1",
        )
        .key(self.reef_key(lease))
        .key(self.reef_key(channel))
        .arg(token)
        .arg(body)
        .invoke_async::<i32>(&mut self.conn())
        .await?
            == 1)
    }

    /// Keep ephemeral room state from being overwritten by a previous owner.
    pub async fn reef_set_fenced(
        &self,
        lease: &str,
        token: &str,
        key: &str,
        value: &str,
        ttl_ms: u64,
    ) -> anyhow::Result<bool> {
        Ok(redis::Script::new(
            "if redis.call('GET',KEYS[1]) ~= ARGV[1] then return 0 end redis.call('SET',KEYS[2],ARGV[2],'PX',ARGV[3]) return 1",
        )
        .key(self.reef_key(lease))
        .key(self.reef_key(key))
        .arg(token)
        .arg(value)
        .arg(ttl_ms)
        .invoke_async::<i32>(&mut self.conn()).await? == 1)
    }
}

#[cfg(test)]
mod tests {
    use crate::reef::*;
    use futures_util::StreamExt;

    #[tokio::test]
    async fn stalled_subscribers_close_instead_of_buffering_or_silently_losing_messages() {
        let Ok(url) = std::env::var("PINCHORA_TEST_REDIS_URL") else {
            return;
        };
        let cache = Cache::connect_scoped(&url, "bounded_subscription_test").await.unwrap();
        let mut stream = Box::pin(cache.reef_subscribe("slow").await.unwrap());
        for _ in 0..SUBSCRIPTION_QUEUE + 1 {
            cache.reef_publish("slow", "message").await.unwrap();
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
        assert_eq!(
            tokio::time::timeout(Duration::from_secs(1), stream.next())
                .await
                .unwrap(),
            None
        );
        let mut stream = Box::pin(cache.reef_subscribe("oversized").await.unwrap());
        cache
            .reef_publish("oversized", &"x".repeat(MAX_ROUTED_MESSAGE + 1))
            .await
            .unwrap();
        assert_eq!(
            tokio::time::timeout(Duration::from_secs(1), stream.next())
                .await
                .unwrap(),
            None
        );
    }
}
