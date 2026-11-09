use crate::economy_player::*;

#[tokio::test]
async fn shifts_are_ordered_capped_idempotent_and_promote() {
    let _isolation = crate::support::shared().await;
    let Some((db, u)) = setup().await else { return };
    for n in 0..3 {
        let time = now() + Duration::hours(n);
        let v = db
            .economy_action(
                u,
                &format!("start{n}"),
                &A::StartShift {
                    job: "kelp_keeper".into(),
                },
                time,
            )
            .await
            .unwrap();
        let token = v["token"].as_i64().unwrap();
        if n == 0 {
            sqlx::query("UPDATE economy_players SET shifts=9 WHERE user_id=$1")
                .bind(u as i64)
                .execute(db.pool())
                .await
                .unwrap();
        }
        assert!(
            db.economy_action(u, "skip", &A::Step { token, step: 2 }, time)
                .await
                .is_err()
        );
        for step in 0..3 {
            let action = A::Step { token, step };
            let key = format!("step{n}:{step}");
            let (a, b) = tokio::join!(
                db.economy_action(u, &key, &action, time),
                db.economy_action(u, &key, &action, time)
            );
            assert_eq!(a.unwrap(), b.unwrap());
        }
    }
    assert_eq!(db.user(u).await.unwrap().credits, 20350);
    assert!(
        db.economy_action(
            u,
            "cap",
            &A::StartShift {
                job: "beacon_tender".into()
            },
            now() + Duration::hours(3)
        )
        .await
        .is_err()
    );
    db.delete_user_data(u, now() + Duration::hours(3)).await.unwrap();
    assert!(
        db.economy_action(
            u,
            "deleted-cap",
            &A::StartShift {
                job: "kelp_keeper".into()
            },
            now() + Duration::hours(4)
        )
        .await
        .is_err()
    );
}

#[tokio::test]
async fn provisions_chests_season_and_prestige_keep_budgets() {
    let _isolation = crate::support::shared().await;
    let Some((db, u)) = setup().await else { return };
    act(&db, u, "tend", A::Tend).await;
    assert!(
        db.economy_action(u, "early", &A::Provisions, now() + Duration::minutes(59))
            .await
            .is_err()
    );
    let action = A::Provisions;
    let t = now() + Duration::days(10);
    let (a, b) = tokio::join!(
        db.economy_action(u, "provisions-a", &action, t),
        db.economy_action(u, "provisions-b", &action, t)
    );
    assert_eq!(usize::from(a.is_ok()) + usize::from(b.is_ok()), 1);
    assert_eq!(db.user(u).await.unwrap().credits, 20120);
    assert!(db.economy_action(u, "empty", &A::Chest, t).await.is_err());
    sqlx::query("UPDATE economy_players SET chests=1,shifts=40,season=202611,season_xp=500 WHERE user_id=$1")
        .bind(u as i64)
        .execute(db.pool())
        .await
        .unwrap();
    let (a, b) = tokio::join!(
        db.economy_action(u, "chest-a", &A::Chest, t),
        db.economy_action(u, "chest-b", &A::Chest, t)
    );
    assert_eq!(usize::from(a.is_ok()) + usize::from(b.is_ok()), 1);
    for tier in 1..=10 {
        act(&db, u, &format!("tier{tier}"), A::Season { tier }).await;
    }
    assert!(
        db.economy_action(u, "duplicate-tier", &A::Season { tier: 10 }, now())
            .await
            .is_err()
    );
    act(&db, u, "prestige", A::Prestige { expected: 0 }).await;
    let s = db.economy_snapshot(u, now()).await.unwrap();
    assert_eq!(s["player"]["shifts"], 0);
    assert_eq!(s["player"]["prestige"], 1);
    assert_eq!(s["season_claimed"], 10);
    ledger_matches(&db, u).await;
    db.delete_user_data(u, now()).await.unwrap();
    db.user(u).await.unwrap();
    sqlx::query("INSERT INTO economy_players(user_id,season,season_xp) VALUES($1,202611,500)")
        .bind(u as i64)
        .execute(db.pool())
        .await
        .unwrap();
    assert!(
        db.economy_action(u, "privacy-season", &A::Season { tier: 1 }, now())
            .await
            .is_err()
    );
    let dec = Utc.with_ymd_and_hms(2026, 12, 1, 0, 0, 0).unwrap();
    assert!(
        db.economy_action(u, "new-season", &A::Season { tier: 1 }, dec)
            .await
            .is_err()
    );
}

