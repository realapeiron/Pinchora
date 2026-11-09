//! Community v2 personal tools through registered commands in both supported languages.
use crate::*;

#[tokio::test]
async fn checklist_and_highlights_are_private_and_bilingual() {
    let Some(mut sim) = Sim::new("checklist_and_highlights_are_private_and_bilingual").await else {
        return;
    };
    let member = unique();
    let other = unique();
    let added = sim
        .command(
            member,
            "checklist",
            json!([sub("add", vec![text_option("text", "Plan the event")])]),
        )
        .await;
    assert!(added.ephemeral && added.content().contains("Added"), "{added}");
    let listed = sim.command(member, "checklist", json!([sub("list", vec![])])).await;
    assert!(
        listed.ephemeral && listed.content().contains("Plan the event"),
        "{listed}"
    );
    let someone_else = sim.command(other, "checklist", json!([sub("list", vec![])])).await;
    assert!(!someone_else.content().contains("Plan the event"), "{someone_else}");

    sim.locale = "tr";
    let followed = sim
        .command(
            member,
            "highlight",
            json!([sub("add", vec![text_option("word", "mercan")])]),
        )
        .await;
    assert!(
        followed.ephemeral && followed.content().contains("mercan"),
        "{followed}"
    );
    let words = sim.command(member, "highlight", json!([sub("list", vec![])])).await;
    assert!(words.ephemeral && words.content().contains("mercan"), "{words}");
    let other_words = sim.command(other, "highlight", json!([sub("list", vec![])])).await;
    assert!(!other_words.content().contains("mercan"), "{other_words}");
}

#[tokio::test]
async fn highlights_refuse_without_message_content_intent() {
    let Some(mut sim) = Sim::with("highlights_refuse_without_message_content_intent", |app| {
        app.message_content = false;
    })
    .await
    else {
        return;
    };
    let reply = sim
        .command(
            unique(),
            "highlight",
            json!([sub("add", vec![text_option("word", "coral")])]),
        )
        .await;
    assert!(
        reply.ephemeral && reply.content().contains("Message Content"),
        "{reply}"
    );
}

#[tokio::test]
async fn highlights_check_member_overwrites_membership_cooldown_and_bots() {
    let Some(sim) = Sim::new("highlights_check_member_overwrites_membership_cooldown_and_bots").await else {
        return;
    };
    let speaker = unique();
    let denied = unique();
    let allowed = unique();
    let left = unique();
    sim.mock.everyone_role.store(true, Ordering::SeqCst);
    sim.mock.channels.lock().unwrap().insert(CHANNEL_ID,json!({"id":CHANNEL_ID.to_string(),"guild_id":sim.guild.to_string(),"type":0,"permission_overwrites":[{"id":denied.to_string(),"type":1,"allow":"0","deny":"1024"}]}));
    sim.mock.missing_members.lock().unwrap().push((sim.guild, left));
    for user in [speaker, denied, allowed, left] {
        sim.app
            .db
            .add_keyword_highlight(sim.guild, user, "hello")
            .await
            .unwrap();
    }
    on_message(sim.app.clone(), sim.chat(speaker, &[])).await.unwrap();
    let posts = sim.posts();
    let highlights: Vec<_> = posts
        .iter()
        .filter(|p| p.content().contains("A word you follow"))
        .collect();
    assert_eq!(highlights.len(), 1, "only the visible member receives a highlight");
    assert!(highlights[0].content().contains("https://discord.com/channels/"));
    assert_eq!(highlights[0].body["allowed_mentions"]["parse"], json!([]));
    sim.mock.posts.lock().unwrap().clear();
    on_message(sim.app.clone(), sim.chat(speaker, &[])).await.unwrap();
    assert!(
        !sim.posts().iter().any(|p| p.content().contains("A word you follow")),
        "cooldown"
    );
    let bot_follower = unique();
    sim.app
        .db
        .add_keyword_highlight(sim.guild, bot_follower, "hello")
        .await
        .unwrap();
    let mut message = sim.chat(speaker, &[]);
    message.author.bot = true;
    on_message(sim.app.clone(), message).await.unwrap();
    assert!(
        !sim.posts().iter().any(|p| p.content().contains("A word you follow")),
        "bots cannot trigger"
    );
}

