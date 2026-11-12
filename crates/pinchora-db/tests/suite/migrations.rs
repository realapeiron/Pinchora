//! An existing installation must pick up missing older feature migrations.
use sqlx::{Connection, PgConnection, migrate::Migrator};

#[tokio::test]
async fn command_control_switch_upgrade_keeps_existing_servers_on() {
    let _isolation = crate::support::shared().await;
    let Ok(url) = std::env::var("PINCHORA_TEST_DATABASE_URL") else {
        return;
    };
    let mut conn = PgConnection::connect(&url).await.unwrap();
    let mut tx = conn.begin().await.unwrap();
    sqlx::query("CREATE SCHEMA command_switch_regression")
        .execute(&mut *tx)
        .await
        .unwrap();
    sqlx::query("SET LOCAL search_path TO command_switch_regression")
        .execute(&mut *tx)
        .await
        .unwrap();
    let all = sqlx::migrate!("../../migrations");
    Migrator::with_migrations(all.iter().filter(|m| m.version <= 87).cloned().collect())
        .run(&mut *tx)
        .await
        .unwrap();
    sqlx::query("INSERT INTO guilds(id) VALUES(1)")
        .execute(&mut *tx)
        .await
        .unwrap();
    sqlx::query("INSERT INTO guild_modules(guild_id,module,enabled,channel_mode,channels) VALUES(1,'command_control',false,'all','{}')")
        .execute(&mut *tx).await.unwrap();
    all.run(&mut *tx).await.unwrap();
    let enabled: bool =
        sqlx::query_scalar("SELECT enabled FROM guild_modules WHERE guild_id=1 AND module='command_control'")
            .fetch_one(&mut *tx)
            .await
            .unwrap();
    assert!(enabled);
    tx.rollback().await.unwrap();
}

#[tokio::test]
async fn automod_migrates_after_newer_versions() {
    let _isolation = crate::support::shared().await;
    let Ok(url) = std::env::var("PINCHORA_TEST_DATABASE_URL") else {
        return;
    };
    let mut conn = PgConnection::connect(&url).await.expect("connect");
    // Roll back the isolated schema even if an assertion fails; never modify the live schema.
    let mut tx = conn.begin().await.unwrap();
    sqlx::query("CREATE SCHEMA automod_migration_regression")
        .execute(&mut *tx)
        .await
        .unwrap();
    sqlx::query("SET LOCAL search_path TO automod_migration_regression")
        .execute(&mut *tx)
        .await
        .unwrap();
    let all = sqlx::migrate!("../../migrations");
    let old_main = Migrator::with_migrations(
        all.iter()
            .filter(|m| m.version != 45 && m.version <= 53)
            .cloned()
            .collect(),
    );
    old_main.run(&mut *tx).await.unwrap();
    let highest: i64 = sqlx::query_scalar("SELECT max(version) FROM _sqlx_migrations")
        .fetch_one(&mut *tx)
        .await
        .unwrap();
    assert_eq!(highest, 53);
    all.run(&mut *tx)
        .await
        .expect("apply missing 0045 after 0046, 0047 and 0053");
    let applied: bool = sqlx::query_scalar("SELECT success FROM _sqlx_migrations WHERE version = 45")
        .fetch_one(&mut *tx)
        .await
        .unwrap();
    assert!(applied);
    sqlx::query("SELECT 1 FROM automod_settings LIMIT 1")
        .execute(&mut *tx)
        .await
        .expect("AutoMod schema exists");
    all.run(&mut *tx).await.expect("migration is idempotent");
    tx.rollback().await.unwrap();
}