#[tokio::test]
async fn overflow_rolls_back_claim_and_donations_are_idempotent_sinks() {
    let _isolation = crate::support::shared().await;
    let Some((db, u)) = setup().await else { return };
    let a = A::Donate {
        guild: (u + 10).to_string(),
        amount: 100,
    };
    let (a, b) = tokio::join!(
        db.economy_action(u, "donate", &a, now()),
        db.economy_action(u, "donate", &a, now())
    );
    assert_eq!(a.unwrap(), b.unwrap());
    assert_eq!(db.economy_settings(u + 10).await.unwrap()["pool_total"], 100);
    ledger_matches(&db, u).await;
    act(&db, u, "tend", A::Tend).await;
    sqlx::query("UPDATE users SET credits=9223372036854775807 WHERE id=$1")
        .bind(u as i64)
        .execute(db.pool())
        .await
        .unwrap();
    assert!(
        db.economy_action(u, "overflow", &A::Provisions, now() + Duration::hours(12))
            .await
            .is_err()
    );
    assert_eq!(
        db.economy_snapshot(u, now()).await.unwrap()["player"]["provision_claimed"],
        0
    );
    // Restore the synthetic boundary balance so shared leaderboard fixtures stay valid.
    sqlx::query("UPDATE users SET credits=19900 WHERE id=$1")
        .bind(u as i64)
        .execute(db.pool())
        .await
        .unwrap();
    ledger_matches(&db, u).await;
}

#[tokio::test]
async fn server_conditions_and_preview_reset_never_touch_shells() {
    let _isolation = crate::support::shared().await;
    let Some((db, u)) = setup().await else { return };
    let g = u + 10;
    sqlx::query("INSERT INTO members(guild_id,user_id,points) VALUES($1,$2,1001)")
        .bind(g as i64)
        .bind(u as i64)
        .execute(db.pool())
        .await
        .unwrap();
    db.save_economy_settings(
        g,
        &EconomySettings {
            shop_enabled: false,
            ..Default::default()
        },
        0,
    )
    .await
    .unwrap();
    let it = db.add_store_item(g, u + 20, 1, None).await.unwrap().unwrap();
    assert_eq!(
        db.buy_store_item(g, u, it.id).await.unwrap(),
        pinchora_db::StoreBuy::Conditions
    );
    let preview = db.economy_points_preview(g).await.unwrap();
    db.economy_scale_points(g, 50, preview["version"].as_str().unwrap())
        .await
        .unwrap();
    assert!(
        db.economy_scale_points(g, 0, preview["version"].as_str().unwrap())
            .await
            .is_err()
    );
    assert_eq!(db.user(u).await.unwrap().credits, 20000);
    let fresh = db.economy_points_preview(g).await.unwrap();
    db.economy_scale_points(g, 0, fresh["version"].as_str().unwrap())
        .await
        .unwrap();
    assert_eq!(db.economy_points_preview(g).await.unwrap()["total"], "0");
}

#[tokio::test]
async fn worker_receipts_cannot_be_poisoned_and_leadership_releases_on_restart() {
    let _isolation = crate::support::exclusive().await;
    let Some((db, u)) = setup().await else { return };
    let id = act(
        &db,
        u,
        "create",
        A::Trade {
            to: (u + 1).to_string(),
            give: offer(100, ""),
            receive: offer(200, ""),
        },
    )
    .await["id"]
        .as_i64()
        .unwrap();
    act(&db, u, &format!("expire:{id}"), A::Tend).await;
    assert!(
        db.economy_action(
            u,
            &format!("worker:expire:{id}"),
            &A::Provisions,
            now() + Duration::hours(12)
        )
        .await
        .is_err()
    );
    let mut leader = db.pool().begin().await.unwrap();
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended('economy:sweep',0))")
        .execute(&mut *leader)
        .await
        .unwrap();
    db.sweep_economy(now() + Duration::hours(25)).await.unwrap();
    assert_eq!(db.user(u).await.unwrap().credits, 19900);
    leader.rollback().await.unwrap();
    let (a, b) = tokio::join!(
        db.sweep_economy(now() + Duration::hours(25)),
        db.sweep_economy(now() + Duration::hours(25))
    );
    a.unwrap();
    b.unwrap();
    db.sweep_economy(now() + Duration::hours(25)).await.unwrap();
    assert_eq!(db.user(u).await.unwrap().credits, 20000);
    let n: i64 = sqlx::query_scalar("SELECT count(*) FROM credit_ledger WHERE user_id=$1 AND reason='trade_refund'")
        .bind(u as i64)
        .fetch_one(db.pool())
        .await
        .unwrap();
    assert_eq!(n, 1);
}

