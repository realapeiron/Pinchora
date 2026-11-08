//! The collection games, clicked through: chat-driven drops, Critter Drop, Message in a Bottle,
//! Beachcomb, the Tide Pool Dex and the Crab Pot.

use chrono::Duration;
use pinchora_commands::{collection_tick, post_bottle, post_critter_drop};
use pinchora_core::items::{is_soulbound, soulbound};
use pinchora_game::{
    Rarity,
    collection::{Collection, Kind, exchange_cost, exchange_pick},
};

use super::*;

/// Drops are closed by whoever ticks, for every drop in the database, so the tests that make drops and the
/// ones that move the clock on take turns.
pub(super) static COLLECTION: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

#[tokio::test]
async fn collection_depth_dex_panels_work_in_english_and_turkish() {
    let Some(mut sim) = Sim::new("collection_depth_dex_panels_work_in_english_and_turkish").await else {
        return;
    };
    for (language, wish_label, roll_label, claim_label) in [
        (pinchora_core::i18n::Lang::En, "Wishlist", "Hourly roll", "Claim card"),
        (
            pinchora_core::i18n::Lang::Tr,
            "Dilek listesi",
            "Saatlik atış",
            "Kartı al",
        ),
    ] {
        let user = unique();
        sim.app.set_user_lang(user, Some(language)).await.unwrap();
        let dex = sim.command(user, "dex", json!([])).await;
        assert!(dex.ephemeral && dex.files.iter().any(|f| f.0 == "dex.png"));
        let wishes = sim.click(user, &dex, wish_label).await;
        let entry = &Collection::get().critters[0].id;
        sim.pick(user, &wishes, "bc:wp:", entry).await;
        assert!(
            sim.app
                .db
                .wishlist(user)
                .await
                .unwrap()
                .iter()
                .any(|w| w.entry_id == *entry && w.ping_on_spawn)
        );
        let rolled = sim.click(user, &dex, roll_label).await;
        assert!(rolled.description().len() > 10, "{rolled}");
        let claimed = sim.click(user, &rolled, claim_label).await;
        assert!(claimed.description().len() > 5, "{claimed}");
        assert_eq!(sim.app.db.dex(user).await.unwrap().len(), 1);
    }
}

#[tokio::test]
async fn word_drop_needs_the_typed_word_even_if_a_claim_id_is_forged() {
    let Some(mut sim) = Sim::new("word_drop_needs_the_typed_word_even_if_a_claim_id_is_forged").await else {
        return;
    };
    let _turn = COLLECTION.lock().await;
    sim.channel = unique();
    assert!(post_critter_drop(&sim.app, sim.guild, sim.channel).await.unwrap());
    let posts = sim.posts();
    let post = only(&posts, sim.channel, Method::POST);
    let shown = reply_of(post);
    let (id,): (i64,) =
        sqlx::query_as("SELECT id FROM collection_drops WHERE channel_id = $1 ORDER BY id DESC LIMIT 1")
            .bind(sim.channel as i64)
            .fetch_one(sim.app.db.pool())
            .await
            .unwrap();
    sim.app.db.set_drop_mode(id, "word", Some("coral")).await.unwrap();
    let user = unique();
    let refused = sim.component(user, &format!("cd:g:{id}:0"), &[], &shown).await;
    assert!(refused.content().contains("does not match"), "{refused}");
    let form = sim.component(user, &format!("cd:w:{id}:0"), &[], &shown).await;
    assert!(form.modal.is_some(), "{form}");
    let data = json!({
        "custom_id": form.modal.as_ref().unwrap().0,
        "components": [{"type":18,"id":1,"component":{"type":4,"id":2,"custom_id":"answer","value":"coral"}}]
    });
    let claimed = sim.run("submit critter word".into(), user, 5, data, Some(&shown)).await;
    assert!(
        claimed.description().contains("taken by") || claimed.content().contains("taken"),
        "{claimed}"
    );
    assert_eq!(sim.app.db.dex(user).await.unwrap().len(), 1);
}

/// Turns chat drops on for the sim's server, here or in `channel`.
async fn enable_drops(sim: &Sim, channel: Option<u64>) {
    sim.app
        .db
        .set_collection_settings(sim.guild, Some(true), Some(channel))
        .await
        .expect("settings");
}

/// Puts entries in a player's dex without playing for them.
async fn give(sim: &Sim, user: u64, entries: &[&str], caught: i32) {
    for id in entries {
        sqlx::query("INSERT INTO dex_entries (user_id, entry_id, caught) VALUES ($1, $2, $3) ON CONFLICT DO NOTHING")
            .bind(user as i64)
            .bind(*id)
            .bind(caught)
            .execute(sim.app.db.pool())
            .await
            .expect("dex row");
    }
}

async fn give_fragments(sim: &Sim, user: u64, fragments: i64) {
    sqlx::query(
        "INSERT INTO collection_players (user_id, fragments) VALUES ($1, $2)
         ON CONFLICT (user_id) DO UPDATE SET fragments = EXCLUDED.fragments",
    )
    .bind(user as i64)
    .bind(fragments)
    .execute(sim.app.db.pool())
    .await
    .expect("fragments");
}

async fn config_show(sim: &mut Sim, admin: u64) -> Reply {
    sim.command(admin, "config", json!([sub("show", vec![])])).await
}

/// The shells the first of `entry` pays.
fn discovery(entry: &str) -> i64 {
    Collection::get().entry(entry).expect("an entry").pay().new_shells
}

