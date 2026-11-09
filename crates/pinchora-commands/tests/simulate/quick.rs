//! EN/TR simulator paths through every quick game, including stale and forged component ids.
use super::{SWEEPS, Sim, duel, int_option, play_sub, record_of, sub, text_option, turkish_server, unique};
use pinchora_commands::party_tick;
use pinchora_game::play::quick::Othello;
use serde_json::{Value, json};

async fn state(sim: &Sim, user: u64, game: &str) -> Value {
    serde_json::from_str(&sim.app.db.solo_game(user, game).await.unwrap().unwrap().state).unwrap()
}
fn cell_label(n: usize, size: usize) -> String {
    format!("{}{}", char::from(b'A' + (n % size) as u8), n / size + 1)
}
fn memory_label(n: usize) -> String {
    format!("{}{}", char::from(b'A' + (n / 4) as u8), n % 4 + 1)
}
async fn answer(sim: &mut Sim, user: u64, board: &super::Reply, label: &str, value: &str) -> super::Reply {
    let modal = sim.click(user, board, label).await;
    sim.submit_fields(user, &modal, &[("answer", value)]).await
}
async fn run_solo(name: &'static str, tr: bool) {
    let _serial = SWEEPS.lock().await;
    let Some(mut sim) = Sim::new(name).await else {
        return;
    };
    sim.locale = if tr { "tr" } else { "en-US" };
    let games = [
        "memory",
        "minesweeper",
        "higherlower",
        "anagram",
        "typing",
        "guessnumber",
    ];
    for kind in games {
        let user = unique();
        let options = if kind == "minesweeper" && tr {
            vec![int_option("size", 10)]
        } else {
            vec![]
        };
        let mut board = sim.command(user, "play", play_sub(kind, options)).await;
        assert!(board.ephemeral && !board.files.is_empty(), "{kind}: {board}");
        assert_eq!(board.files[0].0, format!("{kind}.png"));
        let button = board
            .controls()
            .find_map(|c| match c {
                twilight_model::channel::message::Component::Button(b) => b.custom_id.clone(),
                _ => None,
            })
            .unwrap();
        let parts: Vec<&str> = button.split(':').collect();
        let forged = format!("{}:{}:{}:0", parts[0], parts[1], parts[2]);
        let refused = sim.component(user, &forged, &[], &board).await;
        assert!(
            refused.ephemeral && refused.content().contains(if tr { "süresi" } else { "timed out" }),
            "{kind}: {refused}"
        );
        match kind {
            "memory" => {
                let s = state(&sim, user, kind).await;
                let pairs: Vec<u64> = s["payload"]["Memory"]["pairs"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|v| v.as_u64().unwrap())
                    .collect();
                for pair in 0..8 {
                    let at: Vec<usize> = (0..16).filter(|&i| pairs[i] == pair).collect();
                    for i in at {
                        board = sim.click(user, &board, &memory_label(i)).await;
                    }
                }
            }
            "minesweeper" => {
                let label = if tr { "Kare seç" } else { "Pick square" };
                board = answer(&mut sim, user, &board, label, "A1").await;
                let s = state(&sim, user, kind).await;
                let mines = s["payload"]["Mines"]["mines"].as_array().unwrap();
                let n = mines.iter().position(|v| v.as_bool() == Some(true)).unwrap();
                let size = if tr { 10 } else { 8 };
                board = answer(&mut sim, user, &board, label, &cell_label(n, size)).await;
            }
            "higherlower" => {
                for _ in 0..8 {
                    if sim.app.db.solo_game(user, kind).await.unwrap().unwrap().over() {
                        break;
                    }
                    board = sim.click(user, &board, if tr { "Yüksek" } else { "Higher" }).await;
                }
            }
            "anagram" => {
                let s = state(&sim, user, kind).await;
                let word = s["payload"]["Anagram"]["word"].as_str().unwrap();
                board = answer(&mut sim, user, &board, if tr { "Yanıtla" } else { "Answer" }, word).await;
            }
            "typing" => {
                let s = state(&sim, user, kind).await;
                let sentence = s["payload"]["Typing"]["sentence"].as_str().unwrap();
                assert!(
                    !board.description().contains(sentence),
                    "typing prompt must only be in the PNG"
                );
                board = answer(&mut sim, user, &board, if tr { "Yanıtla" } else { "Answer" }, sentence).await;
            }
            "guessnumber" => {
                let s = state(&sim, user, kind).await;
                let n = s["payload"]["GuessNumber"]["answer"].as_u64().unwrap();
                board = answer(
                    &mut sim,
                    user,
                    &board,
                    if tr { "Yanıtla" } else { "Answer" },
                    &n.to_string(),
                )
                .await;
            }
            _ => unreachable!(),
        }
        assert!(
            sim.app.db.solo_game(user, kind).await.unwrap().unwrap().over(),
            "{kind}: {board}"
        );
        assert_eq!(record_of(&sim, user, kind).await.0, 1, "{kind}");
        let stale = sim.component(user, &button, &[], &board).await;
        assert!(stale.ephemeral, "{kind}: {stale}");
        assert_eq!(record_of(&sim, user, kind).await.0, 1, "{kind} paid twice");
    }
}
#[tokio::test]
async fn quick_solo_english_flows() {
    run_solo("quick_solo_english_flows", false).await;
}
#[tokio::test]
async fn quick_solo_turkish_flows() {
    run_solo("quick_solo_turkish_flows", true).await;
}

