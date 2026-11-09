//! Real EN/TR command/component flows for every reef party game, with PNG artifacts.
use crate::Reply;
use crate::SWEEPS;
use crate::Sim;
use crate::sub;
use crate::text_option;
use crate::turkish_server;
use crate::unique;
use pinchora_game::games::reef_party::Board;
use pinchora_game::games::reef_party::Kind;
use pinchora_game::games::reef_party::State;
use pinchora_game::play::PlayContent;
use serde_json::json;
fn custom(board: &Reply, action: &str) -> String {
    board
        .controls()
        .find_map(|c| {
            if let twilight_model::channel::message::Component::Button(b) = c {
                b.custom_id
                    .as_ref()
                    .filter(|id| id.split(':').nth(1) == Some(action))
                    .cloned()
            } else {
                None
            }
        })
        .unwrap_or_else(|| panic!("missing {action}: {board}"))
}
async fn click(sim: &mut Sim, user: u64, board: &Reply, action: &str) -> Reply {
    sim.component(user, &custom(board, action), &[], board).await
}
async fn state(sim: &Sim, id: i64) -> State {
    serde_json::from_str(&sim.app.db.game(id).await.unwrap().unwrap().state).unwrap()
}
async fn answer(sim: &mut Sim, user: u64, board: &Reply, value: &str) -> Reply {
    let modal = click(sim, user, board, "answer").await;
    let (id, _) = modal.modal.unwrap();
    sim.run("reef word".into(),user,5,json!({"custom_id":id,"components":[{"type":18,"id":1,"component":{"type":4,"id":2,"custom_id":"answer","value":value}}]}),Some(board)).await
}
async fn run(name: &'static str, kind: Kind, tr: bool, mode: &str) {
    let _serial = SWEEPS.lock().await;
    let Some(mut sim) = Sim::new(name).await else {
        return;
    };
    sim.channel = unique();
    if tr {
        turkish_server(&sim).await;
        sim.locale = "tr";
    }
    let host = unique();
    let guest = unique();
    let root = if kind.solo() { "play" } else { "party" };
    let options = if matches!(kind, Kind::Quiz | Kind::Words) {
        vec![text_option("mode", mode)]
    } else {
        vec![]
    };
    let mut board = sim.command(host, root, json!([sub(kind.key(), options)])).await;
    assert!(!board.files.is_empty() && !board.ephemeral, "{board}");
    assert!(board.pings_nobody());
    let control = board
        .controls()
        .find_map(|c| {
            if let twilight_model::channel::message::Component::Button(b) = c {
                b.custom_id.clone()
            } else {
                None
            }
        })
        .unwrap();
    let id = control.split(':').nth(2).unwrap().parse::<i64>().unwrap();
    let busy = sim.command(guest, "play", json!([sub("kelpmaze", vec![])])).await;
    assert!(
        busy.ephemeral && busy.content().contains(if tr { "sürüyor" } else { "already" }),
        "{busy}"
    );
    let original_message = board.message_id;
    let forged_message = unique();
    board.message_id = forged_message;
    assert!(click(&mut sim, unique(), &board, "leave").await.ephemeral);
    board.message_id = original_message;
    assert_ne!(
        sim.app.db.game(id).await.unwrap().unwrap().message_id,
        Some(forged_message as i64)
    );
    if !kind.solo() {
        board = click(&mut sim, guest, &board, "join").await;
        let no = click(&mut sim, guest, &board, "start").await;
        assert!(no.ephemeral);
        board = click(&mut sim, host, &board, "start").await;
    }
    let before = state(&sim, id).await.seq;
    let denied = click(&mut sim, unique(), &board, "leave").await;
    assert!(denied.ephemeral);
    assert_eq!(state(&sim, id).await.seq, before);
    if !kind.solo() {
        let stale = sim.component(host, &control, &[], &board).await;
        assert!(stale.ephemeral);
    }
    match kind {
        Kind::Quiz => {
            for _ in 0..5 {
                let s = state(&sim, id).await;
                let Board::Quiz { questions, order, .. } = &s.board else {
                    panic!()
                };
                let q = PlayContent::for_lang(s.lang).question(&questions[s.round]).unwrap();
                let correct = q.correct_slot(*order).unwrap();
                board = click(&mut sim, host, &board, &correct.to_string()).await;
                let dup = click(&mut sim, host, &board, &correct.to_string()).await;
                assert!(dup.ephemeral);
                board = click(&mut sim, guest, &board, &((correct + 1) % 4).to_string()).await;
            }
            assert_eq!(state(&sim, id).await.winners, vec![0]);
        }
        Kind::Story => {
            for i in 0..18 {
                board = answer(
                    &mut sim,
                    if i % 2 == 0 { host } else { guest },
                    &board,
                    if tr { "deniz" } else { "sea" },
                )
                .await;
            }
            assert_eq!(state(&sim, id).await.winners.len(), 2);
        }
        Kind::Room => {
            for _ in 0..4 {
                let s = state(&sim, id).await;
                let Board::Room { rooms, order, .. } = &s.board else {
                    panic!()
                };
                let correct = order.iter().position(|r| *r == rooms[s.round]).unwrap();
                board = click(&mut sim, host, &board, &correct.to_string()).await;
                board = click(&mut sim, guest, &board, &((correct + 1) % 4).to_string()).await;
            }
        }
        Kind::Critter => {
            for _ in 0..4 {
                board = click(&mut sim, host, &board, "yes").await;
            }
        }
        Kind::Chess => {
            let before = sim.app.db.game(id).await.unwrap().unwrap().state;
            let denied = click(&mut sim, guest, &board, "answer").await;
            assert!(denied.ephemeral && denied.modal.is_none());
            let modal = click(&mut sim, host, &board, "answer").await;
            let (form, _) = modal.modal.unwrap();
            let forged = sim.run("stolen chess form".into(), guest, 5,
                json!({"custom_id":form,"components":[{"type":18,"id":1,"component":{"type":4,"id":2,"custom_id":"answer","value":"e2e4"}}]}), Some(&board)).await;
            assert!(forged.ephemeral);
            assert!(answer(&mut sim, host, &board, "e2e5").await.ephemeral);
            assert_eq!(sim.app.db.game(id).await.unwrap().unwrap().state, before);
            board = answer(&mut sim, host, &board, "e2e4").await;
            let s = state(&sim, id).await;
            let Board::Chess { moves } = &s.board else { panic!() };
            assert_eq!(pinchora_game::play::chess::Game::from_moves(moves).unwrap().plies(), 2);
            board = click(&mut sim, host, &board, "leave").await;
        }
        Kind::Maze => {
            let s = state(&sim, id).await;
            let Board::Maze(m) = s.board else { panic!() };
            let mut queue = std::collections::VecDeque::from([0]);
            let mut prev = vec![None; 49];
            prev[0] = Some(0);
            while let Some(at) = queue.pop_front() {
                for (to, prior) in prev.iter_mut().enumerate() {
                    if m.cells[to]
                        && prior.is_none()
                        && (at / 7 == to / 7 && at.abs_diff(to) == 1 || at.abs_diff(to) == 7)
                    {
                        *prior = Some(at);
                        queue.push_back(to);
                    }
                }
            }
            let mut path = vec![48];
            while *path.last().unwrap() != 0 {
                path.push(prev[*path.last().unwrap()].unwrap());
            }
            path.reverse();
            for pair in path.windows(2) {
                let action = match pair[1] as i32 - pair[0] as i32 {
                    1 => "right",
                    -1 => "left",
                    7 => "down",
                    _ => "up",
                };
                board = click(&mut sim, host, &board, action).await;
            }
            assert_eq!(state(&sim, id).await.winners, vec![0]);
        }
        Kind::Match => {
            for _ in 0..20 {
                let s = state(&sim, id).await;
                if s.done {
                    break;
                }
                let Board::Match(m) = s.board else { panic!() };
                let pair = (0usize..16)
                    .flat_map(|a| (a + 1..16).map(move |b| (a, b)))
                    .find(|&(a, b)| {
                        if !(a.abs_diff(b) == 4 || a / 4 == b / 4 && a.abs_diff(b) == 1) {
                            return false;
                        }
                        let mut copy = m.clone();
                        copy.cells.swap(a, b);
                        !copy.matches().is_empty()
                    })
                    .unwrap();
                let deadline = sim.app.db.game(id).await.unwrap().unwrap().expires_at;
                board = click(&mut sim, host, &board, &pair.0.to_string()).await;
                assert_eq!(sim.app.db.game(id).await.unwrap().unwrap().expires_at, deadline);
                board = click(&mut sim, host, &board, &pair.1.to_string()).await;
            }
        }
        Kind::Words => {
            for _ in 0..4 {
                let s = state(&sim, id).await;
                let Board::Words { fragment, used, .. } = &s.board else {
                    panic!()
                };
                let content = if tr {
                    include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../content/pearl_words_tr.txt"))
                } else {
                    include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../content/pearl_words.txt"))
                };
                let word = content
                    .lines()
                    .find(|w| {
                        pinchora_game::party::hotpearl::check_word(
                            pinchora_game::party::hotpearl::dictionary_for(s.lang),
                            fragment,
                            w,
                            used,
                        )
                        .is_ok()
                    })
                    .unwrap();
                board = answer(&mut sim, if s.turn == 0 { host } else { guest }, &board, word).await;
            }
            for _ in 0..12 {
                if state(&sim, id).await.done {
                    break;
                }
                sim.app.shift_clock(chrono::Duration::seconds(21));
                pinchora_commands::reef_party_tick(&sim.app).await.unwrap();
            }
        }
        Kind::Survival => {
            board = click(&mut sim, host, &board, "ally-1").await;
            board = click(&mut sim, guest, &board, "ally-0").await;
            for _ in 0..8 {
                if state(&sim, id).await.done {
                    break;
                }
                for (seat, user) in [(0, host), (1, guest)] {
                    let s = state(&sim, id).await;
                    if !s.done && s.seats[seat].active {
                        board = click(&mut sim, user, &board, "shelter").await;
                    }
                }
            }
        }
    }
    let done = state(&sim, id).await;
    assert!(done.done, "{kind:?}: {board}");
    let stats = sim.app.db.game_stats(host).await.unwrap();
    assert!(stats.iter().any(|r| r.game == kind.key() && r.played == 1));
    let replay = sim.component(host, &control, &[], &board).await;
    assert!(replay.ephemeral);
    assert_eq!(
        sim.app
            .db
            .game_stats(host)
            .await
            .unwrap()
            .iter()
            .find(|r| r.game == kind.key())
            .unwrap()
            .played,
        1
    );
}
macro_rules! flow {
    ($name:ident,$kind:ident,$tr:expr,$mode:expr) => {
        #[tokio::test]
        async fn $name() {
            run(stringify!($name), Kind::$kind, $tr, $mode).await;
        }
    };
}
flow!(d9_team_en, Quiz, false, "team");
flow!(d9_team_tr, Quiz, true, "team");
flow!(d9_quick_en, Quiz, false, "quick");
flow!(d9_quick_tr, Quiz, true, "quick");
flow!(d9_words_en, Words, false, "relay");
flow!(d9_words_tr, Words, true, "relay");
flow!(d9_story_en, Story, false, "");
flow!(d9_story_tr, Story, true, "");
flow!(d9_room_en, Room, false, "");
flow!(d9_room_tr, Room, true, "");
flow!(d9_critter_en, Critter, false, "");
flow!(d9_critter_tr, Critter, true, "");
flow!(d9_chess_en, Chess, false, "");
flow!(d9_chess_tr, Chess, true, "");
flow!(d9_maze_en, Maze, false, "");
flow!(d9_maze_tr, Maze, true, "");
flow!(d9_match_en, Match, false, "");
flow!(d9_match_tr, Match, true, "");
flow!(d9_survival_en, Survival, false, "");
flow!(d9_survival_tr, Survival, true, "");

