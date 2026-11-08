use crate::{Cache, PREFIX};
use std::time::Duration;

impl Cache {
    /// Swap the observed role snapshot atomically so duplicate callbacks produce no delta.
    pub async fn replace_workflow_roles(&self, key: &str, roles: &str) -> anyhow::Result<Option<String>> {
        Ok(redis::cmd("SET")
            .arg(format!("{PREFIX}:kv:{key}"))
            .arg(roles)
            .arg("EX")
            .arg(86400)
            .arg("GET")
            .query_async(&mut self.conn())
            .await?)
    }
    /// Claim a workflow's cooldown conditions together; a refusal consumes none of them.
    pub async fn claim_workflow_cooldowns(&self, keys: &[(String, Duration)]) -> anyhow::Result<Vec<usize>> {
        if keys.is_empty() {
            return Ok(Vec::new());
        }
        let script = redis::Script::new(
            "local blocked = {}; for i,key in ipairs(KEYS) do \
             if redis.call('EXISTS',key)==1 then table.insert(blocked,i-1) end end; \
             if #blocked>0 then return blocked end; \
             for i,key in ipairs(KEYS) do redis.call('PSETEX',key,ARGV[i],1) end; return blocked",
        );
        let mut call = script.prepare_invoke();
        for (key, ttl) in keys {
            call.key(format!("{PREFIX}:cd:{key}"))
                .arg(ttl.as_millis().max(1) as u64);
        }
        Ok(call.invoke_async(&mut self.conn()).await?)
    }
}
