use super::*;

#[tokio::test]
async fn tracker_refusals_and_admin_route_speak_both_languages() {
    let Some(mut sim) = Sim::new("tracker_refusals_and_admin_route_speak_both_languages").await else {
        return;
    };
    let user = unique();
    let profile = sim
        .command(
            user,
            "game",
            json!([group(
                "lol",
                sub(
                    "profile",
                    vec![text_option("name", "Player#EUW"), text_option("region", "euw1")]
                )
            )]),
        )
        .await;
    assert!(
        profile.ephemeral && profile.content().contains("Riot API key"),
        "{profile}"
    );
    let private = sim
        .command(
            user,
            "game",
            json!([group("mc", sub("status", vec![text_option("host", "127.0.0.1:25565")]))]),
        )
        .await;
    assert!(
        private.ephemeral && private.content().contains("not public"),
        "{private}"
    );
    let valorant = sim
        .command(user, "game", json!([group("valorant", sub("profile", vec![]))]))
        .await;
    assert!(
        valorant.ephemeral && valorant.content().contains("Riot production"),
        "{valorant}"
    );
    let round = sim
        .command(user, "game", json!([group("mc", sub("trivia", vec![]))]))
        .await;
    assert!(!round.ephemeral && !round.description().is_empty(), "{round}");
    let denied = sim.command(user, "trackers", json!([sub("patch-list", vec![])])).await;
    assert!(
        denied.ephemeral && denied.content().contains("Manage Server"),
        "{denied}"
    );
    sim.app
        .set_user_lang(user, Some(pinchora_core::i18n::Lang::Tr))
        .await
        .unwrap();
    let turkish = sim
        .command(
            user,
            "game",
            json!([group(
                "lol",
                sub(
                    "profile",
                    vec![text_option("name", "Player#EUW"), text_option("region", "euw1")]
                )
            )]),
        )
        .await;
    assert!(
        turkish.ephemeral && turkish.content().contains("Riot API anahtarı"),
        "{turkish}"
    );
}

#[tokio::test]
async fn league_card_tabs_use_short_lived_cached_data() {
    let Some(mut sim) = Sim::new("league_card_tabs_use_short_lived_cached_data").await else {
        return;
    };
    let user = unique();
    let base = sim
        .command(
            user,
            "game",
            json!([group(
                "lol",
                sub(
                    "profile",
                    vec![text_option("name", "Player#EUW"), text_option("region", "euw1")]
                )
            )]),
        )
        .await;
    let token = "0123456789abcdef0123456789abcdef";
    sim.app
        .db
        .tracker_cache_put(
            &format!("riot:card:{token}"),
            &json!({
                "title":"Player#EUW",
                "rank":[{"label":"Rank","value":"GOLD II"}],
                "mastery":[{"label":"Mastery 1","value":"Ahri · 12345"}],
                "image":null
            }),
        )
        .await
        .unwrap();
    let tab = sim.component(user, &format!("gt:{token}:mastery"), &[], &base).await;
    assert!(!tab.files.is_empty(), "{tab}");
    assert!(tab.has_button("Rank") && tab.has_button("Champions"), "{tab}");
    let forged = sim.component(user, "gt:short:rank", &[], &tab).await;
    assert!(forged.ephemeral && forged.content().contains("expired"), "{forged}");
}
