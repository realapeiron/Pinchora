//! Real-DB money, ownership, clock and concurrency boundaries.
use chrono::{Duration, Utc};
use pinchora_core::reef_loop as rules;
use pinchora_db::{Db, Hide, NewCatch, ReefAction as A, ReefOutcome as O};
async fn setup() -> Option<(Db, u64)> {
    let url = std::env::var("PINCHORA_TEST_DATABASE_URL").ok()?;
    let db = Db::connect_waiting(&url, 8, std::time::Duration::from_secs(60))
        .await
        .unwrap();
    db.migrate().await.unwrap();
    let id = crate::support::id();
    db.user(id).await.unwrap();
    db.earn_shells(id, 10_000, "test").await.unwrap();
    db.earn_pearls(id, 20).await.unwrap();
    Some((db, id))
}
async fn fish(db: &Db, user: u64) -> i64 {
    db.record_catch(
        user,
        NewCatch {
            fish_id: "toaststripe_minnow",
            size_cm: 5.,
            ratio: 1.,
            weekly: Some((user + 2, Utc::now().date_naive())),
            milestones: &[],
            known: &["toaststripe_minnow"],
        },
    )
    .await
    .unwrap();
    db.bucket(user).await.unwrap().into_iter().map(|f| f.id).max().unwrap()
}
#[tokio::test]
async fn garden_real_time_and_double_harvest_are_atomic() {
    let _isolation = crate::support::shared().await;
    let Some((db, u)) = setup().await else { return };
    let now = Utc::now();
    assert_eq!(db.reef_action(u, now, A::Plant).await.unwrap(), O::Done);
    assert_eq!(db.reef_action(u, now, A::Plant).await.unwrap(), O::Already);
    assert_eq!(
        db.reef_action(u, now + Duration::seconds(3599), A::Water)
            .await
            .unwrap(),
        O::Wait
    );
    assert_eq!(
        db.reef_action(u, now + Duration::hours(1), A::Water).await.unwrap(),
        O::Done
    );
    assert_eq!(
        db.reef_action(u, now + Duration::seconds(28799), A::Harvest)
            .await
            .unwrap(),
        O::Wait
    );
    let (a, b) = tokio::join!(
        db.reef_action(u, now + Duration::hours(8), A::Harvest),
        db.reef_action(u, now + Duration::hours(8), A::Harvest)
    );
    assert_eq!([a.unwrap(), b.unwrap()].iter().filter(|&&r| r == O::Done).count(), 1);
    assert_eq!(db.reef_state(u).await.unwrap().seaweed, 4);
    assert_eq!(db.user(u).await.unwrap().credits, 9960);
    let spent: i64 =
        sqlx::query_scalar("SELECT sum(delta)::BIGINT FROM credit_ledger WHERE user_id=$1 AND reason='reef_seed'")
            .bind(u as i64)
            .fetch_one(db.pool())
            .await
            .unwrap();
    assert_eq!(spent, -rules::SEED_COST);
}
#[tokio::test]
async fn aquarium_moves_owned_fish_and_protects_it_from_sales_and_crafting() {
    let _isolation = crate::support::shared().await;
    let Some((db, u)) = setup().await else { return };
    let now = Utc::now();
    let id = fish(&db, u).await;
    assert_eq!(
        db.reef_action(u + 1, now, A::AddFish { bucket: id, slot: 1 })
            .await
            .unwrap(),
        O::Missing
    );
    assert_eq!(
        db.reef_action(u, now, A::AddFish { bucket: id, slot: 1 })
            .await
            .unwrap(),
        O::Done
    );
    assert!(db.bucket(u).await.unwrap().is_empty());
    let stale_sale = db.sell_fish(u, &[(id, 999)]).await.unwrap();
    assert_eq!((stale_sale.fish, stale_sale.shells), (0, 0));
    assert_eq!(db.reef_aquarium(u).await.unwrap()[0].size_cm, 5.);
    assert_eq!(
        db.reef_action(u, now, A::AddFish { bucket: id, slot: 2 })
            .await
            .unwrap(),
        O::Missing
    );
    assert_eq!(db.reef_action(u, now, A::RemoveFish(1)).await.unwrap(), O::Done);
    assert_eq!(db.reef_action(u, now, A::RemoveFish(1)).await.unwrap(), O::Missing);
    assert_eq!(db.bucket(u).await.unwrap().len(), 1);
}
#[tokio::test]
async fn request_duplicate_click_consumes_and_rewards_once() {
    let _isolation = crate::support::shared().await;
    let Some((db, u)) = setup().await else { return };
    let now = Utc::now();
    fish(&db, u).await;
    fish(&db, u).await;
    let a = A::Request {
        fish: "toaststripe_minnow",
    };
    let (x, y) = tokio::join!(db.reef_action(u, now, a), db.reef_action(u, now, a));
    assert_eq!([x.unwrap(), y.unwrap()].iter().filter(|&&r| r == O::Done).count(), 1);
    assert_eq!(db.reef_state(u).await.unwrap().seaweed, 2);
    assert!(db.bucket(u).await.unwrap().is_empty());
    assert_eq!(db.user(u).await.unwrap().credits, 10000);
}
#[tokio::test]
async fn crafting_rolls_back_all_costs_on_missing_fish_then_grants_bound_furniture() {
    let _isolation = crate::support::shared().await;
    let Some((db, u)) = setup().await else { return };
    let now = Utc::now();
    db.reef_state(u).await.unwrap();
    sqlx::query("UPDATE reef_loop SET seaweed=8 WHERE user_id=$1")
        .bind(u as i64)
        .execute(db.pool())
        .await
        .unwrap();
    let action = A::Craft {
        recipe: "sprig",
        eligible_fish: &["toaststripe_minnow"],
    };
    assert_eq!(db.reef_action(u, now, action).await.unwrap(), O::Ingredients);
    let before = db.user(u).await.unwrap();
    assert_eq!((before.credits, before.pearls), (10000, 20));
    fish(&db, u).await;
    let (a, b) = tokio::join!(db.reef_action(u, now, action), db.reef_action(u, now, action));
    assert_eq!([a.unwrap(), b.unwrap()].iter().filter(|&&r| r == O::Done).count(), 1);
    assert_eq!(db.reef_state(u).await.unwrap().seaweed, 0);
    let n: i32 = sqlx::query_scalar(
        "SELECT quantity FROM inventory WHERE user_id=$1 AND item_id='sprigstep_planter_box__mint~bound'",
    )
    .bind(u as i64)
    .fetch_one(db.pool())
    .await
    .unwrap();
    assert_eq!(n, 1);
    let after = db.user(u).await.unwrap();
    assert_eq!((after.credits, after.pearls), (9750, 19));
    let (entries, delta): (i64, i64) = sqlx::query_as(
        "SELECT count(*),sum(delta)::BIGINT FROM credit_ledger WHERE user_id=$1 AND reason='reef_craft'",
    )
    .bind(u as i64)
    .fetch_one(db.pool())
    .await
    .unwrap();
    assert_eq!((entries, delta), (1, after.credits - before.credits));
    let item = "sprigstep_planter_box__mint~bound";
    assert!(!db.list_item(u, item, 999999).await.unwrap());
    assert!(!db.gift_item(u, u + 1, item).await.unwrap());
}
#[tokio::test]
async fn diving_replay_loss_expiry_daily_cap_and_privacy_tombstone() {
    let _isolation = crate::support::shared().await;
    let Some((db, u)) = setup().await else { return };
    let now = Utc::now();
    assert_eq!(db.reef_action(u, now, A::DiveStart).await.unwrap(), O::Done);
    let n = db.reef_state(u).await.unwrap().dive_nonce;
    let push = A::DiveMove {
        nonce: n,
        depth: 0,
        surface: false,
        roll: 99,
    };
    let (a, b) = tokio::join!(db.reef_action(u, now, push), db.reef_action(u, now, push));
    assert_eq!([a.unwrap(), b.unwrap()].iter().filter(|&&x| x == O::Done).count(), 1);
    assert_eq!(
        db.reef_action(
            u,
            now,
            A::DiveMove {
                nonce: n,
                depth: 1,
                surface: true,
                roll: 0
            }
        )
        .await
        .unwrap(),
        O::Done
    );
    assert_eq!(
        db.reef_action(
            u,
            now,
            A::DiveMove {
                nonce: n,
                depth: 1,
                surface: true,
                roll: 0
            }
        )
        .await
        .unwrap(),
        O::Stale
    );
    assert_eq!(db.user(u).await.unwrap().pearls, 21);
    assert_eq!(db.reef_action(u, now, A::DiveStart).await.unwrap(), O::Already);
    let (entries, delta): (i64, i64) =
        sqlx::query_as("SELECT count(*),sum(delta)::BIGINT FROM credit_ledger WHERE user_id=$1 AND reason='reef_dive'")
            .bind(u as i64)
            .fetch_one(db.pool())
            .await
            .unwrap();
    assert_eq!((entries, delta), (1, -rules::DIVE_COST));
    assert_eq!(db.user(u).await.unwrap().credits, 10000 + delta);
    db.delete_user_data(u, now).await.unwrap();
    db.earn_shells(u, 1000, "test").await.unwrap();
    assert_eq!(db.reef_action(u, now, A::DiveStart).await.unwrap(), O::Already);
    let tomorrow = now + Duration::days(1);
    assert_eq!(db.reef_action(u, tomorrow, A::DiveStart).await.unwrap(), O::Done);
    let n = db.reef_state(u).await.unwrap().dive_nonce;
    assert_eq!(
        db.reef_action(
            u,
            tomorrow,
            A::DiveMove {
                nonce: n,
                depth: 0,
                surface: false,
                roll: 0
            }
        )
        .await
        .unwrap(),
        O::Lost
    );
    assert_eq!(db.user(u).await.unwrap().pearls, 0);
    let later = tomorrow + Duration::days(1);
    db.reef_action(u, later, A::DiveStart).await.unwrap();
    let s = db.reef_state(u).await.unwrap();
    assert_eq!(
        db.reef_action(
            u,
            later + Duration::seconds(90),
            A::DiveMove {
                nonce: s.dive_nonce,
                depth: 0,
                surface: true,
                roll: 0
            }
        )
        .await
        .unwrap(),
        O::Stale
    );
}
#[tokio::test]
async fn partner_both_check_in_and_repairing_cannot_farm_rewards() {
    let _isolation = crate::support::shared().await;
    let Some((db, u)) = setup().await else { return };
    let v = u + 1;
    let now = Utc::now();
    db.user(v).await.unwrap();
    db.pair_partners(u, v).await.unwrap();
    assert_eq!(db.reef_partner_checkin(u, now).await.unwrap(), (O::Pending, 0));
    assert_eq!(db.reef_partner_checkin(v, now).await.unwrap(), (O::Done, 1));
    assert_eq!(db.reef_partner_checkin(u, now).await.unwrap(), (O::Already, 1));
    assert_eq!(db.reef_state(u).await.unwrap().seaweed, 1);
    assert_eq!(db.reef_state(v).await.unwrap().seaweed, 1);
    db.leave_partner(u).await.unwrap();
    db.pair_partners(u, v).await.unwrap();
    db.reef_partner_checkin(u, now).await.unwrap();
    db.reef_partner_checkin(v, now).await.unwrap();
    assert_eq!(db.reef_state(u).await.unwrap().seaweed, 1);
}
#[tokio::test]
async fn pet_care_streak_requires_successful_care_and_counts_once_across_pets() {
    let _isolation = crate::support::shared().await;
    let Some((db, u)) = setup().await else { return };
    let now = Utc::now();
    let p = db.adopt(u, "crab", "Crumb").await.unwrap();
    assert_eq!(db.reef_state(u).await.unwrap().pet_streak, 0);
    assert!(db.save_pet(u, &p, [90, 90, 90, 90], 1, 5, now).await.unwrap());
    assert!(!db.save_pet(u, &p, [90, 90, 90, 90], 1, 5, now).await.unwrap());
    assert_eq!(db.reef_state(u).await.unwrap().seaweed, 1);
    let p = db.active_pet(u).await.unwrap().unwrap();
    assert!(
        db.save_pet(u, &p, [90, 90, 90, 90], 1, 10, now + Duration::days(2))
            .await
            .unwrap()
    );
    assert_eq!(db.reef_state(u).await.unwrap().pet_streak, 2);
}
#[tokio::test]
async fn catch_boards_and_season_claim_respect_privacy_and_global_claim_limit() {
    let _isolation = crate::support::shared().await;
    let Some((db, u)) = setup().await else { return };
    let now = Utc::now();
    fish(&db, u).await;
    let day = now.date_naive();
    assert_eq!(
        db.reef_board(u + 2, day, day + Duration::days(1)).await.unwrap().len(),
        1
    );
    db.set_privacy(u, Hide::Leaderboard, true).await.unwrap();
    assert!(
        db.reef_board(u + 2, day, day + Duration::days(1))
            .await
            .unwrap()
            .is_empty()
    );
    let season = rules::season_start(rules::season_start(day) - Duration::days(1));
    for guild in [u + 2, u + 3] {
        for i in 0..3 {
            sqlx::query("INSERT INTO reef_catches(guild_id,day,user_id,fish_id,size_cm,score) VALUES($1,$2,$3,'toaststripe_minnow',5,1)").bind(guild as i64).bind(season+Duration::days(i)).bind(u as i64).execute(db.pool()).await.unwrap();
        }
    }
    let action = A::SeasonClaim { guild: u + 2, season };
    let (a, b) = tokio::join!(
        db.reef_action(u, now, action),
        db.reef_action(u, now, A::SeasonClaim { guild: u + 3, season })
    );
    assert_eq!([a.unwrap(), b.unwrap()].iter().filter(|&&r| r == O::Done).count(), 1);
    let quantity: i32 =
        sqlx::query_scalar("SELECT quantity FROM inventory WHERE user_id=$1 AND item_id='reefbubble_lamp__sky~bound'")
            .bind(u as i64)
            .fetch_one(db.pool())
            .await
            .unwrap();
    assert_eq!(quantity, 1);
    assert_eq!(db.reef_action(u, now, action).await.unwrap(), O::Already);
    let current = rules::season_start(day);
    assert_eq!(
        db.reef_action(
            u,
            now,
            A::SeasonClaim {
                guild: u + 2,
                season: current
            }
        )
        .await
        .unwrap(),
        O::Wait
    );
}

