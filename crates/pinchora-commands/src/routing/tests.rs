use super::*;
use pinchora_core::i18n::Lang;
use serde_json::json;

#[test]
fn snapshot_every_original_leaf_and_projected_budget() {
    let tree = crate::commands::legacy_definitions();
    let mut paths: Vec<_> = crate::commands::help::leaves(&tree, Lang::En)
        .into_iter()
        .map(|l| l.path)
        .collect();
    paths.sort();
    assert_eq!(paths.join("\n") + "\n", include_str!("legacy-leaves.txt"));
    assert_eq!(paths, MANIFEST.iter().map(|r| r.legacy.clone()).collect::<Vec<_>>());
    assert_eq!(tree.len(), 96);
    assert_eq!(paths.len(), 334);
    let registered = crate::command_definitions();
    assert_eq!(registered.len(), PROJECTED_ROOTS);
    let actual: BTreeSet<_> = crate::commands::help::leaves(&registered, Lang::En)
        .into_iter()
        .map(|l| l.path)
        .collect();
    assert_eq!(
        actual,
        MANIFEST.iter().map(|r| r.canonical.clone()).collect(),
        "every original action must still be reachable, once"
    );
    validate(&registered).unwrap();
    assert!(has_legacy_paths(&tree));
    assert!(!has_legacy_paths(&registered));
    let projected: BTreeSet<_> = MANIFEST
        .iter()
        .map(|r| r.canonical.split(' ').next().unwrap())
        .collect();
    assert_eq!(projected.len(), PROJECTED_ROOTS);
    assert_eq!(MANIFEST.iter().filter(|r| r.active()).count(), 163);
    assert_eq!(
        MANIFEST.iter().map(|r| &r.canonical).collect::<BTreeSet<_>>().len(),
        MANIFEST.len()
    );
}

#[test]
fn every_alias_preserves_handler_arguments_permissions_and_metadata() {
    let old = crate::commands::legacy_definitions();
    let tree = crate::command_definitions();
    let setup = tree.iter().find(|c| c.name == "setup").unwrap();
    assert_eq!(setup.default_member_permissions, Some(Permissions::MANAGE_GUILD));
    assert_eq!(setup.options.len(), 6);
    assert!(tree.len() + FUTURE_ROOT_RESERVE <= 70);
    for root in tree.iter().filter(|c| is_admin_path(&c.name)) {
        assert_eq!(root.default_member_permissions, Some(Permissions::MANAGE_GUILD));
    }
    for route in MANIFEST.iter().filter(|r| r.active()) {
        let metadata = route.metadata();
        assert_eq!(
            metadata.setup_permissions,
            if is_admin_path(&route.canonical) {
                Permissions::MANAGE_GUILD
            } else {
                Permissions::empty()
            }
        );
        assert_eq!(metadata.operation, route.legacy);
        assert_eq!(metadata.response, crate::Deferral::for_path(&route.canonical));
        let original = source(&old, &route.legacy);
        let aliased = source(&tree, &route.canonical);
        assert_eq!(
            original.options, aliased.options,
            "{} arguments changed",
            route.canonical
        );
        assert_eq!(original.description_localizations, aliased.description_localizations);
        let parts: Vec<_> = route.canonical.split(' ').collect();
        // An opaque argument and resolved map must survive unchanged, regardless of the old depth.
        let mut options = json!([{"type":3,"name":"sentinel","value":"unchanged"}]);
        for (index, part) in parts.iter().enumerate().skip(1).rev() {
            options = json!([{"type": if index == parts.len()-1 {1} else {2}, "name":part,"options":options}]);
        }
        let data: CommandData = serde_json::from_value(json!({
            "id":"1", "name":parts[0], "type":1, "options":options, "resolved":{}
        }))
        .unwrap();
        let resolved = data.resolved.clone();
        let adapted = adapt(data).unwrap();
        assert_eq!(crate::command_control::path(&adapted), route.legacy);
        assert_eq!(adapted.resolved, resolved);
        let mut options = &adapted.options;
        while let Some(CommandDataOption {
            value: CommandOptionValue::SubCommand(inner) | CommandOptionValue::SubCommandGroup(inner),
            ..
        }) = options.first()
        {
            options = inner;
        }
        assert_eq!(options[0].value, CommandOptionValue::String("unchanged".into()));
        assert_eq!(legacy(&route.canonical), legacy(&route.legacy));
        assert_eq!(canonical(&route.legacy), route.canonical);
        assert_eq!(
            route.module(),
            Module::of_command(&adapted.name, route.legacy.split(' ').nth(1))
        );
        assert_eq!(
            crate::commands::help::area_of(&route.canonical),
            crate::commands::help::area_of(&route.legacy)
        );
        assert_eq!(protected(&route.canonical), protected(&route.legacy));
    }
}

