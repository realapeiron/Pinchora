use super::*;

fn perft(fen: &str, depth: u32) -> u64 {
    Position::from_fen(fen).expect("a good FEN").perft(depth)
}

#[test]
fn the_start_position_matches_the_known_perft_counts() {
    for (depth, nodes) in [(1, 20), (2, 400), (3, 8_902), (4, 197_281)] {
        assert_eq!(Position::start().perft(depth), nodes, "depth {depth}");
    }
}

#[test]
fn kiwipete_matches_the_known_perft_counts() {
    let kiwipete = "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1";
    for (depth, nodes) in [(1, 48), (2, 2_039), (3, 97_862)] {
        assert_eq!(perft(kiwipete, depth), nodes, "depth {depth}");
    }
}

#[test]
fn the_other_standard_test_positions_match() {
    // En passant and pins, castling out of and through check, and promotions.
    let endgame = "8/2p5/3p4/KP5r/1R3p1k/8/4P1P1/8 w - - 0 1";
    for (depth, nodes) in [(1, 14), (2, 191), (3, 2_812), (4, 43_238)] {
        assert_eq!(perft(endgame, depth), nodes, "endgame depth {depth}");
    }
    let mirrored = "r3k2r/Pppp1ppp/1b3nbN/nP6/BBP1P3/q4N2/Pp1P2PP/R2Q1RK1 w kq - 0 1";
    for (depth, nodes) in [(1, 6), (2, 264), (3, 9_467)] {
        assert_eq!(perft(mirrored, depth), nodes, "promotions depth {depth}");
    }
    let promotion = "rnbq1k1r/pp1Pbppp/2p5/8/2B5/8/PPP1NnPP/RNBQK2R w KQ - 1 8";
    for (depth, nodes) in [(1, 44), (2, 1_486), (3, 62_379)] {
        assert_eq!(perft(promotion, depth), nodes, "position 5 depth {depth}");
    }
}

#[test]
fn fen_round_trips() {
    for fen in [
        START_FEN,
        "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1",
        "rnbqkbnr/pppp1ppp/8/4p3/4P3/8/PPPP1PPP/RNBQKBNR w KQkq e6 0 2",
        "8/8/8/8/8/8/8/K6k b - - 12 40",
    ] {
        assert_eq!(Position::from_fen(fen).expect("parses").to_fen(), fen);
    }
    for bad in [
        "",
        "8/8/8 w - - 0 1",
        "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR x KQkq - 0 1",
        "9/8/8/8/8/8/8/8 w - - 0 1",
    ] {
        assert!(Position::from_fen(bad).is_none(), "{bad:?}");
    }
}

#[test]
fn moves_read_and_write_as_text() {
    let mv = Move::from_uci("e7e8q").expect("a promotion");
    assert_eq!((mv.from, mv.to, mv.promotion), (52, 60, Some(Kind::Queen)));
    assert_eq!(mv.uci(), "e7e8q");
    assert_eq!(Move::from_uci("a1h8").map(Move::uci), Some("a1h8".to_owned()));
    for bad in ["", "e2", "e2e9", "i2e4", "e7e8k", "e7e8p", "e2e4x1", "e2é4"] {
        assert!(Move::from_uci(bad).is_none(), "{bad:?}");
    }
    assert_eq!(
        (parse_square("a1"), parse_square("h8"), parse_square("e4")),
        (Some(0), Some(63), Some(28))
    );
    assert_eq!(square_name(28), "e4");
}

fn play(game: &mut Game, moves: &str) {
    for word in moves.split_whitespace() {
        let mv = Move::from_uci(word).expect("a move");
        assert!(game.play(mv).is_some(), "{word} should be legal in {:?}", game.position);
    }
}

#[test]
fn fools_mate_is_checkmate() {
    let mut game = Game::new();
    play(&mut game, "f2f3 e7e5 g2g4 d8h4");
    assert_eq!(game.outcome(), Some(Outcome::Checkmate { winner: Color::Black }));
    assert!(game.position.in_check());
    assert_eq!(game.recent_san(8), "1. f3 e5 2. g4 Qh4#");
}

