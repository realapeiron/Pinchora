use crate::admin::settings::*;

/// Every error with a catalog key says in English what its message says, and has Turkish too.
#[test]
fn keyed_errors_read_the_same_in_the_english_catalog() {
    let long = "x".repeat(MESSAGE_MAX + 1);
    let errors = [
        check_accent("zzz").unwrap_err(),
        check_message(MessageKind::Welcome, " ").unwrap_err(),
        check_message(MessageKind::Goodbye, &long).unwrap_err(),
        check_welcome_title(" ").unwrap_err(),
        check_welcome_title(&"y".repeat(WELCOME_TITLE_MAX + 1)).unwrap_err(),
        check_theme("neon").unwrap_err(),
        parse_levelup_mode("somewhere").unwrap_err(),
        check_points_name("").unwrap_err(),
        check_points_emoji("abc").unwrap_err(),
        check_points(-5, 5).unwrap_err(),
        check_points(9, 3).unwrap_err(),
        check_multiplier(9.0).unwrap_err(),
        check_voice_amount(0).unwrap_err(),
        check_level(0).unwrap_err(),
        check_reputation(0).unwrap_err(),
        check_price(0).unwrap_err(),
        check_stock(-1).unwrap_err(),
        crate::admin::modules::check_channels(
            crate::admin::modules::Module::Welcome,
            crate::admin::modules::ChannelMode::Only,
            &[1],
        )
        .unwrap_err(),
        crate::admin::modules::check_channels(
            crate::admin::modules::Module::Pets,
            crate::admin::modules::ChannelMode::Only,
            &[0],
        )
        .unwrap_err(),
        crate::admin::modules::check_channels(
            crate::admin::modules::Module::Pets,
            crate::admin::modules::ChannelMode::Only,
            &[],
        )
        .unwrap_err(),
    ];
    for error in errors {
        assert_eq!(
            error.catalog_text().as_deref(),
            Some(error.message.as_str()),
            "{error:?}"
        );
        let turkish = error.text(Lang::Tr);
        assert_ne!(turkish, error.message, "{error:?} has no Turkish yet");
        assert!(!turkish.contains("settings-error"), "{turkish}");
    }
    assert_eq!(
        FieldError::new("x", "Plain").text(Lang::Tr),
        "Plain",
        "no key: the English message"
    );
}

#[test]
fn role_refusals_have_twin_words_in_turkish() {
    let managed = RoleFacts {
        everyone: false,
        managed: true,
        position: 1,
        permissions: 0,
    };
    assert_eq!(
        role_refusal_in(Lang::En, managed, Some(5), "<@&1>"),
        role_refusal(managed, Some(5), "<@&1>")
    );
    assert!(
        role_refusal_in(Lang::Tr, managed, Some(5), "<@&1>")
            .unwrap()
            .contains("yönetiliyor")
    );
    let admin = (1u64 << 3) | (1 << 5);
    let powers = dangerous_permissions_in(Lang::Tr, admin);
    assert_eq!(powers, ["Yönetici", "Sunucuyu Yönet"]);
    let refusal = run_time_refusal_in(Lang::Tr, admin, "<@&2>").unwrap();
    assert!(refusal.contains("Yönetici ve Sunucuyu Yönet"), "{refusal}");
}

#[test]
fn colours_need_six_hex_digits() {
    assert_eq!(parse_hex_color("#ff6b4a"), Some(0xff6b4a));
    assert_eq!(parse_hex_color(" 06D6A0 "), Some(0x06d6a0));
    for bad in ["", "#fff", "#ff6b4a0", "+ff6b4", "#gg0000", "##ff6b4a"] {
        assert_eq!(parse_hex_color(bad), None, "{bad:?}");
    }
    assert_eq!(check_accent("nope").unwrap_err().field, "welcome_accent");
}

#[test]
fn messages_are_bounded_and_not_blank() {
    assert!(check_message(MessageKind::Welcome, "Hi {user}").is_ok());
    assert!(check_message(MessageKind::Welcome, &"é".repeat(MESSAGE_MAX)).is_ok());
    let long = check_message(MessageKind::Goodbye, &"x".repeat(MESSAGE_MAX + 1)).unwrap_err();
    assert_eq!(long.field, "goodbye_message");
    assert!(long.message.contains("501"), "{}", long.message);
    assert_eq!(
        check_message(MessageKind::LevelUp, "  ").unwrap_err().field,
        "levelup_message"
    );
}

