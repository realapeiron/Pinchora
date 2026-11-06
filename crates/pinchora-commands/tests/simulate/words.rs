//! The party games, clicked through: Hot Pearl, Acronym Tide, Caption the Postcard, Wordle Race, Would You
//! Rather and Truth or Dare.

use chrono::Duration;
use pinchora_commands::party_tick;
use pinchora_db::PartyGame;
use pinchora_game::party::{pearl_reward, race_reward, writing_reward};

use super::*;

/// The ticker moves on every late game in the database, whoever's it is, so the tests that make party
/// games take turns.
static PARTY: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// The word list Hot Pearl plays by, to find a word for any syllable.
static WORDS: &str = include_str!("../../../../content/pearl_words.txt");

/// A word that holds `syllable`: the first four-letter one, or the first at all.
fn word_with(syllable: &str) -> String {
    WORDS
        .lines()
        .find(|w| w.len() == 4 && w.contains(syllable))
        .or_else(|| WORDS.lines().find(|w| w.contains(syllable)))
        .expect("a word for the syllable")
        .to_owned()
}

/// A word that starts with `letter`, for building phrases.
fn word_for(letter: char) -> &'static str {
    match letter {
        'A' => "apples",
        'B' => "bake",
        'C' => "crabs",
        'D' => "dance",
        'E' => "eat",
        'F' => "fish",
        'G' => "giggle",
        'H' => "hum",
        'I' => "ice",
        'J' => "jump",
        'K' => "kind",
        'L' => "love",
        'M' => "mighty",
        'N' => "nap",
        'O' => "ocean",
        'P' => "pearls",
        'R' => "ride",
        'S' => "sing",
        'T' => "tiny",
        'U' => "under",
        'V' => "very",
        'W' => "waves",
        'Y' => "yawn",
        other => panic!("no word for {other}"),
    }
}

fn phrase(letters: &str) -> String {
    letters.chars().map(word_for).collect::<Vec<_>>().join(" ")
}

/// The sim's database is its own, so a test starts from no games at all: whatever an earlier run left
/// behind would be moved on by this test's clock.
async fn fresh(sim: &Sim) {
    sqlx::query("DELETE FROM party_games")
        .execute(sim.app.db.pool())
        .await
        .expect("clear the games");
}

/// The id of the game a message belongs to, from its buttons (`hl:j:<id>`, `ac:w:<id>:<round>`, ...).
fn game_id(reply: &Reply) -> i64 {
    reply
        .controls()
        .find_map(|c| match c {
            Component::Button(b) => b.custom_id.as_deref()?.split(':').nth(2)?.parse().ok(),
            _ => None,
        })
        .expect("a button with a game id")
}

async fn game(sim: &Sim, id: i64) -> PartyGame {
    sim.app.db.party(id).await.expect("game").expect("the game exists")
}

/// The game's own state, as the JSON the database keeps.
async fn state(sim: &Sim, id: i64) -> Value {
    serde_json::from_str(&game(sim, id).await.state).expect("state is JSON")
}

/// Tells the game which message it is shown in, as the follow-up lookup would.
async fn pin(sim: &Sim, id: i64) {
    sim.app
        .db
        .set_party_message(id, sim.channel, unique())
        .await
        .expect("pin the message");
}

/// Moves the clock on and runs the ticker once; how many games it moved.
async fn advance(sim: &Sim, secs: i64) -> usize {
    sim.app.shift_clock(Duration::seconds(secs));
    party_tick(&sim.app).await.expect("the party ticker")
}

/// The messages the handlers edited, oldest first.
fn edits(posts: &[Post]) -> Vec<&Post> {
    posts.iter().filter(|p| p.method == Method::PATCH).collect()
}

fn description(post: &Post) -> &str {
    post.embed()["description"].as_str().unwrap_or_default()
}

/// Keeps a picture a handler edited in, next to the sim's own, under a name that says what it shows.
fn keep(sim: &Sim, name: &str, post: &Post) {
    let (_, png) = post.files.first().expect("the edit carried a picture");
    std::fs::write(sim.dir.join(format!("{name}.png")), png).expect("save the picture");
}

/// Opens a lobby with `command` (`hotpearl`, `acronym`, ...), and has everybody else join it.
async fn lobby_of(sim: &mut Sim, command: &str, options: Vec<Value>, host: u64, joiners: &[u64]) -> (Reply, i64) {
    let mut lobby = sim.command(host, "party", json!([sub(command, options)])).await;
    assert!(!lobby.ephemeral, "{}", lobby.content());
    let id = game_id(&lobby);
    pin(sim, id).await;
    for user in joiners {
        lobby = sim.click(*user, &lobby, "Join").await;
    }
    (lobby, id)
}

async fn balance(sim: &Sim, user: u64) -> i64 {
    sim.app.db.user(user).await.expect("user row").credits
}

