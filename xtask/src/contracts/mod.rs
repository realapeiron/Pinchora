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
        fs::write(
            root.join("target/contracts/routes.json"),
            serde_json::to_vec(&routes::collect(root)?)?,
        )?;
        let runtime = root.join("target/contracts/runtime.json");
        if runtime.exists() {
            fs::remove_file(runtime)?;
        }
        let receipt = json!({
            "inputs": inputs::fingerprint(root)?,
            "run": format!("{}-{}", std::process::id(),
                std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)?.as_nanos())
        });
        fs::write(
            root.join("target/contracts/prepared.json"),
            serde_json::to_vec(&receipt)?,
        )?;
        println!("contract observations cleared; run the complete suite before --check/--update");
        return Ok(());
    }
    let routes = routes::collect(root)?;
    if update {
        let receipt = inputs::receipt(root)?;
        require_observations(root, &routes, &observations(root, &receipt)?)?;
    }
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
    let receipt = inputs::receipt(root).map_err(|e| {
        format!("missing or stale contract preparation: {e}; prepare and run the complete workspace suite")
    })?;
    let runtime: Value = serde_json::from_slice(
        &fs::read(root.join("target/contracts/runtime.json"))
            .map_err(|_| "missing compiled contracts; run the workspace suite first")?,
    )?;
    if runtime["receipt"] != receipt {
        return Err("source inputs changed since recording; prepare and rerun the workspace suite".into());
    }
    for (name, contract) in runtime["contracts"].as_object().ok_or("missing compiled contracts")? {
        source::snapshot(root, name, contract, update)?;
    }
    shapes(root, &routes, &receipt, update)
}

fn shapes(root: &Path, routes: &BTreeSet<String>, receipt: &Value, update: bool) -> Result {
    let shapes = observations(root, receipt)?;
    require_observations(root, routes, &shapes)?;
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

fn require_observations(root: &Path, routes: &BTreeSet<String>, shapes: &BTreeMap<String, BTreeSet<String>>) -> Result {
    let expected: BTreeMap<String, BTreeSet<String>> =
        serde_json::from_slice(&fs::read(root.join("contracts/api-shapes.json"))?)?;
    let missing: Vec<_> = expected
        .keys()
        .filter(|key| {
            let route = key.rsplit_once(' ').unwrap().0;
            let any = format!("ANY {}", route.split_once(' ').unwrap().1);
            (routes.contains(route) || routes.contains(&any)) && !shapes.contains_key(*key)
        })
        .collect();
    if !missing.is_empty() {
        return Err(format!("incomplete API observations (possibly a filtered run): {} response variants missing; first: {}. Run --prepare and the complete suite. For an intentional status removal, review and remove its snapshot entry explicitly before updating.", missing.len(), missing[0]).into());
    }
    let covered: BTreeSet<_> = shapes.keys().map(|k| k.rsplit_once(' ').unwrap().0).collect();
    let missing: Vec<_> = routes
        .iter()
        .filter(|r| {
            r.contains(" /api/") && !r.starts_with("HEAD ") && !r.starts_with("ANY ") && !covered.contains(r.as_str())
        })
        .collect();
    if !missing.is_empty() {
        return Err(format!("API routes without observations: {missing:?}; add fixtures before updating").into());
    }
    Ok(())
}

fn observations(root: &Path, receipt: &Value) -> Result<BTreeMap<String, BTreeSet<String>>> {
    let dir = root.join("target/contracts/observations");
    let mut shapes: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for file in fs::read_dir(&dir).map_err(|_| "no API observations; run the integration suite first")? {
        let file = file?.path();
        if file.extension().is_none_or(|s| s != "json") {
            continue;
        }
        let record: Value = serde_json::from_slice(&fs::read(file)?)?;
        if record["receipt"] != *receipt {
            return Err("stale API observation from another run; prepare and rerun the complete suite".into());
        }
        let records: BTreeMap<String, BTreeSet<String>> = serde_json::from_value(record["shapes"].clone())?;
        for (key, shape) in records {
            shapes.entry(key).or_default().extend(shape);
        }
    }
    if shapes.is_empty() {
        return Err("empty API observations; test services must be enabled".into());
    }
    Ok(shapes)
}
