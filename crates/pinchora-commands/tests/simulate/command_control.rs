use super::*;
use pinchora_core::command_control::{Config, Rule};
use std::collections::BTreeMap;

#[tokio::test]
async fn private_response_after_a_public_timeout_never_edits_private_content_into_public() {
    let Some(sim) = Sim::new("command_control_private_timeout").await else {
        return;
    };
    let interaction = sim.interaction(
        unique(),
        2,
        json!({"id":unique().to_string(),"name":"balance","type":1}),
        None,
    );
    let (tx, _rx) = oneshot::channel();
    let responder = Responder::http(sim.app.clone(), &interaction, tx);
    responder.defer(false).await.unwrap();
    sim.calls.lock().unwrap().clear();
    responder
        .respond(
            twilight_model::http::interaction::InteractionResponseData {
                content: Some("private test result".into()),
                flags: Some(twilight_model::channel::message::MessageFlags::EPHEMERAL),
                ..Default::default()
            },
            false,
        )
        .await
        .unwrap();
    let calls = sim.calls.lock().unwrap();
    assert!(calls.iter().any(|c| c.method == Method::POST && c.body["flags"] == 64));
    assert!(
        !calls
            .iter()
            .any(|c| c.method == Method::PATCH && c.body["content"] == "private test result")
    );
    assert!(
        calls
            .iter()
            .any(|c| c.method == Method::DELETE && c.path.ends_with("/messages/@original"))
    );
}