#[tokio::test]
async fn hot_pearl_goes_from_lobby_to_a_paid_winner() {
    let _alone = PARTY.lock().await;
    let Some(mut sim) = Sim::new("hot_pearl_goes_from_lobby_to_a_paid_winner").await else {
        return;
    };
    fresh(&sim).await;
    sim.channel = unique();
    let (host, ann, bo) = (unique(), unique(), unique());

    let lobby = sim
        .command(host, "party", json!([sub("hotpearl", vec![int_option("lives", 1)])]))
        .await;
    assert!(!lobby.ephemeral && lobby.pings_nobody());
    assert!(lobby.title().contains("Hot Pearl"), "{}", lobby.title());
    assert_eq!(lobby.files.len(), 1);
    assert_eq!(lobby.files[0].0, "lobby.png");
    let id = game_id(&lobby);
    pin(&sim, id).await;

    // One game at a time in a channel; too few players; only the host starts.
    let busy = sim.command(ann, "party", json!([sub("acronym", vec![])])).await;
    assert!(
        busy.ephemeral && busy.content().contains("already going"),
        "{}",
        busy.content()
    );
    let early = sim.click(host, &lobby, "Start").await;
    assert!(
        early.ephemeral && early.content().contains("at least 2"),
        "{}",
        early.content()
    );
    let lobby = sim.click(ann, &lobby, "Join").await;
    assert!(
        !lobby.ephemeral && lobby.content().contains("(2/8)"),
        "{}",
        lobby.content()
    );
    let twice = sim.click(ann, &lobby, "Join").await;
    assert!(
        twice.ephemeral && twice.content().contains("already in"),
        "{}",
        twice.content()
    );
    let nope = sim.click(ann, &lobby, "Start").await;
    assert!(nope.ephemeral && nope.content().contains("Only"), "{}", nope.content());
    let lobby = sim.click(bo, &lobby, "Join").await;
    let left = sim.click(bo, &lobby, "Leave").await;
    assert!(left.content().contains("(2/8)"), "{}", left.content());
    let lobby = sim.click(bo, &left, "Join").await;
    assert!(lobby.content().contains("(3/8)"), "{}", lobby.content());
    let stuck = sim.click(host, &lobby, "Leave").await;
    assert!(
        stuck.ephemeral && stuck.content().contains("Cancel"),
        "{}",
        stuck.content()
    );

    let table = sim.click(host, &lobby, "Start").await;
    assert_eq!(table.files[0].0, "pearl.png");
    assert!(
        table.description().contains("type a word with"),
        "{}",
        table.description()
    );
    let st = state(&sim, id).await;
    let cur = st["cur"].as_u64().expect("a holder");
    let syllable = st["syl"].as_str().expect("a syllable").to_owned();
    assert_eq!(syllable.len(), 2, "the first turns are gentle");
    let others: Vec<u64> = [host, ann, bo].into_iter().filter(|u| *u != cur).collect();

    // Only the holder can type; their form takes real words with the syllable and nothing else.
    let wrong = sim.click(others[0], &table, "Type a word").await;
    assert!(
        wrong.ephemeral && wrong.content().contains("turn"),
        "{}",
        wrong.content()
    );
    let form = sim.click(cur, &table, "Type a word").await;
    assert!(form.modal.is_some(), "the holder gets a form");
    let short = sim.submit_fields(cur, &form, &[("word", "ab")]).await;
    assert!(
        short.ephemeral && short.content().contains("at least 3"),
        "{}",
        short.content()
    );
    let without = ["pearl", "ocean", "tiger", "crab"]
        .into_iter()
        .find(|w| !w.contains(syllable.as_str()))
        .expect("a word without the syllable");
    let missing = sim.submit_fields(cur, &form, &[("word", without)]).await;
    assert!(
        missing.ephemeral && missing.content().contains("doesn't contain"),
        "{}",
        missing.content()
    );
    let fake = format!("zq{syllable}qz");
    let unknown = sim.submit_fields(cur, &form, &[("word", fake.as_str())]).await;
    assert!(
        unknown.ephemeral && unknown.content().contains("word list"),
        "{}",
        unknown.content()
    );
    // The list holds mild words; a strict server turns them away. "ell" is put on the pearl for this.
    let syl = "UPDATE party_games SET state = jsonb_set(state::jsonb, '{syl}', to_jsonb($2::text))::text WHERE id = $1";
    sqlx::query(syl)
        .bind(id)
        .bind("ell")
        .execute(sim.app.db.pool())
        .await
        .expect("syllable");
    let config = |on: bool| json!([sub("partygames", vec![bool_option("strict", on)])]);
    assert!(sim.command(host, "config", config(true)).await.ephemeral);
    let mild = sim.submit_fields(cur, &form, &[("word", "hell")]).await;
    assert!(
        mild.ephemeral && mild.content().contains("friendly"),
        "{}",
        mild.content()
    );
    assert!(sim.command(host, "config", config(false)).await.ephemeral);
    sqlx::query(syl)
        .bind(id)
        .bind(&syllable)
        .execute(sim.app.db.pool())
        .await
        .expect("syllable");
    let good = word_with(&syllable);
    let played = sim.submit_fields(cur, &form, &[("word", good.as_str())]).await;
    assert!(!played.ephemeral, "{}", played.content());
    assert_eq!(played.files[0].0, "pearl.png");
    assert!(
        played.description().contains(&good.to_uppercase()),
        "{}",
        played.description()
    );
    let st = state(&sim, id).await;
    assert_eq!(st["turn"], 1);
    let second = st["cur"].as_u64().expect("the next holder");
    assert_ne!(second, cur);
    // The word that was played stays played, and the old form can not be used again.
    let stale = sim.submit_fields(cur, &form, &[("word", good.as_str())]).await;
    assert!(stale.ephemeral, "{}", stale.content());

    // Time runs out: the pearl pops, the holder (one life) is out, and the next player is up.
    sim.posts();
    assert_eq!(advance(&sim, 60).await, 1);
    let posts = sim.posts();
    let popped = edits(&posts);
    assert_eq!(popped.len(), 1, "one edit for the pop");
    assert!(
        description(popped[0]).contains("popped on"),
        "{}",
        description(popped[0])
    );
    assert!(description(popped[0]).contains("is out"), "{}", description(popped[0]));
    keep(&sim, "pearl-after-the-first-pop", popped[0]);
    let st = state(&sim, id).await;
    assert_eq!(st["turn"], 2);
    let third = st["cur"].as_u64().expect("the last holder");
    assert_ne!(third, second, "the player who is out can not hold the pearl");
    let players = sim.app.db.party_players(id).await.expect("players");
    assert_eq!(players.iter().filter(|p| p.eliminated).count(), 1);

    // The next pop leaves one player standing, who wins and is paid.
    assert_eq!(advance(&sim, 60).await, 1);
    let posts = sim.posts();
    let ended = edits(&posts);
    assert_eq!(ended.len(), 1);
    assert!(ended[0].title().contains("is over"), "{}", ended[0].title());
    assert_eq!(ended[0].files[0].0, "standings.png");
    keep(&sim, "pearl-standings", ended[0]);
    let players = sim.app.db.party_players(id).await.expect("players");
    let winner = players.iter().find(|p| !p.eliminated).expect("one player is left").id();
    assert_eq!(balance(&sim, winner).await, pearl_reward(3));
    assert_eq!(record_of(&sim, winner, "hotpearl").await, (1, 1));
    for loser in [host, ann, bo].into_iter().filter(|u| *u != winner) {
        assert_eq!(record_of(&sim, loser, "hotpearl").await, (1, 0));
        assert_eq!(balance(&sim, loser).await, 0);
    }
    assert!(game(&sim, id).await.is_done());
    // Nothing more happens, and the channel is free for the next game.
    assert_eq!(advance(&sim, 600).await, 0);
    let stale = sim.click(winner, &table, "Type a word").await;
    assert!(stale.ephemeral, "{}", stale.content());
    let next = sim.command(host, "party", json!([sub("wordrace", vec![])])).await;
    assert!(!next.ephemeral, "{}", next.content());
}