#[tokio::test]
async fn critters_can_be_switched_on_per_server_and_per_channel() {
    let Some(mut sim) = Sim::new("critters_can_be_switched_on_per_server_and_per_channel").await else {
        return;
    };
    let admin = unique();
    let channel = unique();
    sim.permissions = MANAGE_GUILD;
    let shown = config_show(&mut sim, admin).await;
    assert!(
        shown.field("Critter drops").contains("**off**"),
        "{}",
        shown.field("Critter drops")
    );

    let both = sim
        .command_resolved(
            admin,
            "config",
            json!([sub(
                "critters",
                vec![channel_option("channel", channel), bool_option("anywhere", true)]
            )]),
            resolved_channels(&[channel]),
        )
        .await;
    assert!(both.content().contains("not both"), "{}", both.content());
    assert!(!sim.app.db.collection_settings(sim.guild).await.unwrap().enabled);

    let on = sim
        .command_resolved(
            admin,
            "config",
            json!([sub(
                "critters",
                vec![bool_option("enabled", true), channel_option("channel", channel)]
            )]),
            resolved_channels(&[channel]),
        )
        .await;
    assert!(on.ephemeral);
    assert!(
        on.content().contains("**on**") && on.content().contains(&format!("<#{channel}>")),
        "{}",
        on.content()
    );
    let shown = config_show(&mut sim, admin).await;
    assert!(shown.field("Critter drops").contains(&format!("<#{channel}>")));

    let anywhere = sim
        .command(
            admin,
            "config",
            json!([sub("critters", vec![bool_option("anywhere", true)])]),
        )
        .await;
    assert!(
        anywhere.content().contains("whichever channel is busy"),
        "{}",
        anywhere.content()
    );
    let off = sim
        .command(
            admin,
            "config",
            json!([sub("critters", vec![bool_option("enabled", false)])]),
        )
        .await;
    assert!(off.content().contains("**off**"), "{}", off.content());
    // Switching it off left the channel setting as it was: anywhere.
    let settings = sim.app.db.collection_settings(sim.guild).await.unwrap();
    assert!(!settings.enabled && settings.channel_id.is_none());
}