#[tokio::test]
async fn invalid_amounts_and_inventory_overflow_leave_no_partial_moves() {
    let _isolation = crate::support::shared().await;
    let Some((db, u)) = setup().await else { return };
    for amount in [i64::MIN, -1, 0, i64::MAX] {
        for a in [
            A::Donate {
                guild: (u + 10).to_string(),
                amount,
            },
            A::Auction {
                item: "chair".into(),
                price: amount,
                hours: 1,
            },
        ] {
            assert!(db.economy_action(u, "invalid", &a, now()).await.is_err());
        }
    }
    item(&db, u, "chair", 1).await;
    item(&db, u + 1, "chair", i32::MAX).await;
    let id = act(
        &db,
        u,
        "trade",
        A::Trade {
            to: (u + 1).to_string(),
            give: offer(100, "chair"),
            receive: offer(200, ""),
        },
    )
    .await["id"]
        .as_i64()
        .unwrap();
    act(&db, u, "a", A::ConfirmTrade { id }).await;
    assert!(
        db.economy_action(u + 1, "b", &A::ConfirmTrade { id }, now())
            .await
            .is_err()
    );
    assert_eq!(db.user(u + 1).await.unwrap().credits, 20000);
    act(&db, u, "cancel", A::CancelTrade { id }).await;
    assert_eq!(db.user(u).await.unwrap().credits, 20000);
    assert_eq!(qty(&db, u, "chair").await, 1);
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM economy_ledger WHERE user_id=$1")
        .bind((u + 1) as i64)
        .fetch_one(db.pool())
        .await
        .unwrap();
    assert_eq!(count, 0);
}

#[tokio::test]
async fn reset_preview_is_bound_to_the_guild() {
    let _isolation = crate::support::shared().await;
    let Some((db, u)) = setup().await else { return };
    for guild in [u + 10, u + 11] {
        sqlx::query("INSERT INTO members(guild_id,user_id,points) VALUES($1,$2,1000)")
            .bind(guild as i64)
            .bind(u as i64)
            .execute(db.pool())
            .await
            .unwrap();
    }
    let a = db.economy_points_preview(u + 10).await.unwrap();
    let b = db.economy_points_preview(u + 11).await.unwrap();
    assert_ne!(a["version"], b["version"]);
    assert!(
        db.economy_scale_points(u + 11, 0, a["version"].as_str().unwrap())
            .await
            .is_err()
    );
    assert_eq!(db.economy_points_preview(u + 11).await.unwrap()["total"], "1000");
}

#[tokio::test]
async fn migration_0083_preserves_existing_balances_inventory_and_access_grants() {
    let _isolation = crate::support::shared().await;
    let Some((db, u)) = setup().await else { return };
    let mut tx = db.pool().begin().await.unwrap();
    let schema = format!("economy_migration_{u}");
    // A private transaction models the live earlier tables; rollback removes the entire schema.
    sqlx::raw_sql(sqlx::AssertSqlSafe(format!("CREATE SCHEMA {schema}; SET LOCAL search_path TO {schema}; CREATE TABLE users(id bigint PRIMARY KEY,credits bigint NOT NULL); CREATE TABLE members(guild_id bigint,user_id bigint,points bigint); CREATE TABLE inventory(user_id bigint,item_id text,quantity integer); INSERT INTO users VALUES(1,987654); INSERT INTO members VALUES(10,1,4321); INSERT INTO inventory VALUES(1,'chair',7);"))).execute(&mut *tx).await.unwrap();
    sqlx::raw_sql(include_str!("../../../../../migrations/0080_dashboard_managers.sql"))
        .execute(&mut *tx)
        .await
        .unwrap();
    sqlx::query("INSERT INTO dashboard_managers VALUES(10,20,ARRAY['configuration'])")
        .execute(&mut *tx)
        .await
        .unwrap();
    sqlx::raw_sql(include_str!("../../../../../migrations/0083_player_economy_v2.sql"))
        .execute(&mut *tx)
        .await
        .unwrap();
    let balances:(i64,i64,i32)=sqlx::query_as("SELECT credits,points,quantity FROM users JOIN members ON users.id=members.user_id JOIN inventory ON inventory.user_id=users.id").fetch_one(&mut *tx).await.unwrap();
    assert_eq!(balances, (987654, 4321, 7));
    let areas: Vec<String> = sqlx::query_scalar("SELECT areas FROM dashboard_managers WHERE role_id=20")
        .fetch_one(&mut *tx)
        .await
        .unwrap();
    assert_eq!(areas, vec!["configuration"]);
    sqlx::query("INSERT INTO dashboard_managers VALUES(10,21,ARRAY['economy'])")
        .execute(&mut *tx)
        .await
        .unwrap();
    tx.rollback().await.unwrap();
}

