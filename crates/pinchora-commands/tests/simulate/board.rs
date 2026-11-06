//! Board and puzzle games through the real handlers: Reef Chess, Pearl Checkers, Sea Fleet, Pixel Picross
//! and Tide 2048. Pictures land in `target/sim/<test>/`.

use pinchora_game::play::{
    fleet::{Fleet, Sea},
    picross::{self, Puzzle},
    tide2048,
};
use serde_json::json;

use super::{Post, Reply, SWEEPS, Sim, duel, expire, fund, int_option, record_of, session_of, shells, unique};

/// Who is to move in a duel message that says `<@id>'s move` or `<@id> fires`.
fn mover(reply: &Reply, a: u64, b: u64) -> u64 {
    let content = reply.content();
    if content.contains(&format!("<@{a}>'s move")) || content.contains(&format!("<@{a}> fires")) {
        a
    } else if content.contains(&format!("<@{b}>'s move")) || content.contains(&format!("<@{b}> fires")) {
        b
    } else {
        panic!("nobody is to move: {content}")
    }
}

/// Sets a session's own fields, keeping the names that ride in its state.
async fn set_fields(sim: &Sim, session: i64, fields: &str) {
    let (state,): (String,) = sqlx::query_as("SELECT state FROM game_sessions WHERE id = $1")
        .bind(session)
        .fetch_one(sim.app.db.pool())
        .await
        .expect("the session");
    let names = state.split_once('|').map_or("", |(_, names)| names);
    sqlx::query("UPDATE game_sessions SET state = $2 WHERE id = $1")
        .bind(session)
        .bind(format!("{fields}|{names}"))
        .execute(sim.app.db.pool())
        .await
        .expect("set the state");
}

/// A message rebuilt from what the bot edited in by itself (a post), so a test can go on clicking it.
fn reply_of(post: &Post) -> Reply {
    use twilight_model::{channel::message::Component, http::interaction::InteractionResponseType};
    let mut reply = Reply::new(InteractionResponseType::UpdateMessage);
    let embeds = serde_json::from_value(post.body["embeds"].clone()).ok();
    let components = serde_json::from_value::<Vec<Component>>(post.body["components"].clone()).ok();
    reply.edit(post.body["content"].as_str().map(str::to_owned), embeds, components);
    reply.files = post.files.clone();
    reply
}

/// The newest message the bot edited by itself. (The mock names the clicked message afresh on every click, so
/// tests cannot tell their game's message by its id; each of them edits one.)
fn latest_post(sim: &Sim) -> Post {
    sim.posts().into_iter().next_back().expect("the message was edited")
}

fn png_size(reply: &Reply) -> (u32, u32) {
    let image = image::load_from_memory(&reply.files[0].1).expect("a png");
    (image.width(), image.height())
}

// /Chess

/// Makes one move of chess (`e2e4`) for whoever is to move.
async fn chess_move(sim: &mut Sim, board: &Reply, a: u64, b: u64, uci: &str) -> Reply {
    let player = mover(board, a, b);
    let picked = sim.pick(player, board, "cc:s:", &uci[..2]).await;
    sim.pick(player, &picked, "cc:t:", uci).await
}

#[tokio::test]
async fn reef_chess_plays_to_checkmate_and_the_winner_takes_the_pot() {
    let Some(mut sim) = Sim::new("reef_chess_plays_to_checkmate_and_the_winner_takes_the_pot").await else {
        return;
    };
    let (a, b, stranger) = (unique(), unique(), unique());
    fund(&sim, a, 500).await;
    fund(&sim, b, 500).await;

    let challenge = duel(&mut sim, a, b, "chess", vec![int_option("bet", 100)]).await;
    assert!(
        challenge.content().contains("challenges") && challenge.content().contains("Reef Chess"),
        "{}",
        challenge.content()
    );
    assert!(challenge.content().contains("forfeits"), "{}", challenge.content());
    let board = sim.click(b, &challenge, "Accept").await;
    assert_eq!(board.files[0].0, "chess.png");
    assert_eq!(png_size(&board), (580, 730));
    assert_eq!(
        (shells(&sim, a).await, shells(&sim, b).await),
        (400, 400),
        "both wagers are held"
    );
    assert_eq!(board.options("cc:s:").len(), 10, "eight pawns and two knights can move");
    assert!(board.has_button("Resign"));
    let first = mover(&board, a, b);
    let second = if first == a { b } else { a };

    // The wrong player, a stranger and a piece with no moves are all turned away.
    let wrong = sim
        .component(second, &board.menu("cc:s:").custom_id.clone(), &["e7"], &board)
        .await;
    assert!(
        wrong.ephemeral && wrong.content().contains("Not your turn"),
        "{}",
        wrong.content()
    );
    let outsider = sim
        .component(stranger, &board.menu("cc:s:").custom_id.clone(), &["e2"], &board)
        .await;
    assert!(
        outsider.ephemeral && outsider.content().contains("not in this game"),
        "{}",
        outsider.content()
    );
    let nothing = sim
        .component(first, &board.menu("cc:s:").custom_id.clone(), &["a1"], &board)
        .await;
    assert!(
        nothing.ephemeral && nothing.content().contains("can't move"),
        "{}",
        nothing.content()
    );

    // Picking a piece shows where it can go, on the same board.
    let picked = sim.pick(first, &board, "cc:s:", "f2").await;
    assert_eq!(picked.options("cc:t:").len(), 2, "f3 and f4");
    assert_eq!(png_size(&picked), (580, 730));
    let illegal = sim
        .component(first, &picked.menu("cc:t:").custom_id.clone(), &["f2f5"], &picked)
        .await;
    assert!(
        illegal.ephemeral && illegal.content().contains("isn't legal"),
        "{}",
        illegal.content()
    );

    // Fool's mate: f3 e5 g4 Qh4#. Whoever has the second side wins.
    let mut board = sim.pick(first, &picked, "cc:t:", "f2f3").await;
    assert!(board.description().contains("f3"), "{}", board.description());
    // A click on the old menu, from before that move, shows the board as it is now.
    let stale = sim
        .component(first, &picked.menu("cc:t:").custom_id.clone(), &["f2f4"], &picked)
        .await;
    assert!(!stale.ephemeral, "the clicked message is brought up to date");
    for uci in ["e7e5", "g2g4", "d8h4"] {
        board = chess_move(&mut sim, &board, a, b, uci).await;
    }
    assert!(board.content().contains("wins by checkmate"), "{}", board.content());
    assert!(board.content().contains(&format!("<@{second}>")), "{}", board.content());
    assert!(
        board.content().contains("take the pot of **200**"),
        "{}",
        board.content()
    );
    assert!(board.components.is_empty(), "no menus once the game is over");
    assert_eq!(
        (shells(&sim, first).await, shells(&sim, second).await),
        (400, 600),
        "the pot goes to the winner and the system adds nothing"
    );
    assert_eq!(record_of(&sim, second, "chess").await, (1, 1));
    assert_eq!(record_of(&sim, first, "chess").await, (1, 0));
    let late = sim.click(first, &challenge, "Accept").await;
    assert!(late.ephemeral, "an old challenge cannot restart it");
}