#[tokio::test]
async fn highlights_fail_closed_for_threads_and_age_restricted_channels() {
    let Some(sim) = Sim::new("highlights_fail_closed_for_threads_and_age_restricted_channels").await else {
        return;
    };
    sim.mock.everyone_role.store(true, Ordering::SeqCst);
    let speaker = unique();
    sim.app
        .db
        .add_keyword_highlight(sim.guild, unique(), "hello")
        .await
        .unwrap();
    for channel in [
        json!({"id": CHANNEL_ID.to_string(), "guild_id": sim.guild.to_string(), "type": 12}),
        json!({"id": CHANNEL_ID.to_string(), "guild_id": sim.guild.to_string(), "type": 0, "nsfw": true}),
    ] {
        sim.mock.channels.lock().unwrap().insert(CHANNEL_ID, channel);
        on_message(sim.app.clone(), sim.chat(speaker, &[])).await.unwrap();
        assert!(!sim.posts().iter().any(|p| p.content().contains("A word you follow")));
    }
}

#[tokio::test]
async fn confession_manage_actions_require_permissions_and_reveal_is_audited() {
    let Some(mut sim) = Sim::new("confession_manage_actions_require_permissions_and_reveal_is_audited").await else {
        return;
    };
    let author = unique();
    let member = unique();
    let id = sim
        .app
        .db
        .submit_confession(sim.guild, author, "A hidden author")
        .await
        .unwrap()
        .unwrap();
    for action in ["review", "ban"] {
        let field = if action == "review" { "approve" } else { "banned" };
        let denied = sim
            .command(
                member,
                "confess",
                json!([sub(action, vec![int_option("id", id), bool_option(field, true)])]),
            )
            .await;
        assert!(
            denied.ephemeral && denied.content().contains("Manage Server"),
            "{denied}"
        );
    }
    let denied = sim
        .command(member, "confess", json!([sub("reveal", vec![int_option("id", id)])]))
        .await;
    assert!(denied.ephemeral && !denied.content().contains(&author.to_string()));
    sim.mock.owner.store(member, Ordering::SeqCst);
    let revealed = sim
        .command(member, "confess", json!([sub("reveal", vec![int_option("id", id)])]))
        .await;
    assert!(revealed.ephemeral && revealed.content().contains(&author.to_string()));
    let (count,): (i64,) =
        sqlx::query_as("SELECT count(*) FROM confession_reveals WHERE confession_id=$1 AND owner_id=$2")
            .bind(id)
            .bind(member as i64)
            .fetch_one(sim.app.db.pool())
            .await
            .unwrap();
    assert_eq!(count, 1);
    assert!(
        !sim.posts().iter().any(|p| p.content().contains(&author.to_string())),
        "no channel post exposes author"
    );
}

#[tokio::test]
async fn private_tools_never_send_private_data_through_prefix_replies() {
    let Some(mut sim) = Sim::new("private_tools_never_send_private_data_through_prefix_replies").await else {
        return;
    };
    let owner = unique();
    let author = unique();
    sim.mock.owner.store(owner, Ordering::SeqCst);
    sim.app
        .db
        .add_checklist_item(owner, "secret-checklist-text")
        .await
        .unwrap();
    sim.app
        .db
        .add_recurring_reminder(
            owner,
            "secret-reminder-text",
            60,
            sim.app.now() + chrono::Duration::hours(1),
            "en",
        )
        .await
        .unwrap();
    let id = sim
        .app
        .db
        .submit_confession(sim.guild, author, "secret-confession-text")
        .await
        .unwrap()
        .unwrap();
    for command in [
        "p!checklist list".to_string(),
        "p!remind repeat-list".into(),
        format!("p!confess reveal {id}"),
    ] {
        let (_, replies) = sim.text(owner, &[], &command, &[]).await;
        assert!(!replies.is_empty());
        for reply in replies {
            assert!(!reply.content().contains("secret-"));
            assert!(!reply.content().contains(&author.to_string()));
        }
    }
    let (count,): (i64,) = sqlx::query_as("SELECT count(*) FROM confession_reveals WHERE confession_id=$1")
        .bind(id)
        .fetch_one(sim.app.db.pool())
        .await
        .unwrap();
    assert_eq!(count, 0, "prefix rejection happens before reveal side effects");
}