#[tokio::test]
async fn command_control_enforcement_surfaces_and_cache() {
    let Some(mut sim) = Sim::new("command_control_enforcement_surfaces_and_cache").await else {
        return;
    };
    let user = unique();
    sim.own_server(user);
    sim.channel_is(sim.channel, 0, None);
    let original = sim.command(user, "balance", json!([])).await;
    let cases = [
        (
            Rule {
                enabled: false,
                ..Rule::default()
            },
            "disabled",
        ),
        (
            Rule {
                allowed_roles: vec![unique().to_string()],
                ..Rule::default()
            },
            "allowed roles",
        ),
        (
            Rule {
                denied_roles: vec![sim.guild.to_string()],
                ..Rule::default()
            },
            "roles is denied",
        ),
        (
            Rule {
                denied_channels: vec![sim.channel.to_string()],
                ..Rule::default()
            },
            "channel or its category",
        ),
    ];
    for (rule, reason) in cases {
        let config = Config {
            rules: BTreeMap::from([("balance".into(), rule.clone()), ("leaderboard".into(), rule.clone())]),
            ..Config::default()
        };
        sim.app.db.set_command_control(sim.guild, &config).await.unwrap();
        sim.app.announce_guild_change(sim.guild).await;
        let slash = sim.command(user, "balance", json!([])).await;
        assert!(slash.content().contains(reason), "{}", slash.content());
        let button = sim.component(user, "lb:page:0", &[], &original).await;
        assert!(button.content().contains(reason), "{}", button.content());
        sim.app.cache.release_cooldown(&format!("text:{user}")).await.unwrap();
        let message:twilight_model::channel::Message=serde_json::from_value(json!({"id":unique().to_string(),"channel_id":sim.channel.to_string(),"guild_id":sim.guild.to_string(),"type":0,"author":user_json(user),"member":{"roles":[],"joined_at":"2024-01-01T00:00:00+00:00","deaf":false,"mute":false,"flags":0},"content":"p!balance","timestamp":"2024-01-01T00:00:00+00:00","edited_timestamp":null,"tts":false,"mention_everyone":false,"mention_roles":[],"mentions":[],"pinned":false,"attachments":[],"embeds":[],"flags":0})).unwrap();
        sim.posts();
        assert!(pinchora_commands::on_text_command(sim.app.clone(), &message).await);
        let posts = sim.posts();
        assert!(
            posts
                .iter()
                .any(|p| p.body["content"].as_str().is_some_and(|s| s.contains(reason))),
            "no text reply matched {reason}"
        );
    }
    let cached = sim.app.command_control(sim.guild).await.unwrap();
    sim.app
        .db
        .set_command_control(sim.guild, &Config::default())
        .await
        .unwrap();
    assert!(
        Arc::ptr_eq(&cached, &sim.app.command_control(sim.guild).await.unwrap()),
        "cache hit does not reload DB"
    );
    sim.app.announce_guild_change(sim.guild).await;
    assert!(sim.app.command_control(sim.guild).await.unwrap().rules.is_empty());

    let config = Config {
        rules: BTreeMap::from([(
            "balance".into(),
            Rule {
                ephemeral: true,
                cooldown_secs: 60,
                ..Rule::default()
            },
        )]),
        ..Config::default()
    };
    sim.app.db.set_command_control(sim.guild, &config).await.unwrap();
    sim.app.announce_guild_change(sim.guild).await;
    assert!(sim.command(user, "balance", json!([])).await.ephemeral);
    assert!(sim.command(user, "balance", json!([])).await.content().contains("Wait"));
    let mut config = config;
    config.rules.get_mut("balance").unwrap().enabled = false;
    sim.app.db.set_command_control(sim.guild, &config).await.unwrap();
    sim.app.announce_guild_change(sim.guild).await;
    sim.locale = "tr";
    assert_eq!(
        sim.command(user, "balance", json!([])).await.content(),
        pinchora_core::t!(pinchora_core::i18n::Lang::Tr, "command_control.disabled")
    );
}
#[tokio::test]
async fn command_control_and_backup_require_manage_server() {
    let Some(mut sim) = Sim::new("command_control_and_backup_require_manage_server").await else {
        return;
    };
    for (name, options) in [
        ("commandcontrol", json!([sub("show", vec![])])),
        (
            "commandcontrol",
            json!([sub(
                "set",
                vec![text_option("command", "balance"), bool_option("enabled", false)]
            )]),
        ),
        ("backup", json!([sub("export", vec![])])),
        ("backup", json!([sub("list", vec![])])),
        ("backup", json!([sub("restore", vec![text_option("id", "1")])])),
    ] {
        let reply = sim.command(unique(), name, options).await;
        assert!(
            reply.ephemeral && reply.content().contains("Manage Server"),
            "{}",
            reply.content()
        );
    }
    sim.permissions = 1 << 5;
    let reply = sim
        .command(
            unique(),
            "commandcontrol",
            json!([sub(
                "alias",
                vec![text_option("name", "w"), text_option("command", "play wordle")]
            )]),
        )
        .await;
    assert!(reply.content().contains("saved"));
    assert_eq!(
        sim.app.db.command_control(sim.guild).await.unwrap().expand("w"),
        "play wordle"
    );
}

