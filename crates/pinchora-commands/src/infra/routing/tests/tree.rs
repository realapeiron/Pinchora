use super::*;

#[test]
fn snapshot_every_original_leaf_and_projected_budget() {
    let tree = crate::commands::legacy_definitions();
    let mut paths: Vec<_> = crate::commands::admin::help::leaves(&tree, Lang::En)
        .into_iter()
        .map(|l| l.path)
        .collect();
    paths.sort();
    assert_eq!(paths.join("\n") + "\n", include_str!("../legacy-leaves.txt"));
    assert_eq!(paths, MANIFEST.iter().map(|r| r.legacy.clone()).collect::<Vec<_>>());
    assert_eq!(tree.len(), 115);
    assert_eq!(paths.len(), 454);
    let registered = crate::command_definitions();
    assert_eq!(registered.len(), PROJECTED_ROOTS);
    assert_eq!(
        registered
            .iter()
            .filter(|c| c.kind == twilight_model::application::command::CommandType::ChatInput)
            .count(),
        78
    );
    assert_eq!(
        registered
            .iter()
            .filter(|c| c.kind == twilight_model::application::command::CommandType::Message && c.name == "Report")
            .count(),
        1
    );
    assert_eq!(
        registered
            .iter()
            .filter(|c| c.kind == twilight_model::application::command::CommandType::Message && c.name == "Open ticket")
            .count(),
        1
    );
    let actual: BTreeSet<_> = crate::commands::admin::help::leaves(&registered, Lang::En)
        .into_iter()
        .map(|l| l.path)
        .collect();
    assert!(
        MANIFEST.iter().all(|r| actual.contains(&r.canonical)),
        "every original action must still be reachable"
    );
    validate(&registered).unwrap();
    assert!(has_legacy_paths(&tree));
    assert!(!has_legacy_paths(&registered));
    let projected: BTreeSet<_> = MANIFEST
        .iter()
        .map(|r| r.canonical.split(' ').next().unwrap())
        .collect();
    assert_eq!(projected.len(), PROJECTED_ROOTS);
    assert_eq!(MANIFEST.iter().filter(|r| r.active()).count(), 243);
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
    assert_eq!(setup.options.len(), 7);
    assert!(tree.len() <= 100);
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
        assert_eq!(crate::admin::command_control::path(&adapted), route.legacy);
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
            crate::commands::admin::help::area_of(&route.canonical),
            crate::commands::admin::help::area_of(&route.legacy)
        );
        assert_eq!(protected(&route.canonical), protected(&route.legacy));
    }
}

#[test]
fn aliases_share_cumulative_policy_without_protecting_setup_root() {
    use pinchora_core::admin::command_control::Config;
    use pinchora_core::admin::command_control::Rule;
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
