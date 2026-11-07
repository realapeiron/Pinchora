use pinchora_core::{
    command_control::{Config, Rule},
    config_backup::{self, Names, Policy},
};
use pinchora_db::Db;
use serde_json::json;
use std::{collections::BTreeMap, time::Duration};
async fn db() -> Option<Db> {
    let url = std::env::var("PINCHORA_TEST_DATABASE_URL").ok()?;
    let db = Db::connect_waiting(&url, 5, Duration::from_secs(30))
        .await
        .expect("database");
    db.migrate().await.expect("migrate");
    Some(db)
}
fn unique() -> u64 {
    (std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos() as u64)
        >> 4
}
#[tokio::test]
async fn palette_swaps_are_atomic_and_duplicate_colours_fail_read_only_validation() {
    let Some(db) = db().await else { return };
    let guild = unique();
    db.guild_settings(guild).await.unwrap();
    sqlx::query(
        "INSERT INTO color_roles(guild_id,role_id,hex,name) VALUES($1,100,'#ff0000','Red'),($1,101,'#0000ff','Blue')",
    )
    .bind(guild as i64)
    .execute(db.pool())
    .await
    .unwrap();
    let before = db.export_config(guild, Names::default()).await.unwrap();
    let mut swapped = before.clone();
    swapped.sections.get_mut("color_roles").unwrap()[0]["hex"] = json!("#0000ff");
    swapped.sections.get_mut("color_roles").unwrap()[1]["hex"] = json!("#ff0000");
    let point = db
        .import_config(guild, &swapped, Policy::Merge, Names::default())
        .await
        .unwrap();
    assert_eq!(db.export_config(guild, Names::default()).await.unwrap(), swapped);
    assert_eq!(db.config_restore_point(guild, point).await.unwrap().unwrap(), before);
    let mut replaced = swapped.clone();
    let rows = replaced.sections.get_mut("color_roles").unwrap();
    rows.truncate(1);
    rows[0]["role_id"] = json!("102");
    db.import_config(guild, &replaced, Policy::Replace, Names::default())
        .await
        .unwrap();
    assert_eq!(db.export_config(guild, Names::default()).await.unwrap(), replaced);
    let mut invalid = replaced.clone();
    let mut duplicate = invalid.sections["color_roles"][0].clone();
    duplicate["role_id"] = json!("103");
    invalid.sections.get_mut("color_roles").unwrap().push(duplicate);
    let points = db.config_restore_points(guild).await.unwrap();
    assert!(config_backup::validate_shape(&invalid, pinchora_core::i18n::Lang::En).is_err());
    assert_eq!(db.config_restore_points(guild).await.unwrap(), points);
    assert_eq!(db.export_config(guild, Names::default()).await.unwrap(), replaced);
}
#[tokio::test]
async fn round_trip_dry_read_restore_and_privacy() {
    let Some(db) = db().await else {
        return;
    };
    let guild = unique();
    db.guild_settings(guild).await.unwrap();
    let config = Config {
        rules: BTreeMap::from([(
            "play wordle".into(),
            Rule {
                enabled: false,
                ..Rule::default()
            },
        )]),
        aliases: BTreeMap::from([("w".into(), "play wordle".into())]),
    };
    db.set_command_control(guild, &config).await.unwrap();
    sqlx::query("INSERT INTO verification(guild_id,exempt_users) VALUES($1,ARRAY[123456789]::bigint[])")
        .bind(guild as i64)
        .execute(db.pool())
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO custom_commands(guild_id,name,responses,created_by) VALUES($1,'hello',ARRAY['Hello!'],123456789)",
    )
    .bind(guild as i64)
    .execute(db.pool())
    .await
    .unwrap();
    let panel: i64 = sqlx::query_scalar("INSERT INTO role_panels(guild_id,title) VALUES($1,'Roles') RETURNING id")
        .bind(guild as i64)
        .fetch_one(db.pool())
        .await
        .unwrap();
    sqlx::query("INSERT INTO role_panel_roles(panel_id,role_id,label,position) VALUES($1,987654321,'Member',0)")
        .bind(panel)
        .execute(db.pool())
        .await
        .unwrap();
    sqlx::query("INSERT INTO role_panels(guild_id,title) VALUES($1,'Other roles')")
        .bind(guild as i64)
        .execute(db.pool())
        .await
        .unwrap();
    sqlx::query("INSERT INTO custom_responders(guild_id,name,trigger_kind,responses,created_by) VALUES($1,'Greeting','member_join',ARRAY['Welcome!'],123456789),($1,'Greeting','member_leave',ARRAY['Goodbye!'],123456789)")
        .bind(guild as i64).execute(db.pool()).await.unwrap();
    let first = db.export_config(guild, Names::default()).await.unwrap();
    config_backup::validate_shape(&first, pinchora_core::i18n::Lang::En).unwrap();
    db.validate_backup_types(&first).await.unwrap();
    assert!(
        db.config_restore_points(guild).await.unwrap().is_empty(),
        "read-only validation never writes"
    );
    let encoded = serde_json::to_string(&first).unwrap();
    for secret in ["123456789", "created_by", "exempt_users", "tokens"] {
        assert!(!encoded.contains(secret), "{secret} leaked");
    }
    let point = db
        .import_config(guild, &first, Policy::Replace, Names::default())
        .await
        .unwrap();
    let second = db.export_config(guild, Names::default()).await.unwrap();
    assert_eq!(first, second);
    assert_eq!(db.config_restore_point(guild, point).await.unwrap().unwrap(), first);
    assert!(db.config_restore_point(guild + 1, point).await.unwrap().is_none());
    db.set_command_control(guild, &Config::default()).await.unwrap();
    db.import_config(guild, &first, Policy::Replace, Names::default())
        .await
        .unwrap();
    assert_eq!(db.command_control(guild).await.unwrap(), config);
    for _ in 0..10 {
        db.import_config(guild, &first, Policy::Merge, Names::default())
            .await
            .unwrap();
    }
    assert_eq!(db.config_restore_points(guild).await.unwrap().len(), 10);
}
#[tokio::test]
async fn import_remaps_roles_channels_and_local_panel_ids() {
    let Some(db) = db().await else {
        return;
    };
    let guild = unique();
    let target = guild + 1;
    db.guild_settings(guild).await.unwrap();
    sqlx::query("INSERT INTO level_roles(guild_id,role_id,level) VALUES($1,100,3)")
        .bind(guild as i64)
        .execute(db.pool())
        .await
        .unwrap();
    sqlx::query("UPDATE guilds SET welcome_channel_id=200 WHERE id=$1")
        .bind(guild as i64)
        .execute(db.pool())
        .await
        .unwrap();
    let panel: i64 = sqlx::query_scalar("INSERT INTO role_panels(guild_id,title) VALUES($1,'Roles') RETURNING id")
        .bind(guild as i64)
        .fetch_one(db.pool())
        .await
        .unwrap();
    sqlx::query("INSERT INTO role_panel_roles(panel_id,role_id,label,position) VALUES($1,100,'Member',0)")
        .bind(panel)
        .execute(db.pool())
        .await
        .unwrap();
    let names = Names {
        roles: BTreeMap::from([("100".into(), "Member".into())]),
        channels: BTreeMap::from([("200".into(), "welcome".into())]),
    };
    let mut backup = db.export_config(guild, names).await.unwrap();
    db.guild_settings(target).await.unwrap();
    let target_panel: i64 =
        sqlx::query_scalar("INSERT INTO role_panels(guild_id,title) VALUES($1,'roles') RETURNING id")
            .bind(target as i64)
            .fetch_one(db.pool())
            .await
            .unwrap();
    let target_names = Names {
        roles: BTreeMap::from([("101".into(), "Member".into())]),
        channels: BTreeMap::from([("201".into(), "welcome".into())]),
    };
    assert!(config_backup::remap(&mut backup, target, &target_names).is_empty());
    db.import_config(target, &backup, Policy::Replace, target_names.clone())
        .await
        .unwrap();
    let imported = db.export_config(target, target_names).await.unwrap();
    assert_eq!(imported.sections["level_roles"][0]["role_id"], "101");
    assert_eq!(imported.sections["guilds"][0]["welcome_channel_id"], "201");
    assert_ne!(imported.sections["role_panels"][0]["id"], json!(panel.to_string()));
    assert_eq!(
        imported.sections["role_panels"][0]["id"],
        json!(target_panel.to_string())
    );
    assert_eq!(
        imported.sections["role_panel_roles"][0]["panel_id"],
        imported.sections["role_panels"][0]["id"]
    );
}