#[tokio::test]
async fn a_click_after_the_clock_pops_the_pearl_instead_of_playing_on() {
    let _alone = PARTY.lock().await;
    let Some(mut sim) = Sim::new("a_click_after_the_clock_pops_the_pearl_instead_of_playing_on").await else {
        return;
    };
    fresh(&sim).await;
    sim.channel = unique();
    let (host, ann) = (unique(), unique());
    let (lobby, id) = lobby_of(&mut sim, "hotpearl", vec![], host, &[ann]).await;
    let table = sim.click(host, &lobby, "Start").await;
    let st = state(&sim, id).await;
    assert_eq!(st["lives"], 2, "two lives unless the host says otherwise");
    let cur = st["cur"].as_u64().expect("a holder");

    sim.app.shift_clock(Duration::seconds(60));
    sim.posts();
    let late = sim.click(cur, &table, "Type a word").await;
    assert!(late.ephemeral && late.modal.is_none(), "no form after the clock");
    assert!(late.content().contains("Too slow"), "{}", late.content());
    let posts = sim.posts();
    let popped = edits(&posts);
    assert_eq!(popped.len(), 1);
    assert!(
        description(popped[0]).contains("1 life left"),
        "{}",
        description(popped[0])
    );
    let players = sim.app.db.party_players(id).await.expect("players");
    assert!(players.iter().all(|p| !p.eliminated));
    assert_eq!(players.iter().find(|p| p.id() == cur).map(|p| p.lives), Some(1));
    assert_eq!(state(&sim, id).await["turn"], 1);
    // The ticker finds nothing left to do for that turn.
    assert_eq!(party_tick(&sim.app).await.unwrap(), 0);
}

#[tokio::test]
async fn a_lobby_can_be_cancelled_or_left_to_expire() {
    let _alone = PARTY.lock().await;
    let Some(mut sim) = Sim::new("a_lobby_can_be_cancelled_or_left_to_expire").await else {
        return;
    };
    fresh(&sim).await;
    sim.channel = unique();
    let (host, ann) = (unique(), unique());
    let (lobby, id) = lobby_of(&mut sim, "acronym", vec![int_option("rounds", 2)], host, &[ann]).await;
    assert_eq!(game(&sim, id).await.rounds, 2);
    let denied = sim.click(ann, &lobby, "Cancel").await;
    assert!(
        denied.ephemeral && denied.content().contains("Only"),
        "{}",
        denied.content()
    );
    let cancelled = sim.click(host, &lobby, "Cancel").await;
    assert!(
        cancelled.content().contains("cancelled") && cancelled.components.is_empty(),
        "{}",
        cancelled.content()
    );
    assert!(game(&sim, id).await.is_done());
    let gone = sim.click(ann, &lobby, "Join").await;
    assert!(
        gone.ephemeral && gone.content().contains("closed"),
        "{}",
        gone.content()
    );

    // Another lobby nobody starts is closed by the clock, with its message.
    let (_, other) = lobby_of(&mut sim, "postcard", vec![], host, &[]).await;
    sim.posts();
    assert_eq!(advance(&sim, 600).await, 1);
    let posts = sim.posts();
    let closed = edits(&posts);
    assert_eq!(closed.len(), 1);
    assert!(
        closed[0].content().contains("Nobody started"),
        "{}",
        closed[0].content()
    );
    assert!(game(&sim, other).await.is_done());
}

