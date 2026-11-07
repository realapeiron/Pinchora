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
