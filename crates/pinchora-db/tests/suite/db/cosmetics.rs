use super::*;

#[tokio::test]
async fn accessories_are_bought_once_and_outfits_keep_three_slots() {
    let _isolation = crate::support::shared().await;
    let Some(db) = db().await else { return };
    let [alice, bob] = ids();

    assert!(db.owned_accessories(alice).await.unwrap().is_empty());
    assert_eq!(
        db.buy_accessory(alice, "top_hat", 900).await.unwrap(),
        CustomizeBuy::Insufficient { balance: 0 }
    );
    assert!(
        db.owned_accessories(alice).await.unwrap().is_empty(),
        "a refused buy owns nothing"
    );
    db.earn_shells(alice, 1_000, "test").await.unwrap();
    assert_eq!(
        db.buy_accessory(alice, "top_hat", 900).await.unwrap(),
        CustomizeBuy::Bought { balance: 100 }
    );
    assert_eq!(
        db.buy_accessory(alice, "top_hat", 900).await.unwrap(),
        CustomizeBuy::AlreadyOwned
    );
    assert_eq!(db.user(alice).await.unwrap().credits, 100, "charged once");
    assert_eq!(db.owned_accessories(alice).await.unwrap(), vec!["top_hat".to_owned()]);
    assert!(db.owned_accessories(bob).await.unwrap().is_empty());
    // What the wearing rule reads: purchases and the numbers, also for a player with no row.
    let facts = db.wardrobe_facts(alice).await.unwrap();
    assert_eq!(facts.bought_accessories, vec!["top_hat".to_owned()]);
    assert!(facts.bought_badges.is_empty());
    assert_eq!(facts.stats.shells, 100);
    let facts = db.wardrobe_facts(bob).await.unwrap();
    assert!(facts.bought_accessories.is_empty() && facts.bought_badges.is_empty());
    assert_eq!(facts.stats.shells, 0);

    assert!(db.outfits(alice).await.unwrap().is_empty());
    assert!(
        !db.rename_outfit(alice, 2, "Beach").await.unwrap(),
        "nothing to rename yet"
    );
    db.save_outfit(alice, 2, "tidewalker.1.1.0.2.0.0").await.unwrap();
    db.save_outfit(alice, 1, "bobbin.0.0.0.0.0.0+crown").await.unwrap();
    assert!(db.rename_outfit(alice, 2, "  Beach day ").await.unwrap());
    db.save_outfit(alice, 2, "curlcove.3.3.3.3.3.3").await.unwrap();
    assert_eq!(
        db.outfits(alice).await.unwrap(),
        vec![
            OutfitRow {
                slot: 1,
                name: "Outfit 1".into(),
                code: "bobbin.0.0.0.0.0.0+crown".into()
            },
            OutfitRow {
                slot: 2,
                name: "Beach day".into(),
                code: "curlcove.3.3.3.3.3.3".into()
            },
        ],
        "saving again keeps the name"
    );
    assert!(db.save_outfit(alice, 4, "x").await.is_err());
    assert!(db.rename_outfit(alice, 1, "").await.is_err());
    assert!(db.rename_outfit(alice, 1, &"x".repeat(33)).await.is_err());
    assert!(db.outfits(bob).await.unwrap().is_empty());
}

#[tokio::test]
async fn characters_are_saved_per_player() {
    let _isolation = crate::support::shared().await;
    let Some(db) = db().await else { return };
    let [alice, bob] = ids();

    assert_eq!(db.character(alice).await.unwrap(), None, "unknown player");
    assert_eq!(
        db.user(alice).await.unwrap().character,
        None,
        "no character designed yet"
    );
    assert_eq!(db.character(alice).await.unwrap(), None);

    db.set_character(alice, "tidewalker.1.1.0.2").await.unwrap();
    db.set_character(bob, "bunhaven.7.11.5.9").await.unwrap();
    assert_eq!(
        db.character(alice).await.unwrap().as_deref(),
        Some("tidewalker.1.1.0.2")
    );
    assert_eq!(
        db.user(bob).await.unwrap().character.as_deref(),
        Some("bunhaven.7.11.5.9")
    );

    db.set_character(alice, "curlcove.0.0.0.0").await.unwrap();
    assert_eq!(db.character(alice).await.unwrap().as_deref(), Some("curlcove.0.0.0.0"));
    assert_eq!(
        db.character(bob).await.unwrap().as_deref(),
        Some("bunhaven.7.11.5.9"),
        "other players are untouched"
    );
}

