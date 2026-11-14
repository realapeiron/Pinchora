use crate::commands::games::duel::*;

#[test]
fn detects_wins_and_draws() {
    let g = |b: &[u8; 9]| Game {
        x: 1,
        o: 2,
        board: *b,
        x_turn: true,
    };
    assert_eq!(g(b"xxx.o.o..").winner().map(|w| w.0), Some(b'x'));
    assert_eq!(g(b"o.xo.xo..").winner().map(|w| w.0), Some(b'o'));
    let draw = g(b"xoxxoooxx");
    assert!(draw.winner().is_none() && draw.full());
}

#[test]
fn the_wager_limits_are_told_in_both_languages() {
    assert_eq!(
        t!(Lang::En, "duel.bet-range", min = MIN_BET, max = MAX_BET),
        "A wager is 10 to 500 shells."
    );
    assert_eq!(
        t!(Lang::Tr, "duel.bet-range", min = MIN_BET, max = MAX_BET),
        "Bahis 10 ile 500 kabuk arasında olmalı."
    );
}
