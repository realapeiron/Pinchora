//! Member and manager Community flows in both supported languages.
use super::*;

#[tokio::test]
async fn member_suggests_and_staff_approves_qotd() {
    let Some(mut sim) = Sim::new("member_suggests_and_staff_approves_qotd").await else {
        return;
    };
    let member = unique();
    let staff = unique();
    let suggested = sim
        .command(
            member,
            "qotd",
            json!([sub("suggest", vec![text_option("question", "What made you smile?")])]),
        )
        .await;
    assert!(
        suggested.ephemeral && suggested.content().contains("review queue"),
        "{suggested}"
    );
    sim.permissions = twilight_model::guild::Permissions::MANAGE_GUILD.bits();
    let queue = sim
        .command(staff, "community", json!([group("qotd", sub("queue", vec![]))]))
        .await;
    assert!(
        queue.ephemeral && queue.content().contains("What made you smile?"),
        "{queue}"
    );
    let id = sim.app.db.qotd_queue(sim.guild).await.unwrap()[0].id;
    let decided = sim.component(staff, &format!("qd:ok:{id}"), &[], &queue).await;
    assert!(decided.ephemeral && decided.content().contains("updated"), "{decided}");
    assert!(sim.app.db.qotd_queue(sim.guild).await.unwrap()[0].approved);
    sqlx::query("UPDATE qotd_questions SET text_tr='Türkçe soru?' WHERE id=$1")
        .bind(id)
        .execute(sim.app.db.pool())
        .await
        .unwrap();
    for _ in 0..14 {
        sim.app
            .db
            .suggest_qotd(sim.guild, member, &"a".repeat(300))
            .await
            .unwrap();
    }
    sim.locale = "tr";
    let queue = sim
        .command(staff, "community", json!([group("qotd", sub("queue", vec![]))]))
        .await;
    assert!(queue.content().contains("Türkçe soru?"));
    assert!(
        queue.content().chars().count() <= 2000,
        "the queue must fit a Discord message"
    );
}

#[tokio::test]
async fn counting_and_forms_reply_in_turkish() {
    let Some(mut sim) = Sim::new("counting_and_forms_reply_in_turkish").await else {
        return;
    };
    sim.locale = "tr";
    let member = unique();
    let stats = sim.command(member, "counting", json!([sub("stats", vec![])])).await;
    assert!(stats.ephemeral && stats.content().contains("Rekor"), "{stats}");
    let form = sim.command(member, "form", json!([sub("list", vec![])])).await;
    assert!(
        form.ephemeral && form.content().contains("Açık başvuru formu yok"),
        "{form}"
    );
}

#[tokio::test]
async fn forms_validate_modal_ids_order_throttle_and_review_permissions() {
    let Some(mut sim) = Sim::new("forms_validate_modal_ids_order_throttle_and_review_permissions").await else {
        return;
    };
    let user = unique();
    let form = sim
        .app
        .db
        .save_form(&pinchora_db::FormRow {
            id: 0,
            guild_id: sim.guild as i64,
            name: "a".repeat(45),
            questions: json!(["First?", "Second?"]),
            submit_channel_id: Some(sim.channel as i64),
            review_roles: vec![],
            accept_role_id: None,
            dm_result: true,
            open: true,
            message_id: None,
        })
        .await
        .unwrap()
        .unwrap();
    let modal = sim
        .command(
            user,
            "form",
            json!([sub("open", vec![json!({"type":4,"name":"id","value":form})])]),
        )
        .await;
    assert!(modal.modal.is_some(), "{modal}");
    let denied = sim.submit_fields(user, &modal, &[("a0", "one"), ("a0", "two")]).await;
    assert!(!denied.content().contains("submitted"));
    let accepted = sim.submit_fields(user, &modal, &[("a1", "two"), ("a0", "one")]).await;
    assert!(
        accepted.ephemeral && accepted.content().contains("submitted"),
        "{accepted}"
    );
    let posts = sim.posts();
    assert_eq!(posts.len(), 1);
    assert!(
        posts[0].embed()["description"]
            .as_str()
            .unwrap()
            .contains("**First?**\none")
    );
    let replay = sim.submit_fields(user, &modal, &[("a0", "one"), ("a1", "two")]).await;
    assert!(!replay.content().contains("submitted"));
    let sub: i64 = sqlx::query_scalar("SELECT id FROM form_submissions WHERE form_id=$1")
        .bind(form)
        .fetch_one(sim.app.db.pool())
        .await
        .unwrap();
    let denied = sim.component(unique(), &format!("fm:a:{sub}"), &[], &accepted).await;
    assert!(denied.content().contains("Manage Server"), "{denied}");
    let status: String = sqlx::query_scalar("SELECT status FROM form_submissions WHERE id=$1")
        .bind(sub)
        .fetch_one(sim.app.db.pool())
        .await
        .unwrap();
    assert_eq!(status, "pending");
    sim.app.db.set_dm_opt_out(user, true).await.unwrap();
    sim.permissions = twilight_model::guild::Permissions::MANAGE_GUILD.bits();
    let reviewed = sim.component(unique(), &format!("fm:a:{sub}"), &[], &accepted).await;
    assert!(reviewed.content().contains("Application updated"));
    assert!(sim.posts().is_empty(), "server DM opt-out must cover form decisions");
}

