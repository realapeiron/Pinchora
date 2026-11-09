use crate::commands::house::house::*;

#[tokio::test]
async fn storing_a_support_and_its_items_refuses_a_stale_group_atomically() {
    let Ok(url) = std::env::var("PINCHORA_TEST_DATABASE_URL") else {
        return;
    };
    crate::fixture_isolation::shared().await;
    let db = pinchora_db::Db::connect(&url, 5).await.unwrap();
    db.migrate().await.unwrap();
    let owner = (std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos()
        >> 4) as u64;
    let content = Content::get();
    db.house(
        owner,
        &NewRoom {
            room_type: "living".into(),
            wallpaper: content.surfaces.wallpapers[0].id.clone(),
            floor: content.surfaces.floors[0].id.clone(),
        },
    )
    .await
    .unwrap();
    for item in ["crumbcorner_table__moss", "pagepair_books__moss"] {
        db.grant_item(owner, item, 1).await.unwrap();
        assert!(matches!(
            db.place_item(owner, 0, item, 1, 1, false, |_| Ok(())).await.unwrap(),
            PlaceOutcome::Placed { .. }
        ));
    }
    let before = db.placements(owner, 0).await.unwrap();
    let support = before.iter().find(|p| p.item_id == "crumbcorner_table__moss").unwrap();
    assert_eq!(standing_on(content, &before, support).len(), 1);
    sqlx::query("UPDATE placements SET flipped=NOT flipped WHERE id=$1")
        .bind(support.id)
        .execute(db.pool())
        .await
        .unwrap();
    assert!(
        take_back(&db, content, Who::own(owner), (8, 8), &before, support)
            .await
            .unwrap()
            .is_none()
    );
    assert_eq!(db.placements(owner, 0).await.unwrap().len(), 2);
    assert!(db.inventory(owner).await.unwrap().iter().all(|r| r.quantity == 0));
    let current = db.placements(owner, 0).await.unwrap();
    let support = current.iter().find(|p| p.item_id == "crumbcorner_table__moss").unwrap();
    assert_eq!(
        take_back(&db, content, Who::own(owner), (8, 8), &current, support)
            .await
            .unwrap()
            .unwrap()
            .len(),
        2
    );
    assert!(db.placements(owner, 0).await.unwrap().is_empty());
    assert!(db.inventory(owner).await.unwrap().iter().all(|r| r.quantity == 1));
}

fn pet(id: i64, species: &str, active: bool, away_until: Option<DateTime<Utc>>) -> PetRow {
    PetRow {
        id,
        owner_id: 1,
        species: species.to_owned(),
        name: species.to_owned(),
        level: 1,
        xp: 0,
        hunger: 80,
        fun: 80,
        energy: 80,
        hygiene: 80,
        needs_at: Utc::now(),
        active,
        trip_dest: away_until.map(|_| "tidepool_meadow".to_owned()),
        trip_ends_at: away_until,
    }
}

#[test]
fn a_pet_on_a_field_trip_is_not_at_home() {
    let now = Utc::now();
    let hour = chrono::Duration::hours(1);
    let pets = vec![
        pet(1, "crab", false, None),
        pet(2, "guppy", true, None),
        pet(3, "otter", false, Some(now + hour)),
        // Back from its trip (the souvenir is still unclaimed): home again.
        pet(4, "jelly", false, Some(now - hour)),
    ];
    let home = |pets| home_pets(pets, now);
    assert_eq!(
        home(pets),
        [RoomPet::awake("guppy"), RoomPet::awake("crab"), RoomPet::awake("jelly")]
    );
    assert!(home(Vec::new()).is_empty());
}

