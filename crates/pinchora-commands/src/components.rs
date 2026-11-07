use pinchora_core::t;
use twilight_model::application::interaction::modal::ModalInteractionComponent;

use crate::{
    Cx,
    commands::{
        board, character, collection, connect4, contest, customcommand, customize, decorate, derby, duel, fish,
        friends, gift, giveaway, guestbook, hangman, heist, help, house, interact, leaderboard, partner, party_games,
        pet, plaza, poll, privacy, quests, remind, role_panels, roles_pack, rps, share, shells, shop, stall, store,
        suggest, surge, tempvoice, trivia, wordle,
    },
};

/// Routes button clicks by the `prefix:` of their custom id. Ids carry all their
/// state, so any pod can serve any click.
pub(crate) async fn dispatch(cx: &Cx, custom_id: &str, values: &[String]) -> anyhow::Result<()> {
    if crate::gate::refuses_component(cx, custom_id).await? {
        return Ok(());
    }
    let mut parts = custom_id.split(':');
    match parts.next() {
        Some("ah") => crate::commands::achievements::on_component(cx, parts).await,
        Some("vf") => crate::verification::on_component(cx, "vf", parts).await,
        Some("cq") => crate::verification::on_component(cx, "cq", parts).await,
        Some("rd") => crate::verification::on_component(cx, "rd", parts).await,
        Some("an") => crate::commands::anti_nuke::on_component(cx, "an", parts).await,
        Some("vp") => crate::commands::anti_nuke::on_component(cx, "vp", parts).await,
        Some("bd") => crate::commands::birthdays::on_component(cx, parts).await,
        Some("am") => crate::commands::automod::on_component(cx, "am", parts, values).await,
        Some("ax") => crate::commands::automod::on_component(cx, "ax", parts, values).await,
        Some("lb") => leaderboard::on_button(cx, parts).await,
        Some("xi") => crate::commands::xp_admin::on_component(cx, parts).await,
        Some("nf") => crate::commands::info::on_component(cx, parts).await,
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
        Some("pn") => pet::on_nursery(cx, parts, values.first().map(String::as_str)).await,
        Some("py") => pet::on_trip(cx, parts, values.first().map(String::as_str)).await,
        Some("fr") => fish::on_reel(cx, parts).await,
        Some("fx") => fish::on_gear(cx, parts, values.first().map(String::as_str)).await,
        Some("fz") => fish::on_bucket(cx, parts).await,
        Some("sl") => stall::on_list(cx, parts, values.first().map(String::as_str)).await,
        Some("su") => stall::on_unlist(cx, values.first().map(String::as_str)).await,
        Some("sb") => stall::on_buy(cx, values.first().map(String::as_str)).await,
        Some("qc") => quests::on_claim(cx, parts).await,
        Some("tt") => duel::on_move(cx, parts).await,
        Some("gf") => gift::on_gift(cx, parts, values.first().map(String::as_str)).await,
        Some("ch") => character::on_old(cx).await,
        Some("ca") => character::on_component(cx, parts, values).await,
        Some("st") => store::on_buy(cx, values.first().map(String::as_str)).await,
        Some("cz") => customize::on_component(cx, parts, values).await,
        Some("ti") => customize::on_title(cx, parts, values).await,
        Some("cs") => customize::on_style(cx, parts, values).await,
        Some("sw") => customize::on_showcase(cx, parts, values).await,
        Some("rr") => role_panels::on_toggle(cx, parts).await,
        Some("rb") => role_panels::on_toggle(cx, parts).await,
        Some("rs") => role_panels::on_select(cx, parts, values).await,
        Some("cr") => roles_pack::on_color_component(cx, parts, values).await,
        Some("gb") => guestbook::on_component(cx, parts, values.first().map(String::as_str)).await,
        Some("rc") => contest::on_component(cx, parts).await,
        Some("c4") => connect4::on_button(cx, parts).await,
        Some("bs") => board::fleet::on_component(cx, parts, values).await,
        Some("ck") => board::checkers::on_component(cx, parts, values).await,
        Some("cc") => board::chess::on_component(cx, parts, values).await,
        Some("pz") => board::picross::on_component(cx, parts, values).await,
        Some("tz") => board::tide2048::on_button(cx, parts).await,
        Some("rp") => rps::on_button(cx, parts).await,
        Some("hm") => hangman::on_pick(cx, parts, values.first().map(String::as_str)).await,
        Some("wo") => wordle::on_button(cx, parts).await,
        Some("tv") => trivia::on_answer(cx, parts).await,
        Some("pt") => partner::on_component(cx, parts, values.first().map(String::as_str)).await,
        Some("fd") => friends::on_component(cx, parts, values.first().map(String::as_str)).await,
        Some("hg") => interact::on_component(cx, parts).await,
        Some("hp") => help::on_component(cx, parts, values.first().map(String::as_str)).await,
        Some("rm") => remind::on_component(cx, parts, values.first().map(String::as_str)).await,
        Some("pr") => privacy::on_component(cx, parts).await,
        Some("gw") => giveaway::on_component(cx, parts).await,
        Some("sg") => suggest::on_vote(cx, parts).await,
        Some("tg") => customcommand::on_page(cx, parts).await,
        Some("pl") => poll::on_component(cx, parts).await,
        Some("cd") => collection::drops::on_grab(cx, parts).await,
        Some("mb") => collection::drops::on_open(cx, parts).await,
        Some("bc") => collection::dex::on_component(cx, parts, values.first().map(String::as_str)).await,
        Some("kp") => collection::crabpot::on_component(cx, parts).await,
        Some("dy") => derby::on_button(cx, parts).await,
        Some("sh") => share::on_button(cx, parts).await,
        Some("sz") => shells::on_button(cx, parts).await,
        Some("ss") => surge::on_button(cx, parts).await,
        Some("hs") => heist::on_button(cx, parts).await,
        Some("sn") => crate::commands::feeds::on_component(cx, parts).await,
        Some(prefix @ ("hl" | "ac" | "cn" | "wx" | "wy" | "td")) => party_games::on_component(cx, prefix, parts).await,
        Some("in") => crate::commands::invites::on_component(cx, parts).await,
        Some("tl") => crate::commands::invites::on_temp_component(cx, parts).await,
        Some("sc") => crate::commands::stats::on_component(cx, parts).await,
        Some("sr") => crate::commands::starboard::on_component(cx, parts).await,
        Some("tk") => crate::tickets::on_open(cx, parts, values).await,
        Some("tp") => crate::tickets::on_control(cx, parts).await,
        Some("tr") => crate::tickets::on_rating(cx, parts).await,
        Some("vc") => tempvoice::on_button(cx, parts).await,
        Some("vo") => tempvoice::on_select(cx, parts, values).await,
        _ => cx.reply_ephemeral(t!(cx.lang(), "component.button-expired")).await,
    }
}