#[tokio::test]
async fn reef_chess_handles_castling_promotion_and_resigning() {
    let Some(mut sim) = Sim::new("reef_chess_handles_castling_promotion_and_resigning").await else {
        return;
    };
    let (a, b) = (unique(), unique());
    fund(&sim, a, 300).await;
    fund(&sim, b, 300).await;
    let challenge = duel(&mut sim, a, b, "chess", vec![int_option("bet", 50)]).await;
    let board = sim.click(b, &challenge, "Accept").await;
    let session = session_of(&board);

    // A position where white can castle either way. Setting the state under the board makes its menus stale,
    // and a click on a stale menu shows the board as it now is.
    set_fields(
        &sim,
        session,
        "0:e2e4 e7e5 g1f3 b8c6 f1c4 g8f6 d2d3 d7d6 c1g5 c8g4 b1c3 a7a6 d1d2 h7h6",
    )
    .await;
    let current = sim
        .component(a, &board.menu("cc:s:").custom_id.clone(), &["e1"], &board)
        .await;
    assert!(!current.ephemeral, "a stale pick brings the board up to date");
    let picked = sim.pick(a, &current, "cc:s:", "e1").await;
    let labels: Vec<_> = picked.options("cc:t:").iter().map(|o| o.label.clone()).collect();
    assert!(labels.iter().any(|l| l.contains("castle kingside")), "{labels:?}");
    assert!(labels.iter().any(|l| l.contains("castle queenside")), "{labels:?}");
    let castled = sim.pick(a, &picked, "cc:t:", "e1g1").await;
    assert!(castled.description().contains("O-O"), "{}", castled.description());

    // A pawn on the seventh rank takes the knight on b8 and can become any of four pieces.
    set_fields(&sim, session, "0:a2a4 b7b5 a4b5 a7a6 b5a6 c8b7 a6a7 g8f6").await;
    let current = sim
        .component(a, &castled.menu("cc:s:").custom_id.clone(), &["a7"], &castled)
        .await;
    let picked = sim.pick(a, &current, "cc:s:", "a7").await;
    let promotions: Vec<_> = picked.options("cc:t:").iter().map(|o| o.value.clone()).collect();
    for want in ["a7b8q", "a7b8r", "a7b8b", "a7b8n"] {
        assert!(promotions.iter().any(|p| p == want), "{promotions:?}");
    }
    assert!(
        picked.options("cc:t:")[0].label.contains("becomes a"),
        "{}",
        picked.options("cc:t:")[0].label
    );
    let promoted = sim.pick(a, &picked, "cc:t:", "a7b8n").await;
    assert!(promoted.description().contains("axb8=N"), "{}", promoted.description());

    // Resigning asks first; the other player wins the pot and the public board says so.
    let ask = sim.click(b, &promoted, "Resign").await;
    assert!(ask.ephemeral && ask.has_button("Yes, resign"), "{}", ask.content());
    assert_eq!(shells(&sim, a).await, 250, "asking does not end the game");
    let done = sim.click(b, &ask, "Yes, resign").await;
    assert!(done.content().contains("You resigned"), "{}", done.content());
    assert_eq!((shells(&sim, a).await, shells(&sim, b).await), (350, 250));
    let post = latest_post(&sim);
    assert!(post.content().contains("wins by resignation"), "{}", post.content());
    assert!(post.content().contains(&format!("<@{a}>")), "{}", post.content());
    assert_eq!(post.body["components"].as_array().map(Vec::len), Some(0));
    let again = sim.click(b, &ask, "Yes, resign").await;
    assert!(
        again.ephemeral && again.content().contains("over"),
        "{}",
        again.content()
    );
}

