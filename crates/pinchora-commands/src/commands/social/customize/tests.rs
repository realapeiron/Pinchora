use pinchora_render::ProfileCard;

use crate::commands::social::customize::*;

fn wardrobe(stats: PlayerStats, bought: &[&str], equipped: &[&str]) -> Wardrobe {
    let owned = |ids: &[&str]| ids.iter().map(|s| (*s).to_owned()).collect();
    Wardrobe {
        stats,
        bought: owned(bought),
        equipped: owned(equipped),
        chosen: true,
        granted: Vec::new(),
    }
}

/// A player who never chose their badges: the card picks.
fn automatic(stats: PlayerStats, bought: &[&str]) -> Wardrobe {
    Wardrobe {
        chosen: false,
        ..wardrobe(stats, bought, &[])
    }
}

fn ids(badges: &[&Badge]) -> Vec<String> {
    badges.iter().map(|b| b.id.clone()).collect()
}

#[test]
fn badges_follow_real_progress() {
    let content = Content::get();
    let stats = PlayerStats {
        level: 7,
        fish_species: 12,
        ..Default::default()
    };
    let w = wardrobe(
        stats,
        &["tiny_tide"],
        &["bright_beacon", "tiny_tide", "little_beacon", "crab_cap"],
    );
    assert_eq!(
        ids(&w.worn()),
        ["tiny_tide", "little_beacon"],
        "level 7 has not reached 10, and the crab cap was never bought; the order stays as saved"
    );
    let wearable = w.wearable();
    assert!(
        wearable.iter().any(|b| b.id == "fin_finder"),
        "12 species earn the 10-species badge"
    );
    assert!(
        !wearable.iter().any(|b| b.id == "reef_regular"),
        "but not the 24-species one"
    );

    let line = |id: &str| w.line(content.badge(id).expect("a badge"), &w.worn(), Lang::En);
    assert_eq!(line("bright_beacon"), "🔒 **Bright Beacon** · 7/10 levels");
    assert_eq!(line("reef_regular"), "🔒 **Reef Regular** · 12/24 fish species");
    assert_eq!(line("little_beacon"), "✅ **Little Beacon** · Reach level 5. *(worn)*");
    assert_eq!(
        line("fin_finder"),
        "✅ **Fin Finder** · Catch 10 different fish species."
    );
    assert_eq!(line("tiny_tide"), "✅ **Tiny Tide** · bought *(worn)*");
    assert_eq!(line("crab_cap"), "🛒 **Crab Cap** · 1,500 shells");
}

#[test]
fn a_badge_that_stopped_counting_leaves_the_card() {
    // A seven-day streak badge, saved while the streak was alive.
    let saved = ["week_of_waves"];
    assert_eq!(
        ids(&wardrobe(
            PlayerStats {
                streak: 9,
                ..Default::default()
            },
            &[],
            &saved
        )
        .worn()),
        ["week_of_waves"]
    );
    assert!(wardrobe(PlayerStats::default(), &[], &saved).worn().is_empty());
    // Ids the content no longer has are ignored too.
    assert!(wardrobe(PlayerStats::default(), &[], &["gone_badge"]).worn().is_empty());
}

#[test]
fn until_they_choose_the_card_shows_the_best_earned_badges_rarest_first() {
    // Nothing earned, nothing shown.
    assert!(automatic(PlayerStats::default(), &[]).worn().is_empty());

    // Level 25 (rare), 12 species (common), a 7-day streak (common), 60 reputation (uncommon),
    // a bought common badge and a bought legendary one.
    let stats = PlayerStats {
        level: 25,
        fish_species: 12,
        streak: 7,
        reputation: 60,
        ..Default::default()
    };
    let w = automatic(stats, &["tiny_tide", "pearl_wyrm"]);
    assert_eq!(
        ids(&w.worn()),
        [
            "pearl_wyrm",
            "harbour_beacon",
            "bright_beacon",
            "harbour_helper",
            "little_beacon",
            "fin_finder"
        ],
        "legendary, rare, the two uncommon ones in content order, then the first commons"
    );
    assert_eq!(w.worn().len(), MAX_BADGES);

    // Once they chose, only the choice counts, however good the rest is.
    let chosen = wardrobe(stats, &["tiny_tide", "pearl_wyrm"], &["tiny_tide"]);
    assert_eq!(ids(&chosen.worn()), ["tiny_tide"]);
    assert!(
        wardrobe(stats, &[], &[]).worn().is_empty(),
        "an explicit empty choice is no badges"
    );
}

