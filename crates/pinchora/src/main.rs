mod gateway;
mod http;
mod register;

use std::sync::Arc;

use clap::{Parser, Subcommand};
use pinchora_cache::Cache;
use pinchora_commands::App;
use pinchora_core::{
    Config,
    telemetry::{self, Readiness},
};
use pinchora_db::Db;
use twilight_http::Client;
use twilight_model::id::Id;

#[cfg(all(target_os = "linux", not(target_env = "msvc")))]
#[global_allocator]
static GLOBAL: tikv_jemallocator::Jemalloc = tikv_jemallocator::Jemalloc;

#[derive(Parser)]
#[command(name = "pinchora", version, about = "Pinchora Discord bot")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Run one gateway shard cluster: chat XP, welcome cards, and interactions when
    /// no Interactions Endpoint URL is set.
    Gateway,
    /// Run the HTTP interactions endpoint (stateless, scale it horizontally).
    Interactions,
    /// Run the web dashboard: Discord login, server settings, served on `web.bind`.
    Web,
    /// Apply database migrations.
    Migrate,
    /// Register slash commands: to `discord.dev_guild_id` when set, else globally.
    Register {
        /// Register globally even when a dev guild is configured.
        #[arg(long)]
        global: bool,
    },
    /// Print the slash command definitions as JSON.
    Commands,
}

fn main() -> anyhow::Result<()> {
    // Command handlers are deep async state machines; workers and render threads get more
    // stack than tokio's 2 MiB default.
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .thread_stack_size(8 * 1024 * 1024)
        .build()?
        .block_on(run())
}

async fn run() -> anyhow::Result<()> {
    let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
    let cli = Cli::parse();
    if let Command::Commands = cli.command {
        println!(
            "{}",
            serde_json::to_string_pretty(&pinchora_commands::command_definitions())?
        );
        return Ok(());
    }

    let config = Config::load()?;
    telemetry::init_tracing(&config.log);
    match cli.command {
        Command::Migrate => {
            let db = Db::connect(config.database.url.expose(), 2).await?;
            db.migrate().await?;
            tracing::info!("migrations applied");
        }
        Command::Register { global } => register::run(&config, global).await?,
        Command::Gateway => {
            let (app, ready) = boot(&config, "gateway").await?;
            gateway::run(&config, app, ready).await?;
        }
        Command::Interactions => {
            let (app, ready) = boot(&config, "interactions").await?;
            http::run(&config, app, ready).await?;
        }
        Command::Web => {
            let (app, ready) = boot(&config, "web").await?;
            pinchora_web::run(&config, app, ready).await?;
        }
        Command::Commands => unreachable!(),
    }
    Ok(())
}

/// Connects everything a serving process needs and starts the ops server.
async fn boot(config: &Config, role: &str) -> anyhow::Result<(Arc<App>, Readiness)> {
    let metrics = telemetry::install_metrics()?;
    let ready = Readiness::default();
    tokio::spawn(telemetry::serve_ops(config.ops.bind, metrics, ready.clone()));

    let db = Db::connect(config.database.url.expose(), config.database.max_connections).await?;
    let cache = Cache::connect(config.redis.url.expose()).await?;

    let token = config.discord.token.expose().to_owned();
    let mut rest = Client::builder().token(token.clone());
    if let Some(proxy) = &config.discord.http_proxy {
        // The proxy owns ratelimiting for every pod; a local limiter would double-count.
        rest = rest.proxy(proxy.clone(), true).ratelimiter(None);
    }
    let interactions = Client::builder().token(token).build();

    let node = std::env::var("HOSTNAME").unwrap_or_else(|_| role.to_owned());
    let mut app = App::new(
        Arc::new(rest.build()),
        Arc::new(interactions),
        Id::new(config.discord.application_id),
        db,
        cache,
        node,
    );
    app.message_content = config.gateway.message_content;
    app.public_url.clone_from(&config.web.public_url);
    let app = Arc::new(app);
    // Drops cached settings the moment the dashboard changes them, on every pod.
    pinchora_commands::listen_for_invalidations(&app).await;
    // Ends duels and rounds whose clock ran out, so no wager stays held after a restart.
    pinchora_commands::spawn_game_janitor(app.clone());
    pinchora_commands::spawn_mod_parity_ticker(app.clone());
    pinchora_commands::spawn_leveling_parity_ticker(app.clone());
    // Sends the reminders that came due; every pod runs it and the table's leases keep one from
    // repeating another.
    pinchora_commands::spawn_reminder_ticker(app.clone());
    // Ends giveaways and closes polls when their time is up, on whichever pod gets there first.
    pinchora_commands::spawn_events_ticker(app.clone());
    pinchora_commands::spawn_automod_ticker(app.clone());
    pinchora_commands::spawn_invite_reward_ticker(app.clone());
    pinchora_commands::spawn_verification_ticker(app.clone());
    pinchora_commands::spawn_antinuke_ticker(app.clone());
    // Lifts temporary bans whose time is up, on whichever pod claims them first.
    pinchora_commands::spawn_moderation_ticker(app.clone());
    // Retires critter drops and bottles nobody took in time.
    pinchora_commands::spawn_collection_ticker(app.clone());
    // Moves the party games on: pops a pearl, ends a writing or voting phase, closes a lobby or a poll.
    pinchora_commands::spawn_party_ticker(app.clone());
    // Gives the auto roles that were waiting out a delay, and takes back the roles whose time ran out.
    pinchora_commands::spawn_roles_ticker(app.clone());
    // Writes the server stats counted in Redis to Postgres every minute and keeps counter channels' names current.
    pinchora_commands::spawn_stats_ticker(app.clone());
    pinchora_commands::spawn_starboard_ticker(app.clone());
    pinchora_commands::spawn_utility_ticker(app.clone());
    // Closes tickets nobody wrote in for longer than their server allows.
    pinchora_commands::spawn_tickets_ticker(app.clone());
    pinchora_commands::spawn_birthdays_ticker(app.clone());
    pinchora_commands::spawn_feeds_ticker(app.clone());
    pinchora_commands::spawn_roblox_ticker(app.clone());
    pinchora_commands::spawn_trackers_ticker(app.clone());
    pinchora_commands::spawn_event_ticker(app.clone());
    pinchora_commands::spawn_community_ticker(app.clone());
    // Voice states only arrive on the gateway; its ticker pays everyone sitting in voice.
    if role == "gateway" {
        pinchora_commands::spawn_voice_ticker(app.clone());
        // Deletes the temp voice rooms nobody is in, in case a leave was never heard.
        pinchora_commands::spawn_tempvoice_ticker(app.clone());
    }
    Ok((app, ready))
}