#[tokio::test]
async fn acronym_tide_collects_phrases_then_votes_then_pays() {
    let _alone = PARTY.lock().await;
    let Some(mut sim) = Sim::new("acronym_tide_collects_phrases_then_votes_then_pays").await else {
        return;
    };
    fresh(&sim).await;
    sim.channel = unique();
    let (ada, bo, cy) = (unique(), unique(), unique());
    let (lobby, id) = lobby_of(&mut sim, "acronym", vec![int_option("rounds", 1)], ada, &[bo]).await;
    let early = sim.click(ada, &lobby, "Start").await;
    assert!(
        early.ephemeral && early.content().contains("at least 3"),
        "{}",
        early.content()
    );
    let lobby = sim.click(cy, &lobby, "Join").await;
    let round = sim.click(ada, &lobby, "Start").await;
    assert_eq!(round.files[0].0, "letters.png");
    assert!(
        round.description().contains("Build a phrase from"),
        "{}",
        round.description()
    );
    let letters = state(&sim, id).await["letters"].as_str().expect("letters").to_owned();
    assert_eq!(letters.len(), 3);
    let good = phrase(&letters);

    // Phrases are checked: nobody outside, wrong count, wrong letters, links, rude words.
    let stranger = sim.click(unique(), &round, "Write a phrase").await;
    assert!(
        stranger.ephemeral && stranger.content().contains("not in this game"),
        "{}",
        stranger.content()
    );
    let form = sim.click(ada, &round, "Write a phrase").await;
    assert!(form.modal.is_some());
    let few = sim.submit_fields(ada, &form, &[("entry", "just two")]).await;
    assert!(
        few.ephemeral && few.content().contains("needs exactly"),
        "{}",
        few.content()
    );
    let backwards = phrase(&letters.chars().rev().collect::<String>());
    if backwards != good {
        let wrong = sim.submit_fields(ada, &form, &[("entry", backwards.as_str())]).await;
        assert!(
            wrong.ephemeral && wrong.content().contains("has to start with"),
            "{}",
            wrong.content()
        );
    }
    let link = sim
        .submit_fields(ada, &form, &[("entry", "https://example.com/x")])
        .await;
    assert!(
        link.ephemeral && link.content().contains("plain words"),
        "{}",
        link.content()
    );
    let rude = sim.submit_fields(ada, &form, &[("entry", "what the fuck")]).await;
    assert!(
        rude.ephemeral && rude.content().contains("friendly"),
        "{}",
        rude.content()
    );

    // A good phrase is locked in privately, and a rewrite replaces it.
    sim.posts();
    let unfit = sim.submit_fields(ada, &form, &[("entry", "ignored words here")]).await;
    assert!(unfit.ephemeral);
    let locked = sim.submit_fields(ada, &form, &[("entry", good.as_str())]).await;
    assert!(
        locked.ephemeral && locked.content().contains("Locked"),
        "{}",
        locked.content()
    );
    let again = sim.submit_fields(ada, &form, &[("entry", good.as_str())]).await;
    assert!(
        again.ephemeral && again.content().contains("Rewritten"),
        "{}",
        again.content()
    );
    let posts = sim.posts();
    assert!(
        edits(&posts).iter().any(|p| description(p).contains("1 of 3")),
        "the message keeps count of the phrases in"
    );
    let b = sim.submit_fields(bo, &form, &[("entry", good.as_str())]).await;
    assert!(b.ephemeral && b.content().contains("Locked"), "{}", b.content());

    // The last phrase moves it on to the vote, anonymously.
    let c = sim.submit_fields(cy, &form, &[("entry", good.as_str())]).await;
    assert!(c.ephemeral);
    let posts = sim.posts();
    let vote = edits(&posts).into_iter().last().expect("the vote went up");
    assert!(vote.title().contains("vote"), "{}", vote.title());
    assert_eq!(vote.files[0].0, "entries.png");
    keep(&sim, "acronym-vote", vote);
    let ballot = posted_reply(vote);
    let entries = sim.app.db.party_entries(id, 1).await.expect("entries");
    assert_eq!(entries.len(), 3);
    let at = |author: u64| entries.iter().position(|e| e.author() == author).expect("an entry");
    let number = |author: u64| (at(author) + 1).to_string();

    // Votes: not for yourself, changeable, counted once each; the last one ends the vote early.
    let own = sim.click(ada, &ballot, &number(ada)).await;
    assert!(own.ephemeral && own.content().contains("yours"), "{}", own.content());
    let first = sim.click(ada, &ballot, &number(cy)).await;
    assert!(first.content().contains("is in"), "{}", first.content());
    let same = sim.click(ada, &ballot, &number(cy)).await;
    assert!(same.content().contains("already voted"), "{}", same.content());
    let moved = sim.click(ada, &ballot, &number(bo)).await;
    assert!(moved.content().contains("Moved"), "{}", moved.content());
    let outsider = sim.click(unique(), &ballot, &number(bo)).await;
    assert!(
        outsider.content().contains("not in this game"),
        "{}",
        outsider.content()
    );
    sim.click(bo, &ballot, &number(cy)).await;
    sim.posts();
    sim.click(cy, &ballot, &number(bo)).await;
    let posts = sim.posts();
    let reveal = edits(&posts).into_iter().last().expect("the reveal went up");
    assert!(reveal.title().contains("results"), "{}", reveal.title());
    assert!(
        description(reveal).contains("takes the round"),
        "{}",
        description(reveal)
    );
    keep(&sim, "acronym-reveal", reveal);
    // Ada and Cy each got one vote, Bo two: Bo takes the round with 2 votes and the bonus.
    let scores: Vec<(u64, i32)> = sim
        .app
        .db
        .party_players(id)
        .await
        .expect("players")
        .iter()
        .map(|p| (p.id(), p.score))
        .collect();
    assert!(
        scores.contains(&(bo, 2 + 2)) && scores.contains(&(cy, 1)) && scores.contains(&(ada, 0)),
        "{scores:?}"
    );

    // Voting is over: a late click is turned away, and the reveal gives way to the final scores.
    let late = sim.click(ada, &ballot, &number(bo)).await;
    assert!(late.ephemeral, "{}", late.content());
    assert_eq!(advance(&sim, 30).await, 1);
    let posts = sim.posts();
    let end = edits(&posts).into_iter().last().expect("the final scores went up");
    assert!(end.title().contains("is over"), "{}", end.title());
    assert_eq!(end.files[0].0, "standings.png");
    keep(&sim, "acronym-standings", end);
    assert_eq!(balance(&sim, bo).await, writing_reward(4));
    assert_eq!(balance(&sim, cy).await, writing_reward(1));
    assert_eq!(balance(&sim, ada).await, writing_reward(0));
    assert_eq!(record_of(&sim, bo, "acronym").await, (1, 1));
    assert_eq!(record_of(&sim, ada, "acronym").await, (1, 0));
}

