//! The web's profile assembler produces the actual /profile PNG, in both languages.
use super::*;

#[tokio::test]
async fn web_player_shared_profile_matches_discord_after_studio_save() {
    let Some(mut sim) = Sim::new("web_player_profile_parity").await else {
        return;
    };
    let id = unique();
    let row = sim.app.db.user(id).await.unwrap();
    let mut draft = pinchora_db::PlayerProfile::from_row(&row);
    let content = pinchora_game::Content::get();
    // Keep ordinary parallel fixtures earning XP from changing the live rank between renders.
    sqlx::query("UPDATE users SET credits=1000000, xp=1000000000 WHERE id=$1")
        .bind(id as i64)
        .execute(sim.app.db.pool())
        .await
        .unwrap();
    for badge in &content.badges {
        if let pinchora_game::BadgeKind::Shop { price } = badge.kind {
            sim.app.db.buy_badge(id, &badge.id, price).await.unwrap();
            draft.badges.push(badge.id.clone());
            if draft.badges.len() == 6 {
                break;
            }
        }
    }
    draft.badges.reverse();
    draft.badges_set = true;
    let background = &content.backgrounds[0];
    sim.app
        .db
        .buy_background(id, &background.id, background.price)
        .await
        .unwrap();
    draft.background = Some(background.id.clone());
    let pet = sim.app.db.adopt(id, &content.pets[0].id, "Pebble").await.unwrap();
    draft.showcase_pet = Some(pet.id.to_string());
    draft.panel_opacity = 0;
    let title = &content.titles[0];
    sim.app.db.grant_title(id, &title.id).await.unwrap();
    draft.title = Some(title.id.clone());

    draft.bio = "My little corner of the reef".into();
    draft.accent = Some("#06d6a0".into());
    draft.frame = "rope".into();
    sim.app.db.save_player_profile(id, &draft).await.unwrap();
    let user: twilight_model::user::User = serde_json::from_value(user_json(id)).unwrap();
    for lang in [pinchora_core::i18n::Lang::En, pinchora_core::i18n::Lang::Tr] {
        sim.app.set_user_lang(id, Some(lang)).await.unwrap();
        let reply = sim.command(id, "profile", json!([])).await;
        let expected = pinchora_commands::profile_card_for(
            &sim.app,
            &user,
            "Sim".into(),
            None,
            pinchora_commands::ProfileContext {
                guild: None,
                viewer: id,
                lang,
            },
        )
        .await
        .unwrap();
        assert_eq!(
            reply.files[0].1, expected,
            "the global web card and /profile must share bytes"
        );
    }
}