#[test]
fn a_pet_that_fell_asleep_is_drawn_asleep() {
    let now = Utc::now();
    // 80 in everything runs dry at 32 h and the pet drops off a day later, at 56 h.
    let dozing = |hours: i64| PetRow {
        needs_at: now - chrono::Duration::hours(hours),
        ..pet(2, "crab", false, None)
    };
    assert_eq!(
        home_pets(vec![pet(1, "guppy", true, None), dozing(60)], now),
        [RoomPet::awake("guppy"), RoomPet::asleep("crab")]
    );
    assert_eq!(home_pets(vec![dozing(50)], now), [RoomPet::awake("crab")]);
    // An away pet is not home at all, asleep or not.
    let gone = PetRow {
        trip_dest: Some("tidepool_meadow".to_owned()),
        trip_ends_at: Some(now + chrono::Duration::hours(1)),
        ..dozing(60)
    };
    assert!(home_pets(vec![gone], now).is_empty());
}

#[test]
fn arrows_follow_the_isometric_screen() {
    let max = (9, 9);
    // Floor and right wall: ↖ x−1, ↘ x+1, ↗ y−1, ↙ y+1.
    assert_eq!(step_cursor("ul", (4, 4), false, max), (3, 4));
    assert_eq!(step_cursor("dr", (4, 4), false, max), (5, 4));
    assert_eq!(step_cursor("ur", (4, 4), false, max), (4, 3));
    assert_eq!(step_cursor("dl", (4, 4), false, max), (4, 5));
    // Left wall: ↗/↙ slide along the wall (x−1/x+1), ↖ is level up, ↘ level down.
    assert_eq!(step_cursor("ur", (4, 1), true, max), (3, 1));
    assert_eq!(step_cursor("dl", (4, 1), true, max), (5, 1));
    assert_eq!(step_cursor("ul", (4, 1), true, max), (4, 0));
    assert_eq!(step_cursor("dr", (4, 1), true, max), (4, 2));
    assert_eq!(step_cursor("?", (4, 4), false, max), (4, 4));
}

#[test]
fn arrows_stay_inside_the_room_or_wall() {
    assert_eq!(step_cursor("ul", (0, 2), false, (5, 4)), (0, 2));
    assert_eq!(step_cursor("ur", (2, 0), false, (5, 4)), (2, 0));
    assert_eq!(step_cursor("dr", (5, 2), false, (5, 4)), (5, 2));
    assert_eq!(step_cursor("dl", (2, 4), false, (5, 4)), (2, 4));
    // A wall: x along it, y is the level (0..WALL_LEVELS).
    let levels = house::WALL_LEVELS - 1;
    assert_eq!(
        step_cursor("ur", (3, 0), false, (6, levels)),
        (3, 0),
        "already on the top level"
    );
    assert_eq!(
        step_cursor("dl", (3, levels), false, (6, levels)),
        (3, levels),
        "already on the bottom level"
    );
    assert_eq!(step_cursor("ul", (3, 0), true, (6, levels)), (3, 0));
    assert_eq!(step_cursor("dr", (3, levels), true, (6, levels)), (3, levels));
    assert_eq!(step_cursor("ur", (0, 1), true, (6, levels)), (0, 1), "at the corner");
    assert_eq!(step_cursor("dl", (6, 1), true, (6, levels)), (6, 1), "at the far end");
}

