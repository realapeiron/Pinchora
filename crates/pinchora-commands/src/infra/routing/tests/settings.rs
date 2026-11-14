use super::*;

#[test]
fn urgent_moderation_roots_keep_d11_options_permissions_and_previous_paths() {
    use pinchora_core::admin::command_control::Config;
    use pinchora_core::admin::command_control::Rule;
    let tree = crate::command_definitions();
    let old = crate::commands::legacy_definitions();
    let mod_root = tree.iter().find(|c| c.name == "mod").unwrap();
    for (name, permissions) in [
        ("ban", Permissions::BAN_MEMBERS),
        ("kick", Permissions::KICK_MEMBERS),
        ("warn", Permissions::MODERATE_MEMBERS),
        ("timeout", Permissions::MODERATE_MEMBERS),
        ("purge", Permissions::MANAGE_MESSAGES),
    ] {
        let root = tree.iter().find(|c| c.name == name).unwrap();
        assert_eq!(root.default_member_permissions, Some(permissions));
        assert_eq!(root.options, old.iter().find(|c| c.name == name).unwrap().options);
        assert!(!mod_root.options.iter().any(|o| o.name == name));
        let previous = format!("mod {name}");
        assert_eq!(canonical(&previous), name);
        assert_eq!(
            resolve_text(&format!("{previous} reason:\"two words\"")).0,
            format!("{name} reason:\"two words\"")
        );
        let data = serde_json::from_value(json!({"id":"1", "type":1, "name":"mod",
            "options":[{"type":1,"name":name,"options":[{"type":3,"name":"sentinel","value":"unchanged"}]}]}))
        .unwrap();
        let adapted = adapt(data).unwrap();
        assert_eq!(adapted.name, name);
        assert_eq!(adapted.options[0].value, CommandOptionValue::String("unchanged".into()));
        let config = Config {
            rules: [
                (
                    "mod".into(),
                    Rule {
                        enabled: false,
                        ..Rule::default()
                    },
                ),
                (
                    previous.clone(),
                    Rule {
                        cooldown_secs: 17,
                        ..Rule::default()
                    },
                ),
            ]
            .into(),
            aliases: [("old-action".into(), previous)].into(),
        };
        let migrated = normalize_control(&config);
        assert_eq!(normalize_control(&migrated), migrated);
        assert_eq!(migrated.aliases["old-action"], name);
        for policies in [&config, &migrated] {
            let rules = matching(policies, name);
            assert!(rules.iter().any(|(_, r)| !r.enabled));
            assert!(rules.iter().any(|(_, r)| r.cooldown_secs == 17));
        }
        migrated
            .validate(&crate::control_catalog(), &crate::command_reserved_aliases(), Lang::En)
            .unwrap();
    }
    let purge = tree.iter().find(|c| c.name == "purge").unwrap();
    for name in [
        "humans",
        "embeds",
        "regex",
        "after",
        "before",
        "starts",
        "ends",
        "not",
        "mentions",
        "reactions",
    ] {
        assert!(purge.options.iter().any(|o| o.name == name));
    }
}

#[test]
fn community_settings_keep_private_admin_boundaries_and_legacy_policies() {
    use pinchora_core::admin::command_control::Config;
    use pinchora_core::admin::command_control::Rule;
    let tree = crate::command_definitions();
    let catalog = crate::control_catalog();
    let leaves = crate::commands::admin::help::leaves(&tree, Lang::En);
    for path in COMMUNITY_ADMIN_PATHS {
        let route = route(path).unwrap();
        assert!(is_admin_path(path));
        assert_eq!(crate::Deferral::for_path(path), crate::Deferral::Private);
        assert_eq!(
            catalog.iter().find(|c| c.path == *path).unwrap().permissions,
            Permissions::MANAGE_GUILD.bits().to_string()
        );
        assert!(leaves.iter().find(|leaf| leaf.path == *path).unwrap().manager);
        assert_eq!(
            resolve_text(&format!("{} value:123", route.legacy)).0,
            format!("{path} value:123")
        );
        let config = Config {
            rules: [
                (
                    "community".into(),
                    Rule {
                        enabled: false,
                        constraints: [(
                            "historical-community".into(),
                            Rule {
                                denied_channels: vec!["9".into()],
                                ..Rule::default()
                            },
                        )]
                        .into(),
                        ..Rule::default()
                    },
                ),
                (
                    route.legacy.clone(),
                    Rule {
                        cooldown_secs: 13,
                        ..Rule::default()
                    },
                ),
            ]
            .into(),
            aliases: [("old-settings".into(), route.legacy.clone())].into(),
        };
        let normalized = normalize_control(&config);
        assert_eq!(normalized.aliases["old-settings"], *path);
        assert_eq!(normalize_control(&normalized), normalized);
        assert_eq!(
            normalized.rules[*path].constraints["historical-community"].denied_channels,
            ["9"]
        );
        for policies in [&config, &normalized] {
            assert!(matching(policies, path).iter().any(|(_, rule)| !rule.enabled));
            assert!(
                matching(policies, path)
                    .iter()
                    .any(|(_, rule)| rule.cooldown_secs == 13)
            );
            assert!(
                matching(policies, "community tickets settings")
                    .iter()
                    .any(|(_, rule)| !rule.enabled)
            );
        }
        normalized
            .validate(&catalog, &crate::command_reserved_aliases(), Lang::En)
            .unwrap();
    }
    for path in [
        "qotd suggest",
        "counting stats",
        "form list",
        "form open",
        "modmail close",
    ] {
        assert!(!is_admin_path(path));
        assert!(!leaves.iter().find(|leaf| leaf.path == path).unwrap().manager);
        assert_eq!(catalog.iter().find(|c| c.path == path).unwrap().permissions, "0");
    }
    validate(&tree).unwrap();
}
