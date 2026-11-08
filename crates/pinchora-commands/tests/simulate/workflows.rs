use super::*;
use pinchora_core::workflows::{Definition, Event};
#[tokio::test]
async fn workflows_button_role_message_and_dry_run_en_tr() {
    let Some(mut sim) = Sim::new("workflows_button").await else {
        return;
    };
    let user = unique();
    let role = unique();
    sim.guild_role(role, "Workflow role", 2);
    sim.channel_is(sim.channel, 0, None);
    for lang in [pinchora_core::i18n::Lang::En, pinchora_core::i18n::Lang::Tr] {
        sim.app.set_user_lang(user, Some(lang)).await.unwrap();
        let def:Definition=serde_json::from_value(json!({"name":format!("greet_{}",lang.code()),"enabled":true,"trigger":{"kind":"button"},"conditions":[],"actions":[{"kind":"tag","action":{"kind":"add_role","role_id":role.to_string()}},{"kind":"reply","text":"Hello @everyone <@&123> {user}"}]})).unwrap();
        let row = sim.app.db.save_workflow(sim.guild, None, &def, user).await.unwrap();
        let event = Event {
            kind: "button".into(),
            user_id: user.to_string(),
            channel_id: sim.channel.to_string(),
            ..Default::default()
        };
        let dry = pinchora_commands::workflows::engine::dry_run(&def, &event, sim.app.now());
        assert_eq!(dry.outcome, "ready");
        assert!(sim.role_changes().is_empty());
        assert!(sim.app.db.workflow_runs(sim.guild, row.id).await.unwrap().is_empty());
        let shown = sim.command(user, "ping", json!([])).await;
        let reply = sim.component(user, &format!("wf:b:{}:0", row.id), &[], &shown).await;
        assert!(reply.ephemeral);
        assert!(
            reply
                .followups
                .last()
                .unwrap_or(&reply)
                .content()
                .contains(if lang == pinchora_core::i18n::Lang::En {
                    "completed"
                } else {
                    "tamamlandı"
                }),
            "{reply}"
        );
        assert_eq!(sim.role_changes(), [format!("PUT {user} {role}")]);
        let posts = sim.posts();
        let post = posts
            .iter()
            .find(|p| p.body["content"].as_str().is_some_and(|t| t.starts_with("Hello")))
            .unwrap();
        assert_eq!(post.body["allowed_mentions"]["parse"], json!([]));
        let mentions: twilight_model::channel::message::AllowedMentions =
            serde_json::from_value(post.body["allowed_mentions"].clone()).unwrap();
        assert!(mentions.roles.is_empty() && mentions.users.is_empty() && !mentions.replied_user);
        let runs = sim.app.db.workflow_runs(sim.guild, row.id).await.unwrap();
        assert_eq!(runs[0].outcome, "completed");
        assert_eq!(runs[0].steps[1]["status"], "done");
        assert!(
            pinchora_commands::workflows::engine::run(&sim.app, &row, Event { depth: 1, ..event })
                .await
                .is_err()
        );
    }
}
#[tokio::test]
async fn workflows_keyword_and_module_refuse_before_actions() {
    let Some(sim) = Sim::with("workflows_intent", |app| app.message_content = false).await else {
        return;
    };
    let user = unique();
    let def:Definition=serde_json::from_value(json!({"name":"keyword","enabled":true,"trigger":{"kind":"message_keyword","keyword":"hello"},"conditions":[],"actions":[{"kind":"log","text":"hello"}]})).unwrap();
    let row = sim.app.db.save_workflow(sim.guild, None, &def, user).await.unwrap();
    assert!(!sim.app.message_content);
    assert!(
        pinchora_commands::workflows::engine::run(
            &sim.app,
            &row,
            Event {
                kind: "message_keyword".into(),
                ..Default::default()
            }
        )
        .await
        .is_err()
    );
    assert!(sim.app.db.workflow_runs(sim.guild, row.id).await.unwrap().is_empty());
}