#[tokio::test]
async fn counting_edits_deletes_and_restart_preserve_accepted_state() {
    use twilight_model::gateway::payload::incoming::{MessageDelete, MessageUpdate};
    let Some(mut sim) = Sim::new("counting_edits_deletes_and_restart_preserve_accepted_state").await else {
        return;
    };
    sim.channel = unique();
    let user = unique();
    sim.app
        .db
        .save_counting(sim.guild, sim.channel, false, false, None)
        .await
        .unwrap();
    let mut message = sim.chat_in(user, sim.channel, &[]);
    message.message.content = "1".into();
    on_message(sim.app.clone(), message.clone()).await.unwrap();
    let mut edited = message.message.clone();
    edited.content = "900".into();
    edited.edited_timestamp = Some(twilight_model::util::Timestamp::from_secs(1_700_000_000).unwrap());
    pinchora_commands::on_message_update(sim.app.clone(), MessageUpdate(edited))
        .await
        .unwrap();
    pinchora_commands::on_message_delete(
        sim.app.clone(),
        MessageDelete {
            channel_id: message.channel_id,
            guild_id: message.guild_id,
            id: message.id,
        },
    )
    .await
    .unwrap();
    let db = Db::connect_waiting(
        &std::env::var("PINCHORA_TEST_DATABASE_URL").unwrap(),
        2,
        std::time::Duration::from_secs(10),
    )
    .await
    .unwrap();
    let row = db.counting_for_guild(sim.guild).await.unwrap().unwrap();
    assert_eq!((row.current, row.record, row.last_user_id), (1, 1, Some(user as i64)));
    assert_eq!(
        db.count_message(sim.channel, user, "2").await.unwrap().unwrap().0,
        pinchora_core::community::CountResult::SameUser
    );
    assert_eq!(
        db.count_message(sim.channel, user + 1, "2").await.unwrap().unwrap().0,
        pinchora_core::community::CountResult::Correct
    );
}

#[tokio::test]
async fn anonymous_modmail_replies_require_staff_even_when_roles_are_empty() {
    let Some(mut sim) = Sim::new("anonymous_modmail_replies_require_staff_even_when_roles_are_empty").await else {
        return;
    };
    sim.channel = unique();
    let user = unique();
    let staff = unique();
    sqlx::query("INSERT INTO modmail_settings(guild_id,enabled,anonymous_staff) VALUES($1,true,true)")
        .bind(sim.guild as i64)
        .execute(sim.app.db.pool())
        .await
        .unwrap();
    sim.app.db.modmail_create(sim.guild, user, sim.channel).await.unwrap();
    let mut message = sim.chat_in(staff, sim.channel, &[]);
    message.message.content = "private response".into();
    on_message(sim.app.clone(), message.clone()).await.unwrap();
    assert!(
        sim.posts().is_empty(),
        "an unconfigured staff list is not public access"
    );
    sim.own_server(staff);
    // Use a staff role to avoid relying on the cached guild owner after its first lookup.
    let role = unique();
    sqlx::query("UPDATE modmail_settings SET staff_roles=$2 WHERE guild_id=$1")
        .bind(sim.guild as i64)
        .bind(vec![role as i64])
        .execute(sim.app.db.pool())
        .await
        .unwrap();
    let mut message = sim.chat_in(staff, sim.channel, &[role]);
    message.message.content = "private response".into();
    let name = message.author.name.clone();
    on_message(sim.app.clone(), message).await.unwrap();
    let posts = sim.posts();
    assert_eq!(posts.len(), 1);
    assert_eq!(posts[0].content(), "private response");
    assert!(!posts[0].body.to_string().contains(&name));
    let foreign_thread = unique();
    sim.app
        .db
        .modmail_create(sim.guild + 1, user + 1, foreign_thread)
        .await
        .unwrap();
    sqlx::query("INSERT INTO modmail_snippets(guild_id,name,text) VALUES($1,'saved','private snippet')")
        .bind(sim.guild as i64)
        .execute(sim.app.db.pool())
        .await
        .unwrap();
    sim.channel = foreign_thread;
    sim.permissions = twilight_model::guild::Permissions::MANAGE_GUILD.bits();
    let denied = sim
        .command(
            staff,
            "modmail",
            json!([sub("snippet-use", vec![text_option("name", "saved")])]),
        )
        .await;
    assert!(denied.content().contains("open modmail thread"), "{denied}");
    assert!(
        sim.posts().is_empty(),
        "a snippet must never target another guild's thread"
    );
}