#[tokio::test]
async fn command_domains_upgrade_applied_0066_and_preserve_constraints() {
    let _isolation = crate::support::shared().await;
    use pinchora_core::admin::command_control::Config;
    use serde_json::json;
    let Ok(url) = std::env::var("PINCHORA_TEST_DATABASE_URL") else {
        return;
    };
    let mut conn = PgConnection::connect(&url).await.expect("connect");
    let mut tx = conn.begin().await.unwrap();
    sqlx::query("CREATE SCHEMA command_domains_regression")
        .execute(&mut *tx)
        .await
        .unwrap();
    sqlx::query("SET LOCAL search_path TO command_domains_regression")
        .execute(&mut *tx)
        .await
        .unwrap();
    let all = sqlx::migrate!("../../migrations");
    let before = Migrator::with_migrations(
        all.iter()
            .filter(|m| m.version <= 67 && m.version != 66)
            .cloned()
            .collect(),
    );
    before.run(&mut *tx).await.unwrap();
    let consolidated = Migrator::with_migrations(all.iter().filter(|m| m.version <= 67).cloned().collect());
    consolidated.run(&mut *tx).await.expect("0066 still applies after 0067");
    let config = json!({"rules": {
        "setup": {"cooldown_secs":12},
        "feeds": {"denied_channels":["2"]},
        "setup feeds list": {"allowed_roles":["3"]},
        "integrations feeds list": {"allowed_roles":["4"]}
    }, "aliases":{"feed":"setup feeds", "old":"feeds list"}});
    sqlx::query("INSERT INTO command_control(guild_id,config) VALUES(1,$1)")
        .bind(config)
        .execute(&mut *tx)
        .await
        .unwrap();
    all.run(&mut *tx).await.expect("0068 after applied 0066");
    let read: serde_json::Value = sqlx::query_scalar("SELECT config FROM command_control WHERE guild_id=1")
        .fetch_one(&mut *tx)
        .await
        .unwrap();
    let migrated: Config = serde_json::from_value(read.clone()).unwrap();
    assert!(!migrated.rules.contains_key("setup"));
    assert!(!migrated.rules.contains_key("setup feeds list"));
    let rule = &migrated.rules["integrations feeds list"];
    assert_eq!(rule.allowed_roles, ["4"]);
    assert_eq!(rule.constraints["setup feeds list"].allowed_roles, ["3"]);
    assert_eq!(rule.constraints["feeds"].denied_channels, ["2"]);
    assert_eq!(rule.constraints["setup"].cooldown_secs, 12);
    assert!(rule.constraints.values().all(|r| r.constraints.is_empty()));
    assert_eq!(migrated.aliases["feed"], "integrations feeds");
    assert_eq!(migrated.aliases["old"], "integrations feeds list");
    assert!(!migrated.rules.contains_key("setup server show"));
    assert!(!migrated.rules.contains_key("mod lock"));
    all.run(&mut *tx).await.unwrap();
    sqlx::query("UPDATE command_control SET config=config WHERE guild_id=1")
        .execute(&mut *tx)
        .await
        .unwrap();
    let again: serde_json::Value = sqlx::query_scalar("SELECT config FROM command_control WHERE guild_id=1")
        .fetch_one(&mut *tx)
        .await
        .unwrap();
    assert_eq!(again, read, "on-write normalization is idempotent");
    tx.rollback().await.unwrap();
}

