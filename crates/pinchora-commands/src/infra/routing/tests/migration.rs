use super::*;

#[test]
fn previous_setup_paths_keep_arguments_and_migrate_flat_constraints() {
    use pinchora_core::admin::command_control::Config;
    use pinchora_core::admin::command_control::Rule;
    for route in MANIFEST.iter().filter(|r| {
        r.slice <= 19
            && previous_path(r).starts_with("setup ")
            && previous_path(r) != r.canonical
            && !r.legacy.starts_with("modtools ")
    }) {
        let previous = previous_path(route);
        assert_eq!(canonical(&previous), route.canonical);
        assert_eq!(
            resolve_text(&format!("{previous} value:\"two words\"")).0,
            format!("{} value:\"two words\"", route.canonical)
        );
        let sql = include_str!("../../../../../../migrations/0068_command_domains.sql");
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
        assert_eq!(crate::admin::command_control::path(&adapted), route.legacy);
    }
}

#[test]
fn remapped_setup_parent_restrictions_still_cover_old_component_owners() {
    use pinchora_core::admin::command_control::Config;
    use pinchora_core::admin::command_control::Rule;
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
