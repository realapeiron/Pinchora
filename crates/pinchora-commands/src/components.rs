use crate::{
    Cx,
    commands::{
        character, customize, decorate, duel, fish, gift, house, leaderboard, pet, plaza, quests, role_panels, shop,
        stall, store,
    },
};

/// Routes button clicks by the `prefix:` of their custom id. Ids carry all their
/// state, so any pod can serve any click.
pub(crate) async fn dispatch(cx: &Cx, custom_id: &str, values: &[String]) -> anyhow::Result<()> {
    let mut parts = custom_id.split(':');
    match parts.next() {
        Some("lb") => leaderboard::on_button(cx, parts).await,
        Some("he") => house::on_edit(cx, "he", parts, values.first().map(String::as_str)).await,
        Some("pe") => house::on_edit(cx, "pe", parts, values.first().map(String::as_str)).await,
        Some(kind @ ("dw" | "df" | "dn")) => {
            decorate::on_component(cx, kind, parts, values.first().map(String::as_str)).await
        }
        Some("pd") => plaza::on_donate(cx, values.first().map(String::as_str)).await,
        Some("hv") => house::on_view_nav(cx, parts).await,
        Some("hu") => house::on_upgrade(cx, parts).await,
        Some("mk") => shop::on_market_buy(cx, parts, values.first().map(String::as_str)).await,
        Some("cb") => shop::on_catalog_buy(cx, values.first().map(String::as_str)).await,
        Some("cp") => shop::on_catalog_page(cx, parts).await,
        Some("iv") => shop::on_inventory_page(cx, parts).await,
        Some("pc") => pet::on_care(cx, parts).await,
        Some("pa") => pet::on_adopt(cx, values.first().map(String::as_str)).await,
        Some("ps") => pet::on_switch(cx, values.first().map(String::as_str)).await,
        Some("fr") => fish::on_reel(cx, parts).await,
        Some("sl") => stall::on_list(cx, parts, values.first().map(String::as_str)).await,
        Some("su") => stall::on_unlist(cx, values.first().map(String::as_str)).await,
        Some("sb") => stall::on_buy(cx, values.first().map(String::as_str)).await,
        Some("qc") => quests::on_claim(cx).await,
        Some("tt") => duel::on_move(cx, parts).await,
        Some("gf") => gift::on_gift(cx, parts, values.first().map(String::as_str)).await,
        Some("ch") => character::on_select(cx, parts, values.first().map(String::as_str)).await,
        Some("st") => store::on_buy(cx, values.first().map(String::as_str)).await,
        Some("cz") => customize::on_component(cx, parts, values).await,
        Some("ti") => customize::on_title(cx, parts, values).await,
        Some("rr") => role_panels::on_toggle(cx, parts).await,
        _ => cx.reply_ephemeral("This button has expired.").await,
    }
}
