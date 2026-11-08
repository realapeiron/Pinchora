//! The web care route and the Discord button share a Redis cooldown and the same stored pet.
use super::*;

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