#[tokio::test]
async fn backgrounds_are_bought_once_and_equipped_only_when_owned() {
    let _isolation = crate::support::shared().await;
    let Some(db) = db().await else { return };
    let [alice] = ids();

    assert_eq!(
        db.buy_background(alice, "dune", 500).await.unwrap(),
        CustomizeBuy::Insufficient { balance: 0 },
        "a new player has no shells"
    );
    assert!(
        db.owned_backgrounds(alice).await.unwrap().is_empty(),
        "a refused buy owns nothing"
    );
    assert!(
        !db.equip_background(alice, "dune").await.unwrap(),
        "not owned, not worn"
    );
    assert!(
        db.buy_background(alice, "dune", 0).await.is_err(),
        "a purchase has a price"
    );

    db.earn_shells(alice, 1_000, "test").await.unwrap();
    assert_eq!(
        db.buy_background(alice, "dune", 500).await.unwrap(),
        CustomizeBuy::Bought { balance: 500 }
    );
    assert_eq!(
        db.buy_background(alice, "dune", 500).await.unwrap(),
        CustomizeBuy::AlreadyOwned
    );
    assert_eq!(db.user(alice).await.unwrap().credits, 500, "a duplicate costs nothing");
    assert_eq!(
        db.buy_background(alice, "reef", 600).await.unwrap(),
        CustomizeBuy::Insufficient { balance: 500 }
    );
    assert_eq!(db.owned_backgrounds(alice).await.unwrap(), vec!["dune"]);

    assert_eq!(db.user(alice).await.unwrap().background, None);
    assert!(db.equip_background(alice, "dune").await.unwrap());
    assert_eq!(db.user(alice).await.unwrap().background.as_deref(), Some("dune"));
    assert!(
        !db.equip_background(alice, "reef").await.unwrap(),
        "wearing one does not unlock another"
    );
    assert_eq!(db.user(alice).await.unwrap().background.as_deref(), Some("dune"));
    db.remove_background(alice).await.unwrap();
    assert_eq!(db.user(alice).await.unwrap().background, None);

    let (net, spent): (i64, i64) = sqlx::query_as(
        "SELECT COALESCE(SUM(delta), 0)::BIGINT, COALESCE(SUM(delta) FILTER (WHERE reason = 'background'), 0)::BIGINT
         FROM credit_ledger WHERE user_id = $1",
    )
    .bind(alice as i64)
    .fetch_one(db.pool())
    .await
    .unwrap();
    assert_eq!((net, spent), (500, -500), "the ledger shows the one purchase");
}

#[tokio::test]
async fn badges_are_bought_once_and_worn_in_order() {
    let _isolation = crate::support::shared().await;
    let Some(db) = db().await else { return };
    let [alice, bob] = ids();
    db.earn_shells(alice, 1_000, "test").await.unwrap();

    // Ten clicks at once buy the badge exactly once.
    let mut tasks = Vec::new();
    for _ in 0..10 {
        let db = db.clone();
        tasks.push(tokio::spawn(async move {
            db.buy_badge(alice, "tiny_tide", 100).await.unwrap()
        }));
    }
    let mut outcomes = Vec::new();
    for t in tasks {
        outcomes.push(t.await.unwrap());
    }
    let bought = outcomes
        .iter()
        .filter(|o| matches!(o, CustomizeBuy::Bought { .. }))
        .count();
    assert_eq!(bought, 1, "{outcomes:?}");
    assert!(outcomes.iter().all(|o| !matches!(o, CustomizeBuy::Insufficient { .. })));
    assert_eq!(db.user(alice).await.unwrap().credits, 900);
    assert_eq!(db.owned_badges(alice).await.unwrap(), vec!["tiny_tide"]);
    assert_eq!(
        db.buy_badge(bob, "tiny_tide", 100).await.unwrap(),
        CustomizeBuy::Insufficient { balance: 0 },
        "owning is per player"
    );
    assert!(db.owned_badges(bob).await.unwrap().is_empty());

    let worn = |ids: &[&str]| ids.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>();
    assert!(db.user(alice).await.unwrap().badges.is_empty());
    db.set_badges(alice, &worn(&["tiny_tide", "fin_finder", "pawshake"]))
        .await
        .unwrap();
    assert_eq!(
        db.user(alice).await.unwrap().badges,
        worn(&["tiny_tide", "fin_finder", "pawshake"]),
        "the order is kept"
    );
    db.set_badges(alice, &worn(&["pawshake", "tiny_tide"])).await.unwrap();
    assert_eq!(db.user(alice).await.unwrap().badges, worn(&["pawshake", "tiny_tide"]));

    let seven = worn(&["a", "b", "c", "d", "e", "f", "g"]);
    assert_eq!(MAX_BADGES, 6);
    assert!(db.set_badges(alice, &seven).await.is_err(), "at most six");
    assert!(db.set_badges(alice, &worn(&["a", "a"])).await.is_err(), "no repeats");
    assert_eq!(
        db.user(alice).await.unwrap().badges,
        worn(&["pawshake", "tiny_tide"]),
        "refused saves change nothing"
    );
    db.set_badges(alice, &seven[..6]).await.unwrap();
    db.set_badges(alice, &[]).await.unwrap();
    assert!(db.user(alice).await.unwrap().badges.is_empty());
}