#[test]
fn aliases_share_cumulative_policy_without_protecting_setup_root() {
    use pinchora_core::command_control::{Config, Rule};
    let config = Config {
        rules: ["setup", "setup roles", "setup roles panel-post", "roles", "roles post"]
            .into_iter()
            .map(|p| (p.into(), Rule::default()))
            .collect(),
        ..Config::default()
    };
    assert_eq!(
        matching(&config, "roles post"),
        matching(&config, "setup roles panel-post")
    );
    assert_eq!(matching(&config, "roles post").len(), 5);
    assert!(protected("setup server modules-toggle"));
    assert!(!protected("setup"));
    assert!(!protected("setup roles panel-post"));
}

#[test]
fn validators_reject_root_depth_and_all_sibling_overflows() {
    let tree = crate::command_definitions();
    let mut broken = tree.clone();
    broken.resize(101, tree[0].clone());
    assert!(validate(&broken).is_err());
    let mut broken = tree.clone();
    let setup = broken.iter_mut().find(|c| c.name == "setup").unwrap();
    setup.options.resize(26, setup.options[0].clone());
    assert!(validate(&broken).is_err());
    let mut broken = tree.clone();
    let group = &mut broken.iter_mut().find(|c| c.name == "setup").unwrap().options[0];
    let children = group.options.as_mut().unwrap();
    children.resize(26, children[0].clone());
    assert!(validate(&broken).is_err());
    let mut broken = tree.clone();
    let group = &mut broken.iter_mut().find(|c| c.name == "setup").unwrap().options[0];
    group.options.as_mut().unwrap()[0].kind = CommandOptionType::SubCommandGroup;
    assert!(validate(&broken).is_err());
    let mut broken = tree;
    let group = &mut broken.iter_mut().find(|c| c.name == "setup").unwrap().options[0];
    group.options.as_mut().unwrap()[0].options = Some(vec![group.clone()]);
    assert!(validate(&broken).is_err());
}

#[test]
fn projected_tree_has_valid_depth_and_group_capacity() {
    let mut children = std::collections::BTreeMap::<String, BTreeSet<String>>::new();
    for route in MANIFEST.iter() {
        let parts: Vec<_> = route.canonical.split(' ').collect();
        assert!(parts.len() <= 3, "{}", route.canonical);
        for depth in 1..parts.len() {
            children
                .entry(parts[..depth].join(" "))
                .or_default()
                .insert(parts[depth].into());
        }
    }
    for (parent, children) in children {
        assert!(children.len() <= 25, "{parent}: {} children", children.len());
    }
}

#[test]
fn setup_autocomplete_retains_focus_and_original_module_suggestions() {
    let data: CommandData = serde_json::from_value(json!({
        "id":"1", "name":"setup", "type":1,
        "options":[{"type":2,"name":"server","options":[{"type":1,"name":"modules-toggle",
            "options":[{"type":3,"name":"module","value":"pets","focused":true}]}]}]
    }))
    .unwrap();
    let adapted = adapt(data).unwrap();
    let choices = crate::commands::modules::autocomplete(Lang::En, &adapted);
    assert!(!choices.is_empty());
    assert_eq!(crate::command_control::path(&adapted), "config modules toggle");
}

#[test]
fn validators_reject_argument_and_choice_overflows() {
    let mut tree = crate::command_definitions();
    let mut argument: CommandOption =
        serde_json::from_value(json!({"type":3,"name":"argument","description":"An argument"})).unwrap();
    let setup = tree.iter_mut().find(|c| c.name == "setup").unwrap();
    setup.options[0].options.as_mut().unwrap()[0].options = Some(vec![argument.clone(); 26]);
    assert!(
        validate(&tree)
            .unwrap_err()
            .to_string()
            .contains("more than 25 children/options")
    );
    argument.choices = Some(serde_json::from_value(json!([{"name":"Choice","value":"choice"}])).unwrap());
    let choice = argument.choices.as_ref().unwrap()[0].clone();
    argument.choices.as_mut().unwrap().resize(26, choice);
    let setup = tree.iter_mut().find(|c| c.name == "setup").unwrap();
    setup.options[0].options.as_mut().unwrap()[0].options = Some(vec![argument]);
    assert!(
        validate(&tree)
            .unwrap_err()
            .to_string()
            .contains("more than 25 choices")
    );
}

