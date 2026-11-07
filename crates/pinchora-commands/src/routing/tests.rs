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
    assert_eq!(tree.len(), 95);
    let registered = crate::command_definitions();
    assert_eq!(registered.len(), 96);
    validate(&registered).unwrap();
    let projected: BTreeSet<_> = MANIFEST
        .iter()
        .map(|r| r.canonical.split(' ').next().unwrap())
        .collect();
    assert_eq!(projected.len(), PROJECTED_ROOTS);
    assert_eq!(MANIFEST.iter().filter(|r| r.active()).count(), 84);
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
    for route in MANIFEST.iter().filter(|r| r.active()) {
        let metadata = route.metadata();
        assert_eq!(metadata.setup_permissions, Permissions::MANAGE_GUILD);
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
        let data: CommandData = serde_json::from_value(json!({
            "id":"1", "name":"setup", "type":1,
            "options":[{"type":2,"name":parts[1],"options":[{"type":1,"name":parts[2],
                "options":[{"type":3,"name":"sentinel","value":"unchanged"}]}]}],
            "resolved":{}
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
