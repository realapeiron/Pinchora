use crate::community::starboard::*;

fn board() -> Starboard {
    Starboard {
        id: 1,
        guild_id: 1,
        name: "Highlights".into(),
        channel_id: 2,
        emoji: "⭐".into(),
        threshold: 3,
        self_star: false,
        allow_bots: false,
        only_channels: vec![3],
        except_channels: vec![4],
        require_image: false,
        color: 0xffc857,
        role_weights: serde_json::json!({"5": 2, "6": 4}),
        remove_below: true,
        enabled: true,
    }
}

#[test]
fn emoji_and_channel_rules_match_exactly() {
    let board = board();
    assert!(emoji_matches(
        &board.emoji,
        &EmojiReactionType::Unicode { name: "⭐".into() }
    ));
    assert!(!emoji_matches(
        &board.emoji,
        &EmojiReactionType::Unicode { name: "🌟".into() }
    ));
    assert!(eligible(&board, 3));
    assert!(!eligible(&board, 4));
    assert!(!eligible(&board, 2));
}

#[test]
fn nsfw_sources_only_enter_nsfw_boards() {
    assert!(!nsfw_compatible(Some(true), None, Some(false)));
    assert!(nsfw_compatible(Some(true), None, Some(true)));
    assert!(!nsfw_compatible(None, Some(true), Some(false)));
    assert!(!nsfw_compatible(Some(false), Some(true), Some(false)));
    assert!(!nsfw_compatible(None, None, Some(false)));
}

#[test]
fn highest_held_role_weight_wins() {
    let board = board();
    assert_eq!(weight(&board, &[]), 1);
    assert_eq!(weight(&board, &[Id::new(5)]), 2);
    assert_eq!(weight(&board, &[Id::new(5), Id::new(6)]), 4);
}
