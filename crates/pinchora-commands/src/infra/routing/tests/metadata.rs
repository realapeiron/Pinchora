use super::*;

#[test]
fn migrated_split_parents_preserve_conjunctive_roles_channels_and_cooldown_identity() {
    use pinchora_core::admin::command_control::Config;
    use pinchora_core::admin::command_control::Rule;
    let config = Config {
        rules: [
            (
                "roles".into(),
                Rule {
                    allowed_roles: vec!["1".into(), "2".into()],
                    cooldown_secs: 12,
                    ..Rule::default()
                },
            ),
            (
                "roles post".into(),
                Rule {
                    allowed_roles: vec!["3".into()],
                    ..Rule::default()
                },
            ),
            (
                "setup roles panel-post".into(),
                Rule {
                    denied_channels: vec!["9".into()],
                    ..Rule::default()
                },
            ),
            (
                "automod".into(),
                Rule {
                    enabled: false,
                    ..Rule::default()
                },
            ),
        ]
        .into(),
        ..Config::default()
    };
    let migrated = normalize_control(&config);
    assert_eq!(normalize_control(&migrated), migrated);
    for roles in [vec![], vec!["1".into()], vec!["2".into(), "3".into()]] {
        for channels in [vec![], vec!["9".into()]] {
            let check = |config: &Config| {
                matching(config, "setup roles panel-post")
                    .iter()
                    .all(|(_, r)| r.check(&roles, &channels).is_ok())
            };
            assert_eq!(check(&config), check(&migrated));
        }
    }
    assert!(
        matching(&migrated, "setup roles panel-add")
            .iter()
            .any(|(source, r)| source.as_str() == "roles" && r.cooldown_secs == 12)
    );
    assert!(
        matching(&migrated, "automod").iter().any(|(_, r)| !r.enabled),
        "old family component owners keep parent denies"
    );
    assert!(
        !matching(&migrated, "setup roles color-setup")
            .iter()
            .any(|(_, r)| r.cooldown_secs == 12)
    );
    migrated
        .validate(&crate::control_catalog(), &crate::command_reserved_aliases(), Lang::En)
        .unwrap();
}

#[test]
fn quick_games_retain_paths_arguments_and_localizations() {
    let old = crate::commands::legacy_definitions();
    let new = crate::command_definitions();
    for path in [
        "play memory",
        "play minesweeper",
        "play higherlower",
        "play anagram",
        "play typing",
        "play guessnumber",
        "duel othello",
        "party nhie",
        "party fun",
    ] {
        let route = route(path).expect("quick game in route manifest");
        assert_eq!(route.legacy, path);
        assert_eq!(route.canonical, path);
        assert_eq!(source(&old, path), source(&new, path), "{path}");
        assert_eq!(resolve_text(path).0, path);
        assert!(route.metadata().help_area.is_some(), "{path}");
        assert!(route.metadata().module.is_some(), "{path}");
    }
}

#[test]
fn registered_command_sizes_keep_growth_headroom() {
    let tree = crate::command_definitions();
    // Collection depth lives in these existing roots; do not silently skip its budget check.
    for name in ["dex", "critterdrop", "beachcomb", "crabpot"] {
        let root = tree.iter().find(|c| c.name == name).expect("collection root exists");
        let size = crate::infra::routing::command_size(root);
        eprintln!("/{name}: {size} characters (collection)");
        assert!(size <= 6_650, "collection root /{name}: {size}");
    }
    let mut sizes: Vec<_> = tree
        .iter()
        .map(|c| (crate::infra::routing::command_size(c), &c.name))
        .collect();
    sizes.sort_by(|a, b| b.cmp(a));
    for (size, name) in sizes.iter().take(10) {
        eprintln!("/{name}: {size} characters");
    }
    assert!(sizes[0].0 <= 6_800, "largest root must retain Wave 8 headroom");
    for (size, name) in sizes {
        if matches!(name.as_str(), "collection" | "setup") {
            assert!(size <= 6_650, "{name} root: {size}");
        }
        assert!(size <= 6_800, "{name}: {size}");
        if name == "community" {
            assert!(size <= COMMUNITY_SIZE_BUDGET);
        }
        assert!(
            size <= crate::infra::routing::COMMAND_SIZE_BUDGET,
            "/{name}: {size} exceeds {} characters (growth budget)",
            crate::infra::routing::COMMAND_SIZE_BUDGET
        );
    }
}

#[test]
fn size_counts_longest_localizations_unicode_choices_and_nested_options() {
    let node = json!({
        "name":"a", "name_localizations":{"tr":"ışık", "de":"ab"},
        "description":"xy", "description_localizations":{"tr":"ü"},
        "options":[{"name":"g", "description":"ç", "options":[{
            "name":"s", "description":"d", "options":[{
                "name":"o", "description":"e", "choices":[
                    {"name":"a", "name_localizations":{"tr":"ığü"}, "value":"şö"},
                    {"name":"b", "value":123}, {"name":"c", "value":-1.5}
                ]
            }]
        }]}]
    });
    // 6 root + 2 group + 2 action + 2 option + 5 string + 4 integer + 5 number.
    assert_eq!(size_of_node(&node), 26);
    // Localizations replace shorter defaults; they are neither ignored nor added together.
    assert_eq!(
        size_of_node(&json!({"name":"x", "name_localizations":{"tr":"öö","de":"abc"}})),
        3
    );
    let mut tree = crate::command_definitions();
    tree[0].description = "a".repeat(COMMAND_SIZE_BUDGET);
    assert!(validate(&tree).unwrap_err().to_string().contains("characters exceeds"));
}
