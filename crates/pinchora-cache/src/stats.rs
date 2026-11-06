//! Server stats counted in Redis between two flushes to Postgres.
//!
//! Each server has one hash per hour, bumped once per message (and per voice minute, join, leave and
//! command). A flush seals the hashes that changed: each is renamed to a batch key of its own, in one
//! step on the server, so counts that arrive while it is written start a fresh hash. A batch stays in
//! Redis until Postgres has it; Postgres remembers which batches it already took, so a flush cut short
//! and run again never counts anything twice.

use std::{collections::HashMap, time::Duration};

use redis::AsyncCommands;

use crate::{Cache, PREFIX};

/// How long counts wait for a flush at most: well past a stopped bot's restart, short enough that
/// Redis never fills up with them.
const KEEP: Duration = Duration::from_secs(3 * 86_400);

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
    /// Adds `fields` to what `guild` did in `hour` (hours since the Unix epoch), in one round trip.
    pub async fn stats_add(&self, guild: u64, hour: i64, fields: &[(String, i64)]) -> anyhow::Result<()> {
        if fields.is_empty() {
            return Ok(());
        }
        let member = format!("{guild}:{hour}");
        let key = format!("{}{member}", hash_prefix());
        let mut pipe = redis::pipe();
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
                    local target = ARGV[3] .. member .. ':' .. seq
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
}
