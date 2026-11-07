//! Bounded, atomic per-actor rolling windows. Audit IDs stay strings (Discord snowflakes exceed
//! Lua's exact integer range); a prune may count several removed members in one audit entry.
use crate::Cache;
impl Cache {
    #[allow(clippy::too_many_arguments)]
    pub async fn antinuke_window(
        &self,
        guild: u64,
        actor: u64,
        kind: &str,
        event: u64,
        now_ms: i64,
        seconds: u32,
        weight: u32,
    ) -> anyhow::Result<u32> {
        anyhow::ensure!(
            (5..=300).contains(&seconds) && (1..=1000).contains(&weight),
            "invalid anti-nuke window"
        );
        let script = r#"
        local now, window = tonumber(ARGV[1]), tonumber(ARGV[2])*1000
        local kept, count, seen = {}, 0, false
        for _, raw in ipairs(redis.call('LRANGE',KEYS[1],0,999)) do
            local item=cjson.decode(raw)
            if item.t>now-window and item.t<=now then
                table.insert(kept,raw)
                count=count+item.n
                if item.id==ARGV[3] then seen=true end
            end
        end
        if not seen then
            table.insert(kept,1,cjson.encode({t=now,id=ARGV[3],n=tonumber(ARGV[4])}))
            count=count+tonumber(ARGV[4])
        end
        redis.call('DEL',KEYS[1])
        for i=1,math.min(#kept,1000) do redis.call('RPUSH',KEYS[1],kept[i]) end
        redis.call('EXPIRE',KEYS[1],ARGV[2])
        return count
        "#;
        Ok(redis::Script::new(script)
            .key(format!("pinchora:an:{guild}:{actor}:{kind}"))
            .arg(now_ms)
            .arg(seconds)
            .arg(event.to_string())
            .arg(weight)
            .invoke_async(&mut self.conn())
            .await?)
    }
}
