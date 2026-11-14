use super::*;

impl App {
    /// What a server set for the bot's commands under Server Settings, Integrations, cached for five
    /// minutes. A server that cannot have any (Discord says the application is not there, as for a bot
    /// added without the commands scope) has none; any other failure is an error, which a text
    /// command answers by refusing rather than guessing.
    pub(crate) async fn command_overrides(&self, guild_id: u64) -> anyhow::Result<Arc<CommandOverrides>> {
        self.command_overrides
            .try_get_with(guild_id, async {
                let entries = match self
                    .interaction()
                    .guild_command_permissions(Id::new(guild_id.max(1)))
                    .await
                {
                    Ok(response) => response.models().await?,
                    Err(error) if is_missing(&error) => Vec::new(),
                    Err(error) => return Err(error.into()),
                };
                if entries.is_empty() {
                    return anyhow::Ok(Arc::new(CommandOverrides::default()));
                }
                let ids = self.command_ids().await?;
                anyhow::Ok(Arc::new(CommandOverrides::new(
                    entries,
                    self.application_id.get(),
                    &ids,
                )))
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e:#}"))
    }

    /// Registration invalidates both caches together, on every serving process.
    pub async fn forget_command_registration(&self) {
        self.command_ids.invalidate_all();
        self.command_overrides.invalidate_all();
        self.saved_overrides.invalidate_all();
    }

    pub(crate) async fn saved_command_overrides(&self, guild: u64) -> anyhow::Result<Option<Arc<CommandOverrides>>> {
        self.saved_overrides
            .try_get_with(guild, async {
                let Some(value) = self
                    .db
                    .saved_command_permissions(guild, self.application_id.get())
                    .await?
                else {
                    return anyhow::Ok(None);
                };
                let entries = serde_json::from_value(value["entries"].clone())?;
                let ids = serde_json::from_value(value["ids"].clone())?;
                Ok(Some(Arc::new(CommandOverrides::new(
                    entries,
                    self.application_id.get(),
                    &ids,
                ))))
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e:#}"))
    }

    /// Forgets a server's command overrides, when they changed.
    pub async fn forget_command_overrides(&self, guild_id: u64) {
        self.command_overrides.invalidate(&guild_id).await;
        self.command_ids.invalidate_all();
    }

    /// The ids of the bot's commands by name, which tell the entries of Discord's command permissions
    /// apart; cached for ten minutes.
    async fn command_ids(&self) -> anyhow::Result<Arc<HashMap<String, u64>>> {
        self.command_ids
            .try_get_with((), async {
                let commands = match self.interaction().global_commands().await {
                    Ok(response) => response.models().await?,
                    Err(error) if is_missing(&error) => Vec::new(),
                    Err(error) => return Err(error.into()),
                };
                anyhow::Ok(Arc::new(
                    commands
                        .into_iter()
                        .filter_map(|c| Some((c.name, c.id?.get())))
                        .collect(),
                ))
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e:#}"))
    }
}
