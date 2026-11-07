//! Captcha state changes are atomic across pods. Only hashes are stored, for ten minutes.
use crate::Cache;
impl Cache {
    pub async fn captcha_issue(&self, guild: u64, user: u64, nonce: &str, hash: &str) -> anyhow::Result<bool> {
        let script = r#"
          if tonumber(redis.call('GET',KEYS[2]) or '0')>=3 then return 0 end
          if redis.call('EXISTS',KEYS[3])==1 then return 0 end
          redis.call('SET',KEYS[1],ARGV[1]..':'..ARGV[2],'EX',600)
          redis.call('SET',KEYS[3],1,'EX',30)
          return 1
        "#;
        let result: i32 = redis::Script::new(script)
            .key(format!("pinchora:cq:{guild}:{user}"))
            .key(format!("pinchora:cq:attempts:{guild}:{user}"))
            .key(format!("pinchora:cq:issue:{guild}:{user}"))
            .arg(nonce)
            .arg(hash)
            .invoke_async(&mut self.conn())
            .await?;
        Ok(result == 1)
    }
    /// 1 success (consumed), 0 wrong, -1 expired/replaced, -2 cooldown. A wrong nonce never burns attempts.
    pub async fn captcha_answer(&self, guild: u64, user: u64, nonce: &str, hash: &str) -> anyhow::Result<i32> {
        let script = r#"
          if tonumber(redis.call('GET',KEYS[2]) or '0')>=3 then return -2 end
          local value=redis.call('GET',KEYS[1])
          if not value or string.sub(value,1,string.len(ARGV[1])+1)~=ARGV[1]..':' then return -1 end
          if value==ARGV[1]..':'..ARGV[2] then
            redis.call('DEL',KEYS[1],KEYS[3]); return 1
          end
          redis.call('DEL',KEYS[1],KEYS[3])
          local n=redis.call('INCR',KEYS[2])
          if n==1 then redis.call('EXPIRE',KEYS[2],900) end
          if n>=3 then redis.call('DEL',KEYS[1]);redis.call('EXPIRE',KEYS[2],900);return -2 end
          return 0
        "#;
        Ok(redis::Script::new(script)
            .key(format!("pinchora:cq:{guild}:{user}"))
            .key(format!("pinchora:cq:attempts:{guild}:{user}"))
            .key(format!("pinchora:cq:issue:{guild}:{user}"))
            .arg(nonce)
            .arg(hash)
            .invoke_async(&mut self.conn())
            .await?)
    }
}
impl Cache {
    /// A leave/rejoin invalidates old cards, while retaining the abuse cooldown.
    pub async fn captcha_clear(&self, guild: u64, user: u64) -> anyhow::Result<()> {
        let _: i32 = redis::cmd("DEL")
            .arg(format!("pinchora:cq:{guild}:{user}"))
            .query_async(&mut self.conn())
            .await?;
        Ok(())
    }
}
impl Cache {
    /// A bounded sliding join window with one entry per member (gateway replays do not count twice).
    pub async fn raid_window(&self, guild: u64, user: u64, now_ms: i64, seconds: u32) -> anyhow::Result<u32> {
        let script = r#"
          redis.call('ZREMRANGEBYSCORE',KEYS[1],'-inf',tonumber(ARGV[1])-tonumber(ARGV[2])*1000)
          redis.call('ZADD',KEYS[1],ARGV[1],ARGV[3])
          redis.call('ZREMRANGEBYRANK',KEYS[1],0,-1001)
          redis.call('EXPIRE',KEYS[1],ARGV[2])
          return redis.call('ZCARD',KEYS[1])
        "#;
        Ok(redis::Script::new(script)
            .key(format!("pinchora:raid:{guild}"))
            .arg(now_ms)
            .arg(seconds)
            .arg(user.to_string())
            .invoke_async(&mut self.conn())
            .await?)
    }
}

/// Renewable ownership of a safety transition, without occupying a database connection during REST.
/// The owner token prevents an old worker from deleting a replacement worker's lease.
pub struct VerificationLease {
    cache: Cache,
    key: String,
    owner: String,
    heartbeat: tokio::task::JoinHandle<()>,
}
impl Cache {
    pub async fn verification_lease(&self, key: &str, owner: &str) -> anyhow::Result<Option<VerificationLease>> {
        let key = format!("pinchora:vf:lease:{key}");
        let won: Option<String> = redis::cmd("SET")
            .arg(&key)
            .arg(owner)
            .arg("NX")
            .arg("EX")
            .arg(180)
            .query_async(&mut self.conn())
            .await?;
        if won.is_none() {
            return Ok(None);
        }
        let cache = self.clone();
        let renewal_key = key.clone();
        let renewal_owner = owner.to_owned();
        let heartbeat = tokio::spawn(async move {
            loop {
                tokio::time::sleep(std::time::Duration::from_secs(30)).await;
                let renewed: Result<i32, _> = redis::Script::new(
                    "if redis.call('GET',KEYS[1])==ARGV[1] then return redis.call('EXPIRE',KEYS[1],180) end return 0",
                )
                .key(&renewal_key)
                .arg(&renewal_owner)
                .invoke_async(&mut cache.conn())
                .await;
                if !matches!(renewed, Ok(1)) {
                    break;
                }
            }
        });
        Ok(Some(VerificationLease {
            cache: self.clone(),
            key,
            owner: owner.to_owned(),
            heartbeat,
        }))
    }
}
impl Drop for VerificationLease {
    fn drop(&mut self) {
        self.heartbeat.abort();
        let cache = self.cache.clone();
        let key = self.key.clone();
        let owner = self.owner.clone();
        if let Ok(runtime) = tokio::runtime::Handle::try_current() {
            runtime.spawn(async move {
                let _: Result<i32, _> = redis::Script::new(
                    "if redis.call('GET',KEYS[1])==ARGV[1] then return redis.call('DEL',KEYS[1]) end return 0",
                )
                .key(key)
                .arg(owner)
                .invoke_async(&mut cache.conn())
                .await;
            });
        }
    }
}

impl Cache {
    /// Fleet-wide admission before safety REST, leaving headroom for ordinary bot traffic.
    pub async fn verification_rest_slot(&self, application: u64) -> anyhow::Result<bool> {
        let result: i32 = redis::Script::new(
            r#"
            local t=redis.call('TIME')
            local now=tonumber(t[1])*1000+math.floor(tonumber(t[2])/1000)
            local next=tonumber(redis.call('GET',KEYS[1]) or '0')
            if next>now then return 0 end
            redis.call('SET',KEYS[1],now+50,'PX',1000)
            return 1
        "#,
        )
        .key(format!("pinchora:vf:rest:{application}"))
        .invoke_async(&mut self.conn())
        .await?;
        Ok(result == 1)
    }
}