#[tokio::test]
async fn command_control_autocomplete_and_modals_obey_policy_without_spending_cooldown() {
    let Some(mut sim) = Sim::new("command_control_autocomplete_modals").await else {
        return;
    };
    let user = unique();
    sim.permissions = 1 << 5;
    sim.channel_is(sim.channel, 0, None);
    sqlx::query(
        "INSERT INTO custom_commands(guild_id,name,responses,created_by) VALUES($1,'secret',ARRAY['hello'],$2)",
    )
    .bind(sim.guild as i64)
    .bind(user as i64)
    .execute(sim.app.db.pool())
    .await
    .unwrap();
    let original = sim.command(user, "commandcontrol", json!([sub("show", vec![])])).await;
    for disabled in [true, false] {
        let config = Config {
            rules: BTreeMap::from([
                (
                    "customcommand".into(),
                    Rule {
                        enabled: !disabled,
                        cooldown_secs: 60,
                        ..Rule::default()
                    },
                ),
                (
                    "play wordle".into(),
                    Rule {
                        enabled: false,
                        ..Rule::default()
                    },
                ),
            ]),
            ..Config::default()
        };
        sim.app.db.set_command_control(sim.guild, &config).await.unwrap();
        sim.app.announce_guild_change(sim.guild).await;
        for _ in 0..2 {
            let data = json!({"id":unique().to_string(),"name":"customcommand","type":1,
                "options":[{"name":"info","type":1,"options":[{"name":"name","type":3,"value":"sec","focused":true}]}]});
            let interaction = sim.interaction(user, 4, data, None);
            let (tx, mut rx) = oneshot::channel();
            let responder = Arc::new(Responder::http(sim.app.clone(), &interaction, tx));
            handle_interaction(sim.app.clone(), interaction, responder).await;
            let reply = rx.try_recv().unwrap();
            assert_eq!(
                reply.kind,
                InteractionResponseType::ApplicationCommandAutocompleteResult
            );
            assert_eq!(reply.data.unwrap().choices.unwrap().is_empty(), disabled);
        }
        let result = sim.command(user, "customcommand", json!([sub("list", vec![])])).await;
        assert!(!result.content().contains("Wait"), "autocomplete consumed the cooldown");
        let button = sim.component(user, "wo:g:123", &[], &original).await;
        assert!(button.content().contains("disabled"));
        let modal = sim
            .run(
                "forged wordle modal".into(),
                user,
                5,
                json!({"custom_id":"wo:m:123","components":[]}),
                None,
            )
            .await;
        assert!(modal.content().contains("disabled"));
    }
}

#[tokio::test]
async fn backup_rejects_numeric_dangerous_grants_and_automod_actions() {
    use pinchora_core::{
        config_backup::{Import, Names, Policy},
        i18n::Lang,
    };
    let Some(sim) = Sim::new("backup_dangerous_grants").await else {
        return;
    };
    let actor = unique();
    let danger = unique();
    let foreign = unique();
    sim.guild_role(danger, "Danger", 1);
    let manager = unique();
    sim.guild_role_with(manager, "Manager", 5, 1 << 5);
    sim.mock.member_roles.lock().unwrap().insert(actor, vec![manager]);
    sim.channel_is(sim.channel, 0, None);
    let before = sim.app.db.export_config(sim.guild, Names::default()).await.unwrap();
    let mut safe = before.clone();
    safe.sections
        .get_mut("level_roles")
        .unwrap()
        .push(json!({"level":2,"role_id":danger.to_string()}));
    let (_, preview) = pinchora_commands::backup_preview(
        &sim.app,
        sim.guild,
        &Import {
            backup: safe,
            policy: Policy::Replace,
        },
        Lang::En,
        actor,
    )
    .await
    .unwrap();
    assert!(preview.can_import, "{:?}", preview.errors);
    sim.mock
        .roles
        .lock()
        .unwrap()
        .iter_mut()
        .find(|r| r["id"].as_str().and_then(|id| id.parse::<u64>().ok()) == Some(danger))
        .unwrap()["permissions"] = json!("8");
    for role in [json!(danger), json!(danger.to_string())] {
        let mut backup = before.clone();
        backup
            .sections
            .get_mut("level_roles")
            .unwrap()
            .push(json!({"level":2,"role_id":role}));
        let request = Import {
            backup,
            policy: Policy::Replace,
        };
        let (_, preview) = pinchora_commands::backup_preview(&sim.app, sim.guild, &request, Lang::En, actor)
            .await
            .unwrap();
        assert!(
            !preview.can_import,
            "numeric and string grants must both run role safety"
        );
        assert!(
            preview.errors.iter().any(|e| e.contains("Administrator")),
            "{:?}",
            preview.errors
        );
    }
    // AutoMod uses `role`, including escalation actions, rather than the usual `role_id`.
    for (escalation, role) in [(false, danger), (true, danger), (false, foreign)] {
        let mut backup = before.clone();
        backup.sections.get_mut("automod_settings").unwrap().push(json!({
            "profile":"standard","dm_on_action":false,"alert_channel_id":null,
            "escalation":if escalation { json!([{"strikes":2,"window_secs":60,"action":{"kind":"add_role","secs":0,"role":role.to_string()}}]) } else { json!([]) }
        }));
        if !escalation {
            backup.sections.get_mut("automod_rules").unwrap().push(json!({
                "id":"1","name":"unsafe","kind":"spam","enabled":true,"log_only":false,"native":false,
                "params":{},"actions":[{"kind":"add_role","secs":0,"role":role.to_string()}],
                "exempt_roles":[],"exempt_channels":[],"only_channels":[]
            }));
        }
        let request = Import {
            backup,
            policy: Policy::Replace,
        };
        let (_, preview) = pinchora_commands::backup_preview(&sim.app, sim.guild, &request, Lang::En, actor)
            .await
            .unwrap();
        assert!(
            !preview.can_import,
            "AutoMod grants must be checked: {:?}",
            preview.errors
        );
        if role == danger {
            assert!(
                preview.errors.iter().any(|e| e.contains("Administrator")),
                "{:?}",
                preview.errors
            );
        }
    }
    assert!(sim.app.db.config_restore_points(sim.guild).await.unwrap().is_empty());
    assert_eq!(
        sim.app.db.export_config(sim.guild, Names::default()).await.unwrap(),
        before
    );
}

