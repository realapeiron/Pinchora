//! What a switched-off module still did, now fixed: giveaway and poll timers, the chat XP minute, quest
//! progress from chat, and level-ups posted into a channel the leveling rule leaves out.

use pinchora_game::quests;
use pinchora_game::quests::QuestKind;

use crate::*;

/// Runs the giveaway and poll ticker once.
async fn tick(sim: &Sim) {
    let app = sim.app.clone();
    pinchora_commands::events_tick(&app).await.expect("tick");
}

async fn open_polls(sim: &Sim) -> i64 {
    sqlx::query_scalar("SELECT count(*) FROM polls WHERE guild_id = $1 AND status = 'open'")
        .bind(sim.guild as i64)
        .fetch_one(sim.app.db.pool())
        .await
        .expect("count polls")
}

#[tokio::test]
async fn giveaways_and_polls_wait_while_their_module_is_off() {
    let Some(mut sim) = Sim::new("giveaways_and_polls_wait_while_their_module_is_off").await else {
        return;
    };
    let _turn = EVENTS.lock().await;
    sim.channel = unique();
    let manager = unique();
    sim.permissions = MANAGE_GUILD;
    let started = giveaway_cmd(
        &mut sim,
        manager,
        "start",
        giveaway_start("1h", 1, "Golden shell", None),
    )
    .await;
    assert!(started.content().contains("is live in"), "{}", started.content());
    let opened = poll_cmd(&mut sim, manager, "Pizza or tacos?", "Pizza | Tacos", Some("1h")).await;
    assert!(opened.content().contains("is open in"), "{}", opened.content());
    sqlx::query("UPDATE giveaways SET ends_at = now() - interval '1 minute' WHERE guild_id = $1")
        .bind(sim.guild as i64)
        .execute(sim.app.db.pool())
        .await
        .expect("make the giveaway due");
    sqlx::query("UPDATE polls SET ends_at = now() - interval '1 minute' WHERE guild_id = $1")
        .bind(sim.guild as i64)
        .execute(sim.app.db.pool())
        .await
        .expect("make the poll due");

    // Off, the timers wait: nothing is drawn, closed or posted, however many ticks pass.
    modules(&mut sim, manager, toggle("giveaways", false), &[]).await;
    modules(&mut sim, manager, toggle("polls", false), &[]).await;
    sim.posts();
    tick(&sim).await;
    tick(&sim).await;
    assert_eq!(running_giveaways(&sim).await, 1, "the giveaway is paused, not ended");
    assert_eq!(open_polls(&sim).await, 1, "the poll is paused, not closed");
    // Paused ones are not even scanned, so they cannot crowd out the due ones of other servers.
    let soon = sim.app.now() + chrono::Duration::hours(2);
    let mine = |due: Vec<pinchora_db::Due>| due.iter().filter(|d| d.guild_id == sim.guild as i64).count();
    assert_eq!(mine(sim.app.db.due_giveaways(soon, 100_000).await.unwrap()), 0);
    assert_eq!(mine(sim.app.db.due_polls(soon, 100_000).await.unwrap()), 0);

    // One at a time: switching only polls back on closes only the poll.
    modules(&mut sim, manager, toggle("polls", true), &[]).await;
    tick(&sim).await;
    assert_eq!((running_giveaways(&sim).await, open_polls(&sim).await), (1, 0));
    // And a module kept to other channels pauses the ones in the channel it is off in.
    modules(&mut sim, manager, toggle("giveaways", true), &[]).await;
    let elsewhere = unique();
    modules(
        &mut sim,
        manager,
        limit("giveaways", "only", elsewhere, false),
        &[elsewhere],
    )
    .await;
    tick(&sim).await;
    assert_eq!(running_giveaways(&sim).await, 1, "off in the channel it runs in");
    let mine = |due: Vec<pinchora_db::Due>| due.iter().filter(|d| d.guild_id == sim.guild as i64).count();
    assert_eq!(
        mine(sim.app.db.due_giveaways(soon, 100_000).await.unwrap()),
        1,
        "an `only` rule may reach the channel through its category, so the ticker decides"
    );
    modules(
        &mut sim,
        manager,
        limit("giveaways", "only", elsewhere, true),
        &[elsewhere],
    )
    .await;
    tick(&sim).await;
    assert_eq!(running_giveaways(&sim).await, 0, "back on, the overdue giveaway ends");
}