#[test]
fn titles_names_and_emoji_are_trimmed_and_bounded() {
    assert_eq!(check_welcome_title("  HI THERE ").unwrap(), "HI THERE");
    assert!(check_welcome_title(&"W".repeat(WELCOME_TITLE_MAX + 1)).is_err());
    assert!(check_welcome_title(" ").is_err());
    assert_eq!(check_points_name(" coins ").unwrap(), "coins");
    assert!(check_points_name("").is_err());
    for good in [
        "⭐",
        " 🪙 ",
        "🇹🇷",
        "👨‍👩‍👧",
        "<:coin:123456789012345678>",
        "<a:spin:123456789012345678>",
    ] {
        assert!(check_points_emoji(good).is_ok(), "{good:?}");
    }
    for bad in ["", "coins", "⭐⭐", "<:c:1>", "<coin:123456789012345678>"] {
        assert!(check_points_emoji(bad).is_err(), "{bad:?}");
    }
}

#[test]
fn prefixes_are_short_and_cannot_ping() {
    assert_eq!(check_prefix(" p! ").unwrap(), "p!");
    for good in ["?", "z!", "pin.", "!!!!!", "ğ$"] {
        assert!(check_prefix(good).is_ok(), "{good:?}");
    }
    for bad in ["", "  ", "toolong", "p !", "@", "<@1>", "#", "`p", "p\tq"] {
        assert_eq!(check_prefix(bad).unwrap_err().field, "prefix", "{bad:?}");
    }
}

#[test]
fn points_ranges_and_order() {
    assert!(check_points(0, 1_000).is_ok());
    assert_eq!(check_points(5, 4).unwrap_err().field, "points_min");
    assert_eq!(check_points(0, 1_001).unwrap_err().field, "points_max");
    assert_eq!(check_points(-1, 3).unwrap_err().field, "points_min");
}

#[test]
fn multipliers_round_and_stay_in_range() {
    assert_eq!(check_multiplier(1.234).unwrap(), 1.23);
    assert_eq!(check_multiplier(0.0).unwrap(), 0.0);
    assert_eq!(check_multiplier(5.004).unwrap(), 5.0);
    for bad in [5.01, -0.01, f64::NAN, f64::INFINITY] {
        assert!(check_multiplier(bad).is_err(), "{bad}");
    }
    assert_eq!(
        check_multiplier(9.0).unwrap_err().message,
        "A multiplier goes from 0 to 5."
    );
}

#[test]
fn counts_have_ranges() {
    assert_eq!(check_voice_amount(10).unwrap(), 10);
    assert!(check_voice_amount(0).is_err() && check_voice_amount(101).is_err());
    assert!(check_level(1).is_ok() && check_level(1_001).is_err());
    assert!(check_reputation(100_000).is_ok() && check_reputation(0).is_err());
    assert!(check_price(1).is_ok() && check_price(0).is_err());
    assert!(check_stock(1_000_000).is_ok() && check_stock(0).is_err());
}

#[test]
fn themes_and_modes_are_known_keys() {
    assert!(check_theme("midnight").is_ok() && check_theme("neon").is_err());
    assert_eq!(parse_levelup_mode("dm").unwrap(), LevelUpMode::Dm);
    assert!(parse_levelup_mode("everywhere").is_err());
    for mode in LEVELUP_MODES {
        assert_eq!(LevelUpMode::parse(mode.key()), mode);
    }
    for kind in MessageKind::ALL {
        assert_eq!(MessageKind::parse(kind.key()), Some(kind));
        assert!(kind.placeholders().contains(&"{user}"));
    }
}

#[test]
fn dangerous_permissions_are_named() {
    assert!(dangerous_permissions(0).is_empty());
    assert!(dangerous_permissions((1 << 10) | (1 << 11)).is_empty(), "view and send");
    assert_eq!(dangerous_permissions(1 << 3), ["Administrator"]);
    assert_eq!(
        dangerous_permissions((1 << 40) | (1 << 1)),
        ["Kick Members", "Timeout Members"]
    );
    let all = DANGEROUS_PERMISSIONS.iter().fold(0, |bits, (bit, _)| bits | bit);
    assert_eq!(dangerous_permissions(all).len(), DANGEROUS_PERMISSIONS.len());
}

