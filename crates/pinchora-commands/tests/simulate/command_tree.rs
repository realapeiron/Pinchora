use super::*;
use pinchora_commands::routing::{MANIFEST, route};

pub(super) fn setup_options(path: &str, options: &Value) -> Option<(String, String, Vec<Value>)> {
    let route = route(path).filter(|r| r.active())?;
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
        for route in MANIFEST.iter().filter(|r| r.active()) {
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