#[tokio::test]
async fn qotd_daily_claim_survives_failed_send_and_local_day_boundaries() {
    let Some(sim) = Sim::new("qotd_daily_claim_survives_failed_send_and_local_day_boundaries").await else {
        return;
    };
    let id = sim
        .app
        .db
        .suggest_qotd(sim.guild, unique(), "Daily question?")
        .await
        .unwrap()
        .unwrap();
    sim.app.db.decide_qotd(sim.guild, id, true).await.unwrap();
    sqlx::query("UPDATE qotd_settings SET channel_id=$2,hour=0,timezone='Pacific/Kiritimati' WHERE guild_id=$1")
        .bind(sim.guild as i64)
        .bind(sim.channel as i64)
        .execute(sim.app.db.pool())
        .await
        .unwrap();
    sim.block_channel(sim.channel);
    pinchora_commands::community_tick(&sim.app).await.unwrap();
    let claimed: Option<chrono::NaiveDate> =
        sqlx::query_scalar("SELECT last_posted_on FROM qotd_settings WHERE guild_id=$1")
            .bind(sim.guild as i64)
            .fetch_one(sim.app.db.pool())
            .await
            .unwrap();
    assert_eq!(
        claimed,
        Some(
            sim.app
                .now()
                .with_timezone(&chrono_tz::Pacific::Kiritimati)
                .date_naive()
        )
    );
    sim.mock.blocked.lock().unwrap().clear();
    let (a, b) = tokio::join!(
        pinchora_commands::community_tick(&sim.app),
        pinchora_commands::community_tick(&sim.app)
    );
    a.unwrap();
    b.unwrap();
    assert!(sim.posts().is_empty());
    sim.app.shift_clock(chrono::Duration::days(1));
    pinchora_commands::community_tick(&sim.app).await.unwrap();
    assert_eq!(sim.posts().len(), 1);
    sqlx::query("DELETE FROM qotd_settings WHERE guild_id=$1")
        .bind(sim.guild as i64)
        .execute(sim.app.db.pool())
        .await
        .unwrap();
}

pub(super) fn mock(mock: &Mock, method: &Method, parts: &[&str], body: &[u8]) -> Option<(StatusCode, String)> {
    if let (&Method::POST, [.., "channels", forum, "threads"]) = (method, parts) {
        let body: Value = serde_json::from_slice(body).unwrap();
        let thread = unique();
        mock.mod_calls.lock().unwrap().push(format!("forum {forum}"));
        mock.posts.lock().unwrap().push(Post {
            method: Method::POST,
            channel: thread,
            message: thread,
            body: body["message"].clone(),
            files: vec![],
        });
        return Some((
            StatusCode::OK,
            json!({"id":thread.to_string(),"type":11,"name":"Support","message":message_json(thread, thread)})
                .to_string(),
        ));
    }
    None
}

