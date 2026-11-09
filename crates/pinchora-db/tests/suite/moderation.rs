//! Runs against a real PostgreSQL when `PINCHORA_TEST_DATABASE_URL` is set; skipped otherwise.
//!
//! Moderation cases (numbered per server, also when moderators act at once), warns that count, the
//! warn ladder, temporary bans the ticker lifts, the staff log and the server log's settings.

use chrono::{Duration, Utc};
use pinchora_core::admin::logging::Group;
use pinchora_core::admin::logging::LogCategory;
use pinchora_core::admin::logging::LogEvent;
use pinchora_core::moderation::moderation::CaseKind;
use pinchora_core::moderation::moderation::Punishment;
use pinchora_core::moderation::moderation::Threshold;
use pinchora_core::moderation::moderation::check_threshold;
use pinchora_db::{CaseFilter, Db, NewCase, NewStaffAction, StaffFilter};

async fn db() -> Option<Db> {
    let url = std::env::var("PINCHORA_TEST_DATABASE_URL").ok()?;
    let db = Db::connect_waiting(&url, 12, std::time::Duration::from_secs(60))
        .await
        .expect("connect");
    db.migrate().await.expect("migrate");
    Some(db)
}

/// Ids unique per run, so the suite can run repeatedly against one database.
use crate::support::spaced_ids as ids;

fn case(guild: u64, kind: CaseKind, target: u64) -> NewCase<'static> {
    NewCase {
        guild,
        kind,
        target,
        target_name: "Mia",
        moderator: 9,
        moderator_name: "Mod",
        reason: Some("rule 1"),
        duration: None,
        expires_at: None,
        details: None,
    }
}