#[tokio::test]
async fn admin_audit_failure_rolls_back_settings_and_reset() {
    let _isolation = crate::support::shared().await;
    let Some((db, u)) = setup().await else { return };
    let actor = pinchora_db::economy::player::EconomyAudit {
        actor: u,
        name: "invalid\0name",
        source: pinchora_db::AuditSource::Web,
    };
    assert!(
        db.save_economy_settings_audited(
            u + 10,
            &EconomySettings {
                shop_enabled: false,
                ..Default::default()
            },
            0,
            Some(&actor)
        )
        .await
        .is_err()
    );
    assert_eq!(db.economy_settings(u + 10).await.unwrap()["shop_enabled"], true);
    sqlx::query("INSERT INTO members(guild_id,user_id,points) VALUES($1,$2,1000)")
        .bind((u + 10) as i64)
        .bind(u as i64)
        .execute(db.pool())
        .await
        .unwrap();
    let preview = db.economy_points_preview(u + 10).await.unwrap();
    assert!(
        db.economy_scale_points_audited(u + 10, 0, preview["version"].as_str().unwrap(), Some(&actor))
            .await
            .is_err()
    );
    assert_eq!(db.economy_points_preview(u + 10).await.unwrap()["total"], "1000");
    assert_eq!(db.user(u).await.unwrap().credits, 20000);
}

#[tokio::test]
async fn earned_chest_moves_are_ledgered_once_and_respect_storage_cap() {
    let _isolation = crate::support::shared().await;
    let Some((db, u)) = setup().await else { return };
    for (n, stored) in [(0, 0), (1, 10)] {
        let time = now() + Duration::hours(n);
        let v = db
            .economy_action(
                u,
                &format!("chest-start:{n}"),
                &A::StartShift {
                    job: "kelp_keeper".into(),
                },
                time,
            )
            .await
            .unwrap();
        sqlx::query("UPDATE economy_players SET shifts=4,chests=$2 WHERE user_id=$1")
            .bind(u as i64)
            .bind(stored)
            .execute(db.pool())
            .await
            .unwrap();
        let token = v["token"].as_i64().unwrap();
        for step in 0..3 {
            let action = A::Step { token, step };
            let key = format!("chest-step:{n}:{step}");
            let (a, b) = tokio::join!(
                db.economy_action(u, &key, &action, time),
                db.economy_action(u, &key, &action, time)
            );
            assert_eq!(a.unwrap(), b.unwrap());
        }
        let held: i32 = sqlx::query_scalar("SELECT chests FROM economy_players WHERE user_id=$1")
            .bind(u as i64)
            .fetch_one(db.pool())
            .await
            .unwrap();
        assert_eq!(held, if n == 0 { 1 } else { 10 });
        if n == 0 {
            let (a, b) = tokio::join!(
                db.economy_action(u, "chest-open", &A::Chest, time),
                db.economy_action(u, "chest-open", &A::Chest, time)
            );
            assert_eq!(a.unwrap(), b.unwrap());
        }
    }
    let entries: Vec<(String, String, i64)> = sqlx::query_as(
        "SELECT reason,account,delta FROM economy_ledger WHERE user_id=$1 AND asset='chests'
         ORDER BY reason,account",
    )
    .bind(u as i64)
    .fetch_all(db.pool())
    .await
    .unwrap();
    assert_eq!(
        entries,
        vec![
            ("chest_open".into(), "sink".into(), 1),
            ("chest_open".into(), "wallet".into(), -1),
            ("job_chest".into(), "source".into(), -1),
            ("job_chest".into(), "wallet".into(), 1),
        ]
    );
    ledger_matches(&db, u).await;
}
