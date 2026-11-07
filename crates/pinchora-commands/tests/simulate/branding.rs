use super::*;

#[tokio::test]
async fn branding_applies_profile_color_to_english_and_turkish_embeds() {
    let Some(mut sim) = Sim::new("branding_applies_profile_color").await else {
        return;
    };
    let manager = unique();
    sim.permissions = MANAGE_GUILD;
    sim.own_server(manager);
    let applied = sim
        .legacy_command(
            manager,
            "setup",
            json!([group(
                "branding",
                sub(
                    "apply",
                    vec![
                        text_option("nickname", "Coral Helper"),
                        text_option("bio", "Welcome aboard"),
                        text_option("colour", "#31B8D4")
                    ]
                )
            )]),
        )
        .await;
    assert!(applied.ephemeral && applied.content().contains("applied"), "{applied}");
    assert!(
        sim.mock
            .mod_calls
            .lock()
            .unwrap()
            .iter()
            .any(|c| c.contains("\"nick\":\"Coral Helper\""))
    );
    assert!(
        sim.mock
            .mod_calls
            .lock()
            .unwrap()
            .iter()
            .any(|c| c.contains("\"bio\":\"Welcome aboard\""))
    );
    let english = sim.command(manager, "ping", json!([])).await;
    assert_eq!(english.embeds[0].color, Some(0x31b8d4));
    sim.locale = "tr";
    let turkish = sim.command(manager, "ping", json!([])).await;
    assert_eq!(turkish.embeds[0].color, Some(0x31b8d4));
    poll_cmd(&mut sim, manager, "Which reef?", "A | B", None).await;
    let posts = sim.posts();
    assert_eq!(
        only(&posts, sim.channel, Method::POST).embed()["color"],
        json!(0x31b8d4)
    );
    let saved = sim.app.db.guild_branding(sim.guild).await.unwrap();
    assert_eq!(saved.embed_color, Some(0x31b8d4));
    assert_eq!(saved.bio_supported, Some(true));
    assert!(
        sim.app
            .db
            .audit_page(sim.guild, None, 10)
            .await
            .unwrap()
            .iter()
            .any(|row| row.action == "branding.apply")
    );
    let cooldown = sim
        .legacy_command(manager, "setup", json!([group("branding", sub("apply", vec![]))]))
        .await;
    assert!(cooldown.content().contains("10"));
}