#[tokio::test]
async fn a_slow_chess_player_forfeits_the_wager_and_a_game_without_one_just_ends() {
    let _serial = SWEEPS.lock().await;
    let Some(mut sim) = Sim::new("a_slow_chess_player_forfeits_the_wager_and_a_game_without_one_just_ends").await
    else {
        return;
    };
    let (a, b, c, d) = (unique(), unique(), unique(), unique());
    for user in [a, b, c, d] {
        fund(&sim, user, 300).await;
    }
    let challenge = duel(&mut sim, a, b, "chess", vec![int_option("bet", 100)]).await;
    let board = sim.click(b, &challenge, "Accept").await;
    let session = session_of(&board);
    let slow = mover(&board, a, b);
    let quick = if slow == a { b } else { a };
    assert_eq!((shells(&sim, a).await, shells(&sim, b).await), (200, 200));
    expire(&sim, session).await;
    pinchora_commands::sweep_games(&sim.app).await.expect("sweep");
    assert_eq!(
        (shells(&sim, slow).await, shells(&sim, quick).await),
        (200, 400),
        "the player who ran out of time forfeits the pot"
    );
    let text = latest_post(&sim);
    assert!(text.content().contains("forfeits"), "{}", text.content());
    assert!(
        text.content().contains(&format!("<@{slow}>")) && text.content().contains("pot of **200**"),
        "{}",
        text.content()
    );
    pinchora_commands::sweep_games(&sim.app).await.expect("sweep again");
    assert_eq!(shells(&sim, quick).await, 400, "paid once");
    assert_eq!(record_of(&sim, quick, "chess").await, (0, 0), "timeouts do not count");

    // One move later it is the other player's clock.
    let challenge = duel(&mut sim, c, d, "chess", vec![int_option("bet", 100)]).await;
    let board = sim.click(d, &challenge, "Accept").await;
    let session = session_of(&board);
    let first = mover(&board, c, d);
    let second = if first == c { d } else { c };
    let board = chess_move(&mut sim, &board, c, d, "e2e4").await;
    assert_eq!(mover(&board, c, d), second);
    expire(&sim, session).await;
    pinchora_commands::sweep_games(&sim.app).await.expect("sweep");
    assert_eq!(
        (shells(&sim, first).await, shells(&sim, second).await),
        (400, 200),
        "now the second player is the slow one"
    );

    // Without a wager nothing moves.
    let (e, f) = (unique(), unique());
    let challenge = duel(&mut sim, e, f, "chess", vec![]).await;
    let board = sim.click(f, &challenge, "Accept").await;
    expire(&sim, session_of(&board)).await;
    pinchora_commands::sweep_games(&sim.app).await.expect("sweep");
    assert_eq!((shells(&sim, e).await, shells(&sim, f).await), (0, 0));
}

#[tokio::test]
async fn a_stalemate_is_a_draw_that_gives_the_wagers_back() {
    let Some(mut sim) = Sim::new("a_stalemate_is_a_draw_that_gives_the_wagers_back").await else {
        return;
    };
    let (a, b) = (unique(), unique());
    fund(&sim, a, 300).await;
    fund(&sim, b, 300).await;
    let challenge = duel(&mut sim, a, b, "chess", vec![int_option("bet", 100)]).await;
    let board = sim.click(b, &challenge, "Accept").await;
    let session = session_of(&board);

    // Sam Loyd's ten-move stalemate, set up but for its last move: the host has coral and plays it.
    set_fields(
        &sim,
        session,
        "0:e2e3 a7a5 d1h5 a8a6 h5a5 h7h5 h2h4 a6h6 a5c7 f7f6 c7d7 e8f7 d7b7 d8d3 b7b8 d3h7 b8c8 f7g6",
    )
    .await;
    let current = sim
        .component(a, &board.menu("cc:s:").custom_id.clone(), &["c8"], &board)
        .await;
    let picked = sim.pick(a, &current, "cc:s:", "c8").await;
    let draw = sim.pick(a, &picked, "cc:t:", "c8e6").await;
    assert!(draw.content().contains("draw by stalemate"), "{}", draw.content());
    assert!(
        draw.content().contains("Both wagers were given back"),
        "{}",
        draw.content()
    );
    assert!(draw.components.is_empty());
    assert_eq!((shells(&sim, a).await, shells(&sim, b).await), (300, 300));
    assert_eq!(record_of(&sim, a, "chess").await, (1, 0));
    assert_eq!(record_of(&sim, b, "chess").await, (1, 0));
}

#[tokio::test]
async fn help_and_stats_list_the_board_games() {
    let Some(mut sim) = Sim::new("help_and_stats_list_the_board_games").await else {
        return;
    };
    let user = unique();
    let help = sim.command(user, "help", json!([])).await;
    let games = help.field("Board & puzzle games");
    for command in [
        "/duel seafleet",
        "/duel chess",
        "/duel checkers",
        "/play picross",
        "/play 2048",
    ] {
        assert!(games.contains(command), "{command} is missing from /help: {games}");
    }
    let stats = sim.command(user, "play", json!([super::sub("stats", vec![])])).await;
    for game in [
        "Sea Fleet",
        "Pixel Picross",
        "Tide 2048",
        "Pearl Checkers",
        "Reef Chess",
    ] {
        assert!(
            stats.embeds[0].fields.iter().any(|f| f.name.contains(game)),
            "{game} is missing from /play stats"
        );
    }
}

