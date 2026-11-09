use super::*;

#[tokio::test]
async fn xp_ranks_and_settings() {
    let _isolation = crate::support::shared().await;
    let Some(db) = db(5).await else { return };
    let [guild, u1, u2, u3] = ids();

    db.add_message_xp(guild, u1, 20, 0).await.unwrap();
    let gain = db.add_message_xp(guild, u1, 20, 0).await.unwrap();
    assert_eq!((gain.old_xp, gain.new_xp), (20, 40));
    db.add_message_xp(guild, u2, 100, 0).await.unwrap();
    db.add_message_xp(guild, u3, 5, 0).await.unwrap();

    let r1 = db.member_rank(guild, u1).await.unwrap().unwrap();
    assert_eq!((r1.xp, r1.messages, r1.rank), (40, 2, 2));
    assert!(db.member_rank(guild, 1).await.unwrap().is_none());
    assert_eq!(db.guild_member_count(guild).await.unwrap(), 3);

    let board = db.guild_leaderboard(guild, 10, 0).await.unwrap();
    let order: Vec<i64> = board.iter().map(|r| r.user_id).collect();
    assert_eq!(order, vec![u2 as i64, u1 as i64, u3 as i64]);
    assert_eq!(db.user(u1).await.unwrap().xp, 40, "global XP follows guild XP");

    let s = db.guild_settings(guild).await.unwrap();
    assert!(s.leveling_enabled && !s.welcome_enabled);
    let s = db
        .update_welcome(guild, Some(true), Some(123), Some("hi {user}"))
        .await
        .unwrap();
    assert!(s.welcome_enabled);
    assert_eq!(s.welcome_channel_id, Some(123));
    let s = db.update_welcome(guild, None, None, None).await.unwrap();
    assert_eq!(
        s.welcome_message.as_deref(),
        Some("hi {user}"),
        "unset options keep old values"
    );
    let s = db.update_leveling(guild, Some(false), None, None).await.unwrap();
    assert!(!s.leveling_enabled);
}

#[tokio::test]
async fn level_roles_are_capped_and_replaceable() {
    let _isolation = crate::support::shared().await;
    let Some(db) = db(5).await else { return };
    let [guild, other] = ids();

    assert!(db.level_roles(guild).await.unwrap().is_empty());
    assert!(db.add_level_role(guild, 10, 1_000).await.unwrap());
    assert!(db.add_level_role(guild, 5, 1_001).await.unwrap());
    assert!(
        db.add_level_role(guild, 5, 1_002).await.unwrap(),
        "roles can share a level"
    );
    let rows: Vec<_> = db
        .level_roles(guild)
        .await
        .unwrap()
        .iter()
        .map(|r| (r.level, r.role_id))
        .collect();
    assert_eq!(rows, vec![(5, 1_001), (5, 1_002), (10, 1_000)]);

    // Adding a role again moves it to the new level.
    assert!(db.add_level_role(guild, 20, 1_000).await.unwrap());
    assert_eq!(db.level_roles(guild).await.unwrap().last().unwrap().level, 20);

    // At the cap only new roles are refused; existing ones can still be moved.
    for role in 0..(pinchora_db::MAX_LEVEL_ROLES - 3) as u64 {
        assert!(db.add_level_role(guild, 1, 2_000 + role).await.unwrap());
    }
    assert_eq!(
        db.level_roles(guild).await.unwrap().len() as i64,
        pinchora_db::MAX_LEVEL_ROLES
    );
    assert!(!db.add_level_role(guild, 1, 9_999).await.unwrap());
    assert!(db.add_level_role(guild, 30, 1_000).await.unwrap());
    assert!(
        db.add_level_role(other, 1, 9_999).await.unwrap(),
        "the cap is per server"
    );

    assert!(db.remove_level_role(guild, 1_001).await.unwrap());
    assert!(!db.remove_level_role(guild, 1_001).await.unwrap());

    // A role deleted on Discord leaves both the rewards and the store.
    db.add_store_item(guild, 1_002, 50, None).await.unwrap().unwrap();
    db.forget_role(guild, 1_002).await.unwrap();
    assert!(db.level_roles(guild).await.unwrap().iter().all(|r| r.role_id != 1_002));
    assert!(db.store_items(guild).await.unwrap().is_empty());

    assert!(db.guild_settings(guild).await.unwrap().level_roles_stack);
    assert!(!db.set_level_roles_stack(guild, false).await.unwrap().level_roles_stack);
    assert!(db.set_level_roles_stack(guild, true).await.unwrap().level_roles_stack);
}

