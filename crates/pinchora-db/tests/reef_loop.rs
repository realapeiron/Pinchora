//! D7 real-DB money, ownership, clock and concurrency boundaries.
use chrono::{Duration, Utc};
use pinchora_core::reef_loop as rules;
use pinchora_db::{Db, Hide, NewCatch, ReefAction as A, ReefOutcome as O};
async fn setup() -> Option<(Db, u64)> {
    let url = std::env::var("PINCHORA_TEST_DATABASE_URL").ok()?;
    let db = Db::connect_waiting(&url, 8, std::time::Duration::from_secs(60))
        .await
        .unwrap();
    db.migrate().await.unwrap();
    let id = (std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos() as u64)
        >> 4;
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
}
#[tokio::test]
async fn aquarium_moves_owned_fish_and_protects_it_from_sales_and_crafting() {
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
}
#[tokio::test]
async fn diving_replay_loss_expiry_daily_cap_and_privacy_tombstone() {
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
    for i in 0..3 {
        sqlx::query("INSERT INTO reef_catches(guild_id,day,user_id,fish_id,size_cm,score) VALUES($1,$2,$3,'toaststripe_minnow',5,1)").bind((u+2) as i64).bind(season+Duration::days(i)).bind(u as i64).execute(db.pool()).await.unwrap();
    }
    let action = A::SeasonClaim { guild: u + 2, season };
    let (a, b) = tokio::join!(db.reef_action(u, now, action), db.reef_action(u, now, action));
    assert_eq!([a.unwrap(), b.unwrap()].iter().filter(|&&r| r == O::Done).count(), 1);
}
