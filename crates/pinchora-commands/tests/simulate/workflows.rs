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
        let expected = if lang == pinchora_core::i18n::Lang::En {
            vec![format!("PUT {user} {role}")]
        } else {
            Vec::new()
        };
        assert_eq!(
            sim.role_changes(),
            expected,
            "an already-held role needs no write or provenance reservation"
        );
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
    // A delayed callback has outlived the old short-lived Redis marker.
    sim.app
        .cache
        .take(&format!("auto-own:{}:{user}:{role}", sim.guild))
        .await
        .unwrap();
    event.roles = vec![Id::new(role)];
    tokio::join!(
        pinchora_commands::workflows::triggers::roles(&sim.app, &event),
        pinchora_commands::workflows::triggers::roles(&sim.app, &event)
    );
    sim.app.drain(std::time::Duration::from_secs(2)).await;
    assert!(sim.app.db.workflow_runs(sim.guild, second.id).await.unwrap().is_empty());
    assert!(sim.app.db.workflow_vars(sim.guild, user).await.unwrap().is_empty());
}

#[tokio::test]
async fn workflows_recheck_moved_managed_and_dangerous_roles_for_every_operation() {
    let Some(sim) = Sim::new("workflow_role_safety").await else {
        return;
    };
    let user = unique();
    let role = unique();
    sim.guild_role(role, "Safe when saved", 2);
    let event = Event {
        kind: "button".into(),
        user_id: user.to_string(),
        ..Default::default()
    };
    for operation in ["add_role", "remove_role", "toggle_role"] {
        let def: Definition = serde_json::from_value(json!({"name":operation,"enabled":true,"trigger":{"kind":"button"},"conditions":[],"actions":[{"kind":"tag","action":{"kind":operation,"role_id":role.to_string()}}]})).unwrap();
        let row = sim.app.db.save_workflow(sim.guild, None, &def, user).await.unwrap();
        for change in ["position", "managed", "permissions", "deleted"] {
            {
                let mut roles = sim.mock.roles.lock().unwrap();
                roles.retain(|r| r["id"].as_str().and_then(|id| id.parse::<u64>().ok()) != Some(role));
                if change != "deleted" {
                    let mut changed = role_json(role, "Changed after save", 2, change == "managed");
                    if change == "position" {
                        changed["position"] = json!(1000);
                    }
                    if change == "permissions" {
                        changed["permissions"] =
                            json!(twilight_model::guild::Permissions::ADMINISTRATOR.bits().to_string());
                    }
                    roles.push(changed);
                }
            }
            let result = pinchora_commands::workflows::engine::run(&sim.app, &row, event.clone())
                .await
                .unwrap();
            assert_eq!(result.outcome, "failed", "{operation}: {change}");
            assert!(sim.role_changes().is_empty(), "{operation}: {change}");
        }
    }
}

#[tokio::test]
async fn workflows_preview_and_live_messages_use_identical_rendering() {
    let Some(sim) = Sim::new("workflow_render_parity").await else {
        return;
    };
    let user = unique();
    sim.channel_is(sim.channel, 0, None);
    let def: Definition = serde_json::from_value(json!({"name":"render","enabled":true,"trigger":{"kind":"button"},"conditions":[],"actions":[{"kind":"set_var","scope":"server","key":"hello","value":"test"},{"kind":"reply","text":"  @everyone <@&123> {var.server.hello} {user}  ","embed":{"title":"  @here {var.server.hello}  "}}]})).unwrap();
    let row = sim.app.db.save_workflow(sim.guild, None, &def, user).await.unwrap();
    let event = Event {
        kind: "button".into(),
        user_id: user.to_string(),
        channel_id: sim.channel.to_string(),
        ..Default::default()
    };
    let preview = pinchora_commands::workflows::engine::dry_run(&def, &event, sim.app.now());
    assert!(sim.posts().is_empty());
    assert!(sim.app.db.workflow_vars(sim.guild, user).await.unwrap().is_empty());
    let live = pinchora_commands::workflows::engine::run(&sim.app, &row, event)
        .await
        .unwrap();
    assert_eq!(live.outcome, "completed");
    let posts = sim.posts();
    assert_eq!(posts.len(), 1);
    assert_eq!(preview.steps[2].detail["text"], posts[0].body["content"]);
    assert_eq!(
        preview.steps[2].detail["embed"]["title"],
        posts[0].body["embeds"][0]["title"]
    );
    assert_eq!(posts[0].body["allowed_mentions"]["parse"], json!([]));
}

