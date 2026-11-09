//! Runs against a real PostgreSQL when `PINCHORA_TEST_DATABASE_URL` is set; skipped otherwise.
//!
//! Module switches per server and channel, and the points range checked as the row is written.

use pinchora_core::{
    modules::{ChannelMode, Module, ModuleRule},
    settings::GuildSettingsPatch,
};
use pinchora_db::{Db, PatchRefused};

use crate::support::db_five as db;

/// Ids unique per run, so the suite can run repeatedly against one database.
use crate::support::ids;

async fn rows(db: &Db, guild: u64) -> i64 {
    let (n,): (i64,) = sqlx::query_as("SELECT count(*) FROM guild_modules WHERE guild_id = $1")
        .bind(guild as i64)
        .fetch_one(db.pool())
        .await
        .unwrap();
    n
}

fn rule_of(states: &[(Module, ModuleRule)], module: Module) -> ModuleRule {
    states
        .iter()
        .find(|(m, _)| *m == module)
        .map(|(_, r)| r.clone())
        .unwrap()
}

#[tokio::test]
async fn a_server_nobody_configured_has_everything_on_except_welcome_cards_and_gambling() {
    let _isolation = crate::support::shared().await;
    let Some(db) = db().await else { return };
    let [guild] = ids();
    let states = db.module_states(guild).await.unwrap();
    assert_eq!(states.len(), Module::ALL.len());
    for (module, rule) in &states {
        let off_by_default = matches!(module, Module::Gambling | Module::Welcome);
        assert_eq!(rule.enabled, !off_by_default, "{}", module.key());
        assert_eq!(
            (rule.mode, rule.channels.len()),
            (ChannelMode::All, 0),
            "{}",
            module.key()
        );
    }
    assert_eq!(rows(&db, guild).await, 0, "reading makes no rows");
    assert_eq!(db.guild_modules(guild).await.unwrap(), Default::default());
}

#[tokio::test]
async fn a_module_switch_and_its_channels_are_stored_together_and_a_default_rule_leaves_no_row() {
    let _isolation = crate::support::shared().await;
    let Some(db) = db().await else { return };
    let [guild, a, b] = ids();

    let off = db.set_module(guild, Module::Pets, Some(false), None).await.unwrap();
    assert!(!off.enabled);
    assert_eq!(rows(&db, guild).await, 1);
    // A switch kept off, with channels written later, keeps both.
    let both = db
        .set_module(guild, Module::Pets, None, Some((ChannelMode::Only, &[a, b])))
        .await
        .unwrap();
    assert_eq!(
        both,
        ModuleRule {
            enabled: false,
            mode: ChannelMode::Only,
            channels: vec![a, b]
        }
    );
    assert_eq!(db.guild_modules(guild).await.unwrap().rule(Module::Pets), both);
    // On everywhere again: no row is left behind.
    let back = db
        .set_module(guild, Module::Pets, Some(true), Some((ChannelMode::All, &[])))
        .await
        .unwrap();
    assert_eq!(back, ModuleRule::default());
    assert_eq!(rows(&db, guild).await, 0);
    // Changing nothing is allowed and changes nothing.
    assert_eq!(
        db.set_module(guild, Module::Pets, None, None).await.unwrap(),
        ModuleRule::default()
    );
    assert_eq!(rows(&db, guild).await, 0);

    // One server's rules are not another's.
    let [other] = ids();
    db.set_module(guild, Module::Fishing, Some(false), None).await.unwrap();
    assert!(rule_of(&db.module_states(other).await.unwrap(), Module::Fishing).enabled);
}

