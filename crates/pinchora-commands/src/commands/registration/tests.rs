use std::collections::BTreeSet;

#[test]
fn typed_definitions_have_unique_matching_dispatch_entries() {
    let mut names = BTreeSet::new();
    let mut order = BTreeSet::new();
    for entry in crate::commands::entries() {
        assert!(names.insert(entry.name), "duplicate command {}", entry.name);
        assert!(order.insert(entry.order), "duplicate command order {}", entry.order);
        assert_eq!((entry.definition)().name, entry.name);
    }
    let fixture = include_str!("../../infra/routing/legacy-leaves.txt");
    for path in fixture.lines() {
        let root = if path == "Open ticket" {
            path
        } else {
            path.split(' ').next().unwrap()
        };
        assert!(
            names.contains(root),
            "frozen legacy command {path} has no dispatch entry"
        );
    }
}

#[test]
fn compiled_tree_matches_frozen_contract() {
    let expected: serde_json::Value =
        serde_json::from_str(include_str!("../../../../../contracts/commands.json")).unwrap();
    let mut definitions = crate::command_definitions();
    definitions.sort_by(|left, right| left.name.cmp(&right.name));
    assert_eq!(serde_json::to_value(definitions).unwrap(), expected);
}