#[tokio::test]
async fn acronym_rounds_follow_the_clock_when_nobody_hurries() {
    let _alone = PARTY.lock().await;
    let Some(mut sim) = Sim::new("acronym_rounds_follow_the_clock_when_nobody_hurries").await else {
        return;
    };
    fresh(&sim).await;
    sim.channel = unique();
    let (ada, bo, cy) = (unique(), unique(), unique());
    let (lobby, id) = lobby_of(&mut sim, "acronym", vec![int_option("rounds", 2)], ada, &[bo, cy]).await;
    let round = sim.click(ada, &lobby, "Start").await;
    let letters = state(&sim, id).await["letters"].as_str().expect("letters").to_owned();
    let form = sim.click(ada, &round, "Write a phrase").await;
    sim.submit_fields(ada, &form, &[("entry", phrase(&letters).as_str())])
        .await;
    sim.submit_fields(bo, &form, &[("entry", phrase(&letters).as_str())])
        .await;

    // Time to write ran out with two phrases in: up for a vote. Nobody votes, so nobody scores.
    sim.posts();
    assert_eq!(advance(&sim, 100).await, 1);
    let posts = sim.posts();
    let vote = edits(&posts).into_iter().last().expect("the vote");
    assert!(vote.title().contains("vote"), "{}", vote.title());
    assert_eq!(advance(&sim, 100).await, 1);
    let posts = sim.posts();
    let reveal = edits(&posts).into_iter().last().expect("the reveal");
    assert!(
        description(reveal).contains("Nobody got a vote"),
        "{}",
        description(reveal)
    );
    assert!(description(reveal).contains("Next round"), "{}", description(reveal));

    // Round two: more letters, and only one phrase comes in, so there is nothing to vote on.
    assert_eq!(advance(&sim, 100).await, 1);
    let posts = sim.posts();
    let next = edits(&posts).into_iter().last().expect("round two");
    assert!(next.title().contains("Round 2/2"), "{}", next.title());
    let letters = state(&sim, id).await["letters"].as_str().expect("letters").to_owned();
    assert_eq!(letters.len(), 4);
    let next = posted_reply(next);
    let form = sim.click(cy, &next, "Write a phrase").await;
    sim.submit_fields(cy, &form, &[("entry", phrase(&letters).as_str())])
        .await;
    sim.posts();
    assert_eq!(advance(&sim, 100).await, 1);
    let posts = sim.posts();
    let reveal = edits(&posts).into_iter().last().expect("the reveal");
    assert!(
        description(reveal).contains("Only one phrase"),
        "{}",
        description(reveal)
    );
    assert!(description(reveal).contains("Final scores"), "{}", description(reveal));

    // The last reveal gives way to the final scores: everyone who wrote is paid the base, Bo's... nobody won.
    assert_eq!(advance(&sim, 100).await, 1);
    let posts = sim.posts();
    let end = edits(&posts).into_iter().last().expect("the final scores");
    assert!(
        end.title().contains("is over") && description(end).contains("Nobody scored"),
        "{}",
        description(end)
    );
    for writer in [ada, bo, cy] {
        assert_eq!(
            balance(&sim, writer).await,
            writing_reward(0),
            "everyone wrote at least once"
        );
        assert_eq!(record_of(&sim, writer, "acronym").await, (1, 0));
    }
}

