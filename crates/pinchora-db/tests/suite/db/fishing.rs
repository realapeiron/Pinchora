use super::*;

#[tokio::test]
async fn contest_week_closes_once() {
    let _isolation = crate::support::shared().await;
    let Some(db) = db(5).await else { return };
    let [guild, a, b, voter] = ids();
    let starter = pinchora_db::NewRoom {
        room_type: "living".into(),
        wallpaper: "w".into(),
        floor: "f".into(),
    };
    for user in [a, b] {
        db.house(user, &starter).await.unwrap();
        db.grant_item(user, "chair", 1).await.unwrap();
        db.place_item(user, 0, "chair", 1, 1, false, |_| Ok(())).await.unwrap();
        assert_eq!(db.enter_contest(guild, 100, user, "Name", 0).await.unwrap(), 1);
    }
    assert!(db.contest_vote(guild, 100, a, voter).await.unwrap());
    assert!(
        !db.contest_vote(guild, 100, a, voter).await.unwrap(),
        "one vote per entry"
    );
    assert!(!db.contest_vote(guild, 100, a, a).await.unwrap(), "not for your own");
    assert!(db.contest_vote(guild, 100, a, b).await.unwrap());
    assert!(db.contest_vote(guild, 100, b, voter).await.unwrap());
    assert_eq!(db.contest_pieces(guild, 100, a).await.unwrap().len(), 1);

    let before = db.user(a).await.unwrap().credits;
    // Two calls race to close the week; only one pays.
    let (one, two) = tokio::join!(
        db.close_contest_weeks(guild, 101, &[500, 250, 100], "house_of_the_week"),
        db.close_contest_weeks(guild, 101, &[500, 250, 100], "house_of_the_week"),
    );
    let closed = [one.unwrap(), two.unwrap()];
    assert_eq!(closed.iter().map(Vec::len).sum::<usize>(), 1, "{closed:?}");
    assert!(
        db.close_contest_weeks(guild, 101, &[500], "t")
            .await
            .unwrap()
            .is_empty()
    );
    assert_eq!(db.user(a).await.unwrap().credits, before + 500);
    let result = db.last_contest_result(guild, 101).await.unwrap().unwrap();
    assert_eq!(result.winner_ids, vec![a as i64, b as i64]);
    assert_eq!(result.winner_votes, vec![2, 1]);
    assert_eq!(result.entries, 2);
}