#[test]
fn who_may_set_a_role_to_be_handed_out() {
    const ADMIN: u64 = 1 << 3;
    const MANAGE_MESSAGES: u64 = 1 << 13;
    const BAN: u64 = 1 << 2;
    let role = |position, permissions| RoleFacts {
        everyone: false,
        managed: false,
        position,
        permissions,
    };
    let owner = Grantor {
        owner: true,
        administrator: true,
        top_role: Some(0),
    };
    let admin = |top| Grantor {
        owner: false,
        administrator: true,
        top_role: top,
    };
    let manager = |top| Grantor {
        owner: false,
        administrator: false,
        top_role: top,
    };
    // (role, grantor, configured roles allowed, custom actions allowed)
    let cases = [
        (role(3, 0), manager(Some(5)), true, true),
        (role(5, 0), manager(Some(5)), false, false),
        (role(6, 0), manager(Some(5)), false, false),
        (role(3, 0), manager(None), false, false),
        (role(3, ADMIN), manager(Some(5)), false, false),
        (role(3, MANAGE_MESSAGES), manager(Some(5)), false, false),
        (role(3, BAN | ADMIN), admin(Some(5)), true, false),
        (role(6, ADMIN), admin(Some(5)), false, false),
        (role(6, 0), admin(Some(5)), false, false),
        (role(30, ADMIN), owner, true, false),
        (role(30, 0), owner, true, true),
        (
            role(30, 0),
            Grantor {
                top_role: None,
                ..owner
            },
            true,
            true,
        ),
    ];
    for (n, (role, grantor, configured, action)) in cases.into_iter().enumerate() {
        assert_eq!(
            grant_refusal(role, grantor, "@R").is_none(),
            configured,
            "case {n}: {role:?} {grantor:?}"
        );
        assert_eq!(
            action_role_refusal(role, grantor, "@R").is_none(),
            action,
            "case {n} as an action: {role:?} {grantor:?}"
        );
    }
    let why = grant_refusal(role(3, ADMIN | BAN | MANAGE_MESSAGES), manager(Some(5)), "@Mod").unwrap();
    assert_eq!(
        why,
        "@Mod has Administrator and 2 more, so only the server owner or an administrator can set it to be handed out."
    );
    assert!(
        grant_refusal(role(5, 0), manager(Some(5)), "@Top")
            .unwrap()
            .contains("not below your highest role")
    );
    assert_eq!(run_time_refusal(0, "@Fan"), None);
    assert_eq!(
        run_time_refusal(ADMIN | BAN, "@Fan").unwrap(),
        "@Fan has Administrator and Ban Members, so custom commands and auto-responders can't hand it out."
    );
}

#[test]
fn role_refusals_explain_themselves() {
    let role = |everyone, managed, position| RoleFacts {
        everyone,
        managed,
        position,
        permissions: 0,
    };
    assert_eq!(role_refusal(role(false, false, 3), Some(5), "@Fan"), None);
    assert!(
        role_refusal(role(true, false, 0), Some(5), "@everyone")
            .unwrap()
            .contains("every member")
    );
    assert!(
        role_refusal(role(false, true, 1), Some(5), "@Bot")
            .unwrap()
            .starts_with("@Bot is managed")
    );
    assert!(
        role_refusal(role(false, false, 5), Some(5), "@Top")
            .unwrap()
            .contains("above my highest role")
    );
    assert!(
        role_refusal(role(false, false, 1), None, "@Fan")
            .unwrap()
            .contains("couldn't check")
    );
}

#[test]
fn a_patch_is_cleaned_or_every_problem_is_listed() {
    let patch = GuildSettingsPatch {
        welcome_title: Some(Some("  HELLO ".into())),
        points_name: Some(" shells ".into()),
        points_max: Some(5),
        ..Default::default()
    };
    let clean = check_patch(patch, (1, 3)).unwrap();
    assert_eq!(clean.welcome_title, Some(Some("HELLO".into())));
    let prefix = |p: &str| GuildSettingsPatch {
        prefix: Some(Some(p.into())),
        ..Default::default()
    };
    assert_eq!(
        check_patch(prefix(" z! "), (1, 3)).unwrap().prefix,
        Some(Some("z!".into()))
    );
    assert_eq!(check_patch(prefix("p!"), (1, 3)).unwrap().prefix, Some(None));
    assert_eq!(check_patch(prefix("@"), (1, 3)).unwrap_err()[0].field, "prefix");
    assert_eq!(clean.points_name.as_deref(), Some("shells"));

    let bad = GuildSettingsPatch {
        welcome_message: Some(Some(" ".into())),
        welcome_theme: Some(Some("neon".into())),
        points_max: Some(2),
        voice_xp_amount: Some(0),
        ..Default::default()
    };
    let errors = check_patch(bad, (3, 5)).unwrap_err();
    let fields: Vec<_> = errors.iter().map(|e| e.field).collect();
    assert_eq!(
        fields,
        ["welcome_message", "voice_xp_amount", "welcome_theme", "points_min"]
    );
    // Clearing is always allowed.
    let clear = GuildSettingsPatch {
        welcome_message: Some(None),
        welcome_title: Some(None),
        ..Default::default()
    };
    assert!(check_patch(clear, (1, 3)).is_ok());
}