// /Checkers

/// A playing square by its name, as the number the menus use.
fn square(name: &str) -> String {
    (0..32u8)
        .find(|&sq| pinchora_game::play::checkers::name(sq) == name)
        .expect("a playing square")
        .to_string()
}

#[tokio::test]
async fn pearl_checkers_forces_the_capture_and_pays_the_winner() {
    let Some(mut sim) = Sim::new("pearl_checkers_forces_the_capture_and_pays_the_winner").await else {
        return;
    };
    let (a, b) = (unique(), unique());
    fund(&sim, a, 300).await;
    fund(&sim, b, 300).await;
    let challenge = duel(&mut sim, a, b, "checkers", vec![int_option("bet", 100)]).await;
    let board = sim.click(b, &challenge, "Accept").await;
    assert_eq!(board.files[0].0, "checkers.png");
    assert_eq!(png_size(&board), (580, 730));
    assert_eq!(board.options("ck:s:").len(), 4, "four pieces of the front row can step");
    let session = session_of(&board);
    let (first, second) = (mover(&board, a, b), if mover(&board, a, b) == a { b } else { a });

    // c3-d4, f6-e5: the pearl on d4 must now jump.
    let (c3, d4) = (square("c3"), square("d4"));
    let moved = sim.pick(first, &board, "ck:s:", &c3).await;
    let labels: Vec<_> = moved.options("ck:t:").iter().map(|o| o.label.clone()).collect();
    assert_eq!(labels, ["c3-b4", "c3-d4"], "{labels:?}");
    let to_d4 = moved
        .options("ck:t:")
        .iter()
        .find(|o| o.label == "c3-d4")
        .expect("c3-d4")
        .value
        .clone();
    let board = sim.pick(first, &moved, "ck:t:", &to_d4).await;
    let f6 = square("f6");
    let moved = sim.pick(second, &board, "ck:s:", &f6).await;
    let step = moved
        .options("ck:t:")
        .iter()
        .find(|o| o.label == "f6-e5")
        .expect("f6-e5")
        .value
        .clone();
    let board = sim.pick(second, &moved, "ck:t:", &step).await;
    let options: Vec<_> = board.options("ck:s:").iter().map(|o| o.label.clone()).collect();
    assert_eq!(
        options,
        ["Man on d4"],
        "only the piece that can jump may move: {options:?}"
    );
    assert!(
        board.options("ck:s:")[0]
            .description
            .as_deref()
            .is_some_and(|d| d.contains("jumps"))
    );
    let jump = sim.pick(first, &board, "ck:s:", &d4).await;
    assert_eq!(jump.options("ck:t:").len(), 1);
    assert_eq!(jump.options("ck:t:")[0].label, "d4xf6");
    let board = sim
        .pick(first, &jump, "ck:t:", &jump.options("ck:t:")[0].value.clone())
        .await;
    // The second player must take back: two pieces can.
    let labels: Vec<_> = board.options("ck:s:").iter().map(|o| o.label.clone()).collect();
    assert_eq!(labels.len(), 2, "{labels:?}");

    // Fast-forward: player 1 (the one with the first side) has one jump left that takes the last piece.
    let (pearl_player, other) = (first, second);
    let pearl_index = usize::from(pearl_player != a);
    let mut cells = ".".repeat(32);
    let at = |name: &str| square(name).parse::<usize>().expect("a number");
    cells.replace_range(at("d4")..=at("d4"), "a");
    cells.replace_range(at("e5")..=at("e5"), "b");
    set_fields(&sim, session, &format!("{pearl_index}:{cells}:1:0:20:-")).await;
    let current = sim
        .component(pearl_player, &format!("ck:s:{session}:0"), &[&square("d4")], &board)
        .await;
    let jump = sim.pick(pearl_player, &current, "ck:s:", &square("d4")).await;
    let done = sim
        .pick(pearl_player, &jump, "ck:t:", &jump.options("ck:t:")[0].value.clone())
        .await;
    assert!(
        done.content().contains("wins by taking or blocking every piece"),
        "{}",
        done.content()
    );
    assert!(done.content().contains("take the pot of **200**"), "{}", done.content());
    assert!(done.components.is_empty());
    assert_eq!(
        (shells(&sim, pearl_player).await, shells(&sim, other).await),
        (400, 200)
    );
    assert_eq!(record_of(&sim, pearl_player, "checkers").await, (1, 1));
    assert_eq!(record_of(&sim, other, "checkers").await, (1, 0));
}

// /Sea Fleet

/// A cell's column letter menu value and row menu value.
fn aim(cell: usize) -> (String, String) {
    ((cell % 10).to_string(), (cell / 10).to_string())
}

/// Fires at `cell` for `player`: a column, then a row.
async fn fire(sim: &mut Sim, player: u64, board: &Reply, cell: usize) -> Reply {
    let (col, row) = aim(cell);
    let column = sim.pick(player, board, "bs:a:", &col).await;
    sim.pick(player, &column, "bs:f:", &row).await
}