#[tokio::test]
async fn fishing_gear_is_bought_equipped_and_used_up() {
    let _isolation = crate::support::shared().await;
    let Some(db) = db(5).await else { return };
    let [alice, bob] = ids();

    // Nothing chosen yet: the shore, the starter rod, plain bait, and nothing owned.
    assert_eq!(db.fishing_gear(alice).await.unwrap(), GearRow::default());
    assert_eq!(GearRow::default().spot, DEFAULT_SPOT);
    assert_eq!(GearRow::default().rod, DEFAULT_ROD);
    assert!(db.owned_rods(alice).await.unwrap().is_empty());
    assert!(db.bait_stock(alice).await.unwrap().is_empty());

    // Rods cost shells, once. A poor buyer keeps both the shells and no rod.
    assert_eq!(
        db.buy_rod(alice, "reedline_rod", 800).await.unwrap(),
        GearBuy::Insufficient { balance: 0 }
    );
    assert!(db.owned_rods(alice).await.unwrap().is_empty(), "the rod was taken back");
    db.earn_shells(alice, 1_000, "test").await.unwrap();
    assert_eq!(
        db.buy_rod(alice, "reedline_rod", 800).await.unwrap(),
        GearBuy::Bought { balance: 200, owned: 1 }
    );
    assert_eq!(
        db.buy_rod(alice, "reedline_rod", 800).await.unwrap(),
        GearBuy::AlreadyOwned
    );
    assert_eq!(db.user(alice).await.unwrap().credits, 200, "a second buy costs nothing");
    assert_eq!(db.owned_rods(alice).await.unwrap(), ["reedline_rod"]);
    assert!(db.buy_rod(alice, "free_rod", 0).await.is_err(), "a rod needs a price");

    // Only a rod they own goes on the line; the starter rod is everybody's.
    assert!(!db.equip_rod(alice, "pearlwhip_rod", false).await.unwrap());
    assert_eq!(db.fishing_gear(alice).await.unwrap().rod, DEFAULT_ROD);
    assert!(db.equip_rod(alice, "reedline_rod", false).await.unwrap());
    assert_eq!(db.fishing_gear(alice).await.unwrap().rod, "reedline_rod");
    assert!(db.equip_rod(alice, DEFAULT_ROD, true).await.unwrap());
    assert!(
        !db.equip_rod(bob, "reedline_rod", false).await.unwrap(),
        "not bob's rod"
    );

    // Bait comes in packs and adds up.
    assert_eq!(
        db.buy_bait(alice, "glow_worm", 5, 60).await.unwrap(),
        GearBuy::Bought { balance: 140, owned: 5 }
    );
    assert_eq!(
        db.buy_bait(alice, "glow_worm", 5, 60).await.unwrap(),
        GearBuy::Bought { balance: 80, owned: 10 }
    );
    assert_eq!(
        db.buy_bait(alice, "silver_lure", 5, 175).await.unwrap(),
        GearBuy::Insufficient { balance: 80 }
    );
    assert_eq!(db.bait_stock(alice).await.unwrap(), [("glow_worm".to_owned(), 10)]);
    assert!(
        !db.equip_bait(alice, Some("silver_lure")).await.unwrap(),
        "none of it owned"
    );
    assert!(db.equip_bait(alice, Some("glow_worm")).await.unwrap());
    assert_eq!(db.fishing_gear(alice).await.unwrap().bait.as_deref(), Some("glow_worm"));

    // A cast uses one up; the last one takes the bait off the hook.
    for left in (1..10).rev() {
        assert_eq!(db.take_bait(alice, "glow_worm").await.unwrap(), Some(left));
    }
    assert_eq!(db.fishing_gear(alice).await.unwrap().bait.as_deref(), Some("glow_worm"));
    assert_eq!(db.take_bait(alice, "glow_worm").await.unwrap(), Some(0));
    assert_eq!(db.fishing_gear(alice).await.unwrap().bait, None, "plain bait again");
    assert_eq!(db.take_bait(alice, "glow_worm").await.unwrap(), None, "none left");
    assert!(db.bait_stock(alice).await.unwrap().is_empty());
    assert_eq!(db.take_bait(bob, "glow_worm").await.unwrap(), None);
    assert!(db.equip_bait(alice, None).await.unwrap(), "plain bait is always there");

    // The last spot is remembered, and changing it leaves the rest alone.
    db.set_fishing_spot(alice, "reef").await.unwrap();
    let gear = db.fishing_gear(alice).await.unwrap();
    assert_eq!((gear.spot.as_str(), gear.rod.as_str()), ("reef", DEFAULT_ROD));

    // Two buys of one rod at once pay once.
    let [carol] = ids();
    db.earn_shells(carol, 5_000, "test").await.unwrap();
    let (a, b) = tokio::join!(
        db.buy_rod(carol, "pearlwhip_rod", 3_500),
        db.buy_rod(carol, "pearlwhip_rod", 3_500)
    );
    let bought = [a.unwrap(), b.unwrap()]
        .iter()
        .filter(|o| matches!(o, GearBuy::Bought { .. }))
        .count();
    assert_eq!(bought, 1);
    assert_eq!(db.user(carol).await.unwrap().credits, 1_500);
}

