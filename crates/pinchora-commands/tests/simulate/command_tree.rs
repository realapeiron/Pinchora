use super::*;
use pinchora_commands::routing::{MANIFEST, route};

pub(super) fn setup_options(path: &str, options: &Value) -> Option<(String, String, Vec<Value>)> {
    let route = route(path).filter(|r| r.active() && r.canonical.starts_with("setup "))?;
    let mut arguments = options.as_array().unwrap();
    while let Some(option) = arguments.first().filter(|o| o["type"] == 1 || o["type"] == 2) {
        arguments = option["options"].as_array().unwrap();
    }
    let mut parts = route.canonical.split(' ').skip(1);
    Some((parts.next()?.into(), parts.next()?.into(), arguments.clone()))
}

#[tokio::test]
async fn setup_every_action_refuses_without_manage_server_without_mutation() {
    use pinchora_core::config_backup::Names;
    let Some(mut sim) = Sim::new("setup_every_action_refuses").await else {
        return;
    };
    let user = unique();
    let before = sim.app.db.export_config(sim.guild, Names::default()).await.unwrap();
    for permissions in [0, ((1u64 << 50) - 1) & !(MANAGE_GUILD | ADMINISTRATOR)] {
        sim.permissions = permissions;
        for route in MANIFEST
            .iter()
            .filter(|r| r.active() && r.canonical.starts_with("setup "))
        {
            let parts: Vec<_> = route.canonical.split(' ').collect();
            let reply = sim
                .command(user, "setup", json!([group(parts[1], sub(parts[2], vec![]))]))
                .await;
            assert!(
                reply.ephemeral && reply.content().contains("Manage Server"),
                "{}: {reply}",
                route.canonical
            );
        }
    }
    assert_eq!(
        sim.app.db.export_config(sim.guild, Names::default()).await.unwrap(),
        before
    );
}

#[tokio::test]
async fn setup_groups_reach_the_same_handlers_as_legacy_paths() {
    let Some(mut sim) = Sim::new("setup_groups_same_handlers").await else {
        return;
    };
    let user = unique();
    sim.permissions = MANAGE_GUILD;
    sim.own_server(user);
    let rows = [
        ("config prefix show", json!([group("prefix", sub("show", vec![]))])),
        ("config welcome", json!([sub("welcome", vec![])])),
        ("config moderation", json!([sub("moderation", vec![])])),
        ("config leveling", json!([sub("leveling", vec![])])),
        ("config points", json!([sub("points", vec![])])),
        ("autoroles list", json!([sub("list", vec![])])),
        ("tickets panel list", json!([group("panel", sub("list", vec![]))])),
        ("voice creators list", json!([group("creators", sub("list", vec![]))])),
        ("automod show", json!([sub("show", vec![])])),
        (
            "verification mode",
            json!([sub("mode", vec![text_option("mode", "button")])]),
        ),
        ("antinuke status", json!([sub("status", vec![])])),
    ];
    for (path, options) in rows {
        let old = sim
            .command(user, path.split(' ').next().unwrap(), options.clone())
            .await;
        let (group_name, action, args) = setup_options(path, &options).unwrap();
        let new = sim
            .command(user, "setup", json!([group(&group_name, sub(&action, args))]))
            .await;
        assert_eq!(new.content(), old.content(), "{path}");
        assert_eq!(new.ephemeral, old.ephemeral, "{path}");
        assert_eq!(new.files, old.files, "{path}");
    }
}

#[tokio::test]
async fn setup_cannot_bypass_antinuke_owner_checks_or_legacy_policy() {
    use pinchora_core::command_control::{Config, Rule};
    let Some(mut sim) = Sim::new("setup_policy_and_owner").await else {
        return;
    };
    let user = unique();
    sim.permissions = MANAGE_GUILD;
    for (name, options) in [
        ("antinuke", json!([sub("status", vec![])])),
        ("setup", json!([group("antinuke", sub("status", vec![]))])),
    ] {
        let reply = sim.command(user, name, options).await;
        assert!(
            reply.content().contains("owner") || reply.content().contains("trusted"),
            "{reply}"
        );
    }
    for denied in ["automod", "setup automod", "setup automod show"] {
        let config = Config {
            rules: [(
                denied.into(),
                Rule {
                    enabled: false,
                    ..Rule::default()
                },
            )]
            .into(),
            ..Config::default()
        };
        sim.app.db.set_command_control(sim.guild, &config).await.unwrap();
        sim.app.announce_guild_change(sim.guild).await;
        for (name, options) in [
            ("automod", json!([sub("show", vec![])])),
            ("setup", json!([group("automod", sub("show", vec![]))])),
        ] {
            let reply = sim.command(user, name, options).await;
            assert!(reply.content().contains("disabled"), "{denied}: {reply}");
        }
    }
}

