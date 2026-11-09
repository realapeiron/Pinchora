use std::time::Duration;

use pinchora_core::progression::leveling::week_start;
use pinchora_core::progression::leveling_policy::Curve;
use pinchora_core::progression::leveling_policy::LevelingPolicy;
use pinchora_db::Db;

#[tokio::test]
async fn policy_voice_ledger_and_leave_reset() {
    let _isolation = crate::support::shared().await;
    let Ok(url) = std::env::var("PINCHORA_TEST_DATABASE_URL") else {
        return;
    };
    let db = Db::connect_waiting(&url, 4, Duration::from_secs(60)).await.unwrap();
    db.migrate().await.unwrap();
    let base = crate::support::id();
    let (guild, user) = (base, base + 1);
    db.guild_settings(guild).await.unwrap();
    let mut config = LevelingPolicy {
        curve: Curve::Linear {
            base: 100,
            per_level: 25,
        },
        ..Default::default()
    };
    config.vanity_slug = Some(format!("level-{guild}"));
    db.save_leveling_policy(guild, &config).await.unwrap();
    assert_eq!(db.leveling_policy(guild).await.unwrap(), config);
    assert_eq!(
        db.vanity_guild(config.vanity_slug.as_deref().unwrap()).await.unwrap(),
        Some(guild)
    );
    let week = week_start(chrono::Utc::now());
    let separate = db.add_separate_voice_xp(guild, &[(user, 120)], week).await.unwrap();
    assert_eq!(separate[0].1.old_xp, 0);
    assert_eq!(separate[0].1.new_xp, 120);
    assert!(db.member_rank(guild, user).await.unwrap().is_none());
    assert_eq!(db.voice_level_xp(guild, user).await.unwrap(), 120);
    let shared = db.add_voice_xp(guild, &[(user, 15)], week).await.unwrap();
    assert_eq!(shared[0].1.new_xp, 15);
    assert_eq!(db.voice_level_xp(guild, user).await.unwrap(), 135);
    assert_eq!(db.voice_level_board(guild, 10, 0).await.unwrap()[0].value, 135);
    db.reset_departed_level(guild, user).await.unwrap();
    assert_eq!(db.member_rank(guild, user).await.unwrap().unwrap().xp, 0);
    assert_eq!(db.voice_level_xp(guild, user).await.unwrap(), 0);
    assert!(db.weekly_leaderboard(guild, week, 10, 0).await.unwrap().is_empty());
}

async fn fixture() -> Option<(Db, u64, u64)> {
    let url = std::env::var("PINCHORA_TEST_DATABASE_URL").ok()?;
    let db = Db::connect_waiting(&url, 4, Duration::from_secs(60)).await.unwrap();
    db.migrate().await.unwrap();
    let base = crate::support::id();
    db.guild_settings(base).await.unwrap();
    Some((db, base, base + 1))
}

#[tokio::test]
async fn reactions_are_once_even_across_connections_and_after_leave_reset() {
    let _isolation = crate::support::shared().await;
    let Some((db, guild, user)) = fixture().await else {
        return;
    };
    let week = week_start(chrono::Utc::now());
    let (a, b) = tokio::join!(
        db.add_reaction_xp(guild, user, 42, 5, week),
        db.add_reaction_xp(guild, user, 42, 5, week)
    );
    assert_eq!(usize::from(a.unwrap().is_some()) + usize::from(b.unwrap().is_some()), 1);
    assert_eq!(db.member_rank(guild, user).await.unwrap().unwrap().xp, 5);
    assert_eq!(db.weekly_leaderboard(guild, week, 10, 0).await.unwrap()[0].value, 5);
    db.reset_departed_level(guild, user).await.unwrap();
    assert!(db.add_reaction_xp(guild, user, 42, 5, week).await.unwrap().is_none());
    assert_eq!(db.member_rank(guild, user).await.unwrap().unwrap().xp, 0);
}

