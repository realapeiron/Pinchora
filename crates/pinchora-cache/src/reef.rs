//! Environment/database-isolated Reef transport and compare-and-renew leases.
use crate::Cache;
use futures_util::{Stream, StreamExt};
use redis::AsyncCommands;

impl Cache {
    pub fn namespace(&self) -> &str {
        &self.namespace
    }
    pub fn reef_key(&self, suffix: &str) -> String {
        format!("{}:reef:{suffix}", self.namespace)
    }
    pub async fn reef_subscribe(&self, channel: &str) -> anyhow::Result<impl Stream<Item = String> + Send + use<>> {
        let mut pubsub = self.client.get_async_pubsub().await?;
        pubsub.subscribe(self.reef_key(channel)).await?;
        Ok(pubsub
            .into_on_message()
            .filter_map(|m| async move { m.get_payload().ok() }))
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
}
