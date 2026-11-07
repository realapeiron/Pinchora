//! The command list and the content counts as the public site shows them, written to
//! `web/src/lib/commands.json`, `commands.tr.json`, and `facts.json`: every command grouped in the areas of `/help`,
//! from the same registered command tree the bot answers with, and how much furniture, how many pets, fish,
//! backgrounds and badges there are. The `/commands` page and the landing page read these files, so a command
//! added (or renamed, or re-described) or content added and forgotten here fails `committed_site_data_matches`.
//!
//! Rewrite after changing a command or the content:
//! `cargo test -p pinchora-web commands_catalog -- --ignored write_site_data`.

use pinchora_commands::{command_catalog, command_catalog_tr, site_facts};
use serde_json::Value;

const DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../web/src/lib");

fn rendered(value: &Value) -> String {
    let mut text = serde_json::to_string_pretty(value).expect("the data serializes");
    text.push('\n');
    text
}

fn files() -> [(&'static str, String); 3] {
    [
        ("commands.json", rendered(&command_catalog())),
        ("commands.tr.json", rendered(&command_catalog_tr())),
        ("facts.json", rendered(&site_facts())),
    ]
}

#[test]
fn committed_site_data_matches() {
    for (name, text) in files() {
        let committed = std::fs::read_to_string(format!("{DIR}/{name}")).unwrap_or_default();
        assert!(
            committed == text,
            "web/src/lib/{name} is out of date: run `cargo test -p pinchora-web commands_catalog -- --ignored write_site_data`"
        );
    }
}

#[test]
#[ignore = "writes web/src/lib/commands.json and facts.json"]
fn write_site_data() {
    for (name, text) in files() {
        std::fs::write(format!("{DIR}/{name}"), text).expect("write the site data");
    }
}