#[tokio::test]
async fn leave_grace_preserves_short_rejoins_and_expires_once() {
    let _isolation = crate::support::shared().await;
    let Some((db, guild, user)) = fixture().await else {
        return;
    };
    let config = LevelingPolicy {
        reset_on_leave: true,
        leave_grace_secs: 300,
        ..Default::default()
    };
    db.save_leveling_policy(guild, &config).await.unwrap();
    let now = chrono::Utc::now();
    let week = week_start(now);
    db.add_voice_xp(guild, &[(user, 255)], week).await.unwrap();
    db.schedule_level_reset(guild, user, now + chrono::Duration::seconds(300))
        .await
        .unwrap();
    assert!(
        !db.finish_level_departure(guild, user, now + chrono::Duration::seconds(60), true)
            .await
            .unwrap()
    );
    assert_eq!(db.member_rank(guild, user).await.unwrap().unwrap().xp, 255);
    assert!(
        !db.finish_level_departure(guild, user, now + chrono::Duration::hours(1), false)
            .await
            .unwrap()
    );
    db.schedule_level_reset(guild, user, now).await.unwrap();
    let (a, b) = tokio::join!(
        db.finish_level_departure(guild, user, now, false),
        db.finish_level_departure(guild, user, now, false)
    );
    assert_ne!(a.unwrap(), b.unwrap());
    assert_eq!(db.member_rank(guild, user).await.unwrap().unwrap().xp, 0);
    assert_eq!(db.voice_level_xp(guild, user).await.unwrap(), 0);
    assert!(db.weekly_leaderboard(guild, week, 10, 0).await.unwrap().is_empty());
}

#[tokio::test]
async fn imports_commit_with_audit_and_replay_cannot_overwrite_new_xp() {
    let _isolation = crate::support::shared().await;
    let Some((db, guild, user)) = fixture().await else {
        return;
    };
    let rows = [(user, 255)];
    let audit = pinchora_db::NewAudit {
        guild,
        actor: user,
        actor_name: "admin",
        source: pinchora_db::AuditSource::Web,
        action: "xp.import.offline",
        target: Some("fixture"),
        before: None,
        after: None,
    };
    let bad = pinchora_db::NewAudit {
        before: Some("invalid JSON"),
        ..audit.clone()
    };
    assert!(db.apply_level_import(&rows, true, &bad).await.is_err());
    assert!(db.member_rank(guild, user).await.unwrap().is_none());
    let (a, b) = tokio::join!(
        db.apply_level_import(&rows, true, &audit),
        db.apply_level_import(&rows, true, &audit)
    );
    let (a, b) = (a.unwrap(), b.unwrap());
    assert_eq!((a.0, b.0), (1, 1));
    assert_ne!(a.1, b.1);
    assert_eq!(db.audit_page(guild, None, 10).await.unwrap().len(), 1);
    db.edit_member_xp(guild, user, pinchora_db::XpEdit::Add, 100)
        .await
        .unwrap();
    assert_eq!(db.apply_level_import(&rows, true, &audit).await.unwrap(), (1, false));
    assert_eq!(db.member_rank(guild, user).await.unwrap().unwrap().xp, 355);
}

#[tokio::test]
async fn changing_curve_preserves_xp_and_recomputes_achievement_level() {
    let _isolation = crate::support::shared().await;
    let Some((db, guild, user)) = fixture().await else {
        return;
    };
    db.edit_member_xp(guild, user, pinchora_db::XpEdit::Set, 255)
        .await
        .unwrap();
    assert_eq!(db.leveling_policy(guild).await.unwrap().level_of(255), 2);
    let config = LevelingPolicy {
        curve: Curve::Linear { base: 10, per_level: 0 },
        ..Default::default()
    };
    db.save_leveling_policy(guild, &config).await.unwrap();
    let xp = db.member_rank(guild, user).await.unwrap().unwrap().xp;
    assert_eq!(xp, 255);
    assert_eq!(db.leveling_policy(guild).await.unwrap().level_of(xp), 25);
    assert_eq!(
        db.achievement_progress(guild, user, pinchora_core::progression::achievements::Kind::Level)
            .await
            .unwrap(),
        25
    );
}

#[tokio::test]
async fn concurrent_voice_role_configuration_respects_the_limit() {
    let _isolation = crate::support::shared().await;
    let Some((db, guild, _)) = fixture().await else {
        return;
    };
    for role in 1..=24 {
        assert!(db.add_voice_level_role(guild, role, 1).await.unwrap());
    }
    let (a, b) = tokio::join!(
        db.add_voice_level_role(guild, 25, 1),
        db.add_voice_level_role(guild, 26, 1)
    );
    assert_ne!(a.unwrap(), b.unwrap());
    assert_eq!(db.voice_level_roles(guild).await.unwrap().len(), 25);
}

#[tokio::test]
async fn deleted_discord_roles_are_removed_from_voice_rewards() {
    let _isolation = crate::support::shared().await;
    let Some((db, guild, _)) = fixture().await else {
        return;
    };
    db.add_voice_level_role(guild, 42, 1).await.unwrap();
    db.forget_role(guild, 42).await.unwrap();
    assert!(db.voice_level_roles(guild).await.unwrap().is_empty());
}
