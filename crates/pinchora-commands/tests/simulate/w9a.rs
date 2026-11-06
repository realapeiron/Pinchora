//! Correctness follow-ups: stale buttons, wagers that forfeit, requests that cannot be repeated, one open
//! drop per channel, forged ids across kinds of game and the voice XP scale fixes.

use std::sync::atomic::{AtomicUsize, Ordering};

use pinchora_game::{Content, play::PlayContent};
use pinchora_render::Theme;

use super::*;

// /Quests

#[tokio::test]
async fn a_quest_button_from_an_earlier_day_says_it_expired() {
    let Some(mut sim) = Sim::new("a_quest_button_from_an_earlier_day_says_it_expired").await else {
        return;
    };
    let user = unique();
    let board = sim.command(user, "quests", json!([])).await;
    assert!(board.title().contains("Today's quests"), "{}", board.title());
    // Yesterday's board is still on screen: its button claims nothing today, and says why.
    let stale = sim.component(user, &format!("qc:{}", today() - 1), &[], &board).await;
    assert!(
        stale.description().contains("earlier day") && stale.description().contains("today's quests"),
        "{}",
        stale.description()
    );
    // A button that carries no day at all is just as old.
    let bare = sim.component(user, "qc", &[], &board).await;
    assert!(bare.description().contains("earlier day"), "{}", bare.description());
    // Today's own button works: nothing is finished yet, so nothing is claimed.
    let own = sim.component(user, &format!("qc:{}", today()), &[], &board).await;
    assert!(
        own.description().contains("Nothing new to claim"),
        "{}",
        own.description()
    );
    assert_eq!(shells(&sim, user).await, 0);
}

// /Duel rps

#[tokio::test]
async fn a_wagered_rock_paper_scissors_match_forfeits_to_the_player_who_picked() {
    let _serial = SWEEPS.lock().await;
    let Some(mut sim) = Sim::new("a_wagered_rock_paper_scissors_match_forfeits_to_the_player_who_picked").await else {
        return;
    };
    forget_late_games(&sim).await;
    let (a, b, c, d, e, f) = (unique(), unique(), unique(), unique(), unique(), unique());
    for user in [a, b, c, d, e, f] {
        fund(&sim, user, 300).await;
    }
    let options = |bet: Option<i64>| {
        let mut options = vec![int_option("rounds", 1)];
        options.extend(bet.map(|bet| int_option("bet", bet)));
        options
    };

    // A picks, B never does: B ran out of time, so A takes the pot.
    let challenge = duel(&mut sim, a, b, "rps", options(Some(100))).await;
    let board = sim.click(b, &challenge, "Accept").await;
    let session = session_of(&board);
    assert_eq!((shells(&sim, a).await, shells(&sim, b).await), (200, 200));
    let picker = sim.click(a, &board, "Choose your move").await;
    sim.click(a, &picker, "🪨 Rock").await;
    expire(&sim, session).await;
    sweep_games(&sim.app).await.expect("sweep");
    assert_eq!(
        (shells(&sim, a).await, shells(&sim, b).await),
        (400, 200),
        "the slow player loses the wager to the one who picked"
    );
    let message = message_of(&sim, session).await;
    let posts = sim.posts();
    let ended = posts
        .iter()
        .find(|p| p.message == message && p.content().contains("forfeits"))
        .expect("the message says who forfeited");
    assert!(
        ended
            .content()
            .contains(&format!("<@{b}> ran out of time and forfeits"))
            && ended
                .content()
                .contains(&format!("<@{a}> takes the pot of **200** shells")),
        "{}",
        ended.content()
    );
    sweep_games(&sim.app).await.expect("sweep again");
    assert_eq!((shells(&sim, a).await, shells(&sim, b).await), (400, 200), "paid once");

    // Nobody picked: nobody is to blame, and both wagers go back.
    let challenge = duel(&mut sim, c, d, "rps", options(Some(100))).await;
    let board = sim.click(d, &challenge, "Accept").await;
    assert_eq!((shells(&sim, c).await, shells(&sim, d).await), (200, 200));
    expire(&sim, session_of(&board)).await;
    sweep_games(&sim.app).await.expect("sweep");
    assert_eq!((shells(&sim, c).await, shells(&sim, d).await), (300, 300));

    // Without a wager the game just ends.
    let challenge = duel(&mut sim, e, f, "rps", options(None)).await;
    let board = sim.click(f, &challenge, "Accept").await;
    let session = session_of(&board);
    let picker = sim.click(e, &board, "Choose your move").await;
    sim.click(e, &picker, "🪨 Rock").await;
    expire(&sim, session).await;
    sweep_games(&sim.app).await.expect("sweep");
    assert_eq!((shells(&sim, e).await, shells(&sim, f).await), (300, 300));
    let message = message_of(&sim, session).await;
    let posts = sim.posts();
    let ended = posts
        .iter()
        .find(|p| p.message == message && p.content().contains("ran out of time"))
        .expect("the message says the game timed out");
    assert!(
        ended.content().contains("so the game is over") && !ended.content().contains("forfeits"),
        "{}",
        ended.content()
    );
}