#[tokio::test]
async fn sea_fleet_places_in_private_fires_and_sinks_a_fleet() {
    let Some(mut sim) = Sim::new("sea_fleet_places_in_private_fires_and_sinks_a_fleet").await else {
        return;
    };
    let (a, b, stranger) = (unique(), unique(), unique());
    fund(&sim, a, 300).await;
    fund(&sim, b, 300).await;
    let challenge = duel(&mut sim, a, b, "seafleet", vec![int_option("bet", 100)]).await;
    let public = sim.click(b, &challenge, "Accept").await;
    assert_eq!(public.files[0].0, "seafleet.png");
    assert_eq!(png_size(&public), (836, 550));
    assert!(public.content().contains("place your fleet"), "{}", public.content());
    let session = session_of(&public);
    assert_eq!((shells(&sim, a).await, shells(&sim, b).await), (200, 200));

    // Each captain places in private: the placement is ephemeral, with presets, Shuffle and Lock in.
    let outsider = sim.click(stranger, &public, "Place my fleet").await;
    assert!(
        outsider.ephemeral && outsider.content().contains("not in this game"),
        "{}",
        outsider.content()
    );
    let open = sim.click(a, &public, "Place my fleet").await;
    assert!(open.ephemeral, "the placement is private");
    assert_eq!(png_size(&open), (620, 500));
    for label in [
        "Harbour line",
        "Scatter",
        "Corners",
        "Diagonal",
        "Fortress",
        "Shuffle",
        "Lock in fleet",
    ] {
        assert!(open.has_button(label), "no {label}");
    }
    let shuffled = sim.click(a, &open, "Shuffle").await;
    assert!(shuffled.ephemeral && shuffled.has_button("Lock in fleet"));
    let preset = sim.click(a, &shuffled, "Harbour line").await;
    let locked = sim.click(a, &preset, "Lock in fleet").await;
    assert!(
        locked.components.is_empty() && locked.title() == "Your fleet",
        "{}",
        locked.title()
    );
    let waiting = latest_post(&sim);
    assert!(waiting.content().contains("place your fleet"), "{}", waiting.content());
    let again = sim.click(a, &public, "Place my fleet").await;
    assert!(
        again.ephemeral && again.content().contains("locked in"),
        "{}",
        again.content()
    );
    let early = sim.component(a, &format!("bs:a:{session}:0"), &["3"], &public).await;
    assert!(
        early.ephemeral && early.content().contains("hasn't begun"),
        "{}",
        early.content()
    );

    // The second captain locks in the same layout, which starts the battle on the public message.
    let open = sim.click(b, &public, "Place my fleet").await;
    let preset = sim.click(b, &open, "Harbour line").await;
    sim.click(b, &preset, "Lock in fleet").await;
    let battle = reply_of(&latest_post(&sim));
    assert!(battle.content().contains("fires"), "{}", battle.content());
    assert!(battle.has_button("My fleet") && battle.has_button("Resign"));
    assert_eq!(battle.options("bs:a:").len(), 10, "ten columns to pick from");
    let (first, second) = {
        let first = mover(&battle, a, b);
        (first, if first == a { b } else { a })
    };

    // Not your turn; a miss; a hit; the turn passes either way.
    let early = sim
        .component(second, &battle.menu("bs:a:").custom_id.clone(), &["3"], &battle)
        .await;
    assert!(
        early.ephemeral && early.content().contains("Not your turn"),
        "{}",
        early.content()
    );
    let column = sim.pick(first, &battle, "bs:a:", "4").await;
    assert_eq!(column.options("bs:f:").len(), 10);
    assert!(column.content().contains("Column **E** picked"), "{}", column.content());
    let miss = sim.pick(first, &column, "bs:f:", "4").await;
    assert!(miss.content().contains("E5: a miss"), "{}", miss.content());
    assert_eq!(mover(&miss, a, b), second);
    let hit = fire(&mut sim, second, &miss, 0).await;
    assert!(hit.content().contains("A1: a hit!"), "{}", hit.content());
    assert_eq!(png_size(&hit), (836, 550));
    // The same cell twice is refused, and the menus do not offer it.
    let column = sim.pick(first, &hit, "bs:a:", "4").await;
    assert_eq!(
        column.options("bs:f:").len(),
        9,
        "E5 was fired at, so it is gone from the rows"
    );

    // A captain looks at their own waters in private.
    let mine = sim.click(second, &hit, "My fleet").await;
    assert!(mine.ephemeral && mine.has_button("Refresh"), "{}", mine.content());
    assert_eq!(png_size(&mine), (620, 500));
    let theirs = sim.click(first, &hit, "My fleet").await;
    assert!(theirs.ephemeral);

    // Fast-forward to the last shot: every cell of the other fleet is hit but one.
    let fleet0 = Fleet::from_seed(0);
    let victim = usize::from(second != a);
    let mut sea = Sea::new();
    let cells: Vec<usize> = (0..5).flat_map(|ship| fleet0.cells_of(ship)).collect();
    for &cell in &cells[1..] {
        sea.fire(&fleet0, cell);
    }
    let own = Sea::new();
    let (s0, s1) = if victim == 0 {
        (sea.encode(), own.encode())
    } else {
        (own.encode(), sea.encode())
    };
    let shots = 3;
    // The captain `first` fires next in the new state; the other fleet is down to one cell.
    set_fields(
        &sim,
        session,
        &format!(
            "b:{}:{}:{}:{s0}:{s1}:-:{shots}",
            usize::from(first != a),
            fleet0.encode(),
            fleet0.encode()
        ),
    )
    .await;
    let stale = sim
        .component(first, &format!("bs:a:{session}:{shots}"), &["0"], &hit)
        .await;
    assert!(!stale.ephemeral);
    let (col, row) = aim(cells[0]);
    let column = sim.pick(first, &stale, "bs:a:", &col).await;
    let done = sim.pick(first, &column, "bs:f:", &row).await;
    assert!(
        done.content().contains("wins by sinking the whole fleet"),
        "{}",
        done.content()
    );
    assert!(done.content().contains("take the pot of **200**"), "{}", done.content());
    assert!(done.components.is_empty());
    assert_eq!((shells(&sim, first).await, shells(&sim, second).await), (400, 200));
    assert_eq!(record_of(&sim, first, "seafleet").await, (1, 1));
    assert_eq!(record_of(&sim, second, "seafleet").await, (1, 0));
}