#[test]
fn mod_permissions_and_form_deferrals_are_action_specific() {
    let tree = crate::command_definitions();
    assert_eq!(
        tree.iter()
            .find(|c| c.name == "mod")
            .unwrap()
            .default_member_permissions,
        None
    );
    let catalog = crate::control_catalog();
    for route in MANIFEST.iter().filter(|r| r.slice == 9 || r.slice == 10) {
        let legacy_catalog = crate::command_control::control_validation_catalog();
        let old = legacy_catalog.iter().find(|c| c.path == route.legacy).unwrap();
        let new = catalog.iter().find(|c| c.path == route.canonical).unwrap();
        let original = crate::commands::legacy_definitions();
        let root = original
            .iter()
            .find(|c| Some(c.name.as_str()) == route.legacy.split(' ').next())
            .unwrap();
        assert_eq!(
            root.default_member_permissions
                .unwrap_or_else(Permissions::empty)
                .bits()
                .to_string(),
            new.permissions,
            "{}",
            route.canonical
        );
        assert_eq!(old.ephemeral_supported, new.ephemeral_supported);
    }
    for path in ["setup commands custom-create", "setup commands custom-edit"] {
        assert_eq!(crate::Deferral::for_path(path), crate::Deferral::Never);
        assert!(!protected(path));
    }
    for path in [
        "setup commands set",
        "setup commands reset",
        "setup backup restore",
        "setup server modules-toggle",
    ] {
        assert!(protected(path), "{path}");
    }
    assert!(!protected("setup welcome message"));
    assert!(!protected("setup commands"));
    assert_eq!(canonical("customcommand"), "customcommand");
    assert_eq!(canonical("sticky"), "sticky");
}

#[test]
fn legacy_prefix_resolution_preserves_argument_bytes_and_expands_once() {
    use pinchora_core::command_control::Config;
    for (old, new) in [
        ("config xp voice amount:5", "progression leveling xp-voice amount:5"),
        (
            "tickets panel create name:\"two words\"",
            "community tickets panel-create name:\"two words\"",
        ),
        (
            "XP\tIMPORT  MEE6  multiplier:2",
            "progression leveling xp-import-mee6  multiplier:2",
        ),
        ("weekly", "rewards weekly"),
        (
            "bio \"I like weekly rewards\"",
            "customize bio \"I like weekly rewards\"",
        ),
        ("feeds listing", "feeds listing"),
        ("tag setup", "tag setup"),
        ("mod lock", "mod lock"),
    ] {
        assert_eq!(resolve_text(old).0, new);
    }
    let config = Config {
        aliases: [
            ("levels".into(), "config xp voice".into()),
            ("setup".into(), "ping".into()),
        ]
        .into(),
        ..Config::default()
    };
    assert_eq!(
        resolve_text(&config.expand("levels amount:3")).0,
        "progression leveling xp-voice amount:3"
    );
    // Existing colliding aliases keep their meaning. New ones are rejected by validation.
    assert_eq!(resolve_text(&config.expand("setup")).0, "ping");
    assert!(
        config
            .validate(&crate::control_catalog(), &crate::command_reserved_aliases(), Lang::En)
            .is_err()
    );
    config
        .validate_update(
            &config,
            &crate::command_control::control_validation_catalog(),
            &crate::command_reserved_aliases(),
            Lang::En,
        )
        .unwrap();
    for route in MANIFEST.iter().filter(|r| r.active()) {
        assert_eq!(resolve_text(&route.legacy).0, route.canonical);
    }
}

#[test]
fn migration_covers_every_active_leaf_without_broadening_split_parents() {
    let sql = include_str!("../../../../migrations/0066_command_paths.sql");
    for route in MANIFEST.iter().filter(|r| r.active()) {
        if route.legacy.starts_with("branding ") {
            continue;
        }
        assert!(
            sql.contains(&format!("('{}', '{}')", route.legacy, previous_path(route))),
            "{}",
            route.legacy
        );
    }
    for path in ["sticky", "schedule", "config", "customcommand", "commandcontrol"] {
        assert_eq!(canonical(path), path);
        assert!(!sql.contains(&format!("('{path}',")));
    }
}

#[test]
fn member_additions_preserve_dm_contexts() {
    let old = crate::commands::legacy_definitions();
    let new = crate::command_definitions();
    for route in MANIFEST.iter().filter(|r| r.slice == 10) {
        let source = old.iter().find(|c| c.name == route.legacy).unwrap();
        let target = new
            .iter()
            .find(|c| Some(c.name.as_str()) == route.canonical.split(' ').next())
            .unwrap();
        assert_eq!(source.contexts, target.contexts, "{}", route.canonical);
    }
}

