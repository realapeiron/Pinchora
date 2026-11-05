use pinchora_core::Config;
use twilight_http::Client;
use twilight_model::id::Id;

pub async fn run(config: &Config, global: bool) -> anyhow::Result<()> {
    let http = Client::new(config.discord.token.expose().to_owned());
    let client = http.interaction(Id::new(config.discord.application_id));
    let commands = pinchora_commands::command_definitions();
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
    Ok(())
}