#[tokio::test]
async fn workflows_ignore_bot_and_webhook_keywords_and_disabled_source_modules() {
    use pinchora_core::modules::Module;
    let Some(sim) = Sim::new("workflow_source_gates").await else {
        return;
    };
    let user = unique();
    let keyword: Definition = serde_json::from_value(json!({"name":"keyword","enabled":true,"trigger":{"kind":"message_keyword","keyword":"hello"},"conditions":[],"actions":[{"kind":"set_var","scope":"server","key":"loop","value":"1"}]})).unwrap();
    let keyword = sim.app.db.save_workflow(sim.guild, None, &keyword, user).await.unwrap();
    for webhook in [false, true] {
        let mut msg = message_json(sim.channel, unique());
        msg["guild_id"] = json!(sim.guild.to_string());
        msg["content"] = json!("hello");
        msg["author"] = person_json(user, "author", None, !webhook);
        if webhook {
            msg["webhook_id"] = json!(unique().to_string());
        }
        pinchora_commands::workflows::triggers::message(&sim.app, &serde_json::from_value(msg).unwrap());
    }
    sim.app.drain(std::time::Duration::from_secs(2)).await;
    assert!(
        sim.app
            .db
            .workflow_runs(sim.guild, keyword.id)
            .await
            .unwrap()
            .is_empty()
    );
    for (module, kind) in [
        (Module::Leveling, "level_up"),
        (Module::Birthdays, "birthday"),
        (Module::Achievements, "achievement"),
        (Module::Tickets, "ticket_opened"),
        (Module::Starboard, "starboard_post"),
        (Module::Feeds, "feed_item"),
    ] {
        let def: Definition = serde_json::from_value(json!({"name":kind,"enabled":true,"trigger":{"kind":kind},"conditions":[],"actions":[{"kind":"log","text":"blocked"}]})).unwrap();
        let row = sim.app.db.save_workflow(sim.guild, None, &def, user).await.unwrap();
        sim.app
            .db
            .set_module(sim.guild, module, Some(false), None)
            .await
            .unwrap();
        sim.app.forget_guild_settings(sim.guild).await;
        assert!(
            !pinchora_commands::workflows::trigger_catalog(&sim.app, sim.guild)
                .await
                .iter()
                .any(|t| t.key() == kind)
        );
        assert!(
            pinchora_commands::workflows::engine::run(
                &sim.app,
                &row,
                Event {
                    kind: kind.into(),
                    user_id: user.to_string(),
                    ..Default::default()
                }
            )
            .await
            .is_err()
        );
        assert!(sim.app.db.workflow_runs(sim.guild, row.id).await.unwrap().is_empty());
    }
    assert!(sim.app.db.workflow_vars(sim.guild, user).await.unwrap().is_empty());
}

#[tokio::test]
async fn workflows_refuse_excess_writes_before_discord_and_fail_unknown_channels_cleanly() {
    let Some(sim) = Sim::new("workflow_write_limits").await else {
        return;
    };
    let user = unique();
    let def: Definition = serde_json::from_value(json!({"name":"writes","enabled":true,"trigger":{"kind":"button"},"conditions":[],"actions":[{"kind":"reply","text":"hello"}]})).unwrap();
    let mut row = sim.app.db.save_workflow(sim.guild, None, &def, user).await.unwrap();
    let event = Event {
        kind: "button".into(),
        user_id: user.to_string(),
        channel_id: unique().to_string(),
        ..Default::default()
    };
    let result = pinchora_commands::workflows::engine::run(&sim.app, &row, event.clone())
        .await
        .unwrap();
    assert_eq!(result.outcome, "failed");
    assert!(sim.posts().is_empty());
    sim.channel_is(sim.channel, 0, None);
    sim.mock.blocked.lock().unwrap().push(sim.channel);
    let denied = pinchora_commands::workflows::engine::run(
        &sim.app,
        &row,
        Event {
            channel_id: sim.channel.to_string(),
            ..event.clone()
        },
    )
    .await
    .unwrap();
    assert_eq!(denied.outcome, "failed");
    assert!(sim.posts().is_empty());
    row.actions = json!(
        (0..6)
            .map(|_| json!({"kind":"reply","text":"hello"}))
            .collect::<Vec<_>>()
    );
    assert!(
        pinchora_commands::workflows::engine::run(&sim.app, &row, event)
            .await
            .is_err()
    );
    assert!(sim.posts().is_empty());
    assert_eq!(sim.app.db.workflow_runs(sim.guild, row.id).await.unwrap().len(), 2);
}

