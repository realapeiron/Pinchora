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

#[tokio::test]
async fn branding_all_command_actions_refuse_members_without_manage_server() {
    let Some(mut sim) = Sim::new("branding_permissions").await else {
        return;
    };
    sim.permissions = 0;
    let member = unique();
    for action in ["show", "apply", "reset"] {
        let answer = sim
            .legacy_command(member, "setup", json!([group("branding", sub(action, vec![]))]))
            .await;
        assert!(answer.ephemeral);
        assert!(answer.content().contains("Manage Server"), "{answer}");
    }
    assert!(sim.app.db.guild_branding(sim.guild).await.unwrap().applied_at.is_none());
    assert!(sim.mock.mod_calls.lock().unwrap().is_empty());
}

#[tokio::test]
async fn branding_rejects_foreign_uploads_before_saving_a_draft() {
    let Some(mut sim) = Sim::new("branding_foreign_upload").await else {
        return;
    };
    let manager = unique();
    sim.permissions = MANAGE_GUILD;
    sim.own_server(manager);
    let answer = sim
        .legacy_command(
            manager,
            "setup",
            json!([group(
                "branding",
                sub("apply", vec![text_option("avatar", &format!("{:032x}", unique()))])
            )]),
        )
        .await;
    assert!(answer.content().contains("uploaded to this server"), "{answer}");
    let saved = sim.app.db.guild_branding(sim.guild).await.unwrap();
    assert!(saved.avatar_upload_id.is_none() && saved.applied_at.is_none());
}

#[tokio::test]
async fn branding_show_reports_accepted_profile_instead_of_an_unapplied_draft() {
    let Some(mut sim) = Sim::new("branding_show_active").await else {
        return;
    };
    let manager = unique();
    sim.permissions = MANAGE_GUILD;
    sim.own_server(manager);
    let row = pinchora_db::GuildBranding {
        nickname: Some("Accepted".into()),
        embed_color: Some(0x123456),
        ..Default::default()
    };
    sim.app.db.save_guild_branding(sim.guild, &row).await.unwrap();
    pinchora_commands::branding::apply(&sim.app, sim.guild, &row)
        .await
        .unwrap();
    let draft = pinchora_db::GuildBranding {
        nickname: Some("Unapplied".into()),
        embed_color: Some(0x654321),
        ..Default::default()
    };
    sim.app.db.save_guild_branding(sim.guild, &draft).await.unwrap();
    for locale in ["en-US", "tr"] {
        sim.locale = locale;
        let shown = sim
            .legacy_command(manager, "setup", json!([group("branding", sub("show", vec![]))]))
            .await;
        assert!(
            shown.content().contains("Accepted") && shown.content().contains("#123456"),
            "{shown}"
        );
        assert!(!shown.content().contains("Unapplied"));
    }
}