#[tokio::test]
async fn caption_the_postcard_draws_a_scene_and_filters_captions() {
    let _alone = PARTY.lock().await;
    let Some(mut sim) = Sim::new("caption_the_postcard_draws_a_scene_and_filters_captions").await else {
        return;
    };
    fresh(&sim).await;
    sim.channel = unique();
    sim.permissions = MANAGE_GUILD;
    let (ada, bo, cy) = (unique(), unique(), unique());
    let (lobby, id) = lobby_of(&mut sim, "postcard", vec![int_option("rounds", 1)], ada, &[bo, cy]).await;
    let round = sim.click(ada, &lobby, "Start").await;
    assert_eq!(round.files[0].0, "postcard.png");
    let picture = image::load_from_memory(&round.files[0].1).expect("a png");
    assert_eq!((picture.width(), picture.height()), (1040, 700));
    std::fs::write(sim.dir.join("postcard-round.png"), &round.files[0].1).expect("keep the postcard");
    assert!(
        round.description().contains("Write a caption"),
        "{}",
        round.description()
    );
    let form = sim.click(ada, &round, "Write a caption").await;

    // Captions: tidied, no pings or links, a short or rude one is turned away.
    let short = sim.submit_fields(ada, &form, &[("entry", "no")]).await;
    assert!(
        short.ephemeral && short.content().contains("at least 3"),
        "{}",
        short.content()
    );
    let rude = sim.submit_fields(ada, &form, &[("entry", "this is bullshit")]).await;
    assert!(
        rude.ephemeral && rude.content().contains("friendly"),
        "{}",
        rude.content()
    );
    let ok = sim
        .submit_fields(
            ada,
            &form,
            &[("entry", "Hi <@123> @everyone   look https://x.example/y  here")],
        )
        .await;
    assert!(ok.ephemeral && ok.content().contains("Locked"), "{}", ok.content());
    let kept = sim.app.db.party_entries(id, 1).await.expect("entries");
    assert_eq!(kept[0].text, "Hi everyone look here", "mentions and links are gone");

    // A server can ask for the strict filter: mild words that passed before are turned away.
    let mild = sim.submit_fields(bo, &form, &[("entry", "oh damn the crab")]).await;
    assert!(
        mild.ephemeral && mild.content().contains("Locked"),
        "{}",
        mild.content()
    );
    let on = sim
        .command(
            ada,
            "config",
            json!([sub("partygames", vec![bool_option("strict", true)])]),
        )
        .await;
    assert!(on.ephemeral && on.content().contains("strict"), "{}", on.content());
    let shown = sim.command(ada, "config", json!([sub("show", vec![])])).await;
    assert!(
        shown.field("Party games").contains("strict"),
        "{}",
        shown.field("Party games")
    );
    let refused = sim.submit_fields(cy, &form, &[("entry", "oh damn the crab")]).await;
    assert!(
        refused.ephemeral && refused.content().contains("friendly"),
        "{}",
        refused.content()
    );
    let off = sim
        .command(
            ada,
            "config",
            json!([sub("partygames", vec![bool_option("strict", false)])]),
        )
        .await;
    assert!(off.content().contains("normal"), "{}", off.content());
    sim.posts();
    sim.submit_fields(cy, &form, &[("entry", "The crab regrets everything")])
        .await;

    // Three captions are in: the vote shows the postcard above them.
    let posts = sim.posts();
    let vote = edits(&posts).into_iter().last().expect("the vote");
    assert_eq!(vote.files[0].0, "entries.png");
    keep(&sim, "postcard-vote", vote);
    let ballot = posted_reply(vote);
    let entries = sim.app.db.party_entries(id, 1).await.expect("entries");
    let number = |author: u64| (entries.iter().position(|e| e.author() == author).expect("entry") + 1).to_string();
    sim.click(ada, &ballot, &number(cy)).await;
    sim.click(bo, &ballot, &number(cy)).await;
    sim.posts();
    sim.click(cy, &ballot, &number(ada)).await;
    let posts = sim.posts();
    let reveal = edits(&posts).into_iter().last().expect("the reveal");
    assert!(
        description(reveal).contains("takes the round"),
        "{}",
        description(reveal)
    );
    keep(&sim, "postcard-reveal", reveal);
    assert_eq!(advance(&sim, 30).await, 1);
    let end = sim.posts();
    assert!(edits(&end).last().expect("final").title().contains("is over"));
    assert_eq!(balance(&sim, cy).await, writing_reward(4));
}

#[tokio::test]
async fn wordle_race_keeps_boards_private_and_pays_the_first_solver() {
    let _alone = PARTY.lock().await;
    let Some(mut sim) = Sim::new("wordle_race_keeps_boards_private_and_pays_the_first_solver").await else {
        return;
    };
    fresh(&sim).await;
    sim.channel = unique();
    let (ada, bo, cy, outsider) = (unique(), unique(), unique(), unique());
    let (lobby, id) = lobby_of(&mut sim, "wordrace", vec![], ada, &[bo, cy]).await;
    let race = sim.click(ada, &lobby, "Start").await;
    assert_eq!(race.files[0].0, "race.png");
    let word = state(&sim, id).await["word"].as_str().expect("the word").to_owned();
    assert!(
        !race.description().contains(&word) && !race.content().contains(&word),
        "the word stays secret"
    );

    let outside = sim.click(outsider, &race, "Guess").await;
    assert!(
        outside.ephemeral && outside.content().contains("not in this race"),
        "{}",
        outside.content()
    );
    let form = sim.click(bo, &race, "Guess").await;
    assert!(form.modal.is_some());
    let short = sim.submit_fields(bo, &form, &[("guess", "abc")]).await;
    assert!(
        short.ephemeral && short.content().contains("five letters"),
        "{}",
        short.content()
    );
    let unknown = sim.submit_fields(bo, &form, &[("guess", "zzzzz")]).await;
    assert!(
        unknown.ephemeral && unknown.content().contains("word list"),
        "{}",
        unknown.content()
    );

    // A wrong guess comes back as a private board, and the table's card gains a row of colours.
    let other = PlayContent::get()
        .words
        .allowed
        .iter()
        .find(|w| **w != word)
        .expect("another word")
        .clone();
    sim.posts();
    let board = sim.submit_fields(bo, &form, &[("guess", other.as_str())]).await;
    assert!(board.ephemeral, "a board is only for its owner");
    assert_eq!(board.files[0].0, "board.png");
    std::fs::write(sim.dir.join("race-private-board.png"), &board.files[0].1).expect("keep the board");
    let posts = sim.posts();
    let progress = edits(&posts).into_iter().last().expect("the card moved on");
    assert_eq!(progress.files[0].0, "race.png");
    keep(&sim, "race-progress", progress);
    assert!(!description(progress).contains(&other), "the guess stays private");

    // The first to find the word wins; the race ends for everyone and shows the word.
    let solved = sim.submit_fields(ada, &form, &[("guess", other.as_str())]).await;
    assert!(solved.ephemeral);
    sim.posts();
    let win = sim.submit_fields(ada, &form, &[("guess", word.as_str())]).await;
    assert!(
        win.ephemeral && win.description().contains("found it"),
        "{}",
        win.description()
    );
    let posts = sim.posts();
    let end = edits(&posts).into_iter().last().expect("the result");
    assert!(end.title().contains("is over"), "{}", end.title());
    assert!(description(end).contains(&word.to_uppercase()), "{}", description(end));
    keep(&sim, "race-standings", end);
    assert_eq!(balance(&sim, ada).await, race_reward(2));
    assert_eq!(record_of(&sim, ada, "wordrace").await, (1, 1));
    assert_eq!(record_of(&sim, bo, "wordrace").await, (1, 0));
    assert_eq!(balance(&sim, bo).await, 0);
    // A guess after the end is refused.
    let late = sim.submit_fields(cy, &form, &[("guess", other.as_str())]).await;
    assert!(late.ephemeral && late.content().contains("over"), "{}", late.content());
}