#[tokio::test]
async fn the_chat_xp_minute_is_not_spent_where_nothing_counts() {
    let Some(mut sim) = Sim::new("the_chat_xp_minute_is_not_spent_where_nothing_counts").await else {
        return;
    };
    let (manager, member) = (unique(), unique());
    let (off, on) = (unique(), unique());
    modules(&mut sim, manager, limit("leveling", "except", off, false), &[off]).await;
    modules(&mut sim, manager, limit("points", "except", off, false), &[off]).await;

    sim.say_in(member, off, &[]).await;
    assert_eq!(
        (sim.server_xp(member).await, sim.points(member).await),
        (0, 0),
        "nothing counts there"
    );
    sim.say_in(member, on, &[]).await;
    assert!(
        sim.server_xp(member).await > 0,
        "the minute was not spent in the channel where nothing counted"
    );
    // Where something counts, the minute is spent as ever.
    let before = sim.server_xp(member).await;
    sim.say_in(member, on, &[]).await;
    assert_eq!(sim.server_xp(member).await, before);
}

/// A member whose quests today include chatting.
fn chatter() -> u64 {
    let day = today();
    std::iter::repeat_with(unique)
        .find(|user| quests::for_day(*user, day).iter().any(|q| q.kind == QuestKind::Chat))
        .expect("a member with a chat quest")
}

#[tokio::test]
async fn chat_counts_towards_quests_only_while_quests_are_on() {
    let Some(mut sim) = Sim::new("chat_counts_towards_quests_only_while_quests_are_on").await else {
        return;
    };
    let manager = unique();
    let (a, b) = (chatter(), chatter());
    let key = |user: u64| format!("quest:{}:{user}:{}", today(), QuestKind::Chat.key());

    modules(&mut sim, manager, toggle("quests", false), &[]).await;
    sim.say(a, &[]).await;
    assert!(sim.server_xp(a).await > 0, "XP is another module");
    assert_eq!(
        sim.app.cache.counter(&key(a)).await.unwrap(),
        0,
        "quests are off: no progress"
    );

    modules(&mut sim, manager, toggle("quests", true), &[]).await;
    sim.say(b, &[]).await;
    assert_eq!(sim.app.cache.counter(&key(b)).await.unwrap(), 1);
}

#[tokio::test]
async fn level_ups_are_not_posted_where_the_leveling_rule_leaves_a_channel_out() {
    let Some(mut sim) = Sim::new("level_ups_are_not_posted_where_the_leveling_rule_leaves_a_channel_out").await else {
        return;
    };
    let (admin, manager, first, second) = (unique(), unique(), unique(), unique());
    let (announce, chatty) = (unique(), unique());
    sim.permissions = MANAGE_GUILD;
    sim.command_resolved(
        admin,
        "config",
        json!([sub("leveling", vec![channel_option("channel", announce)])]),
        resolved_channels(&[announce]),
    )
    .await;
    let reached = |posts: &[Post], channel: u64, user: u64| {
        in_channel(posts, channel)
            .iter()
            .any(|p| p.content().contains(&format!("<@{user}> reached **level 1**")))
    };

    sim.app.db.add_message_xp(sim.guild, first, 99, 0).await.unwrap();
    sim.say_in(first, chatty, &[]).await;
    assert!(reached(&sim.posts(), announce, first), "the level-up channel gets it");

    modules(
        &mut sim,
        manager,
        limit("leveling", "except", announce, false),
        &[announce],
    )
    .await;
    sim.app.db.add_message_xp(sim.guild, second, 99, 0).await.unwrap();
    sim.say_in(second, chatty, &[]).await;
    let posts = sim.posts();
    assert!(
        reached(&posts, chatty, second) && !reached(&posts, announce, second),
        "leveling is off in the level-up channel, so the level-up is told where it was earned"
    );
}