#[tokio::test]
async fn partner_checkins_never_rewind_across_utc_midnight() {
    let _isolation = crate::support::shared().await;
    let Some((db, u)) = setup().await else { return };
    let v = u + 1;
    db.user(v).await.unwrap();
    db.pair_partners(u, v).await.unwrap();
    let midnight = Utc::now().date_naive().and_hms_opt(0, 0, 0).unwrap().and_utc();
    let yesterday = midnight - Duration::seconds(1);
    assert_eq!(db.reef_partner_checkin(u, midnight).await.unwrap(), (O::Pending, 0));
    // An older in-flight check-in must not erase today's check-in.
    db.reef_partner_checkin(u, yesterday).await.unwrap();
    assert_eq!(db.reef_partner_checkin(v, midnight).await.unwrap(), (O::Done, 1));
    db.reef_partner_checkin(u, yesterday).await.unwrap();
    db.reef_partner_checkin(v, yesterday).await.unwrap();
    assert_eq!(db.reef_state(u).await.unwrap().seaweed, 1);
    assert_eq!(db.reef_partner_checkin(u, midnight).await.unwrap(), (O::Already, 1));
    // A missed UTC day is forgiven, but two missed days reset the count.
    for (gap, expected) in [(2, 2), (5, 1)] {
        let now = midnight + Duration::days(gap);
        db.reef_partner_checkin(u, now).await.unwrap();
        assert_eq!(db.reef_partner_checkin(v, now).await.unwrap(), (O::Done, expected));
    }
}

