//! The duel cards in Turkish: every label comes out of the catalog (a missing key would be drawn as itself),
//! the pixel font draws the Turkish letters, and the cards still render.

use pinchora_core::i18n::Lang;
use pinchora_render::{
    CheckersCard, ChessCard, Connect4Card, FleetCard, Mood, PickView, RadarCard, Renderer, SeaView, ShareGrabCard,
    ShipView, Theme,
};

/// No catalog key (`board.card.winner`, `seafleet.card.sunk`) may be drawn as text.
fn assert_no_keys(svg: &str) {
    for key in [
        "board.",
        "connect4.",
        "checkers.",
        "chess.",
        "seafleet.",
        "share.",
        "duel.",
    ] {
        assert!(!svg.contains(key), "{key}… is drawn as text: {svg}");
    }
}

fn png_size(png: &[u8]) -> (u32, u32) {
    let image = image::load_from_memory(png).expect("a png");
    (image.width(), image.height())
}

#[test]
fn connect_four_names_its_discs_and_the_winner_in_turkish() {
    let cells = [0u8; 42];
    let card = |turn, winner| Connect4Card {
        lang: Lang::Tr,
        cells: &cells,
        names: ["Ada", "Can"],
        turn,
        winner,
        win_cells: &[],
        last: None,
        status: "SIRA: CAN",
        pot: 0,
        theme: Theme::reef(),
    };
    let svg = card(Some(2), None).to_svg();
    assert!(svg.contains("MERCAN") && svg.contains("SENİN SIRAN"), "{svg}");
    assert!(svg.contains("SIRA: CAN"));
    let won = card(None, Some(1)).to_svg();
    assert!(won.contains("KAZANAN!") && won.contains("GÖK"), "{won}");
    assert_no_keys(&svg);
    assert_no_keys(&won);
    let png = Renderer::new().connect4_card(&card(Some(1), None)).expect("renders");
    assert_eq!(png_size(&png), (580, 670));
}

#[test]
fn the_board_cards_count_pieces_and_name_the_sides_in_turkish() {
    let cells: [u8; 32] = std::array::from_fn(|sq| match sq {
        0..=11 => 2,
        20..=31 => 1,
        _ => 0,
    });
    let checkers = CheckersCard {
        lang: Lang::Tr,
        cells: &cells,
        names: ["Ada", "Can"],
        turn: Some(0),
        winner: None,
        last: &[],
        selected: None,
        targets: &[],
        status: "SIRA: ADA",
        pot: 200,
        theme: Theme::reef(),
    };
    let svg = checkers.to_svg();
    assert!(svg.contains("SENİN SIRAN") && svg.contains("12 KALDI"), "{svg}");
    assert_no_keys(&svg);
    let png = Renderer::new().checkers_card(&checkers).expect("renders");
    assert_eq!(png_size(&png), (CheckersCard::WIDTH, CheckersCard::HEIGHT));

    let pieces = [0u8; 64];
    let chess = ChessCard {
        lang: Lang::Tr,
        pieces: &pieces,
        names: ["Ada", "Can"],
        turn: Some(1),
        winner: None,
        last: None,
        selected: None,
        targets: &[],
        check: None,
        status: "SIRA: CAN, ŞAH TEHLİKEDE!",
        pot: 0,
        theme: Theme::reef(),
    };
    let svg = chess.to_svg();
    // The first player's note names the side that moves first; the second player is on turn.
    assert!(svg.contains("MERCAN, İLK") && svg.contains("SENİN SIRAN"), "{svg}");
    assert_no_keys(&svg);
    let over = ChessCard {
        turn: None,
        winner: Some(1),
        ..chess
    };
    let svg = over.to_svg();
    assert!(svg.contains("KAZANAN!") && svg.contains("MERCAN, İLK"), "{svg}");
    let png = Renderer::new().chess_card(&over).expect("renders");
    assert_eq!(png_size(&png), (ChessCard::WIDTH, ChessCard::HEIGHT));
}