#[test]
fn a_picked_item_lands_on_the_nearest_free_spot() {
    let content = Content::get();
    let tier = house::tier(1);
    let sofa = content
        .items()
        .find(|i| i.base.layer == Layer::Floor && i.base.footprint == [2, 1])
        .map(|i| i.id())
        .expect("content has a 2×1 floor item");
    let blocker = PlacementRow {
        id: 1,
        room_idx: 0,
        item_id: sofa.clone(),
        x: 3,
        y: 3,
        flipped: false,
    };
    let mut state = EditState {
        prefix: "he",
        room: 0,
        x: 3,
        y: 3,
        page: 0,
        flipped: false,
        mode: CursorMode::Floor,
        item: Some(sofa.clone()),
    };
    let placements = [blocker];
    // The cursor sits on the first sofa, so the second moves one tile away.
    let flat = |_: &str| 0;
    assert_eq!(
        settle(content, tier, &placements, &mut state, &flat, false),
        Settle::Moved
    );
    assert_eq!((state.x + state.y).abs_diff(6), 1);
    assert_eq!(
        settle(content, tier, &placements, &mut state, &flat, false),
        Settle::Stayed
    );
    let spots = free_spots(content, tier, &placements, &state, &flat);
    assert_eq!(spots.first(), Some(&(state.x, state.y)));
    assert!(!spots.contains(&(3, 3)) && !spots.contains(&(2, 3)));
    assert_eq!(spot_label(Lang::En, 2, 3, None), "C4");
    assert_eq!(spot_label(Lang::En, 0, 1, Some(true)), "Left wall 1 · middle");
    assert_eq!(spot_label(Lang::Tr, 0, 1, Some(true)), "Sol duvar 1 · orta");
}

/// A small lamp, a tall bed and the tier-1 room, for the tests of what the jump avoids.
fn lamp_and_bed() -> (&'static Content, &'static house::Tier, String, String, PlacementRow) {
    let content = Content::get();
    let find = |layer: Layer, footprint: [u32; 2]| {
        content
            .items()
            .find(|i| i.base.layer == layer && i.base.footprint == footprint)
            .map(|i| i.id())
            .expect("content has this kind of item")
    };
    let (lamp, bed) = (find(Layer::Surface, [1, 1]), find(Layer::Floor, [2, 2]));
    // The bed stands right in front of the room's back corner.
    let bed_row = PlacementRow {
        id: 1,
        room_idx: 0,
        item_id: bed.clone(),
        x: 0,
        y: 1,
        flipped: false,
    };
    (content, house::tier(1), lamp, bed, bed_row)
}

fn holding(item: &str, x: u32, y: u32) -> EditState {
    EditState {
        prefix: "he",
        room: 0,
        x,
        y,
        page: 0,
        flipped: false,
        mode: CursorMode::Floor,
        item: Some(item.to_owned()),
    }
}

#[test]
fn a_picked_lamp_is_not_put_where_a_bed_hides_it() {
    let (content, tier, lamp, bed, bed_row) = lamp_and_bed();
    // The lamp's art is 20 px tall, the bed's 120: the bed in front of A1 covers the lamp there.
    let tall = {
        let bed = bed.clone();
        move |id: &str| if id == bed { 120 } else { 20 }
    };
    let placements = [bed_row];
    let placed = placed_list(&placements);
    assert!(
        house::is_hidden(content, &placed, &tall, &lamp, 0, 0, false),
        "A1 is behind the bed"
    );
    assert!(!house::is_hidden(content, &placed, &tall, &lamp, 5, 0, false));

    // The cursor starts on A1, which is free: the lamp still moves to the nearest spot in sight.
    let mut state = holding(&lamp, 0, 0);
    assert!(
        free_spots(content, tier, &placements, &state, &tall).contains(&(0, 0)),
        "A1 is free"
    );
    assert_eq!(
        settle(content, tier, &placements, &mut state, &tall, false),
        Settle::Moved
    );
    let (x, y) = (state.x, state.y);
    assert!(
        !house::is_hidden(content, &placed, &tall, &lamp, x, y, false),
        "{} is in sight",
        spot_label(Lang::En, x, y, None)
    );
    let spots = free_spots(content, tier, &placements, &state, &tall);
    let distance = |&(sx, sy): &(u32, u32)| sx.abs_diff(0) + sy.abs_diff(0);
    let nearest_seen = spots
        .iter()
        .filter(|&&(sx, sy)| !house::is_hidden(content, &placed, &tall, &lamp, sx, sy, false))
        .map(distance)
        .min();
    assert_eq!(Some(x + y), nearest_seen, "and it is the nearest one that is");
    // The menu lists what shows before what is hidden.
    let first_hidden = spots
        .iter()
        .position(|&(sx, sy)| house::is_hidden(content, &placed, &tall, &lamp, sx, sy, false));
    if let Some(at) = first_hidden {
        assert!(
            spots[at..]
                .iter()
                .all(|&(sx, sy)| house::is_hidden(content, &placed, &tall, &lamp, sx, sy, false)),
            "no spot in sight after a hidden one"
        );
    }

    // Turning an item keeps it where the player put it as long as it fits.
    let mut turned = holding(&lamp, 0, 0);
    turned.flipped = true;
    assert_eq!(
        settle(content, tier, &placements, &mut turned, &tall, true),
        Settle::Stayed
    );
    assert_eq!((turned.x, turned.y), (0, 0));
}