#[tokio::test]
async fn the_collection_commands_fit_discords_limits() {
    let definitions = pinchora_commands::command_definitions();
    for name in ["dex", "beachcomb", "crabpot", "critterdrop"] {
        let command = serde_json::to_value(definitions.iter().find(|c| c.name == name).expect(name)).unwrap();
        assert!(command["description"].as_str().unwrap().len() <= 100, "{name}");
        for option in command["options"].as_array().into_iter().flatten() {
            assert!(option["description"].as_str().unwrap().len() <= 100, "{name} {option}");
        }
    }
    let critters = command_tree::definition("progression economy critters");
    let names: Vec<&str> = critters["options"]
        .as_array()
        .unwrap()
        .iter()
        .map(|o| o["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, ["enabled", "channel", "anywhere"]);
    assert!(definitions.len() <= 100);
}

#[tokio::test]
async fn chat_makes_things_wash_up_but_only_with_enough_talk_from_enough_people() {
    let Some(mut sim) = Sim::new("chat_makes_things_wash_up_but_only_with_enough_talk_from_enough_people").await else {
        return;
    };
    let channel = unique();
    sim.channel = channel;

    // Switched off, however much is said.
    for _ in 0..30 {
        sim.say_in(unique(), channel, &[]).await;
    }
    assert!(sim.posts().is_empty(), "off by default");

    // One chatterbox counts once in a while, so they cannot fill the counter alone.
    sim.guild = unique();
    enable_drops(&sim, None).await;
    let chatterbox = unique();
    for _ in 0..40 {
        sim.say_in(chatterbox, channel, &[]).await;
    }
    assert!(sim.posts().is_empty(), "one person is not a conversation");

    // Twenty-four people, nothing yet; the twenty-fifth message tips it.
    sim.guild = unique();
    enable_drops(&sim, None).await;
    for _ in 0..24 {
        sim.say_in(unique(), channel, &[]).await;
    }
    assert!(sim.posts().is_empty(), "24 messages are not enough");
    sim.say_in(unique(), channel, &[]).await;
    let posts = sim.posts();
    let post = only(&posts, channel, Method::POST);
    assert!(pings_nobody(post));
    let file = &post.files[0].0;
    assert!(file == "drop.png" || file == "bottle.png", "{file}");
    assert!(
        post.title().contains("washed") && post.title().len() > 8,
        "{}",
        post.title()
    );
    // The server's cooldown holds the next one back, however busy it gets.
    for _ in 0..60 {
        sim.say_in(unique(), channel, &[]).await;
    }
    assert!(sim.posts().is_empty(), "one spawn per cooldown");

    // With a channel set, only chat there counts and the drop lands there.
    sim.guild = unique();
    let (elsewhere, here) = (unique(), unique());
    enable_drops(&sim, Some(here)).await;
    for _ in 0..30 {
        sim.say_in(unique(), elsewhere, &[]).await;
    }
    assert!(sim.posts().is_empty(), "chat in other channels does not count");
    for _ in 0..25 {
        sim.say_in(unique(), here, &[]).await;
    }
    let posts = sim.posts();
    only(&posts, here, Method::POST);

    // A channel the bot may not post in is no error: the chat goes on and XP is still given.
    sim.guild = unique();
    let walled = unique();
    sim.block_channel(walled);
    enable_drops(&sim, None).await;
    for _ in 0..25 {
        sim.say_in(unique(), walled, &[]).await;
    }
    assert!(sim.posts().is_empty());
    let (open,): (i64,) = sqlx::query_as("SELECT count(*) FROM collection_drops WHERE channel_id = $1")
        .bind(walled as i64)
        .fetch_one(sim.app.db.pool())
        .await
        .unwrap();
    assert_eq!(open, 0, "a drop that could not be posted is taken back");

    // And it cost the server nothing: the next 25 messages, in a channel that works, spawn at once instead
    // of waiting out the cooldown.
    let fine = unique();
    for _ in 0..25 {
        sim.say_in(unique(), fine, &[]).await;
    }
    let posts = sim.posts();
    only(&posts, fine, Method::POST);
}

#[tokio::test]
async fn critter_drop_goes_to_the_quickest_and_each_gets_one() {
    let Some(mut sim) = Sim::new("critter_drop_goes_to_the_quickest_and_each_gets_one").await else {
        return;
    };
    let _turn = COLLECTION.lock().await;
    sim.channel = unique();
    let channel = sim.channel;
    let (a, b, c) = (unique(), unique(), unique());

    assert!(post_critter_drop(&sim.app, sim.guild, channel).await.expect("posts"));
    let posts = sim.posts();
    let post = only(&posts, channel, Method::POST);
    assert_eq!(post.title(), "🦀 Critters washed up!");
    assert_eq!(post.files.len(), 1);
    assert_eq!(post.files[0].0, "drop.png");
    assert!(pings_nobody(post));
    let message = post.message;
    let drop = reply_of(post);
    for label in ["1", "2", "3"] {
        assert!(!drop.button(label).disabled);
    }
    assert_eq!(drop.rows(), 1);

    // Ana takes card 1: everyone sees it taken, she alone gets the details.
    let ana = sim.click(a, &drop, "1").await;
    assert!(
        ana.description().contains(&format!("taken by <@{a}>")),
        "{}",
        ana.description()
    );
    assert!(ana.button("1").disabled && !ana.button("2").disabled);
    assert!(ana.pings_nobody());
    let details = &ana.followups[0];
    assert!(details.ephemeral);
    assert_eq!(details.files[0].0, "critter.png");
    assert!(details.description().contains("You took"), "{}", details.description());
    assert!(
        details.description().contains("new for your dex"),
        "{}",
        details.description()
    );
    let ana_dex = sim.app.db.dex(a).await.unwrap();
    assert_eq!(ana_dex.len(), 1);
    assert_eq!(shells(&sim, a).await, discovery(&ana_dex[0].entry_id));

    // Too slow for the same card, and one card each.
    let slow = sim.click(b, &drop, "1").await;
    assert!(
        slow.ephemeral && slow.content().contains("Too slow"),
        "{}",
        slow.content()
    );
    assert_eq!(shells(&sim, b).await, 0);
    let greedy = sim.click(a, &drop, "2").await;
    assert!(
        greedy.ephemeral && greedy.content().contains("already took card **1**"),
        "{}",
        greedy.content()
    );
    let bea = sim.click(b, &drop, "2").await;
    assert!(bea.description().contains(&format!("taken by <@{b}>")));
    assert_eq!(bea.followups.len(), 1);

    // Counted in the stats.
    let stats = sim.command(a, "play", json!([sub("stats", vec![])])).await;
    assert!(
        stats.embeds[0]
            .fields
            .iter()
            .any(|f| f.name.contains("Critter drop") && f.value.contains("**1** won of **1**")),
        "{:?}",
        stats.embeds[0].fields
    );

    // Time runs out: the card nobody took drifts away, the buttons go, and late clicks are told.
    sim.app.shift_clock(Duration::seconds(95));
    let late = sim.click(c, &drop, "3").await;
    assert!(
        late.ephemeral && late.content().contains("drifted away"),
        "{}",
        late.content()
    );
    assert_eq!(sim.app.db.dex(c).await.unwrap().len(), 0);
    collection_tick(&sim.app).await.expect("tick");
    let posts = sim.posts();
    let over = posts
        .iter()
        .find(|p| p.message == message && p.method == Method::PATCH)
        .expect("the drop's message is edited");
    assert_eq!(over.title(), "🦀 Critter drop over");
    let text = over.embed()["description"].as_str().unwrap();
    assert!(
        text.contains("drifted away") && text.contains(&format!("taken by <@{a}>")),
        "{text}"
    );
    let after = reply_of(over);
    assert!(["1", "2", "3"].iter().all(|l| after.button(l).disabled));
    // Closing it twice does nothing.
    collection_tick(&sim.app).await.expect("tick");
    assert!(sim.posts().iter().all(|p| p.message != message));
}

#[tokio::test]
async fn a_bottle_is_opened_by_the_first_who_clicks_and_the_message_is_read_aloud() {
    let Some(mut sim) = Sim::new("a_bottle_is_opened_by_the_first_who_clicks_and_the_message_is_read_aloud").await
    else {
        return;
    };
    let _turn = COLLECTION.lock().await;
    sim.channel = unique();
    let channel = sim.channel;
    let (a, b) = (unique(), unique());

    assert!(post_bottle(&sim.app, sim.guild, channel).await.expect("posts"));
    let posts = sim.posts();
    let post = only(&posts, channel, Method::POST);
    assert_eq!(post.title(), "🍾 A bottle washed ashore!");
    assert_eq!(post.files[0].0, "bottle.png");
    let bottle = reply_of(post);
    assert!(!bottle.button("Open the bottle").disabled);

    let opened = sim.click(a, &bottle, "Open the bottle").await;
    assert_eq!(opened.title(), "🍾 The bottle was opened");
    let text = opened.description();
    assert!(text.contains(&format!("Opened by <@{a}>")), "{text}");
    assert!(
        Collection::get().bottles.iter().any(|m| text.contains(&m.text)),
        "one of the bottle messages is quoted: {text}"
    );
    assert!(text.contains("Tucked inside") || text.contains("fell out"), "{text}");
    assert!(opened.button("Opened").disabled);
    let gained =
        shells(&sim, a).await + sim.app.db.fragments(a).await.unwrap() + sim.app.db.dex(a).await.unwrap().len() as i64;
    assert!(gained > 0, "the bottle held something");

    let slow = sim.click(b, &bottle, "Open the bottle").await;
    assert!(
        slow.ephemeral && slow.content().contains(&format!("Too slow! <@{a}> opened it first")),
        "{}",
        slow.content()
    );
    assert_eq!(shells(&sim, b).await, 0);
    let again = sim.click(a, &bottle, "Open the bottle").await;
    assert!(
        again.ephemeral && again.content().contains("already opened"),
        "{}",
        again.content()
    );
    let stats = sim.app.db.game_stats(a).await.unwrap();
    assert!(stats.iter().any(|s| s.game == "bottle" && s.played == 1 && s.won == 1));

    // A bottle nobody opens drifts away.
    assert!(post_bottle(&sim.app, sim.guild, channel).await.expect("posts"));
    let posts = sim.posts();
    let second_post = only(&posts, channel, Method::POST);
    let second = second_post.message;
    let second_open = reply_of(second_post)
        .button("Open the bottle")
        .custom_id
        .clone()
        .expect("custom id");
    // The test DB is persistent and the worker closes at most 50 drops per tick. Seed a
    // full older batch so this exercises eventual expiry rather than assuming an empty queue.
    sqlx::query(
        "INSERT INTO collection_drops(kind,guild_id,channel_id,cards,takers,expires_at)
        SELECT 'bottle',$1,$2,ARRAY['backlog'],ARRAY[0::bigint],now()-interval '1 hour' FROM generate_series(1,51)",
    )
    .bind(sim.guild as i64)
    .bind(channel as i64)
    .execute(sim.app.db.pool())
    .await
    .unwrap();
    sim.app.shift_clock(Duration::minutes(11));
    let mut posts = Vec::new();
    for _ in 0..100 {
        let closed = collection_tick(&sim.app).await.expect("tick");
        posts.extend(sim.posts());
        if closed < 50 || posts.iter().any(|p| p.message == second && p.method == Method::PATCH) {
            break;
        }
    }
    let gone = posts
        .iter()
        .find(|p| p.message == second && p.method == Method::PATCH)
        .expect("the bottle's message is edited");
    assert_eq!(gone.title(), "🍾 The bottle drifted away");
    let adrift = reply_of(gone);
    assert!(adrift.button("Drifted away").disabled);
    // Nobody opened that one, so a late click is told it is gone rather than that somebody was quicker.
    let late = sim.component(b, &second_open, &[], &adrift).await;
    assert!(
        late.ephemeral && late.content().contains("drifted off"),
        "{}",
        late.content()
    );
}

#[tokio::test]
async fn critterdrop_command_follows_the_server_setting_and_has_cooldowns() {
    let Some(mut sim) = Sim::new("critterdrop_command_follows_the_server_setting_and_has_cooldowns").await else {
        return;
    };
    let _turn = COLLECTION.lock().await;
    sim.channel = unique();
    let channel = sim.channel;
    let (host, other) = (unique(), unique());

    // Switched off (the default): refused politely, and neither cooldown was started.
    let off = sim.command(host, "critterdrop", json!([])).await;
    assert!(
        off.ephemeral
            && off.content().contains("switched off")
            && off.content().contains("/progression economy critters"),
        "{}",
        off.content()
    );
    assert!(sim.posts().is_empty());
    assert!(
        sim.app
            .cache
            .try_cooldown(&format!("critterdrop:{host}"), std::time::Duration::from_secs(1))
            .await
            .unwrap()
            .is_none(),
        "a refused call does not use up the caller's turn"
    );
    sim.app
        .cache
        .release_cooldown(&format!("critterdrop:{host}"))
        .await
        .unwrap();

    // Kept to another channel: told where to go.
    sim.guild = unique();
    let elsewhere = unique();
    enable_drops(&sim, Some(elsewhere)).await;
    let wrong = sim.command(host, "critterdrop", json!([])).await;
    assert!(
        wrong.ephemeral && wrong.content().contains(&format!("<#{elsewhere}>")),
        "{}",
        wrong.content()
    );
    assert!(sim.posts().is_empty());

    // On everywhere, it works.
    sim.guild = unique();
    enable_drops(&sim, None).await;
    let drop = sim.command(host, "critterdrop", json!([])).await;
    assert!(!drop.ephemeral, "the whole channel sees it");
    assert_eq!(drop.title(), "🦀 Critters washed up!");
    assert!(
        drop.description().contains(&format!("Called by <@{host}>")),
        "{}",
        drop.description()
    );
    assert_eq!(drop.files[0].0, "drop.png");
    assert!(drop.pings_nobody());
    // The caller takes part like everyone else.
    let grabbed = sim.click(host, &drop, "2").await;
    assert_eq!(grabbed.followups.len(), 1);

    // One at a time in a channel.
    let busy = sim.command(other, "critterdrop", json!([])).await;
    assert!(
        busy.ephemeral && busy.content().contains("still waiting"),
        "{}",
        busy.content()
    );

    // Once it is over, the channel waits out its own cooldown whoever calls, on top of the caller's.
    sim.app.shift_clock(Duration::seconds(100));
    collection_tick(&sim.app).await.expect("tick");
    let again = sim.command(host, "critterdrop", json!([])).await;
    assert!(
        again.ephemeral && again.content().contains("This channel had a drop"),
        "{}",
        again.content()
    );
    let others = sim.command(other, "critterdrop", json!([])).await;
    assert!(
        others.ephemeral
            && others.content().contains("This channel had a drop")
            && others.content().contains("minutes"),
        "{}",
        others.content()
    );
    // The channel's ten minutes pass (Redis keeps real time, so end them by hand): the caller still has
    // their own half hour to wait, and that does not cost the channel its turn.
    sim.app
        .cache
        .release_cooldown(&format!("critterdrop:channel:{channel}"))
        .await
        .unwrap();
    let again = sim.command(host, "critterdrop", json!([])).await;
    assert!(
        again.ephemeral && again.content().contains("called a drop"),
        "{}",
        again.content()
    );
    let another = sim.command(other, "critterdrop", json!([])).await;
    assert_eq!(another.title(), "🦀 Critters washed up!");
    let _ = sim.posts();
}

#[tokio::test]
async fn beachcomb_finds_something_then_waits_for_the_tide() {
    let Some(mut sim) = Sim::new("beachcomb_finds_something_then_waits_for_the_tide").await else {
        return;
    };
    let user = unique();

    let walk = sim.command(user, "beachcomb", json!([])).await;
    assert!(walk.ephemeral);
    assert_eq!(walk.files.len(), 1);
    assert_eq!(walk.files[0].0, "beachcomb.png");
    assert!(
        walk.description().contains("🆕"),
        "a first find is new: {}",
        walk.description()
    );
    let dex = sim.app.db.dex(user).await.unwrap();
    assert_eq!(dex.len(), 1);
    let entry = Collection::get().entry(&dex[0].entry_id).expect("a real entry");
    assert!(walk.description().contains(&entry.name), "{}", walk.description());
    assert_eq!(shells(&sim, user).await, entry.pay().new_shells);

    let soon = sim.command(user, "beachcomb", json!([])).await;
    assert!(
        soon.ephemeral && soon.content().contains("walked the shore"),
        "{}",
        soon.content()
    );
    assert_eq!(sim.app.db.dex(user).await.unwrap().len(), 1);

    // After twenty minutes the tide has brought more.
    sim.app.shift_clock(Duration::minutes(21));
    let walk = sim.command(user, "beachcomb", json!([])).await;
    assert_eq!(walk.files[0].0, "beachcomb.png");
    let stats = sim.app.db.game_stats(user).await.unwrap();
    assert!(stats.iter().any(|s| s.game == "beachcomb" && s.played == 2));

    // Each walk's card is worth looking at: a rarer find than usual.
    let other = unique();
    for _ in 0..4 {
        sim.app.shift_clock(Duration::minutes(21));
        sim.command(other, "beachcomb", json!([])).await;
    }
}

#[tokio::test]
async fn the_dex_pages_its_critters_and_finds_and_spends_fragments_on_decor() {
    let Some(mut sim) = Sim::new("the_dex_pages_its_critters_and_finds_and_spends_fragments_on_decor").await else {
        return;
    };
    let (owner, visitor) = (unique(), unique());
    let collection = Collection::get();
    let critters: Vec<&str> = collection.critters.iter().take(25).map(|e| e.id.as_str()).collect();
    let finds: Vec<&str> = collection.finds.iter().take(9).map(|e| e.id.as_str()).collect();
    give(&sim, owner, &critters, 1).await;
    give(&sim, owner, &finds[..3], 4).await;
    give(&sim, owner, &finds[3..], 1).await;
    give_fragments(&sim, owner, 120).await;

    // A first page: it cannot go back, it can go on, and the owner can spend fragments.
    let book = sim.command(owner, "dex", json!([])).await;
    assert!(book.ephemeral);
    assert_eq!(book.files[0].0, "dex.png");
    assert!(
        book.description().contains("critters 25/48") && book.description().contains("34 of 84 found"),
        "{}",
        book.description()
    );
    assert!(book.button("◀").disabled && !book.button("▶").disabled);
    assert!(book.button("Critters").disabled && !book.button("Finds").disabled);
    assert!(book.has_button("Spend fragments"));

    let second = sim.click(owner, &book, "▶").await;
    assert!(!second.button("◀").disabled && !second.button("▶").disabled);
    let third = sim.click(owner, &second, "▶").await;
    assert!(third.button("▶").disabled, "48 critters make three pages");
    assert_eq!(third.files[0].0, "dex.png");
    let finds_tab = sim.click(owner, &third, "Finds").await;
    assert!(finds_tab.button("Finds").disabled && finds_tab.button("◀").disabled);
    assert!(
        finds_tab.description().contains("finds 9/36"),
        "{}",
        finds_tab.description()
    );
    let second_finds = sim.click(owner, &finds_tab, "▶").await;
    assert!(second_finds.button("▶").disabled, "36 finds make two pages");

    // Somebody else's dex: the same pages, nothing to spend.
    let theirs = sim
        .command_resolved(
            visitor,
            "dex",
            json!([user_option("user", owner), text_option("tab", "finds")]),
            resolved(&[owner], &[]),
        )
        .await;
    assert!(theirs.description().contains("finds 9/36") && !theirs.has_button("Spend fragments"));
    let sneaky = sim.component(visitor, &format!("bc:x:{owner}"), &[], &theirs).await;
    assert!(
        sneaky.ephemeral && sneaky.content().contains("not your dex"),
        "{}",
        sneaky.content()
    );
    let empty = sim.command(visitor, "dex", json!([])).await;
    assert!(empty.description().contains("0 of 84 found"), "{}", empty.description());

    // Fragments: a common piece is within reach; a legendary one is far out of it. The panel says what the
    // pieces are: soulbound.
    let panel = sim.click(owner, &book, "Spend fragments").await;
    assert_eq!(panel.title(), "Fragment exchange");
    assert_eq!(panel.options("bc:xs:").len(), 5);
    assert!(panel.description().contains("120"), "{}", panel.description());
    assert!(panel.description().contains("soulbound"), "{}", panel.description());
    let broke = sim.pick(owner, &panel, "bc:xs:", "legendary").await;
    assert!(
        broke.description().contains("costs **8,900** fragments"),
        "{}",
        broke.description()
    );
    assert_eq!(sim.app.db.fragments(owner).await.unwrap(), 120);
    let common = exchange_cost(Rarity::Common);
    assert_eq!(common, 12);
    let bought = sim.pick(owner, &panel, "bc:xs:", "common").await;
    assert!(bought.title().starts_with("You got a "), "{}", bought.title());
    assert_eq!(bought.files[0].0, "piece.png");
    assert!(bought.description().contains("soulbound"), "{}", bought.description());
    assert_eq!(sim.app.db.fragments(owner).await.unwrap(), 120 - common);
    let owned = sim.app.db.inventory(owner).await.unwrap();
    assert!(
        owned.iter().any(|r| is_soulbound(&r.item_id)
            && bought
                .title()
                .contains(&Content::get().item(&r.item_id).expect("item").name())),
        "the piece is in the inventory, soulbound: {}",
        bought.title()
    );
    assert!(
        owned.iter().all(|r| is_soulbound(&r.item_id)),
        "and it is all they have"
    );
    let back = sim.click(owner, &bought, "Back to the dex").await;
    assert_eq!(back.files[0].0, "dex.png");
    assert!(back.description().contains("critters 25/48"));
    let _ = Kind::Critter;
}

#[tokio::test]
async fn the_crab_pot_soaks_hauls_once_and_upgrades() {
    let Some(mut sim) = Sim::new("the_crab_pot_soaks_hauls_once_and_upgrades").await else {
        return;
    };
    let user = unique();

    let empty = sim.command(user, "crabpot", json!([])).await;
    assert!(empty.ephemeral);
    assert_eq!(empty.files[0].0, "pot.png");
    assert!(
        empty.description().contains("level 1: empty"),
        "{}",
        empty.description()
    );
    for label in ["Set for 1 h", "Set for 4 h", "Set for 8 h", "Upgrade (400 shells)"] {
        assert!(empty.has_button(label), "{label}");
    }

    let soaking = sim.click(user, &empty, "Set for 1 h").await;
    assert!(
        soaking.description().contains("in the water for 1 h"),
        "{}",
        soaking.description()
    );
    assert!(soaking.has_button("Check the tide") && !soaking.has_button("Haul it in!"));
    // A second soak cannot be started on top of the first.
    let twice = sim.click(user, &empty, "Set for 8 h").await;
    assert!(
        twice.description().contains("already in the water"),
        "{}",
        twice.description()
    );
    assert_eq!(sim.app.db.pot(user).await.unwrap().hours, Some(1));
    let early = sim.click(user, &soaking, "Check the tide").await;
    assert!(early.description().contains("soaking"), "{}", early.description());
    // Nobody else can haul it.
    let thief = sim.component(unique(), &format!("kp:h:{user}"), &[], &early).await;
    assert!(
        thief.ephemeral && thief.content().contains("not your pot"),
        "{}",
        thief.content()
    );

    // An hour later it is ready: one haul, filed in the dex and paid.
    sim.app.shift_clock(Duration::minutes(61));
    let ready = sim.command(user, "crabpot", json!([])).await;
    assert!(ready.has_button("Haul it in!"), "{}", ready.description());
    let haul = sim.click(user, &ready, "Haul it in!").await;
    assert_eq!(haul.files[0].0, "haul.png");
    assert!(
        haul.description().contains("You hauled in **1** catch"),
        "{}",
        haul.description()
    );
    assert_eq!(sim.app.db.dex(user).await.unwrap().len(), 1);
    assert!(shells(&sim, user).await > 0);
    let again = sim.click(user, &ready, "Haul it in!").await;
    assert!(
        again.description().contains("Your pot is empty"),
        "{}",
        again.description()
    );
    assert_eq!(sim.app.db.dex(user).await.unwrap().len(), 1, "nothing was paid twice");

    // An eight hour soak brings a whole haul: a grid of what it caught.
    let reset = sim.click(user, &haul, "Back to the pot").await;
    let soak = sim.click(user, &reset, "Set for 8 h").await;
    assert!(soak.has_button("Check the tide"));
    sim.app.shift_clock(Duration::hours(9));
    let ready = sim.command(user, "crabpot", json!([])).await;
    let big = sim.click(user, &ready, "Haul it in!").await;
    assert!(
        big.description().contains("You hauled in **10** catches"),
        "{}",
        big.description()
    );
    let kinds = sim.app.db.dex(user).await.unwrap().len();
    assert!(kinds >= 2);

    // The day's third paying haul is used up: the next one fills the dex and says that is all it did.
    let reset = sim.click(user, &big, "Back to the pot").await;
    let soak = sim.click(user, &reset, "Set for 1 h").await;
    assert!(soak.has_button("Check the tide"));
    sim.app.shift_clock(Duration::minutes(61));
    let day = sim.app.now().timestamp().div_euclid(86_400) as i32;
    sqlx::query(
        "INSERT INTO collection_daily (user_id, day, game, plays) VALUES ($1, $2, 'crabpot', 3)
         ON CONFLICT (user_id, day, game) DO UPDATE SET plays = 3",
    )
    .bind(user as i64)
    .bind(day)
    .execute(sim.app.db.pool())
    .await
    .unwrap();
    let before = shells(&sim, user).await;
    let ready = sim.command(user, "crabpot", json!([])).await;
    let capped = sim.click(user, &ready, "Haul it in!").await;
    assert!(
        capped.description().contains("Daily haul limit reached"),
        "{}",
        capped.description()
    );
    assert_eq!(shells(&sim, user).await, before, "nothing was paid");

    // Upgrading costs shells, and says so when there are not enough.
    sqlx::query("UPDATE users SET credits = 0 WHERE id = $1")
        .bind(user as i64)
        .execute(sim.app.db.pool())
        .await
        .unwrap();
    let pot = sim.command(user, "crabpot", json!([])).await;
    let poor = sim.click(user, &pot, "Upgrade (400 shells)").await;
    assert!(
        poor.description().contains("costs **400** shells"),
        "{}",
        poor.description()
    );
    fund(&sim, user, 500).await;
    let upgraded = sim.click(user, &pot, "Upgrade (400 shells)").await;
    assert!(upgraded.description().contains("level 2"), "{}", upgraded.description());
    assert!(upgraded.has_button("Upgrade (1,000 shells)"));
    assert_eq!(shells(&sim, user).await, 100);
    // The old button is out of date now.
    let stale = sim.click(user, &pot, "Upgrade (400 shells)").await;
    assert!(
        stale.description().contains("not at that level"),
        "{}",
        stale.description()
    );
    assert_eq!(shells(&sim, user).await, 100);
}

#[tokio::test]
async fn help_tells_about_collecting() {
    let Some(mut sim) = Sim::new("help_tells_about_collecting").await else {
        return;
    };
    let help = sim.command(unique(), "help", json!([])).await;
    let areas = super::help_areas(&help);
    assert!(areas.iter().any(|a| a == "Crowd games & collecting"), "{areas:?}");
}

/// A bought-with-fragments piece (soulbound) and a plain one of another design, for `user`.
async fn pieces_for(sim: &Sim, user: u64) -> (String, String) {
    let content = pinchora_game::Content::get();
    let base = exchange_pick(content, Rarity::Common, 0.0).expect("a common piece");
    let plain = exchange_pick(content, Rarity::Uncommon, 0.0).expect("an uncommon piece");
    let db = &sim.app.db;
    give_fragments(sim, user, 50).await;
    db.exchange_fragments(user, exchange_cost(Rarity::Common), &base)
        .await
        .unwrap()
        .expect("fragments cover a common piece");
    db.grant_item(user, &plain, 3).await.unwrap();
    (soulbound(&base), plain)
}

fn values(reply: &Reply, menu: &str) -> Vec<String> {
    reply.options(menu).iter().map(|o| o.value.clone()).collect()
}

#[tokio::test]
async fn soulbound_decor_cannot_be_sold_gifted_or_donated() {
    let Some(mut sim) = Sim::new("soulbound_decor_cannot_be_sold_gifted_or_donated").await else {
        return;
    };
    let (owner, friend, only_bound) = (unique(), unique(), unique());
    let (bound, plain) = pieces_for(&sim, owner).await;
    // Somebody whose only piece is a soulbound one.
    let (only, _) = pieces_for(&sim, only_bound).await;
    sqlx::query("DELETE FROM inventory WHERE user_id = $1 AND item_id <> $2")
        .bind(only_bound as i64)
        .bind(&only)
        .execute(sim.app.db.pool())
        .await
        .unwrap();
    let inventory = sim.command(owner, "inventory", json!([])).await;
    assert_eq!(
        inventory.files[0].0, "inventory.png",
        "a soulbound piece is drawn like any other"
    );

    // A stall: the menu leaves the piece out, a hand-made pick of it is refused, the plain piece sells.
    let sell = || json!([sub("sell", vec![int_option("price", 500)])]);
    let menu = sim.command(owner, "stall", sell()).await;
    let offered = values(&menu, "sl:");
    assert!(offered.contains(&plain) && !offered.contains(&bound), "{offered:?}");
    let refused = sim.component(owner, "sl:500", &[&bound], &menu).await;
    assert!(
        refused.content().contains("can't go on a stall") && refused.content().contains("soulbound"),
        "{}",
        refused.content()
    );
    assert!(sim.app.db.listings(owner).await.unwrap().is_empty());
    assert_eq!(owned(&sim, owner, &bound).await, 1);
    let listed = sim.pick(owner, &menu, "sl:", &plain).await;
    assert!(listed.content().contains("is on your stall"), "{}", listed.content());
    let rows = sim.app.db.listings(owner).await.unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].item_id, plain);
    let nothing = sim.command(only_bound, "stall", sell()).await;
    assert!(
        nothing.ephemeral && nothing.content().contains("soulbound") && nothing.rows() == 0,
        "{}",
        nothing.content()
    );

    // A gift: the same.
    let gift = || (json!([user_option("user", friend)]), resolved(&[friend], &[]));
    let (options, people) = gift();
    let menu = sim.command_resolved(owner, "gift", options, people).await;
    let offered = values(&menu, "gf:");
    assert!(offered.contains(&plain) && !offered.contains(&bound), "{offered:?}");
    let refused = sim.component(owner, &format!("gf:{friend}"), &[&bound], &menu).await;
    assert!(
        refused.content().contains("can't be given away") && refused.content().contains("soulbound"),
        "{}",
        refused.content()
    );
    assert_eq!(
        (owned(&sim, owner, &bound).await, owned(&sim, friend, &bound).await),
        (1, 0)
    );
    let sent = sim.pick(owner, &menu, "gf:", &plain).await;
    assert!(sent.content().contains("Sent"), "{}", sent.content());
    assert_eq!(owned(&sim, friend, &plain).await, 1);
    let (options, people) = gift();
    let nothing = sim.command_resolved(only_bound, "gift", options, people).await;
    assert!(
        nothing.ephemeral && nothing.content().contains("soulbound") && nothing.rows() == 0,
        "{}",
        nothing.content()
    );

    // A donation to the server's plaza: the same again.
    let donate = || json!([sub("donate", vec![])]);
    let menu = sim.command(owner, "plaza", donate()).await;
    let offered = values(&menu, "pd");
    assert!(offered.contains(&plain) && !offered.contains(&bound), "{offered:?}");
    let refused = sim.component(owner, "pd", &[&bound], &menu).await;
    assert!(
        refused.content().contains("can't be donated") && refused.content().contains("soulbound"),
        "{}",
        refused.content()
    );
    assert_eq!(
        (owned(&sim, owner, &bound).await, owned(&sim, sim.guild, &bound).await),
        (1, 0)
    );
    let donated = sim.pick(owner, &menu, "pd", &plain).await;
    assert!(donated.content().contains("Donated"), "{}", donated.content());
    assert_eq!(owned(&sim, sim.guild, &plain).await, 1);
    let nothing = sim.command(only_bound, "plaza", donate()).await;
    assert!(
        nothing.ephemeral && nothing.content().contains("soulbound") && nothing.rows() == 0,
        "{}",
        nothing.content()
    );

    // The piece never moved: still the owner's, and it is all they have of it.
    assert_eq!(owned(&sim, owner, &bound).await, 1);
    assert_eq!(owned(&sim, owner, &soulbound(&plain)).await, 0);
}