#[tokio::test]
async fn backup_budgets_are_shared_per_guild_and_independent_by_operation() {
    use pinchora_core::i18n::Lang;
    let Some(sim) = Sim::new("backup_rate_limit").await else {
        return;
    };
    for _ in 0..5 {
        pinchora_commands::backup_rate_limit(&sim.app, sim.guild, "apply", 5, Lang::En)
            .await
            .unwrap();
    }
    assert!(
        pinchora_commands::backup_rate_limit(&sim.app, sim.guild, "apply", 5, Lang::En)
            .await
            .is_err()
    );
    pinchora_commands::backup_rate_limit(&sim.app, sim.guild, "preview", 10, Lang::En)
        .await
        .unwrap();
    pinchora_commands::backup_rate_limit(&sim.app, sim.guild + 1, "apply", 5, Lang::En)
        .await
        .unwrap();
}

#[tokio::test]
async fn backup_editor_validation_rejects_unclean_values_limits_and_orphan_reactions() {
    use pinchora_core::{config_backup::Names, i18n::Lang};
    let Some(sim) = Sim::new("backup_editor_validation").await else {
        return;
    };
    let before = sim.app.db.export_config(sim.guild, Names::default()).await.unwrap();
    pinchora_commands::validate_backup(&before, Lang::En).unwrap();
    let cases = [
        ("custom_commands", vec![json!({"id":"1","name":" UPPERCASE ","description":"","responses":["ok"],"embed":null,"reply_mode":"public","actions":[],"allow_roles":[],"deny_roles":[],"allow_channels":[],"deny_channels":[],"cooldown_secs":0,"enabled":true,"allow_mentions":false})]),
        ("custom_responders", vec![json!({"id":"1","name":"  Greeting  ","trigger_kind":"member_join","trigger_value":"","match_kind":"contains","responses":["ok"],"embed":null,"actions":[],"destination":"channel","channel_id":"1","chance":100,"allow_channels":[],"deny_channels":[],"cooldown_secs":0,"enabled":true,"allow_mentions":false})]),
        ("voice_creators", (1..=11).map(|id| json!({"channel_id":id.to_string(),"category_id":"99","name_template":"{user}","default_limit":0,"default_locked":false,"max_per_user":1,"panel":false,"panel_channel_id":null,"enabled":true})).collect()),
        ("vote_channels", vec![json!({"channel_id":"1","up_emoji":"x".repeat(65),"down_emoji":"y","only_media":false,"create_thread":false,"enabled":true})]),
        ("panel_reactions", vec![json!({"panel_id":"1","emoji":"u:👍","role_id":"2"})]),
        ("counter_channels", vec![json!({"id":"1","channel_id":"1","kind":"role_members","role_id":null,"template":"Members: {count}"})]),
    ];
    let mut ticket = before.clone();
    ticket.sections.get_mut("ticket_panels").unwrap().push(json!({"id":"1","name":"Help","channel_id":"1","title":"Support","description":"","color":0,"style":"buttons","buttons":[{"label":"Help"}],"form_id":null,"support_roles":[],"welcome_text":"","open_category_id":null,"log_channel_id":null,"enabled":true}));
    pinchora_commands::validate_backup(&ticket, Lang::En).unwrap();
    let mut injected = ticket.clone();
    injected.sections.get_mut("ticket_panels").unwrap()[0]["buttons"][0]["member_id"] = json!("1234");
    assert!(pinchora_commands::validate_backup(&injected, Lang::En).is_err());
    let mut form = before.clone();
    form.sections
        .get_mut("ticket_forms")
        .unwrap()
        .push(json!({"id":"1","name":"Support","questions":[{"label":"Issue"}]}));
    pinchora_commands::validate_backup(&form, Lang::En).unwrap();
    form.sections.get_mut("ticket_forms").unwrap()[0]["questions"][0]["member_id"] = json!("1234");
    assert!(pinchora_commands::validate_backup(&form, Lang::En).is_err());
    ticket.sections.get_mut("ticket_panels").unwrap()[0]["color"] = json!(-1);
    assert!(pinchora_commands::validate_backup(&ticket, Lang::En).is_err());
    let mut accent = before.clone();
    accent.sections.get_mut("guilds").unwrap()[0]["welcome_accent"] = json!(0x1000000);
    assert!(pinchora_commands::validate_backup(&accent, Lang::En).is_err());
    for (section, rows) in cases {
        let mut backup = before.clone();
        backup.sections.insert(section.into(), rows);
        assert!(
            pinchora_commands::validate_backup(&backup, Lang::En).is_err(),
            "{section} accepted invalid editor input"
        );
    }
}

