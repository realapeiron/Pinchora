use crate::*;

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
    pinchora_commands::admin::branding::apply(&sim.app, sim.guild, &row)
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

#[tokio::test]
async fn branding_covers_wave3_forms_moderation_logs_and_roblox_embeds() {
    use pinchora_core::admin::logging::LogEvent;
    let Some(mut sim) = Sim::new("branding_wave3_embeds").await else {
        return;
    };
    let manager = unique();
    sim.permissions = MANAGE_GUILD;
    sim.own_server(manager);
    let row = pinchora_db::GuildBranding {
        embed_color: Some(0x31b8d4),
        ..Default::default()
    };
    sim.app.db.save_guild_branding(sim.guild, &row).await.unwrap();
    pinchora_commands::admin::branding::apply(&sim.app, sim.guild, &row)
        .await
        .unwrap();
    sim.app
        .db
        .update_log_config(sim.guild, |config| {
            config.default_channel = Some(sim.channel);
            config.toggle(LogEvent::Report, true);
        })
        .await
        .unwrap();
    sim.app
        .db
        .save_moderation_policy_config(
            sim.guild,
            &pinchora_core::moderation::policy::Config {
                report_channel: Some(sim.channel.to_string()),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    let form = sim
        .app
        .db
        .save_form(&pinchora_db::FormRow {
            id: 0,
            guild_id: sim.guild as i64,
            name: "Branding coverage".into(),
            questions: json!(["Why?"]),
            submit_channel_id: Some(sim.channel as i64),
            review_roles: vec![],
            accept_role_id: None,
            dm_result: false,
            open: true,
            message_id: None,
        })
        .await
        .unwrap()
        .unwrap();
    sim.posts();
    for locale in ["en-US", "tr"] {
        sim.locale = locale;
        let user = unique();
        let binds = sim
            .command(manager, "robloxadmin", json!([sub("bind-list", vec![])]))
            .await;
        assert_eq!(binds.embeds.len(), 1, "{binds}");
        assert_eq!(binds.embeds[0].color, Some(0x31b8d4));
        let modal = sim
            .command(
                user,
                "form",
                json!([sub("open", vec![json!({"type":4,"name":"id","value":form})])]),
            )
            .await;
        assert!(modal.modal.is_some(), "{modal}");
        sim.submit_fields(user, &modal, &[("a0", "A helpful member")]).await;
        let posts = sim.posts();
        assert_eq!(posts.len(), 1, "form review post");
        assert_eq!(posts[0].embed()["color"], json!(0x31b8d4));
        let target = unique();
        sim.command_resolved(
            user,
            "modtools",
            json!([sub(
                "report",
                vec![user_option("user", target), text_option("reason", "Please investigate")]
            )]),
            json!({"users":{target.to_string():user_json(target)}}),
        )
        .await;
        let posts = sim.posts();
        let embeds: Vec<_> = posts
            .iter()
            .flat_map(|post| post.body["embeds"].as_array().into_iter().flatten())
            .collect();
        assert_eq!(embeds.len(), 2, "case log and parity event log");
        for embed in embeds {
            assert_eq!(embed["color"], json!(0x31b8d4));
        }
    }
}

#[tokio::test]
async fn branding_covers_long_moderation_batch_preview_and_result_embeds() {
    let Some(mut sim) = Sim::new("branding_moderation_batch_embeds").await else {
        return;
    };
    let manager = unique();
    sim.permissions = MANAGE_GUILD | twilight_model::guild::Permissions::BAN_MEMBERS.bits();
    sim.own_server(manager);
    assert!(
        sim.app
            .db
            .claim_branding_apply(sim.guild, Some(0x31b8d4))
            .await
            .unwrap()
    );
    sim.app.db.set_mod_settings(sim.guild, None, Some(false)).await.unwrap();
    sim.mock.bans_refused.store(true, Ordering::SeqCst);
    let ids = (9_000_000_000_000_000_000_u64..9_000_000_000_000_000_100)
        .map(|id| id.to_string())
        .collect::<Vec<_>>()
        .join(" ");
    for locale in ["en-US", "tr"] {
        sim.locale = locale;
        let preview = sim
            .command(
                manager,
                "modtools",
                json!([sub(
                    "mass",
                    vec![text_option("action", "ban"), text_option("ids", &ids)]
                )]),
            )
            .await;
        assert_eq!(preview.embeds.len(), 1, "long preview: {preview}");
        assert_eq!(preview.embeds[0].color, Some(0x31b8d4));
        let description = preview.embeds[0].description.as_ref().unwrap();
        let token = description.split("confirm:").last().unwrap().trim();
        let result = sim
            .command(
                manager,
                "modtools",
                json!([sub(
                    "mass",
                    vec![text_option("action", "ban"), text_option("confirm", token)]
                )]),
            )
            .await;
        assert_eq!(result.embeds.len(), 1, "long result: {result}");
        assert_eq!(result.embeds[0].color, Some(0x31b8d4));
        assert!(sim.mock.bans.lock().unwrap().is_empty());
        sim.app
            .cache
            .release_cooldown(&format!("modparity:batch:{}", sim.guild))
            .await
            .unwrap();
    }
}
