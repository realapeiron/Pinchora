use super::*;

#[tokio::test]
async fn server_points_accrue_give_and_take() {
    let _isolation = crate::support::shared().await;
    let Some(db) = db().await else { return };
    let [guild, chatter, helper, bystander] = ids();

    let s = db.guild_settings(guild).await.unwrap();
    assert!(s.points_enabled && s.level_roles_stack);
    assert_eq!(
        (
            s.points_name.as_str(),
            s.points_emoji.as_str(),
            s.points_min,
            s.points_max
        ),
        ("points", "⭐", 1, 3)
    );

    // Chat earns XP and points in one query; a second message adds to both.
    db.add_message_xp(guild, chatter, 20, 2).await.unwrap();
    let gain = db.add_message_xp(guild, chatter, 20, 3).await.unwrap();
    assert_eq!((gain.old_xp, gain.new_xp), (20, 40));
    assert_eq!(db.member_points(guild, chatter).await.unwrap(), (5, Some(1)));

    // Giving creates the member row; the points board ranks by points, the XP board by XP.
    assert_eq!(db.give_points(guild, helper, 9).await.unwrap(), 9);
    assert_eq!(db.give_points(guild, helper, 1).await.unwrap(), 10);
    assert_eq!(db.member_points(guild, chatter).await.unwrap(), (5, Some(2)));
    assert_eq!(db.member_points(guild, bystander).await.unwrap(), (0, None));
    let points: Vec<_> = db
        .guild_points_leaderboard(guild, 10, 0)
        .await
        .unwrap()
        .iter()
        .map(|r| (r.user_id, r.value))
        .collect();
    assert_eq!(points, vec![(helper as i64, 10), (chatter as i64, 5)]);
    let xp: Vec<i64> = db
        .guild_leaderboard(guild, 10, 0)
        .await
        .unwrap()
        .iter()
        .map(|r| r.user_id)
        .collect();
    assert_eq!(xp, vec![chatter as i64], "a member who only got points has no XP rank");
    assert_eq!(db.guild_member_count(guild).await.unwrap(), 1);
    let recent: Vec<i64> = db
        .recent_xp_members(guild, 10)
        .await
        .unwrap()
        .iter()
        .map(|r| r.user_id)
        .collect();
    assert_eq!(recent, vec![chatter as i64]);

    // Taking stops at zero and says how much it really took.
    assert_eq!(db.take_points(guild, helper, 4).await.unwrap(), (4, 6));
    assert_eq!(db.take_points(guild, helper, 100).await.unwrap(), (6, 0));
    assert_eq!(db.take_points(guild, helper, 1).await.unwrap(), (0, 0));
    assert_eq!(
        db.take_points(guild, bystander, 1).await.unwrap(),
        (0, 0),
        "no row at all"
    );
    assert_eq!(db.member_points(guild, helper).await.unwrap(), (0, None));
    assert!(db.give_points(guild, helper, 0).await.is_err());
    assert!(db.take_points(guild, helper, -1).await.is_err());

    // Settings keep unset options and refuse an upside-down range.
    let s = db
        .update_points(guild, Some(false), Some("coins"), Some("🪙"), Some(2), Some(5))
        .await
        .unwrap();
    assert_eq!(
        (
            s.points_enabled,
            s.points_name.as_str(),
            s.points_emoji.as_str(),
            s.points_min,
            s.points_max
        ),
        (false, "coins", "🪙", 2, 5)
    );
    let s = db.update_points(guild, None, None, None, None, Some(9)).await.unwrap();
    assert_eq!((s.points_name.as_str(), s.points_min, s.points_max), ("coins", 2, 9));
    assert!(db.update_points(guild, None, None, None, Some(10), None).await.is_err());
}

