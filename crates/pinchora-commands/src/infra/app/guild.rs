use super::*;

impl App {
    pub async fn guild_settings(&self, guild_id: u64) -> anyhow::Result<Arc<GuildSettings>> {
        self.guild_settings
            .try_get_with(guild_id, async { self.db.guild_settings(guild_id).await.map(Arc::new) })
            .await
            .map_err(|e| anyhow::anyhow!("{e:#}"))
    }

    /// A server's settings changed here: forgets this pod's copy and tells every other pod through Redis to
    /// forget theirs, so the change counts from their next message (and voice minute) instead of after their
    /// caches run out. A failed announcement is only logged: the caches run out within minutes.
    pub async fn announce_guild_change(&self, guild_id: u64) {
        self.forget_guild_settings(guild_id).await;
        self.publish(pinchora_cache::Invalidation::Guild(guild_id)).await;
    }

    pub async fn automod_config(&self, guild: u64) -> anyhow::Result<Arc<AutoModConfig>> {
        self.automod_configs
            .try_get_with(guild, async { self.db.automod_config(guild).await.map(Arc::new) })
            .await
            .map_err(|e| anyhow::anyhow!("{e:#}"))
    }

    pub async fn forget_automod_config(&self, guild: u64) {
        self.automod_configs.invalidate(&guild).await;
    }

    /// What a server switched off, and where: its module rules, cached for a minute like its settings.
    pub async fn guild_modules(&self, guild_id: u64) -> anyhow::Result<Arc<GuildModules>> {
        self.module_rules
            .try_get_with(guild_id, async { self.db.guild_modules(guild_id).await.map(Arc::new) })
            .await
            .map_err(|e| anyhow::anyhow!("{e:#}"))
    }

    pub async fn command_control(
        &self,
        guild: u64,
    ) -> anyhow::Result<Arc<pinchora_core::admin::command_control::Config>> {
        if !self.guild_modules(guild).await?.rule(Module::CommandControl).enabled {
            return Ok(Arc::new(Default::default()));
        }
        self.command_control
            .try_get_with(guild, async { self.db.command_control(guild).await.map(Arc::new) })
            .await
            .map_err(|e| anyhow::anyhow!("{e:#}"))
    }

    /// Whether `module` may act in a server channel, following threads and categories.
    pub async fn module_access(&self, guild_id: u64, channel: Option<u64>, module: Module) -> anyhow::Result<Access> {
        let rule = self.guild_modules(guild_id).await?.rule(module);
        if !rule.needs_lineage() {
            return Ok(rule.access(&[]));
        }
        let lineage = match channel {
            Some(channel) => self.lineage(channel, false, |id| rule.names(id)).await,
            None => Vec::new(),
        };
        Ok(rule.access(&lineage))
    }

    /// [`App::module_access`] for events, which have nobody to answer: allowed unless the server
    /// switched the module off here. A lookup that fails reads as allowed, so a database hiccup
    /// never silences a server.
    pub async fn module_allowed(&self, guild_id: u64, channel: Option<u64>, module: Module) -> bool {
        match self.module_access(guild_id, channel, module).await {
            Ok(access) => access == Access::Allowed,
            Err(error) => {
                tracing::warn!(%error, guild = guild_id, module = module.key(), "module rules unavailable");
                true
            }
        }
    }

    pub async fn forget_guild_settings(&self, guild_id: u64) {
        self.antinuke_enabled.invalidate(&guild_id).await;
        self.branding_colors.invalidate(&guild_id).await;
        self.forget_automod_config(guild_id).await;
        self.guild_settings.invalidate(&guild_id).await;
        self.log_state.forget_config(guild_id).await;
        self.module_rules.invalidate(&guild_id).await;
        self.command_control.invalidate(&guild_id).await;
        self.voice_guilds.invalidate(&guild_id).await;
        self.custom.forget(guild_id).await;
    }
}