// /Play wordle

#[tokio::test]
async fn the_wordle_guess_button_opens_its_form_before_anything_is_read() {
    let Some(mut sim) = Sim::new("the_wordle_guess_button_opens_its_form_before_anything_is_read").await else {
        return;
    };
    let player = unique();
    let board = sim.command(player, "play", json!([sub("wordle", vec![])])).await;
    // The first click of anything warms what every click needs (the server's switches); only then is the
    // number of statements a button costs a number about the button. A button that does nothing is the
    // baseline.
    sim.component(player, "wo:zz", &[], &board).await;
    let before = queries();
    sim.component(player, "wo:zz", &[], &board).await;
    let idle = queries() - before;
    let before = queries();
    let form = sim.click(player, &board, "Guess").await;
    assert_eq!(
        queries() - before,
        idle,
        "the form is the first answer: it does not wait for a read of the board"
    );
    assert_eq!(
        form.modal.as_ref().map(|m| m.0.clone()),
        Some(format!("wo:m:daily:{}", today())),
        "{form}"
    );
    // A board that is over is told when the guess is sent.
    let words = &PlayContent::get().words;
    let answer = pinchora_game::play::wordle::daily_word(&words.answers, today()).to_owned();
    let finished = sim.submit(player, &form, &answer, &board).await;
    assert!(
        finished.description().contains("You got it in **1**"),
        "{}",
        finished.description()
    );
    let late = sim.submit(player, &form, "crane", &finished).await;
    assert!(
        late.ephemeral && late.content().contains("finished"),
        "{}",
        late.content()
    );
}

#[tokio::test]
async fn a_new_practice_word_from_an_old_button_counts_the_unfinished_board_as_a_loss() {
    let Some(mut sim) = Sim::new("a_new_practice_word_from_an_old_button_counts_the_unfinished_board_as_a_loss").await
    else {
        return;
    };
    let player = unique();
    let words = &PlayContent::get().words;
    let practice = || json!([sub("wordle", vec![text_option("mode", "practice")])]);
    let board = sim.command(player, "play", practice()).await;
    let first = sim
        .app
        .db
        .wordle_game(player, "practice", 0)
        .await
        .unwrap()
        .expect("a board");
    assert_eq!(record_of(&sim, player, "wordle").await, (0, 0));

    // A board with no guess in it is nothing to lose.
    let untouched = sim.component(player, "wo:n", &[], &board).await;
    assert_eq!(untouched.title(), "Practice wordle");
    assert_eq!(record_of(&sim, player, "wordle").await, (0, 0));

    // A guess in, and the old "New word" button of a finished board gets clicked: the game is lost.
    let current = sim
        .app
        .db
        .wordle_game(player, "practice", 0)
        .await
        .unwrap()
        .expect("a board");
    let miss = ["crane", "slate", "pious", "tough"]
        .into_iter()
        .find(|w| *w != current.word && *w != first.word && words.is_allowed(w))
        .expect("a word that is not the answer");
    let form = sim.click(player, &board, "Guess").await;
    sim.submit(player, &form, miss, &board).await;
    assert_eq!(
        sim.app
            .db
            .wordle_game(player, "practice", 0)
            .await
            .unwrap()
            .unwrap()
            .guesses
            .len(),
        1
    );
    let replaced = sim.component(player, "wo:n", &[], &board).await;
    assert_eq!(replaced.title(), "Practice wordle");
    assert_eq!(
        record_of(&sim, player, "wordle").await,
        (1, 0),
        "walking away from a board being played is a lost game"
    );
    let fresh = sim.app.db.wordle_game(player, "practice", 0).await.unwrap().unwrap();
    assert!(fresh.guesses.is_empty() && fresh.status == "playing");
    // The new board has no guess either, so clicking again costs nothing more.
    sim.component(player, "wo:n", &[], &board).await;
    assert_eq!(record_of(&sim, player, "wordle").await, (1, 0));
}

