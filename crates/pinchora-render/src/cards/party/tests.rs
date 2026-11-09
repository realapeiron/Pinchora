use super::*;

use image::RgbaImage;

use crate::{Renderer, encode_png};

fn png() -> Vec<u8> {
    encode_png(&RgbaImage::from_pixel(72, 68, image::Rgba([0xe0, 0x4a, 0x3f, 255]))).expect("encodes")
}

fn size(png: &[u8]) -> (u32, u32) {
    let image = image::load_from_memory(png).expect("a png");
    (image.width(), image.height())
}

#[test]
fn the_derby_lobby_shows_every_lane_and_the_pool() {
    let sprite = png();
    let names: Vec<String> = ["Ann", "Bo", "Cy"].map(String::from).to_vec();
    let lanes: Vec<DerbyLane<'_>> = (0..6)
        .map(|n| DerbyLane {
            name: ["Pebble", "Scuttle", "Kelp", "Barnacle", "Clawdia", "Tidbit"][n],
            accent: 0xff6b4a,
            art: SpriteArt {
                png: &sprite,
                w: 72,
                h: 68,
            },
            backers: if n == 1 { &names } else { &[] },
        })
        .collect();
    let card = DerbyCard {
        lang: Lang::En,
        lanes: &lanes,
        stake: 50,
        racers: 3,
        status: "Pick your crab",
        theme: Theme::reef(),
    };
    let svg = card.to_svg();
    for name in [
        "Pebble",
        "Tidbit",
        "LANE 6",
        "NO BACKERS",
        "3 BACKERS",
        "POOL 150",
        "Ann +2",
    ] {
        assert!(svg.contains(name), "{name}");
    }
    assert_eq!(svg.matches("<image").count(), 6, "a crab per lane");
    assert_eq!(size(&Renderer::new().svg_to_png(&svg).expect("renders")), (600, 470));
    let free = DerbyCard { stake: 0, ..card }.to_svg();
    assert!(free.contains("FOR FUN") && !free.contains("POOL"));
}

#[test]
fn share_or_grab_hides_the_picks_until_both_are_in() {
    let card = |picks, banner| ShareGrabCard {
        lang: pinchora_core::infra::i18n::Lang::En,
        names: ["Ann", "A very very long display name"],
        picks,
        pool: 60,
        wager: false,
        banner,
        mood: Mood::Wait,
        theme: Theme::reef(),
    };
    let waiting = card([PickView::Locked, PickView::Thinking], "Waiting for both").to_svg();
    assert!(waiting.contains("LOCKED IN") && waiting.contains("THINKING...") && waiting.contains("?"));
    assert!(!waiting.contains(">SHARE<") && !waiting.contains(">GRAB<"));
    let shown = card([PickView::Share, PickView::Grab], "Ann was tricked").to_svg();
    assert!(shown.contains(">SHARE<") && shown.contains(">GRAB<"));
    assert_eq!(size(&Renderer::new().svg_to_png(&shown).expect("renders")), (580, 350));
}

#[test]
fn the_shell_game_lifts_the_shells_to_show_the_pearl() {
    let card = |picked, pearl| ShellCard {
        lang: Lang::En,
        stake: 20,
        picked,
        pearl,
        status: "Pick a shell",
        payout: 50,
        theme: Theme::reef(),
    };
    let hidden = card(None, None).to_svg();
    assert!(!hidden.contains("YOUR PICK"));
    let shown = card(Some(1), Some(1)).to_svg();
    assert!(hidden.contains("STAKE 20 PAYS 50"));
    assert!(shown.contains("YOUR PICK"));
    assert!(
        shown.matches("<rect").count() > hidden.matches("<rect").count(),
        "the pearl is drawn"
    );
    assert_eq!(size(&Renderer::new().svg_to_png(&shown).expect("renders")), (520, 320));
}

#[test]
fn a_right_pick_is_green_a_wrong_one_red_and_the_status_has_a_plate_to_itself() {
    let card = |picked, pearl| ShellCard {
        lang: Lang::En,
        stake: 20,
        picked,
        pearl,
        status: "Pick a shell",
        payout: 50,
        theme: Theme::reef(),
    };
    let t = Theme::reef();
    let found = card(Some(1), Some(1)).to_svg();
    assert!(
        found.contains(&t.mint) && !found.contains(&t.accent),
        "a found pearl is green"
    );
    let missed = card(Some(0), Some(2)).to_svg();
    assert!(missed.contains(&t.accent) && !missed.contains(&t.mint), "a miss is red");
    // The fixed sparkle at 70% of the width and 94% of the height used to sit right after the words.
    let png = Renderer::new().svg_to_png(&card(None, None).to_svg()).expect("renders");
    let image = image::load_from_memory(&png).expect("a PNG").to_rgba8();
    assert_eq!(
        image.get_pixel(366, 302),
        image.get_pixel(400, 302),
        "no sparkle beside the status"
    );
}