#[tokio::test]
async fn recurring_reminders_recover_expired_leases_and_retry_failed_delivery() {
    let Some(sim) = Sim::new("recurring_reminders_recover_expired_leases_and_retry_failed_delivery").await else {
        return;
    };
    let user = unique();
    let id = sim
        .app
        .db
        .add_recurring_reminder(
            user,
            "@everyone <@&42> reminder",
            60,
            sim.app.now() - chrono::Duration::hours(1),
            "en",
        )
        .await
        .unwrap()
        .unwrap();
    sqlx::query("UPDATE personal_recurring_reminders SET lease_until=now()-interval '1 minute' WHERE id=$1")
        .bind(id)
        .execute(sim.app.db.pool())
        .await
        .unwrap();
    pinchora_commands::community_tools_tick(&sim.app).await.unwrap();
    let posts = sim.posts();
    let sent = posts
        .iter()
        .find(|p| p.content().contains("@everyone <@&42> reminder"))
        .unwrap();
    assert_eq!(sent.body["allowed_mentions"]["parse"], json!([]));
    let dm = sent.channel;
    sim.mock.posts.lock().unwrap().clear();
    pinchora_commands::community_tools_tick(&sim.app).await.unwrap();
    assert!(
        !sim.posts()
            .iter()
            .any(|p| p.content().contains("@everyone <@&42> reminder"))
    );
    sqlx::query("UPDATE personal_recurring_reminders SET next_at=now()-interval '1 minute' WHERE id=$1")
        .bind(id)
        .execute(sim.app.db.pool())
        .await
        .unwrap();
    sim.block_channel(dm);
    pinchora_commands::community_tools_tick(&sim.app).await.unwrap();
    let (attempts, soon): (i16, bool) = sqlx::query_as(
        "SELECT attempts,next_at<now()+interval '3 minutes' FROM personal_recurring_reminders WHERE id=$1",
    )
    .bind(id)
    .fetch_one(sim.app.db.pool())
    .await
    .unwrap();
    assert_eq!(
        (attempts, soon),
        (1, true),
        "failed delivery retries instead of skipping an interval"
    );
    // Leave no due global work for subsequent simulator binaries.
    sqlx::query("DELETE FROM personal_recurring_reminders WHERE id=$1")
        .bind(id)
        .execute(sim.app.db.pool())
        .await
        .unwrap();
}

#[tokio::test]
async fn scheduled_giveaway_activation_reenables_buttons_after_restart() {
    let Some(sim) = Sim::new("scheduled_giveaway_activation_reenables_buttons_after_restart").await else {
        return;
    };
    let host = unique();
    let message = unique();
    let pinchora_db::GiveawayOpen::Started(draw) = sim
        .app
        .db
        .create_giveaway(pinchora_db::NewGiveaway {
            guild_id: sim.guild,
            channel_id: sim.channel,
            host,
            prize: "Scheduled",
            shells: 0,
            winners: 1,
            required_role: None,
            ends_at: sim.app.now() + chrono::Duration::hours(1),
        })
        .await
        .unwrap()
    else {
        panic!("giveaway")
    };
    sim.app
        .db
        .set_giveaway_message(draw.id, sim.channel, message)
        .await
        .unwrap();
    sqlx::query("UPDATE giveaways SET status='scheduled',starts_at=$2 WHERE id=$1")
        .bind(draw.id)
        .bind(sim.app.now() - chrono::Duration::minutes(1))
        .execute(sim.app.db.pool())
        .await
        .unwrap();
    sim.delete_message(message);
    pinchora_commands::events_tick(&sim.app).await.unwrap();
    assert!(sim.app.db.giveaway(draw.id).await.unwrap().unwrap().is_running());
    let (announced,): (bool,) = sqlx::query_as("SELECT start_announced FROM giveaways WHERE id=$1")
        .bind(draw.id)
        .fetch_one(sim.app.db.pool())
        .await
        .unwrap();
    assert!(!announced, "failed activation is durable pending work");
    sim.mock.deleted.lock().unwrap().clear();
    pinchora_commands::events_tick(&sim.app).await.unwrap();
    let posts = sim.posts();
    let edit = posts
        .iter()
        .find(|p| p.message == message && p.method == Method::PATCH)
        .unwrap();
    assert_eq!(
        edit.body["components"][0]["components"][0]["custom_id"],
        format!("gw:e:{}", draw.id)
    );
    assert_ne!(edit.body["components"][0]["components"][0]["disabled"], true);
    let (announced,): (bool,) = sqlx::query_as("SELECT start_announced FROM giveaways WHERE id=$1")
        .bind(draw.id)
        .fetch_one(sim.app.db.pool())
        .await
        .unwrap();
    assert!(announced);
}

