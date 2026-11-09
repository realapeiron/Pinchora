use pinchora_core::Config;
use twilight_http::Client;
use twilight_model::id::Id;

pub async fn run(config: &Config, global: bool) -> anyhow::Result<()> {
    let http = Client::new(config.discord.token.expose().to_owned());
    let client = http.interaction(Id::new(config.discord.application_id));
    let commands = pinchora_commands::command_definitions();
    pinchora_commands::infra::routing::validate(&commands)?;
    let scope = config.discord.dev_guild_id.filter(|_| !global);
    let db = pinchora_db::Db::connect(config.database.url.expose(), 2).await?;
    let cache = pinchora_cache::Cache::connect(config.redis.url.expose()).await?;
    let old = match scope {
        Some(guild) => client.guild_commands(Id::new(guild)).await?.models().await?,
        None => client.global_commands().await?.models().await?,
    };
    let replacing = pinchora_commands::infra::routing::has_legacy_paths(&old);
    let ids: std::collections::BTreeMap<_, _> = old.into_iter().filter_map(|c| Some((c.name, c.id?.get()))).collect();
    if replacing {
        let mut guilds = Vec::new();
        if let Some(guild) = scope {
            guilds.push(guild);
        } else {
            // Enumerate Discord membership, including guilds that have no settings row yet.
            let mut after = None;
            loop {
                let mut request = http.current_user_guilds().limit(200);
                if let Some(id) = after {
                    request = request.after(id);
                }
                let batch = request.await?.models().await?;
                if batch.is_empty() {
                    break;
                }
                after = batch.iter().map(|g| g.id).max();
                guilds.extend(batch.iter().map(|g| g.id.get()));
                if batch.len() < 200 {
                    break;
                }
            }
        }
        // Every read and durable write must succeed before the destructive bulk overwrite.
        for guild in guilds {
            let entries = client.guild_command_permissions(Id::new(guild)).await?.models().await?;
            let snapshot = serde_json::json!({"version":1,"scope":scope,"ids":ids,"entries":entries});
            db.save_command_permissions(guild, config.discord.application_id, &snapshot)
                .await?;
        }
    }
    // Bulk overwrite: idempotent, and removes commands that no longer exist.
    match config.discord.dev_guild_id.filter(|_| !global) {
        Some(guild) => {
            client.set_guild_commands(Id::new(guild), &commands).await?;
            tracing::info!(count = commands.len(), guild, "registered guild commands");
        }
        None => {
            client.set_global_commands(&commands).await?;
            tracing::info!(count = commands.len(), "registered global commands");
        }
    }
    cache
        .publish_invalidation(pinchora_cache::Invalidation::Commands(config.discord.application_id))
        .await?;
    tracing::info!("command caches invalidated; re-check Integrations overrides on the new roots");
    Ok(())
}