#[tokio::test]
async fn a_captain_who_never_places_forfeits_and_two_idle_captains_get_their_shells_back() {
    let _serial = SWEEPS.lock().await;
    let Some(mut sim) =
        Sim::new("a_captain_who_never_places_forfeits_and_two_idle_captains_get_their_shells_back").await
    else {
        return;
    };
    let (a, b, c, d) = (unique(), unique(), unique(), unique());
    for user in [a, b, c, d] {
        fund(&sim, user, 300).await;
    }
    // b never places; a did.
    let challenge = duel(&mut sim, a, b, "seafleet", vec![int_option("bet", 100)]).await;
    let public = sim.click(b, &challenge, "Accept").await;
    let session = session_of(&public);
    let open = sim.click(a, &public, "Place my fleet").await;
    sim.click(a, &open, "Lock in fleet").await;
    expire(&sim, session).await;
    pinchora_commands::sweep_games(&sim.app).await.expect("sweep");
    assert_eq!(
        (shells(&sim, a).await, shells(&sim, b).await),
        (400, 200),
        "the idle captain forfeits"
    );

    // Neither places: the wagers go back.
    let challenge = duel(&mut sim, c, d, "seafleet", vec![int_option("bet", 100)]).await;
    let public = sim.click(d, &challenge, "Accept").await;
    expire(&sim, session_of(&public)).await;
    pinchora_commands::sweep_games(&sim.app).await.expect("sweep");
    assert_eq!((shells(&sim, c).await, shells(&sim, d).await), (300, 300));
    let text = latest_post(&sim);
    assert!(
        text.content().contains("ran out of time") && text.content().contains("Both wagers"),
        "{}",
        text.content()
    );
}

// /Pixel Picross

/// The row menu values that fill row `y` of the puzzle's finished picture.
fn row_values(puzzle: &Puzzle, y: usize) -> Vec<String> {
    (0..picross::SIZE)
        .filter(|&x| puzzle.filled(y * picross::SIZE + x))
        .map(|x| x.to_string())
        .collect()
}

