use super::*;

#[test]
fn glimmer_variants_have_a_visible_sparkle_frame() {
    let card = DropCard {
        title: "Critter drop".into(),
        subtitle: String::new(),
        cards: vec![DropCritter {
            art: picture(),
            name: "Glimmer crab".into(),
            rarity: "Rare".into(),
            frame: 0x9af5ec,
            habitat: "Shore".into(),
            sparkles: 4,
        }],
        footer: String::new(),
        theme: Theme::reef(),
    };
    assert!(card.to_svg().contains("stroke-dasharray=\"12 8\""));
    let dex = DexCard {
        title: "Dex".into(),
        tab: "CRITTERS".into(),
        owner: "Sim".into(),
        found: 1,
        total: 1,
        page: 0,
        pages: 1,
        cells: vec![DexCell {
            number: 1,
            name: "Glimmer crab".into(),
            art: picture(),
            frame: 0x9af5ec,
            owned: true,
            caught: 1,
        }],
        footer: String::new(),
        lang: Lang::En,
        theme: Theme::reef(),
    };
    assert!(dex.to_svg().contains("stroke-dasharray=\"6 4\""));
}

fn picture() -> EntryPicture {
    EntryPicture {
        png: vec![],
        w: 32,
        h: 32,
    }
}

#[test]
fn a_dex_page_draws_locked_slots_dark_and_nameless() {
    let card = DexCard {
        title: "Tide pool dex".into(),
        tab: "CRITTERS".into(),
        owner: "Sim".into(),
        found: 1,
        total: 2,
        page: 0,
        pages: 1,
        cells: vec![
            DexCell {
                number: 1,
                name: "Pebblesnap Crab".into(),
                art: picture(),
                frame: 0x8fa0d8,
                owned: true,
                caught: 3,
            },
            DexCell {
                number: 2,
                name: "Secret".into(),
                art: picture(),
                frame: 0xffd166,
                owned: false,
                caught: 0,
            },
        ],
        footer: String::new(),
        lang: Lang::En,
        theme: Theme::reef(),
    };
    let svg = card.to_svg();
    assert!(svg.contains("Pebblesnap Crab") && svg.contains("x3"));
    assert!(!svg.contains("Secret"), "a locked slot never gives its name away");
    assert!(svg.contains("???") && svg.contains(r##"filter="url(#unseen)""##));
}

#[test]
fn a_long_name_is_shrunk_or_split_before_it_is_cut() {
    // English names keep the size they always had.
    assert_eq!(
        fit_name("Pebblesnap Crab", 32.0, 22.0, 372.0),
        ("Pebblesnap Crab".into(), 32.0)
    );
    assert_eq!(
        name_lines("Pebblesnap Crab", 14.0, 132.0),
        (vec!["Pebblesnap Crab".into()], 14.0)
    );
    // The longest Turkish ones are not cut: smaller on the big card, on two lines in a small box.
    for name in [
        "Thunderspine Deniz Kestanesi",
        "Cobbleshell Keşiş Yengeci",
        "Kapı Eşiği Keşiş Yengeci",
    ] {
        let (text, size) = fit_name(name, 32.0, 22.0, 372.0);
        assert!(text == name && (22.0..32.0).contains(&size), "{name}: {text} at {size}");
        assert!(laid_out_width(&text, SANS, true, size) <= 372.0, "{name} at {size}");
        for (box_width, base) in [(132.0, 14.0), (204.0, 16.0)] {
            let (lines, size) = name_lines(name, base, box_width);
            assert!(
                lines.len() <= 2 && lines.join(" ") == name && !lines.iter().any(|l| l.contains('…')),
                "{name} in {box_width}: {lines:?}"
            );
            for line in &lines {
                assert!(laid_out_width(line, SANS, true, size) <= box_width, "{line} at {size}");
            }
        }
    }
    // A name that cannot fit even so is cut, never left to spill.
    let (cut, size) = fit_name(&"W".repeat(40), 32.0, 22.0, 372.0);
    assert!(cut.ends_with('…') && size == 22.0 && laid_out_width(&cut, SANS, true, size) <= 372.0);
}

#[test]
fn a_haul_of_one_is_drawn_big_and_several_in_a_grid() {
    let tile = |name: &str| HaulTile {
        art: picture(),
        name: name.into(),
        rarity: "Rare".into(),
        frame: 0x3a9bdc,
        tag: "NEW".into(),
        line: "+14 shells".into(),
        flavor: "Sleeps through every storm.".into(),
    };
    let one = HaulCard {
        title: "Beachcomb".into(),
        subtitle: String::new(),
        tiles: vec![tile("Sunburst Star")],
        footer: String::new(),
        theme: Theme::reef(),
    };
    let svg = one.to_svg();
    assert!(svg.contains("width=\"760\"") && svg.contains("Sleeps through every storm."));
    let many = HaulCard {
        tiles: (0..11).map(|n| tile(&format!("Critter {n}"))).collect(),
        ..one
    };
    let svg = many.to_svg();
    assert!(
        svg.contains("Critter 7") && !svg.contains("Critter 8"),
        "eight tiles at most"
    );
}

#[test]
fn a_hauls_subtitle_sits_on_a_plate_that_covers_the_backgrounds_sparkle() {
    // The background puts a 4 px sparkle at 93% of the width and 8% of the height. On a card two rows
    // tall that is right where the 18 px subtitle is, so the text has a plate that covers it.
    let subtitle = "8 H SOAK";
    let (px, py, pw, ph) = subtitle_plate(subtitle);
    let text_width = pixel_width(subtitle, 18.0);
    for rows in 1..=2 {
        let (w, h) = (1000.0_f32, 108.0 + rows as f32 * 214.0 + 40.0);
        let (sx, sy) = ((w * 0.93).round(), (h * 0.08).round());
        let on_text = sx + 4.0 > 958.0 - text_width && sx < 958.0 && sy + 4.0 > 47.0 && sy < 60.0;
        let under_plate = sx >= px && sx + 4.0 <= px + pw && sy >= py && sy + 4.0 <= py + ph;
        assert!(
            !on_text || under_plate,
            "{rows} rows: the sparkle at {sx},{sy} shows on the subtitle"
        );
    }
    assert!(px + pw <= 1000.0 && px > 600.0, "the plate stays on the card");
    let tile = || HaulTile {
        art: picture(),
        name: "Pebblesnap Crab".into(),
        rarity: "Common".into(),
        frame: 0x8fa0d8,
        tag: String::new(),
        line: String::new(),
        flavor: String::new(),
    };
    let card = HaulCard {
        title: "Crab pot".into(),
        subtitle: subtitle.into(),
        tiles: vec![tile(), tile()],
        footer: String::new(),
        theme: Theme::reef(),
    };
    assert!(card.to_svg().contains(subtitle));
}
