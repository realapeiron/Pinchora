use super::*;

impl App {
    pub async fn render<T, F>(&self, job: F) -> anyhow::Result<T>
    where
        T: Send + 'static,
        F: FnOnce(&Renderer) -> anyhow::Result<T> + Send + 'static,
    {
        self.services.infrastructure.render(job).await
    }

    pub async fn render_preview<T, F>(&self, job: F) -> Result<T, PreviewError>
    where
        T: Send + 'static,
        F: FnOnce(&Renderer) -> anyhow::Result<T> + Send + 'static,
    {
        self.services.infrastructure.render_preview(job).await
    }

    /// A user's name and avatar as Discord has them, cached for ten minutes; `None` when
    /// Discord cannot find them (deleted accounts) or is unreachable.
    pub async fn user(&self, id: u64) -> Option<Arc<User>> {
        self.users
            .try_get_with(id, async {
                let user = self.http.user(Id::new(id.max(1))).await?.model().await?;
                anyhow::Ok(Arc::new(user))
            })
            .await
            .map_err(|error| tracing::debug!(%error, id, "user lookup failed"))
            .ok()
    }

    /// The user's avatar as a `size`² PNG, pixelated to `grid` when given. Falls back
    /// to a generated pixel creature when the CDN fails.
    pub async fn avatar(&self, user: &User, size: u32, grid: Option<u32>) -> Arc<Vec<u8>> {
        self.avatar_of(user.id, user.avatar, size, grid).await
    }

    /// [`App::avatar`] by id and avatar hash, for callers without a [`User`] (the dashboard).
    pub async fn avatar_of(
        &self,
        id: Id<UserMarker>,
        hash: Option<ImageHash>,
        size: u32,
        grid: Option<u32>,
    ) -> Arc<Vec<u8>> {
        let url = match hash {
            Some(hash) => format!("https://cdn.discordapp.com/avatars/{id}/{hash}.png?size=256"),
            None => format!("https://cdn.discordapp.com/embed/avatars/{}.png", (id.get() >> 22) % 6),
        };
        let key = format!("{url}|{size}|{grid:?}");
        let seed = id.get();
        self.avatars
            .get_with(key, async move {
                let processed = async {
                    let bytes = self.web.get(&url).send().await?.error_for_status()?.bytes().await?;
                    let png = self.render(move |_| prepare_avatar(&bytes, size, grid)).await?;
                    anyhow::Ok(png)
                }
                .await;
                Arc::new(processed.unwrap_or_else(|error| {
                    tracing::debug!(%error, "avatar fetch failed, using placeholder");
                    placeholder_avatar(seed, size)
                }))
            })
            .await
    }
}