#[tokio::test]
async fn soulbound_decor_is_placed_in_your_own_house_but_not_a_partners() {
    let Some(mut sim) = Sim::new("soulbound_decor_is_placed_in_your_own_house_but_not_a_partners").await else {
        return;
    };
    let (a, b) = (unique(), unique());
    sim.app.db.pair_partners(a, b).await.unwrap();
    sim.app.db.remember_name(b, "Bea").await.unwrap();
    let (bound, plain) = pieces_for(&sim, a).await;
    let edit_options = |owner: u64| json!([sub("edit", vec![user_option("owner", owner)])]);

    // In a's own editor the piece is on the menu, marked, and goes into the room like any other.
    let own = sim.command(a, "house", json!([{ "name": "edit", "type": 1 }])).await;
    let entry = own
        .options("he:s:")
        .iter()
        .find(|o| o.value == bound)
        .expect("the soulbound piece is on the menu");
    assert!(
        entry.description.as_deref().is_some_and(|d| d.contains("Soulbound")),
        "{:?}",
        entry.description
    );
    let held = sim.pick(a, &own, "he:s:", &bound).await;
    let placed = sim.click(a, &held, "Place").await;
    assert!(placed.description().contains("Placed **"), "{}", placed.description());
    assert_eq!(owned(&sim, a, &bound).await, 0);
    assert!(room_pieces(&sim, a, 0).await.iter().any(|p| p.0 == bound));
    // Storing it puts it back as a soulbound piece, not a plain one.
    let stored = sim.click(a, &placed, "Store").await;
    assert!(stored.description().contains("Stored"), "{}", stored.description());
    assert_eq!(
        (owned(&sim, a, &bound).await, owned(&sim, a, &soulbound(&plain)).await),
        (1, 0)
    );

    // Decorating b's house: the soulbound piece is left off a's menu, and a hand-made pick cannot place it.
    let edit = sim
        .command_resolved(a, "house", edit_options(b), named(&[(b, "Bea")]))
        .await;
    let offered = values(&edit, "pt:s:");
    assert!(offered.contains(&plain) && !offered.contains(&bound), "{offered:?}");
    let footer = edit.embeds[0]
        .footer
        .as_ref()
        .map(|f| f.text.clone())
        .unwrap_or_default();
    assert!(footer.contains("soulbound pieces stay out of this menu"), "{footer}");
    let id = edit.menu("pt:s:").custom_id.clone();
    let held = sim.component(a, &id, &[&bound], &edit).await;
    let place_id = held.button("Place").custom_id.clone().expect("custom id");
    let blocked = sim.component(a, &place_id, &[], &held).await;
    assert!(
        blocked
            .description()
            .contains("soulbound pieces only go in your own house"),
        "{}",
        blocked.description()
    );
    assert!(room_pieces(&sim, b, 0).await.is_empty());
    assert_eq!((owned(&sim, a, &bound).await, owned(&sim, b, &bound).await), (1, 0));
    // The plain piece of another design still goes in.
    let held = sim.pick(a, &edit, "pt:s:", &plain).await;
    let placed = sim.click(a, &held, "Place").await;
    assert!(placed.description().contains("Placed **"), "{}", placed.description());
    assert_eq!(room_pieces(&sim, b, 0).await.len(), 1);
}