#[tokio::test]
async fn store_purchase_checks_balance_and_stock() {
    let _isolation = crate::support::shared().await;
    use pinchora_db::StoreBuy;

    let Some(db) = db().await else { return };
    let [guild, buyer, poor] = ids();
    db.give_points(guild, buyer, 100).await.unwrap();

    let unlimited = db.add_store_item(guild, 7_001, 30, None).await.unwrap().unwrap();
    let scarce = db.add_store_item(guild, 7_002, 40, Some(1)).await.unwrap().unwrap();
    let ids_by_price: Vec<i64> = db.store_items(guild).await.unwrap().iter().map(|i| i.id).collect();
    assert_eq!(ids_by_price, vec![unlimited.id, scarce.id], "cheapest first");

    // Too poor: nothing moves.
    assert_eq!(
        db.buy_store_item(guild, poor, unlimited.id).await.unwrap(),
        StoreBuy::Insufficient { price: 30, balance: 0 }
    );

    assert_eq!(
        db.buy_store_item(guild, buyer, unlimited.id).await.unwrap(),
        StoreBuy::Bought {
            role_id: 7_001,
            price: 30,
            balance: 70
        }
    );
    assert_eq!(db.store_item(guild, unlimited.id).await.unwrap().unwrap().stock, None);

    // The last unit sells once; the next buyer finds it sold out and keeps their points.
    assert!(matches!(
        db.buy_store_item(guild, buyer, scarce.id).await.unwrap(),
        StoreBuy::Bought { balance: 30, .. }
    ));
    assert_eq!(
        db.buy_store_item(guild, buyer, scarce.id).await.unwrap(),
        StoreBuy::SoldOut
    );
    assert_eq!(db.member_points(guild, buyer).await.unwrap().0, 30);

    // A refund gives back the points and the unit.
    assert_eq!(db.refund_store_purchase(guild, buyer, scarce.id, 40).await.unwrap(), 70);
    assert_eq!(db.store_item(guild, scarce.id).await.unwrap().unwrap().stock, Some(1));
    assert_eq!(
        db.refund_store_purchase(guild, buyer, unlimited.id, 30).await.unwrap(),
        100,
        "unlimited stock stays unlimited"
    );
    assert_eq!(db.store_item(guild, unlimited.id).await.unwrap().unwrap().stock, None);

    // Re-adding a role updates it in place and keeps its id.
    let again = db.add_store_item(guild, 7_002, 10, Some(5)).await.unwrap().unwrap();
    assert_eq!((again.id, again.price, again.stock), (scarce.id, 10, Some(5)));

    assert!(db.remove_store_item(guild, 7_002).await.unwrap());
    assert!(!db.remove_store_item(guild, 7_002).await.unwrap());
    assert_eq!(
        db.buy_store_item(guild, buyer, scarce.id).await.unwrap(),
        StoreBuy::Gone
    );
}

#[tokio::test]
async fn store_never_oversells_or_overspends_under_contention() {
    let _isolation = crate::support::shared().await;
    use pinchora_db::StoreBuy;

    let Some(db) = db().await else { return };
    let guild = ids::<1>()[0];
    let buyers: [u64; 20] = ids();
    for &b in &buyers {
        db.give_points(guild, b, 100).await.unwrap();
    }

    // Twenty buyers, three units.
    let scarce = db.add_store_item(guild, 8_001, 40, Some(3)).await.unwrap().unwrap();
    let tasks: Vec<_> = buyers
        .iter()
        .map(|&b| {
            let db = db.clone();
            tokio::spawn(async move { db.buy_store_item(guild, b, scarce.id).await.unwrap() })
        })
        .collect();
    let mut bought = 0;
    for t in tasks {
        if matches!(t.await.unwrap(), StoreBuy::Bought { .. }) {
            bought += 1;
        }
    }
    assert_eq!(bought, 3, "stock of 3 sells exactly 3");
    assert_eq!(db.store_item(guild, scarce.id).await.unwrap().unwrap().stock, Some(0));
    let spent: i64 = {
        let mut total = 0;
        for &b in &buyers {
            total += 100 - db.member_points(guild, b).await.unwrap().0;
        }
        total
    };
    assert_eq!(spent, 3 * 40, "only the three buyers paid");

    // One buyer with 100 points clicks a 60-point item ten times at once.
    let [solo]: [u64; 1] = ids();
    db.give_points(guild, solo, 100).await.unwrap();
    let plenty = db.add_store_item(guild, 8_002, 60, None).await.unwrap().unwrap();
    let tasks: Vec<_> = (0..10)
        .map(|_| {
            let db = db.clone();
            tokio::spawn(async move { db.buy_store_item(guild, solo, plenty.id).await.unwrap() })
        })
        .collect();
    let mut bought = 0;
    for t in tasks {
        if matches!(t.await.unwrap(), StoreBuy::Bought { .. }) {
            bought += 1;
        }
    }
    assert_eq!(bought, 1, "100 points afford one 60-point item");
    assert_eq!(db.member_points(guild, solo).await.unwrap().0, 40);
}