#[tokio::test]
async fn the_daily_picross_is_solved_row_by_row_and_paid_once() {
    let Some(mut sim) = Sim::new("the_daily_picross_is_solved_row_by_row_and_paid_once").await else {
        return;
    };
    let (user, friend) = (unique(), unique());
    let day = sim.app.now().timestamp().div_euclid(86_400);
    let (picture, variant) = picross::daily(day);
    let puzzle = Puzzle::new(picture, variant);

    let board = sim.command(user, "play", json!([super::sub("picross", vec![])])).await;
    assert!(board.ephemeral, "the puzzle is private");
    assert_eq!(png_size(&board), (540, 610));
    assert!(board.title().starts_with("Daily picross #"), "{}", board.title());
    assert_eq!(board.options("pz:r:").len(), 10);
    assert!(board.has_button("Mark cells") && board.has_button("Check"));
    // Everyone gets the same puzzle on a day.
    let other = sim
        .command(friend, "play", json!([super::sub("picross", vec![])]))
        .await;
    assert_eq!(other.title(), board.title());

    // Pick a row, then its filled cells: row 1 by the menu.
    let row = sim.pick(user, &board, "pz:r:", "3").await;
    assert_eq!(row.options("pz:c:").len(), 10);
    assert_eq!(row.options("pz:c:")[0].label, "A4");
    let values = row_values(&puzzle, 3);
    let refs: Vec<&str> = values.iter().map(String::as_str).collect();
    let mut state = sim
        .component(user, &row.menu("pz:c:").custom_id.clone(), &refs, &row)
        .await;
    assert_eq!(
        state.options("pz:c:").iter().filter(|o| o.default).count(),
        values.len(),
        "the filled cells stay ticked"
    );

    // Typed coordinates: a run along a column, then a mistake, then Check shows it and a clear removes it.
    let modal = sim.click(user, &state, "Mark cells").await;
    assert!(modal.modal.is_some(), "the form opens");
    let bad = sim.submit_fields(user, &modal, &[("cells", "A1-B2")]).await;
    assert!(
        bad.ephemeral && bad.content().contains("couldn't read"),
        "{}",
        bad.content()
    );
    let empty = (0..100).find(|&c| !puzzle.filled(c)).expect("an empty cell");
    let name = format!("{}{}", char::from(b'A' + (empty % 10) as u8), empty / 10 + 1);
    state = sim.submit_fields(user, &modal, &[("cells", &name)]).await;
    let checked = sim.click(user, &state, "Check").await;
    assert!(checked.description().contains("in red"), "{}", checked.description());
    state = sim.submit_fields(user, &modal, &[("cells", &format!("-{name}"))]).await;

    // The rest of the rows by the menus.
    let mut solved = state;
    for y in 0..picross::SIZE {
        if y == 3 {
            continue;
        }
        let values = row_values(&puzzle, y);
        if values.is_empty() {
            continue;
        }
        let picked = sim.pick(user, &solved, "pz:r:", &y.to_string()).await;
        let refs: Vec<&str> = values.iter().map(String::as_str).collect();
        solved = sim
            .component(user, &picked.menu("pz:c:").custom_id.clone(), &refs, &picked)
            .await;
    }
    assert!(
        solved
            .description()
            .contains(&format!("You found a **{}**", puzzle.name())),
        "{}",
        solved.description()
    );
    assert!(
        solved.description().contains("+**35** shells"),
        "{}",
        solved.description()
    );
    assert!(solved.has_button("Share result"));
    assert_eq!(png_size(&solved), (540, 610));
    assert_eq!(shells(&sim, user).await, 35);
    assert_eq!(record_of(&sim, user, "picross").await, (1, 1));

    // Solved is solved: the board comes back finished, a second try pays nothing, the clock must not matter.
    let revisit = sim.command(user, "play", json!([super::sub("picross", vec![])])).await;
    assert!(
        revisit.description().contains("You found a"),
        "{}",
        revisit.description()
    );
    let late = sim
        .component(user, &format!("pz:c:daily:{day}:3"), &["0"], &revisit)
        .await;
    assert!(
        late.ephemeral && late.content().contains("finished"),
        "{}",
        late.content()
    );
    assert_eq!(shells(&sim, user).await, 35);

    // Sharing posts no picture and only once.
    let shared = sim.click(user, &solved, "Share result").await;
    assert!(
        !shared.ephemeral && shared.content().contains("solved the daily Pixel Picross"),
        "{}",
        shared.content()
    );
    assert!(shared.pings_nobody() && shared.files.is_empty());
    let twice = sim.click(user, &solved, "Share result").await;
    assert!(
        twice.ephemeral && twice.content().contains("already shared"),
        "{}",
        twice.content()
    );

    // A form opened on the day and sent after midnight does not mark (or pay for) yesterday's puzzle.
    let open = sim.click(friend, &other, "Mark cells").await;
    sim.app.shift_clock(chrono::Duration::days(1));
    let after_midnight = sim.submit_fields(friend, &open, &[("cells", "A1")]).await;
    assert!(
        after_midnight.ephemeral && after_midnight.content().contains("has ended"),
        "{}",
        after_midnight.content()
    );
}

#[tokio::test]
async fn picross_streaks_pay_more_and_practice_puzzles_are_endless_but_capped() {
    let Some(mut sim) = Sim::new("picross_streaks_pay_more_and_practice_puzzles_are_endless_but_capped").await else {
        return;
    };
    let user = unique();
    let day = sim.app.now().timestamp().div_euclid(86_400);
    // Yesterday and the day before were solved.
    for back in [1, 2] {
        let d = (day - back) as i32;
        sim.app
            .db
            .start_picross(user, "daily", d, (0, 0), false)
            .await
            .expect("a board");
        let game = sim.app.db.picross_game(user, "daily", d).await.unwrap().expect("board");
        sim.app
            .db
            .picross_marks(user, "daily", d, &game.grid, &game.grid, true)
            .await
            .unwrap()
            .expect("solved");
    }
    let (picture, variant) = picross::daily(day);
    let puzzle = Puzzle::new(picture, variant);
    let mut board = sim.command(user, "play", json!([super::sub("picross", vec![])])).await;
    for y in 0..picross::SIZE {
        let values = row_values(&puzzle, y);
        if values.is_empty() {
            continue;
        }
        let picked = sim.pick(user, &board, "pz:r:", &y.to_string()).await;
        let refs: Vec<&str> = values.iter().map(String::as_str).collect();
        board = sim
            .component(user, &picked.menu("pz:c:").custom_id.clone(), &refs, &picked)
            .await;
    }
    assert!(
        board.description().contains("+**45** shells, a streak of **3** days"),
        "{}",
        board.description()
    );

    // Practice: a random puzzle each time, ten shells for each solve, five solves a day.
    let mut paid = 0;
    let mut practice = sim
        .command(
            user,
            "play",
            json!([super::sub("picross", vec![super::text_option("mode", "practice")])]),
        )
        .await;
    assert_eq!(practice.title(), "Practice picross");
    for round in 0..6 {
        let game = sim
            .app
            .db
            .picross_game(user, "practice", 0)
            .await
            .unwrap()
            .expect("a practice board");
        let puzzle = Puzzle::new(game.picture as usize, game.variant as u8);
        for y in 0..picross::SIZE {
            let values = row_values(&puzzle, y);
            if values.is_empty() {
                continue;
            }
            let picked = sim.pick(user, &practice, "pz:r:", &y.to_string()).await;
            let refs: Vec<&str> = values.iter().map(String::as_str).collect();
            practice = sim
                .component(user, &picked.menu("pz:c:").custom_id.clone(), &refs, &picked)
                .await;
        }
        if practice.description().contains("+**10** shells") {
            paid += 1;
        } else {
            assert!(
                practice.description().contains("limit"),
                "round {round}: {}",
                practice.description()
            );
        }
        practice = sim.click(user, &practice, "New puzzle").await;
        assert!(practice.has_button("Check"), "a fresh board");
    }
    assert_eq!(paid, 5, "the daily limit for practice");
    assert_eq!(record_of(&sim, user, "picross").await, (7, 7));
}