#[tokio::test]
async fn a_catch_goes_to_the_book_the_bucket_and_the_weekly_board() {
    let _isolation = crate::support::exclusive().await;
    let Some(db) = db(5).await else { return };
    let [alice, guild] = ids();
    let this_week = week(1);
    let catch = |fish: &'static str, size: f32, ratio: f64| NewCatch {
        fish_id: fish,
        size_cm: size,
        ratio,
        weekly: Some((guild, this_week)),
        milestones: &[],
        known: &["koi"],
    };

    // A first of its kind: in the book, the bucket and on the board, no record to beat.
    let first = db.record_catch(alice, catch("koi", 50.0, 1.0)).await.unwrap();
    assert!(first.first && !first.record && first.week_best);
    assert_eq!((first.previous_best, first.species, first.bucket), (0.0, 1, 1));

    // A smaller one of the same: the book counts it, the record stays, the bucket grows.
    let smaller = db.record_catch(alice, catch("koi", 40.0, 0.8)).await.unwrap();
    assert!(!smaller.first && !smaller.record && !smaller.week_best);
    assert_eq!((smaller.previous_best, smaller.species, smaller.bucket), (50.0, 1, 2));
    let book = db.fish_book(alice).await.unwrap();
    assert_eq!((book[0].caught, book[0].best_cm), (2, 50.0));

    // A bigger one is a record, and the best of the week replaces the first.
    let bigger = db.record_catch(alice, catch("koi", 61.5, 1.23)).await.unwrap();
    assert!(!bigger.first && bigger.record && bigger.week_best);
    assert_eq!(bigger.previous_best, 50.0);
    assert_eq!(db.fish_book(alice).await.unwrap()[0].best_cm, 61.5);
    let board = db.weekly_catch_board(guild, this_week, 10, 0).await.unwrap();
    assert_eq!(board.len(), 1, "one place per player");
    assert_eq!((board[0].fish_id.as_str(), board[0].size_cm), ("koi", 61.5));

    // A different fish beats it only by being bigger for its kind, not in centimetres.
    let tiny = db.record_catch(alice, catch("minnow", 7.9, 1.4)).await.unwrap();
    assert!(tiny.first && tiny.week_best);
    let board = db.weekly_catch_board(guild, this_week, 10, 0).await.unwrap();
    assert_eq!((board[0].fish_id.as_str(), board[0].size_cm), ("minnow", 7.9));
    let outsized = db.record_catch(alice, catch("whale", 900.0, 1.1)).await.unwrap();
    assert!(outsized.first && !outsized.week_best, "huge, but ordinary for a whale");
    assert_eq!(
        db.weekly_catch_board(guild, this_week, 10, 0).await.unwrap()[0].fish_id,
        "minnow"
    );

    // Boards belong to a server and a week; a catch with no server counts for none.
    let [other_guild] = ids();
    assert!(
        db.weekly_catch_board(other_guild, this_week, 10, 0)
            .await
            .unwrap()
            .is_empty()
    );
    assert!(db.weekly_catch_board(guild, week(8), 10, 0).await.unwrap().is_empty());
    let elsewhere = db
        .record_catch(
            alice,
            NewCatch {
                weekly: None,
                ..catch("koi", 30.0, 0.6)
            },
        )
        .await
        .unwrap();
    assert!(!elsewhere.week_best);

    // Players rank by ratio, then by who got there first.
    let [bob, carol] = ids();
    db.record_catch(bob, catch("koi", 70.0, 1.5)).await.unwrap();
    db.record_catch(carol, catch("koi", 70.0, 1.5)).await.unwrap();
    let order: Vec<u64> = db
        .weekly_catch_board(guild, this_week, 10, 0)
        .await
        .unwrap()
        .iter()
        .map(|r| r.user_id as u64)
        .collect();
    assert_eq!(order, [bob, carol, alice]);
    let page: Vec<_> = db.weekly_catch_board(guild, this_week, 1, 1).await.unwrap();
    assert_eq!(page[0].user_id as u64, carol);

    // Old weeks are forgotten, this one is kept.
    assert_eq!(db.forget_catch_weeks_before(this_week).await.unwrap(), 0);
    db.forget_catch_weeks_before(week(8)).await.unwrap();
    assert!(db.weekly_catch_board(guild, this_week, 10, 0).await.unwrap().is_empty());
}

