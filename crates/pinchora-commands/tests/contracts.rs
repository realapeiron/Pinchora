//! Compiled definitions: no network, fixture IDs or runtime values enter these snapshots.
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::Path,
};
#[path = "support/inputs.rs"]
mod inputs;

fn snapshot(name: &str, value: &Value) {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let path = root.join("contracts").join(format!("{name}.json"));
    let text = format!("{}\n", serde_json::to_string_pretty(&value).unwrap());
    if std::env::var_os("PINCHORA_UPDATE_CONTRACTS").is_some() {
        fs::write(path, text).unwrap();
    } else {
        let expected: Value = serde_json::from_str(&fs::read_to_string(path).expect("checked-in contract")).unwrap();
        assert_eq!(
            &expected, value,
            "{name} contract changed; run cargo xtask contracts --update after review"
        );
    }
}

#[test]
fn compiled_contracts() {
    let mut commands = pinchora_commands::command_definitions();
    commands.sort_by(|a, b| a.name.cmp(&b.name));
    assert_eq!(commands.len(), 80);
    let mut contracts = BTreeMap::new();
    contracts.insert("commands", json!(commands));
    use pinchora_core::infra::i18n::{self, Lang};
    let keys: std::collections::BTreeMap<_, _> = Lang::ALL
        .into_iter()
        .map(|lang| {
            let keys: BTreeSet<_> = i18n::keys(lang).into_iter().collect();
            (lang.code(), keys)
        })
        .collect();
    contracts.insert("i18n", json!(keys));
    let sections: BTreeSet<_> = pinchora_core::admin::config_backup::SECTIONS
        .iter()
        .map(|s| s.table)
        .collect();
    contracts.insert("backup-sections", json!(sections));
    use pinchora_db::*;
    let tables: BTreeSet<_> = USER_COLUMNS
        .iter()
        .map(|r| r.0)
        .chain(ID_LISTS.iter().map(|r| r.0))
        .chain(CLEARED_COLUMNS.iter().map(|r| r.0))
        .chain(DAILY_LIMITS.iter().map(|r| r.0))
        .chain(ANONYMISED.iter().map(|r| r.0))
        .chain(ZEROED_COLUMNS.iter().map(|r| r.0))
        .chain(OTHER_TABLES.iter().copied())
        .collect();
    contracts.insert("privacy-tables", json!(tables));
    for (name, value) in &contracts {
        snapshot(name, value);
    }
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let output = root.join("target/contracts");
    fs::create_dir_all(&output).unwrap();
    let record = json!({"inputs": inputs::fingerprint(&root).unwrap(), "contracts": contracts});
    fs::write(output.join("runtime.tmp"), serde_json::to_vec(&record).unwrap()).unwrap();
    fs::rename(output.join("runtime.tmp"), output.join("runtime.json")).unwrap();
}