#[tokio::test]
async fn store_is_capped_per_server() {
    let _isolation = crate::support::shared().await;
    let Some(db) = db().await else { return };
    let [guild] = ids();
    for role in 0..pinchora_db::MAX_STORE_ITEMS as u64 {
        assert!(db.add_store_item(guild, 6_000 + role, 5, None).await.unwrap().is_some());
    }
    assert!(db.add_store_item(guild, 6_999, 5, None).await.unwrap().is_none());
    assert!(
        db.add_store_item(guild, 6_000, 99, Some(2)).await.unwrap().is_some(),
        "an existing role can still be updated"
    );
}

#[tokio::test]
async fn join_role_goodbye_and_log_settings() {
    let _isolation = crate::support::shared().await;
    let Some(db) = db().await else { return };
    let [guild, other] = ids();

    // Everything starts off, and a log channel alone switches every kind of event on.
    let fresh = db.guild_settings(guild).await.unwrap();
    assert!(fresh.join_role_id.is_none() && fresh.goodbye_channel_id.is_none() && fresh.log_channel_id.is_none());
    assert!(fresh.log_members && fresh.log_names && fresh.log_bans && fresh.log_voice);
    assert_eq!(fresh.log_channel(LogKind::Voice), None);

    assert_eq!(db.set_join_role(guild, Some(77)).await.unwrap().join_role_id, Some(77));
    assert_eq!(db.set_join_role(guild, None).await.unwrap().join_role_id, None);

    // The message survives turning goodbyes off.
    let on = db.update_goodbye(guild, Some(5), Some("bye {user}")).await.unwrap();
    assert_eq!(
        (on.goodbye_channel_id, on.goodbye_message.as_deref()),
        (Some(5), Some("bye {user}"))
    );
    let kept = db.update_goodbye(guild, None, None).await.unwrap();
    assert_eq!(kept.goodbye_channel_id, Some(5), "nothing given, nothing changed");
    let off = db.clear_goodbye(guild).await.unwrap();
    assert_eq!(
        (off.goodbye_channel_id, off.goodbye_message.as_deref()),
        (None, Some("bye {user}"))
    );

    let logged = db.set_log_channel(guild, Some(9)).await.unwrap();
    assert_eq!(logged.log_channel(LogKind::Bans), Some(9));
    let some = db
        .update_log_events(guild, Some(false), None, None, Some(false))
        .await
        .unwrap();
    assert_eq!(some.log_channel(LogKind::Members), None);
    assert_eq!(some.log_channel(LogKind::Names), Some(9));
    assert_eq!(some.log_channel(LogKind::Voice), None);
    assert_eq!(
        db.set_log_channel(guild, None)
            .await
            .unwrap()
            .log_channel(LogKind::Names),
        None
    );

    // Another server is untouched.
    assert!(db.guild_settings(other).await.unwrap().join_role_id.is_none());

    // A role deleted on Discord stops being the join role.
    db.set_join_role(guild, Some(88)).await.unwrap();
    db.forget_role(guild, 99).await.unwrap();
    assert_eq!(db.guild_settings(guild).await.unwrap().join_role_id, Some(88));
    db.forget_role(guild, 88).await.unwrap();
    assert_eq!(db.guild_settings(guild).await.unwrap().join_role_id, None);
}

