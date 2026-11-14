use crate::commands::social::pet::*;

fn pet(hunger: i16, fun: i16, hygiene: i16, needs_at: DateTime<Utc>) -> PetRow {
    PetRow {
        id: 1,
        owner_id: 1,
        species: "crumbclaw_crab".to_owned(),
        name: "Pip".to_owned(),
        level: 1,
        xp: 0,
        hunger,
        fun,
        energy: 100,
        hygiene,
        needs_at,
        active: true,
        trip_dest: None,
        trip_ends_at: None,
    }
}

#[test]
fn a_pet_sleeps_a_day_after_everything_ran_out() {
    let t0 = Utc::now();
    let hours = |h: i64| t0 + chrono::Duration::hours(h);
    // 80 hunger lasts 20 h, 80 fun 16 h and 80 hygiene 32 h: all dry at 32 h, asleep at 56 h.
    let tended = pet(80, 80, 80, t0);
    assert_eq!(status(&tended, hours(1)), PetStatus::Awake);
    assert_eq!(status(&tended, hours(55)), PetStatus::Awake);
    assert_eq!(status(&tended, hours(57)), PetStatus::Asleep);
    // Already dry when stored: a day later.
    let dry = pet(0, 0, 0, t0);
    assert_eq!(status(&dry, hours(23)), PetStatus::Awake);
    assert_eq!(status(&dry, hours(25)), PetStatus::Asleep);
    // One need that was looked after keeps it up.
    assert_eq!(status(&pet(0, 0, 40, t0), hours(24 + 15)), PetStatus::Awake);
}

#[test]
fn a_pet_on_a_trip_keeps_its_needs_and_comes_back_to_a_souvenir() {
    let t0 = Utc::now();
    let hours = |h: i64| t0 + chrono::Duration::hours(h);
    let mut away = pet(40, 40, 40, hours(4));
    away.trip_dest = Some("kelp_forest_library".to_owned());
    away.trip_ends_at = Some(hours(4));
    // Mid-trip the needs are frozen and it cannot sleep; there is no souvenir yet.
    assert_eq!(status(&away, hours(1)), PetStatus::Away);
    assert_eq!(current_needs(&away, hours(1)).hunger, 40);
    assert_eq!(current_needs(&away, hours(4)).hunger, 40);
    // Back: needs drift from the return, and the souvenir waits.
    assert_eq!(status(&away, hours(5)), PetStatus::Back);
    assert_eq!(current_needs(&away, hours(5)).hunger, 36);
    assert!(away.has_souvenir(hours(5)) && !away.is_away(hours(5)));
    assert_eq!(place_of(&away, Lang::En), "Kelp Forest Library");
}

#[test]
fn spans_read_naturally() {
    assert_eq!(span(0, Lang::En), "under a minute");
    assert_eq!(span(1, Lang::En), "1 min");
    assert_eq!(span(25 * 60, Lang::En), "25 min");
    assert_eq!(span(3600, Lang::En), "1 h");
    assert_eq!(span(2 * 3600 + 5 * 60, Lang::En), "2 h 5 min");
    assert_eq!(span(-5, Lang::En), "under a minute");
    assert_eq!(span(2 * 3600 + 5 * 60, Lang::Tr), "2 sa 5 dk");
}

#[test]
fn a_level_up_names_the_trips_it_opens() {
    let note = level_up_note(Lang::En, "Pip", 3, 4);
    assert!(
        note.contains("reached level 4") && note.contains("Driftwood Cove"),
        "{note}"
    );
    let jump = level_up_note(Lang::En, "Pip", 1, 10);
    assert!(jump.contains("Driftwood Cove") && jump.contains("Sandcastle Shallows"));
    assert!(jump.contains("Kelp Forest Library"));
    assert!(!jump.contains("Lantern Lagoon"));
    assert!(!level_up_note(Lang::En, "Pip", 5, 6).contains("unlocked"));
}
