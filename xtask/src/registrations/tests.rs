use super::*;

#[test]
fn every_domain_file_is_connected_and_generated_output_is_current() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    run(root, true).unwrap();
}

#[test]
fn ambiguous_and_duplicate_prefixes_fail_closed() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let original = collect(root).unwrap();
    for (section, tokens) in [
        ("components", "Some(\"lb\") => other(cx).await,"),
        ("owners", "\"lb\" => \"daily\","),
        ("opens_modal", "\"wo:g:extra\","),
        ("private", "\"house edit nested\","),
    ] {
        let mut broken = original.clone();
        broken
            .sections
            .get_mut(section)
            .unwrap()
            .insert(usize::MAX, tokens.parse().unwrap());
        assert!(validation::check(&broken).is_err(), "{section} accepted ambiguity");
    }
}

#[test]
fn whole_segments_allow_unrelated_sibling_prefixes() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let mut registration = collect(root).unwrap();
    registration
        .sections
        .get_mut("opens_modal")
        .unwrap()
        .insert(usize::MAX, "\"wo:gx\",".parse().unwrap());
    assert!(validation::check(&registration).is_ok());
}

#[test]
fn missing_required_fixture_metadata_fails_generation() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let folder = root.join("crates/pinchora-commands/src/commands/registration");
    let registration = read_registration(&folder.join("fixture.rs")).unwrap();
    let source = fs::read_to_string(folder.join("fixture_output.rs"))
        .unwrap()
        .replace("fixture-registrations:", "registrations:");
    for section in [
        "components",
        "owners",
        "private",
        "opens_modal",
        "help_start",
        "slash_only",
        "aliases",
        "component_gates",
    ] {
        let mut broken = registration.clone();
        broken.sections.remove(section);
        assert!(
            output::replace(&source, &broken, &mut BTreeSet::new()).is_err(),
            "accepted missing {section}"
        );
    }
}

#[test]
fn frozen_component_fixtures_keep_their_exact_handlers_and_guards() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let registration = collect(root).unwrap();
    let fixture: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(root.join("contracts/components.json")).unwrap()).unwrap();
    let mut actual = BTreeSet::new();
    for (section, kind) in [("components", "dispatch"), ("modals", "dispatch_modal")] {
        for tokens in registration.sections[section].values() {
            let arm: syn::Arm = syn::parse2(tokens.clone()).unwrap();
            for prefix in source::strings(&arm.pat) {
                let key = format!("{kind}:{prefix}");
                assert!(actual.insert(key.clone()), "duplicate {key}");
                assert_eq!(fixture["prefixes"][&key]["handler"], source::tokens(&arm.body), "{key}");
                assert_eq!(
                    fixture["prefixes"][&key]["guard"],
                    serde_json::json!(arm.guard.as_ref().map(|(_, guard)| source::tokens(guard))),
                    "{key}"
                );
            }
        }
    }
    assert_eq!(
        actual,
        fixture["prefixes"].as_object().unwrap().keys().cloned().collect()
    );
}

#[test]
fn missing_owner_help_or_gate_is_rejected_against_independent_fixtures() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let original = collect(root).unwrap();
    for section in ["owners", "component_gates", "help_house"] {
        let mut broken = original.clone();
        broken.sections.remove(section);
        assert!(
            validation::coverage(root, &broken).is_err(),
            "accepted missing {section}"
        );
    }
}

#[test]
fn adding_a_route_does_not_require_renumbering_other_domains() {
    let registration: Registration = syn::parse_str(
        r#"
        routes {
            ("zebra", "zebra", 0),
            ("alpha", "alpha", 0),
        }
        legacy_leaves {
            "zebra",
            "alpha",
        }
    "#,
    )
    .unwrap();
    let routes = output::routes(&registration).unwrap();
    assert_eq!(
        routes.iter().map(|r| r.0.as_str()).collect::<Vec<_>>(),
        ["alpha", "zebra"]
    );
    let output = output::data(&registration).unwrap();
    assert_eq!(output[1].1, "alpha\nzebra\n");
}

#[test]
fn overlapping_patterns_need_an_explicit_fallback() {
    for source in [
        r#"components { Some(kind @ ("dw" | "df")) => first(), Some("df") => second(), }"#,
        r#"owner_actions { ("xi", _) => first(), ("xi", "confirm") => second(), }"#,
        r#"autocomplete { _ => first(), "birthday" => second(), }"#,
        r#"autocomplete { fallback _ => first(), fallback _ => second(), }"#,
        r#"autocomplete { _ => first(), fallback _ => second(), }"#,
        r#"autocomplete { _ => first(), }"#,
        r#"owner_actions { (r"xi", "confirm") => first(), ("xi", "confirm") => second(), }"#,
    ] {
        let registration: Registration = syn::parse_str(source).unwrap();
        assert!(overlap::check(&registration).is_err(), "accepted {source}");
    }
    let registration: Registration = syn::parse_str(
        r#"owner_actions { ("xi", "confirm") => first(), ("xi", "cancel") => second(), }
        autocomplete { fallback _ => first(), "birthday" => second(), }"#,
    )
    .unwrap();
    assert!(overlap::check(&registration).is_ok());
}

#[test]
fn fallback_is_last_even_when_its_domain_is_first() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let registration = collect(root).unwrap();
    let entries = &registration.sections["autocomplete"];
    let arm: syn::Arm = syn::parse2(entries.last_key_value().unwrap().1.clone()).unwrap();
    assert!(matches!(arm.pat, syn::Pat::Wild(_)));
    assert!(overlap::check(&registration).is_ok());
}

#[test]
fn deleting_one_required_entry_cannot_hide_behind_an_existing_section() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let original = collect(root).unwrap();
    for section in ["owners", "component_gates"] {
        let mut broken = original.clone();
        let entries = broken.sections.get_mut(section).unwrap();
        let key = *entries
            .iter()
            .find(|(_, tokens)| source::strings(*tokens).contains(&"he".to_owned()))
            .unwrap()
            .0;
        entries.remove(&key);
        assert!(
            validation::coverage(root, &broken).is_err(),
            "accepted missing {section} entry"
        );
    }
    let fixture =
        read_registration(&root.join("crates/pinchora-commands/src/commands/registration/fixture.rs")).unwrap();
    validation::components(root, &fixture).unwrap();
    for section in ["owners", "component_gates"] {
        let mut broken = fixture.clone();
        broken.sections.get_mut(section).unwrap().clear();
        assert!(
            validation::components(root, &broken).is_err(),
            "accepted empty fixture {section}"
        );
    }
}