#[tokio::test]
async fn role_panels_belong_to_their_server_and_keep_order() {
    let _isolation = crate::support::shared().await;
    let Some(db) = db().await else { return };
    let [guild, other] = ids();

    // A panel is found by title (any case) or number, only in its own server.
    let (panel, created) = db.save_panel(guild, "Colours", Some("Pick one")).await.unwrap();
    assert!(created && panel.channel_id.is_none() && panel.message_id.is_none());
    let (again, created) = db.save_panel(guild, "colours", None).await.unwrap();
    assert!(!created && again.id == panel.id, "same title, same panel");
    assert_eq!(
        again.description.as_deref(),
        Some("Pick one"),
        "no description given, none lost"
    );
    let (updated, _) = db.save_panel(guild, "COLOURS", Some("Pick two")).await.unwrap();
    assert_eq!(updated.description.as_deref(), Some("Pick two"));
    assert_eq!(updated.title, "Colours", "the first title stays");
    assert_eq!(db.find_panel(guild, "COLOURS").await.unwrap().unwrap().id, panel.id);
    assert_eq!(
        db.find_panel(guild, &format!("#{}", panel.id))
            .await
            .unwrap()
            .unwrap()
            .id,
        panel.id
    );
    assert!(db.find_panel(other, "Colours").await.unwrap().is_none());
    assert!(db.find_panel(other, &panel.id.to_string()).await.unwrap().is_none());
    let (theirs, created) = db.save_panel(other, "Colours", None).await.unwrap();
    assert!(created && theirs.id != panel.id, "titles only clash within a server");

    // Buttons keep the order they were added in; adding again changes the label in place.
    assert!(db.add_panel_role(panel.id, 301, "Red", Some("🔴")).await.unwrap());
    assert!(db.add_panel_role(panel.id, 300, "Blue", None).await.unwrap());
    assert!(db.add_panel_role(panel.id, 301, "Crimson", None).await.unwrap());
    let roles: Vec<_> = db
        .panel_roles(panel.id)
        .await
        .unwrap()
        .into_iter()
        .map(|r| (r.role_id, r.label, r.emoji))
        .collect();
    assert_eq!(
        roles,
        vec![(301, "Crimson".to_owned(), None), (300, "Blue".to_owned(), None)]
    );

    // A click is only valid for a button of this server's panel.
    assert_eq!(
        db.panel_role(guild, panel.id, 300).await.unwrap().unwrap().label,
        "Blue"
    );
    assert!(
        db.panel_role(other, panel.id, 300).await.unwrap().is_none(),
        "another server"
    );
    assert!(
        db.panel_role(guild, panel.id, 999).await.unwrap().is_none(),
        "not on the panel"
    );
    assert!(
        db.panel_role(guild, theirs.id, 300).await.unwrap().is_none(),
        "not on that panel"
    );

    // Posting remembers the message.
    db.set_panel_message(panel.id, 11, 22).await.unwrap();
    let posted = db.find_panel(guild, "Colours").await.unwrap().unwrap();
    assert_eq!((posted.channel_id, posted.message_id), (Some(11), Some(22)));

    // Removing a button, and a role deleted on Discord, both take it off the panel.
    assert!(db.remove_panel_role(panel.id, 300).await.unwrap());
    assert!(!db.remove_panel_role(panel.id, 300).await.unwrap());
    db.forget_role(guild, 301).await.unwrap();
    assert!(db.panel_roles(panel.id).await.unwrap().is_empty());
}