#[tokio::test]
async fn a_race_nobody_wins_ends_with_the_clock() {
    let _alone = PARTY.lock().await;
    let Some(mut sim) = Sim::new("a_race_nobody_wins_ends_with_the_clock").await else {
        return;
    };
    fresh(&sim).await;
    sim.channel = unique();
    let (ada, bo) = (unique(), unique());
    let (lobby, id) = lobby_of(&mut sim, "wordrace", vec![], ada, &[bo]).await;
    sim.click(ada, &lobby, "Start").await;
    sim.posts();
    assert_eq!(advance(&sim, 600).await, 1);
    let posts = sim.posts();
    let end = edits(&posts).into_iter().last().expect("the result");
    assert!(description(end).contains("Nobody found"), "{}", description(end));
    assert_eq!(balance(&sim, ada).await, 0);
    assert_eq!(record_of(&sim, ada, "wordrace").await, (1, 0));
    assert!(game(&sim, id).await.is_done());
}

#[tokio::test]
async fn would_you_rather_hides_the_split_until_the_poll_closes() {
    let _alone = PARTY.lock().await;
    let Some(mut sim) = Sim::new("would_you_rather_hides_the_split_until_the_poll_closes").await else {
        return;
    };
    fresh(&sim).await;
    sim.channel = unique();
    let (ada, bo, cy) = (unique(), unique(), unique());
    let poll = sim
        .command(
            ada,
            "party",
            json!([sub("wouldyourrather", vec![text_option("topic", "ocean")])]),
        )
        .await;
    assert!(!poll.ephemeral && poll.pings_nobody());
    assert_eq!(poll.files[0].0, "rather.png");
    assert!(poll.has_button_starting("A · ") && poll.has_button_starting("B · "));
    let id = game_id(&poll);
    pin(&sim, id).await;
    let waits = sim.command(bo, "party", json!([sub("wouldyourrather", vec![])])).await;
    assert!(
        waits.ephemeral && waits.content().contains("just posted"),
        "{}",
        waits.content()
    );

    let first = sim.click_starting(ada, &poll, "A · ").await;
    assert!(
        first.ephemeral && first.content().contains("You picked **A**"),
        "{}",
        first.content()
    );
    assert!(first.content().contains("+1 shell"), "{}", first.content());
    assert_eq!(balance(&sim, ada).await, 1);
    let same = sim.click_starting(ada, &poll, "A · ").await;
    assert!(same.content().contains("already picked"), "{}", same.content());
    sim.click_starting(bo, &poll, "B · ").await;
    sim.click_starting(cy, &poll, "B · ").await;
    let moved = sim.click_starting(ada, &poll, "B · ").await;
    assert!(moved.content().contains("moved your vote"), "{}", moved.content());
    assert!(moved.content().contains("B 100%"), "{}", moved.content());
    // Changing a vote earns nothing more.
    assert_eq!(balance(&sim, ada).await, 1);

    sim.posts();
    assert_eq!(advance(&sim, 700).await, 1);
    let posts = sim.posts();
    let closed = edits(&posts).into_iter().last().expect("the result");
    assert_eq!(closed.files[0].0, "rather.png");
    assert!(description(closed).contains("closed"), "{}", description(closed));
    keep(&sim, "rather-result", closed);
    let result = posted_reply(closed);
    assert!(
        result
            .controls()
            .all(|c| matches!(c, Component::Button(b) if b.disabled)),
        "buttons are off"
    );
    for voter in [ada, bo, cy] {
        assert_eq!(
            record_of(&sim, voter, "wouldyourrather").await,
            (1, 1),
            "all three were on the bigger side"
        );
    }
    let late = sim.click_starting(bo, &poll, "A · ").await;
    assert!(
        late.ephemeral && late.content().contains("closed"),
        "{}",
        late.content()
    );
}