#[tokio::test]
async fn modmail_dm_spam_blocks_membership_and_attachment_delivery_without_intent() {
    let Some(mut sim) = Sim::with(
        "modmail_dm_spam_blocks_membership_and_attachment_delivery_without_intent",
        |a| a.message_content = false,
    )
    .await
    else {
        return;
    };
    // Creation must not reserve one connection and wait forever for a second one.
    Arc::get_mut(&mut sim.app).unwrap().db = Db::connect_waiting(
        &std::env::var("PINCHORA_TEST_DATABASE_URL").unwrap(),
        1,
        std::time::Duration::from_secs(10),
    )
    .await
    .unwrap();
    let user = unique();
    sim.app
        .set_user_lang(user, Some(pinchora_core::i18n::Lang::Tr))
        .await
        .unwrap();
    let forum = unique();
    sqlx::query("INSERT INTO modmail_settings(guild_id,enabled,forum_channel_id) VALUES($1,true,$2)")
        .bind(sim.guild as i64)
        .bind(forum as i64)
        .execute(sim.app.db.pool())
        .await
        .unwrap();
    // Selects must re-check membership even when a pending DM exists.
    sqlx::query("INSERT INTO modmail_pending(user_id,body) VALUES($1,'private')")
        .bind(user as i64)
        .execute(sim.app.db.pool())
        .await
        .unwrap();
    sim.mock.missing_members.lock().unwrap().push((sim.guild, user));
    let on = sim.command(user, "form", json!([sub("list", vec![])])).await;
    let guild = sim.guild.to_string();
    let refused = sim.component(user, "mml:g", &[&guild], &on).await;
    assert!(refused.content().contains("başlatamazsın"), "{refused}");
    assert!(sim.posts().is_empty());
    sim.mock.missing_members.lock().unwrap().clear();
    let mut dm = sim.chat(user, &[]);
    dm.message.guild_id = None;
    dm.message.member = None;
    dm.message.content.clear();
    dm.message.attachments = serde_json::from_value(json!([{"id":unique().to_string(),"filename":"note.txt","size":12,
        "url":"https://cdn.discordapp.com/attachments/1/2/note.txt","proxy_url":"https://media.discordapp.net/attachments/1/2/note.txt"}])).unwrap();
    let (a, b) = tokio::join!(
        on_message(sim.app.clone(), dm.clone()),
        on_message(sim.app.clone(), dm.clone())
    );
    a.unwrap();
    b.unwrap();
    // Existing other test guilds can cause a picker; select this guild if no thread opened directly.
    if sim.app.db.modmail_open(sim.guild, user).await.unwrap().is_none() {
        sim.component(user, "mml:g", &[&guild], &on).await;
    }
    let thread = sim.app.db.modmail_open(sim.guild, user).await.unwrap().unwrap();
    let posts = sim.posts();
    assert_eq!(
        sim.mock
            .mod_calls
            .lock()
            .unwrap()
            .iter()
            .filter(|s| *s == &format!("forum {forum}"))
            .count(),
        1
    );
    assert!(
        posts
            .iter()
            .any(|p| p.channel == thread.thread_id as u64 && p.content().contains("note.txt"))
    );
    assert!(posts.iter().any(|p| p.content().contains("Mesajın ekibe ulaştı")));
    sim.app
        .db
        .modmail_close(sim.guild, thread.thread_id as u64, user)
        .await
        .unwrap();
    for key in [
        format!("community:dm:{user}"),
        format!("community:mail-delivery:{user}"),
    ] {
        sim.app.cache.release_cooldown(&key).await.unwrap();
    }
    on_message(sim.app.clone(), dm.clone()).await.unwrap();
    sim.component(user, "mml:g", &[&guild], &on).await;
    assert!(
        sim.app.db.modmail_open(sim.guild, user).await.unwrap().is_none(),
        "closing must not bypass creation throttle"
    );
    sqlx::query("INSERT INTO modmail_blocks(guild_id,user_id) VALUES($1,$2)")
        .bind(sim.guild as i64)
        .bind(user as i64)
        .execute(sim.app.db.pool())
        .await
        .unwrap();
    sqlx::query("INSERT INTO modmail_pending(user_id,body) VALUES($1,'blocked') ON CONFLICT(user_id) DO UPDATE SET body='blocked',created_at=now()")
        .bind(user as i64).execute(sim.app.db.pool()).await.unwrap();
    let refused = sim.component(user, "mml:g", &[&guild], &on).await;
    assert!(refused.content().contains("başlatamazsın"), "{refused}");
    sqlx::query("DELETE FROM modmail_settings WHERE guild_id=$1")
        .bind(sim.guild as i64)
        .execute(sim.app.db.pool())
        .await
        .unwrap();
}

#[tokio::test]
async fn every_community_admin_action_refuses_unprivileged_members() {
    let Some(mut sim) = Sim::new("every_community_admin_action_refuses_unprivileged_members").await else {
        return;
    };
    for (section, actions) in [
        ("qotd", vec!["settings", "queue"]),
        ("counting", vec!["setup", "reset"]),
        ("forms", vec!["create", "post", "close"]),
        ("modmail", vec!["settings"]),
    ] {
        for action in actions {
            let reply = sim
                .command(unique(), "community", json!([group(section, sub(action, vec![]))]))
                .await;
            assert!(
                reply.ephemeral && reply.content().contains("Manage Server"),
                "{section} {action}: {reply}"
            );
        }
    }
}

