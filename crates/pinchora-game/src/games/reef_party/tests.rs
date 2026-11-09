use super::*;

fn started(kind: Kind, lang: Lang, mode: &str) -> State {
    let mut s = State::new(kind, lang, mode, "all", 42);
    if s.lobby {
        s.add_seat();
        assert!(s.start());
    }
    s
}
#[test]
fn party_rewards_require_two_actual_players_and_exclude_self_reported_critter() {
    for kind in [
        Kind::Quiz,
        Kind::Words,
        Kind::Story,
        Kind::Room,
        Kind::Survival,
        Kind::Critter,
    ] {
        let mut s = started(kind, Lang::En, "quick");
        s.seats[0].actions = 4;
        s.seats[0].score = 4;
        s.finish();
        assert!(
            s.reward_seats().is_empty(),
            "{kind:?}: an idle second seat is not participation"
        );
        s.seats[1].actions = 2;
        if kind == Kind::Critter {
            assert!(s.reward_seats().is_empty(), "self-reported guesses cannot earn shells");
        } else {
            assert_eq!(s.reward_seats(), vec![0]);
        }
    }
}

#[test]
fn departing_answers_do_not_skip_other_players() {
    for kind in [Kind::Quiz, Kind::Room, Kind::Survival] {
        let mut s = State::new(kind, Lang::En, "quick", "all", 42);
        s.add_seat();
        s.add_seat();
        s.start();
        let action = if kind == Kind::Survival { "shelter" } else { "0" };
        s.act(0, action, false).unwrap();
        s.forfeit(0);
        s.act(1, action, false).unwrap();
        assert_eq!(s.round, 0, "{kind:?}: seat 2 still needs a turn");
        s.act(2, action, false).unwrap();
        assert_eq!(s.round, 1);
    }
}

#[test]
fn last_unanswered_departure_advances_the_round() {
    for kind in [Kind::Quiz, Kind::Room, Kind::Survival] {
        let mut s = State::new(kind, Lang::En, "quick", "all", 42);
        s.add_seat();
        s.add_seat();
        s.start();
        let action = if kind == Kind::Survival { "shelter" } else { "0" };
        s.act(0, action, false).unwrap();
        s.act(1, action, false).unwrap();
        s.forfeit(2);
        assert_eq!(s.round, 1, "{kind:?}: all remaining players have answered");
    }
}

#[test]
fn longest_word_departure_awards_the_remaining_best_word_after_resume() {
    let mut s = State::new(Kind::Words, Lang::En, "longest", "all", 42);
    s.add_seat();
    s.add_seat();
    s.start();
    let Board::Words { fragment, .. } = &mut s.board else {
        panic!()
    };
    *fragment = "ea".into();
    s.act(0, "pearl", false).unwrap();
    s.act(2, "sea", false).unwrap();
    let mut s: State = serde_json::from_str(&serde_json::to_string(&s).unwrap()).unwrap();
    s.forfeit(0);
    s.timeout();
    assert_eq!(s.seats[0].score, 0);
    assert_eq!(s.seats[1].score, 0);
    assert_eq!(s.seats[2].score, 1);
    let Board::Words { longest, .. } = &s.board else {
        panic!()
    };
    assert!(longest.is_empty());
}

#[test]
fn old_word_rounds_without_candidate_history_still_resume() {
    let s = started(Kind::Words, Lang::En, "longest");
    let mut saved = serde_json::to_value(s).unwrap();
    saved["board"]["Words"].as_object_mut().unwrap().remove("longest");
    let mut restored: State = serde_json::from_value(saved).unwrap();
    restored.timeout();
    assert_eq!(restored.round, 1);
}

#[test]
fn turkish_wordwake_preserves_every_turkish_letter() {
    let mut s = started(Kind::Words, Lang::Tr, "shoal");
    for (fragment, word, normalized) in [
        ("iş", "İŞÇİ", "işçi"),
        ("ış", "IŞIK", "ışık"),
        ("ağ", "AĞAÇ", "ağaç"),
        ("gö", "GÖZ", "göz"),
        ("gü", "GÜNEŞ", "güneş"),
    ] {
        if let Board::Words { fragment: f, .. } = &mut s.board {
            *f = fragment.into();
        }
        s.act(0, word, false).unwrap();
        let Board::Words { last, .. } = &s.board else { panic!() };
        assert_eq!(last, normalized);
        assert_eq!(s.act(1, normalized, false), Err("word"));
    }
}