#[test]
fn the_sea_fleet_cards_speak_turkish_down_to_the_ship_names() {
    let shots: String = ".".repeat(100);
    let ships = |sunk: bool| {
        vec![
            ShipView {
                cell: 0,
                horizontal: true,
                ship: 0,
                sunk: false,
            },
            ShipView {
                cell: 50,
                horizontal: true,
                ship: 3,
                sunk,
            },
        ]
    };
    let radar = |turn, winner| RadarCard {
        lang: Lang::Tr,
        seas: [
            SeaView {
                name: "Ada",
                shots: &shots,
                ships: Vec::new(),
                afloat: 5,
            },
            SeaView {
                name: "Can",
                shots: &shots,
                ships: ships(true),
                afloat: 4,
            },
        ],
        turn,
        winner,
        last: None,
        status: "ATEŞ SIRASI: ADA",
        pot: 100,
        theme: Theme::reef(),
    };
    let svg = radar(Some(0), None).to_svg();
    // The pixel font draws Ş and İ directly.
    assert!(svg.contains("BURAYA ATEŞ") && svg.contains("BEKLİYOR"), "{svg}");
    let won = radar(None, Some(0)).to_svg();
    assert!(won.contains("KAZANAN!") && won.contains("FİLO BATTI"), "{won}");
    assert_no_keys(&svg);
    assert_no_keys(&won);
    let png = Renderer::new().radar_card(&radar(Some(0), None)).expect("renders");
    assert_eq!(png_size(&png), (RadarCard::WIDTH, RadarCard::HEIGHT));

    let fleet = FleetCard {
        lang: Lang::Tr,
        title: "FİLON",
        subtitle: "5 GEMİDEN 4 TANESİ YÜZÜYOR",
        sea: SeaView {
            name: "Ada",
            shots: &shots,
            ships: ships(true),
            afloat: 4,
        },
        sunk: [false, false, false, true, false],
        footer: "BUNU YALNIZCA SEN GÖRÜYORSUN",
        last: None,
        theme: Theme::reef(),
    };
    let svg = fleet.to_svg();
    for ship in ["Kalyon", "Fırkateyn", "Brik", "Şalupa (battı)", "Sandal"] {
        assert!(svg.contains(ship), "{ship}: {svg}");
    }
    assert!(svg.contains("FİLON"), "{svg}");
    assert_no_keys(&svg);
    let png = Renderer::new().fleet_card(&fleet).expect("renders");
    assert_eq!(png_size(&png), (FleetCard::WIDTH, FleetCard::HEIGHT));
}

#[test]
fn share_or_grab_labels_its_pool_and_picks_in_turkish() {
    let card = |picks, wager| ShareGrabCard {
        lang: Lang::Tr,
        names: ["Ada", "Can"],
        picks,
        pool: 60,
        wager,
        banner: "KİMSE KAZANMADI",
        mood: Mood::Wait,
        theme: Theme::reef(),
    };
    let svg = card([PickView::Locked, PickView::Thinking], false).to_svg();
    assert!(
        svg.contains("PAYLAŞ YA DA KAP") && svg.contains("DENİZİN HAVUZU") && svg.contains("HAVUZDA"),
        "{svg}"
    );
    assert!(svg.contains("SEÇTİ") && svg.contains("DÜŞÜNÜYOR..."), "{svg}");
    assert!(
        !svg.contains(">PAYLAŞ<") && !svg.contains(">KAP<"),
        "the picks stay hidden"
    );
    let svg = card([PickView::Share, PickView::Grab], true).to_svg();
    assert!(
        svg.contains("YATIRIMLARINIZ") && svg.contains(">PAYLAŞ<") && svg.contains(">KAP<"),
        "{svg}"
    );
    assert_no_keys(&svg);
    let png = Renderer::new()
        .share_grab_card(&card([PickView::Share, PickView::Grab], true))
        .expect("renders");
    assert_eq!(
        png_size(&png),
        (ShareGrabCard::WIDTH as u32, ShareGrabCard::HEIGHT as u32)
    );
}
