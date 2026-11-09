use crate::Result;
use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
    process::Command,
};

fn allowed(from: &str, to: &str) -> bool {
    if to == "pinchora-testkit" {
        return true;
    } // Kind is checked separately, without exceptions.
    match from {
        "pinchora" | "pinchora-testkit" | "xtask" => true,
        "pinchora-core" => false,
        "pinchora-db" | "pinchora-cache" => to == "pinchora-core",
        "pinchora-render" => to == "pinchora-core",
        "pinchora-game" => matches!(to, "pinchora-core" | "pinchora-render"),
        "pinchora-services" => matches!(
            to,
            "pinchora-core" | "pinchora-db" | "pinchora-cache" | "pinchora-game" | "pinchora-render"
        ),
        "pinchora-web" | "pinchora-commands" => matches!(
            to,
            "pinchora-services" | "pinchora-core" | "pinchora-render" | "pinchora-game"
        ),
        _ => false,
    }
}

pub fn run(root: &Path) -> Result {
    let output = Command::new("cargo")
        .args(["metadata", "--no-deps", "--locked", "--format-version", "1"])
        .current_dir(root)
        .output()?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).into_owned().into());
    }
    let meta: Value = serde_json::from_slice(&output.stdout)?;
    let packages = meta["packages"].as_array().ok_or("missing packages")?;
    let identities: BTreeMap<_, _> = packages
        .iter()
        .map(|p| {
            (
                Path::new(p["manifest_path"].as_str().unwrap())
                    .parent()
                    .unwrap()
                    .canonicalize()
                    .unwrap(),
                p["name"].as_str().unwrap(),
            )
        })
        .collect();
    let exceptions: BTreeSet<String> =
        serde_json::from_slice(&std::fs::read(root.join("contracts/dependency-exceptions.json"))?)?;
    let mut graph: BTreeMap<String, Vec<(String, String)>> = BTreeMap::new();
    let mut used = BTreeSet::new();
    let mut errors = vec![];
    for package in packages {
        let from = package["name"].as_str().unwrap();
        for dep in package["dependencies"].as_array().unwrap() {
            let Some(path) = dep["path"].as_str() else {
                continue;
            };
            let canonical = Path::new(path).canonicalize()?;
            let Some(&to) = identities.get(&canonical) else {
                errors.push(format!(
                    "{from}: local dependency {} is outside the audited workspace",
                    dep["name"]
                ));
                continue;
            };
            let kind = dep["kind"].as_str().unwrap_or("normal");
            let target = dep["target"].as_str().unwrap_or("all");
            let edge = format!("{from} -> {to} ({kind}, {target})");
            graph.entry(from.into()).or_default().push((to.into(), edge.clone()));
            if to == "pinchora-testkit" && kind != "dev" {
                errors.push(edge);
            } else if !allowed(from, to) {
                if exceptions.contains(&edge) {
                    used.insert(edge);
                } else {
                    errors.push(edge);
                }
            }
        }
    }
    errors.extend(transitive(&graph, &exceptions));
    for stale in exceptions.difference(&used) {
        errors.push(format!("remove stale exception: {stale}"));
    }
    // Compare against the committed ratchet as well: editing the exception file cannot bless a new edge.
    super::size::ratchet_set(root, "contracts/dependency-exceptions.json", &exceptions)?;
    if !errors.is_empty() {
        return Err(format!("dependency direction:\n{}", errors.join("\n")).into());
    }
    println!("depcheck: {} transitional edges", used.len());
    crate::boundary::run(root, &meta)
}

// Dev helpers must not become a tunnel from lower layers to adapters.
fn transitive(graph: &BTreeMap<String, Vec<(String, String)>>, exceptions: &BTreeSet<String>) -> Vec<String> {
    let mut errors = BTreeSet::new();
    for source in graph.keys() {
        let mut pending = vec![(source.clone(), vec![source.clone()])];
        while let Some((node, path)) = pending.pop() {
            for (target, edge) in graph.get(&node).into_iter().flatten() {
                if path.contains(target) {
                    continue;
                }
                let mut next = path.clone();
                next.push(target.clone());
                let reachable = allowed(source, target)
                    || (matches!(source.as_str(), "pinchora-commands" | "pinchora-web")
                        && matches!(target.as_str(), "pinchora-db" | "pinchora-cache"));
                if !reachable && !(path.len() == 1 && exceptions.contains(edge)) {
                    errors.insert(format!("transitive: {}", next.join(" -> ")));
                }
                pending.push((target.clone(), next));
            }
        }
    }
    errors.into_iter().collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn dev_helpers_cannot_hide_upward_paths() {
        let graph = BTreeMap::from([
            ("pinchora-db".into(), vec![("pinchora-testkit".into(), "dev".into())]),
            (
                "pinchora-testkit".into(),
                vec![("pinchora-commands".into(), "normal".into())],
            ),
        ]);
        assert!(
            transitive(&graph, &BTreeSet::new())
                .iter()
                .any(|e| e.contains("pinchora-db -> pinchora-testkit -> pinchora-commands"))
        );
    }
}