#[test]
fn when_every_spot_is_hidden_any_free_one_will_do() {
    let (content, tier, lamp, _, _) = lamp_and_bed();
    let tall = |id: &str| if id == lamp { 20 } else { 400 };
    let block = content
        .items()
        .find(|i| i.base.layer == Layer::Floor && i.base.footprint == [1, 1])
        .map(|i| i.id())
        .expect("a 1×1 floor item");
    // Tall pieces fill every row but the back one, so what is left is all behind them.
    let placements: Vec<PlacementRow> = (1..tier.depth)
        .flat_map(|y| (0..tier.width).map(move |x| (x, y)))
        .enumerate()
        .map(|(n, (x, y))| PlacementRow {
            id: n as i64 + 1,
            room_idx: 0,
            item_id: block.clone(),
            x: x as i16,
            y: y as i16,
            flipped: false,
        })
        .collect();
    let placed = placed_list(&placements);
    let mut state = holding(&lamp, 0, 3);
    let spots = free_spots(content, tier, &placements, &state, &tall);
    assert!(!spots.is_empty() && spots.iter().all(|&(_, y)| y == 0), "{spots:?}");
    assert!(
        spots
            .iter()
            .any(|&(x, y)| house::is_hidden(content, &placed, &tall, &lamp, x, y, false)),
        "the back row is behind the tall pieces"
    );
    // Nothing shows well, yet the lamp still goes to a free spot instead of being refused.
    assert_eq!(
        settle(content, tier, &placements, &mut state, &tall, false),
        Settle::Moved
    );
    assert!(spots.contains(&(state.x, state.y)));
}

#[test]
fn picks_the_right_placement_under_the_cursor() {
    let content = Content::get();
    let find = |layer: Layer, footprint: [u32; 2]| {
        content
            .items()
            .find(|i| i.base.layer == layer && i.base.footprint == footprint)
            .map(|i| i.id())
            .expect("content has this kind of item")
    };
    let row = |id: i64, item_id: String, x: i16, y: i16, flipped: bool| PlacementRow {
        id,
        room_idx: 0,
        item_id,
        x,
        y,
        flipped,
    };
    let rug = row(1, find(Layer::Rug, [3, 2]), 0, 0, false);
    let sofa = row(2, find(Layer::Floor, [3, 2]), 1, 1, true);
    let lamp = row(3, find(Layer::Surface, [1, 1]), 2, 2, false);
    let frame = row(4, find(Layer::Wall, [2, 2]), 1, 0, false);
    let all = [rug, sofa, lamp, frame];
    let at = |x, y, wall| under_cursor(content, &all, x, y, wall).map(|p| p.id);
    // The turned sofa covers x 1..3, y 1..4 (a 3×2 footprint would reach x = 3).
    assert_eq!(at(2, 3, None), Some(2));
    assert_eq!(at(3, 1, None), None);
    // Surface over floor over rug.
    assert_eq!(at(2, 2, None), Some(3));
    assert_eq!(at(1, 1, None), Some(2));
    assert_eq!(at(0, 0, None), Some(1));
    // Wall items only answer on their own wall.
    assert_eq!(at(2, 1, Some(false)), Some(4));
    assert_eq!(at(2, 1, Some(true)), None);
    assert_eq!(at(3, 0, Some(false)), None);
}