#[tokio::test]
async fn exports_and_restore_downloads_redact_feed_credentials_without_mutating_the_source() {
    let Some(db) = db().await else { return };
    let guild = unique();
    db.guild_settings(guild).await.unwrap();
    let source = format!("https://example.com/{guild}/rss?access_token=private-feed-token&format=atom");
    sqlx::query("INSERT INTO feed_sources(kind,source) VALUES('rss',$1)")
        .bind(&source)
        .execute(db.pool())
        .await
        .unwrap();
    sqlx::query("INSERT INTO feeds(guild_id,kind,source,channel_id) VALUES($1,'rss',$2,42)")
        .bind(guild as i64)
        .bind(&source)
        .execute(db.pool())
        .await
        .unwrap();
    let exported = db.export_config(guild, Names::default()).await.unwrap();
    assert!(!serde_json::to_string(&exported).unwrap().contains("private-feed-token"));
    assert_eq!(exported.sections["feeds"][0]["enabled"], false);
    assert!(
        exported.sections["feeds"][0]["source"]
            .as_str()
            .unwrap()
            .contains("format=atom")
    );
    let stored: String = sqlx::query_scalar("SELECT source FROM feeds WHERE guild_id=$1")
        .bind(guild as i64)
        .fetch_one(db.pool())
        .await
        .unwrap();
    assert_eq!(stored, source, "redaction must not mutate the live subscription");
    // An unrelated merge creates an automatic snapshot; downloading it uses the same redaction.
    let mut incoming = exported;
    incoming.sections.get_mut("feeds").unwrap().clear();
    let point = db
        .import_config(guild, &incoming, Policy::Merge, Names::default())
        .await
        .unwrap();
    let restore = db.config_restore_point(guild, point).await.unwrap().unwrap();
    assert!(!serde_json::to_string(&restore).unwrap().contains("private-feed-token"));
}

