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
    let app = App::new(
        Arc::new(rest.build()),
        Arc::new(interactions),
        Id::new(config.discord.application_id),
        db,
        cache,
        node,
    );
    let app = Arc::new(app);
    // Ends duels and rounds whose clock ran out, so no wager stays held after a restart.
    pinchora_commands::spawn_game_janitor(app.clone());
    // Ends giveaways and closes polls when their time is up, on whichever pod gets there first.
    pinchora_commands::spawn_events_ticker(app.clone());
    // Retires critter drops and bottles nobody took in time.
    pinchora_commands::spawn_collection_ticker(app.clone());
    // Moves the party games on: pops a pearl, ends a writing or voting phase, closes a lobby or a poll.
    pinchora_commands::spawn_party_ticker(app.clone());
    // Voice states only arrive on the gateway; its ticker pays everyone sitting in voice.
    if role == "gateway" {
        pinchora_commands::spawn_voice_ticker(app.clone());
    }
    Ok((app, ready))
}