#[tokio::test]
async fn dive_bank_at_balance_limit_is_recoverable_and_atomic() {
    let _isolation = crate::support::shared().await;
    let Some((db, u)) = setup().await else { return };
    let now = Utc::now();
    db.reef_action(u, now, A::DiveStart).await.unwrap();
    let nonce = db.reef_state(u).await.unwrap().dive_nonce;
    let push = A::DiveMove {
        nonce,
        depth: 0,
        surface: false,
        roll: 99,
    };
    assert_eq!(db.reef_action(u, now, push).await.unwrap(), O::Done);
    sqlx::query("UPDATE users SET pearls=$2 WHERE id=$1")
        .bind(u as i64)
        .bind(i64::MAX)
        .execute(db.pool())
        .await
        .unwrap();
    let bank = A::DiveMove {
        nonce,
        depth: 1,
        surface: true,
        roll: 0,
    };
    assert_eq!(db.reef_action(u, now, bank).await.unwrap(), O::Full);
    assert_eq!(db.user(u).await.unwrap().pearls, i64::MAX);
    assert!(db.reef_state(u).await.unwrap().dive_until.is_some());
    sqlx::query("UPDATE users SET pearls=pearls-1 WHERE id=$1")
        .bind(u as i64)
        .execute(db.pool())
        .await
        .unwrap();
    assert_eq!(db.reef_action(u, now, bank).await.unwrap(), O::Done);
    assert_eq!(db.reef_action(u, now, bank).await.unwrap(), O::Stale);
    assert_eq!(db.user(u).await.unwrap().pearls, i64::MAX);
}