// /Tide 2048

#[tokio::test]
async fn tide_2048_slides_remembers_the_game_and_pays_for_a_big_tile() {
    let Some(mut sim) = Sim::new("tide_2048_slides_remembers_the_game_and_pays_for_a_big_tile").await else {
        return;
    };
    let user = unique();
    let board = sim.command(user, "play", json!([super::sub("2048", vec![])])).await;
    assert!(board.ephemeral);
    assert_eq!(board.files[0].0, "tide2048.png");
    assert_eq!(png_size(&board), (560, 712));
    assert!(
        board.has_button("Finish") && board.button("Finish").disabled,
        "ending needs a few moves"
    );
    // Some arrow moves the tiles; a disabled one would do nothing.
    let arrow = ["←", "↑", "↓", "→"]
        .into_iter()
        .find(|label| !board.button(label).disabled)
        .expect("some move is possible");
    let moved = sim.click(user, &board, arrow).await;
    assert!(
        moved.description().contains("Slide with the arrows"),
        "{}",
        moved.description()
    );
    let game = sim.app.db.solo_game(user, "tide2048").await.unwrap().expect("a game");
    assert_eq!(game.moves, 1);
    // Clicking the old board again shows the game as it is instead of moving twice.
    let stale = sim
        .component(user, &board.button(arrow).custom_id.clone().expect("id"), &[], &board)
        .await;
    assert_eq!(
        sim.app
            .db
            .solo_game(user, "tide2048")
            .await
            .unwrap()
            .expect("game")
            .moves,
        1
    );
    assert_eq!(stale.files.len(), 1, "the click shows the board as it is");
    // The command picks the same game up again.
    let again = sim.command(user, "play", json!([super::sub("2048", vec![])])).await;
    assert!(again.has_button("Finish"));
    assert_eq!(
        sim.app
            .db
            .solo_game(user, "tide2048")
            .await
            .unwrap()
            .expect("game")
            .moves,
        1
    );

    // A board one slide from the end: a latin-square of 8, 16 and 512 with one hole, so after the slide and
    // the new 2 or 4 nothing can merge.
    let values = [8u32, 16, 512];
    let mut board16 = tide2048::Board::new();
    for cell in 0..15usize {
        let (r, c) = (cell / 4, cell % 4);
        board16.set(cell, values[(c + 2 * r) % 3].trailing_zeros() as u8);
    }
    sqlx::query("UPDATE solo_games SET state = $2, score = 1234, moves = 40 WHERE user_id = $1 AND game = 'tide2048'")
        .bind(user as i64)
        .bind(board16.encode())
        .execute(sim.app.db.pool())
        .await
        .expect("set the board");
    let live = sim.command(user, "play", json!([super::sub("2048", vec![])])).await;
    assert!(
        !live.button("→").disabled && live.button("←").disabled,
        "left does nothing, right moves"
    );
    let over = sim.click(user, &live, "→").await;
    assert!(
        over.description().contains("Final score **1234**"),
        "{}",
        over.description()
    );
    assert!(over.description().contains("+**10** shells"), "{}", over.description());
    assert!(over.has_button("New game") && !over.has_button("Finish"));
    assert_eq!(shells(&sim, user).await, 10);
    assert_eq!(record_of(&sim, user, "tide2048").await, (1, 0));
    let stats = sim.command(user, "play", json!([super::sub("stats", vec![])])).await;
    assert!(
        stats.field("🌊 Tide 2048").contains("Best score **1,234**"),
        "{}",
        stats.field("🌊 Tide 2048")
    );
    // The finished game cannot be cashed in again.
    let replay = sim.click(user, &live, "→").await;
    assert_eq!(shells(&sim, user).await, 10);
    assert!(replay.has_button("New game"));
    let fresh = sim.click(user, &over, "New game").await;
    assert!(
        fresh.has_button("Finish") && fresh.description().contains("Slide with the arrows"),
        "{}",
        fresh.description()
    );
    assert_eq!(
        sim.app
            .db
            .solo_game(user, "tide2048")
            .await
            .unwrap()
            .expect("game")
            .moves,
        0
    );

    // Finishing by hand needs a few moves and counts the game.
    sqlx::query("UPDATE solo_games SET moves = 12, score = 900 WHERE user_id = $1 AND game = 'tide2048'")
        .bind(user as i64)
        .execute(sim.app.db.pool())
        .await
        .expect("set the moves");
    let board = sim.command(user, "play", json!([super::sub("2048", vec![])])).await;
    let ended = sim.click(user, &board, "Finish").await;
    assert!(
        ended.description().contains("Final score **900**"),
        "{}",
        ended.description()
    );
    assert_eq!(record_of(&sim, user, "tide2048").await, (2, 0));
    let best = sim.app.db.game_stats(user).await.unwrap();
    assert_eq!(
        best.iter().find(|s| s.game == "tide2048").map(|s| s.best),
        Some(1234),
        "the best score is kept"
    );
}