#[tokio::test]
async fn backup_restore_checks_permissions_and_writes_discord_audit() {
    use pinchora_core::config_backup::{Names, Policy};
    let Some(mut sim) = Sim::new("backup_restore_audit").await else {
        return;
    };
    let user = unique();
    let backup = sim.app.db.export_config(sim.guild, Names::default()).await.unwrap();
    let point = sim
        .app
        .db
        .import_config(sim.guild, &backup, Policy::Merge, Names::default())
        .await
        .unwrap();
    sim.permissions = 1 << 5;
    let original = sim.command(user, "backup", json!([sub("list", vec![])])).await;
    sim.permissions = 0;
    let denied = sim
        .component(user, &format!("cf:restore:{point}"), &[], &original)
        .await;
    assert!(denied.content().contains("Manage Server"));
    assert_eq!(sim.app.db.config_restore_points(sim.guild).await.unwrap().len(), 1);
    sim.permissions = 1 << 5;
    let restored = sim
        .component(user, &format!("cf:restore:{point}"), &[], &original)
        .await;
    assert!(restored.content().contains("imported"), "{}", restored.content());
    let count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM dashboard_audit WHERE guild_id=$1 AND action='backup.restore' AND source='discord'",
    )
    .bind(sim.guild as i64)
    .fetch_one(sim.app.db.pool())
    .await
    .unwrap();
    assert_eq!(count, 1);
}