#[test]
fn the_collection_shows_every_badge_in_its_state() {
    let stats = PlayerStats {
        level: 7,
        fish_species: 3,
        ..Default::default()
    };
    let w = wardrobe(stats, &["tiny_tide"], &["tiny_tide", "little_beacon"]);
    let new = fresh_mask(&[
        "little_beacon".to_owned(),
        "tiny_tide".to_owned(),
        "not_a_badge".to_owned(),
    ]);
    let cells = collection_cells(&Sprites::default(), &w, &w.worn(), new, Lang::En);
    let content = Content::get();
    assert_eq!(cells.len(), content.badges.len(), "every badge has a cell");
    let state = |id: &str| {
        let i = content.badges.iter().position(|b| b.id == id).expect("a badge");
        cells[i].state
    };
    assert_eq!(
        state("little_beacon"),
        BadgeState::Earned {
            new: true,
            worn: Some(2)
        },
        "worn second: the order is the saved one"
    );
    assert_eq!(
        state("tiny_tide"),
        BadgeState::Earned {
            new: true,
            worn: Some(1)
        }
    );
    assert_eq!(state("bright_beacon"), BadgeState::Locked { have: 7, at: 10 });
    assert_eq!(state("fin_finder"), BadgeState::Locked { have: 3, at: 10 });
    assert_eq!(state("crab_cap"), BadgeState::ForSale { price: 1_500 });
    // The plate colour is the rarity's.
    let i = content
        .badges
        .iter()
        .position(|b| b.id == "pearl_wyrm")
        .expect("a badge");
    assert_eq!(cells[i].rarity, Rarity::Legendary.color());

    // The NEW markers survive a round trip through the buttons' hex ids.
    assert_eq!(fresh_mask(&[]), 0);
    let hex = format!("{new:x}");
    assert!(format!("cz:ws:1:{hex}").len() <= 100);
    assert_eq!(u128::from_str_radix(&hex, 16).ok(), Some(new));
    assert!(is_fresh(new, 0) && !is_fresh(new, 1));
    assert!(!is_fresh(new, 500), "out of range is never new");
}

#[test]
fn the_badge_list_fits_one_embed_whatever_is_earned() {
    let everything = PlayerStats {
        level: 1_000,
        fish_species: 72,
        house_likes: 1_000_000,
        pets: 99,
        streak: 1_000,
        placed: 1_000_000,
        reputation: 1_000_000,
        shells: 1_000_000_000,
    };
    let bought: Vec<_> = Content::get().badges.iter().map(|b| b.id.as_str()).collect();
    let worn: Vec<_> = bought.iter().copied().take(MAX_BADGES).collect();
    for (lang, w) in Lang::ALL.into_iter().flat_map(|lang| {
        [
            (lang, wardrobe(PlayerStats::default(), &[], &[])),
            (lang, wardrobe(everything, &bought, &worn)),
        ]
    }) {
        let fields = badge_fields(&w, lang);
        assert_eq!(fields.len(), BadgeStat::ALL.len() + 1);
        assert!(
            fields.iter().all(|(_, lines)| lines.chars().count() <= 1024),
            "a field value is capped at 1,024"
        );
        let chars: usize = fields
            .iter()
            .map(|(name, lines)| name.chars().count() + lines.chars().count())
            .sum();
        // The title, the description (six badge names and some text) and the fields share Discord's 6,000.
        assert!(chars + 600 < 6_000, "{chars} characters of fields");
    }
}