#[tokio::test]
async fn leveling_0077_preserves_existing_xp_and_levels() {
    let _isolation = crate::support::shared().await;
    use pinchora_core::progression::leveling_policy::LevelingPolicy;
    let Ok(url) = std::env::var("PINCHORA_TEST_DATABASE_URL") else {
        return;
    };
    let mut conn = PgConnection::connect(&url).await.unwrap();
    let mut tx = conn.begin().await.unwrap();
    sqlx::query("CREATE SCHEMA leveling_upgrade_fixture")
        .execute(&mut *tx)
        .await
        .unwrap();
    sqlx::query("SET LOCAL search_path TO leveling_upgrade_fixture")
        .execute(&mut *tx)
        .await
        .unwrap();
    let all = sqlx::migrate!("../../migrations");
    let before = Migrator::with_migrations(all.iter().filter(|m| m.version < 77).cloned().collect());
    before.run(&mut *tx).await.unwrap();
    // Golden current-schema fixture: level boundaries and an established high-XP member.
    let fixture = [
        (11_i64, 0_i64, 0),
        (12, 99, 0),
        (13, 100, 1),
        (14, 254, 1),
        (15, 255, 2),
        (16, 474, 2),
        (17, 475, 3),
        (18, 1_000_000, 79),
    ];
    sqlx::query("INSERT INTO guilds(id) VALUES (100),(200)")
        .execute(&mut *tx)
        .await
        .unwrap();
    for (user, xp, level) in fixture {
        assert_eq!(pinchora_core::progression::leveling::level_of(xp), level);
        sqlx::query("INSERT INTO users(id,xp) VALUES($1,$2)")
            .bind(user)
            .bind(xp)
            .execute(&mut *tx)
            .await
            .unwrap();
        sqlx::query("INSERT INTO members(guild_id,user_id,xp) VALUES(100,$1,$2),(200,$1,$2)")
            .bind(user)
            .bind(xp)
            .execute(&mut *tx)
            .await
            .unwrap();
    }
    let snapshot: Vec<(i64, i64, i64)> =
        sqlx::query_as("SELECT guild_id,user_id,xp FROM members ORDER BY guild_id,user_id")
            .fetch_all(&mut *tx)
            .await
            .unwrap();
    all.run(&mut *tx).await.unwrap();
    let after: Vec<(i64, i64, i64)> =
        sqlx::query_as("SELECT guild_id,user_id,xp FROM members ORDER BY guild_id,user_id")
            .fetch_all(&mut *tx)
            .await
            .unwrap();
    assert_eq!(after, snapshot);
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM guild_leveling_parity")
            .fetch_one(&mut *tx)
            .await
            .unwrap(),
        0
    );
    let defaults: LevelingPolicy = serde_json::from_str("{}").unwrap();
    for (_, user, xp) in after {
        let expected = fixture.iter().find(|r| r.0 == user).unwrap().2;
        assert_eq!(defaults.level_of(xp), expected);
        assert_eq!(
            defaults.progress(xp),
            pinchora_core::progression::leveling::progress(xp)
        );
    }
    let global: Vec<(i64, i64)> = sqlx::query_as("SELECT id,xp FROM users ORDER BY id")
        .fetch_all(&mut *tx)
        .await
        .unwrap();
    assert_eq!(global, fixture.iter().map(|(id, xp, _)| (*id, *xp)).collect::<Vec<_>>());
    tx.rollback().await.unwrap();
}

#[tokio::test]
async fn community_0079_preserves_existing_live_rows() {
    let _isolation = crate::support::shared().await;
    let Ok(url) = std::env::var("PINCHORA_TEST_DATABASE_URL") else {
        return;
    };
    let mut conn = PgConnection::connect(&url).await.unwrap();
    let mut tx = conn.begin().await.unwrap();
    sqlx::query("CREATE SCHEMA community_upgrade_fixture")
        .execute(&mut *tx)
        .await
        .unwrap();
    sqlx::query("SET LOCAL search_path TO community_upgrade_fixture")
        .execute(&mut *tx)
        .await
        .unwrap();
    let all = sqlx::migrate!("../../migrations");
    Migrator::with_migrations(all.iter().filter(|m| m.version < 79).cloned().collect())
        .run(&mut *tx)
        .await
        .unwrap();
    sqlx::raw_sql("INSERT INTO giveaways(id,guild_id,channel_id,host_id,prize,shells,winners,ends_at) VALUES(1,10,20,30,'existing prize',0,1,now()); INSERT INTO giveaway_entries(giveaway_id,user_id) VALUES(1,40); INSERT INTO polls(id,guild_id,channel_id,author_id,question,options,ends_at) VALUES(1,10,20,30,'existing poll',ARRAY['yes','no'],now()); INSERT INTO poll_votes(poll_id,user_id,choice) VALUES(1,40,1); INSERT INTO suggestions(guild_id,author_id,number,body) VALUES(10,30,1,'existing idea');")
        .execute(&mut *tx).await.unwrap();
    all.run(&mut *tx).await.unwrap();
    let row:(String,i16,i64)=sqlx::query_as("SELECT prize,weight,(SELECT count(*) FROM poll_votes) FROM giveaways JOIN giveaway_entries ON giveaway_id=giveaways.id WHERE giveaways.id=1").fetch_one(&mut *tx).await.unwrap();
    assert_eq!(row, ("existing prize".into(), 1, 1));
    sqlx::query("UPDATE giveaways SET status='scheduled',starts_at=now()+interval '1 day' WHERE id=1")
        .execute(&mut *tx)
        .await
        .unwrap();
    tx.rollback().await.unwrap();
}

