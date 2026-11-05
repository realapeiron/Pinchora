use std::time::Instant;

use chrono::Utc;
use pinchora_core::snowflake;
use twilight_interactions::command::{CommandModel, CreateCommand};
use twilight_util::builder::embed::{EmbedBuilder, EmbedFieldBuilder, EmbedFooterBuilder};

use crate::{Cx, util::BRAND};

#[derive(CommandModel, CreateCommand)]
#[command(name = "ping", desc = "Check that Pinchora is awake and how fast it answers")]
pub struct Ping;

impl Ping {
    pub async fn run(self, cx: &Cx) -> anyhow::Result<()> {
        let lag = (Utc::now() - snowflake::timestamp(cx.interaction.id.get()))
            .num_milliseconds()
            .max(0);
        let t = Instant::now();
        cx.app.db.ping().await?;
        let db = t.elapsed().as_millis();
        let t = Instant::now();
        cx.app.cache.ping().await?;
        let redis = t.elapsed().as_millis();

        let embed = EmbedBuilder::new()
            .color(BRAND)
            .title("Pong!")
            .field(EmbedFieldBuilder::new("Interaction", format!("{lag} ms")).inline())
            .field(EmbedFieldBuilder::new("Database", format!("{db} ms")).inline())
            .field(EmbedFieldBuilder::new("Redis", format!("{redis} ms")).inline())
            .footer(EmbedFooterBuilder::new(format!("Node: {}", cx.app.node)))
            .build();
        cx.reply_embed(embed).await
    }
}