#[tokio::test]
async fn contest_rejects_nonfinite_or_nonpositive_scores_without_recording_a_catch() {
    let _isolation = crate::support::shared().await;
    let Some((db, u)) = setup().await else { return };
    for (size_cm, ratio) in [
        (5., f64::NAN),
        (5., f64::INFINITY),
        (5., -1.),
        (5., 0.),
        (f32::NAN, 1.),
        (f32::INFINITY, 1.),
        (-1., 1.),
    ] {
        assert!(
            db.record_catch(
                u,
                NewCatch {
                    fish_id: "toaststripe_minnow",
                    size_cm,
                    ratio,
                    weekly: Some((u + 2, Utc::now().date_naive())),
                    milestones: &[],
                    known: &["toaststripe_minnow"],
                }
            )
            .await
            .is_err()
        );
    }
    assert!(db.bucket(u).await.unwrap().is_empty());
    let day = Utc::now().date_naive();
    assert!(
        db.reef_board(u + 2, day, day + Duration::days(1))
            .await
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn contest_timestamps_are_taken_after_waiting_for_the_player_lock() {
    let _isolation = crate::support::shared().await;
    let Some((db, u)) = setup().await else { return };
    let mut blocker = db.pool().begin().await.unwrap();
    let blocker_pid: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
        .fetch_one(&mut *blocker)
        .await
        .unwrap();
    sqlx::query("SELECT lock_user($1)")
        .bind(u as i64)
        .execute(&mut *blocker)
        .await
        .unwrap();
    let other = db.clone();
    let pending = tokio::spawn(async move { fish(&other, u).await });
    // Observe the actual lock wait, rather than relying on scheduler timing.
    tokio::time::timeout(std::time::Duration::from_secs(10), async {
        loop {
            let waiting: bool =
                sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_stat_activity WHERE $1=ANY(pg_blocking_pids(pid)))")
                    .bind(blocker_pid)
                    .fetch_one(db.pool())
                    .await
                    .unwrap();
            if waiting {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    let released: chrono::DateTime<Utc> = sqlx::query_scalar("SELECT clock_timestamp()")
        .fetch_one(&mut *blocker)
        .await
        .unwrap();
    blocker.commit().await.unwrap();
    pending.await.unwrap();
    let (day, caught): (chrono::NaiveDate, chrono::DateTime<Utc>) =
        sqlx::query_as("SELECT day,caught_at FROM reef_catches WHERE user_id=$1")
            .bind(u as i64)
            .fetch_one(db.pool())
            .await
            .unwrap();
    assert!(caught >= released, "a queued catch must not write a closed UTC day");
    assert_eq!(day, caught.date_naive());
    assert!(
        db.reef_board(u + 3, day, day + Duration::days(1))
            .await
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn reef_migration_is_additive_indexed_and_cascades_personal_data() {
    let _isolation = crate::support::shared().await;
    let Ok(url) = std::env::var("PINCHORA_TEST_DATABASE_URL") else {
        return;
    };
    let db = Db::connect_waiting(&url, 2, std::time::Duration::from_secs(60))
        .await
        .unwrap();
    // Exercise the exact unreleased SQL on a populated earlier schema, then roll it all back.
    let mut tx = db.pool().begin().await.unwrap();
    let schema = format!("reef_migration_{}", Utc::now().timestamp_nanos_opt().unwrap());
    sqlx::raw_sql(sqlx::AssertSqlSafe(format!("CREATE SCHEMA {schema}; SET LOCAL search_path TO {schema}; CREATE TABLE users(id BIGINT PRIMARY KEY,credits BIGINT NOT NULL); CREATE TABLE partners(user_a BIGINT PRIMARY KEY,user_b BIGINT NOT NULL); INSERT INTO users VALUES(1,42),(2,84); INSERT INTO partners VALUES(1,2);")))
        .execute(&mut *tx).await.unwrap();
    sqlx::raw_sql(include_str!("../../../../migrations/0078_reef_loop.sql"))
        .execute(&mut *tx)
        .await
        .unwrap();
    let credits: i64 = sqlx::query_scalar("SELECT sum(credits)::BIGINT FROM users")
        .fetch_one(&mut *tx)
        .await
        .unwrap();
    assert_eq!(credits, 126);
    let indexes: Vec<String> = sqlx::query_scalar("SELECT indexname FROM pg_indexes WHERE schemaname=$1")
        .bind(&schema)
        .fetch_all(&mut *tx)
        .await
        .unwrap();
    for name in [
        "reef_loop_daily_prune",
        "reef_requests_prune",
        "reef_catches_user",
        "reef_catches_prune",
        "reef_partner_streaks_user_b",
    ] {
        assert!(indexes.iter().any(|index| index == name), "missing index: {name}");
    }
    sqlx::raw_sql("INSERT INTO reef_loop(user_id) VALUES(1); INSERT INTO reef_aquarium VALUES(1,1,'fish',5); INSERT INTO reef_requests VALUES(1,1); INSERT INTO reef_catches(guild_id,day,user_id,fish_id,size_cm,score) VALUES(3,CURRENT_DATE,1,'fish',5,1); INSERT INTO reef_season_claims VALUES(1,CURRENT_DATE); INSERT INTO reef_loop_daily(user_id,day,dives) VALUES(1,1,1); INSERT INTO reef_partner_streaks(user_a,user_b,since) VALUES(1,2,now()); DELETE FROM partners WHERE user_a=1; DELETE FROM users WHERE id=1;")
        .execute(&mut *tx).await.unwrap();
    for table in [
        "reef_loop",
        "reef_aquarium",
        "reef_requests",
        "reef_catches",
        "reef_season_claims",
        "reef_partner_streaks",
    ] {
        let count: i64 = sqlx::query_scalar(sqlx::AssertSqlSafe(format!("SELECT count(*) FROM {table}")))
            .fetch_one(&mut *tx)
            .await
            .unwrap();
        assert_eq!(count, 0, "{table} must cascade");
    }
    let retained: i64 = sqlx::query_scalar("SELECT count(*) FROM reef_loop_daily WHERE dives=1")
        .fetch_one(&mut *tx)
        .await
        .unwrap();
    assert_eq!(
        retained, 1,
        "daily anti-abuse tombstones intentionally survive deletion"
    );
    tx.rollback().await.unwrap();
}

#[tokio::test]
async fn garden_cannot_harvest_before_its_water_timestamp_after_clock_rollback() {
    let _isolation = crate::support::shared().await;
    let Some((db, u)) = setup().await else { return };
    let now = Utc::now();
    assert_eq!(db.reef_action(u, now, A::Plant).await.unwrap(), O::Done);
    assert_eq!(
        db.reef_action(u, now - Duration::seconds(1), A::Water).await.unwrap(),
        O::Wait
    );
    assert_eq!(
        db.reef_action(u, now + Duration::hours(10), A::Water).await.unwrap(),
        O::Done
    );
    assert_eq!(
        db.reef_action(u, now + Duration::hours(8), A::Harvest).await.unwrap(),
        O::Wait
    );
    assert_eq!(db.reef_state(u).await.unwrap().seaweed, 0);
    assert_eq!(
        db.reef_action(u, now + Duration::hours(10), A::Harvest).await.unwrap(),
        O::Done
    );
}

#[tokio::test]
async fn dive_cannot_move_before_entry_or_revive_after_observed_expiration() {
    let _isolation = crate::support::shared().await;
    let Some((db, u)) = setup().await else { return };
    let now = Utc::now();
    assert_eq!(db.reef_action(u, now, A::DiveStart).await.unwrap(), O::Done);
    let nonce = db.reef_state(u).await.unwrap().dive_nonce;
    let push = A::DiveMove {
        nonce,
        depth: 0,
        surface: false,
        roll: 99,
    };
    assert_eq!(
        db.reef_action(u, now - Duration::seconds(1), push).await.unwrap(),
        O::Stale
    );
    assert_eq!(
        db.reef_action(u, now + Duration::seconds(rules::DIVE_SECS), push)
            .await
            .unwrap(),
        O::Stale
    );
    assert_eq!(db.reef_action(u, now, push).await.unwrap(), O::Stale);
    assert_eq!(db.reef_action(u, now, A::DiveStart).await.unwrap(), O::Already);
    assert_eq!(db.user(u).await.unwrap().pearls, 20);
}
