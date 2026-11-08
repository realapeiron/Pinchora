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

#[tokio::test]
async fn minecraft_cached_status_is_shared_and_failed_probes_are_not_repeated() {
    let Some(mut sim) = Sim::new("minecraft_cached_status_is_shared_and_failed_probes_are_not_repeated").await else {
        return;
    };
    let host = format!("f17-{}.example", unique());
    let key = format!("minecraft:status:{host}:25565");
    sim.app
        .db
        .tracker_cache_put(
            &key,
            &json!({
                "version":"1.21", "online":4,"max":20,"motd":"<script> @everyone שלום", "favicon":null
            }),
        )
        .await
        .unwrap();
    let fetched: String = sqlx::query_scalar("SELECT fetched_at::text FROM tracker_cache WHERE key=$1")
        .bind(&key)
        .fetch_one(sim.app.db.pool())
        .await
        .unwrap();
    for _ in 0..2 {
        let user = unique();
        let card = sim
            .command(
                user,
                "game",
                json!([group(
                    "mc",
                    sub(
                        "status",
                        vec![text_option("host", &format!("{}.:25565", host.to_uppercase()))]
                    )
                )]),
            )
            .await;
        assert_eq!(card.files.len(), 1, "cached server rendered: {card}");
    }
    let unchanged: String = sqlx::query_scalar("SELECT fetched_at::text FROM tracker_cache WHERE key=$1")
        .bind(&key)
        .fetch_one(sim.app.db.pool())
        .await
        .unwrap();
    assert_eq!(fetched, unchanged, "a shared host must not be probed for every viewer");
    sim.app
        .db
        .tracker_cache_put(&key, &serde_json::Value::Null)
        .await
        .unwrap();
    let failed_at: String = sqlx::query_scalar("SELECT fetched_at::text FROM tracker_cache WHERE key=$1")
        .bind(&key)
        .fetch_one(sim.app.db.pool())
        .await
        .unwrap();
    let user = unique();
    let failed = sim
        .command(
            user,
            "game",
            json!([group("mc", sub("status", vec![text_option("host", &host)]))]),
        )
        .await;
    assert!(failed.ephemeral, "{failed}");
    let unchanged: String = sqlx::query_scalar("SELECT fetched_at::text FROM tracker_cache WHERE key=$1")
        .bind(&key)
        .fetch_one(sim.app.db.pool())
        .await
        .unwrap();
    assert_eq!(failed_at, unchanged, "negative cache must suppress repeated probes too");
}