#[tokio::test]
async fn book_milestones_pay_once_and_selling_keeps_the_book() {
    let _isolation = crate::support::shared().await;
    let Some(db) = db(5).await else { return };
    let [alice, guild] = ids();
    let milestones = [
        MilestoneReward {
            species: 2,
            shells: 100,
            cosmetic: Cosmetic::Badge("tiny_tide".to_owned()),
        },
        MilestoneReward {
            species: 3,
            shells: 500,
            cosmetic: Cosmetic::Background("dune".to_owned()),
        },
    ];
    let catch = |fish: &'static str, size: f32| NewCatch {
        fish_id: fish,
        size_cm: size,
        ratio: 1.0,
        weekly: Some((guild, week(2))),
        milestones: &milestones,
        known: &["a", "b", "c"],
    };

    // One species: nothing yet. The second pays its milestone.
    assert!(db.record_catch(alice, catch("a", 10.0)).await.unwrap().paid.is_empty());
    let second = db.record_catch(alice, catch("b", 10.0)).await.unwrap();
    assert_eq!(second.paid.len(), 1);
    assert_eq!(second.paid[0].reward.species, 2);
    assert!(second.paid[0].cosmetic_new);
    assert_eq!(db.user(alice).await.unwrap().credits, 100);
    assert_eq!(db.owned_badges(alice).await.unwrap(), ["tiny_tide"]);

    // The same species again, and again: no second payment.
    assert!(db.record_catch(alice, catch("b", 12.0)).await.unwrap().paid.is_empty());
    assert!(db.record_catch(alice, catch("a", 12.0)).await.unwrap().paid.is_empty());
    assert_eq!(db.user(alice).await.unwrap().credits, 100);

    // The third pays its own, and a background they already own is not "new".
    db.earn_shells(alice, 5_000, "test").await.unwrap();
    db.buy_background(alice, "dune", 1_000).await.unwrap();
    let third = db.record_catch(alice, catch("c", 10.0)).await.unwrap();
    assert_eq!(third.paid.len(), 1);
    assert!(!third.paid[0].cosmetic_new, "bought it earlier");
    assert_eq!(db.user(alice).await.unwrap().credits, 100 + 5_000 - 1_000 + 500);

    // A player who passed a milestone before it existed is paid at their next catch, all at once.
    let [veteran] = ids();
    for fish in ["a", "b", "c"] {
        db.log_catch(veteran, fish, 5.0).await.unwrap();
    }
    let late = db.record_catch(veteran, catch("a", 6.0)).await.unwrap();
    assert_eq!(late.paid.iter().map(|p| p.reward.species).collect::<Vec<_>>(), [2, 3]);
    assert_eq!(db.user(veteran).await.unwrap().credits, 600);

    // Two catches at once pay a milestone once; one that missed the other's species is caught up
    // by the next catch.
    let [racer] = ids();
    db.record_catch(racer, catch("a", 5.0)).await.unwrap();
    let (x, y) = tokio::join!(
        db.record_catch(racer, catch("b", 5.0)),
        db.record_catch(racer, catch("c", 5.0))
    );
    let next = db.record_catch(racer, catch("a", 6.0)).await.unwrap();
    let paid: usize = [x.unwrap(), y.unwrap(), next].iter().map(|l| l.paid.len()).sum();
    assert_eq!(paid, 2, "milestone 2 and 3, each once");
    assert_eq!(db.user(racer).await.unwrap().credits, 600);

    // Selling empties the bucket for shells, never the book, and a second sale of the same fish pays nothing.
    let bucket = db.bucket(alice).await.unwrap();
    assert_eq!(bucket.len(), 5);
    assert_eq!(db.bucket_count(alice, &["a", "b", "c"]).await.unwrap(), 5);
    assert!(
        bucket.windows(2).all(|w| w[0].fish_id <= w[1].fish_id),
        "species by species"
    );
    let before = db.user(alice).await.unwrap().credits;
    let sales: Vec<(i64, i64)> = bucket.iter().map(|r| (r.id, 10)).collect();
    let sold = db.sell_fish(alice, &sales[..3]).await.unwrap();
    assert_eq!((sold.fish, sold.shells, sold.balance), (3, 30, before + 30));
    let again = db.sell_fish(alice, &sales[..3]).await.unwrap();
    assert_eq!((again.fish, again.shells, again.balance), (0, 0, before + 30));
    assert_eq!(db.bucket_count(alice, &["a", "b", "c"]).await.unwrap(), 2);
    // Someone else's fish cannot be sold.
    let theirs = db.sell_fish(veteran, &sales[3..]).await.unwrap();
    assert_eq!((theirs.fish, theirs.shells), (0, 0));
    assert_eq!(db.bucket_count(alice, &["a", "b", "c"]).await.unwrap(), 2);
    let rest = db.sell_fish(alice, &sales[3..]).await.unwrap();
    assert_eq!((rest.fish, rest.shells), (2, 20));
    assert_eq!(db.bucket_count(alice, &["a", "b", "c"]).await.unwrap(), 0);
    assert_eq!(
        db.fish_book(alice).await.unwrap().len(),
        3,
        "the book keeps every species"
    );

    // A species the game no longer knows does not count towards the bucket (not in the catch's count
    // either), and selling it for nothing clears it.
    let credits = db.user(alice).await.unwrap().credits;
    let log = db.record_catch(alice, catch("a", 9.0)).await.unwrap();
    assert_eq!(log.bucket, 1);
    let gone = db.record_catch(alice, catch("gone", 9.0)).await.unwrap();
    assert_eq!(gone.bucket, 1, "the dropped species is not counted");
    assert_eq!(db.bucket(alice).await.unwrap().len(), 2);
    assert_eq!(db.bucket_count(alice, &["a", "b", "c"]).await.unwrap(), 1);
    assert_eq!(db.bucket_count(alice, &["a", "b", "c", "gone"]).await.unwrap(), 2);
    let stray = db
        .bucket(alice)
        .await
        .unwrap()
        .into_iter()
        .find(|r| r.fish_id == "gone")
        .unwrap();
    let cleared = db.sell_fish(alice, &[(stray.id, 0)]).await.unwrap();
    assert_eq!((cleared.fish, cleared.shells, cleared.balance), (1, 0, credits));
    assert_eq!(db.bucket(alice).await.unwrap().len(), 1);
}