#[tokio::test]
async fn recycle_selection_is_reversible_and_confirmation_replay_is_idempotent() {
    let Some(mut sim) = Sim::new("recycle_selection_is_reversible_and_confirmation_replay_is_idempotent").await else {
        return;
    };
    for (lang, recycle, confirm) in [
        (pinchora_core::i18n::Lang::En, "Recycle", "Confirm recycle"),
        (pinchora_core::i18n::Lang::Tr, "Dönüştür", "Geri dönüşümü onayla"),
    ] {
        let user = unique();
        sim.app.set_user_lang(user, Some(lang)).await.unwrap();
        let entry = &Collection::get().critters[0];
        sqlx::query("INSERT INTO dex_entries(user_id,entry_id,caught,kept) VALUES($1,$2,4,4)")
            .bind(user as i64)
            .bind(&entry.id)
            .execute(sim.app.db.pool())
            .await
            .unwrap();
        let dex = sim.command(user, "dex", json!([])).await;
        let menu = sim.click(user, &dex, recycle).await;
        let confirmation = sim.pick(user, &menu, "bc:rs:", &entry.id).await;
        assert_eq!(sim.app.db.dex(user).await.unwrap()[0].kept, 4);
        assert_eq!(sim.app.db.shards(user).await.unwrap(), 0);
        let other = unique();
        sim.click(other, &confirmation, confirm).await;
        assert_eq!(sim.app.db.dex(user).await.unwrap()[0].kept, 4);
        sim.click(user, &confirmation, confirm).await;
        assert_eq!(sim.app.db.dex(user).await.unwrap()[0].kept, 3);
        sim.app
            .cache
            .release_cooldown(&format!("collection-depth:{user}:rc"))
            .await
            .unwrap();
        sim.click(user, &confirmation, confirm).await;
        assert_eq!(sim.app.db.dex(user).await.unwrap()[0].kept, 3);
        assert_eq!(
            sim.app.db.shards(user).await.unwrap(),
            pinchora_game::collection::recycle_shards(entry.rarity)
        );
        sim.app
            .db
            .recycle_critter(user, &entry.id, 24, "craft-funds")
            .await
            .unwrap();
        let label = if lang == pinchora_core::i18n::Lang::En {
            "Craft loot"
        } else {
            "Ganimet üret"
        };
        sim.click(user, &dex, label).await;
        let left = sim.app.db.shards(user).await.unwrap();
        sim.app
            .cache
            .release_cooldown(&format!("collection-depth:{user}:c"))
            .await
            .unwrap();
        sim.click(user, &dex, label).await;
        assert_eq!(sim.app.db.shards(user).await.unwrap(), left);
        assert_eq!(
            sim.app
                .db
                .inventory(user)
                .await
                .unwrap()
                .iter()
                .map(|i| i.quantity)
                .sum::<i32>(),
            1
        );
    }
}

