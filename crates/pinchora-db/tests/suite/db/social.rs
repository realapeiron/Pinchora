use super::*;

#[tokio::test]
async fn one_partner_each_even_when_pairings_race() {
    let _isolation = crate::support::shared().await;
    let Some(db) = db().await else { return };
    let [x, y, z, w, v] = ids();

    let PairOutcome::Paired { since } = db.pair_partners(y, z).await.unwrap() else {
        panic!("two free players pair up");
    };
    let partner = db.partner(y).await.unwrap().expect("y has a partner");
    assert_eq!((partner.partner_id, partner.since), (z, since));
    assert_eq!(db.partner(z).await.unwrap().map(|p| p.partner_id), Some(y));

    // `y` is the smaller id of the first pair and the larger of the second: only the trigger
    // sees that this is a second partner. Either order of the arguments is the same pair.
    assert_eq!(db.pair_partners(x, y).await.unwrap(), PairOutcome::Taken { user: y });
    assert_eq!(db.pair_partners(y, x).await.unwrap(), PairOutcome::Taken { user: y });
    assert_eq!(db.pair_partners(z, w).await.unwrap(), PairOutcome::Taken { user: z });
    assert!(db.partner(x).await.unwrap().is_none() && db.partner(w).await.unwrap().is_none());
    assert!(db.pair_partners(x, x).await.is_err());

    // Names ride along for the profile card.
    assert_eq!(db.partner_name(y).await.unwrap(), None, "no name remembered yet");
    db.remember_name(z, "Zed").await.unwrap();
    db.remember_name(z, "Zed the Second").await.unwrap();
    assert_eq!(db.partner_name(y).await.unwrap().as_deref(), Some("Zed the Second"));
    assert_eq!(db.partner_name(x).await.unwrap(), None);

    // Leaving frees both, and tells who the partner was.
    let left = db.leave_partner(z).await.unwrap().expect("z had a partner");
    assert_eq!((left.partner_id, left.since), (y, since));
    assert!(db.leave_partner(z).await.unwrap().is_none());
    assert!(db.partner(y).await.unwrap().is_none());
    assert!(matches!(
        db.pair_partners(x, y).await.unwrap(),
        PairOutcome::Paired { .. }
    ));

    // Pairings that share a player race: exactly one wins.
    let outcomes = tokio::join!(
        db.pair_partners(w, v),
        db.pair_partners(v, z),
        db.pair_partners(z, w),
        db.pair_partners(w, z),
    );
    let outcomes = [outcomes.0, outcomes.1, outcomes.2, outcomes.3].map(|o| o.unwrap());
    let paired = outcomes
        .iter()
        .filter(|o| matches!(o, PairOutcome::Paired { .. }))
        .count();
    // (w,v) and (v,z) and (z,w) all share players: at most one pairing among w, v, z survives.
    assert_eq!(paired, 1, "{outcomes:?}");
    let in_pairs = [w, v, z].into_iter().map(|u| db.partner(u)).collect::<Vec<_>>();
    let mut partnered = 0;
    for p in in_pairs {
        partnered += usize::from(p.await.unwrap().is_some());
    }
    assert_eq!(partnered, 2, "one pair, two partnered players");
}

#[tokio::test]
async fn friends_are_mutual_and_capped() {
    let _isolation = crate::support::shared().await;
    let Some(db) = db().await else { return };
    let [a, b, c] = ids();

    assert_eq!(db.befriend(a, b).await.unwrap(), BefriendOutcome::Added);
    assert_eq!(db.befriend(b, a).await.unwrap(), BefriendOutcome::AlreadyFriends);
    assert!(db.are_friends(a, b).await.unwrap() && db.are_friends(b, a).await.unwrap());
    assert!(!db.are_friends(a, c).await.unwrap());
    assert!(db.befriend(a, a).await.is_err());

    db.remember_name(b, "Bea").await.unwrap();
    db.grant_item(b, "chair", 1).await.unwrap();
    let starter = pinchora_db::NewRoom {
        room_type: "living".into(),
        wallpaper: "w".into(),
        floor: "f".into(),
    };
    db.house(b, &starter).await.unwrap();
    db.like_house(b, c, chrono::NaiveDate::from_ymd_opt(2026, 11, 5).unwrap())
        .await
        .unwrap();
    assert_eq!(db.befriend(a, c).await.unwrap(), BefriendOutcome::Added);
    let (page, total) = db.friends(a, 0, 10).await.unwrap();
    assert_eq!(total, 2);
    assert_eq!(
        page.iter().map(|f| f.friend_id).collect::<Vec<_>>(),
        vec![c, b],
        "newest first"
    );
    let bea = page.iter().find(|f| f.friend_id == b).unwrap();
    assert_eq!((bea.name.as_str(), bea.likes, bea.locked), ("Bea", 1, false));
    assert_eq!(db.friends(a, 1, 1).await.unwrap().0.len(), 1, "paging");

    // The shortlist leaves out locked houses and puts the most liked first.
    let shortlist = db.visitable_friends(a, 25).await.unwrap();
    assert_eq!(shortlist.iter().map(|f| f.friend_id).collect::<Vec<_>>(), vec![b, c]);
    db.set_house_locked(b, true).await.unwrap();
    let shortlist = db.visitable_friends(a, 25).await.unwrap();
    assert_eq!(shortlist.iter().map(|f| f.friend_id).collect::<Vec<_>>(), vec![c]);

    // Removing a friend removes both sides.
    assert!(db.unfriend(b, a).await.unwrap());
    assert!(!db.unfriend(a, b).await.unwrap());
    assert!(!db.are_friends(a, b).await.unwrap() && !db.are_friends(b, a).await.unwrap());
    assert_eq!(db.friend_count(b).await.unwrap(), 0);

    // 100 friends is the most; the other side's cap counts too.
    let [full, last] = ids();
    let crowd = (0..MAX_FRIENDS as u64)
        .map(|n| full + 1_000_000 + n)
        .collect::<Vec<_>>();
    for friend in &crowd {
        assert_eq!(db.befriend(full, *friend).await.unwrap(), BefriendOutcome::Added);
    }
    assert_eq!(
        db.befriend(full, last).await.unwrap(),
        BefriendOutcome::Full { user: full }
    );
    assert_eq!(
        db.befriend(last, full).await.unwrap(),
        BefriendOutcome::Full { user: full }
    );
    assert_eq!(db.friend_count(full).await.unwrap(), MAX_FRIENDS);
    assert!(db.unfriend(full, crowd[0]).await.unwrap());
    assert_eq!(db.befriend(last, full).await.unwrap(), BefriendOutcome::Added);
}