#[tokio::test]
async fn setup_role_panel_keeps_member_buttons_and_old_custom_ids() {
    let Some(mut sim) = Sim::new("setup_member_role_panel").await else {
        return;
    };
    let (admin, member, role_id) = (unique(), unique(), unique());
    sim.permissions = MANAGE_GUILD;
    sim.own_server(admin);
    sim.channel_is(sim.channel, 0, None);
    let role = sim.guild_role(role_id, "Coast", 2);
    sim.command(
        admin,
        "setup",
        json!([group("roles", sub("panel-create", vec![text_option("title", "Coast")]))]),
    )
    .await;
    sim.command_resolved(
        admin,
        "setup",
        json!([group(
            "roles",
            sub(
                "panel-add",
                vec![
                    text_option("panel", "Coast"),
                    role_option("role", &role),
                    text_option("label", "Coast")
                ]
            )
        )]),
        resolved(&[], &[&role]),
    )
    .await;
    let channel = sim.channel;
    let posted = sim
        .command_resolved(
            admin,
            "setup",
            json!([group(
                "roles",
                sub(
                    "panel-post",
                    vec![text_option("panel", "Coast"), channel_option("channel", channel)]
                )
            )]),
            resolved_channels(&[channel]),
        )
        .await;
    assert!(posted.content().starts_with("Posted"), "{posted}");
    let posts = sim.posts();
    let panel_message = posted_reply(posts.last().unwrap());
    let panel = sim.app.db.find_panel(sim.guild, "Coast").await.unwrap().unwrap();
    sim.permissions = 0;
    let clicked = sim
        .component(member, &format!("rr:{}:{role_id}", panel.id), &[], &panel_message)
        .await;
    assert_eq!(clicked.content(), format!("Added <@&{role_id}>."));
    assert_eq!(sim.role_changes(), [format!("PUT {member} {role_id}")]);
}

fn options_for(path: &str, mut arguments: Vec<Value>) -> Value {
    let parts: Vec<_> = path.split(' ').collect();
    for (index, name) in parts.iter().enumerate().skip(1).rev() {
        arguments = vec![json!({"type": if index == parts.len()-1 {1} else {2}, "name":name, "options":arguments})];
    }
    json!(arguments)
}

#[tokio::test]
async fn mod_every_action_preserves_its_own_permission_checks() {
    let Some(mut sim) = Sim::new("mod_every_action_permissions").await else {
        return;
    };
    let (user, target, role_id) = (unique(), unique(), unique());
    let role = sim.guild_role(role_id, "Coast", 2);
    let resolved = json!({"users":{target.to_string(): user_json(target)}, "roles":{role_id.to_string():role}});
    for route in MANIFEST.iter().filter(|r| r.slice == 9) {
        let args = match route.legacy.as_str() {
            "unban" | "untimeout" | "warnings" | "nick" => vec![user_option("user", target)],
            "unwarn" => vec![int_option("case", 1)],
            "case" | "suggestion approve" | "suggestion deny" => vec![int_option("id", 1)],
            "reason" => vec![int_option("id", 1), text_option("reason", "Test")],
            "slowmode" => vec![text_option("duration", "30s")],
            "role add" | "role remove" => vec![user_option("user", target), role_option("role", &role)],
            _ => vec![],
        };
        sim.permissions = 0;
        let old = sim
            .command_resolved(
                user,
                route.legacy.split(' ').next().unwrap(),
                options_for(&route.legacy, args.clone()),
                resolved.clone(),
            )
            .await;
        let new = sim
            .command_resolved(user, "mod", options_for(&route.canonical, args), resolved.clone())
            .await;
        assert!(old.ephemeral && new.ephemeral, "{}", route.canonical);
        assert_eq!(old.content(), new.content(), "{}", route.canonical);
        assert!(!new.content().contains("Something went wrong"), "{new}");
    }
    // Case lookup accepts Ban Members alone, without Manage Server or Moderate Members.
    sim.permissions = 1 << 2;
    let old = sim.command(user, "case", json!([int_option("id", 999)])).await;
    let new = sim
        .command(user, "mod", options_for("mod case", vec![int_option("id", 999)]))
        .await;
    assert_eq!(old.content(), new.content());
    assert!(new.content().contains("999"), "{new}");
    assert!(sim.mock.mod_calls.lock().unwrap().is_empty());
}

#[tokio::test]
async fn setup_slices_seven_eight_eleven_reach_legacy_handlers() {
    let Some(mut sim) = Sim::new("setup_remaining_same_handlers").await else {
        return;
    };
    let user = unique();
    sim.permissions = MANAGE_GUILD;
    sim.own_server(user);
    for path in [
        "feeds list",
        "starboard list",
        "counters list",
        "inviteadmin settings",
        "birthdays settings",
        "achievementadmin list",
        "sticky show",
        "schedule list",
        "config suggestions",
        "customcommand list",
        "commandcontrol show",
        "backup list",
    ] {
        let route = route(path).unwrap();
        let old = sim
            .command(user, path.split(' ').next().unwrap(), options_for(path, vec![]))
            .await;
        let new = sim.command(user, "setup", options_for(&route.canonical, vec![])).await;
        assert_eq!(old.content(), new.content(), "{path}");
        assert_eq!(old.ephemeral, new.ephemeral, "{path}");
        assert_eq!(old.components, new.components, "{path}");
    }
}