#[tokio::test]
async fn utility_event_backup_round_trip_remaps_channels_and_excludes_member_state() {
    let Some(db) = db().await else { return };
    let guild = unique();
    let target = guild + 1;
    let channel = guild + 2;
    let target_channel = guild + 3;
    db.guild_settings(guild).await.unwrap();
    for query in [
        "INSERT INTO utility_settings(guild_id,timezone) VALUES($1,'Europe/Istanbul')",
        "INSERT INTO event_settings(guild_id,timezone) VALUES($1,'Europe/Berlin')",
        "INSERT INTO afk(guild_id,user_id,message) VALUES($1,123456789,'private AFK reason')",
    ] {
        sqlx::query(query).bind(guild as i64).execute(db.pool()).await.unwrap();
    }
    sqlx::query(
        "INSERT INTO sticky_messages(guild_id,channel_id,content,updated_by) VALUES($1,$2,'Sticky note',123456789)",
    )
    .bind(guild as i64)
    .bind(channel as i64)
    .execute(db.pool())
    .await
    .unwrap();
    sqlx::query("INSERT INTO scheduled_messages(guild_id,channel_id,content,schedule,next_run_at,created_by,ping_allowed) VALUES($1,$2,'Scheduled note','{\"kind\":\"interval\",\"seconds\":3600}',now()+interval '1 day',123456789,true)")
        .bind(guild as i64).bind(channel as i64).execute(db.pool()).await.unwrap();
    let names = Names {
        channels: BTreeMap::from([(channel.to_string(), "notes".into())]),
        ..Names::default()
    };
    let original = db.export_config(guild, names.clone()).await.unwrap();
    db.validate_backup_types(&original).await.unwrap();
    let encoded = serde_json::to_string(&original).unwrap();
    for excluded in [
        "123456789",
        "private AFK reason",
        "updated_by",
        "created_by",
        "ping_allowed",
        "last_message_id",
        "runs",
    ] {
        assert!(!encoded.contains(excluded), "{excluded} leaked");
    }
    let point = db
        .import_config(guild, &original, Policy::Replace, names.clone())
        .await
        .unwrap();
    assert_eq!(db.export_config(guild, names).await.unwrap(), original);
    assert_eq!(db.config_restore_point(guild, point).await.unwrap().unwrap(), original);
    let mut incoming = original.clone();
    let target_names = Names {
        channels: BTreeMap::from([(target_channel.to_string(), "notes".into())]),
        ..Names::default()
    };
    assert!(config_backup::remap(&mut incoming, target, &target_names).is_empty());
    db.import_config(target, &incoming, Policy::Replace, target_names.clone())
        .await
        .unwrap();
    let imported = db.export_config(target, target_names).await.unwrap();
    assert_eq!(imported.sections["utility_settings"][0]["timezone"], "Europe/Istanbul");
    assert_eq!(imported.sections["event_settings"][0]["timezone"], "Europe/Berlin");
    for table in ["sticky_messages", "scheduled_messages"] {
        assert_eq!(imported.sections[table][0]["channel_id"], target_channel.to_string());
    }
    assert_ne!(
        imported.sections["scheduled_messages"][0]["id"],
        original.sections["scheduled_messages"][0]["id"]
    );
    let allowed: i64 = sqlx::query_scalar("SELECT count(*) FROM scheduled_messages WHERE guild_id IN ($1,$2) AND (ping_allowed OR created_by IS NOT NULL)")
        .bind(guild as i64).bind(target as i64).fetch_one(db.pool()).await.unwrap();
    assert_eq!(allowed, 0);
}
