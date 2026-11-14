    use crate::commands::games::collection::drops::*;
    fn sample() -> DropRow {
        let now = chrono::Utc::now();
        DropRow {
            id: 1,
            kind: DROP_CRITTERS.into(),
            guild_id: 2,
            channel_id: 3,
            message_id: None,
            host_id: None,
            cards: Collection::get()
                .critters
                .iter()
                .take(3)
                .map(|e| e.id.clone())
                .collect(),
            takers: vec![0; 3],
            status: "open".into(),
            expires_at: now + Span::seconds(90),
            mode: "word".into(),
            challenge_word: Some("a91bc42f".into()),
            vote_winner: None,
            created_at: now,
            protected_user_id: Some(4),
            protected_slot: Some(0),
            protected_until: Some(now + Span::seconds(15)),
        }
    }
    #[test]
    fn word_answers_and_hidden_cards_never_leak_in_discord_metadata() {
        let mut row = sample();
        for lang in [Lang::En, Lang::Tr] {
            let payload =
                serde_json::to_string(&(critter_embed(&row, false, lang), critter_buttons(&row, false))).unwrap();
            assert!(!payload.contains("a91bc42f"));
        }
        row.mode = "hidden".into();
        for lang in [Lang::En, Lang::Tr] {
            let payload =
                serde_json::to_string(&(critter_embed(&row, false, lang), critter_buttons(&row, false))).unwrap();
            for entry in cards(&row) {
                assert!(!payload.contains(&entry.name_in(lang)));
                assert!(!payload.contains(&entry.id));
            }
        }
        let allowed = ping_wish(&row).unwrap();
        assert_eq!(allowed.users.len(), 1);
        assert!(allowed.parse.is_empty() && allowed.roles.is_empty());
    }
