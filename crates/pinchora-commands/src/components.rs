use crate::{
    Cx,
    commands::{house, leaderboard},
};

/// Routes button clicks by the `prefix:` of their custom id. Ids carry all their
/// state, so any pod can serve any click.
pub(crate) async fn dispatch(cx: &Cx, custom_id: &str, values: &[String]) -> anyhow::Result<()> {
    let mut parts = custom_id.split(':');
    match parts.next() {
        Some("lb") => leaderboard::on_button(cx, parts).await,
        Some("he") => house::on_edit(cx, parts, values.first().map(String::as_str)).await,
        Some("hv") => house::on_view_nav(cx, parts).await,
        Some("hu") => house::on_upgrade(cx, parts).await,
        _ => cx.reply_ephemeral("This button has expired.").await,
    }
}
