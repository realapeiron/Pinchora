//! Version-one command path normalization, also used for pre-consolidation backups.
use pinchora_core::admin::command_control::Config;
use pinchora_core::admin::config_backup::Backup;

use crate::infra::routing::MANIFEST;
use crate::infra::routing::canonical;
use crate::infra::routing::previous_path;
use crate::infra::routing::protected;

/// Expand a split parent over exactly its former leaves; never broaden it to a shared setup group.
pub fn normalize_control(config: &Config) -> Config {
    let mut result = config.clone();
    for (path, rule) in &config.rules {
        let targets: Vec<_> = MANIFEST
            .iter()
            .filter(|r| r.legacy == *path || r.legacy.strip_prefix(path).is_some_and(|s| s.starts_with(' ')))
            .collect();
        if !targets.iter().any(|r| r.legacy != r.canonical) {
            continue;
        }
        if path != "community" {
            result.rules.remove(path);
        }
        for target in targets {
            if protected(&target.canonical) {
                continue;
            }
            // Keep the original key as the cooldown identity, including across split siblings.
            let constraints = &mut result.rules.entry(previous_path(target)).or_default().constraints;
            constraints.extend(rule.constraints.clone());
            let mut own = rule.clone();
            own.constraints.clear();
            constraints.insert(path.clone(), own);
        }
    }
    // 0068 runs after v1. Flatten inherited constraints so every old cooldown identity and
    // independent role/channel restriction survives a second remap and backup validation.
    let original = result.rules.clone();
    for (path, rule) in &original {
        let targets: Vec<_> = MANIFEST
            .iter()
            .filter(|r| {
                let previous = previous_path(r);
                (previous.starts_with("setup ") || previous != r.canonical)
                    && (previous == *path || previous.strip_prefix(path).is_some_and(|s| s.starts_with(' ')))
            })
            .collect();
        if !targets.iter().any(|r| previous_path(r) != r.canonical) {
            continue;
        }
        // `/mod` still owns secondary actions; retain its rule as we split off urgent roots.
        if path != "mod" {
            result.rules.remove(path);
        }
        for target in targets {
            if protected(&target.canonical) {
                continue;
            }
            let constraints = &mut result.rules.entry(target.canonical.clone()).or_default().constraints;
            constraints.extend(rule.constraints.clone());
            let mut own = rule.clone();
            own.constraints.clear();
            constraints.insert(path.clone(), own);
        }
    }
    for path in result.aliases.values_mut() {
        *path = canonical(path).to_owned();
    }
    result
}

/// Version 1 predates the tree. The mapping version is fixed by the import implementation;
/// exported documents retain their schema version and old restore points need no rewrite.
pub fn normalize_backup(backup: &mut Backup) -> anyhow::Result<()> {
    anyhow::ensure!(backup.version == 1, "unsupported command path mapping version");
    for row in backup.sections.get_mut("command_control").into_iter().flatten() {
        let config: Config = serde_json::from_value(row["config"].clone())?;
        row["config"] = serde_json::to_value(normalize_control(&config))?;
    }
    Ok(())
}