#[tokio::test]
async fn autopost_jobs_are_durable_deduplicated_and_retry_at_most_five_times() {
    let Some(sim) = Sim::new("autopost_jobs_are_durable_deduplicated_and_retry_at_most_five_times").await else {
        return;
    };
    sim.app
        .set_guild_lang(sim.guild, Some(pinchora_core::infra::i18n::Lang::Tr))
        .await
        .unwrap();
    let channel = unique();
    let user = unique();
    sqlx::query(
        "INSERT INTO community_auto_channels(guild_id,channel_id,auto_thread,auto_crosspost) VALUES($1,$2,true,true)",
    )
    .bind(sim.guild as i64)
    .bind(channel as i64)
    .execute(sim.app.db.pool())
    .await
    .unwrap();
    let message = sim.chat_in(user, channel, &[]);
    let id = message.id.get() as i64;
    on_message(sim.app.clone(), message.clone()).await.unwrap();
    on_message(sim.app.clone(), message.clone()).await.unwrap();
    let (count,): (i64,) = sqlx::query_as("SELECT count(*) FROM community_autopost_jobs WHERE message_id=$1")
        .bind(id)
        .fetch_one(sim.app.db.pool())
        .await
        .unwrap();
    assert_eq!(count, 1);
    sim.block_channel(channel);
    pinchora_commands::community_tools_tick(&sim.app).await.unwrap();
    sim.mock.blocked.lock().unwrap().clear();
    sqlx::query("UPDATE community_autopost_jobs SET next_at=now()-interval '1 minute',lease_until=now()-interval '1 minute' WHERE message_id=$1").bind(id).execute(sim.app.db.pool()).await.unwrap();
    pinchora_commands::community_tools_tick(&sim.app).await.unwrap();
    on_message(sim.app.clone(), message).await.unwrap();
    pinchora_commands::community_tools_tick(&sim.app).await.unwrap();
    assert_eq!(
        sim.mock.channels.lock().unwrap().get(&(id as u64)).unwrap()["name"],
        "Sohbet"
    );
    let actions = sim.mock.mod_calls.lock().unwrap().clone();
    assert_eq!(actions.iter().filter(|a| **a == format!("auto-thread {id}")).count(), 1);
    assert_eq!(
        actions.iter().filter(|a| **a == format!("auto-crosspost {id}")).count(),
        1
    );
    let message = sim.chat_in(user, channel, &[]);
    let failed = message.id.get() as i64;
    on_message(sim.app.clone(), message).await.unwrap();
    sim.block_channel(channel);
    for _ in 0..7 {
        sqlx::query("UPDATE community_autopost_jobs SET next_at=now()-interval '1 minute' WHERE message_id=$1")
            .bind(failed)
            .execute(sim.app.db.pool())
            .await
            .unwrap();
        pinchora_commands::community_tools_tick(&sim.app).await.unwrap();
    }
    let (attempts,): (i16,) = sqlx::query_as("SELECT attempts FROM community_autopost_jobs WHERE message_id=$1")
        .bind(failed)
        .fetch_one(sim.app.db.pool())
        .await
        .unwrap();
    assert_eq!(attempts, 5);
}

#[tokio::test]
async fn private_lists_fit_discord_even_at_the_quota() {
    let Some(mut sim) = Sim::new("private_lists_fit_discord_even_at_the_quota").await else {
        return;
    };
    let user = unique();
    for _ in 0..50 {
        sim.app.db.add_checklist_item(user, &"x".repeat(200)).await.unwrap();
    }
    for _ in 0..25 {
        sim.app
            .db
            .add_recurring_reminder(
                user,
                &"y".repeat(200),
                60,
                sim.app.now() + chrono::Duration::days(1),
                "en",
            )
            .await
            .unwrap();
    }
    for (root, leaf) in [("checklist", "list"), ("remind", "repeat-list")] {
        let reply = sim.command(user, root, json!([sub(leaf, vec![])])).await;
        assert!(reply.ephemeral && reply.content().chars().count() <= 2000);
    }
}

#[tokio::test]
async fn autopost_list_labels_follow_the_command_language() {
    let Some(mut sim) = Sim::new("autopost_list_labels_follow_the_command_language").await else {
        return;
    };
    let user = unique();
    sqlx::query(
        "INSERT INTO community_auto_channels(guild_id,channel_id,auto_thread,auto_crosspost) VALUES($1,$2,true,true)",
    )
    .bind(sim.guild as i64)
    .bind(sim.channel as i64)
    .execute(sim.app.db.pool())
    .await
    .unwrap();
    sim.permissions = MANAGE_GUILD;
    for (locale, word) in [("en", "Crosspost"), ("tr", "Duyuruyu paylaş")] {
        sim.locale = locale;
        let reply = sim.command(user, "autopost", json!([sub("list", vec![])])).await;
        assert!(reply.ephemeral && reply.content().contains(word), "{reply}");
    }
}