#[tokio::test]
async fn weekly_xp_is_kept_per_week_and_chat_splits_server_and_global_xp() {
    let _isolation = crate::support::exclusive().await;
    let Some(db) = db(5).await else { return };
    let [guild, a, b, c] = ids();
    let this_week = chrono::NaiveDate::from_ymd_opt(2026, 11, 2).unwrap();
    let last_week = this_week - chrono::Duration::days(7);
    let grant = |server_xp, global_xp, points| XpGrant {
        server_xp,
        global_xp,
        points,
    };

    // A boosted chat tick: the server gets 40 XP and 6 points, the global total only the base 20.
    let gain = db.add_xp(guild, a, grant(40, 20, 6), last_week).await.unwrap();
    assert_eq!((gain.old_xp, gain.new_xp), (0, 40));
    assert_eq!(db.user(a).await.unwrap().xp, 20, "global XP is not multiplied");
    assert_eq!(db.member_points(guild, a).await.unwrap().0, 6);
    // A no-XP tick: nothing for the server, the member row and the weekly board stay clean.
    let gain = db.add_xp(guild, b, grant(0, 20, 0), this_week).await.unwrap();
    assert_eq!((gain.old_xp, gain.new_xp), (0, 0));
    assert_eq!(db.user(b).await.unwrap().xp, 20);
    assert!(db.weekly_leaderboard(guild, this_week, 10, 0).await.unwrap().is_empty());

    // Voice minutes are batched: one statement, every member counted, level-ups visible in the gains.
    db.add_xp(guild, a, grant(10, 10, 0), this_week).await.unwrap();
    let gains = db
        .add_voice_xp(guild, &[(a, 10), (b, 30), (c, 20)], this_week)
        .await
        .unwrap();
    let gain_of = |user: u64| gains.iter().find(|(u, _)| *u == user).unwrap().1;
    assert_eq!((gain_of(a).old_xp, gain_of(a).new_xp), (50, 60));
    assert_eq!((gain_of(b).old_xp, gain_of(b).new_xp), (0, 30));
    assert_eq!((gain_of(c).old_xp, gain_of(c).new_xp), (0, 20));
    assert!(db.add_voice_xp(guild, &[], this_week).await.unwrap().is_empty());
    assert_eq!(db.user(c).await.unwrap().xp, 0, "voice earns server XP only");

    // The weekly boards: this week counts only this week's XP, in order, and last week keeps its own.
    let board =
        |rows: Vec<pinchora_db::LeaderboardRow>| rows.iter().map(|r| (r.user_id as u64, r.value)).collect::<Vec<_>>();
    assert_eq!(
        board(db.weekly_leaderboard(guild, this_week, 10, 0).await.unwrap()),
        vec![(b, 30), (a, 20), (c, 20)]
    );
    assert_eq!(
        board(db.weekly_leaderboard(guild, this_week, 1, 1).await.unwrap()),
        vec![(a, 20)],
        "paging, ties by user id"
    );
    assert_eq!(
        board(db.weekly_leaderboard(guild, last_week, 10, 0).await.unwrap()),
        vec![(a, 40)]
    );
    assert!(
        db.weekly_leaderboard(guild + 1, this_week, 10, 0)
            .await
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        db.guild_leaderboard(guild, 10, 0).await.unwrap()[0].value,
        60,
        "all-time XP is untouched"
    );

    // Old weeks can be pruned without touching the current one.
    let ancient = this_week - chrono::Duration::days(70);
    db.add_voice_xp(guild, &[(a, 5)], ancient).await.unwrap();
    assert!(
        db.forget_weeks_before(this_week - chrono::Duration::days(35))
            .await
            .unwrap()
            >= 1
    );
    assert!(db.weekly_leaderboard(guild, ancient, 10, 0).await.unwrap().is_empty());
    assert_eq!(db.weekly_leaderboard(guild, this_week, 10, 0).await.unwrap().len(), 3);
}

#[tokio::test]
async fn pruning_old_weekly_xp_has_an_index_on_the_week() {
    let _isolation = crate::support::shared().await;
    let Some(db) = db(5).await else { return };
    // The primary key and the board index both lead with guild_id; with a table scan ruled out, the
    // prune must still find an index it can use for `week < ...`.
    let mut tx = db.pool().begin().await.unwrap();
    sqlx::query("SET LOCAL enable_seqscan = off")
        .execute(&mut *tx)
        .await
        .unwrap();
    let plan: Vec<(String,)> = sqlx::query_as("EXPLAIN DELETE FROM weekly_xp WHERE week < '2020-01-06'::date")
        .fetch_all(&mut *tx)
        .await
        .unwrap();
    let plan: Vec<String> = plan.into_iter().map(|(line,)| line).collect();
    assert!(
        plan.iter().any(|line| line.contains("weekly_xp_week_idx")),
        "{}",
        plan.join("\n")
    );
}

