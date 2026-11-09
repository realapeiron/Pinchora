//! The web care route and the Discord button share a Redis cooldown and the same stored pet.
use crate::*;
use pinchora_core::infra::i18n::Lang;

#[tokio::test]
async fn web_care_cooldown_interleaves_with_discord_in_both_languages() {
    for (name, locale, rest) in [
        ("web_care_interleave_en", "en", "break"),
        ("web_care_interleave_tr", "tr", "dinlenmeli"),
    ] {
        let Some(mut sim) = Sim::new(name).await else {
            return;
        };
        sim.locale = locale;
        let user = unique();
        let pet = sim.app.db.adopt(user, "crumbclaw_crab", "Pip").await.unwrap();
        let card = sim.command(user, "pet", pet_command("view")).await;
        assert_png(&card, 860, 420);
        let key = format!("pet:{}:Play", pet.id);
        // The web endpoint takes this key before writing the same Db pet row.
        assert!(
            sim.app
                .cache
                .try_cooldown(&key, std::time::Duration::from_secs(20 * 60))
                .await
                .unwrap()
                .is_none()
        );
        let held = sim.component(user, &format!("pc:play:{}", pet.id), &[], &card).await;
        assert!(held.ephemeral && held.content().contains(rest), "{}", held.content());
        assert_eq!(sim.app.db.pets(user).await.unwrap()[0].xp, 0);
        sim.app.cache.release_cooldown(&key).await.unwrap();
        let played = sim.component(user, &format!("pc:play:{}", pet.id), &[], &card).await;
        assert_png(&played, 860, 420);
        assert_eq!(sim.app.db.pets(user).await.unwrap()[0].xp, 15);
    }
}

#[tokio::test]
async fn failed_discord_feed_can_retry_after_funding_in_both_languages() {
    for (name, locale) in [("depth_feed_retry_en", "en"), ("depth_feed_retry_tr", "tr")] {
        let Some(mut sim) = Sim::new(name).await else { return };
        sim.locale = locale;
        let user = unique();
        let pet = sim.app.db.adopt(user, "crumbclaw_crab", "Pip").await.unwrap();
        let card = sim.command(user, "pet", pet_command("view")).await;
        let refused = sim.component(user, &format!("pc:feed:{}", pet.id), &[], &card).await;
        assert!(refused.ephemeral);
        assert_eq!(sim.app.db.pets(user).await.unwrap()[0].xp, 0);
        sim.app.db.earn_shells(user, 100, "test").await.unwrap();
        let fed = sim.component(user, &format!("pc:feed:{}", pet.id), &[], &card).await;
        assert_png(&fed, 860, 420);
        assert_eq!(
            sim.app.db.user(user).await.unwrap().credits,
            100 - pinchora_game::pets::FOOD_PRICE
        );
    }
}

#[tokio::test]
async fn web_social_blocks_stop_discord_friend_requests_and_pending_accepts() {
    for (name, locale, label) in [
        ("depth_friend_block_en", "en", "Accept"),
        ("depth_friend_block_tr", "tr", "Kabul et"),
    ] {
        let Some(mut sim) = Sim::new(name).await else { return };
        sim.locale = locale;
        sim.app
            .set_guild_lang(sim.guild, Some(if locale == "tr" { Lang::Tr } else { Lang::En }))
            .await
            .unwrap();
        let (a, b) = (unique(), unique());
        let options = json!([sub("add", vec![user_option("user", b)])]);
        let request = sim
            .command_resolved(a, "friends", options.clone(), named(&[(b, "Bea")]))
            .await;
        // Web moderation calls this same transaction while the request is still open.
        sim.app.db.block_signer(b, a, "A").await.unwrap();
        let refused = sim.command_resolved(a, "friends", options, named(&[(b, "Bea")])).await;
        assert!(refused.ephemeral);
        let _ = sim.click(b, &request, label).await;
        assert!(!sim.app.db.are_friends(a, b).await.unwrap());
    }
}