#[tokio::test]
async fn reef_wave7_upgrades_populated_tables_without_rewrites_or_live_index_builds() {
    let _isolation = crate::support::shared().await;
    let Ok(url) = std::env::var("PINCHORA_TEST_DATABASE_URL") else {
        return;
    };
    let mut conn = PgConnection::connect(&url).await.unwrap();
    let mut tx = conn.begin().await.unwrap();
    sqlx::query("CREATE SCHEMA reef_upgrade_regression")
        .execute(&mut *tx)
        .await
        .unwrap();
    sqlx::query("SET LOCAL search_path TO reef_upgrade_regression")
        .execute(&mut *tx)
        .await
        .unwrap();
    let all = sqlx::migrate!("../../migrations");
    let before = Migrator::with_migrations(all.iter().filter(|m| m.version < 82).cloned().collect());
    before.run(&mut *tx).await.unwrap();
    sqlx::query("INSERT INTO users(id) SELECT generate_series(1,1000)")
        .execute(&mut *tx)
        .await
        .unwrap();
    sqlx::query("INSERT INTO houses(user_id) SELECT generate_series(1,1000)")
        .execute(&mut *tx)
        .await
        .unwrap();
    sqlx::query("INSERT INTO reef_chat_evidence(owner_id,room_idx,sender_id,text) SELECT 1,0,n,'Hello' FROM generate_series(1,1000) n").execute(&mut *tx).await.unwrap();
    let files: Vec<(String,i64)> = sqlx::query_as("SELECT relname::text,relfilenode::bigint FROM pg_class WHERE oid IN ('houses'::regclass,'reef_chat_evidence'::regclass) ORDER BY relname")
        .fetch_all(&mut *tx).await.unwrap();
    // Check 0082's own timeout contract before later migrations set theirs.
    let reef = Migrator::with_migrations(all.iter().filter(|m| m.version <= 82).cloned().collect());
    reef.run(&mut *tx).await.unwrap();
    let after: Vec<(String,i64)> = sqlx::query_as("SELECT relname::text,relfilenode::bigint FROM pg_class WHERE oid IN ('houses'::regclass,'reef_chat_evidence'::regclass) ORDER BY relname")
        .fetch_all(&mut *tx).await.unwrap();
    assert_eq!(files, after, "no rewrite of live tables");
    let (count, revision): (i64, i64) = sqlx::query_as("SELECT count(*),sum(edit_revision)::bigint FROM houses")
        .fetch_one(&mut *tx)
        .await
        .unwrap();
    assert_eq!((count, revision), (1000, 0));
    let indexes: Vec<String> = sqlx::query_scalar(
        "SELECT indexname::text FROM pg_indexes WHERE schemaname=current_schema() AND tablename='reef_doorbells'",
    )
    .fetch_all(&mut *tx)
    .await
    .unwrap();
    for expected in [
        "reef_doorbells_pair",
        "reef_doorbells_guest_created",
        "reef_doorbells_host_pending",
        "reef_doorbells_retention",
    ] {
        assert!(indexes.iter().any(|i| i == expected), "{expected}");
    }
    let lock_timeout: String = sqlx::query_scalar("SHOW lock_timeout")
        .fetch_one(&mut *tx)
        .await
        .unwrap();
    let statement_timeout: String = sqlx::query_scalar("SHOW statement_timeout")
        .fetch_one(&mut *tx)
        .await
        .unwrap();
    assert_eq!(lock_timeout, "2s");
    assert_eq!(statement_timeout, "15s");
    let evidence: i64 = sqlx::query_scalar("SELECT count(*) FROM reef_chat_evidence")
        .fetch_one(&mut *tx)
        .await
        .unwrap();
    assert_eq!(evidence, 1000);
    all.run(&mut *tx).await.unwrap();
    tx.rollback().await.unwrap();
}
