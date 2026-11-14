use super::*;

// /Pet care

#[tokio::test]
async fn a_care_click_that_lost_the_race_gives_its_cooldown_back() {
    let Some(mut sim) = Sim::new("a_care_click_that_lost_the_race_gives_its_cooldown_back").await else {
        return;
    };
    let Some(mut other) = Sim::new_pod(
        "a_care_click_that_lost_the_race_gives_its_cooldown_back_2",
        "a_care_click_that_lost_the_race_gives_its_cooldown_back",
    )
    .await
    else {
        return;
    };
    let user = unique();
    let db = sim.app.db.clone();
    let pet = db.adopt(user, "crumbclaw_crab", "Pip").await.unwrap();
    let card = sim.command(user, "pet", pet_command("view")).await;
    let mut refused = false;
    // Two clicks of different kinds read the same pet; one write lands and the other is told the pet
    // changed. Which pair of reads and writes interleaves like that is up to the scheduler, so ask a few times.
    for round in 0..15 {
        set_pet(&db, user, pet.id, fresh_needs(40), 2, 10, sim.app.now())
            .await
            .unwrap();
        for action in ["play", "clean"] {
            sim.app
                .cache
                .release_cooldown(&format!(
                    "pet:{}:{}",
                    pet.id,
                    if action == "play" { "Play" } else { "Clean" }
                ))
                .await
                .unwrap();
        }
        let (play_id, clean_id) = (format!("pc:play:{}", pet.id), format!("pc:clean:{}", pet.id));
        let (play, clean) = tokio::join!(
            sim.component(user, &play_id, &[], &card),
            other.component(user, &clean_id, &[], &card),
        );
        let lost = [("Play", &play), ("Clean", &clean)]
            .into_iter()
            .find(|(_, reply)| reply.content().contains("changed while you clicked"));
        if let Some((action, _)) = lost {
            refused = true;
            // Nothing was cared for, so the cooldown it started is not kept: the next try is not held back.
            let key = format!("pet:{}:{action}", pet.id);
            assert!(
                sim.app
                    .cache
                    .try_cooldown(&key, std::time::Duration::from_secs(5))
                    .await
                    .unwrap()
                    .is_none(),
                "round {round}: the refused {action} click gave its cooldown back"
            );
            sim.app.cache.release_cooldown(&key).await.unwrap();
            break;
        }
    }
    assert!(refused, "no round had a click lose the race");
}

#[tokio::test]
async fn a_pod_with_old_settings_cannot_write_who_is_in_voice_after_it_was_switched_off() {
    let Some(mut sim) =
        Sim::new("a_pod_with_old_settings_cannot_write_who_is_in_voice_after_it_was_switched_off").await
    else {
        return;
    };
    let Some(mut other) = Sim::new_pod(
        "a_pod_with_old_settings_cannot_write_who_is_in_voice_after_it_was_switched_off_2",
        "a_pod_with_old_settings_cannot_write_who_is_in_voice_after_it_was_switched_off",
    )
    .await
    else {
        return;
    };
    let _turn = VOICE_TICKS.lock().await;
    sim.app.shift_clock(chrono::Duration::days((unique() % 36_500) as i64));
    other.guild = sim.guild;
    let (admin, lounge, a) = (unique(), unique(), unique());
    sim.permissions = MANAGE_GUILD;
    let voice = |enabled: bool| sub("voice", vec![bool_option("enabled", enabled), int_option("amount", 20)]);
    sim.xp(admin, voice(true)).await;
    // The other pod has read the settings with voice XP on.
    assert!(other.app.guild_settings(sim.guild).await.unwrap().voice_xp_enabled);

    sim.xp(admin, voice(false)).await;
    assert!(
        other.app.guild_settings(sim.guild).await.unwrap().voice_xp_enabled,
        "its view is old: it has not heard of the switch"
    );
    other.voice_in(a, Some(lounge), &[], false, false, false).await;
    assert!(
        sim.voice_members().await.is_empty(),
        "an event handled by a pod with the old view writes nothing"
    );
    let event = other.guild_create(&[(a, lounge, false, false)], &[(a, false, &[])], None);
    pinchora_commands::on_guild_create(other.app.clone(), event)
        .await
        .expect("rebuild");
    assert!(sim.voice_members().await.is_empty(), "nor does its rebuild");

    // Switched on again, the same event is written down.
    sim.xp(admin, voice(true)).await;
    other.voice_in(a, Some(lounge), &[], false, false, false).await;
    assert_eq!(sim.voice_members().await, vec![a]);
}

#[tokio::test]
async fn a_settings_change_reaches_the_other_pods_through_redis() {
    let Some(mut sim) = Sim::new("a_settings_change_reaches_the_other_pods_through_redis").await else {
        return;
    };
    let Some(mut other) = Sim::new_pod(
        "a_settings_change_reaches_the_other_pods_through_redis_2",
        "a_settings_change_reaches_the_other_pods_through_redis",
    )
    .await
    else {
        return;
    };
    other.guild = sim.guild;
    pinchora_commands::listen_for_invalidations(&other.app).await;
    let admin = unique();
    sim.permissions = MANAGE_GUILD;
    // The other pod has the settings and the voice view cached, with leveling on.
    assert!(other.app.guild_settings(sim.guild).await.unwrap().leveling_enabled);
    sim.command(
        admin,
        "config",
        json!([sub("leveling", vec![bool_option("enabled", false)])]),
    )
    .await;
    let mut heard = false;
    for _ in 0..100 {
        if !other.app.guild_settings(sim.guild).await.unwrap().leveling_enabled {
            heard = true;
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(30)).await;
    }
    assert!(heard, "the other pod forgot its copy as soon as it was told");

    // Multipliers too.
    let everyone = role_json(sim.guild, "@everyone", 0, false);
    assert!(other.app.xp_rules(sim.guild).await.unwrap().roles.is_empty());
    sim.xp(
        admin,
        sub(
            "multiplier",
            vec![number_option("value", 2.0), role_option("role", &everyone)],
        ),
    )
    .await;
    let mut heard = false;
    for _ in 0..100 {
        if !other.app.xp_rules(sim.guild).await.unwrap().roles.is_empty() {
            heard = true;
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(30)).await;
    }
    assert!(heard, "the other pod's XP rules were forgotten too");
}