#[tokio::test]
async fn d9_lobby_and_solo_expiry_release_channels_in_both_languages() {
    let _serial = SWEEPS.lock().await;
    let Some(mut sim) = Sim::new("d9_expiry").await else {
        return;
    };
    for tr in [false, true] {
        sim.channel = unique();
        if tr {
            turkish_server(&sim).await;
            sim.locale = "tr";
        }
        let host = unique();
        let board = sim.command(host, "party", json!([sub("lastreef", vec![])])).await;
        let old = custom(&board, "start");
        sim.app.shift_clock(chrono::Duration::seconds(91));
        assert!(pinchora_commands::reef_party_tick(&sim.app).await.unwrap() > 0);
        let gone = sim.component(host, &old, &[], &board).await;
        assert!(gone.ephemeral);
        let board = sim.command(host, "play", json!([sub("kelpmaze", vec![])])).await;
        assert!(!board.files.is_empty());
        let old = custom(&board, "right");
        sim.app.shift_clock(chrono::Duration::seconds(121));
        let gone = sim.component(host, &old, &[], &board).await;
        assert!(gone.ephemeral);
        let new = sim.command(host, "play", json!([sub("gemtide", vec![])])).await;
        assert!(!new.files.is_empty());
        click(&mut sim, host, &new, "leave").await;
    }
}

