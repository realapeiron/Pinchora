use crate::{Result, source};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::Path,
    process::Command,
};
mod identifiers;
#[path = "../../../crates/pinchora-commands/tests/support/inputs.rs"]
mod inputs;
mod keys;
mod routes;

pub fn run(root: &Path, update: bool, prepare: bool) -> Result {
    if prepare {
        let path = root.join("target/contracts/observations");
        if path.exists() {
            fs::remove_dir_all(&path)?;
        }
        fs::create_dir_all(path)?;
        let runtime = root.join("target/contracts/runtime.json");
        if runtime.exists() {
            fs::remove_file(runtime)?;
        }
        fs::write(
            root.join("target/contracts/prepared-inputs.txt"),
            inputs::fingerprint(root)?,
        )?;
        println!("contract observations cleared; run the complete suite before --check/--update");
        return Ok(());
    }
    let routes = routes::collect(root)?;
    source::snapshot(root, "routes", &json!(routes), update)?;
    identifiers::run(root, update)?;
    if update {
        let status = Command::new("cargo")
            .args(["test", "--locked", "-p", "pinchora-commands", "--test", "contracts"])
            .env("PINCHORA_UPDATE_CONTRACTS", "1")
            .current_dir(root)
            .status()?;
        if !status.success() {
            return Err("compiled contracts changed".into());
        }
    }
    let prepared = fs::read_to_string(root.join("target/contracts/prepared-inputs.txt"))
        .map_err(|_| "missing preparation receipt; prepare and rerun the workspace suite")?;
    if prepared != inputs::fingerprint(root)? {
        return Err("source inputs changed since preparation; prepare and rerun the workspace suite".into());
    }
    let runtime: Value = serde_json::from_slice(
        &fs::read(root.join("target/contracts/runtime.json"))
            .map_err(|_| "missing compiled contracts; run the workspace suite first")?,
    )?;
    if runtime["inputs"].as_str() != Some(&inputs::fingerprint(root)?) {
        return Err("source inputs changed since recording; prepare and rerun the workspace suite".into());
    }
    for (name, contract) in runtime["contracts"].as_object().ok_or("missing compiled contracts")? {
        source::snapshot(root, name, contract, update)?;
    }
    shapes(root, &routes, update)
}

fn shapes(root: &Path, routes: &BTreeSet<String>, update: bool) -> Result {
    let dir = root.join("target/contracts/observations");
    let mut shapes: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for file in fs::read_dir(&dir).map_err(|_| "no API observations; run the integration suite first")? {
        let file = file?.path();
        if file.extension().is_none_or(|s| s != "json") {
            continue;
        }
        let records: BTreeMap<String, BTreeSet<String>> = serde_json::from_slice(&fs::read(file)?)?;
        for (key, shape) in records {
            shapes.entry(key).or_default().extend(shape);
        }
    }
    if shapes.is_empty() {
        return Err("empty API observations; test services must be enabled".into());
    }
    source::snapshot(root, "api-shapes", &json!(shapes), update)?;
    let covered: BTreeSet<_> = shapes
        .keys()
        .map(|k| k.rsplit_once(' ').unwrap().0.to_owned())
        .collect();
    let api: BTreeSet<_> = routes
        .iter()
        .filter(|r| r.contains(" /api/") && !r.starts_with("HEAD ") && !r.starts_with("ANY "))
        .cloned()
        .collect();
    let missing: Vec<_> = api.difference(&covered).cloned().collect();
    let successes: BTreeSet<_> = shapes
        .keys()
        .filter(|k| k.rsplit_once(' ').unwrap().1.starts_with('2'))
        .map(|k| k.rsplit_once(' ').unwrap().0.to_owned())
        .collect();
    let coverage = json!({"total": api.len(), "observed": api.intersection(&covered).count(), "successful": api.intersection(&successes).count(), "unobserved": missing});
    source::snapshot(root, "api-coverage", &coverage, update)?;
    println!(
        "API coverage: {}/{} routes; {} successful",
        coverage["observed"], coverage["total"], coverage["successful"]
    );
    Ok(())
}