#[tokio::test]
async fn workflows_share_the_tag_points_cap_without_touching_shells() {
    let Some(mut sim) = Sim::new("workflow_shared_points").await else {
        return;
    };
    let user = unique();
    sim.permissions = MANAGE_GUILD;
    sim.command(
        user,
        "config",
        json!([sub("points", vec![bool_option("enabled", true)])]),
    )
    .await;
    sim.permissions = 0;
    let tag: pinchora_core::tags::CommandDef = serde_json::from_value(
        json!({"name":"reward","responses":["ok"],"cooldown_secs":30,"actions":[{"kind":"give_points","amount":900}]}),
    )
    .unwrap();
    sim.app.db.add_custom_command(sim.guild, &tag, user).await.unwrap();
    sim.command(user, "tag", json!([text_option("name", "reward")])).await;
    assert_eq!(sim.points(user).await, 900);
    let before = sim.app.db.user(user).await.unwrap().credits;
    let def:Definition=serde_json::from_value(json!({"name":"remaining","enabled":true,"trigger":{"kind":"button"},"conditions":[],"actions":[{"kind":"tag","action":{"kind":"give_points","amount":100}}]})).unwrap();
    let row = sim.app.db.save_workflow(sim.guild, None, &def, user).await.unwrap();
    let event = Event {
        kind: "button".into(),
        user_id: user.to_string(),
        ..Default::default()
    };
    assert_eq!(
        pinchora_commands::workflows::engine::run(&sim.app, &row, event.clone())
            .await
            .unwrap()
            .outcome,
        "completed"
    );
    assert_eq!(
        pinchora_commands::workflows::engine::run(&sim.app, &row, event)
            .await
            .unwrap()
            .outcome,
        "failed"
    );
    assert_eq!(sim.points(user).await, 1000);
    assert_eq!(sim.app.db.user(user).await.unwrap().credits, before);
}

#[tokio::test]
async fn workflows_role_callbacks_cannot_start_another_workflow() {
    let Some(sim) = Sim::new("workflow_no_chain").await else {
        return;
    };
    let user = unique();
    let role = unique();
    sim.guild_role(role, "Workflow role", 2);
    let def: Definition = serde_json::from_value(json!({"name":"first","enabled":true,"trigger":{"kind":"button"},"conditions":[],"actions":[{"kind":"tag","action":{"kind":"add_role","role_id":role.to_string()}}]})).unwrap();
    let first = sim.app.db.save_workflow(sim.guild, None, &def, user).await.unwrap();
    let next: Definition = serde_json::from_value(json!({"name":"second","enabled":true,"trigger":{"kind":"role_added","role_id":role.to_string()},"conditions":[],"actions":[{"kind":"set_var","scope":"server","key":"chained","value":"1"}]})).unwrap();
    let second = sim.app.db.save_workflow(sim.guild, None, &next, user).await.unwrap();
    let mut event: MemberUpdate = serde_json::from_value(json!({
        "guild_id":sim.guild.to_string(),"user":person_json(user,"member",Some("member"),false),
        "roles":[],"nick":null,"avatar":null,"communication_disabled_until":null,
        "joined_at":null,"deaf":null,"mute":null,"pending":false,"premium_since":null
    }))
    .unwrap();
    pinchora_commands::workflows::triggers::roles(&sim.app, &event).await;
    let run = pinchora_commands::workflows::engine::run(
        &sim.app,
        &first,
        Event {
            kind: "button".into(),
            user_id: user.to_string(),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    assert_eq!(run.outcome, "completed");
    event.roles = vec![Id::new(role)];
    pinchora_commands::workflows::triggers::roles(&sim.app, &event).await;
    sim.app.drain(std::time::Duration::from_secs(2)).await;
    assert!(sim.app.db.workflow_runs(sim.guild, second.id).await.unwrap().is_empty());
    assert!(sim.app.db.workflow_vars(sim.guild, user).await.unwrap().is_empty());
}
