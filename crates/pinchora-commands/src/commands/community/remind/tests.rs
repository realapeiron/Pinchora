    use crate::commands::community::remind::*;

    #[test]
    fn durations_are_read_like_people_write_them() {
        let m = chrono::Duration::minutes;
        let h = chrono::Duration::hours;
        assert_eq!(parse_duration("45m"), Ok(m(45)));
        assert_eq!(parse_duration("2h30m"), Ok(h(2) + m(30)));
        assert_eq!(parse_duration(" 1d 12H "), Ok(h(36)));
        assert_eq!(parse_duration("90 minutes"), Ok(m(90)));
        assert_eq!(parse_duration("1w"), Ok(chrono::Duration::days(7)));
        assert_eq!(parse_duration("1h,30min"), Ok(m(90)));
        assert_eq!(parse_duration("60s"), Ok(m(1)));
    }

    #[test]
    fn turkish_units_are_read_too() {
        let m = chrono::Duration::minutes;
        let h = chrono::Duration::hours;
        assert_eq!(parse_duration("45dk"), Ok(m(45)));
        assert_eq!(parse_duration("2sa30dk"), Ok(h(2) + m(30)));
        assert_eq!(parse_duration("2 saat 30 dakika"), Ok(h(2) + m(30)));
        assert_eq!(parse_duration(" 1G 12SA "), Ok(h(36)));
        assert_eq!(parse_duration("3 gün"), Ok(chrono::Duration::days(3)));
        assert_eq!(parse_duration("3 gun"), Ok(chrono::Duration::days(3)));
        assert_eq!(parse_duration("1hf"), Ok(chrono::Duration::days(7)));
        assert_eq!(parse_duration("2 HAFTA"), Ok(chrono::Duration::days(14)));
        assert_eq!(parse_duration("60sn"), Ok(m(1)));
        assert_eq!(parse_duration("90 saniye"), Ok(m(1) + chrono::Duration::seconds(30)));
        assert_eq!(parse_duration("1d 30dk"), Ok(h(24) + m(30)));
        for bad in ["yakında", "5ş", "5gü", "dk", "2sa30"] {
            assert_eq!(parse_duration(bad), Err("remind.bad-time"), "{bad:?}");
        }
        assert!(t!(Lang::Tr, parse_duration("yakında").unwrap_err()).contains("hf, g, sa, dk, sn"));
    }

    #[test]
    fn durations_that_make_no_sense_are_refused() {
        for bad in ["", "soon", "30", "m5", "5x", "h", "2h30", "1.5h", "-5m"] {
            assert!(parse_duration(bad).is_err(), "{bad:?}");
        }
        assert!(t!(Lang::En, parse_duration("30s").unwrap_err()).contains("too soon"));
        assert!(parse_duration("0m").is_err());
        assert!(t!(Lang::En, parse_duration("366d").unwrap_err()).contains("a year"));
        assert!(parse_duration("99999999999999999999h").is_err());
        assert!(parse_duration("9999999999999w").is_err());
        assert!(parse_duration("365d").is_ok());
    }

    #[test]
    fn mentions_are_stripped_and_the_text_tidied() {
        assert_eq!(strip_mentions("call <@123> and <@!456>"), "call and");
        assert_eq!(strip_mentions("<@&789> meeting in <#42>"), "meeting in");
        assert_eq!(strip_mentions("ping @everyone and @HERE now"), "ping and now");
        assert_eq!(strip_mentions("sneaky @ever\u{200b}yone"), "sneaky");
        assert_eq!(strip_mentions("a < b and b > c"), "a < b and b > c");
        assert_eq!(strip_mentions("<@abc> stays"), "<@abc> stays");
        assert_eq!(strip_mentions("  two   spaces\nand a line "), "two spaces and a line");
        assert_eq!(strip_mentions("<@1>"), "");
        assert_eq!(strip_mentions("plain 🐚 text <t:1234:R>"), "plain 🐚 text <t:1234:R>");
        assert_eq!(strip_mentions(&"x".repeat(300)).chars().count(), MAX_TEXT);
    }

    #[test]
    fn a_pet_is_not_told_about_at_once() {
        let now = Utc::now();
        let pet = |fun: i16| PetRow {
            id: 1,
            owner_id: 1,
            species: "crab".into(),
            name: "Pinchy".into(),
            level: 1,
            xp: 0,
            hunger: 100,
            fun,
            energy: 100,
            hygiene: 100,
            needs_at: now,
            active: true,
            trip_dest: None,
            trip_ends_at: None,
        };
        // Fun 80 loses 5 an hour and is low (30) after 10 h.
        assert_eq!(pet_due(&pet(80)), now + chrono::Duration::hours(10));
        // Already low: the grace period after the last care.
        assert_eq!(pet_due(&pet(5)), now + chrono::Duration::hours(2));
    }
