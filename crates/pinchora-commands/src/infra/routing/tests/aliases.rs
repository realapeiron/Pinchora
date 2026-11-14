use super::*;

#[test]
fn setup_autocomplete_retains_focus_and_original_module_suggestions() {
    let data: CommandData = serde_json::from_value(json!({
        "id":"1", "name":"setup", "type":1,
        "options":[{"type":2,"name":"server","options":[{"type":1,"name":"modules-toggle",
            "options":[{"type":3,"name":"module","value":"pets","focused":true}]}]}]
    }))
    .unwrap();
    let adapted = adapt(data).unwrap();
    let choices = crate::commands::admin::modules::autocomplete(Lang::En, &adapted);
    assert!(!choices.is_empty());
    assert_eq!(crate::admin::command_control::path(&adapted), "config modules toggle");
}

#[test]
fn validators_reject_argument_and_choice_overflows() {
    let mut tree = crate::command_definitions();
    let mut argument: CommandOption =
        serde_json::from_value(json!({"type":3,"name":"argument","description":"An argument"})).unwrap();
    let setup = tree.iter_mut().find(|c| c.name == "setup").unwrap();
    // Isolate the option limit from the complete root's independent character budget.
    setup.options.truncate(1);
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
        let legacy_catalog = crate::admin::command_control::control_validation_catalog();
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
    use pinchora_core::admin::command_control::Config;
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
            &crate::admin::command_control::control_validation_catalog(),
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
    let sql = concat!(
        include_str!("../../../../../../migrations/0066_command_paths.sql"),
        include_str!("../../../../../../migrations/0074_mod_parity.sql")
    );
    for route in MANIFEST.iter().filter(|r| {
        r.active()
            && r.slice <= 18
            && !r.legacy.starts_with("community ")
            && !r.legacy.starts_with("roblox")
            && !r.legacy.starts_with("branding ")
    }) {
        assert!(
            sql.contains(&format!("('{}', '{}')", route.legacy, previous_path(route)))
                || sql.contains(&format!("('{}', '{}')", route.legacy, route.canonical)),
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