#[tokio::test]
async fn d9_idle_guests_and_self_reported_guesses_never_pay() {
    let _serial = SWEEPS.lock().await;
    let Some(mut sim) = Sim::new("d9_no_solo_party_rewards").await else {
        return;
    };
    for tr in [false, true] {
        if tr {
            turkish_server(&sim).await;
            sim.locale = "tr";
        }
        for kind in [Kind::Quiz, Kind::Critter] {
            sim.channel = unique();
            let (host, guest) = (unique(), unique());
            let mut board = sim.command(host, "party", json!([sub(kind.key(), vec![])])).await;
            let id: i64 = custom(&board, "start").split(':').nth(2).unwrap().parse().unwrap();
            board = click(&mut sim, guest, &board, "join").await;
            board = click(&mut sim, host, &board, "start").await;
            if kind == Kind::Critter {
                for _ in 0..4 {
                    board = click(&mut sim, host, &board, "yes").await;
                }
            } else {
                let s = state(&sim, id).await;
                let Board::Quiz { questions, order, .. } = &s.board else {
                    panic!()
                };
                let correct = PlayContent::for_lang(s.lang)
                    .question(&questions[0])
                    .unwrap()
                    .correct_slot(*order)
                    .unwrap();
                board = click(&mut sim, host, &board, &correct.to_string()).await;
                click(&mut sim, guest, &board, "leave").await;
            }
            assert!(state(&sim, id).await.done);
            if kind == Kind::Critter {
                assert!(board.embeds[0].description.as_deref().unwrap().contains(if tr {
                    "kabuk ödülü verilmez"
                } else {
                    "no shells are awarded"
                }));
            }
            assert_eq!(sim.app.db.user(host).await.unwrap().credits, 0);
            assert_eq!(sim.app.db.user(guest).await.unwrap().credits, 0);
            assert!(
                !sim.command(host, "play", json!([sub("kelpmaze", vec![])]))
                    .await
                    .files
                    .is_empty()
            );
        }
    }
}

#[tokio::test]
async fn d9_deleted_board_does_not_block_other_channel_expiry() {
    let _serial = SWEEPS.lock().await;
    let Some(mut sim) = Sim::new("d9_deleted_board_expiry").await else {
        return;
    };
    let host = unique();
    sim.channel = unique();
    let board = sim.command(host, "play", json!([sub("kelpmaze", vec![])])).await;
    let first: i64 = custom(&board, "leave").split(':').nth(2).unwrap().parse().unwrap();
    sim.app
        .db
        .remember_game_message(first, sim.channel, board.message_id)
        .await
        .unwrap();
    sim.mock.deleted.lock().unwrap().push(board.message_id);
    sim.channel = unique();
    let board = sim.command(host, "play", json!([sub("reefmind", vec![])])).await;
    let second: i64 = custom(&board, "leave").split(':').nth(2).unwrap().parse().unwrap();
    sim.app.shift_clock(chrono::Duration::seconds(121));
    pinchora_commands::reef_party_tick(&sim.app).await.unwrap();
    assert!(state(&sim, first).await.done);
    assert!(state(&sim, second).await.done);
    assert!(
        !sim.command(host, "play", json!([sub("gemtide", vec![])]))
            .await
            .files
            .is_empty()
    );
}
