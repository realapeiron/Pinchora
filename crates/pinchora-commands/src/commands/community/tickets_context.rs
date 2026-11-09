//! Open a ticket from the Discord message context menu, retaining a link to the source message.
use crate::Cx;
use pinchora_core::t;
use twilight_model::application::{command::Command, interaction::application_command::CommandData};

pub(crate) fn definition() -> Command {
    serde_json::from_value(serde_json::json!({
        "name": "Open ticket",
        "name_localizations": {"tr": "Bilet aç"},
        "type": 3,
        "description": "",
        "contexts": [0],
        "options": [],
        "version": "1"
    }))
    .expect("message command")
}

pub(crate) async fn run(cx: &Cx, data: CommandData) -> anyhow::Result<()> {
    let Some(guild) = cx.guild_id() else {
        return cx.reply_ephemeral(t!(cx.lang(), "tickets.guild-only")).await;
    };
    let Some(message) = data.target_id.and_then(|id| {
        data.resolved
            .as_ref()
            .and_then(|resolved| resolved.messages.get(&id.cast()))
    }) else {
        return cx.reply_ephemeral(t!(cx.lang(), "tickets.expired")).await;
    };
    crate::community::tickets::on_message_context(cx, guild, message.channel_id.get(), message.id.get()).await
}