#[tokio::test]
async fn command_path_migration_is_idempotent_and_keeps_conflicting_restrictions() {
    use pinchora_core::command_control::{Config, Rule};
    let Some(sim) = Sim::new("command_path_migration").await else {
        return;
    };
    let config = Config {
        rules: [
            (
                "feeds list".into(),
                Rule {
                    enabled: false,
                    ..Rule::default()
                },
            ),
            (
                "setup feeds list".into(),
                Rule {
                    allowed_roles: vec!["123".into()],
                    ..Rule::default()
                },
            ),
            (
                "starboard top".into(),
                Rule {
                    enabled: false,
                    ..Rule::default()
                },
            ),
            (
                "sticky".into(),
                Rule {
                    enabled: false,
                    ..Rule::default()
                },
            ),
        ]
        .into(),
        aliases: [
            ("feed".into(), "feeds list".into()),
            ("xpimport".into(), "xp import mee6".into()),
        ]
        .into(),
    };
    sim.app.db.set_command_control(sim.guild, &config).await.unwrap();
    let migrated = sim.app.db.command_control(sim.guild).await.unwrap();
    assert_eq!(migrated.aliases["feed"], "setup feeds list");
    assert_eq!(migrated.aliases["xpimport"], "setup leveling xp-import-mee6");
    assert!(migrated.rules.contains_key("setup starboard top"));
    assert!(!migrated.rules.contains_key("starboard top"));
    assert_eq!(migrated.rules["feeds list"], config.rules["feeds list"]);
    assert_eq!(migrated.rules["setup feeds list"], config.rules["setup feeds list"]);
    assert!(migrated.rules.contains_key("sticky"));
    sim.app.db.set_command_control(sim.guild, &migrated).await.unwrap();
    assert_eq!(sim.app.db.command_control(sim.guild).await.unwrap(), migrated);
}

#[tokio::test]
async fn player_additions_keep_arguments_and_original_modules() {
    use pinchora_core::modules::Module;
    let Some(mut sim) = Sim::new("tree_player_additions").await else {
        return;
    };
    let user = unique();
    sim.permissions = 0;
    for path in ["bio", "customize bio"] {
        let reply = sim
            .command(
                user,
                path.split(' ').next().unwrap(),
                options_for(path, vec![text_option("text", "Coast life")]),
            )
            .await;
        assert!(
            reply.content() == pinchora_core::t!(pinchora_core::i18n::Lang::En, "bio.updated"),
            "{reply}"
        );
    }
    {
        let module = Module::Profile;
        let paths = ["wrapped", "info wrapped"];
        sim.app
            .db
            .set_module(sim.guild, module, Some(false), None)
            .await
            .unwrap();
        sim.app.announce_guild_change(sim.guild).await;
        for path in paths {
            let reply = sim
                .command(user, path.split(' ').next().unwrap(), options_for(path, vec![]))
                .await;
            assert!(
                reply.content().contains("off") || reply.content().contains("disabled"),
                "{reply}"
            );
        }
    }
    // Both trees share the reward ledger and cooldown; disabling Profile does not disable Economy.
    let before = shells(&sim, user).await;
    for (reward, amount) in [("weekly", 1_500), ("monthly", 6_500)] {
        let claimed = sim
            .command(user, "rewards", options_for(&format!("rewards {reward}"), vec![]))
            .await;
        assert!(claimed.title().contains("shells"), "{claimed}");
        let again = sim.command(user, reward, json!([])).await;
        assert!(again.title().contains("Already collected"), "{again}");
        assert_eq!(
            shells(&sim, user).await,
            before + if reward == "weekly" { amount } else { 8_000 }
        );
    }
}

#[tokio::test]
async fn wrapped_keeps_dm_access_while_info_actions_keep_the_guild_guard() {
    let Some(sim) = Sim::new("tree_wrapped_dm").await else {
        return;
    };
    let user = unique();
    let mut old = None;
    for path in ["wrapped", "info wrapped", "info user"] {
        let args = if path == "info user" {
            vec![]
        } else {
            vec![int_option("year", 2200)]
        };
        let interaction = sim.interaction(
            user,
            2,
            json!({
                "id":"1", "name":path.split(' ').next().unwrap(), "type":1, "options":options_for(path, args)
            }),
            None,
        );
        let mut value = serde_json::to_value(interaction).unwrap();
        value["user"] = value["member"]["user"].clone();
        value["member"] = Value::Null;
        value["guild_id"] = Value::Null;
        value["guild_locale"] = Value::Null;
        value["context"] = json!(1);
        value["channel"] = json!({"id":sim.channel.to_string(), "type":1});
        let interaction: Interaction = serde_json::from_value(value).unwrap();
        let (tx, rx) = oneshot::channel();
        let responder = Arc::new(Responder::http(sim.app.clone(), &interaction, tx));
        handle_interaction(sim.app.clone(), interaction, responder).await;
        let response = rx.await.unwrap().data.unwrap();
        let content = response.content.unwrap();
        match path {
            "wrapped" => old = Some(content),
            "info wrapped" => assert_eq!(Some(content), old),
            _ => assert_eq!(
                content,
                pinchora_core::t!(pinchora_core::i18n::Lang::En, "error.guild-only")
            ),
        }
    }
}