#[test]
fn custom_ids_and_options_fit_discords_limits() {
    let content = Content::get();
    for b in &content.backgrounds {
        assert!(
            format!("cz:bp:99:{}", b.id).len() <= 100 && b.name.chars().count() <= 100,
            "{}",
            b.id
        );
    }
    for b in &content.badges {
        let option = option(&b.name, &b.id, format!("999,999 shells · {}", b.description), false);
        assert!(b.id.len() <= 100 && option.label.chars().count() <= 100);
        assert!(option.description.is_some_and(|d| d.chars().count() <= 100));
    }
}

#[test]
fn the_card_has_room_for_what_the_database_allows() {
    assert_eq!(MAX_BADGES, ProfileCard::MAX_BADGES);
}

/// A player row with the defaults every new player has.
fn player() -> UserRow {
    UserRow {
        id: 1,
        credits: 0,
        xp: 0,
        reputation: 0,
        daily_streak: 0,
        last_daily_at: None,
        last_rep_at: None,
        bio: String::new(),
        accent_color: None,
        created_at: chrono::Utc::now(),
        pearls: 0,
        character: None,
        background: None,
        badges: Vec::new(),
        badges_set: false,
        badges_seen: None,
        title: None,
        accent: None,
        panel_opacity: 1,
        frame: "plain".to_owned(),
        showcase_fish: None,
        showcase_pet: None,
        showcase_item: None,
        showcase_house: false,
    }
}

#[test]
fn the_accent_follows_the_background_unless_the_player_chose_one() {
    let content = Content::get();
    let background = content.background("shellwash_morning").expect("a background");
    let mut row = player();
    row.accent_color = Some(0x112233);
    assert_eq!(
        theme(&row, Some(background)).accent,
        background.accent,
        "the picture wins over the colour from before accents were a choice"
    );
    assert_eq!(theme(&row, None).accent, "#112233", "else that colour");
    assert_eq!(theme(&player(), None).accent, Theme::reef().accent);

    // A chosen accent beats the background; Auto (NULL) gives the background back.
    row.accent = Some("#06d6a0".to_owned());
    assert_eq!(theme(&row, Some(background)).accent, "#06d6a0");
    assert_eq!(theme(&row, None).accent, "#06d6a0");
    // A value the card could not show is ignored, as if the player had never chosen.
    row.accent = Some("#000000".to_owned());
    assert_eq!(theme(&row, Some(background)).accent, background.accent);
    row.accent = Some("nonsense".to_owned());
    assert_eq!(theme(&row, None).accent, "#112233");
}

#[test]
fn frames_and_opacity_come_from_the_row_and_frames_need_their_level() {
    let mut row = player();
    assert_eq!(theme(&row, None).frame, Frame::Plain);
    assert_eq!(theme(&row, None).opacity, Opacity::Normal);
    row.panel_opacity = 0;
    assert_eq!(theme(&row, None).opacity, Opacity::Light);
    row.panel_opacity = 9;
    assert_eq!(theme(&row, None).opacity, Opacity::Normal, "an unknown value is Normal");

    row.frame = "pearl".to_owned();
    assert_eq!(frame_of(&row), Frame::Plain, "level 1 has not unlocked the pearl frame");
    row.xp = 30_000; // level 20 takes 23,850 XP in all
    assert!(leveling::level_of(row.xp) >= 20);
    assert_eq!(frame_of(&row), Frame::Pearl);
    row.frame = "rope".to_owned();
    row.xp = 0;
    assert_eq!(frame_of(&row), Frame::Rope, "the rope frame is free");
    row.frame = "no_such_frame".to_owned();
    assert_eq!(frame_of(&row), Frame::Plain);
}