#[tokio::test]
async fn quick_solo_timeout_is_counted_once_by_the_janitor() {
    let _serial = SWEEPS.lock().await;
    let Some(mut sim) = Sim::new("quick_solo_timeout_is_counted_once_by_the_janitor").await else {
        return;
    };
    let user = unique();
    let board = sim.command(user, "play", play_sub("typing", vec![])).await;
    assert!(board.ephemeral);
    sim.app.shift_clock(chrono::Duration::minutes(2));
    assert!(super::sweep_games(&sim.app).await.unwrap() >= 1);
    assert!(sim.app.db.solo_game(user, "typing").await.unwrap().unwrap().over());
    assert_eq!(record_of(&sim, user, "typing").await, (1, 0));
    assert_eq!(super::sweep_games(&sim.app).await.unwrap(), 0);
    assert_eq!(record_of(&sim, user, "typing").await, (1, 0));
}

async fn run_public(name: &'static str, tr: bool) {
    let _serial = SWEEPS.lock().await;
    // This flow advances both game janitors, including the shared party-game clock.
    let _party = super::words::PARTY.lock().await;
    let Some(mut sim) = Sim::new(name).await else {
        return;
    };
    sim.locale = if tr { "tr" } else { "en-US" };
    sim.channel = unique();
    if tr {
        turkish_server(&sim).await;
    }
    let (host, guest, stranger) = (unique(), unique(), unique());
    let challenge = duel(&mut sim, host, guest, "othello", vec![]).await;
    let board = sim
        .click(guest, &challenge, if tr { "Kabul et" } else { "Accept" })
        .await;
    assert_eq!(board.files[0].0, "othello.png");
    assert!(board.pings_nobody());
    let id = super::session_of(&board);
    let session = sim.app.db.game(id).await.unwrap().unwrap();
    let raw = session.state.split('|').next().unwrap();
    let s: Value = serde_json::from_str(raw).unwrap();
    let turn = s["game"]["turn"].as_u64().unwrap() as u8;
    let game = Othello {
        cells: s["game"]["cells"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_u64().unwrap() as u8)
            .collect(),
        turn,
        passes: 0,
    };
    let cell = game.legal(turn)[0];
    let mover = if s["pearl"].as_u64().unwrap() == 0 { host } else { guest };
    let mover = if turn == 1 {
        mover
    } else if mover == host {
        guest
    } else {
        host
    };
    let button = board
        .controls()
        .find_map(|c| match c {
            twilight_model::channel::message::Component::Button(b) => b.custom_id.clone(),
            _ => None,
        })
        .unwrap();
    let forged = button.rsplit_once(':').unwrap().0.to_owned() + ":0";
    let fake_modal = sim.component(mover, &forged, &[], &board).await;
    let bad = sim
        .submit_fields(mover, &fake_modal, &[("coord", &cell_label(cell, 8))])
        .await;
    assert!(bad.ephemeral, "{bad}");
    let modal = sim
        .click(mover, &board, if tr { "İnci yerleştir" } else { "Place a pearl" })
        .await;
    let next = sim
        .submit_fields(mover, &modal, &[("coord", &cell_label(cell, 8))])
        .await;
    assert_eq!(next.files[0].0, "othello.png");
    let outsider = sim.component(stranger, &button, &[], &board).await;
    assert!(outsider.modal.is_some() || outsider.ephemeral);
    sqlx::query("UPDATE game_sessions SET expires_at = now() - interval '1 second' WHERE id = $1")
        .bind(id)
        .execute(sim.app.db.pool())
        .await
        .unwrap();
    super::sweep_games(&sim.app).await.unwrap();
    assert_eq!(sim.app.db.game(id).await.unwrap().unwrap().status, "done");

    let question = sim.command(host, "party", json!([sub("nhie", vec![])])).await;
    assert_eq!(question.files[0].0, "nhie.png");
    assert!(question.pings_nobody());
    let vote_id = question
        .controls()
        .find_map(|c| match c {
            twilight_model::channel::message::Component::Button(b) => b.custom_id.clone(),
            _ => None,
        })
        .unwrap();
    let forged = vote_id.rsplit_once(':').unwrap().0.to_owned() + ":0";
    let bad = sim.component(guest, &forged, &[], &question).await;
    assert!(bad.ephemeral);
    let saved = sim.click(guest, &question, if tr { "Yaptım" } else { "I have" }).await;
    assert!(saved.ephemeral);
    sim.app.shift_clock(chrono::Duration::minutes(6));
    party_tick(&sim.app).await.unwrap();
    assert_eq!(record_of(&sim, guest, "nhie").await, (1, 1));
    let fun = sim
        .command(host, "party", json!([sub("fun", vec![text_option("mode", "roll")])]))
        .await;
    assert_eq!(fun.files[0].0, "fun.png");
}
#[tokio::test]
async fn quick_public_english_flows() {
    run_public("quick_public_english_flows", false).await;
}
#[tokio::test]
async fn quick_public_turkish_flows() {
    run_public("quick_public_turkish_flows", true).await;
}
