//! Server stats: chat, joins and leaves counted in Redis, flushed to Postgres once and only once, the
//! `/stats` cards and their period buttons, and `/integrations stats` with its limit and permission checks.

use chrono::Utc;
use pinchora_commands::{stats_flush, stats_refresh_counters};
use pinchora_core::admin::server_stats::HourTally;
use pinchora_core::admin::server_stats::hour_of;
use pinchora_core::admin::server_stats::hour_start;
use pinchora_core::admin::server_stats::message_fields;

use crate::*;

/// Flushes until `guild` has `messages` stored (other tests flush the same Redis too, and a batch
/// another one took may still be on its way), and returns what is stored.
async fn flushed(sim: &Sim, messages: i64) -> (i64, i64, i64) {
    let from = Utc::now() - chrono::Duration::days(2);
    for _ in 0..100 {
        stats_flush(&sim.app).await.expect("flush");
        let hours = sim.app.db.guild_hours(sim.guild, from).await.unwrap();
        let sum = |f: fn(&pinchora_core::admin::server_stats::HourRow) -> i64| hours.iter().map(f).sum::<i64>();
        let found = (sum(|h| h.messages), sum(|h| h.joins), sum(|h| h.leaves));
        if found.0 >= messages {
            return found;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    panic!("the stats never reached Postgres");
}

#[tokio::test]
async fn chat_is_counted_once_and_drawn_on_the_stats_cards() {
    let _stats = fixture_lock::FixtureLock(14).lock().await;
    let Some(mut sim) = Sim::new("chat_is_counted_once_and_drawn_on_the_stats_cards").await else {
        return;
    };
    let (talker, quiet, newcomer) = (unique(), unique(), unique());
    sim.mock.everyone_role.store(true, Ordering::SeqCst);
    sim.channel_is(CHANNEL_ID, 0, None);
    for _ in 0..3 {
        sim.say(talker, &[]).await;
    }
    sim.say(quiet, &[]).await;
    sim.join(newcomer, "Newcomer", false, None).await;
    sim.leave(newcomer, "Newcomer", false).await;
    assert_eq!(flushed(&sim, 4).await, (4, 1, 1));
    // Nothing new was counted: another flush writes nothing more.
    stats_flush(&sim.app).await.unwrap();
    assert_eq!(flushed(&sim, 4).await, (4, 1, 1));
    let today = Utc::now().date_naive();
    let days = sim.app.db.member_days(sim.guild, talker, today).await.unwrap();
    assert_eq!(days.iter().map(|d| d.messages).sum::<i64>(), 3);
    assert_eq!(
        sim.app.db.member_place(sim.guild, quiet, today).await.unwrap(),
        Some((2, 2))
    );
    let top = sim.app.db.top_channels(sim.guild, today, 5).await.unwrap();
    assert_eq!((top[0].id as u64, top[0].messages), (CHANNEL_ID, 4));

    let server = sim.command(talker, "stats", json!([sub("server", vec![])])).await;
    assert!(server.ephemeral, "stats cards contain viewer-scoped activity");
    assert_eq!(server.files.len(), 1, "{server}");
    let card = image::load_from_memory(&server.files[0].1).expect("a png");
    assert_eq!(card.width(), 900);
    assert!(card.height() > 1_000, "the server card has every panel");
    assert!(server.button("30 days").disabled, "the period showing is greyed out");
    let quarter = sim.click(talker, &server, "90 days").await;
    assert_eq!(quarter.files.len(), 1, "{quarter}");
    assert!(quarter.button("90 days").disabled);
    assert!(!quarter.button("30 days").disabled);

    let me = sim
        .command(talker, "stats", json!([sub("me", vec![text_option("period", "7d")])]))
        .await;
    assert_eq!(me.files.len(), 1, "{me}");
    let small = image::load_from_memory(&me.files[0].1).unwrap();
    assert!(small.height() < card.height(), "a member's card is the short one");
    assert!(me.button("7 days").disabled);

    let channel = sim
        .command_resolved(
            talker,
            "stats",
            json!([sub("channel", vec![channel_option("channel", CHANNEL_ID)])]),
            resolved_channels(&[CHANNEL_ID]),
        )
        .await;
    assert_eq!(channel.files.len(), 1, "{channel}");
    let week = sim.click(quiet, &channel, "7 days").await;
    assert_eq!(week.files.len(), 1, "{week}");

    // A player whose Discord is in Turkish gets the card and its buttons in Turkish.
    sim.locale = "tr";
    let turkish = sim.command(talker, "stats", json!([sub("server", vec![])])).await;
    assert_eq!(turkish.files.len(), 1, "{turkish}");
    assert!(turkish.button("30 gün").disabled);
    stats_flush(&sim.app).await.unwrap();
    let commands: i64 = sim
        .app
        .db
        .guild_hours(sim.guild, Utc::now() - chrono::Duration::days(2))
        .await
        .unwrap()
        .iter()
        .map(|hour| hour.commands)
        .sum();
    assert_eq!(commands, 4, "the command dispatch hook still counts accepted commands");
}

#[tokio::test]
async fn channel_stats_and_period_buttons_recheck_the_viewer() {
    let _stats = fixture_lock::FixtureLock(14).lock().await;
    let Some(mut sim) = Sim::new("channel_stats_and_period_buttons_recheck_the_viewer").await else {
        return;
    };
    let viewer = unique();
    let channel = unique();
    sim.mock.everyone_role.store(true, Ordering::SeqCst);
    sim.channel_is(channel, 0, None);
    let options = json!([sub("channel", vec![channel_option("channel", channel)])]);
    let resolved = resolved_channels(&[channel]);
    let allowed = sim
        .command_resolved(viewer, "stats", options.clone(), resolved.clone())
        .await;
    assert!(allowed.ephemeral);
    assert_eq!(allowed.files.len(), 1, "{allowed}");

    let deny = json!([{
        "id": sim.guild.to_string(),
        "type": 0,
        "allow": "0",
        "deny": (1u64 << 10).to_string(),
    }]);
    sim.mock.channels.lock().unwrap().get_mut(&channel).unwrap()["permission_overwrites"] = deny;
    let refused = sim.command_resolved(viewer, "stats", options, resolved).await;
    assert!(refused.ephemeral);
    assert!(refused.files.is_empty(), "private channel card was rendered");
    let click = sim.click(viewer, &allowed, "7 days").await;
    assert!(click.ephemeral);
    assert!(click.files.is_empty(), "button revealed a private channel");
}

#[tokio::test]
async fn a_batch_cut_short_is_written_once() {
    let _stats = fixture_lock::FixtureLock(14).lock().await;
    let Some(sim) = Sim::new("a_batch_cut_short_is_written_once").await else {
        return;
    };
    let (guild, user) = (unique(), unique());
    let hour = hour_of(Utc::now());
    let cache = &sim.app.cache;
    cache.stats_add(guild, hour, &message_fields(7, user)).await.unwrap();
    cache.stats_add(guild, hour, &message_fields(7, user)).await.unwrap();
    // A flush seals the hash and writes it, then the pod dies before Redis forgets the batch.
    cache.stats_seal(100_000).await.unwrap();
    let batch = loop {
        let pending = cache.stats_pending(100_000).await.unwrap();
        if let Some(batch) = pending.into_iter().find(|b| b.guild == guild) {
            break batch;
        }
        // Another test's flush may have taken it already; then it is in Postgres.
        let stored = sim.app.db.guild_hours(guild, hour_start(hour)).await.unwrap();
        if !stored.is_empty() {
            return;
        }
        cache.stats_seal(100_000).await.unwrap();
    };
    let tally = HourTally::from_fields(&batch.fields);
    assert_eq!(tally.messages, 2);
    assert!(
        sim.app
            .db
            .write_stats_batch(&batch.key, guild, hour_start(hour), &tally)
            .await
            .unwrap()
    );
    // The next flush finds the batch again and writes nothing twice.
    stats_flush(&sim.app).await.unwrap();
    assert!(
        !sim.app
            .db
            .write_stats_batch(&batch.key, guild, hour_start(hour), &tally)
            .await
            .unwrap()
    );
    let stored = sim.app.db.guild_hours(guild, hour_start(hour)).await.unwrap();
    assert_eq!(stored.iter().map(|h| h.messages).sum::<i64>(), 2);
    let days = sim
        .app
        .db
        .member_days(guild, user, Utc::now().date_naive())
        .await
        .unwrap();
    assert_eq!(days.iter().map(|d| d.messages).sum::<i64>(), 2);
}

/// `data.resolved` for one voice channel.
fn resolved_voice(id: u64) -> Value {
    json!({
        "channels": {
            id.to_string(): { "id": id.to_string(), "type": 2, "name": "counter", "permissions": "0" },
        },
    })
}

fn add_counter(id: u64, kind: &str) -> Value {
    json!([sub(
        "add",
        vec![channel_option("channel", id), text_option("kind", kind)]
    )])
}

#[tokio::test]
async fn counter_channels_are_added_listed_limited_and_removed() {
    let _stats = fixture_lock::FixtureLock(14).lock().await;
    let Some(mut sim) = Sim::new("counter_channels_are_added_listed_limited_and_removed").await else {
        return;
    };
    let manager = unique();
    sim.permissions = MANAGE_GUILD;
    let voice = unique();
    sim.channel_is(voice, 2, None);

    let added = sim
        .command_resolved(
            manager,
            "counters",
            add_counter(voice, "members"),
            resolved_voice(voice),
        )
        .await;
    assert!(added.ephemeral);
    assert!(added.content().contains("Members: 42"), "{added}");
    assert!(added.content().contains("1/10"), "{added}");

    let listed = sim.command(manager, "counters", json!([sub("list", vec![])])).await;
    assert!(listed.content().contains("(1/10 used)"), "{listed}");
    assert!(listed.content().contains(&format!("<#{voice}>")), "{listed}");

    // A role counter needs its role.
    let other = unique();
    sim.channel_is(other, 2, None);
    let no_role = sim
        .command_resolved(
            manager,
            "counters",
            add_counter(other, "role_members"),
            resolved_voice(other),
        )
        .await;
    assert!(no_role.content().contains("role:"), "{no_role}");

    // Ten at most; the one already there can still be changed.
    let mut ghost = 0;
    for _ in 0..9 {
        let id = unique();
        ghost = id;
        sim.channel_is(id, 2, None);
        sim.command_resolved(manager, "counters", add_counter(id, "boosts"), resolved_voice(id))
            .await;
    }
    let full = sim
        .command_resolved(manager, "counters", add_counter(other, "bots"), resolved_voice(other))
        .await;
    assert!(full.content().contains("10 counter channels"), "{full}");
    let renamed = json!([sub(
        "add",
        vec![
            channel_option("channel", voice),
            text_option("kind", "members"),
            text_option("template", "People: {count}"),
        ]
    )]);
    let changed = sim
        .command_resolved(manager, "counters", renamed, resolved_voice(voice))
        .await;
    assert!(changed.content().contains("People: 42"), "{changed}");
    assert!(changed.content().contains("10/10"), "{changed}");
    // A deleted channel frees a slot even when its number and name would not change.
    sim.mock.channels.lock().unwrap().remove(&ghost);
    stats_refresh_counters(&sim.app, sim.guild).await.unwrap();
    assert_eq!(sim.app.db.counters(sim.guild).await.unwrap().len(), 9);
    // A template without the number is refused.
    let bare = json!([sub(
        "add",
        vec![
            channel_option("channel", voice),
            text_option("kind", "members"),
            text_option("template", "People"),
        ]
    )]);
    let refused = sim
        .command_resolved(manager, "counters", bare, resolved_voice(voice))
        .await;
    assert!(refused.content().contains("{count}"), "{refused}");

    let removed = sim
        .command_resolved(
            manager,
            "counters",
            json!([sub("remove", vec![channel_option("channel", voice)])]),
            resolved_voice(voice),
        )
        .await;
    assert!(removed.content().contains("no longer a counter"), "{removed}");
    let again = sim
        .command_resolved(
            manager,
            "counters",
            json!([sub("remove", vec![channel_option("channel", voice)])]),
            resolved_voice(voice),
        )
        .await;
    assert!(again.content().contains("isn't a counter"), "{again}");

    // A bot that cannot see the channel says so instead of saving a counter that never changes.
    sim.own_server(unique());
    sim.app.forget_permission_facts(sim.guild).await;
    let blind = unique();
    sim.channel_is(blind, 2, None);
    let refused = sim
        .command_resolved(
            manager,
            "counters",
            add_counter(blind, "members"),
            resolved_voice(blind),
        )
        .await;
    assert!(refused.content().contains("**View Channel**"), "{refused}");

    // Members without Manage Server are refused, whatever Discord showed them.
    sim.permissions = 0;
    let member = sim.command(unique(), "counters", json!([sub("list", vec![])])).await;
    assert!(member.content().contains("Manage Server"), "{member}");
}