#[test]
fn edit_state_survives_the_custom_id() {
    let state = EditState {
        prefix: "pe",
        room: 2,
        x: 7,
        y: 3,
        page: 4,
        flipped: true,
        mode: CursorMode::LeftWall,
        item: Some("sofa__mint".to_owned()),
    };
    let id = state.id("ul");
    let mut parts = id.split(':');
    assert_eq!(parts.next(), Some("pe"));
    let (action, back) = EditState::parse("pe", &mut parts).expect("parses");
    assert_eq!(action, "ul");
    assert_eq!((back.room, back.x, back.y, back.page), (2, 7, 3, 4));
    assert!(back.flipped);
    assert_eq!(back.mode, CursorMode::LeftWall);
    assert_eq!(back.item.as_deref(), Some("sofa__mint"));
    // The cursor mode survives with an empty hand too.
    for mode in CursorMode::ALL {
        let empty = EditState {
            mode,
            item: None,
            ..state.clone()
        };
        let id = empty.id("dr");
        let mut parts = id.split(':');
        parts.next();
        let (_, back) = EditState::parse("pe", &mut parts).expect("parses");
        assert_eq!((back.mode, back.item), (mode, None));
    }
    // Editors from before the flip flag and before the cursor mode are told to start over.
    let mut old = "he:l:0:1:2:0:sofa".split(':');
    old.next();
    assert!(EditState::parse("he", &mut old).is_none());
    let mut old = "he:ul:0:1:2:0:0:sofa".split(':');
    old.next();
    assert!(EditState::parse("he", &mut old).is_none());
    let mut bad = "he:ul:0:1:2:0:0:x:sofa".split(':');
    bad.next();
    assert!(EditState::parse("he", &mut bad).is_none());
}

#[test]
fn empty_hand_cursor_follows_its_mode() {
    let content = Content::get();
    let at = |mode, item: Option<&str>, flipped| EditState {
        prefix: "he",
        room: 0,
        x: 0,
        y: 0,
        page: 0,
        flipped,
        mode,
        item: item.map(str::to_owned),
    };
    assert_eq!(at(CursorMode::Floor, None, false).wall(content), None);
    assert_eq!(at(CursorMode::RightWall, None, false).wall(content), Some(false));
    assert_eq!(at(CursorMode::LeftWall, None, false).wall(content), Some(true));
    // A held item decides for itself, whatever the mode says.
    let frame = content
        .items()
        .find(|i| i.base.layer == Layer::Wall)
        .map(|i| i.id())
        .expect("content has wall items");
    let sofa = content
        .items()
        .find(|i| i.base.layer == Layer::Floor)
        .map(|i| i.id())
        .expect("content has floor items");
    assert_eq!(at(CursorMode::Floor, Some(&frame), true).wall(content), Some(true));
    assert_eq!(at(CursorMode::LeftWall, Some(&sofa), false).wall(content), None);
    // Putting a held wall item down leaves the cursor on its wall.
    let mut state = at(CursorMode::Floor, Some(&frame), true);
    state.release(content);
    assert_eq!((state.mode, state.item), (CursorMode::LeftWall, None));
    let mut state = at(CursorMode::RightWall, Some(&sofa), false);
    state.release(content);
    assert_eq!(state.mode, CursorMode::Floor);
}

#[test]
fn custom_ids_fit_discords_limit() {
    for item in Content::get().items() {
        let state = EditState {
            prefix: "he",
            room: 5,
            x: 99,
            y: 99,
            page: 999,
            flipped: true,
            mode: CursorMode::RightWall,
            item: Some(item.id()),
        };
        assert!(state.id("dl").len() <= 100, "{} is too long", item.id());
    }
}