#[tokio::test]
async fn truth_or_dare_takes_turns_pays_for_done_and_ends_when_it_goes_quiet() {
    let _alone = PARTY.lock().await;
    let Some(mut sim) = Sim::new("truth_or_dare_takes_turns_pays_for_done_and_ends_when_it_goes_quiet").await else {
        return;
    };
    fresh(&sim).await;
    sim.channel = unique();
    let (ada, bo, cy) = (unique(), unique(), unique());
    let (lobby, id) = lobby_of(
        &mut sim,
        "truthordare",
        vec![text_option("topic", "cozy")],
        ada,
        &[bo, cy],
    )
    .await;
    let turn = sim.click(ada, &lobby, "Start").await;
    assert_eq!(turn.files[0].0, "turn.png");
    let st = state(&sim, id).await;
    let first = st["cur"].as_u64().expect("somebody is up");
    let order: Vec<u64> = st["order"]
        .as_array()
        .expect("an order")
        .iter()
        .map(|v| v.as_u64().unwrap())
        .collect();
    assert_eq!(order.len(), 3);
    assert_eq!(order[0], first);

    // Only whoever is up can pick.
    let not_you = sim.click(order[1], &turn, "Truth").await;
    assert!(
        not_you.ephemeral && not_you.content().contains("turn"),
        "{}",
        not_you.content()
    );
    let truth = sim.click(first, &turn, "Truth").await;
    assert!(!truth.ephemeral);
    assert!(truth.description().contains("a truth"), "{}", truth.description());
    let prompt = st_prompt(&sim, id).await;
    assert!(
        prompt.starts_with("truth_cozy_"),
        "the host asked for cozy ones: {prompt}"
    );
    keep_reply(&sim, "truth-card", &truth);
    let skipped = sim.click(order[1], &truth, "Done").await;
    assert!(skipped.ephemeral, "{}", skipped.content());
    let done = sim.click(first, &truth, "Done").await;
    assert!(done.description().contains("did it"), "{}", done.description());
    assert!(done.description().contains("+4 shells"), "{}", done.description());
    assert_eq!(balance(&sim, first).await, 4);
    assert_eq!(record_of(&sim, first, "truthordare").await, (1, 1));
    assert_eq!(
        state(&sim, id).await["cur"].as_u64(),
        Some(order[1]),
        "the next in order is up"
    );

    // A dare that is passed pays nothing and counts as played.
    let dare = sim.click(order[1], &done, "Dare").await;
    assert!(dare.description().contains("a dare"), "{}", dare.description());
    assert!(st_prompt(&sim, id).await.starts_with("dare_cozy_"));
    keep_reply(&sim, "dare-card", &dare);
    let passed = sim.click(order[1], &dare, "Pass").await;
    assert!(passed.description().contains("passed"), "{}", passed.description());
    assert_eq!(balance(&sim, order[1]).await, 0);
    assert_eq!(record_of(&sim, order[1], "truthordare").await, (1, 0));

    // Only the host ends the game by hand.
    let denied = sim.click(bo, &passed, "End game").await;
    assert!(
        denied.ephemeral && denied.content().contains("Only"),
        "{}",
        denied.content()
    );

    // Nobody answers for two turns in a row: the game ends by itself.
    sim.posts();
    assert_eq!(advance(&sim, 200).await, 1);
    let posts = sim.posts();
    let quiet = edits(&posts).into_iter().last().expect("a quiet turn");
    assert!(description(quiet).contains("did not answer"), "{}", description(quiet));
    assert_eq!(advance(&sim, 200).await, 1);
    let posts = sim.posts();
    let over = edits(&posts).into_iter().last().expect("the end");
    assert!(over.title().contains("is over"), "{}", over.title());
    assert!(description(over).contains("quiet"), "{}", description(over));
    keep(&sim, "dare-standings", over);
    assert!(game(&sim, id).await.is_done());

    // A second game: the host ends it with the button.
    let (lobby, other) = lobby_of(&mut sim, "truthordare", vec![], ada, &[bo]).await;
    let turn = sim.click(ada, &lobby, "Start").await;
    let ended = sim.click(ada, &turn, "End game").await;
    assert!(
        ended.title().contains("is over") && ended.components.is_empty(),
        "{}",
        ended.title()
    );
    assert!(game(&sim, other).await.is_done());
}

impl Reply {
    /// Whether the message has a button whose label starts with `prefix`.
    fn has_button_starting(&self, prefix: &str) -> bool {
        self.controls()
            .any(|c| matches!(c, Component::Button(b) if b.label.as_deref().is_some_and(|l| l.starts_with(prefix))))
    }
}

impl Sim {
    /// Clicks the button whose label starts with `prefix`.
    async fn click_starting(&mut self, user: u64, on: &Reply, prefix: &str) -> Reply {
        let label = on
            .controls()
            .find_map(|c| match c {
                Component::Button(b) => b.label.clone().filter(|l| l.starts_with(prefix)),
                _ => None,
            })
            .unwrap_or_else(|| panic!("no button starting {prefix:?}"));
        self.click(user, on, &label).await
    }
}

/// The id of the truth or dare the player is on.
async fn st_prompt(sim: &Sim, id: i64) -> String {
    state(sim, id).await["prompt"].as_str().expect("a prompt").to_owned()
}

/// Keeps the picture of a reply under a name that says what it shows.
fn keep_reply(sim: &Sim, name: &str, reply: &Reply) {
    let (_, png) = reply.files.first().expect("the reply carried a picture");
    std::fs::write(sim.dir.join(format!("{name}.png")), png).expect("save the picture");
}

#[tokio::test]
async fn the_party_commands_fit_discords_limits_and_help_knows_them() {
    let definitions = pinchora_commands::command_definitions();
    let party = serde_json::to_value(definitions.iter().find(|c| c.name == "party").expect("/party")).unwrap();
    assert!(party["description"].as_str().unwrap().len() <= 100);
    let subs: Vec<&str> = party["options"]
        .as_array()
        .unwrap()
        .iter()
        .map(|o| o["name"].as_str().unwrap())
        .collect();
    assert_eq!(
        subs,
        [
            "hotpearl",
            "acronym",
            "postcard",
            "wordrace",
            "wouldyourrather",
            "truthordare"
        ]
    );
    for sub in party["options"].as_array().unwrap() {
        assert!(sub["description"].as_str().unwrap().len() <= 100, "{}", sub["name"]);
        for option in sub["options"].as_array().into_iter().flatten() {
            assert!(option["description"].as_str().unwrap().len() <= 100);
        }
    }
    let config = serde_json::to_value(definitions.iter().find(|c| c.name == "config").expect("/config")).unwrap();
    assert!(
        config["options"]
            .as_array()
            .unwrap()
            .iter()
            .any(|o| o["name"] == "partygames")
    );

    let Some(mut sim) = Sim::new("the_party_commands_fit_discords_limits_and_help_knows_them").await else {
        return;
    };
    let help = sim.command(unique(), "help", json!([])).await;
    assert!(
        help.field("Party games").contains("/party hotpearl"),
        "{}",
        help.field("Party games")
    );
    assert!(help.field("Party games").contains("/party truthordare"));
    // Like the rest of /config, it is for managers: Discord keeps it from everyone else.
    assert!(config["default_member_permissions"].is_string(), "{config}");
}