#[tokio::test]
async fn collection_claim_components_cannot_cross_guild_or_channel_boundaries() {
    let Some(mut sim) = Sim::new("collection_claim_components_cannot_cross_guild_or_channel_boundaries").await else {
        return;
    };
    let _turn = COLLECTION.lock().await;
    sim.channel = unique();
    post_critter_drop(&sim.app, sim.guild, sim.channel).await.unwrap();
    let posts = sim.posts();
    let shown = reply_of(only(&posts, sim.channel, Method::POST));
    let (id,): (i64,) = sqlx::query_as("SELECT id FROM collection_drops WHERE channel_id=$1 ORDER BY id DESC LIMIT 1")
        .bind(sim.channel as i64)
        .fetch_one(sim.app.db.pool())
        .await
        .unwrap();
    let user = unique();
    let (guild, channel) = (sim.guild, sim.channel);
    sim.guild = unique();
    sim.component(user, &format!("cd:g:{id}:0"), &[], &shown).await;
    assert!(sim.app.db.dex(user).await.unwrap().is_empty());
    sim.guild = guild;
    sim.channel = unique();
    sim.component(user, &format!("cd:g:{id}:0"), &[], &shown).await;
    assert!(sim.app.db.dex(user).await.unwrap().is_empty());
    sim.channel = channel;
    sim.component(user, &format!("cd:g:{id}:0"), &[], &shown).await;
    assert_eq!(sim.app.db.dex(user).await.unwrap().len(), 1);
}