#[test]
fn stalemate_is_a_draw() {
    // Black to move with no legal move and not in check.
    let position = Position::from_fen("7k/5Q2/6K1/8/8/8/8/8 b - - 0 1").expect("fen");
    assert!(position.legal_moves().is_empty() && !position.in_check());
    let game = Game {
        position,
        moves: Vec::new(),
        keys: vec![position.key()],
    };
    assert_eq!(game.outcome(), Some(Outcome::Stalemate));
}

#[test]
fn castling_needs_rights_empty_squares_and_safe_squares() {
    let moves = |fen: &str| -> Vec<String> {
        Position::from_fen(fen)
            .expect("fen")
            .legal_moves()
            .into_iter()
            .filter(|m| m.from == 4 || m.from == 60)
            .filter(|m| m.to.abs_diff(m.from) == 2)
            .map(Move::uci)
            .collect()
    };
    assert_eq!(moves("r3k2r/8/8/8/8/8/8/R3K2R w KQkq - 0 1"), ["e1g1", "e1c1"]);
    assert_eq!(moves("r3k2r/8/8/8/8/8/8/R3K2R b KQkq - 0 1"), ["e8g8", "e8c8"]);
    // Rights gone, or a piece in the way, or the king attacked, or a square on the way attacked.
    assert!(moves("r3k2r/8/8/8/8/8/8/R3K2R w kq - 0 1").is_empty());
    assert!(moves("4k3/8/8/8/8/8/8/R2QK1NR w KQ - 0 1").is_empty());
    assert!(moves("4k3/8/8/8/8/8/4r3/R3K2R w KQ - 0 1").is_empty(), "in check");
    assert_eq!(
        moves("4k3/8/8/8/8/8/5r2/R3K2R w KQ - 0 1"),
        ["e1c1"],
        "f1 is attacked, d1 and c1 are not"
    );
    assert_eq!(
        moves("1r2k3/8/8/8/8/8/8/R3K2R w KQ - 0 1"),
        ["e1g1", "e1c1"],
        "b1 may be attacked"
    );
}

#[test]
fn castling_moves_the_rook_and_loses_the_rights() {
    let mut game = Game::new();
    play(&mut game, "e2e4 e7e5 g1f3 b8c6 f1c4 g8f6 e1g1");
    assert_eq!(game.position.piece_at(6).map(|p| p.kind), Some(Kind::King));
    assert_eq!(game.position.piece_at(5).map(|p| p.kind), Some(Kind::Rook));
    assert!(game.position.piece_at(7).is_none());
    assert!(game.recent_san(2).ends_with("4. O-O"), "{}", game.recent_san(2));
    assert!(game.position.to_fen().contains(" kq "), "{:?}", game.position);
}

#[test]
fn en_passant_must_be_taken_at_once_or_not_at_all() {
    let mut game = Game::new();
    play(&mut game, "e2e4 a7a6 e4e5 d7d5");
    assert_eq!(game.position.en_passant().map(square_name).as_deref(), Some("d6"));
    let mut after = game.clone();
    play(&mut after, "e5d6");
    assert!(
        after.position.piece_at(parse_square("d5").unwrap()).is_none(),
        "the pawn that passed is taken"
    );
    assert!(after.recent_san(2).ends_with("3. exd6"), "{}", after.recent_san(2));
    // One move later the chance is gone.
    play(&mut game, "h2h3 h7h6");
    assert!(game.play(Move::from_uci("e5d6").unwrap()).is_none());
}

#[test]
fn promotion_offers_all_four_pieces_and_makes_the_choice() {
    let position = Position::from_fen("8/P7/8/8/8/8/8/k1K5 w - - 0 1").expect("fen");
    let moves: Vec<String> = position
        .legal_moves()
        .into_iter()
        .filter(|m| m.from == 48)
        .map(Move::uci)
        .collect();
    assert_eq!(moves, ["a7a8q", "a7a8r", "a7a8b", "a7a8n"]);
    let after = position.make(Move::from_uci("a7a8n").unwrap());
    assert_eq!(
        after.piece_at(56),
        Some(Piece {
            color: Color::White,
            kind: Kind::Knight
        })
    );
    assert_eq!(position.san(Move::from_uci("a7a8q").unwrap()), "a8=Q#");
}