#[tokio::test]
async fn the_switches_that_live_in_settings_columns_stay_there() {
    let _isolation = crate::support::shared().await;
    let Some(db) = db().await else { return };
    let [guild, channel] = ids();

    // Leveling: the column holds the switch, the row only the channel rule.
    assert!(db.guild_settings(guild).await.unwrap().leveling_enabled);
    let rule = db
        .set_module(
            guild,
            Module::Leveling,
            Some(false),
            Some((ChannelMode::Except, &[channel])),
        )
        .await
        .unwrap();
    assert_eq!(
        rule,
        ModuleRule {
            enabled: false,
            mode: ChannelMode::Except,
            channels: vec![channel]
        }
    );
    assert!(!db.guild_settings(guild).await.unwrap().leveling_enabled);
    let (stored,): (bool,) =
        sqlx::query_as("SELECT enabled FROM guild_modules WHERE guild_id = $1 AND module = 'leveling'")
            .bind(guild as i64)
            .fetch_one(db.pool())
            .await
            .unwrap();
    assert!(stored, "the row never says off: the column does");
    // What the bot enforces reads the channel rule and treats the row's switch as on.
    let enforced = db.guild_modules(guild).await.unwrap().rule(Module::Leveling);
    assert!(enforced.enabled && enforced.mode == ChannelMode::Except);
    // The settings page flips the same column; the module list agrees.
    db.patch_guild_settings(
        guild,
        &GuildSettingsPatch {
            leveling_enabled: Some(true),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    assert!(rule_of(&db.module_states(guild).await.unwrap(), Module::Leveling).enabled);

    // Points, welcome and gambling the same way; gambling's column is the party games' own.
    for module in [Module::Points, Module::Welcome, Module::Gambling] {
        let on = db.set_module(guild, module, Some(true), None).await.unwrap();
        assert!(on.enabled, "{}", module.key());
        let off = db.set_module(guild, module, Some(false), None).await.unwrap();
        assert!(!off.enabled, "{}", module.key());
    }
    assert!(!db.gambling_enabled(guild).await.unwrap());
    db.set_module(guild, Module::Gambling, Some(true), None).await.unwrap();
    assert!(db.gambling_enabled(guild).await.unwrap());
    db.set_gambling(guild, false).await.unwrap();
    assert!(!rule_of(&db.module_states(guild).await.unwrap(), Module::Gambling).enabled);
    let settings = db.guild_settings(guild).await.unwrap();
    assert!(!settings.points_enabled && !settings.welcome_enabled);
    // A module with a column and no channel rule has no row at all.
    assert_eq!(rows(&db, guild).await, 1, "only leveling's channel rule");
}

#[tokio::test]
async fn a_patch_that_would_put_the_least_points_above_the_most_is_refused_as_the_row_is_written() {
    let _isolation = crate::support::shared().await;
    let Some(db) = db().await else { return };
    let [guild] = ids();
    let set = |min: Option<i64>, max: Option<i64>| GuildSettingsPatch {
        points_min: min,
        points_max: max,
        ..Default::default()
    };
    db.patch_guild_settings(guild, &set(Some(1), Some(500))).await.unwrap();
    // Another save lowered the most while this one was being checked against 500.
    db.patch_guild_settings(guild, &set(None, Some(100))).await.unwrap();
    let error = db.patch_guild_settings(guild, &set(Some(400), None)).await.unwrap_err();
    let refused = error
        .downcast_ref::<PatchRefused>()
        .expect("a refusal, not a database error");
    assert_eq!(refused.0.field, "points_min");
    let row = db.guild_settings(guild).await.unwrap();
    assert_eq!((row.points_min, row.points_max), (1, 100), "nothing was written");
    // Both ends in one patch are judged together.
    db.patch_guild_settings(guild, &set(Some(400), Some(900)))
        .await
        .unwrap();
    let row = db.guild_settings(guild).await.unwrap();
    assert_eq!((row.points_min, row.points_max), (400, 900));
    // Other fields never trip over the range.
    let name = GuildSettingsPatch {
        points_name: Some("coins".into()),
        ..Default::default()
    };
    assert_eq!(
        db.patch_guild_settings(guild, &name).await.unwrap().points_name,
        "coins"
    );
}
