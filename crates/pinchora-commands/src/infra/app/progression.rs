use super::*;

impl App {
    /// A server's XP multipliers, cached for a minute like its settings.
    pub async fn xp_rules(&self, guild_id: u64) -> anyhow::Result<Arc<XpRules>> {
        self.xp_rules
            .try_get_with(guild_id, async { self.db.xp_rules(guild_id).await.map(Arc::new) })
            .await
            .map_err(|e| anyhow::anyhow!("{e:#}"))
    }

    /// Read the authoritative policy so curve changes are consistent across pods.
    pub async fn leveling_policy(&self, guild_id: u64) -> anyhow::Result<Arc<LevelingPolicy>> {
        self.db.leveling_policy(guild_id).await.map(Arc::new)
    }

    pub async fn forget_xp_rules(&self, guild_id: u64) {
        self.xp_rules.invalidate(&guild_id).await;
        self.voice_guilds.invalidate(&guild_id).await;
    }

    /// [`App::announce_guild_change`] for a server's XP multipliers.
    pub async fn announce_xp_change(&self, guild_id: u64) {
        self.forget_xp_rules(guild_id).await;
        self.publish(pinchora_cache::Invalidation::XpRules(guild_id)).await;
    }

    pub(super) async fn publish(&self, what: pinchora_cache::Invalidation) {
        if let Err(error) = self.cache.publish_invalidation(what).await {
            tracing::warn!(%error, ?what, "invalidation not published; other pods catch up as their caches run out");
        }
    }

    /// The settings and multipliers voice XP is paid by, cached for five minutes.
    pub(crate) async fn voice_guild(&self, guild_id: u64) -> anyhow::Result<Arc<VoiceGuild>> {
        self.voice_guilds
            .try_get_with(guild_id, async {
                let settings = self.db.guild_settings(guild_id).await?;
                let rules = self.db.xp_rules(guild_id).await?;
                anyhow::Ok(Arc::new(VoiceGuild {
                    settings: Arc::new(settings),
                    rules: Arc::new(rules),
                }))
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e:#}"))
    }

    pub(crate) async fn forget_voice_guild(&self, guild_id: u64) {
        self.voice_guilds.invalidate(&guild_id).await;
    }

    /// `channel` followed by what it sits under, nearest first: a thread or forum post's channel, then
    /// that channel's category. Only looked up as far as `rules` needs: nothing when the server has no
    /// channel rules or the channel has its own. Discord is asked once per channel and the answer
    /// kept; a lookup that fails just leaves the lineage short.
    pub async fn xp_lineage(&self, rules: &XpRules, channel: u64) -> Vec<u64> {
        self.lineage(channel, rules.channels.is_empty(), |id| {
            rules.channels.contains_key(&id)
        })
        .await
    }

    /// Counts progress on today's quest of `kind`, if the player has that quest today.
    /// Failures are logged and swallowed: quest progress never breaks a command.
    pub async fn quest(&self, user: u64, kind: pinchora_game::quests::QuestKind) {
        let day = chrono::Utc::now().timestamp().div_euclid(86_400);
        if !pinchora_game::quests::for_day(user, day).iter().any(|q| q.kind == kind) {
            return;
        }
        let key = format!("quest:{day}:{user}:{}", kind.key());
        if let Err(error) = self.cache.incr(&key, Duration::from_secs(2 * 86_400)).await {
            tracing::debug!(%error, "quest progress not saved");
        }
    }

    /// Counts something the player did for their `/wrapped`. Failures are logged and swallowed: the
    /// numbers never break a command.
    pub async fn count(&self, user: u64, counter: pinchora_db::Counter, n: i64) {
        if let Err(error) = self.db.bump_activity(user, counter, n).await {
            tracing::debug!(%error, user, "activity not counted");
        }
    }
}
