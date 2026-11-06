//! Atomic, bounded sliding windows. The caller supplies its clock so replay tests need no sleeps.
use crate::Cache;
impl Cache {
    #[allow(clippy::too_many_arguments)] // A scoped window, event identity, clock and optional content hash.
    pub async fn automod_window(
        &self,
        guild: u64,
        user: u64,
        rule: &str,
        message: u64,
        now_ms: i64,
        seconds: u32,
        duplicate: Option<u64>,
    ) -> anyhow::Result<u32> {
        let key = format!(
            "pinchora:am:{}:{guild}:{user}:{rule}",
            if duplicate.is_some() { "dup" } else { "spam" }
        );
        let script = r#"
            local now, window = tonumber(ARGV[1]), tonumber(ARGV[2]) * 1000
            local items = redis.call('LRANGE', KEYS[1], 0, 999)
            local kept, count, seen = {}, 0, false
            for _, raw in ipairs(items) do
                local item = cjson.decode(raw)
                if item.t > now - window and item.t <= now then
                    table.insert(kept, raw)
                    if item.h == ARGV[4] then count = count + 1 end
                    if item.m == ARGV[3] then seen = true end
                end
            end
            if not seen then
                table.insert(kept, 1, cjson.encode({t=now,m=ARGV[3],h=ARGV[4]}))
                count = count + 1
            end
            redis.call('DEL', KEYS[1])
            for i=1,math.min(#kept,1000) do redis.call('RPUSH', KEYS[1], kept[i]) end
            redis.call('EXPIRE', KEYS[1], ARGV[2])
            return count
        "#;
        Ok(redis::Script::new(script)
            .key(key)
            .arg(now_ms)
            .arg(seconds)
            .arg(message.to_string())
            .arg(duplicate.map(|n| n.to_string()).unwrap_or_default())
            .invoke_async(&mut self.conn())
            .await?)
    }
}
