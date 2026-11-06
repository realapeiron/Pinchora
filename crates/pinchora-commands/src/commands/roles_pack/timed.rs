//! Roles on a timer: auto roles that wait for their delay, and roles that are taken away again after a
//! while (a panel's expiring role, an auto role's `remove_after`). One ticker serves both.

use std::{sync::Arc, time::Duration};

use pinchora_core::{modules::Module, roles_pack::STICKY_KEPT_DAYS};

use super::give::{self, is_final};
use crate::App;

/// How often the ticker looks for what is due.
const WAKE: Duration = Duration::from_secs(30);
/// Roles handled in one batch, and batches in one pass: a server's worth of joins at once still finishes.
const BATCH: i64 = 50;
const BATCHES: usize = 10;
/// Passes between clearing out the roles kept for members who left long ago: an hour.
const PRUNE_EVERY: u32 = 120;

/// One pass: gives the roles whose delay is over, then takes the roles whose time is up. Returns how many
/// roles changed hands. A change Discord refuses for good (the member left, the role is gone, the bot may
/// not) is dropped; one that failed for want of an answer is tried again in a few minutes.
pub async fn tick(app: &Arc<App>) -> anyhow::Result<usize> {
    let now = app.now();
    let mut changed = 0;
    for _ in 0..BATCHES {
        let due = app.db.claim_due_grants(now, BATCH).await?;
        let full = due.len() as i64 == BATCH;
        for row in due {
            let (guild, user, role) = (row.guild_id as u64, row.user_id as u64, row.role_id as u64);
            if !app.module_allowed(guild, None, Module::AutoRoles).await
                || !app
                    .db
                    .delayed_auto_role_active(guild, role, row.remove_after_secs)
                    .await?
            {
                app.db.finish_grant(row.id).await?;
                continue;
            }
            match give::give_timed(app, guild, user, role, row.remove_after_secs, "Auto role").await {
                Ok(()) => changed += 1,
                Err(error) if give::is_final_grant(&error) => {
                    tracing::debug!(guild, user, role, %error, "a waiting auto role was dropped");
                }
                Err(error) => {
                    tracing::warn!(guild, user, role, %error, "a waiting auto role will be tried again");
                    continue;
                }
            }
            if let Err(error) = app.db.finish_grant(row.id).await {
                tracing::warn!(guild, user, role, %error, "could not finish a waiting auto role");
            }
        }
        if !full {
            break;
        }
    }
    for _ in 0..BATCHES {
        let due = app.db.claim_due_removals(now, BATCH).await?;
        let full = due.len() as i64 == BATCH;
        for row in due {
            let (guild, user, role) = (row.guild_id as u64, row.user_id as u64, row.role_id as u64);
            let _lock = app.db.lock_role_member(guild, user).await?;
            if !app.db.removal_current(&row).await? {
                continue;
            }
            match give::take(app, guild, user, role, "Role expired").await {
                Ok(()) => changed += 1,
                Err(error) if is_final(&error) => {
                    tracing::debug!(guild, user, role, %error, "an expiring role was dropped");
                    if crate::roles::role_is_gone(&error) {
                        give::forget(app, guild, role).await;
                    }
                }
                Err(error) => {
                    tracing::warn!(guild, user, role, %error, "an expiring role will be tried again");
                    continue;
                }
            }
            if let Err(error) = app.db.finish_removal(&row).await {
                tracing::warn!(guild, user, role, %error, "could not finish an expiring role");
            }
        }
        if !full {
            break;
        }
    }
    metrics::counter!("pinchora_timed_roles_total").increment(changed as u64);
    Ok(changed)
}

/// Runs [`tick`] every 30 seconds for the life of the process, and forgets the roles kept for members who
/// left more than [`STICKY_KEPT_DAYS`] ago once an hour. Every pod runs it; the leases keep them apart.
pub fn spawn_ticker(app: Arc<App>) {
    tokio::spawn(async move {
        let mut wake = tokio::time::interval(WAKE);
        wake.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        let mut rounds = 0u32;
        loop {
            wake.tick().await;
            if let Err(error) = tick(&app).await {
                tracing::warn!(%error, "timed roles tick failed");
            }
            rounds = rounds.wrapping_add(1);
            if rounds.is_multiple_of(PRUNE_EVERY) {
                let before = app.now() - chrono::Duration::days(STICKY_KEPT_DAYS);
                if let Err(error) = app.db.prune_sticky(before).await {
                    tracing::warn!(%error, "old sticky roles not forgotten");
                }
            }
        }
    });
}
