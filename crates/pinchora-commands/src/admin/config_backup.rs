//! Backup orchestration and the same pure validators used by the feature editors.
mod grants;
mod validation;

use grants::check_grants;
pub use validation::validate_backup;

use crate::App;
use pinchora_core::admin::config_backup;
use pinchora_core::admin::config_backup::Backup;
use pinchora_core::admin::config_backup::Import;
use pinchora_core::admin::config_backup::Names;
use pinchora_core::admin::config_backup::Policy;
use pinchora_core::admin::config_backup::Preview;
use pinchora_core::admin::config_backup::SECTIONS;
use pinchora_core::infra::i18n::Lang;
use pinchora_core::t;
use serde_json::Value;
use twilight_model::id::Id;

pub async fn backup_names(app: &App, guild: u64) -> anyhow::Result<Names> {
    let roles = app.http.roles(Id::new(guild)).await?.models().await?;
    let channels = app.http.guild_channels(Id::new(guild)).await?.models().await?;
    Ok(Names {
        roles: roles.into_iter().map(|r| (r.id.to_string(), r.name)).collect(),
        channels: channels
            .into_iter()
            .filter_map(|c| Some((c.id.to_string(), c.name?)))
            .collect(),
    })
}
/// Expensive backup operations share a guild-wide budget across Discord and the dashboard.
pub async fn backup_rate_limit(app: &App, guild: u64, operation: &str, limit: i64, lang: Lang) -> anyhow::Result<()> {
    let hits = app
        .cache
        .incr(
            &format!("backup:{guild}:{operation}"),
            std::time::Duration::from_secs(60),
        )
        .await?;
    anyhow::ensure!(hits <= limit, "{}", t!(lang, "config_backup.rate_limited"));
    Ok(())
}

pub async fn backup_preview(
    app: &App,
    guild: u64,
    request: &Import,
    lang: Lang,
    actor: u64,
) -> anyhow::Result<(Backup, Preview)> {
    backup_rate_limit(app, guild, "preview", 10, lang).await?;
    config_backup::validate_shape(&request.backup, lang).map_err(anyhow::Error::msg)?;
    let names = backup_names(app, guild).await?;
    let before = app.db.snapshot_config(guild, names.clone()).await?;
    let mut backup = request.backup.clone();
    crate::infra::routing::normalize_backup(&mut backup)?;
    let unmapped: Vec<String> = config_backup::remap(&mut backup, guild, &names)
        .into_iter()
        .map(|entry| {
            let (kind, detail) = entry.split_once(':').unwrap_or(("channel", &entry));
            format!("{}: {detail}", t!(lang, &format!("command_control.ui.{kind}")))
        })
        .collect();
    backup.names = names.clone();
    config_backup::align_serial_ids(&before, &mut backup, request.backup.source_guild == guild.to_string());
    // Upload ids are global. Allocate a deterministic, guild-scoped copy and rewrite design references.
    if request.backup.source_guild != guild.to_string() {
        use sha2::{Digest, Sha256};
        let uploads: Vec<(String, String)> = backup.sections["card_uploads"]
            .iter()
            .filter_map(|r| {
                let old = r["id"].as_str()?.to_owned();
                let digest = Sha256::digest(format!("{guild}:{old}"));
                let new: String = digest[..16].iter().map(|b| format!("{b:02x}")).collect();
                Some((old, new))
            })
            .collect();
        for (old, new) in uploads {
            for section in ["card_uploads", "card_designs", "guild_branding"] {
                for row in backup.sections.get_mut(section).unwrap() {
                    replace_string(row, &old, &new);
                }
            }
        }
    }
    if request.policy == Policy::Merge
        && let (Some(old), Some(incoming)) = (
            before.sections["command_control"].first(),
            backup
                .sections
                .get_mut("command_control")
                .and_then(|rows| rows.first_mut()),
        )
    {
        let mut merged: pinchora_core::admin::command_control::Config = serde_json::from_value(old["config"].clone())?;
        let incoming_config: pinchora_core::admin::command_control::Config =
            serde_json::from_value(incoming["config"].clone())?;
        merged.rules.extend(incoming_config.rules);
        merged.aliases.extend(incoming_config.aliases);
        incoming["config"] = serde_json::to_value(merged)?;
    }
    let mut effective = backup.clone();
    if request.policy == Policy::Merge {
        for section in SECTIONS {
            let rows = effective.sections.get_mut(section.table).unwrap();
            for row in &before.sections[section.table] {
                if !rows
                    .iter()
                    .any(|r| config_backup::row_key(section, r) == config_backup::row_key(section, row))
                {
                    rows.push(row.clone());
                }
            }
        }
    }
    let mut errors = vec![];
    for (role, id) in config_backup::references(&backup) {
        if !(if role { &names.roles } else { &names.channels }).contains_key(&id) {
            errors.push(format!("{}: {id}", t!(lang, "config_backup.invalid")));
        }
    }
    if let Err(error) = validate_backup(&effective, lang) {
        errors.push(error);
    }
    if errors.is_empty()
        && let Err(error) = app.db.validate_backup_types(&backup).await
    {
        tracing::debug!(%error, "invalid backup field types");
        errors.push(t!(lang, "config_backup.invalid"));
    }
    if errors.is_empty()
        && unmapped.is_empty()
        && let Err(error) = check_grants(app, guild, actor, &effective, lang).await
    {
        errors.push(error.to_string());
    }
    let differences = config_backup::diff(&before, &backup, request.policy);
    let can_import = unmapped.is_empty() && errors.is_empty();
    Ok((
        backup,
        Preview {
            differences,
            unmapped,
            errors,
            can_import,
        },
    ))
}
fn replace_string(value: &mut Value, old: &str, new: &str) {
    match value {
        Value::String(s) if s == old => *s = new.into(),
        Value::Object(map) => {
            for v in map.values_mut() {
                replace_string(v, old, new);
            }
        }
        Value::Array(values) => {
            for v in values {
                replace_string(v, old, new);
            }
        }
        _ => {}
    }
}
pub async fn backup_apply(app: &App, guild: u64, request: &Import, lang: Lang, actor: u64) -> anyhow::Result<i64> {
    backup_rate_limit(app, guild, "apply", 5, lang).await?;
    let (backup, preview) = backup_preview(app, guild, request, lang, actor).await?;
    anyhow::ensure!(preview.can_import, "{}", t!(lang, "config_backup.blocked"));
    let id = app
        .db
        .import_config(guild, &backup, request.policy, backup.names.clone())
        .await?;
    app.announce_guild_change(guild).await;
    app.announce_xp_change(guild).await;
    Ok(id)
}