#[tokio::test]
async fn badges_stay_automatic_until_the_player_chooses() {
    let _isolation = crate::support::shared().await;
    let Some(db) = db().await else { return };
    let [alice, bob] = ids();

    // A new player has not chosen: the card picks their badges.
    let row = db.user(alice).await.unwrap();
    assert!(!row.badges_set && row.badges.is_empty());
    assert_eq!(
        row.badges_seen,
        Some(Vec::new()),
        "nothing seen yet, so what they earn first is new"
    );
    assert!(row.title.is_none());

    // Any save is a choice, an empty one included: it now means "no badges".
    db.set_badges(alice, &[]).await.unwrap();
    assert!(db.user(alice).await.unwrap().badges_set);
    db.set_badges(alice, &["pawshake".to_owned()]).await.unwrap();
    assert!(db.user(alice).await.unwrap().badges_set);

    // Resetting hands the choice back to the card.
    db.reset_badges(alice).await.unwrap();
    let row = db.user(alice).await.unwrap();
    assert!(!row.badges_set && row.badges.is_empty());

    // Saving for a player the database has never seen creates them with the choice made.
    db.set_badges(bob, &[]).await.unwrap();
    assert!(db.user(bob).await.unwrap().badges_set);
}

#[tokio::test]
async fn badge_notices_are_claimed_once_and_old_news_is_not_announced() {
    let _isolation = crate::support::shared().await;
    let Some(db) = db().await else { return };
    let [alice, old, nobody] = ids();
    let list = |ids: &[&str]| ids.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>();

    // A new player starts with nothing announced.
    assert_eq!(db.user(alice).await.unwrap().credits, 0);
    let (stats, notified) = db.badge_progress(alice).await.unwrap();
    assert_eq!(stats, PlayerStats::default());
    assert_eq!(notified, Some(Vec::new()));
    assert_eq!(
        db.claim_badge_notices(alice, &list(&["little_beacon"])).await.unwrap(),
        list(&["little_beacon"])
    );
    assert_eq!(
        db.claim_badge_notices(alice, &list(&["little_beacon"])).await.unwrap(),
        Vec::<String>::new(),
        "never twice"
    );
    assert_eq!(
        db.claim_badge_notices(alice, &list(&["little_beacon", "pawshake"]))
            .await
            .unwrap(),
        list(&["pawshake"]),
        "only what is new"
    );
    assert_eq!(
        db.badge_progress(alice).await.unwrap().1,
        Some(list(&["little_beacon", "pawshake"]))
    );

    // Commands finishing together announce a badge exactly once between them.
    let mut tasks = Vec::new();
    for _ in 0..10 {
        let db = db.clone();
        tasks.push(tokio::spawn(async move {
            db.claim_badge_notices(alice, &list(&["fin_finder"])).await.unwrap()
        }));
    }
    let mut claimed = 0;
    for t in tasks {
        claimed += t.await.unwrap().len();
    }
    assert_eq!(claimed, 1);

    // A player from before notices existed has no list. The first claim fills it with what
    // they already have and announces nothing; the next one announces what comes after.
    db.user(old).await.unwrap();
    sqlx::query("UPDATE users SET badges_notified = NULL, badges_seen = NULL WHERE id = $1")
        .bind(old as i64)
        .execute(db.pool())
        .await
        .unwrap();
    assert_eq!(db.badge_progress(old).await.unwrap().1, None);
    assert_eq!(db.user(old).await.unwrap().badges_seen, None);
    assert!(
        db.claim_badge_notices(old, &list(&["little_beacon", "fin_finder"]))
            .await
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        db.claim_badge_notices(old, &list(&["little_beacon", "fin_finder", "pawshake"]))
            .await
            .unwrap(),
        list(&["pawshake"])
    );

    // Nobody the database does not know has nothing yet; a pet alone is enough to count, and
    // announcing it gives them their row.
    assert_eq!(
        db.badge_progress(nobody).await.unwrap(),
        (PlayerStats::default(), Some(Vec::new()))
    );
    db.adopt(nobody, "crumbclaw_crab", "Sir Clicks").await.unwrap();
    let (stats, notified) = db.badge_progress(nobody).await.unwrap();
    assert_eq!((stats.pets, notified), (1, Some(Vec::new())));
    assert_eq!(
        db.claim_badge_notices(nobody, &list(&["pawshake"])).await.unwrap(),
        list(&["pawshake"])
    );
    assert!(
        db.claim_badge_notices(nobody, &list(&["pawshake"]))
            .await
            .unwrap()
            .is_empty()
    );

    // What the player has seen in the collection is kept for its NEW markers.
    db.mark_badges_seen(old, &list(&["little_beacon"])).await.unwrap();
    assert_eq!(db.user(old).await.unwrap().badges_seen, Some(list(&["little_beacon"])));
}

