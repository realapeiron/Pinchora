use super::*;

impl App {
    async fn channel_place(&self, id: u64) -> Option<ChannelPlace> {
        if let Some(place) = self.channel_places.get(&id).await {
            return Some(place);
        }
        // A channel Discord could not tell us about a moment ago is not asked about again on every message.
        if self.channel_misses.contains_key(&id) {
            return None;
        }
        let asked = self
            .channel_places
            .try_get_with(id, async {
                let channel = self.http.channel(Id::new(id.max(1))).await?.model().await?;
                anyhow::Ok(ChannelPlace {
                    parent: channel.parent_id.map(|p| p.get()),
                    thread: channel.kind.is_thread(),
                })
            })
            .await;
        match asked {
            Ok(place) => Some(place),
            Err(error) => {
                tracing::debug!(%error, id, "channel lookup failed");
                self.channel_misses.insert(id, ()).await;
                None
            }
        }
    }

    /// Remembers where channels and threads sit, as GUILD_CREATE lists them, so Discord is never asked
    /// about them.
    pub(crate) async fn prefill_channel_places(&self, places: Vec<(u64, ChannelPlace)>) {
        for (id, place) in places {
            self.channel_places.insert(id, place).await;
            self.channel_misses.invalidate(&id).await;
        }
    }

    /// Whether `id`'s place is known without asking Discord, for tests.
    pub fn knows_channel_place(&self, id: u64) -> bool {
        self.channel_places.contains_key(&id)
    }

    /// A server's role names by id, cached for a minute; `None` when Discord cannot be asked.
    pub async fn guild_roles(&self, guild_id: u64) -> Option<Arc<HashMap<u64, String>>> {
        self.guild_roles
            .try_get_with(guild_id, async {
                let roles = self.http.roles(Id::new(guild_id.max(1))).await?.models().await?;
                anyhow::Ok(Arc::new(roles.into_iter().map(|r| (r.id.get(), r.name)).collect()))
            })
            .await
            .map_err(|error| tracing::debug!(%error, guild_id, "role lookup failed"))
            .ok()
    }

    /// A server's owner and roles, cached for a minute; asked for by text commands only.
    pub(crate) async fn permission_facts(&self, guild_id: u64) -> anyhow::Result<Arc<PermissionFacts>> {
        self.permission_facts
            .try_get_with(guild_id, async {
                let id = Id::new(guild_id.max(1));
                let guild = self.http.guild(id).await?.model().await?;
                let roles = self.http.roles(id).await?.models().await?;
                anyhow::Ok(Arc::new(PermissionFacts {
                    owner: guild.owner_id.get(),
                    roles,
                }))
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e:#}"))
    }

    /// Forgets a server's cached owner and roles, when either changed.
    pub async fn forget_permission_facts(&self, guild_id: u64) {
        self.permission_facts.invalidate(&guild_id).await;
    }

    /// Where command permissions are looked up in a channel and which overwrites apply there, cached
    /// for a minute. Fails when Discord cannot say, which a text command answers by refusing.
    pub(crate) async fn channel_rules(&self, channel: u64) -> anyhow::Result<Arc<ChannelRules>> {
        if self.channel_misses.contains_key(&channel) {
            anyhow::bail!("channel lookup recently failed");
        }
        let result = self
            .channel_rules
            .try_get_with(channel, async {
                let response = match self.http.channel(Id::new(channel.max(1))).await {
                    Ok(response) => response,
                    Err(error) => {
                        if crate::roles::roles::error_code(&error) == Some(10003) {
                            self.channel_misses.insert(channel, ()).await;
                        }
                        return Err(error.into());
                    }
                };
                let found = response.model().await?;
                self.channel_places
                    .insert(
                        channel,
                        ChannelPlace {
                            parent: found.parent_id.map(|id| id.get()),
                            thread: found.kind.is_thread(),
                        },
                    )
                    .await;
                let mut lineage = vec![channel];
                let mut overwrites = found.permission_overwrites.unwrap_or_default();
                if found.kind.is_thread()
                    && let Some(parent) = found.parent_id
                {
                    lineage.push(parent.get());
                    let parent = self.http.channel(parent).await?.model().await?;
                    overwrites = parent.permission_overwrites.unwrap_or_default();
                }
                anyhow::Ok(Arc::new(ChannelRules { lineage, overwrites }))
            })
            .await;
        result.map_err(|error| anyhow::anyhow!("{error:#}"))
    }

    /// Forgets what a channel's permissions were, when they changed.
    pub async fn forget_channel_rules(&self, channel: u64) {
        self.channel_rules.invalidate(&channel).await;
        self.channel_misses.invalidate(&channel).await;
    }

    /// `channel` followed by everything it sits under (a thread's channel, a channel's category), for
    /// the server log's channel ignore list.
    pub(crate) async fn place_lineage(&self, channel: u64) -> Vec<u64> {
        self.lineage(channel, false, |_| false).await
    }

    /// Complete ancestry for exclusion checks; an unknown parent fails closed.
    pub(crate) async fn automod_lineage(&self, channel: u64) -> Option<Vec<u64>> {
        let mut ids = vec![channel];
        let place = self.channel_place(channel).await?;
        if let Some(parent) = place.parent {
            ids.push(parent);
            if place.thread {
                let parent_place = self.channel_place(parent).await?;
                if let Some(category) = parent_place.parent {
                    ids.push(category);
                }
            }
        }
        Some(ids)
    }

    /// [`App::xp_lineage`] for any rule: `none` says the server has no channel rules at all, `has_rule`
    /// whether a channel has its own.
    pub(crate) async fn lineage(&self, channel: u64, none: bool, has_rule: impl Fn(u64) -> bool) -> Vec<u64> {
        let mut lineage = vec![channel];
        if none || has_rule(channel) {
            return lineage;
        }
        let Some(place) = self.channel_place(channel).await else {
            return lineage;
        };
        let Some(parent) = place.parent else {
            return lineage;
        };
        lineage.push(parent);
        if place.thread
            && !has_rule(parent)
            && let Some(category) = self.channel_place(parent).await.and_then(|p| p.parent)
        {
            lineage.push(category);
        }
        lineage
    }
}