#[tokio::test]
async fn permissive_overrides_and_aliases_cannot_open_manage_server_paths() {
    let Some(mut sim) = Sim::new("command_control_no_privilege_escalation").await else {
        return;
    };
    let user = unique();
    sim.channel_is(sim.channel, 0, None);
    sim.mock.everyone_role.store(true, Ordering::SeqCst);
    let config = Config {
        rules: BTreeMap::from([(
            "customcommand".into(),
            Rule {
                allowed_roles: vec![sim.guild.to_string()],
                ..Rule::default()
            },
        )]),
        aliases: BTreeMap::from([("ccmanage".into(), "customcommand list".into())]),
    };
    sim.app.db.set_command_control(sim.guild, &config).await.unwrap();
    sim.app.announce_guild_change(sim.guild).await;
    assert_eq!(
        sim.app.command_control(sim.guild).await.unwrap().expand("ccmanage"),
        "customcommand list"
    );
    assert_eq!(
        sim.app.command_control(sim.guild + 1).await.unwrap().expand("ccmanage"),
        "ccmanage"
    );
    let denied = sim.command(user, "customcommand", json!([sub("list", vec![])])).await;
    assert!(denied.content().contains("Manage Server"));
    let button = sim.component(user, "tg:l:0", &[], &denied).await;
    assert!(button.content().contains("Manage Server"));
    let modal = sim
        .run(
            "forged manager modal".into(),
            user,
            5,
            json!({"custom_id":"cm:c:rogue:0","components":[]}),
            None,
        )
        .await;
    assert!(modal.content().contains("Manage Server"));
    let message: twilight_model::channel::Message = serde_json::from_value(json!({
        "id":unique().to_string(),"channel_id":sim.channel.to_string(),"guild_id":sim.guild.to_string(),"type":0,
        "author":user_json(user),"member":{"roles":[],"joined_at":"2024-01-01T00:00:00+00:00","deaf":false,"mute":false,"flags":0},
        "content":"p!ccmanage","timestamp":"2024-01-01T00:00:00+00:00","edited_timestamp":null,
        "tts":false,"mention_everyone":false,"mention_roles":[],"mentions":[],"pinned":false,"attachments":[],"embeds":[],"flags":0
    })).unwrap();
    sim.posts();
    assert!(pinchora_commands::on_text_command(sim.app.clone(), &message).await);
    assert!(
        sim.posts()
            .iter()
            .any(|p| p.body["content"].as_str().is_some_and(|s| s.contains("Manage Server")))
    );
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM custom_commands WHERE guild_id=$1")
        .bind(sim.guild as i64)
        .fetch_one(sim.app.db.pool())
        .await
        .unwrap();
    assert_eq!(count, 0);
}

/// The shared Discord mock exposes individual channels; backup also enumerates guild channels.
pub(super) fn mock(state: &Mock, method: &Method, parts: &[&str]) -> Option<(StatusCode, String)> {
    if let (&Method::GET, [.., "guilds", guild, "channels"]) = (method, parts) {
        let channels: Vec<_> = state
            .channels
            .lock()
            .unwrap()
            .values()
            .filter(|c| c["guild_id"].as_str() == Some(*guild))
            .cloned()
            .collect();
        return Some((StatusCode::OK, json!(channels).to_string()));
    }
    None
}

#[tokio::test]
async fn backup_channel_types_are_checked_for_numeric_ids_too() {
    use pinchora_core::{
        config_backup::{Import, Names, Policy},
        i18n::Lang,
    };
    let Some(sim) = Sim::new("backup_channel_types").await else {
        return;
    };
    let actor = unique();
    sim.channel_is(sim.channel, 0, None);
    let before = sim.app.db.export_config(sim.guild, Names::default()).await.unwrap();
    for id in [json!(sim.channel), json!(sim.channel.to_string())] {
        let mut backup = before.clone();
        backup.sections.get_mut("voice_creators").unwrap().push(json!({
            "channel_id":id,"category_id":null,"name_template":"{user}","default_limit":0,
            "default_locked":false,"max_per_user":1,"panel":false,"panel_channel_id":null,"enabled":true
        }));
        let (_, preview) = pinchora_commands::backup_preview(
            &sim.app,
            sim.guild,
            &Import {
                backup,
                policy: Policy::Merge,
            },
            Lang::En,
            actor,
        )
        .await
        .unwrap();
        assert!(!preview.can_import, "a text channel cannot be a voice creator");
    }
    assert!(sim.app.db.config_restore_points(sim.guild).await.unwrap().is_empty());
}