#[tokio::test]
async fn titles_are_worn_and_granted() {
    let _isolation = crate::support::shared().await;
    let Some(db) = db().await else { return };
    let [alice, bob] = ids();

    assert!(db.granted_titles(alice).await.unwrap().is_empty());
    assert!(db.grant_title(alice, "house_of_the_week").await.unwrap());
    assert!(
        !db.grant_title(alice, "house_of_the_week").await.unwrap(),
        "granted once"
    );
    assert!(db.grant_title(alice, "tide_angler").await.unwrap());
    assert_eq!(
        db.granted_titles(alice).await.unwrap(),
        ["house_of_the_week", "tide_angler"],
        "oldest first"
    );
    assert!(
        db.granted_titles(bob).await.unwrap().is_empty(),
        "grants are per player"
    );

    assert_eq!(db.user(alice).await.unwrap().title, None);
    db.set_title(alice, Some("tide_angler")).await.unwrap();
    assert_eq!(db.user(alice).await.unwrap().title.as_deref(), Some("tide_angler"));
    db.set_title(alice, None).await.unwrap();
    assert_eq!(db.user(alice).await.unwrap().title, None);
    db.set_title(bob, Some("early_riser")).await.unwrap();
    assert_eq!(
        db.user(bob).await.unwrap().title.as_deref(),
        Some("early_riser"),
        "saving creates the player"
    );
}