#[tokio::test]
async fn role_panels_hold_at_most_25_roles_even_when_filled_at_once() {
    let _isolation = crate::support::shared().await;
    let Some(db) = db().await else { return };
    let [guild] = ids();
    let (panel, _) = db.save_panel(guild, "Everything", None).await.unwrap();

    // 40 managers adding at once: exactly 25 get in.
    let mut tasks = Vec::new();
    for role in 0..40u64 {
        let db = db.clone();
        tasks.push(tokio::spawn(async move {
            db.add_panel_role(panel.id, 5_000 + role, &format!("Role {role}"), None)
                .await
                .unwrap()
        }));
    }
    let mut added = 0;
    for task in tasks {
        added += i64::from(task.await.unwrap());
    }
    assert_eq!(added, MAX_PANEL_ROLES);
    assert_eq!(db.panel_roles(panel.id).await.unwrap().len() as i64, MAX_PANEL_ROLES);

    // Existing roles can still be relabelled when full; new ones are refused.
    let first = db.panel_roles(panel.id).await.unwrap()[0].role_id as u64;
    assert!(db.add_panel_role(panel.id, first, "Renamed", Some("⭐")).await.unwrap());
    assert!(!db.add_panel_role(panel.id, 9_999, "One too many", None).await.unwrap());
    let renamed = db.panel_role(guild, panel.id, first).await.unwrap().unwrap();
    assert_eq!(
        (renamed.label.as_str(), renamed.emoji.as_deref()),
        ("Renamed", Some("⭐"))
    );
}

#[tokio::test]
async fn restore_is_all_or_nothing() {
    let _isolation = crate::support::shared().await;
    let Some(db) = db().await else { return };
    let [user] = ids();
    let starter = pinchora_db::NewRoom {
        room_type: "living".into(),
        wallpaper: "w".into(),
        floor: "f".into(),
    };
    db.house(user, &starter).await.unwrap();
    db.grant_item(user, "chair", 2).await.unwrap();
    db.place_item(user, 0, "chair", 1, 1, false, |_| Ok(())).await.unwrap();
    db.place_item(user, 0, "chair", 2, 2, true, |_| Ok(())).await.unwrap();
    let removed = db.clear_room(user, 0).await.unwrap();
    assert_eq!(removed.len(), 2);
    assert!(db.placements(user, 0).await.unwrap().is_empty());
    assert_eq!(db.inventory(user).await.unwrap()[0].quantity, 2);

    // Putting back three chairs with two in the inventory changes nothing.
    let mut changes: Vec<pinchora_db::RoomChange> = removed.iter().cloned().map(pinchora_db::RoomChange::Add).collect();
    let mut extra = removed[0].clone();
    extra.x = 5;
    changes.push(pinchora_db::RoomChange::Add(extra));
    let refused = db.restore(user, 0, &changes, |_, _| Ok(())).await.unwrap();
    assert!(matches!(
        refused,
        pinchora_db::RestoreOutcome::Refused(pinchora_db::Refusal::NotOwned(ref item)) if item == "chair"
    ));
    assert!(db.placements(user, 0).await.unwrap().is_empty(), "rolled back");

    // The room check can refuse too; then the real two go back exactly.
    changes.pop();
    let blocked = db.restore(user, 0, &changes, |_, _| Err("no".into())).await.unwrap();
    assert!(matches!(
        blocked,
        pinchora_db::RestoreOutcome::Refused(pinchora_db::Refusal::Blocked(_))
    ));
    let pinchora_db::RestoreOutcome::Done { added } = db.restore(user, 0, &changes, |_, _| Ok(())).await.unwrap()
    else {
        panic!("restores")
    };
    let back = db.placements(user, 0).await.unwrap();
    assert_eq!(back.iter().map(|p| p.id).collect::<Vec<_>>(), added);
    assert_eq!(
        back.iter().map(|p| (p.x, p.y, p.flipped)).collect::<Vec<_>>(),
        [(1, 1, false), (2, 2, true)]
    );

    // A remove or flip must find the row exactly as given.
    let mut moved = back[0].clone();
    moved.x = 7;
    let stale = db
        .restore(user, 0, &[pinchora_db::RoomChange::Flip(moved)], |_, _| Ok(()))
        .await
        .unwrap();
    assert!(matches!(
        stale,
        pinchora_db::RestoreOutcome::Refused(pinchora_db::Refusal::Moved(_))
    ));
    db.restore(
        user,
        0,
        &[
            pinchora_db::RoomChange::Flip(back[0].clone()),
            pinchora_db::RoomChange::Remove(back[1].clone()),
        ],
        |_, _| Ok(()),
    )
    .await
    .unwrap();
    let now = db.placements(user, 0).await.unwrap();
    assert_eq!(now.len(), 1);
    assert!(now[0].flipped);
}