#[tokio::test]
async fn cases_are_numbered_one_after_another_when_moderators_act_at_once() {
    let _isolation = crate::support::shared().await;
    let Some(db) = db().await else { return };
    let [guild, other, target] = ids(7);
    let mut tasks = tokio::task::JoinSet::new();
    for n in 0..20u64 {
        let db = db.clone();
        tasks.spawn(async move {
            let kind = if n % 2 == 0 { CaseKind::Warn } else { CaseKind::Kick };
            db.open_case(&case(guild, kind, target + n % 3))
                .await
                .unwrap()
                .case
                .number
        });
    }
    let mut numbers = Vec::new();
    while let Some(number) = tasks.join_next().await {
        numbers.push(number.unwrap());
    }
    numbers.sort_unstable();
    assert_eq!(numbers, (1..=20).collect::<Vec<i32>>(), "no number twice, none skipped");
    // Another server counts on its own.
    let first = db.open_case(&case(other, CaseKind::Warn, target)).await.unwrap();
    assert_eq!(first.case.number, 1);
    // And every case reached the staff log with its number.
    let log = db
        .staff_actions(
            guild,
            &StaffFilter {
                limit: 100,
                ..StaffFilter::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(log.iter().filter(|l| l.case_number.is_some()).count(), 20);
}

#[tokio::test]
async fn warns_that_race_each_other_each_see_the_count_they_made() {
    let _isolation = crate::support::shared().await;
    let Some(db) = db().await else { return };
    let [guild, target] = ids(7);
    let mut tasks = tokio::task::JoinSet::new();
    for _ in 0..8 {
        let db = db.clone();
        tasks.spawn(async move {
            db.open_case(&case(guild, CaseKind::Warn, target))
                .await
                .unwrap()
                .active_warns
        });
    }
    let mut counts = Vec::new();
    while let Some(count) = tasks.join_next().await {
        counts.push(count.unwrap());
    }
    counts.sort_unstable();
    // Each count is seen exactly once, so a ladder rung at any of them fires once.
    assert_eq!(counts, (1..=8).collect::<Vec<i64>>());
}

#[tokio::test]
async fn a_warn_taken_back_stops_counting_and_leaves_a_case() {
    let _isolation = crate::support::shared().await;
    let Some(db) = db().await else { return };
    let [guild, target] = ids(7);
    let first = db.open_case(&case(guild, CaseKind::Warn, target)).await.unwrap();
    db.open_case(&case(guild, CaseKind::Warn, target)).await.unwrap();
    assert_eq!(db.active_warns(guild, target).await.unwrap().len(), 2);
    let unwarn = NewCase {
        target: 0,
        target_name: "",
        details: Some("case #1"),
        ..case(guild, CaseKind::Unwarn, 0)
    };
    let (warn, opened) = db
        .remove_warn(first.case.number, &unwarn)
        .await
        .unwrap()
        .expect("a warn that counts");
    assert!(!warn.active);
    assert_eq!(opened.case.kind, "unwarn");
    assert_eq!(
        opened.case.target_id, target as i64,
        "the unwarn is about the warned member"
    );
    assert_eq!(opened.active_warns, 1);
    assert!(
        db.remove_warn(first.case.number, &unwarn).await.unwrap().is_none(),
        "only once"
    );
    assert!(
        db.remove_warn(opened.case.number, &unwarn).await.unwrap().is_none(),
        "not a warn"
    );
    assert_eq!(db.active_warns(guild, target).await.unwrap().len(), 1);
}

#[tokio::test]
async fn temporary_bans_are_lifted_once_and_a_new_ban_replaces_the_old_end() {
    let _isolation = crate::support::exclusive().await;
    let Some(db) = db().await else { return };
    // The exclusive guard makes old fixtures safe to clear before the bounded worker scan.
    sqlx::query("DELETE FROM mod_cases WHERE kind = 'ban' AND expires_at IS NOT NULL")
        .execute(db.pool())
        .await
        .unwrap();
    let [guild, early, late, rebanned] = ids(7);
    let now = Utc::now();
    let temp = |target, ends| NewCase {
        duration: Some(Duration::hours(1)),
        expires_at: Some(ends),
        ..case(guild, CaseKind::Ban, target)
    };
    let early_ban = db
        .open_case(&temp(early, now - Duration::minutes(1)))
        .await
        .unwrap()
        .case;
    db.open_case(&temp(late, now + Duration::hours(1))).await.unwrap();
    db.open_case(&temp(rebanned, now - Duration::minutes(2))).await.unwrap();
    // Banned again for good before the first ban ran out: nothing to lift.
    db.open_case(&case(guild, CaseKind::Ban, rebanned)).await.unwrap();

    let claimed = db.claim_expired_bans(now, 100).await.unwrap();
    let ours: Vec<_> = claimed.iter().filter(|c| c.guild_id == guild as i64).collect();
    assert_eq!(ours.len(), 1, "{ours:?}");
    assert_eq!(ours[0].number, early_ban.number);
    // Leased: a second pod looking now finds nothing of this server.
    let again = db.claim_expired_bans(now, 100).await.unwrap();
    assert!(again.iter().all(|c| c.guild_id != guild as i64));
    // Lifting it is an unban case, which ends the ban.
    db.open_case(&case(guild, CaseKind::Unban, early)).await.unwrap();
    let ban = db.case(guild, early_ban.number).await.unwrap().unwrap();
    assert!(!ban.active);
    // Even once the lease has run out.
    let later = db.claim_expired_bans(now + Duration::hours(3), 100).await.unwrap();
    let ours: Vec<i64> = later
        .iter()
        .filter(|c| c.guild_id == guild as i64)
        .map(|c| c.target_id)
        .collect();
    assert_eq!(ours, [late as i64], "only the ban whose time came since");
    // One Discord would not lift waits an hour.
    db.postpone_lift(later.iter().find(|c| c.target_id == late as i64).unwrap())
        .await
        .unwrap();
    let soon = db.claim_expired_bans(now + Duration::hours(3), 100).await.unwrap();
    assert!(soon.iter().all(|c| c.guild_id != guild as i64));
}

/// The ticker takes a ban off right before it asks Discord, so a ban lifted (or replaced) after the claim
/// is left alone and none is lifted twice; one Discord refused stands again unless a newer ban came; and a
/// ban lifted by hand ends only the bans opened before it.
#[tokio::test]
async fn the_ticker_rechecks_each_ban_and_a_hand_unban_ends_the_older_ones() {
    let _isolation = crate::support::exclusive().await;
    let Some(db) = db().await else { return };
    // The exclusive guard makes old fixtures safe to clear before the bounded worker scan.
    sqlx::query("DELETE FROM mod_cases WHERE kind = 'ban' AND expires_at IS NOT NULL")
        .execute(db.pool())
        .await
        .unwrap();
    let [guild, mia, ana] = ids(7);
    let now = Utc::now();
    let temp = |target| NewCase {
        duration: Some(Duration::minutes(1)),
        expires_at: Some(now - Duration::seconds(1)),
        ..case(guild, CaseKind::Ban, target)
    };
    let mia_ban = db.open_case(&temp(mia)).await.unwrap().case;
    let ana_ban = db.open_case(&temp(ana)).await.unwrap().case;
    let claimed = db.claim_expired_bans(now, 100).await.unwrap();
    assert_eq!(claimed.iter().filter(|c| c.guild_id == guild as i64).count(), 2);

    // Mia was unbanned by hand after the claim: nothing left for the ticker to lift.
    assert_eq!(db.end_bans(guild, mia, Utc::now()).await.unwrap(), 1);
    assert!(!db.close_ban(&mia_ban).await.unwrap());
    // Ana's is taken off once, never twice.
    assert!(db.close_ban(&ana_ban).await.unwrap());
    assert!(!db.close_ban(&ana_ban).await.unwrap());
    // Discord refused to lift it: it stands again, leased for an hour.
    db.postpone_lift(&ana_ban).await.unwrap();
    assert!(db.case(guild, ana_ban.number).await.unwrap().unwrap().active);
    let soon = db.claim_expired_bans(now + Duration::minutes(5), 100).await.unwrap();
    assert!(soon.iter().all(|c| c.guild_id != guild as i64));
    // Refused again, but banned for good meanwhile: the newer ban decides.
    assert!(db.close_ban(&ana_ban).await.unwrap());
    let newer = db.open_case(&case(guild, CaseKind::Ban, ana)).await.unwrap().case;
    db.postpone_lift(&ana_ban).await.unwrap();
    assert!(!db.case(guild, ana_ban.number).await.unwrap().unwrap().active);
    // An unban by hand from before the newer ban leaves it standing; one after it ends it.
    let before = newer.created_at - Duration::milliseconds(1);
    assert_eq!(db.end_bans(guild, ana, before).await.unwrap(), 0);
    assert!(db.case(guild, newer.number).await.unwrap().unwrap().active);
    assert_eq!(db.end_bans(guild, ana, Utc::now()).await.unwrap(), 1);
}

/// What a lock took is remembered on its case and read back by the next unlock, until an unlock comes.
#[tokio::test]
async fn a_lock_remembers_what_it_took() {
    let _isolation = crate::support::shared().await;
    let Some(db) = db().await else { return };
    let [guild, channel] = ids(7);
    assert_eq!(db.lock_state(guild, channel).await.unwrap(), None, "never locked here");
    let lock = db.open_case(&case(guild, CaseKind::Lock, channel)).await.unwrap().case;
    assert_eq!(
        db.lock_state(guild, channel).await.unwrap(),
        None,
        "nothing remembered yet"
    );
    db.set_lock_state(guild, lock.number, 2048, 1 << 35).await.unwrap();
    assert_eq!(db.lock_state(guild, channel).await.unwrap(), Some((2048, 1 << 35)));
    db.open_case(&case(guild, CaseKind::Unlock, channel)).await.unwrap();
    assert_eq!(db.lock_state(guild, channel).await.unwrap(), None, "unlocked since");
}

/// Servers that logged before the detailed log keep their four old groups and nothing newer: the
/// migration switches off exactly the events no older switch covers.
#[test]
fn servers_that_logged_before_keep_only_the_old_four_on() {
    let sql = include_str!("../../../../migrations/0044_moderation.sql");
    let list = sql
        .split("SELECT id, ARRAY[")
        .nth(1)
        .and_then(|rest| rest.split(']').next())
        .expect("the backfill's list");
    let off: Vec<&str> = list
        .split(',')
        .map(|key| key.trim().trim_matches('\''))
        .filter(|key| !key.is_empty())
        .collect();
    let newer: Vec<&str> = LogEvent::ALL
        .into_iter()
        .filter(|e| e.group().is_none())
        // Migration 0044 is immutable and predates the opt-in moderation workflows.
        .filter(|e| {
            !matches!(
                e,
                LogEvent::Report | LogEvent::Appeal | LogEvent::Restriction | LogEvent::RoleRule | LogEvent::AutoPurge
            )
        })
        .map(LogEvent::key)
        .collect();
    assert_eq!(off, newer);
}

#[tokio::test]
async fn the_warn_ladder_holds_ten_rungs_one_per_count() {
    let _isolation = crate::support::shared().await;
    let Some(db) = db().await else { return };
    let [guild] = ids(7);
    let rung = |n, action, d: Option<Duration>| check_threshold(n, action, d).unwrap();
    db.put_warn_threshold(guild, rung(3, "timeout", Some(Duration::hours(1))))
        .await
        .unwrap()
        .unwrap();
    let ladder = db
        .put_warn_threshold(guild, rung(3, "kick", None))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        ladder,
        [Threshold {
            warns: 3,
            action: Punishment::Kick,
            duration: None
        }],
        "the same count again replaces it"
    );
    for n in 4..=12 {
        db.put_warn_threshold(guild, rung(n, "kick", None))
            .await
            .unwrap()
            .unwrap();
    }
    assert!(
        db.put_warn_threshold(guild, rung(13, "kick", None))
            .await
            .unwrap()
            .is_err(),
        "full"
    );
    assert!(
        db.put_warn_threshold(guild, rung(12, "ban", None))
            .await
            .unwrap()
            .is_ok(),
        "replacing still works"
    );
    assert!(db.delete_warn_threshold(guild, 12).await.unwrap());
    assert!(!db.delete_warn_threshold(guild, 12).await.unwrap());
    let ladder = db
        .replace_warn_thresholds(guild, &[rung(2, "ban", Some(Duration::days(1)))])
        .await
        .unwrap();
    assert_eq!(ladder.len(), 1);
    assert_eq!(ladder[0].duration, Some(Duration::days(1)));
}

#[tokio::test]
async fn cases_are_found_by_member_kind_and_words_and_reasons_change() {
    let _isolation = crate::support::shared().await;
    let Some(db) = db().await else { return };
    let [guild, mia, zed] = ids(7);
    db.open_case(&case(guild, CaseKind::Warn, mia)).await.unwrap();
    let kick = NewCase {
        target_name: "Zed_100%",
        reason: Some("spam links"),
        ..case(guild, CaseKind::Kick, zed)
    };
    let kicked = db.open_case(&kick).await.unwrap().case;
    let find = |filter: CaseFilter| {
        let db = db.clone();
        async move { db.cases(guild, &CaseFilter { limit: 50, ..filter }).await.unwrap() }
    };
    assert_eq!(find(CaseFilter::default()).await.len(), 2);
    assert_eq!(
        find(CaseFilter {
            target: Some(mia),
            ..CaseFilter::default()
        })
        .await
        .len(),
        1
    );
    assert_eq!(
        find(CaseFilter {
            kind: Some(CaseKind::Kick),
            ..CaseFilter::default()
        })
        .await
        .len(),
        1
    );
    assert_eq!(
        find(CaseFilter {
            query: Some("SPAM".into()),
            ..CaseFilter::default()
        })
        .await
        .len(),
        1
    );
    assert_eq!(
        find(CaseFilter {
            query: Some("100%".into()),
            ..CaseFilter::default()
        })
        .await
        .len(),
        1
    );
    assert_eq!(
        find(CaseFilter {
            query: Some("_".into()),
            ..CaseFilter::default()
        })
        .await
        .len(),
        1,
        "a literal underscore"
    );
    assert_eq!(
        find(CaseFilter {
            query: Some(zed.to_string()),
            ..CaseFilter::default()
        })
        .await
        .len(),
        1
    );
    assert_eq!(
        find(CaseFilter {
            active_only: true,
            ..CaseFilter::default()
        })
        .await
        .len(),
        1,
        "the warn"
    );
    assert_eq!(
        find(CaseFilter {
            before: Some(kicked.number),
            ..CaseFilter::default()
        })
        .await
        .len(),
        1
    );

    let edited = db
        .set_case_reason(guild, kicked.number, Some("raid"), 11, "Lead", "web")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(edited.reason.as_deref(), Some("raid"));
    assert!(edited.updated_at.is_some());
    assert!(
        db.set_case_reason(guild, 999_999, Some("x"), 11, "Lead", "web")
            .await
            .unwrap()
            .is_none()
    );
    let log = db
        .staff_actions(
            guild,
            &StaffFilter {
                query: Some("raid".into()),
                limit: 10,
                ..StaffFilter::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(log.len(), 1);
    assert_eq!((log[0].action.as_str(), log[0].actor_name.as_str()), ("reason", "Lead"));
}

#[tokio::test]
async fn the_staff_log_reads_commands_discord_and_the_dashboard_together() {
    let _isolation = crate::support::shared().await;
    let Some(db) = db().await else { return };
    let [guild, actor, target] = ids(7);
    db.open_case(&case(guild, CaseKind::Ban, target)).await.unwrap();
    db.record_staff_action(&NewStaffAction {
        guild,
        actor,
        actor_name: "Hand",
        source: "discord",
        action: "role_update",
        target: Some(5),
        target_name: Some("Helpers"),
        case_number: None,
        reason: None,
        details: Some("Name: a → b"),
    })
    .await
    .unwrap();
    db.audit(&pinchora_db::NewAudit {
        guild,
        actor,
        actor_name: "Hand",
        source: pinchora_db::AuditSource::Web,
        action: "settings.patch",
        target: None,
        before: Some("{}"),
        after: Some(r#"{"leveling_enabled":false}"#),
    })
    .await
    .unwrap();
    let all = db
        .staff_actions(
            guild,
            &StaffFilter {
                limit: 10,
                ..StaffFilter::default()
            },
        )
        .await
        .unwrap();
    let sources: Vec<&str> = all.iter().map(|a| a.source.as_str()).collect();
    assert_eq!(sources, ["web", "discord", "command"], "newest first");
    let by_hand = db
        .staff_actions(
            guild,
            &StaffFilter {
                actor: Some(actor),
                limit: 10,
                ..StaffFilter::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(by_hand.len(), 2);
    let web = db
        .staff_actions(
            guild,
            &StaffFilter {
                source: Some("web".into()),
                limit: 10,
                ..StaffFilter::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(web.len(), 1);
    assert!(web[0].key.starts_with("w:"));
    let older = db
        .staff_actions(
            guild,
            &StaffFilter {
                before: Some(all[1].created_at),
                limit: 10,
                ..StaffFilter::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(older.len(), 1);
}

#[tokio::test]
async fn log_settings_keep_the_old_switches_and_add_channels_events_and_ignores() {
    let _isolation = crate::support::shared().await;
    let Some(db) = db().await else { return };
    let [guild, channel, moderation] = ids(7);
    let fresh = db.log_config(guild).await.unwrap();
    assert!(fresh.groups.members && fresh.groups.voice);
    assert_eq!(fresh.disabled, [LogEvent::VoiceSelf], "self mutes start off");
    db.set_log_channel(guild, Some(channel)).await.unwrap();
    db.update_log_events(guild, None, Some(false), None, None)
        .await
        .unwrap();
    let config = db
        .update_log_config(guild, |c| {
            c.channels.insert(LogCategory::Moderation, moderation);
            c.toggle(LogEvent::MessageEdit, false);
            c.ignored_users.push(42);
            c.ignore_bots = true;
        })
        .await
        .unwrap();
    assert_eq!(config.default_channel, Some(channel));
    assert!(!config.groups.names, "the old switch is read");
    assert_eq!(config.channel(LogEvent::Kick), Some(moderation));
    assert_eq!(config.channel(LogEvent::MessageDelete), Some(channel));
    assert_eq!(config.channel(LogEvent::MessageEdit), None);
    assert_eq!(config.ignored_users, [42]);
    assert!(config.ignore_bots);
    // The old switch, set from the event matrix, is the same column `/config log-events` writes.
    let config = db
        .update_log_config(guild, |c| c.set_group(Group::Names, true))
        .await
        .unwrap();
    assert!(db.guild_settings(guild).await.unwrap().log_names);
    assert_eq!(config, db.log_config(guild).await.unwrap(), "stored as returned");
}

/// A stale lease and a delayed audit entry must never close a replacement case, even if privacy
/// deletion caused its case number to be reused. Two tickers can close the exact case only once.
#[tokio::test]
async fn stale_ban_identity_and_delayed_unban_cannot_close_a_replacement() {
    let _isolation = crate::support::shared().await;
    let Some(db) = db().await else { return };
    let [guild, target] = ids(7);
    let old = db.open_case(&case(guild, CaseKind::Ban, target)).await.unwrap().case;
    let at = old.created_at + Duration::milliseconds(1);
    sqlx::query("UPDATE mod_cases SET created_at = $3 WHERE guild_id = $1 AND number = $2")
        .bind(guild as i64)
        .bind(old.number)
        .bind(at + Duration::seconds(1))
        .execute(db.pool())
        .await
        .unwrap();
    assert!(
        !db.close_ban(&old).await.unwrap(),
        "the same id with a new creation time is not our ban"
    );
    db.postpone_lift(&old).await.unwrap();
    assert_eq!(db.end_bans(guild, target, at).await.unwrap(), 0);
    let replacement = db.case(guild, old.number).await.unwrap().unwrap();
    assert!(replacement.active);
    assert_eq!(
        db.end_bans(guild, target, replacement.created_at).await.unwrap(),
        0,
        "a case must be strictly older than the audit entry"
    );
    let (one, two) = tokio::join!(db.close_ban(&replacement), db.close_ban(&replacement));
    assert_ne!(one.unwrap(), two.unwrap(), "only one ticker may call Discord");
    let newest = db.open_case(&case(guild, CaseKind::Ban, target)).await.unwrap().case;
    assert!(!db.close_ban(&replacement).await.unwrap());
    assert!(db.case(guild, newest.number).await.unwrap().unwrap().active);
}

#[tokio::test]
async fn ban_http_operations_are_serialized_across_pods() {
    let _isolation = crate::support::shared().await;
    let Some(db) = db().await else { return };
    let [guild, target] = ids(7);
    let guard = db.lock_ban(guild, target).await.unwrap();
    assert!(
        tokio::time::timeout(std::time::Duration::from_millis(100), db.lock_ban(guild, target))
            .await
            .is_err()
    );
    guard.rollback().await.unwrap();
    db.lock_ban(guild, target).await.unwrap().rollback().await.unwrap();
}