#[tokio::test]
async fn xp_rules_are_capped_replaced_and_cleared() {
    let _isolation = crate::support::shared().await;
    let Some(db) = db(5).await else { return };
    let [guild, other] = ids();

    assert!(db.xp_rules(guild).await.unwrap().is_empty());
    assert!(db.set_xp_rule(guild, XpTarget::Channel, 100, 2.0).await.unwrap());
    assert!(
        db.set_xp_rule(guild, XpTarget::Role, 100, 0.0).await.unwrap(),
        "same id, other kind"
    );
    assert!(
        db.set_xp_rule(guild, XpTarget::Channel, 100, 1.5).await.unwrap(),
        "replaced"
    );
    let rules = db.xp_rules(guild).await.unwrap();
    assert_eq!(rules.guild, guild);
    assert_eq!((rules.channels[&100], rules.roles[&100]), (1.5, 0.0));

    // The cap counts channels and roles together; existing rules can still be changed.
    for n in 0..(MAX_XP_RULES - 2) as u64 {
        assert!(db.set_xp_rule(guild, XpTarget::Channel, 1_000 + n, 3.0).await.unwrap());
    }
    assert!(
        !db.set_xp_rule(guild, XpTarget::Role, 999, 2.0).await.unwrap(),
        "at the limit"
    );
    assert!(db.set_xp_rule(guild, XpTarget::Role, 100, 4.0).await.unwrap());
    assert!(
        db.set_xp_rule(other, XpTarget::Role, 999, 2.0).await.unwrap(),
        "the cap is per server"
    );

    assert!(db.remove_xp_rule(guild, XpTarget::Channel, 100).await.unwrap());
    assert!(!db.remove_xp_rule(guild, XpTarget::Channel, 100).await.unwrap());
    assert!(
        db.set_xp_rule(guild, XpTarget::Role, 999, 2.0).await.unwrap(),
        "room again"
    );

    // A role deleted on Discord takes its rule with it.
    db.forget_role(guild, 999).await.unwrap();
    assert!(!db.xp_rules(guild).await.unwrap().roles.contains_key(&999));
    assert_eq!(db.clear_xp_rules(guild).await.unwrap(), MAX_XP_RULES as u64 - 1);
    assert!(db.xp_rules(guild).await.unwrap().is_empty());
    assert_eq!(db.xp_rules(other).await.unwrap().roles.len(), 1);

    // The database refuses a multiplier out of range.
    assert!(db.set_xp_rule(guild, XpTarget::Role, 1, 5.5).await.is_err());
    assert!(db.set_xp_rule(guild, XpTarget::Role, 1, -1.0).await.is_err());
}

#[tokio::test]
async fn rep_roles_are_capped_and_replaceable() {
    let _isolation = crate::support::shared().await;
    let Some(db) = db(5).await else { return };
    let [guild, other] = ids();

    assert!(db.rep_roles(guild).await.unwrap().is_empty());
    assert!(db.add_rep_role(guild, 10, 1_000).await.unwrap());
    assert!(db.add_rep_role(guild, 3, 1_001).await.unwrap());
    assert!(
        db.add_rep_role(guild, 3, 1_002).await.unwrap(),
        "roles can share a count"
    );
    let rows: Vec<_> = db
        .rep_roles(guild)
        .await
        .unwrap()
        .iter()
        .map(|r| (r.reputation, r.role_id))
        .collect();
    assert_eq!(rows, vec![(3, 1_001), (3, 1_002), (10, 1_000)]);
    assert!(
        db.add_rep_role(guild, 25, 1_000).await.unwrap(),
        "adding again moves it"
    );
    assert_eq!(db.rep_roles(guild).await.unwrap().last().unwrap().reputation, 25);

    for role in 0..(MAX_REP_ROLES - 3) as u64 {
        assert!(db.add_rep_role(guild, 1, 2_000 + role).await.unwrap());
    }
    assert!(!db.add_rep_role(guild, 1, 9_999).await.unwrap(), "at the limit");
    assert!(
        db.add_rep_role(guild, 30, 1_000).await.unwrap(),
        "existing ones can still move"
    );
    assert!(db.add_rep_role(other, 1, 9_999).await.unwrap(), "the cap is per server");

    assert!(db.remove_rep_role(guild, 1_001).await.unwrap());
    assert!(!db.remove_rep_role(guild, 1_001).await.unwrap());
    db.forget_role(guild, 1_002).await.unwrap();
    assert!(db.rep_roles(guild).await.unwrap().iter().all(|r| r.role_id != 1_002));
}

#[tokio::test]
async fn leveling_toolkit_settings_have_safe_defaults() {
    let _isolation = crate::support::shared().await;
    let Some(db) = db(5).await else { return };
    let [guild] = ids();

    let s = db.guild_settings(guild).await.unwrap();
    assert_eq!(s.levelup_mode, "channel", "level-ups keep working as before");
    assert!(!s.voice_xp_enabled, "voice XP starts off");
    assert_eq!(s.voice_xp_amount, 10);

    assert_eq!(db.set_levelup_mode(guild, "dm").await.unwrap().levelup_mode, "dm");
    assert!(
        db.set_levelup_mode(guild, "nowhere").await.is_err(),
        "unknown modes are refused"
    );
    let s = db.update_voice_xp(guild, Some(true), None).await.unwrap();
    assert_eq!((s.voice_xp_enabled, s.voice_xp_amount), (true, 10));
    let s = db.update_voice_xp(guild, None, Some(25)).await.unwrap();
    assert_eq!(
        (s.voice_xp_enabled, s.voice_xp_amount),
        (true, 25),
        "unset options keep old values"
    );
    assert!(db.update_voice_xp(guild, None, Some(0)).await.is_err());
    assert!(db.update_voice_xp(guild, None, Some(101)).await.is_err());
    assert_eq!(db.guild_settings(guild).await.unwrap().levelup_mode, "dm");
}