#[tokio::test]
async fn workflows_concurrent_points_share_one_daily_allowance() {
    let Some(mut sim) = Sim::new("workflow_points_race").await else {
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
    let def: Definition = serde_json::from_value(json!({"name":"points","enabled":true,"trigger":{"kind":"button"},"conditions":[],"actions":[{"kind":"tag","action":{"kind":"give_points","amount":100}}]})).unwrap();
    let row = sim.app.db.save_workflow(sim.guild, None, &def, user).await.unwrap();
    let mut tasks = tokio::task::JoinSet::new();
    for _ in 0..20 {
        let app = sim.app.clone();
        let row = row.clone();
        tasks.spawn(async move {
            pinchora_commands::workflows::engine::run(
                &app,
                &row,
                Event {
                    kind: "button".into(),
                    user_id: user.to_string(),
                    ..Default::default()
                },
            )
            .await
            .unwrap()
            .outcome
        });
    }
    let mut completed = 0;
    while let Some(result) = tasks.join_next().await {
        completed += usize::from(result.unwrap() == "completed");
    }
    assert_eq!(completed, 10);
    assert_eq!(sim.points(user).await, 1000);
}

#[tokio::test]
async fn workflows_claim_all_cooldowns_atomically_without_consuming_on_refusal() {
    use pinchora_core::workflows::{Condition, Scope};
    let Some(sim) = Sim::new("workflow_cooldown_race").await else {
        return;
    };
    let user = unique();
    let def = Definition {
        name: "cooldowns".into(),
        enabled: true,
        trigger: pinchora_core::workflows::Trigger::Button {},
        conditions: vec![
            Condition::Cooldown {
                scope: Scope::Server,
                seconds: 60,
            },
            Condition::Cooldown {
                scope: Scope::Member,
                seconds: 60,
            },
        ],
        actions: vec![pinchora_core::workflows::StepAction::Log { text: "ok".into() }],
    };
    let row = sim.app.db.save_workflow(sim.guild, None, &def, user).await.unwrap();
    let server = format!("wf:cool:{}:{}:0:0", sim.guild, row.id);
    let member = format!("wf:cool:{}:{}:1:{user}", sim.guild, row.id);
    sim.app
        .cache
        .try_cooldown(&member, std::time::Duration::from_secs(60))
        .await
        .unwrap();
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
        "filtered"
    );
    sim.app.cache.release_cooldown(&member).await.unwrap();
    assert_eq!(
        pinchora_commands::workflows::engine::run(&sim.app, &row, event)
            .await
            .unwrap()
            .outcome,
        "completed"
    );
    sim.app.cache.release_cooldown(&server).await.unwrap();
    let mut tasks = tokio::task::JoinSet::new();
    for _ in 0..8 {
        let app = sim.app.clone();
        let row = row.clone();
        let user = unique();
        tasks.spawn(async move {
            pinchora_commands::workflows::engine::run(
                &app,
                &row,
                Event {
                    kind: "button".into(),
                    user_id: user.to_string(),
                    ..Default::default()
                },
            )
            .await
            .unwrap()
            .outcome
        });
    }
    let mut completed = 0;
    while let Some(result) = tasks.join_next().await {
        completed += usize::from(result.unwrap() == "completed");
    }
    assert_eq!(completed, 1);
}

#[tokio::test]
async fn workflows_member_leave_still_evaluates_account_age_without_a_member_fetch() {
    let Some(sim) = Sim::new("workflow_leave_context").await else {
        return;
    };
    let user = unique();
    sim.mock.missing_members.lock().unwrap().push((sim.guild, user));
    let def: Definition = serde_json::from_value(json!({"name":"leave","enabled":true,"trigger":{"kind":"member_leave"},"conditions":[{"kind":"account_age","days":1}],"actions":[{"kind":"log","text":"goodbye"}]})).unwrap();
    let row = sim.app.db.save_workflow(sim.guild, None, &def, user).await.unwrap();
    let result = pinchora_commands::workflows::engine::run(
        &sim.app,
        &row,
        Event {
            kind: "member_leave".into(),
            user_id: user.to_string(),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    assert_eq!(result.outcome, "completed");
}