#[test]
fn the_heist_card_shows_the_crew_and_how_each_stage_went() {
    let crew = [
        CrewView { name: "Ann", role: 0 },
        CrewView { name: "Bo", role: 1 },
        CrewView { name: "Cy", role: 2 },
    ];
    let stage = |passed| StageView {
        title: "Crack the clam lock",
        chance: 64,
        passed,
        line: "Click, click, clunk: the clam lock yawns open.",
    };
    let card = |passed: Option<bool>, headline| HeistCard {
        lang: Lang::En,
        crew: &crew,
        stages: [stage(passed), stage(passed), stage(passed)],
        headline,
        each: if passed.is_some() { 60 } else { 0 },
        theme: Theme::reef(),
    };
    let lobby = card(None, "CREW 3/8").to_svg();
    assert!(lobby.contains("SEALED") && lobby.contains("TO COME") && !lobby.contains("EACH"));
    let won = card(Some(true), "THE VAULT IS OPEN").to_svg();
    assert!(won.contains("OPEN!") && won.contains("PASSED") && won.contains("60 EACH"));
    let lost = card(Some(false), "BUSTED").to_svg();
    assert!(lost.contains("STAYS SHUT") && lost.contains("FAILED"));
    // Three in the crew take two rows, and the card is only as tall as they need.
    assert_eq!(size(&Renderer::new().svg_to_png(&won).expect("renders")), (600, 504));
    assert!(card(None, "x").to_svg().contains("LOOKOUT") && card(None, "x").to_svg().contains("DIVER"));
}

#[test]
fn the_heist_card_is_as_tall_as_its_crew_and_a_good_haul_is_not_red() {
    let crew = |n: usize| -> Vec<CrewView<'static>> {
        (0..n)
            .map(|k| CrewView {
                name: "Ann",
                role: k % 3,
            })
            .collect()
    };
    let stage = |passed| StageView {
        title: "Crack the clam lock",
        chance: 64,
        passed,
        line: "Click, click, clunk.",
    };
    let card = |crew: &[CrewView<'_>], stages: [Option<bool>; 3], headline: &str| {
        HeistCard {
            lang: Lang::En,
            crew,
            stages: stages.map(stage),
            headline,
            each: 0,
            theme: Theme::reef(),
        }
        .to_svg()
    };
    let height = |n: usize| {
        let crew = crew(n);
        size(
            &Renderer::new()
                .svg_to_png(&card(&crew, [None; 3], "CREW"))
                .expect("renders"),
        )
        .1
    };
    assert_eq!(
        [0, 1, 2, 3, 4, 5, 6, 7, 8, 9].map(height),
        [476, 476, 476, 504, 504, 532, 532, 560, 560, 560]
    );

    // The headline's colour: gold when the vault opens, mint for a haul, red only when nothing came out.
    let team = crew(3);
    let t = Theme::reef();
    let colour_of = |svg: &str, headline: &str| {
        let end = svg.find(&format!(">{headline}</text>")).expect("the headline is drawn");
        let start = svg[..end].rfind("<text").expect("in a text element");
        svg[start..end]
            .split("fill=\"")
            .nth(1)
            .and_then(|rest| rest.split('"').next())
            .map(str::to_owned)
    };
    let (yes, no) = (Some(true), Some(false));
    for (stages, headline, expected) in [
        ([yes, yes, yes], "THE VAULT IS OPEN", &t.gold),
        ([yes, yes, no], "A GOOD HAUL", &t.mint),
        ([yes, no, no], "SCRAPS ONLY", &t.mint),
        ([no, no, no], "BUSTED", &t.accent),
    ] {
        let svg = card(&team, stages, headline);
        assert_eq!(
            colour_of(&svg, headline).as_deref(),
            Some(expected.as_str()),
            "{headline}"
        );
    }
}

#[test]
fn the_storm_wall_rises_with_the_sandbags_and_the_card_says_how_it_ended() {
    let top = vec![("Ann".to_owned(), 12), ("Bo".to_owned(), 9)];
    let card = |wall, state| SurgeCard {
        lang: Lang::En,
        wall,
        goal: 40,
        helpers: 2,
        top: &top,
        state,
        theme: Theme::reef(),
    };
    let bags = |svg: &str| svg.matches("fill=\"#c9ab6a\"").count() + svg.matches("fill=\"#b99a5a\"").count();
    let low = card(2, SurgeState::Rolling).to_svg();
    let high = card(36, SurgeState::Rolling).to_svg();
    assert!(bags(&high) > bags(&low) * 2, "{} then {}", bags(&low), bags(&high));
    assert!(low.contains("HOLD THE WALL") && low.contains("WALL 2 / 40") && low.contains("2 CRABS HELPING"));
    assert!(card(40, SurgeState::Held).to_svg().contains("THE WALL HELD"));
    assert!(card(3, SurgeState::Broken).to_svg().contains("THE SEA WON"));
    assert!(low.contains("Ann") && low.contains("12 BAGS"));
    assert_eq!(size(&Renderer::new().svg_to_png(&high).expect("renders")), (640, 380));
    // An empty wall and a goal of nothing do not break the drawing.
    let _ = card(0, SurgeState::Rolling).to_svg();
    let _ = SurgeCard {
        lang: Lang::En,
        goal: 0,
        ..card(0, SurgeState::Rolling)
    }
    .to_svg();
}
