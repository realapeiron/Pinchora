use std::collections::BTreeSet;
use std::env;
use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::Value;

const START: &str = "<!-- crate-table:start -->";
const END: &str = "<!-- crate-table:end -->";

mod boundary;
mod contracts;
mod dependency;
mod names;
mod registrations;
mod size;
mod source;
type Result<T = ()> = std::result::Result<T, Box<dyn Error>>;

fn main() -> Result {
    let args: Vec<String> = env::args().skip(1).collect();
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let update = args.get(1).is_some_and(|s| s == "--update");
    match args.first().map(String::as_str) {
        Some("registrations") => registrations::run(root, args.get(1).is_some_and(|s| s == "--check")),
        Some("architecture") => architecture(args.get(1).is_some_and(|s| s == "--check")),
        Some("contracts") => contracts::run(root, update, args.get(1).is_some_and(|s| s == "--prepare")),
        Some("depcheck") => dependency::run(root),
        Some("names") => names::run(root),
        Some("size") => size::run(root, update),
        Some("check") => {
            let results = [registrations::run(root, true), dependency::run(root), names::run(root), size::run(root, false), contracts::run(root, false, false)];
            let failures: Vec<_> = results.into_iter().filter_map(std::result::Result::err).collect();
            for failure in &failures { eprintln!("{failure}"); }
            if failures.is_empty() { Ok(()) } else { Err(format!("{} quality gates failed", failures.len()).into()) }
        }
        _ => Err("Usage: cargo xtask {check|contracts [--check|--update|--prepare]|depcheck|names|size [--update]|architecture [--check]}".into()),
    }
}

fn architecture(check: bool) -> Result {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .ok_or("xtask has no workspace parent")?;
    let path = root.join("docs/ARCHITECTURE.md");
    let original = fs::read_to_string(&path)?;
    let updated = replace_table(&original, &crate_table(root)?)?;
    if check {
        if original != updated {
            eprintln!(
                "{} has a stale crate table; run cargo xtask architecture",
                path.display()
            );
            std::process::exit(1);
        }
    } else if original != updated {
        fs::write(path, updated)?;
    }
    Ok(())
}

fn crate_table(root: &Path) -> std::result::Result<String, Box<dyn Error>> {
    let output = Command::new("cargo")
        .args(["metadata", "--no-deps", "--locked", "--format-version", "1"])
        .current_dir(root)
        .output()?;
    if !output.status.success() {
        return Err(format!("cargo metadata failed: {}", String::from_utf8_lossy(&output.stderr)).into());
    }
    let metadata: Value = serde_json::from_slice(&output.stdout)?;
    let packages = metadata["packages"].as_array().ok_or("metadata packages are missing")?;
    let crates: Vec<&Value> = packages.iter().filter(|package| is_crate(package)).collect();
    let names: BTreeSet<&str> = crates.iter().filter_map(|package| package["name"].as_str()).collect();
    let mut rows = Vec::new();
    for package in crates {
        let name = package["name"].as_str().ok_or("package name is missing")?;
        let description = package["description"].as_str().ok_or("crate description is missing")?;
        let dependencies = internal_dependencies(package, &names)?;
        rows.push((name.to_owned(), description.to_owned(), dependencies));
    }
    rows.sort_by(|left, right| left.0.cmp(&right.0));
    let mut table = String::from("| Crate | Description | Internal dependencies |\n|---|---|---|\n");
    for (name, description, dependencies) in rows {
        let dependencies = if dependencies.is_empty() {
            "—".to_owned()
        } else {
            dependencies
                .iter()
                .map(|name| format!("`{name}`"))
                .collect::<Vec<_>>()
                .join(", ")
        };
        table.push_str(&format!(
            "| `{name}` | {} | {dependencies} |\n",
            escape_cell(&description)
        ));
    }
    Ok(table)
}

fn is_crate(package: &Value) -> bool {
    let Some(path) = package["manifest_path"].as_str() else {
        return false;
    };
    PathBuf::from(path)
        .parent()
        .and_then(Path::parent)
        .and_then(Path::file_name)
        .is_some_and(|name| name == "crates")
}

fn internal_dependencies(package: &Value, names: &BTreeSet<&str>) -> std::result::Result<Vec<String>, Box<dyn Error>> {
    let dependencies = package["dependencies"]
        .as_array()
        .ok_or("package dependencies are missing")?;
    let mut internal = BTreeSet::new();
    for dependency in dependencies {
        if dependency["path"].is_null() {
            continue;
        }
        if let Some(name) = dependency["name"].as_str()
            && names.contains(name)
        {
            internal.insert(name.to_owned());
        }
    }
    Ok(internal.into_iter().collect())
}

fn escape_cell(value: &str) -> String {
    value.replace('|', "\\|").replace('\n', " ")
}

fn replace_table(document: &str, table: &str) -> std::result::Result<String, Box<dyn Error>> {
    let start = document.find(START).ok_or("crate table start marker is missing")?;
    let body = start + START.len();
    let end = document[body..].find(END).ok_or("crate table end marker is missing")? + body;
    if document[end + END.len()..].contains(END) || document[body..].contains(START) {
        return Err("crate table markers are duplicated".into());
    }
    Ok(format!("{}\n\n{}{}", &document[..body], table, &document[end..]))
}