#[tokio::test]
async fn interactions_count_per_pair_and_per_user() {
    let _isolation = crate::support::shared().await;
    let Some(db) = db().await else { return };
    let [a, b, c] = ids();

    assert_eq!(
        db.record_interaction("hug", a, b).await.unwrap(),
        Tally {
            pair: 1,
            given: 1,
            received: 1
        }
    );
    assert_eq!(
        db.record_interaction("hug", a, b).await.unwrap(),
        Tally {
            pair: 2,
            given: 2,
            received: 2
        }
    );
    // Another receiver: the pair starts over, the sender's total goes on.
    assert_eq!(
        db.record_interaction("hug", a, c).await.unwrap(),
        Tally {
            pair: 1,
            given: 3,
            received: 1
        }
    );
    // The other direction is its own pair; a different kind has its own counters.
    assert_eq!(
        db.record_interaction("hug", b, a).await.unwrap(),
        Tally {
            pair: 1,
            given: 1,
            received: 1
        }
    );
    assert_eq!(
        db.record_interaction("pat", a, b).await.unwrap(),
        Tally {
            pair: 1,
            given: 1,
            received: 1
        }
    );
    assert_eq!(
        db.exchanged(a, b).await.unwrap(),
        vec![("hug".to_owned(), 2, 1), ("pat".to_owned(), 1, 0)]
    );
    assert_eq!(
        db.exchanged(b, a).await.unwrap(),
        vec![("hug".to_owned(), 1, 2), ("pat".to_owned(), 0, 1)]
    );
    assert!(db.exchanged(b, c).await.unwrap().is_empty());
}

#[tokio::test]
async fn a_repeat_visit_moves_the_time_but_pays_once() {
    let _isolation = crate::support::shared().await;
    let Some(db) = db().await else { return };
    let [host, guest] = ids();
    let day = chrono::NaiveDate::from_ymd_opt(2026, 11, 5).unwrap();

    let first = db.record_visit(host, guest, day, 25, 10).await.unwrap();
    assert!(
        matches!(first, pinchora_db::VisitOutcome::Rewarded { reward: 25 }),
        "{first:?}"
    );
    // Pretend the first visit was long ago; the next visit of the day must bring it up to date.
    sqlx::query("UPDATE visits SET visited_at = now() - interval '30 hours' WHERE host_id = $1 AND guest_id = $2")
        .bind(host as i64)
        .bind(guest as i64)
        .execute(db.pool())
        .await
        .unwrap();
    assert!(
        db.recent_visitors(host, 10).await.unwrap()[0].visited_at < chrono::Utc::now() - chrono::Duration::hours(24)
    );
    let again = db.record_visit(host, guest, day, 25, 10).await.unwrap();
    assert!(matches!(again, pinchora_db::VisitOutcome::AlreadyToday), "{again:?}");
    let visitors = db.recent_visitors(host, 10).await.unwrap();
    assert!(
        visitors[0].visited_at > chrono::Utc::now() - chrono::Duration::minutes(1),
        "{visitors:?}"
    );
    assert_eq!(db.user(guest).await.unwrap().credits, 25, "paid once");
    let (rows,): (i64,) = sqlx::query_as("SELECT count(*) FROM visits WHERE host_id = $1 AND guest_id = $2")
        .bind(host as i64)
        .bind(guest as i64)
        .fetch_one(db.pool())
        .await
        .unwrap();
    assert_eq!(rows, 1);
}

#[tokio::test]
async fn friendship_respects_blocks_in_both_directions() {
    let _isolation = crate::support::shared().await;
    let Some(db) = db().await else { return };
    let [a, b] = ids();
    assert!(db.befriend(a, a).await.is_err());
    db.block_signer(a, b, "B").await.unwrap();
    assert_eq!(db.befriend(a, b).await.unwrap(), BefriendOutcome::Blocked);
    assert_eq!(db.befriend(b, a).await.unwrap(), BefriendOutcome::Blocked);
    assert!(!db.are_friends(a, b).await.unwrap());
    db.unblock_signer(a, b).await.unwrap();
    assert_eq!(db.befriend(a, b).await.unwrap(), BefriendOutcome::Added);
}