#[tokio::test]
async fn card_style_and_showcase_are_saved_per_player() {
    let _isolation = crate::support::shared().await;
    let Some(db) = db().await else { return };
    let [alice, bob] = ids();

    // A new player has the look every player starts with and an empty showcase.
    let row = db.user(alice).await.unwrap();
    assert_eq!(
        (row.accent.as_deref(), row.panel_opacity, row.frame.as_str()),
        (None, 1, "plain")
    );
    assert_eq!(
        (
            row.showcase_fish,
            row.showcase_pet,
            row.showcase_item,
            row.showcase_house
        ),
        (None, None, None, false)
    );

    // Style: each part is saved on its own; saving creates the player; the database refuses nonsense.
    db.set_accent(alice, Some("#3a9bdc")).await.unwrap();
    db.set_panel_opacity(alice, 0).await.unwrap();
    db.set_frame(alice, "rope").await.unwrap();
    let row = db.user(alice).await.unwrap();
    assert_eq!(
        (row.accent.as_deref(), row.panel_opacity, row.frame.as_str()),
        (Some("#3a9bdc"), 0, "rope")
    );
    for bad in ["3a9bdc", "#3A9BDC", "#fff", "#3a9bd", "#3a9bdcc", "#gggggg", ""] {
        assert!(
            db.set_accent(alice, Some(bad)).await.is_err(),
            "{bad:?} is not an accent"
        );
    }
    assert!(db.set_panel_opacity(alice, 3).await.is_err() && db.set_panel_opacity(alice, -1).await.is_err());
    assert_eq!(
        db.user(alice).await.unwrap().accent.as_deref(),
        Some("#3a9bdc"),
        "refusals change nothing"
    );
    db.set_frame(bob, "gold").await.unwrap();
    assert_eq!(db.user(bob).await.unwrap().frame, "gold", "saving creates the player");
    assert_eq!(db.user(alice).await.unwrap().frame, "rope", "styles are per player");
    db.set_accent(alice, None).await.unwrap();
    assert_eq!(db.user(alice).await.unwrap().accent, None);
    db.set_accent(alice, Some("#06d6a0")).await.unwrap();
    db.reset_card_style(alice).await.unwrap();
    let row = db.user(alice).await.unwrap();
    assert_eq!(
        (row.accent.as_deref(), row.panel_opacity, row.frame.as_str()),
        (None, 1, "plain")
    );

    // What can be put on the showcase: the fish book, first catch first.
    assert!(db.fish_book(alice).await.unwrap().is_empty());
    db.log_catch(alice, "trout", 20.0).await.unwrap();
    db.log_catch(alice, "carp", 31.5).await.unwrap();
    db.log_catch(alice, "trout", 25.0).await.unwrap();
    let book = db.fish_book(alice).await.unwrap();
    assert_eq!(
        book.iter().map(|b| b.fish_id.as_str()).collect::<Vec<_>>(),
        ["trout", "carp"]
    );
    assert_eq!((book[0].caught, book[0].best_cm), (2, 25.0));
    assert!(db.fish_book(bob).await.unwrap().is_empty(), "books are per player");

    // Furniture they own: in the inventory or standing in a room, each once, by id.
    let starter = pinchora_db::NewRoom {
        room_type: "living".into(),
        wallpaper: "w".into(),
        floor: "f".into(),
    };
    assert!(db.house_preview(alice).await.unwrap().is_none(), "no house yet");
    db.house(alice, &starter).await.unwrap();
    db.grant_item(alice, "chair", 2).await.unwrap();
    db.grant_item(alice, "lamp", 1).await.unwrap();
    db.place_item(alice, 0, "lamp", 1, 1, false, |_| Ok(())).await.unwrap();
    db.place_item(alice, 0, "chair", 2, 2, false, |_| Ok(())).await.unwrap();
    assert_eq!(
        db.owned_item_ids(alice).await.unwrap(),
        ["chair", "lamp"],
        "a chair left in the inventory and a placed one count once; a lamp that is only placed counts too"
    );
    assert!(db.owned_item_ids(bob).await.unwrap().is_empty());
    let preview = db.house_preview(alice).await.unwrap().expect("a house");
    assert_eq!((preview.room.idx, preview.room.wallpaper.as_str()), (0, "w"));
    assert_eq!(preview.placements.len(), 2);
    assert_eq!(preview.house.tier, 1);

    // The picks themselves, and clearing them.
    db.set_showcase_fish(alice, Some("carp")).await.unwrap();
    db.set_showcase_pet(alice, Some(42)).await.unwrap();
    db.set_showcase_item(alice, Some("lamp")).await.unwrap();
    db.set_showcase_house(alice, true).await.unwrap();
    let row = db.user(alice).await.unwrap();
    assert_eq!(
        (
            row.showcase_fish.as_deref(),
            row.showcase_pet,
            row.showcase_item.as_deref(),
            row.showcase_house
        ),
        (Some("carp"), Some(42), Some("lamp"), true)
    );
    db.set_showcase_fish(alice, None).await.unwrap();
    assert_eq!(db.user(alice).await.unwrap().showcase_fish, None);
    assert_eq!(
        db.user(alice).await.unwrap().showcase_pet,
        Some(42),
        "the other picks stay"
    );
    db.clear_showcase(alice).await.unwrap();
    let row = db.user(alice).await.unwrap();
    assert_eq!(
        (
            row.showcase_fish,
            row.showcase_pet,
            row.showcase_item,
            row.showcase_house
        ),
        (None, None, None, false)
    );
}