#[test]
fn quiz_packs_do_not_repeat_paraphrased_facts() {
    let repeated = [
        ["science_tr_015", "science_tr_t01"],
        ["nature_tr_001", "science_tr_t02"],
        ["nature_tr_035", "science_tr_t03"],
        ["nature_tr_031", "science_tr_t05"],
        ["nature_tr_036", "science_tr_t07"],
        ["ocean_tr_012", "ocean_tr_t05"],
        ["ocean_tr_003", "ocean_tr_t08"],
        ["geography_tr_t10", "ocean_tr_t06"],
    ];
    for (category, seed) in [("science", 712), ("ocean", 55), ("all", 547)] {
        let s = State::new(Kind::Quiz, Lang::Tr, "team", category, seed);
        let Board::Quiz { questions, .. } = s.board else {
            panic!()
        };
        for pair in repeated {
            assert!(
                !pair.iter().all(|id| questions.iter().any(|q| q == id)),
                "{category}: {pair:?}"
            );
        }
    }
}

#[test]
fn reviewed_nature_answers_match_the_question_in_both_languages() {
    // USGS: magnitude is distinct from shaking intensity. NOAA: hail grows in thunderstorms.
    for (lang, prefix, magnitude, storm, richter, hail) in [
        (
            Lang::En,
            "nature_",
            "magnitude",
            "thunderstorm",
            "Richter scale",
            "Hail",
        ),
        (
            Lang::Tr,
            "nature_tr_",
            "büyüklüğünü",
            "fırtına",
            "Richter ölçeği",
            "Dolu",
        ),
    ] {
        let content = PlayContent::for_lang(lang);
        let q = content.question(&format!("{prefix}041")).unwrap();
        assert!(q.question.contains(magnitude));
        assert_eq!(q.answer, richter);
        let q = content.question(&format!("{prefix}047")).unwrap();
        assert!(q.question.contains(storm));
        assert_eq!(q.answer, hail);
    }
}

#[test]
fn quiz_packs_unique_and_complete_in_both_languages() {
    for lang in [Lang::En, Lang::Tr] {
        for cat in ["all", "ocean", "science", "nature"] {
            let s = State::new(Kind::Quiz, lang, "team", cat, 1);
            let Board::Quiz { questions, .. } = s.board else {
                panic!()
            };
            assert_eq!(questions.len(), 5);
            let qs: Vec<_> = questions
                .iter()
                .map(|id| PlayContent::for_lang(lang).question(id).unwrap())
                .collect();
            assert_eq!(
                qs.iter()
                    .map(|q| &q.question)
                    .collect::<std::collections::HashSet<_>>()
                    .len(),
                5
            );
            for q in qs {
                assert_eq!(q.options.len(), 4);
                assert!(q.options.contains(&q.answer));
            }
        }
    }
}
#[test]
fn team_scores_and_quickfire_finish() {
    for lang in [Lang::En, Lang::Tr] {
        for mode in ["team", "quick"] {
            let mut s = started(Kind::Quiz, lang, mode);
            for _ in 0..5 {
                let Board::Quiz { questions, order, .. } = &s.board else {
                    panic!()
                };
                let correct = PlayContent::for_lang(lang)
                    .question(&questions[s.round])
                    .unwrap()
                    .correct_slot(*order)
                    .unwrap();
                s.act(0, &correct.to_string(), false).unwrap();
                assert_eq!(s.act(0, "0", false), Err("answered"));
                s.act(1, &((correct + 1) % 4).to_string(), false).unwrap();
            }
            assert!(s.done);
            assert_eq!(s.winners, vec![0]);
        }
    }
}
#[test]
fn story_is_single_word_turn_based_and_filtered() {
    for lang in [Lang::En, Lang::Tr] {
        let mut s = started(Kind::Story, lang, "");
        assert_eq!(s.act(1, "sea", false), Err("turn"));
        assert_eq!(s.act(0, "two words", false), Err("word"));
        assert_eq!(s.act(0, "@everyone", false), Err("word"));
        for _ in 0..18 {
            let seat = s.turn;
            s.act(seat, if lang == Lang::Tr { "deniz" } else { "sea" }, false)
                .unwrap();
        }
        assert!(s.done);
        assert_eq!(s.winners.len(), 2);
    }
}
#[test]
fn words_reject_duplicates_and_wrong_turns() {
    for lang in [Lang::En, Lang::Tr] {
        for mode in ["relay", "chain", "longest", "shoal", "mixed"] {
            let mut s = started(Kind::Words, lang, mode);
            let Board::Words { fragment, .. } = &mut s.board else {
                panic!()
            };
            *fragment = if lang == Lang::Tr { "de" } else { "ea" }.into();
            let word = if lang == Lang::Tr { "deniz" } else { "pearl" };
            s.act(0, word, false).unwrap();
            if let Board::Words { fragment, .. } = &mut s.board {
                *fragment = if lang == Lang::Tr { "de" } else { "ea" }.into();
            }
            assert!(s.act(s.turn, word, false).is_err());
            for _ in 0..40 {
                if s.done {
                    break;
                }
                s.timeout();
            }
            assert!(s.done);
        }
    }
}
#[test]
fn room_rounds_never_repeat() {
    for lang in [Lang::En, Lang::Tr] {
        let mut s = started(Kind::Room, lang, "");
        let Board::Room { rooms, .. } = &s.board else { panic!() };
        assert_eq!(rooms.iter().collect::<std::collections::HashSet<_>>().len(), 4);
        for _ in 0..4 {
            let Board::Room { rooms, order, .. } = &s.board else {
                panic!()
            };
            let pick = order.iter().position(|r| *r == rooms[s.round]).unwrap();
            s.act(0, &pick.to_string(), false).unwrap();
            s.timeout();
        }
        assert!(s.done);
        assert_eq!(s.winners, vec![0]);
    }
}
#[test]
fn compass_covers_every_creature() {
    for lang in [Lang::En, Lang::Tr] {
        for creature in 0..8 {
            let mut s = started(Kind::Critter, lang, "");
            for bit in 0..3 {
                s.act(0, if creature & (1 << bit) > 0 { "yes" } else { "no" }, false)
                    .unwrap();
            }
            let Board::Critter { bits, .. } = &s.board else {
                panic!()
            };
            assert_eq!(*bits, creature);
            s.act(0, "yes", false).unwrap();
            assert!(s.done);
        }
    }
}
#[test]
fn chess_does_not_end_a_legal_game_at_an_arbitrary_ply_limit() {
    let mut game = chess::Game::new();
    let mut seed = 17;
    for _ in 0..238 {
        let mut candidates = game.position.legal_moves();
        let mut chosen = None;
        while !candidates.is_empty() {
            let index = next(&mut seed) as usize % candidates.len();
            let mv = candidates.swap_remove(index);
            let mut after = game.clone();
            after.play(mv).unwrap();
            if after.outcome().is_none() {
                chosen = Some(after);
                break;
            }
        }
        game = chosen.expect("the fixture can continue legally");
    }
    let mut s = State::new(Kind::Chess, Lang::En, "easy", "all", 1);
    s.board = Board::Chess {
        moves: game.moves_text(),
    };
    for mv in game.position.legal_moves() {
        let mut candidate = s.clone();
        candidate.act(0, &mv.uci(), false).unwrap();
        let Board::Chess { moves } = &candidate.board else {
            panic!()
        };
        let after = chess::Game::from_moves(moves).unwrap();
        if after.outcome().is_none() {
            assert_eq!(after.plies(), 240);
            assert!(!candidate.done, "only a rules outcome, forfeit or timeout ends chess");
            return;
        }
    }
    panic!("the fixture has a non-terminal continuation");
}

