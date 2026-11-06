//! MEE6 leaderboard import, shared by Discord and the dashboard.

use std::time::Duration;

use anyhow::Context;
use pinchora_core::leveling;
use pinchora_db::{AuditSource, NewAudit};
use serde::Deserialize;

use crate::App;

const DAY: Duration = Duration::from_secs(24 * 60 * 60);
const PAGE_LIMIT: usize = 100;

#[derive(Debug, Clone, Copy, Default, serde::Serialize)]
pub struct ImportProgress {
    pub checked: usize,
    pub updated: usize,
    pub total: usize,
}

#[derive(Debug, Clone, Copy)]
pub enum ImportMode {
    Merge,
    Replace,
}

/// Who requested an import and how imported XP is applied.
pub struct ImportRequest<'a> {
    pub guild: u64,
    pub actor: u64,
    pub actor_name: &'a str,
    pub source: AuditSource,
    pub mode: ImportMode,
}

impl ImportMode {
    pub fn key(self) -> &'static str {
        match self {
            Self::Merge => "merge",
            Self::Replace => "replace",
        }
    }
    pub fn parse(key: &str) -> Option<Self> {
        match key {
            "merge" => Some(Self::Merge),
            "replace" => Some(Self::Replace),
            _ => None,
        }
    }
}

#[derive(Deserialize)]
struct Leaderboard {
    players: Vec<Mee6Player>,
}

#[derive(Deserialize)]
struct Mee6Player {
    id: String,
    xp: i64,
    level: u32,
    #[serde(default)]
    detailed_xp: Vec<i64>,
}

/// MEE6's current-level XP is mapped through Pinchora's level table. Its total XP is a fallback.
fn mapped_xp(player: &Mee6Player) -> i64 {
    let Some(current) = player.detailed_xp.first().copied() else {
        return player.xp.max(0);
    };
    let completed: u64 = (0..player.level.min(10_000)).map(leveling::xp_to_next).sum();
    completed.saturating_add(current.max(0) as u64).min(i64::MAX as u64) as i64
}

/// The last successful import is audited in Postgres, so restarts do not reset the daily limit.
pub async fn mee6_resting(app: &App, guild: u64) -> anyhow::Result<Option<Duration>> {
    let Some(last) = app.db.last_mee6_import(guild).await? else {
        return Ok(None);
    };
    let elapsed = chrono::Utc::now().signed_duration_since(last);
    let remaining = chrono::Duration::hours(24) - elapsed;
    Ok(remaining.to_std().ok().filter(|left| !left.is_zero()))
}

/// Import leaderboard pages. `base` is injectable so tests only reach a local mock server.
pub async fn import_mee6(
    app: &App,
    base: &str,
    request: ImportRequest<'_>,
    mut progress: impl FnMut(ImportProgress),
) -> anyhow::Result<ImportProgress> {
    let ImportRequest {
        guild,
        actor,
        actor_name,
        source,
        mode,
    } = request;
    anyhow::ensure!(
        mee6_resting(app, guild).await?.is_none(),
        "A MEE6 import already finished today."
    );
    let key = format!("xp:mee6:{guild}");
    anyhow::ensure!(
        app.cache.try_cooldown(&key, DAY).await?.is_none(),
        "A MEE6 import is already running or finished today."
    );
    let result = async {
        let mut report = ImportProgress::default();
        for page in 0..PAGE_LIMIT {
            let url = format!("{base}/{guild}?page={page}&limit=1000");
            let response = app.web.get(url).send().await.context("MEE6 request failed")?;
            anyhow::ensure!(response.status().is_success(), "MEE6 returned {}", response.status());
            let body: Leaderboard = serde_json::from_str(&response.text().await?)?;
            anyhow::ensure!(body.players.len() <= 1000, "MEE6 page exceeds 1000 players");
            let count = body.players.len();
            let rows: Vec<(u64, i64)> = body
                .players
                .iter()
                .filter_map(|p| {
                    p.id.parse::<u64>()
                        .ok()
                        .filter(|id| *id > 0)
                        .map(|id| (id, mapped_xp(p)))
                })
                .collect();
            report.updated += app
                .db
                .import_member_xp_page(guild, &rows, matches!(mode, ImportMode::Replace))
                .await? as usize;
            report.checked += rows.len();
            progress(report);
            if count < 1000 {
                break;
            }
            anyhow::ensure!(
                page + 1 < PAGE_LIMIT,
                "MEE6 leaderboard exceeds the 100-page safety limit"
            );
        }
        report.total = report.checked;
        let after =
            serde_json::json!({ "mode": mode.key(), "checked": report.checked, "updated": report.updated }).to_string();
        app.db
            .audit(&NewAudit {
                guild,
                actor,
                actor_name,
                source,
                action: "xp.import.mee6",
                target: None,
                before: None,
                after: Some(&after),
            })
            .await?;
        Ok(report)
    }
    .await;
    if result.is_err() {
        app.cache.release_cooldown(&key).await?;
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn maps_level_progress_and_fallback() {
        let player = Mee6Player {
            id: "1".into(),
            xp: 999,
            level: 2,
            detailed_xp: vec![55, 220],
        };
        assert_eq!(mapped_xp(&player), 100 + 155 + 55);
        let fallback = Mee6Player {
            detailed_xp: vec![],
            ..player
        };
        assert_eq!(mapped_xp(&fallback), 999);
    }
}
