//! Discord message context menu. Uses the same guild policy and rate limit as /mod report.
use crate::Cx;
use twilight_model::application::{command::Command, interaction::application_command::CommandData};
pub(crate) fn definition() -> Command {
    serde_json::from_value(
        serde_json::json!({"name":"Report","type":3,"description":"","contexts":[0],"options":[],"version":"1"}),
    )
    .expect("message command")
}
pub(crate) async fn run(cx: &Cx, data: CommandData) -> anyhow::Result<()> {
    let Some(g) = cx.guild_id() else { return Ok(()) };
    let target = data
        .target_id
        .and_then(|id| data.resolved.as_ref().and_then(|r| r.messages.get(&id.cast())));
    let Some(msg) = target else {
        return cx.reply_ephemeral(pinchora_core::t!(cx.lang(), "parity.invalid")).await;
    };
    super::parity::submit_report(
        cx,
        g,
        msg.author.id.get(),
        Some(msg.id.get()),
        pinchora_core::t!(cx.lang(), "parity.context-reason"),
    )
    .await
}