#[tokio::test]
async fn community_backup_cannot_bypass_form_limits_and_labels_are_localized() {
    let Some(sim) = Sim::new("community_backup_cannot_bypass_form_limits_and_labels_are_localized").await else {
        return;
    };
    let mut backup = sim.app.db.export_config(sim.guild, Default::default()).await.unwrap();
    let form = json!({"id":"1","name":"Form","questions":["Why?"],"submit_channel_id":null,"review_roles":[],"accept_role_id":null,"dm_result":false,"open":true});
    backup.sections.insert("forms".into(), vec![form.clone()]);
    assert!(pinchora_commands::validate_backup(&backup, pinchora_core::i18n::Lang::En).is_ok());
    backup.sections.get_mut("forms").unwrap()[0]["name"] = json!("x".repeat(46));
    assert!(pinchora_commands::validate_backup(&backup, pinchora_core::i18n::Lang::En).is_err());
    backup.sections.insert(
        "forms".into(),
        (0..11)
            .map(|i| {
                let mut f = form.clone();
                f["id"] = json!((i + 1).to_string());
                f["name"] = json!(format!("Form {i}"));
                f
            })
            .collect(),
    );
    assert!(pinchora_commands::validate_backup(&backup, pinchora_core::i18n::Lang::Tr).is_err());
    for lang in [pinchora_core::i18n::Lang::En, pinchora_core::i18n::Lang::Tr] {
        for table in [
            "qotd_settings",
            "qotd_questions",
            "counting_channels",
            "forms",
            "modmail_settings",
            "modmail_snippets",
        ] {
            let key = format!("config_backup.section.{table}");
            assert_ne!(pinchora_core::t!(lang, &key), key);
        }
    }
}

#[tokio::test]
async fn qotd_dst_skipped_and_repeated_hours_deliver_once_per_local_day() {
    let Some(mut sim) = Sim::new("qotd_dst_skipped_and_repeated_hours_deliver_once_per_local_day").await else {
        return;
    };
    sim.channel = unique();
    let id = sim
        .app
        .db
        .suggest_qotd(sim.guild, unique(), "DST question?")
        .await
        .unwrap()
        .unwrap();
    sim.app.db.decide_qotd(sim.guild, id, true).await.unwrap();
    sqlx::query("UPDATE qotd_settings SET channel_id=$2,hour=2,timezone='America/New_York' WHERE guild_id=$1")
        .bind(sim.guild as i64)
        .bind(sim.channel as i64)
        .execute(sim.app.db.pool())
        .await
        .unwrap();
    let spring = chrono::DateTime::parse_from_rfc3339("2026-03-08T06:30:00Z")
        .unwrap()
        .with_timezone(&chrono::Utc);
    sim.app.shift_clock(spring - sim.app.now());
    pinchora_commands::community_tick(&sim.app).await.unwrap();
    assert!(!sim.posts().iter().any(|p| p.channel == sim.channel));
    sim.app.shift_clock(chrono::Duration::hours(1)); // 01:30 -> 03:30; 02:00 never occurs.
    pinchora_commands::community_tick(&sim.app).await.unwrap();
    assert_eq!(sim.posts().iter().filter(|p| p.channel == sim.channel).count(), 1);
    let autumn = chrono::DateTime::parse_from_rfc3339("2026-11-01T05:30:00Z")
        .unwrap()
        .with_timezone(&chrono::Utc);
    sim.app.shift_clock(autumn - sim.app.now());
    sqlx::query("UPDATE qotd_settings SET hour=1 WHERE guild_id=$1")
        .bind(sim.guild as i64)
        .execute(sim.app.db.pool())
        .await
        .unwrap();
    pinchora_commands::community_tick(&sim.app).await.unwrap();
    assert_eq!(sim.posts().iter().filter(|p| p.channel == sim.channel).count(), 1);
    sim.app.shift_clock(chrono::Duration::hours(1)); // 01:30 occurs again with the other UTC offset.
    pinchora_commands::community_tick(&sim.app).await.unwrap();
    assert!(!sim.posts().iter().any(|p| p.channel == sim.channel));
    sqlx::query("DELETE FROM qotd_settings WHERE guild_id=$1")
        .bind(sim.guild as i64)
        .execute(sim.app.db.pool())
        .await
        .unwrap();
}
