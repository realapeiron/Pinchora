//! MEE6 leaderboard import, shared by Discord and the dashboard.

use std::{collections::BTreeMap, future::Future, time::Duration};

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
    // Sum 5*l*l + 50*l + 100 in constant time, even for hostile remote levels.
    let n = u64::from(player.level.min(10_000));
    let previous = n.saturating_sub(1);
    let completed = 5 * previous * n * (2 * n).saturating_sub(1) / 6 + 25 * previous * n + 100 * n;
    completed.saturating_add(current.max(0) as u64).min(i64::MAX as u64) as i64
}

/// Import attempts are audited in Postgres, so restarts and partial failures keep the daily limit.
pub async fn mee6_resting(app: &App, guild: u64) -> anyhow::Result<Option<Duration>> {
    let Some(last) = app.db.last_mee6_import(guild).await? else {
        return Ok(None);
    };
    let elapsed = chrono::Utc::now().signed_duration_since(last);
    let remaining = chrono::Duration::hours(24) - elapsed;
    Ok(remaining.to_std().ok().filter(|left| !left.is_zero()))
}

/// Fetch only the fixed MEE6 endpoint: no redirects, ambient proxies, or unbounded bodies.
pub async fn import_mee6(
    app: &App,
    request: ImportRequest<'_>,
    progress: impl FnMut(ImportProgress),
) -> anyhow::Result<ImportProgress> {
    // Integration suites must inject their local fetcher, including when exercising failures.
    anyhow::ensure!(
        std::env::var_os("PINCHORA_TEST_DATABASE_URL").is_none(),
        "live MEE6 disabled in tests"
    );
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .no_proxy()
        .connect_timeout(Duration::from_secs(5))
        .timeout(Duration::from_secs(15))
        .build()?;
    let guild = request.guild;
    import_mee6_with_fetch(app, request, progress, |page| {
        let client = client.clone();
        async move {
            let url = format!("https://mee6.xyz/api/plugins/levels/leaderboard/{guild}?page={page}&limit=1000");
            let mut response = client.get(url).send().await.context("MEE6 request failed")?;
            anyhow::ensure!(response.status().is_success(), "MEE6 returned {}", response.status());
            let mut bytes = Vec::new();
            while let Some(chunk) = response.chunk().await? {
                anyhow::ensure!(bytes.len() + chunk.len() <= MAX_PAGE_BYTES, "MEE6 response too large");
                bytes.extend_from_slice(&chunk);
            }
            Ok(bytes)
        }
    })
    .await
}

const MAX_PAGE_BYTES: usize = 2 * 1024 * 1024;

/// Shared import engine; tests inject an offline transport, never an arbitrary production URL.
pub async fn import_mee6_with_fetch<F, Fut>(
    app: &App,
    request: ImportRequest<'_>,
    mut progress: impl FnMut(ImportProgress),
    mut fetch: F,
) -> anyhow::Result<ImportProgress>
where
    F: FnMut(usize) -> Fut,
    Fut: Future<Output = anyhow::Result<Vec<u8>>>,
{
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
    app.db
        .audit(&NewAudit {
            guild,
            actor,
            actor_name,
            source,
            action: "xp.import.mee6.started",
            target: None,
            before: None,
            after: Some(&serde_json::json!({"mode": mode.key()}).to_string()),
        })
        .await?;
    let mut report = ImportProgress::default();
    let result = tokio::time::timeout(Duration::from_secs(600), async {
        for page in 0..PAGE_LIMIT {
            let bytes = tokio::time::timeout(Duration::from_secs(20), fetch(page)).await??;
            anyhow::ensure!(bytes.len() <= MAX_PAGE_BYTES, "MEE6 response too large");
            let body: Leaderboard = serde_json::from_slice(&bytes)?;
            anyhow::ensure!(body.players.len() <= 1000, "MEE6 page exceeds 1000 players");
            let count = body.players.len();
            let mut unique = BTreeMap::new();
            for player in &body.players {
                let id = player.id.parse::<u64>().context("invalid MEE6 member id")?;
                anyhow::ensure!(id > 0 && id <= i64::MAX as u64, "invalid MEE6 member id");
                unique
                    .entry(id)
                    .and_modify(|xp: &mut i64| *xp = (*xp).max(mapped_xp(player)))
                    .or_insert(mapped_xp(player));
            }
            let rows: Vec<_> = unique.into_iter().collect();
            report.updated += app
                .db
                .import_member_xp_page(guild, &rows, matches!(mode, ImportMode::Replace))
                .await? as usize;
            report.checked += rows.len();
            progress(report);
            let users: Vec<_> = rows.iter().map(|(user, _)| *user).collect();
            sync_members_roles(app, guild, &users).await?;
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
    })
    .await
    .context("MEE6 import timed out")
    .and_then(|result| result);
    if result.is_err() {
        app.db
            .audit(&NewAudit {
                guild,
                actor,
                actor_name,
                source,
                action: "xp.import.mee6.failed",
                target: None,
                before: None,
                after: Some(
                    &serde_json::json!({"mode": mode.key(), "checked": report.checked, "updated": report.updated})
                        .to_string(),
                ),
            })
            .await?;
    }
    result
}

/// A failed Discord update must not prevent the rest of a changed batch being reconciled.
pub(crate) async fn sync_members_roles(app: &App, guild: u64, users: &[u64]) -> anyhow::Result<()> {
    let mut first_error = None;
    for &user in users {
        if let Err(error) = sync_member_roles(app, guild, user).await {
            tracing::warn!(guild, user, %error, "XP role sync failed");
            first_error.get_or_insert(error);
        }
    }
    first_error.map_or(Ok(()), Err)
}

/// Reconcile all configured level rewards, including rewards above a newly lowered level.
pub async fn sync_member_roles(app: &App, guild: u64, user: u64) -> anyhow::Result<()> {
    use twilight_model::id::Id;
    let rewards = crate::roles::level_rewards(app, guild).await?;
    if rewards.is_empty() {
        return Ok(());
    }
    let member = match app.http.guild_member(Id::new(guild), Id::new(user)).await {
        Ok(response) => response.model().await?,
        Err(error) if crate::roles::member_is_gone(&error) => return Ok(()),
        Err(error) => return Err(error.into()),
    };
    let xp = app.db.member_rank(guild, user).await?.map_or(0, |r| r.xp);
    let level = leveling::level_of(xp);
    let stack = app.db.guild_settings(guild).await?.level_roles_stack;
    let held: Vec<_> = member.roles.iter().map(|r| r.get()).collect();
    let (wanted, _) = leveling::role_changes(&rewards, level, stack, &[]);
    for role in rewards
        .iter()
        .map(|r| r.role_id)
        .collect::<std::collections::BTreeSet<_>>()
    {
        if held.contains(&role) && !wanted.contains(&role) {
            app.http
                .remove_guild_member_role(Id::new(guild), Id::new(user), Id::new(role))
                .await?;
        } else if !held.contains(&role) && wanted.contains(&role) {
            app.http
                .add_guild_member_role(Id::new(guild), Id::new(user), Id::new(role))
                .await?;
        }
    }
    Ok(())
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