#[test]
fn chess_moves_ai_and_forfeit() {
    for lang in [Lang::En, Lang::Tr] {
        let mut s = started(Kind::Chess, lang, "easy");
        assert_eq!(s.act(0, "e2e5", false), Err("move"));
        s.act(0, "e2e4", false).unwrap();
        let Board::Chess { moves } = &s.board else { panic!() };
        assert_eq!(chess::Game::from_moves(moves).unwrap().plies(), 2);
        s.forfeit(0);
        assert!(s.done);
        assert!(s.reward_seats().is_empty());
    }
}
#[test]
fn survival_alliances_require_reciprocity_and_finish() {
    for lang in [Lang::En, Lang::Tr] {
        let mut s = started(Kind::Survival, lang, "");
        s.act(0, "ally-1", false).unwrap();
        assert_eq!(s.seats[1].ally, None);
        s.act(1, "ally-0", false).unwrap();
        for _ in 0..8 {
            if s.done {
                break;
            }
            for seat in 0..2 {
                if !s.done && s.seats[seat].active {
                    s.act(seat, "shelter", false).unwrap();
                }
            }
        }
        assert!(s.done);
    }
}
#[test]
fn expiry_and_serialization_for_every_mode() {
    for lang in [Lang::En, Lang::Tr] {
        for kind in [
            Kind::Quiz,
            Kind::Words,
            Kind::Story,
            Kind::Room,
            Kind::Critter,
            Kind::Chess,
            Kind::Maze,
            Kind::Match,
            Kind::Survival,
        ] {
            let s = State::new(kind, lang, "relay", "all", 3);
            let mut restored: State = serde_json::from_str(&serde_json::to_string(&s).unwrap()).unwrap();
            restored.timeout();
            assert!(restored.done);
            assert!(restored.reward_seats().is_empty());
        }
    }
}
