//! Embeds every catalog under `locales/<code>/<area>.toml` into the binary: adding a file is all a
//! new area or language needs, with no list to keep in step.

use std::{env, fs, path::Path};

fn main() {
    let root = Path::new(&env::var("CARGO_MANIFEST_DIR").expect("cargo sets it")).join("locales");
    println!("cargo:rerun-if-changed={}", root.display());
    let mut files = Vec::new();
    for lang in fs::read_dir(&root).expect("locales/ exists") {
        let lang = lang.expect("a readable locales/ entry").path();
        if !lang.is_dir() {
            continue;
        }
        println!("cargo:rerun-if-changed={}", lang.display());
        for file in fs::read_dir(&lang).expect("a readable language folder") {
            let file = file.expect("a readable catalog").path();
            if file.extension().is_some_and(|e| e == "toml") {
                println!("cargo:rerun-if-changed={}", file.display());
                files.push(file);
            }
        }
    }
    files.sort();
    let mut out = String::from("/// `(language code, area, catalog text)` for every file under `locales/`.\n");
    out.push_str("pub(crate) static SOURCES: &[(&str, &str, &str)] = &[\n");
    for file in files {
        let lang = file
            .parent()
            .and_then(|p| p.file_name())
            .and_then(|n| n.to_str())
            .expect("utf-8 name");
        let area = file.file_stem().and_then(|n| n.to_str()).expect("utf-8 name");
        let area = match area {
            "community_engagement" => "community_pack",
            "community_tools" => "community_v2",
            "economy_player" => "economy_v2",
            "reef_party" => "party_v2",
            "role_automation" => "roles_pack",
            other => other,
        };
        out.push_str(&format!(
            "    ({lang:?}, {area:?}, include_str!({:?})),\n",
            file.display().to_string()
        ));
    }
    out.push_str("];\n");
    fs::write(
        Path::new(&env::var("OUT_DIR").expect("cargo sets it")).join("locales.rs"),
        out,
    )
    .expect("OUT_DIR is writable");
}