#[test]
fn migrated_split_parents_preserve_conjunctive_roles_channels_and_cooldown_identity() {
    use pinchora_core::command_control::{Config, Rule};
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
fn registered_command_sizes_keep_ten_percent_headroom() {
    let tree = crate::command_definitions();
    let mut sizes: Vec<_> = tree.iter().map(|c| (super::command_size(c), &c.name)).collect();
    sizes.sort_by(|a, b| b.cmp(a));
    for (size, name) in sizes.iter().take(10) {
        eprintln!("/{name}: {size} characters");
    }
    for (size, name) in sizes {
        assert!(
            size <= super::COMMAND_SIZE_BUDGET,
            "/{name}: {size} exceeds {} characters (10% margin)",
            super::COMMAND_SIZE_BUDGET
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

#[test]
fn previous_setup_paths_keep_arguments_and_migrate_flat_constraints() {
    use pinchora_core::command_control::{Config, Rule};
    for route in MANIFEST.iter().filter(|r| previous_path(r) != r.canonical) {
        let previous = previous_path(route);
        assert_eq!(canonical(&previous), route.canonical);
        assert_eq!(
            resolve_text(&format!("{previous} value:\"two words\"")).0,
            format!("{} value:\"two words\"", route.canonical)
        );
        let sql = include_str!("../../../../migrations/0068_command_domains.sql");
        assert!(sql.contains(&format!("('{previous}', '{}')", route.canonical)));
    }
    let config = Config {
        rules: [
            (
                "setup".into(),
                Rule {
                    cooldown_secs: 12,
                    ..Rule::default()
                },
            ),
            (
                "setup feeds".into(),
                Rule {
                    allowed_roles: vec!["1".into()],
                    ..Rule::default()
                },
            ),
            (
                "setup feeds list".into(),
                Rule {
                    constraints: [(
                        "feeds".into(),
                        Rule {
                            denied_channels: vec!["2".into()],
                            ..Rule::default()
                        },
                    )]
                    .into(),
                    allowed_roles: vec!["3".into()],
                    ..Rule::default()
                },
            ),
            (
                "integrations feeds list".into(),
                Rule {
                    allowed_roles: vec!["4".into()],
                    ..Rule::default()
                },
            ),
        ]
        .into(),
        aliases: [("feeds-old".into(), "setup feeds".into())].into(),
    };
    let normalized = normalize_control(&config);
    assert_eq!(normalize_control(&normalized), normalized);
    assert_eq!(normalized.aliases["feeds-old"], "integrations feeds");
    normalized
        .validate(&crate::control_catalog(), &crate::command_reserved_aliases(), Lang::En)
        .unwrap();
    let rule = &normalized.rules["integrations feeds list"];
    assert_eq!(rule.allowed_roles, ["4"]);
    assert_eq!(rule.constraints["setup feeds list"].allowed_roles, ["3"]);
    assert_eq!(rule.constraints["feeds"].denied_channels, ["2"]);
    assert_eq!(rule.constraints["setup feeds"].allowed_roles, ["1"]);
    assert_eq!(rule.constraints["setup"].cooldown_secs, 12);
    assert!(!normalized.rules.contains_key("setup server show"));
    assert!(!normalized.rules.contains_key("mod lock"));
    assert_eq!(
        normalized.rules["setup roles panel-post"].constraints["setup"].cooldown_secs,
        12
    );
}

#[test]
fn all_old_discord_payload_paths_remain_dispatchable() {
    for route in MANIFEST.iter() {
        let parts: Vec<_> = route.legacy.split(' ').collect();
        let mut options = json!([]);
        for (index, part) in parts.iter().enumerate().skip(1).rev() {
            options = json!([{"type": if index == parts.len()-1 {1} else {2}, "name":part, "options":options}]);
        }
        let data: CommandData =
            serde_json::from_value(json!({"id":"1", "type":1,"name":parts[0], "options":options})).unwrap();
        let adapted = adapt(data).unwrap();
        assert_eq!(crate::command_control::path(&adapted), route.legacy);
    }
}

#[test]
fn remapped_setup_parent_restrictions_still_cover_old_component_owners() {
    use pinchora_core::command_control::{Config, Rule};
    let config = Config {
        rules: [(
            "setup automod".into(),
            Rule {
                enabled: false,
                ..Rule::default()
            },
        )]
        .into(),
        ..Config::default()
    };
    let migrated = normalize_control(&config);
    for path in ["automod", "automod show", "safety automod", "safety automod show"] {
        assert!(
            matching(&migrated, path).iter().any(|(_, rule)| !rule.enabled),
            "{path}"
        );
    }
    assert!(matching(&migrated, "verification").iter().all(|(_, rule)| rule.enabled));
}
