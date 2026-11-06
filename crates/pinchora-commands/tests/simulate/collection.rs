//! The collection games, clicked through: chat-driven drops, Critter Drop, Message in a Bottle,
//! Beachcomb, the Tide Pool Dex and the Crab Pot.

use chrono::Duration;
use pinchora_commands::{collection_tick, post_bottle, post_critter_drop};
use pinchora_game::collection::{Collection, Kind};

use super::*;

/// Drops are closed by whoever ticks, for every drop in the database, so the tests that make drops and the
/// ones that move the clock on take turns.
static COLLECTION: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

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
    let config = serde_json::to_value(definitions.iter().find(|c| c.name == "config").unwrap()).unwrap();
    let critters = config["options"]
        .as_array()
        .unwrap()
        .iter()
        .find(|o| o["name"] == "critters")
        .expect("/config critters");
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
        slow.ephemeral && slow.content().contains("drifted off"),
        "{}",
        slow.content()
    );
    assert_eq!(shells(&sim, b).await, 0);
    let stats = sim.app.db.game_stats(a).await.unwrap();
    assert!(stats.iter().any(|s| s.game == "bottle" && s.played == 1 && s.won == 1));

    // A bottle nobody opens drifts away.
    assert!(post_bottle(&sim.app, sim.guild, channel).await.expect("posts"));
    let posts = sim.posts();
    let second = only(&posts, channel, Method::POST).message;
    sim.app.shift_clock(Duration::minutes(11));
    collection_tick(&sim.app).await.expect("tick");
    let posts = sim.posts();
    let gone = posts
        .iter()
        .find(|p| p.message == second && p.method == Method::PATCH)
        .expect("the bottle's message is edited");
    assert_eq!(gone.title(), "🍾 The bottle drifted away");
    assert!(reply_of(gone).button("Drifted away").disabled);
}

#[tokio::test]
async fn critterdrop_command_posts_for_the_channel_and_has_a_cooldown() {
    let Some(mut sim) = Sim::new("critterdrop_command_posts_for_the_channel_and_has_a_cooldown").await else {
        return;
    };
    let _turn = COLLECTION.lock().await;
    sim.channel = unique();
    let (host, other) = (unique(), unique());

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

    // Once it is over, the caller still waits for their own cooldown, and somebody else does not.
    sim.app.shift_clock(Duration::seconds(100));
    collection_tick(&sim.app).await.expect("tick");
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

    // Fragments: 25 buys a common piece; a legendary one is far out of reach.
    let panel = sim.click(owner, &book, "Spend fragments").await;
    assert_eq!(panel.title(), "Fragment exchange");
    assert_eq!(panel.options("bc:xs:").len(), 5);
    assert!(panel.description().contains("120"), "{}", panel.description());
    let broke = sim.pick(owner, &panel, "bc:xs:", "legendary").await;
    assert!(
        broke.description().contains("costs **1,000** fragments"),
        "{}",
        broke.description()
    );
    assert_eq!(sim.app.db.fragments(owner).await.unwrap(), 120);
    let bought = sim.pick(owner, &panel, "bc:xs:", "common").await;
    assert!(bought.title().starts_with("You got a "), "{}", bought.title());
    assert_eq!(bought.files[0].0, "piece.png");
    assert_eq!(sim.app.db.fragments(owner).await.unwrap(), 95);
    let owned = sim.app.db.inventory(owner).await.unwrap();
    assert!(
        owned.iter().any(|r| bought
            .title()
            .contains(&Content::get().item(&r.item_id).expect("item").name())),
        "the piece is in the inventory: {}",
        bought.title()
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
    let text = help.field("Collecting");
    for command in ["/beachcomb", "/crabpot", "/critterdrop", "/dex", "/config critters"] {
        assert!(text.contains(command), "{command}");
    }
}
