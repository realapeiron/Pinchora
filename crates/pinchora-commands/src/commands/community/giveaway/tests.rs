    use crate::commands::community::giveaway::*;

    fn giveaway(running: bool) -> GiveawayRow {
        GiveawayRow {
            id: 12,
            guild_id: 1,
            channel_id: 2,
            message_id: Some(3),
            host_id: 4,
            prize: "Golden shell".to_owned(),
            shells: 50,
            winners: 2,
            required_role_id: Some(5),
            ends_at: Utc::now(),
            starts_at: None,
            min_level: 0,
            min_account_days: 0,
            min_server_days: 0,
            bonus_role_id: None,
            bonus_entries: 0,
            ended_at: (!running).then(Utc::now),
            status: if running { "running" } else { "ended" }.to_owned(),
            winner_ids: if running { Vec::new() } else { vec![7, 8] },
            announced: false,
            entries: 9,
        }
    }

    #[test]
    fn button_ids_round_trip() {
        let id = button_id(12);
        let mut parts = id.split(':');
        assert_eq!(parts.next(), Some("gw"));
        assert_eq!(parse_button(parts), Some(12));
        assert!(id.len() <= 100);
    }

    #[test]
    fn bad_button_ids_are_refused() {
        assert_eq!(parse_button("".split(':')), None);
        assert_eq!(parse_button("e".split(':')), None);
        assert_eq!(parse_button("x:1".split(':')), None);
        assert_eq!(parse_button("e:one".split(':')), None);
        assert_eq!(parse_button("e:1:2".split(':')), None);
    }

    #[test]
    fn a_running_giveaway_shows_its_terms_and_entry_count() {
        let embed = giveaway_embed(Lang::En, &giveaway(true));
        let text = embed.description.expect("a description");
        for part in [
            "Hosted by <@4>",
            "Winners: **2**",
            "**50** shells each",
            "Requires <@&5>",
        ] {
            assert!(text.contains(part), "{part} missing from {text}");
        }
        assert_eq!(embed.fields[0].name, "Entries");
        assert_eq!(embed.fields[0].value, "9");
        let Component::ActionRow(buttons) = &giveaway_components(Lang::En, &giveaway(true))[0] else {
            panic!("no row");
        };
        let Component::Button(enter) = &buttons.components[0] else {
            panic!("no button");
        };
        assert!(!enter.disabled);
        assert_eq!(enter.custom_id.as_deref(), Some("gw:e:12"));
    }

    #[test]
    fn an_ended_giveaway_names_its_winners_and_locks_the_button() {
        let ended = giveaway(false);
        let embed = giveaway_embed(Lang::En, &ended);
        assert_eq!(embed.fields[0].value, "<@7>, <@8>");
        let Component::ActionRow(buttons) = &giveaway_components(Lang::En, &ended)[0] else {
            panic!("no row");
        };
        let Component::Button(enter) = &buttons.components[0] else {
            panic!("no button");
        };
        assert!(enter.disabled);
        let nobody = GiveawayRow {
            winner_ids: Vec::new(),
            ..ended
        };
        assert_eq!(giveaway_embed(Lang::En, &nobody).fields[0].value, "Nobody entered");
    }