#[test]
fn a_pinned_piece_cannot_leave_the_line() {
    let position = Position::from_fen("4r1k1/8/8/8/8/8/4N3/4K3 w - - 0 1").expect("fen");
    assert!(
        position.legal_moves().iter().all(|m| m.from != 12),
        "the knight is pinned to the king"
    );
    assert!(!position.in_check());
}

#[test]
fn san_disambiguates_and_marks_checks() {
    // The king is in the way, so only one rook can reach each square.
    let position = Position::from_fen("4k3/8/8/8/8/8/8/R3K2R w - - 0 1").expect("fen");
    assert_eq!(position.san(Move::from_uci("a1d1").unwrap()), "Rd1");
    assert_eq!(position.san(Move::from_uci("h1f1").unwrap()), "Rf1");
    let both = Position::from_fen("4k3/8/8/8/8/8/4K3/R6R w - - 0 1").expect("fen");
    assert_eq!(both.san(Move::from_uci("a1d1").unwrap()), "Rad1");
    assert_eq!(both.san(Move::from_uci("h1d1").unwrap()), "Rhd1");
    let check = Position::from_fen("4k3/8/8/8/8/8/8/R3K3 w - - 0 1").expect("fen");
    assert_eq!(check.san(Move::from_uci("a1a8").unwrap()), "Ra8+");
    let knights = Position::from_fen("4k3/8/8/8/8/2N3N1/8/4K3 w - - 0 1").expect("fen");
    assert_eq!(knights.san(Move::from_uci("c3e4").unwrap()), "Nce4");
    assert_eq!(knights.san(Move::from_uci("g3e4").unwrap()), "Nge4");
    assert_eq!(Position::start().san(Move::from_uci("g1f3").unwrap()), "Nf3");
}

#[test]
fn pinned_en_passant_does_not_prevent_threefold_repetition() {
    let position = Position::from_fen("1n2k3/8/8/r4pPK/8/8/8/6N1 w - f6 0 1").unwrap();
    assert!(!position.legal_moves().contains(&Move::from_uci("g5f6").unwrap()));
    let without_ep = Position::from_fen("1n2k3/8/8/r4pPK/8/8/8/6N1 w - - 0 1").unwrap();
    assert_eq!(position.key(), without_ep.key());
    let mut game = Game {
        position,
        moves: vec![],
        keys: vec![position.key()],
    };
    play(&mut game, "g1f3 b8c6 f3g1 c6b8 g1f3 b8c6 f3g1 c6b8");
    assert_eq!(game.outcome(), Some(Outcome::Repetition));
    let available = Position::from_fen("4k3/8/8/5pP1/8/8/8/4K3 w - f6 0 1").unwrap();
    let unavailable = Position::from_fen("4k3/8/8/5pP1/8/8/8/4K3 w - - 0 1").unwrap();
    assert_ne!(available.key(), unavailable.key());
}

#[test]
fn threefold_repetition_ends_the_game() {
    let mut game = Game::new();
    play(&mut game, "g1f3 g8f6 f3g1 f6g8");
    assert_eq!(game.outcome(), None, "the start position came up twice");
    play(&mut game, "g1f3 g8f6 f3g1 f6g8");
    assert_eq!(game.outcome(), Some(Outcome::Repetition));
}

#[test]
fn the_fifty_move_rule_counts_plies_without_pawns_or_captures() {
    let mut position = Position::from_fen("4k3/8/8/8/8/8/R7/4K3 w - - 99 80").expect("fen");
    let game = |p: Position| Game {
        position: p,
        moves: Vec::new(),
        keys: vec![p.key()],
    };
    assert_eq!(game(position).outcome(), None);
    position = position.make(Move::from_uci("a2a3").unwrap());
    assert_eq!(position.halfmove, 100);
    assert_eq!(game(position).outcome(), Some(Outcome::FiftyMoves));
    // A pawn move or a capture starts the count again.
    let reset = Position::from_fen("4k3/8/8/8/8/8/P7/4K3 w - - 99 80")
        .expect("fen")
        .make(Move::from_uci("a2a3").unwrap());
    assert_eq!(reset.halfmove, 0);
}