#[test]
fn the_wordle_card_tells_a_wrong_letter_from_a_key_nobody_tried() {
    use pinchora_render::{ABSENT, WordleCard, WordleRow};
    let theme = Theme::reef();
    let rows = [WordleRow {
        letters: ['c', 'r', 'a', 'n', 'e'],
        marks: [ABSENT; 5],
    }];
    let mut keys = [0u8; 26];
    keys[(b'c' - b'a') as usize] = ABSENT;
    let card = WordleCard {
        title: "Practice",
        rows: &rows,
        keys,
        footer: "Try 2 of 6",
        theme,
    };
    let svg = card.to_svg();
    // The wrong-letter tile and the idle key are different colours, and clearly so.
    let fills = |color: &str| svg.matches(&format!(r##"fill="{color}""##)).count();
    assert!(fills("#3b4676") == 0, "the old wrong-letter grey is gone");
    assert!(fills(pinchora_render::WRONG_LETTER) >= 5, "five wrong-letter tiles");
    assert_ne!(pinchora_render::WRONG_LETTER, pinchora_render::IDLE_KEY);
    let distance = |a: &str, b: &str| {
        let channel = |hex: &str, i: usize| i32::from_str_radix(&hex[1 + i * 2..3 + i * 2], 16).unwrap_or(0);
        (0..3).map(|i| (channel(a, i) - channel(b, i)).abs()).sum::<i32>()
    };
    assert!(
        distance(pinchora_render::WRONG_LETTER, pinchora_render::IDLE_KEY) >= 90,
        "{} and {} are too close",
        pinchora_render::WRONG_LETTER,
        pinchora_render::IDLE_KEY
    );
}

// /Friends, /partner, /hug

#[tokio::test]
async fn a_request_cannot_be_sent_again_within_a_minute_and_pings_only_its_receiver() {
    let Some(mut sim) = Sim::new("a_request_cannot_be_sent_again_within_a_minute_and_pings_only_its_receiver").await
    else {
        return;
    };
    let (a, b, c) = (unique(), unique(), unique());
    let friend = |to: u64, name: &str| (json!([sub("add", vec![user_option("user", to)])]), named(&[(to, name)]));
    let pings_only = |reply: &Reply, user: u64| {
        reply
            .mentions
            .as_ref()
            .is_some_and(|m| m["users"] == json!([user.to_string()]) && m["parse"] == json!([]))
    };

    let (options, users) = friend(b, "Bea");
    let asked = sim.command_resolved(a, "friends", options, users).await;
    assert_eq!(asked.content(), mention(b));
    assert!(pings_only(&asked, b), "{:?}", asked.mentions);
    let (options, users) = friend(b, "Bea");
    let again = sim.command_resolved(a, "friends", options, users).await;
    assert!(
        again.ephemeral && again.content().contains("You just asked") && again.content().contains(&mention(b)),
        "{}",
        again.content()
    );
    // The wait is for the pair: somebody else can still be asked, and so can a request the other way.
    let (options, users) = friend(c, "Cy");
    assert!(
        !sim.command_resolved(a, "friends", options, users).await.ephemeral,
        "a different player is not held back"
    );
    let (options, users) = friend(a, "Sim");
    assert!(
        !sim.command_resolved(b, "friends", options, users).await.ephemeral,
        "nor is the other way round"
    );

    // Partners: the same minute.
    let propose = |to: u64, name: &str| {
        (
            partner_options("propose", vec![user_option("user", to)]),
            named(&[(to, name)]),
        )
    };
    let (d, e) = (unique(), unique());
    let (options, users) = propose(e, "Eli");
    let asked = sim.command_resolved(d, "partner", options, users).await;
    assert_eq!(asked.content(), mention(e));
    assert!(pings_only(&asked, e), "{:?}", asked.mentions);
    let (options, users) = propose(e, "Eli");
    let again = sim.command_resolved(d, "partner", options, users).await;
    assert!(
        again.ephemeral && again.content().contains("You just asked"),
        "{}",
        again.content()
    );

    // A hug names both players but pings the one who receives it.
    let (sender, receiver) = (unique(), unique());
    let hug = sim
        .command_resolved(
            sender,
            "hug",
            json!([user_option("user", receiver)]),
            named(&[(receiver, "Bea")]),
        )
        .await;
    assert!(hug.content().contains(&mention(sender)) && hug.content().contains(&mention(receiver)));
    assert!(pings_only(&hug, receiver), "{:?}", hug.mentions);
}

#[tokio::test]
async fn a_friend_nobody_remembers_still_has_a_name_when_their_house_is_locked() {
    let Some(mut sim) = Sim::new("a_friend_nobody_remembers_still_has_a_name_when_their_house_is_locked").await else {
        return;
    };
    let (visitor, host) = (unique(), unique());
    sim.command(host, "house", json!([sub("lock", vec![bool_option("locked", true)])]))
        .await;
    sqlx::query("DELETE FROM social_names WHERE user_id = $1")
        .bind(host as i64)
        .execute(sim.app.db.pool())
        .await
        .unwrap();
    let on = sim.command(visitor, "ping", json!([])).await;
    let reply = sim.component(visitor, "fd:v:0", &[&host.to_string()], &on).await;
    assert!(
        reply.content().ends_with("has locked their house.") && !reply.content().starts_with(' '),
        "{:?}",
        reply.content()
    );
    assert!(reply.content().starts_with("A friend"), "{}", reply.content());
}

// /Fish shop

#[tokio::test]
async fn the_tackle_shop_says_the_lures_are_for_hunting_and_not_for_income() {
    let Some(mut sim) = Sim::new("the_tackle_shop_says_the_lures_are_for_hunting_and_not_for_income").await else {
        return;
    };
    let angler = unique();
    let shop = sim.command(angler, "fish", json!([sub("shop", vec![])])).await;
    assert!(
        shop.description().contains("not for income") && shop.description().contains("Silver Lure"),
        "{}",
        shop.description()
    );
    let menu = shop.options("fx:p:");
    for (value, note) in [("b:silver_lure", "loses shells"), ("b:tide_crumbs", "cuts your wage")] {
        let option = menu.iter().find(|o| o.value == value).expect(value);
        let description = option.description.as_deref().unwrap_or_default();
        assert!(
            description.contains("not income") && description.contains(note),
            "{value}: {description}"
        );
        assert!(description.chars().count() < 100, "{value}: it fits the menu whole");
    }
}

// /Pet care

#[tokio::test]
async fn a_care_click_that_lost_the_race_gives_its_cooldown_back() {
    let Some(mut sim) = Sim::new("a_care_click_that_lost_the_race_gives_its_cooldown_back").await else {
        return;
    };
    let Some(mut other) = Sim::new("a_care_click_that_lost_the_race_gives_its_cooldown_back_2").await else {
        return;
    };
    let user = unique();
    let db = sim.app.db.clone();
    let pet = db.adopt(user, "crumbclaw_crab", "Pip").await.unwrap();
    let card = sim.command(user, "pet", pet_command("view")).await;
    let mut refused = false;
    // Two clicks of different kinds read the same pet; one write lands and the other is told the pet
    // changed. Which pair of reads and writes interleaves like that is up to the scheduler, so ask a few times.
    for round in 0..15 {
        set_pet(&db, user, pet.id, fresh_needs(40), 2, 10, sim.app.now())
            .await
            .unwrap();
        for action in ["play", "clean"] {
            sim.app
                .cache
                .release_cooldown(&format!(
                    "pet:{}:{}",
                    pet.id,
                    if action == "play" { "Play" } else { "Clean" }
                ))
                .await
                .unwrap();
        }
        let (play_id, clean_id) = (format!("pc:play:{}", pet.id), format!("pc:clean:{}", pet.id));
        let (play, clean) = tokio::join!(
            sim.component(user, &play_id, &[], &card),
            other.component(user, &clean_id, &[], &card),
        );
        let lost = [("Play", &play), ("Clean", &clean)]
            .into_iter()
            .find(|(_, reply)| reply.content().contains("changed while you clicked"));
        if let Some((action, _)) = lost {
            refused = true;
            // Nothing was cared for, so the cooldown it started is not kept: the next try is not held back.
            let key = format!("pet:{}:{action}", pet.id);
            assert!(
                sim.app
                    .cache
                    .try_cooldown(&key, std::time::Duration::from_secs(5))
                    .await
                    .unwrap()
                    .is_none(),
                "round {round}: the refused {action} click gave its cooldown back"
            );
            sim.app.cache.release_cooldown(&key).await.unwrap();
            break;
        }
    }
    assert!(refused, "no round had a click lose the race");
}

// /Customize showcase

#[tokio::test]
async fn a_soulbound_piece_is_told_from_its_unbound_twin_in_the_showcase_menu() {
    let Some(mut sim) = Sim::new("a_soulbound_piece_is_told_from_its_unbound_twin_in_the_showcase_menu").await else {
        return;
    };
    let user = unique();
    let lamp = Content::get()
        .items()
        .find(|i| i.base.id.contains("lamp") && i.variant.is_none() && !i.bound)
        .expect("a lamp in the catalog");
    let (plain_id, bound_id) = (lamp.id(), pinchora_core::items::soulbound(&lamp.id()));
    sim.app.db.grant_item(user, &plain_id, 1).await.unwrap();
    sim.app.db.grant_item(user, &bound_id, 1).await.unwrap();
    let editor = sim.command(user, "customize", json!([sub("showcase", vec![])])).await;
    let furniture = sim.pick(user, &editor, "sw:k", "i").await;
    let options = furniture.options("sw:s:i");
    let notes: Vec<(&str, &str)> = options
        .iter()
        .filter(|o| o.value == plain_id || o.value == bound_id)
        .map(|o| (o.value.as_str(), o.description.as_deref().unwrap_or_default()))
        .collect();
    assert_eq!(notes.len(), 2, "{notes:?}");
    let bound = notes
        .iter()
        .find(|(value, _)| value.ends_with("~bound"))
        .expect("the bound twin");
    let plain = notes
        .iter()
        .find(|(value, _)| !value.ends_with("~bound"))
        .expect("the plain one");
    assert!(bound.1.ends_with("Soulbound"), "{bound:?}");
    assert!(!plain.1.contains("Soulbound"), "{plain:?}");
}

// /Critterdrop

#[tokio::test]
async fn a_chat_spawn_waits_while_the_channel_has_an_open_drop() {
    let Some(mut sim) = Sim::new("a_chat_spawn_waits_while_the_channel_has_an_open_drop").await else {
        return;
    };
    let _turn = super::collection::COLLECTION.lock().await;
    let channel = unique();
    sim.channel = channel;
    sim.guild = unique();
    sim.app
        .db
        .set_collection_settings(sim.guild, Some(true), Some(None))
        .await
        .expect("settings");
    let host = unique();
    let called = sim.command(host, "critterdrop", json!([])).await;
    assert_eq!(called.title(), "🦀 Critters washed up!");
    let _ = sim.posts();

    // Plenty of talk, from plenty of people, while the called drop still waits: nothing lands on top of it.
    for _ in 0..30 {
        sim.say_in(unique(), channel, &[]).await;
    }
    assert!(sim.posts().is_empty(), "no second drop while the first is open");
    let (open,): (i64,) =
        sqlx::query_as("SELECT count(*) FROM collection_drops WHERE channel_id = $1 AND status = 'open'")
            .bind(channel as i64)
            .fetch_one(sim.app.db.pool())
            .await
            .unwrap();
    assert_eq!(open, 1);

    // Once it has drifted off, the talk that went on meanwhile spawns the next one at the next message.
    sim.app.shift_clock(chrono::Duration::hours(1));
    sim.say_in(unique(), channel, &[]).await;
    let posts = sim.posts();
    only(&posts, channel, Method::POST);
}

// Forged ids

#[tokio::test]
async fn a_forged_custom_id_cannot_act_on_another_kind_of_game() {
    let _serial = SWEEPS.lock().await;
    let Some(mut sim) = Sim::new("a_forged_custom_id_cannot_act_on_another_kind_of_game").await else {
        return;
    };
    forget_late_games(&sim).await;
    let (a, b) = (unique(), unique());
    fund(&sim, a, 300).await;
    fund(&sim, b, 300).await;
    // A wagered connect four, running.
    let challenge = duel(&mut sim, a, b, "connect4", vec![int_option("bet", 100)]).await;
    let board = sim.click(b, &challenge, "Accept").await;
    let id = session_of(&board);
    assert_eq!((shells(&sim, a).await, shells(&sim, b).await), (200, 200));

    // Every other game's buttons, with this game's id: each says the game is over and touches nothing.
    for (forged, value) in [
        (format!("rp:o:{id}"), "a1"),
        (format!("rp:m:{id}:1:r"), "a1"),
        (format!("bs:R:{id}"), "a1"),
        (format!("bs:o:{id}"), "a1"),
        (format!("ck:R:{id}"), "9"),
        (format!("cc:R:{id}"), "a1"),
        (format!("ck:s:{id}:0"), "9"),
        (format!("cc:s:{id}:0"), "a1"),
    ] {
        let reply = sim.component(a, &forged, &[value], &board).await;
        assert!(reply.content().contains("is over"), "{forged}: {}", reply.content());
    }
    let game = sim.app.db.game(id).await.unwrap().expect("the game");
    assert_eq!(
        (game.status.as_str(), game.kind.as_str()),
        ("active", "connect4"),
        "nobody resigned it from another game's button"
    );
    assert_eq!((shells(&sim, a).await, shells(&sim, b).await), (200, 200));

    // And the other way round: a connect four button on a running match of rock paper scissors.
    let (c, d) = (unique(), unique());
    fund(&sim, c, 300).await;
    fund(&sim, d, 300).await;
    let challenge = duel(
        &mut sim,
        c,
        d,
        "rps",
        vec![int_option("bet", 50), int_option("rounds", 1)],
    )
    .await;
    let board = sim.click(d, &challenge, "Accept").await;
    let rps = session_of(&board);
    let before = sim.app.db.game(rps).await.unwrap().expect("the match");
    let reply = sim.component(c, &format!("c4:m:{rps}:3"), &[], &board).await;
    assert!(reply.content().contains("is over"), "{}", reply.content());
    let after = sim.app.db.game(rps).await.unwrap().expect("the match");
    assert_eq!(
        (after.status.as_str(), after.state.as_str()),
        (before.status.as_str(), before.state.as_str()),
        "the match is as it was"
    );
    expire(&sim, id).await;
    expire(&sim, rps).await;
    sweep_games(&sim.app).await.expect("sweep");
}

// Voice XP at scale

/// A GUILD_CREATE for this server listing the channels and threads as (id, type, parent).
fn guild_with_channels(sim: &Sim, channels: &[(u64, u8, Option<u64>)]) -> GuildCreate {
    let mut guild = guild_json(sim.guild);
    let list = |threads: bool| -> Vec<Value> {
        channels
            .iter()
            .filter(|(_, kind, _)| (*kind == 11) == threads)
            .map(|(id, kind, parent)| {
                json!({
                    "id": id.to_string(),
                    "type": kind,
                    "guild_id": sim.guild.to_string(),
                    "name": "listed",
                    "parent_id": parent.map(|p| p.to_string()),
                })
            })
            .collect()
    };
    guild["channels"] = list(false).into();
    guild["threads"] = list(true).into();
    serde_json::from_value(guild).expect("guild create JSON")
}

#[tokio::test]
async fn guild_rebuilds_have_slots_of_their_own() {
    let Some(sim) = Sim::new("guild_rebuilds_have_slots_of_their_own").await else {
        return;
    };
    // Every slot events may use is taken by an event that is waiting for something.
    let (release, held) = tokio::sync::watch::channel(false);
    for _ in 0..1024 {
        let mut held = held.clone();
        sim.app.spawn_event("test", async move {
            let _ = held.wait_for(|released| *released).await;
        });
    }
    let (told, heard) = oneshot::channel();
    sim.app.spawn_event("test", async move {
        let _ = told.send(());
    });
    assert!(
        matches!(
            tokio::time::timeout(std::time::Duration::from_millis(300), heard).await,
            Ok(Err(_))
        ),
        "the event slots are all taken: a 1025th event is dropped, its sender with it"
    );

    // A rebuild runs all the same, and no more than a handful at a time do.
    let (done, ran) = oneshot::channel();
    sim.app.spawn_rebuild(async move {
        let _ = done.send(());
    });
    tokio::time::timeout(std::time::Duration::from_secs(2), ran)
        .await
        .expect("the rebuild was not held up by the full event slots")
        .expect("the rebuild ran");
    let (running, most, finished) = (
        Arc::new(AtomicUsize::new(0)),
        Arc::new(AtomicUsize::new(0)),
        Arc::new(AtomicUsize::new(0)),
    );
    for _ in 0..60 {
        let (running, most, finished) = (running.clone(), most.clone(), finished.clone());
        sim.app.spawn_rebuild(async move {
            let now = running.fetch_add(1, Ordering::SeqCst) + 1;
            most.fetch_max(now, Ordering::SeqCst);
            tokio::time::sleep(std::time::Duration::from_millis(15)).await;
            running.fetch_sub(1, Ordering::SeqCst);
            finished.fetch_add(1, Ordering::SeqCst);
        });
    }
    for _ in 0..200 {
        if finished.load(Ordering::SeqCst) == 60 {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    }
    assert_eq!(finished.load(Ordering::SeqCst), 60, "every queued rebuild ran");
    let most = most.load(Ordering::SeqCst);
    assert!(
        (2..=16).contains(&most),
        "at most 16 at once, and they do overlap: {most}"
    );
    release.send(true).expect("let the events go");
}

#[tokio::test]
async fn a_leave_or_join_that_came_in_while_a_rebuild_waited_is_not_undone_by_its_snapshot() {
    let Some(mut sim) =
        Sim::new("a_leave_or_join_that_came_in_while_a_rebuild_waited_is_not_undone_by_its_snapshot").await
    else {
        return;
    };
    let _turn = VOICE_TICKS.lock().await;
    sim.app.shift_clock(chrono::Duration::days((unique() % 36_500) as i64));
    let admin = unique();
    let lounge = unique();
    let (a, b, c) = (unique(), unique(), unique());
    sim.permissions = MANAGE_GUILD;
    sim.xp(
        admin,
        sub("voice", vec![bool_option("enabled", true), int_option("amount", 20)]),
    )
    .await;

    // The GUILD_CREATE says A and B are in the lounge, and is turned into its snapshot at once...
    let event = sim.guild_create(
        &[(a, lounge, false, false), (b, lounge, false, false)],
        &[(a, false, &[]), (b, false, &[])],
        None,
    );
    let snapshot = pinchora_commands::voice_snapshot(&sim.app, event).expect("a server");
    // ...but the rebuild waits for its turn, and for the database. Meanwhile B leaves and C joins.
    sim.voice_in(b, None, &[], false, false, false).await;
    sim.voice_in(c, Some(lounge), &[], false, false, false).await;
    pinchora_commands::rebuild_voice(&sim.app, snapshot)
        .await
        .expect("rebuild");

    let mut now = vec![a, c];
    now.sort_unstable();
    assert_eq!(
        sim.voice_members().await,
        now,
        "B's leave stands, C's join stands, A comes from the snapshot"
    );
    sim.app.shift_clock(chrono::Duration::seconds(60));
    pinchora_commands::voice_tick(&sim.app).await.expect("tick");
    assert_eq!((sim.server_xp(a).await, sim.server_xp(c).await), (20, 20));
    assert_eq!(
        sim.server_xp(b).await,
        0,
        "B left before the snapshot was applied: no minute for B"
    );

    // A leave is a note, not a hole: a later snapshot, taken after it, is the truth again.
    sim.app.shift_clock(chrono::Duration::seconds(5));
    sim.snapshot(&[(b, lounge, false, false)], &[(b, false, &[])], None)
        .await;
    assert_eq!(sim.voice_members().await, vec![b]);
}

#[tokio::test]
async fn a_pod_with_old_settings_cannot_write_who_is_in_voice_after_it_was_switched_off() {
    let Some(mut sim) =
        Sim::new("a_pod_with_old_settings_cannot_write_who_is_in_voice_after_it_was_switched_off").await
    else {
        return;
    };
    let Some(mut other) =
        Sim::new("a_pod_with_old_settings_cannot_write_who_is_in_voice_after_it_was_switched_off_2").await
    else {
        return;
    };
    let _turn = VOICE_TICKS.lock().await;
    sim.app.shift_clock(chrono::Duration::days((unique() % 36_500) as i64));
    other.guild = sim.guild;
    let (admin, lounge, a) = (unique(), unique(), unique());
    sim.permissions = MANAGE_GUILD;
    let voice = |enabled: bool| sub("voice", vec![bool_option("enabled", enabled), int_option("amount", 20)]);
    sim.xp(admin, voice(true)).await;
    // The other pod has read the settings with voice XP on.
    assert!(other.app.guild_settings(sim.guild).await.unwrap().voice_xp_enabled);

    sim.xp(admin, voice(false)).await;
    assert!(
        other.app.guild_settings(sim.guild).await.unwrap().voice_xp_enabled,
        "its view is old: it has not heard of the switch"
    );
    other.voice_in(a, Some(lounge), &[], false, false, false).await;
    assert!(
        sim.voice_members().await.is_empty(),
        "an event handled by a pod with the old view writes nothing"
    );
    let event = other.guild_create(&[(a, lounge, false, false)], &[(a, false, &[])], None);
    pinchora_commands::on_guild_create(other.app.clone(), event)
        .await
        .expect("rebuild");
    assert!(sim.voice_members().await.is_empty(), "nor does its rebuild");

    // Switched on again, the same event is written down.
    sim.xp(admin, voice(true)).await;
    other.voice_in(a, Some(lounge), &[], false, false, false).await;
    assert_eq!(sim.voice_members().await, vec![a]);
}

#[tokio::test]
async fn a_settings_change_reaches_the_other_pods_through_redis() {
    let Some(mut sim) = Sim::new("a_settings_change_reaches_the_other_pods_through_redis").await else {
        return;
    };
    let Some(mut other) = Sim::new("a_settings_change_reaches_the_other_pods_through_redis_2").await else {
        return;
    };
    other.guild = sim.guild;
    pinchora_commands::listen_for_invalidations(&other.app).await;
    let admin = unique();
    sim.permissions = MANAGE_GUILD;
    // The other pod has the settings and the voice view cached, with leveling on.
    assert!(other.app.guild_settings(sim.guild).await.unwrap().leveling_enabled);
    sim.command(
        admin,
        "config",
        json!([sub("leveling", vec![bool_option("enabled", false)])]),
    )
    .await;
    let mut heard = false;
    for _ in 0..100 {
        if !other.app.guild_settings(sim.guild).await.unwrap().leveling_enabled {
            heard = true;
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(30)).await;
    }
    assert!(heard, "the other pod forgot its copy as soon as it was told");

    // Multipliers too.
    let everyone = role_json(sim.guild, "@everyone", 0, false);
    assert!(other.app.xp_rules(sim.guild).await.unwrap().roles.is_empty());
    sim.xp(
        admin,
        sub(
            "multiplier",
            vec![number_option("value", 2.0), role_option("role", &everyone)],
        ),
    )
    .await;
    let mut heard = false;
    for _ in 0..100 {
        if !other.app.xp_rules(sim.guild).await.unwrap().roles.is_empty() {
            heard = true;
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(30)).await;
    }
    assert!(heard, "the other pod's XP rules were forgotten too");
}

#[tokio::test]
async fn channel_places_are_prefilled_remembered_when_missing_and_kept() {
    let Some(mut sim) = Sim::new("channel_places_are_prefilled_remembered_when_missing_and_kept").await else {
        return;
    };
    let admin = unique();
    let (category, text, thread, unknown, later) = (unique(), unique(), unique(), unique(), unique());
    sim.permissions = MANAGE_GUILD;
    // A server with a channel rule needs to know where channels sit; the category doubles XP.
    sim.channel_is(category, 4, None);
    sim.command_resolved(
        admin,
        "config",
        json!([group(
            "xp",
            sub(
                "multiplier",
                vec![number_option("value", 2.0), channel_option("channel", category)]
            )
        )]),
        resolved_channels(&[category]),
    )
    .await;
    // Nothing is known until the GUILD_CREATE lists the channels and the thread; then Discord is not asked.
    assert!(!sim.app.knows_channel_place(thread));
    let event = guild_with_channels(
        &sim,
        &[(category, 4, None), (text, 0, Some(category)), (thread, 11, Some(text))],
    );
    on_guild_create(sim.app.clone(), event).await.expect("handler");
    assert!(sim.app.knows_channel_place(thread) && sim.app.knows_channel_place(text));
    let user = unique();
    sim.say_in(user, thread, &[]).await;
    assert_eq!(
        sim.server_xp(user).await,
        sim.global_xp(user).await * 2,
        "a thread follows its channel's category"
    );
    assert_eq!(
        (
            sim.channel_lookups(thread),
            sim.channel_lookups(text),
            sim.channel_lookups(category)
        ),
        (0, 0, 0),
        "everything was in the GUILD_CREATE"
    );

    // A channel Discord has never heard of is asked about once, not on every message.
    for _ in 0..3 {
        sim.say_in(unique(), unknown, &[]).await;
    }
    assert_eq!(sim.channel_lookups(unknown), 1, "a miss is remembered");

    // A channel made after the GUILD_CREATE is asked about once, and then remembered.
    sim.channel_is(later, 0, Some(category));
    for _ in 0..3 {
        sim.say_in(unique(), later, &[]).await;
    }
    assert_eq!(sim.channel_lookups(later), 1);
    assert!(sim.app.knows_channel_place(later));
}