/// Routes a submitted modal by the `prefix:` of its custom id; `fields` are its text inputs
/// as (custom id, value).
pub(crate) async fn dispatch_modal(cx: &Cx, custom_id: &str, fields: &[(String, String)]) -> anyhow::Result<()> {
    if crate::gate::refuses_component(cx, custom_id).await? {
        return Ok(());
    }
    let mut parts = custom_id.split(':');
    match parts.next() {
        Some("cq") => crate::verification::on_modal(cx, parts, fields).await,
        Some("gb") => guestbook::on_modal(cx, parts, fields).await,
        Some("cs") => customize::on_style_modal(cx, parts, fields).await,
        Some("ca") => character::on_modal(cx, parts, fields).await,
        Some("cm") => customcommand::on_modal(cx, parts, fields).await,
        Some("pz") if parts.next() == Some("m") => board::picross::on_modal(cx, parts, fields).await,
        Some(prefix @ ("hl" | "ac" | "cn" | "wx")) => party_games::on_modal(cx, prefix, parts, fields).await,
        Some("vc") => tempvoice::on_modal(cx, parts, fields).await,
        Some("wo") if parts.next() == Some("m") => {
            let text = fields
                .iter()
                .find(|(id, _)| id == wordle::GUESS_FIELD)
                .map_or("", |(_, value)| value.as_str());
            wordle::on_guess(cx, parts, text).await
        }
        Some("tf") => crate::tickets::on_form(cx, parts, fields).await,
        _ => cx.reply_ephemeral(t!(cx.lang(), "component.form-expired")).await,
    }
}

/// Every text input of a submitted modal, wherever its label or row put it.
pub(crate) fn modal_fields(components: &[ModalInteractionComponent]) -> Vec<(String, String)> {
    let mut fields = Vec::new();
    for component in components {
        match component {
            ModalInteractionComponent::ActionRow(row) => fields.extend(modal_fields(&row.components)),
            ModalInteractionComponent::Label(label) => {
                fields.extend(modal_fields(std::slice::from_ref(&label.component)))
            }
            ModalInteractionComponent::TextInput(input) => fields.push((input.custom_id.clone(), input.value.clone())),
            _ => {}
        }
    }
    fields
}