#[test]
fn mate_wins_over_the_fifty_move_rule() {
    let position = Position::from_fen("7k/6pp/8/8/8/8/8/R3K3 w - - 99 80")
        .expect("fen")
        .make(Move::from_uci("a1a8").unwrap());
    let game = Game {
        position,
        moves: Vec::new(),
        keys: vec![position.key()],
    };
    assert_eq!(game.outcome(), Some(Outcome::Checkmate { winner: Color::White }));
}

#[test]
fn insufficient_material_is_spotted() {
    let draw = |fen: &str| Position::from_fen(fen).expect("fen").insufficient_material();
    assert!(draw("4k3/8/8/8/8/8/8/4K3 w - - 0 1"));
    assert!(draw("4k3/8/8/8/8/8/8/3NK3 w - - 0 1"));
    assert!(draw("4k3/8/8/8/8/8/8/3BK3 w - - 0 1"));
    assert!(draw("2b1k3/8/8/8/8/8/8/3BK3 w - - 0 1"), "bishops on the same colour");
    assert!(
        !draw("3bk3/8/8/8/8/8/8/3BK3 w - - 0 1"),
        "bishops on different colours can mate"
    );
    assert!(!draw("4k3/8/8/8/8/8/8/2NNK3 w - - 0 1"));
    assert!(!draw("4k3/8/8/8/8/8/P7/4K3 w - - 0 1"));
    assert!(!draw("4k3/8/8/8/8/8/8/3RK3 w - - 0 1"));
    let game = Game {
        position: Position::from_fen("4k3/8/8/8/8/8/8/3NK3 w - - 0 1").unwrap(),
        moves: Vec::new(),
        keys: vec![0],
    };
    assert_eq!(game.outcome(), Some(Outcome::Insufficient));
}

#[test]
fn the_shortest_known_stalemate_is_recognised() {
    // Sam Loyd's ten-move stalemate: black has a queen, two rooks and a bishop and no move.
    let moves = "e2e3 a7a5 d1h5 a8a6 h5a5 h7h5 h2h4 a6h6 a5c7 f7f6 c7d7 e8f7 d7b7 d8d3 b7b8 d3h7 b8c8 f7g6 c8e6";
    let game = Game::from_moves(moves).expect("every move is legal");
    assert_eq!(game.plies(), 19);
    assert!(!game.position.in_check());
    assert_eq!(game.outcome(), Some(Outcome::Stalemate));
    let before = Game::from_moves(&moves[..moves.len() - 5]).expect("one move less");
    assert_eq!(before.outcome(), None);
}

#[test]
fn games_are_stored_as_their_moves() {
    let mut game = Game::new();
    play(&mut game, "e2e4 e7e5 g1f3 b8c6");
    let text = game.moves_text();
    assert_eq!(text, "e2e4 e7e5 g1f3 b8c6");
    let again = Game::from_moves(&text).expect("replays");
    assert_eq!(again.position, game.position);
    assert_eq!(again.plies(), 4);
    assert!(Game::from_moves("e2e5").is_none(), "an illegal move is refused");
    assert!(Game::from_moves("e2e4 e2e4").is_none());
    assert_eq!(Game::from_moves("").map(|g| g.plies()), Some(0));
}

#[test]
fn lost_pieces_are_counted() {
    let position = Position::from_fen("rnbqkbnr/ppp2ppp/8/8/8/8/PPPP1PPP/RNB1KBNR w KQkq - 0 1").expect("fen");
    assert_eq!(position.missing(Color::White), [1, 0, 0, 0, 1]);
    assert_eq!(position.missing(Color::Black), [2, 0, 0, 0, 0]);
}

#[test]
fn pieces_and_endings_are_named_in_every_language() {
    assert_eq!(Kind::Knight.name(Lang::En), "Knight");
    assert_eq!(Kind::Knight.name(Lang::Tr), "At");
    let mate = Outcome::Checkmate { winner: Color::White };
    assert_eq!(mate.describe(Lang::En), "checkmate");
    assert_eq!(mate.describe(Lang::Tr), "şah mat");
    for outcome in [
        Outcome::Stalemate,
        Outcome::FiftyMoves,
        Outcome::Repetition,
        Outcome::Insufficient,
    ] {
        assert_ne!(outcome.describe(Lang::En), outcome.describe(Lang::Tr), "{outcome:?}");
    }
}
