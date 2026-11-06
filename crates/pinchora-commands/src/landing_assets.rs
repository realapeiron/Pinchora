//! The pictures on the public website, drawn by the bot's own renderer from sample data: the profile
//! card, a room, a few game boards, the welcome, rank and wallet cards, the Open Graph image for link
//! previews and the favicons. They are committed under `web/` so the site needs no bot to run, and
//! `landing_assets_match` fails as soon as a card, the art or the catalogue moves away from what is committed.
//!
//! Regenerate with `cargo test -p pinchora-commands write_landing_assets -- --ignored` and commit
//! `web/src/site/showcase` and `web/public`.

use std::{
    fs,
    path::{Path, PathBuf},
};

use image::{
    ExtendedColorType, ImageEncoder, Rgba, RgbaImage,
    codecs::png::{CompressionType, FilterType, PngEncoder},
    imageops,
};
use pinchora_core::i18n::Lang;
use pinchora_db::{PlacementRow, RoomRow};
use pinchora_game::{Content, character::Character};
use pinchora_render::{
    ABSENT, Backdrop, BadgeCell, BadgeCollectionCard, BadgeIcon, BadgeState, CORRECT, ChessCard, Connect4Card,
    LeaderboardCard, LeaderboardEntry, LevelUpCard, NextRole, PRESENT, PetCard, PetStatus, ProfileCard, RankCard,
    Renderer, ShowcaseKind, ShowcaseSlot, SpriteArt, Theme, WalletCard, WelcomeCard, WordleCard, WordleRow, encode_png,
    placeholder_avatar, sprites::placeholder_badge_color, thousands,
};

use crate::{
    commands::customize::Cosmetics,
    game::scene::{RoomPet, RoomView, Sprites, room_image, scaled_png},
    house_view::RoomData,
};

const REGENERATE: &str = "run `cargo test -p pinchora-commands write_landing_assets -- --ignored` and commit web/src/site/showcase and web/public";

fn web() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../web")
}

/// A picture the site uses: where it lives under `web/`, and what it looks like.
struct Asset {
    path: String,
    image: RgbaImage,
}

fn asset(path: &str, image: RgbaImage) -> Asset {
    Asset {
        path: path.to_owned(),
        image,
    }
}

fn decode(png: &[u8]) -> RgbaImage {
    image::load_from_memory(png).expect("a card is a PNG").to_rgba8()
}

fn sprite_art(art: &crate::game::scene::ScaledArt) -> SpriteArt<'_> {
    SpriteArt {
        png: &art.png,
        w: art.w,
        h: art.h,
    }
}

// /Characters and rooms

fn character(code: &str) -> Character {
    Character::decode(Content::get(), code).unwrap_or_else(|| panic!("`{code}` is not a character"))
}

/// The room the bot's own preview draws, to see the art in place (the same pieces as the browser parity fixtures).
fn placements() -> Vec<PlacementRow> {
    [
        ("pebblepane_window__sand", 1, 0, false),
        ("hemhill_painting__cream", 4, 0, false),
        ("pebblehour_clock__navy", 7, 0, false),
        ("dunearc_window__coral", 2, 0, true),
        ("stillframe_door__cream", 6, 1, true),
        ("stillpanel_wardrobe__cream", 0, 0, false),
        ("stillstep_nightstand__cream", 6, 0, false),
        ("pebblecap_lamp__sand", 6, 0, false),
        ("patchpillow_bed__cream", 7, 0, false),
        ("humskein_upright__plum", 0, 3, true),
        ("rivetgrid_rug__sand", 2, 4, false),
        ("crumbcorner_table__moss", 3, 4, false),
        ("pagepair_books__moss", 3, 4, false),
        ("sprigcup_fern__moss", 4, 5, false),
        ("plainwhorl_chair__sand", 5, 4, false),
        ("plainwhorl_chair__sand", 3, 6, true),
        ("crumbrail_bench__sand", 6, 7, false),
        ("stillstem_rubber_tree__cream", 0, 7, false),
        ("rivetarm_floor_lamp__charcoal", 9, 4, false),
        ("blinkslot_cabinet__plum", 8, 7, false),
        ("duneline_runner__coral", 6, 3, false),
    ]
    .iter()
    .enumerate()
    .map(|(i, (item_id, x, y, flipped))| PlacementRow {
        id: i as i64 + 1,
        room_idx: 0,
        item_id: (*item_id).to_owned(),
        x: *x,
        y: *y,
        flipped: *flipped,
    })
    .collect()
}

fn room_view() -> RoomView {
    let content = Content::get();
    RoomData {
        width: 10,
        depth: 9,
        room: RoomRow {
            idx: 0,
            room_type: "living".to_owned(),
            wallpaper: content.surfaces.wallpapers[0].id.clone(),
            floor: content.surfaces.floors[0].id.clone(),
        },
        placements: placements(),
        pets: vec![RoomPet::awake(content.pets[0].id.clone())],
        characters: vec![character("tidewalker.2.1.1.2.7.0+straw_hat*3")],
        seed: 3,
    }
    .view()
}

// /Cards

/// The profile card with a background, six badges and a showcase row, for a player called Captain Pinch.
fn profile(r: &Renderer, sprites: &Sprites) -> anyhow::Result<RgbaImage> {
    let content = Content::get();
    let background = content
        .backgrounds
        .iter()
        .find(|b| b.id == "starlit_jetty")
        .expect("a background");
    let badges: Vec<&pinchora_game::Badge> = [
        "little_beacon",
        "fin_finder",
        "pawshake",
        "week_of_waves",
        "pearl_wyrm",
        "year_at_the_pier",
    ]
    .iter()
    .map(|id| content.badges.iter().find(|b| b.id == *id).expect("a badge"))
    .collect();
    let cosmetics = Cosmetics::new(sprites, Some(background), &badges);
    let icons = cosmetics.icons();
    let player = character("quiffmoor.1.7.2.19.7.3+bow_tie*1+captain_hat+lantern");
    let (character_png, character_w, character_h) =
        scaled_png(&sprites.character(&player), ProfileCard::CHARACTER_SCALE)?;
    let fish = sprites.showcase_fish("amberwink_squidlet").expect("a fish");
    let pet = sprites.showcase_pet(&content.pets[0].id).expect("a pet");
    let furniture = sprites
        .showcase_item(content, "humskein_upright__plum")
        .expect("furniture");
    let house = sprites.house_thumbnail(&room_view()).expect("a house");
    let slots = [
        ShowcaseSlot {
            kind: ShowcaseKind::Fish,
            art: sprite_art(&fish),
            name: "Amberwink Squidlet",
        },
        ShowcaseSlot {
            kind: ShowcaseKind::Pet,
            art: sprite_art(&pet),
            name: "Sir Clicks-a-lot",
        },
        ShowcaseSlot {
            kind: ShowcaseKind::Furniture,
            art: sprite_art(&furniture),
            name: "Humskein Upright",
        },
        ShowcaseSlot {
            kind: ShowcaseKind::House,
            art: sprite_art(&house),
            name: "Tide Pool Shack",
        },
    ];
    let accent = background.accent_rgb();
    let png = r.profile_card(&ProfileCard {
        display_name: "Captain Pinch",
        title: Some("Tide Angler"),
        username: "captain.pinch",
        avatar_png: &placeholder_avatar(42, ProfileCard::AVATAR_SIZE),
        bio: "Collector of shiny shells, builder of tiny pixel houses. Ask me about my crab, Sir Clicks-a-lot!",
        level: 27,
        xp_current: 3_120,
        xp_needed: 5_095,
        total_xp: 64_820,
        credits: 1_284_500,
        reputation: 133,
        streak: 12,
        global_rank: 1_337,
        server_rank: Some(4),
        character: Some(SpriteArt {
            png: &character_png,
            w: character_w,
            h: character_h,
        }),
        backdrop: cosmetics.backdrop(),
        badges: &icons,
        showcase: &slots,
        partner: Some("Marlowe".to_owned()),
        lang: Lang::En,
        theme: Theme::reef().with_accent(accent),
    })?;
    Ok(decode(&png))
}

fn rank(r: &Renderer, sprites: &Sprites) -> anyhow::Result<RgbaImage> {
    let background = sprites.background("honeywake_sunset").expect("a background");
    let png = r.rank_card(&RankCard {
        display_name: "Captain Pinch",
        title: Some("Tide Angler"),
        username: "captain.pinch",
        avatar_png: &placeholder_avatar(42, RankCard::AVATAR_SIZE),
        level: 27,
        xp_current: 3_120,
        xp_needed: 5_095,
        rank: 4,
        next_role: Some(NextRole {
            name: "Reef Veteran",
            level: 30,
        }),
        backdrop: Some(Backdrop {
            art: Some(sprite_art(&background)),
            accent: "#ffc58b",
        }),
        lang: Lang::En,
        theme: Theme::reef().with_accent(Some(0xffc58b)),
    })?;
    Ok(decode(&png))
}

fn wallet(r: &Renderer, sprites: &Sprites) -> anyhow::Result<RgbaImage> {
    let background = sprites.background("tideglass_noon").expect("a background");
    let png = r.wallet_card(&WalletCard {
        display_name: "Captain Pinch",
        avatar_png: &placeholder_avatar(42, WalletCard::AVATAR_SIZE),
        shells: 1_284_500,
        pearls: 42,
        streak: 12,
        next_daily: "3h 20m",
        next_weekly: "ready",
        next_monthly: "12d 4h",
        pets: 5,
        fish_found: 31,
        fish_total: 72,
        backdrop: Some(Backdrop {
            art: Some(sprite_art(&background)),
            accent: "#72d7ed",
        }),
        lang: Lang::En,
        theme: Theme::reef().with_accent(Some(0x72d7ed)),
    })?;
    Ok(decode(&png))
}

fn welcome(r: &Renderer) -> anyhow::Result<RgbaImage> {
    let png = r.welcome_card(&WelcomeCard {
        display_name: "Marlowe",
        avatar_png: &placeholder_avatar(99, WelcomeCard::AVATAR_SIZE),
        server_name: "The Tide Pool",
        member_number: Some(12_345),
        title: "Welcome",
        theme: Theme::reef(),
    })?;
    Ok(decode(&png))
}

fn level_up(r: &Renderer) -> anyhow::Result<RgbaImage> {
    let png = r.level_up_card(&LevelUpCard {
        display_name: "Captain Pinch",
        avatar_png: &placeholder_avatar(42, LevelUpCard::AVATAR_SIZE),
        from: 26,
        to: 27,
        theme: Theme::reef(),
    })?;
    Ok(decode(&png))
}

fn leaderboard(r: &Renderer) -> anyhow::Result<RgbaImage> {
    let people = [
        ("Captain Pinch", 120_480, 31),
        ("Marlowe", 98_210, 28),
        ("Shelly", 77_005, 25),
        ("tide_pool_enjoyer", 51_990, 22),
        ("Nori", 40_120, 20),
        ("Bartholomew F.", 33_870, 18),
        ("kelp", 20_450, 15),
        ("Pearl", 12_300, 12),
        ("Bubbles", 9_870, 11),
        ("Anchor", 6_100, 9),
    ];
    let entries = people
        .iter()
        .enumerate()
        .map(|(i, (name, xp, level))| LeaderboardEntry {
            rank: i as i64 + 1,
            avatar_png: placeholder_avatar(i as u64 * 31 + 5, LeaderboardCard::AVATAR_SIZE),
            name: (*name).into(),
            value: format!("LV {level}"),
            detail: format!("{} XP", thousands(*xp)),
        })
        .collect();
    let png = r.leaderboard_card(&LeaderboardCard {
        title: "Server leaderboard".into(),
        entries,
        page: 0,
        last_week: Vec::new(),
        empty: "Nobody here yet. Chat a bit and check again!",
        theme: Theme::reef(),
    })?;
    Ok(decode(&png))
}

/// All 40 badges: most earned, a few part-way, the shop ones for sale.
fn badges(r: &Renderer, sprites: &Sprites) -> anyhow::Result<RgbaImage> {
    let content = Content::get();
    let arts: Vec<_> = content.badges.iter().map(|b| sprites.badge(&b.id)).collect();
    let cells: Vec<BadgeCell<'_>> = content
        .badges
        .iter()
        .zip(&arts)
        .enumerate()
        .map(|(i, (badge, art))| {
            let state = match i % 5 {
                0 | 1 | 3 => BadgeState::Earned {
                    new: i == 6,
                    worn: (i < 6).then_some(i as u8 + 1).filter(|n| *n <= 3),
                },
                2 => BadgeState::Locked {
                    have: (i as i64 * 3) % 9 + 1,
                    at: 10,
                },
                _ => BadgeState::ForSale {
                    price: 5_000 * (i as i64 % 5 + 1),
                },
            };
            BadgeCell {
                icon: BadgeIcon {
                    art: art.as_ref().map(|a| sprite_art(a)),
                    letter: badge.name.chars().next().unwrap_or('?'),
                    color: placeholder_badge_color(&badge.id),
                    rarity: badge.rarity.color(),
                },
                name: &badge.name,
                rarity: badge.rarity.label(),
                state,
            }
        })
        .collect();
    let legend = [
        ("Common", 0x8fa0d8),
        ("Uncommon", 0x5bbf6a),
        ("Rare", 0x3a9bdc),
        ("Epic", 0x9b6bc4),
        ("Legendary", 0xffd166),
    ];
    let png = r.badge_collection(&BadgeCollectionCard {
        cells: &cells,
        legend: &legend,
        theme: Theme::reef(),
    })?;
    Ok(decode(&png))
}

fn pet(r: &Renderer, sprites: &Sprites) -> anyhow::Result<RgbaImage> {
    let content = Content::get();
    let species = &content.pets[0];
    let sprite = sprites.pet(&species.id);
    let factor = (PetCard::SPRITE_BOX / sprite.width().max(1))
        .min(PetCard::SPRITE_BOX / sprite.height().max(1))
        .clamp(1, 16);
    let (png, w, h) = scaled_png(&sprite, factor)?;
    let card = r.pet_card(&PetCard {
        sprite_png: png,
        sprite_w: w,
        sprite_h: h,
        name: "Sir Clicks-a-lot".to_owned(),
        species: species.name.clone(),
        rarity: species.rarity.label().to_owned(),
        rarity_color: species.rarity.color(),
        level: 12,
        xp: 140,
        xp_needed: 400,
        needs: [82, 64, 90, 71],
        mood: "Happy".to_owned(),
        personality: species.personality.clone(),
        status: PetStatus::Awake,
        place: String::new(),
        back: String::new(),
        theme: Theme::reef(),
    })?;
    Ok(decode(&card))
}

// /Boards

fn chess(r: &Renderer) -> anyhow::Result<RgbaImage> {
    // The start position after e4 e5 Nf3, a black knight picked up.
    let back = [4u8, 2, 3, 5, 6, 3, 2, 4];
    let mut pieces = [0u8; 64];
    for file in 0..8 {
        pieces[file] = back[file];
        pieces[8 + file] = 1;
        pieces[48 + file] = 7;
        pieces[56 + file] = back[file] + 6;
    }
    pieces[12] = 0;
    pieces[28] = 1;
    pieces[52] = 0;
    pieces[36] = 7;
    pieces[6] = 0;
    pieces[21] = 2;
    let png = r.chess_card(&ChessCard {
        pieces: &pieces,
        names: ["Captain Pinch", "Marlowe"],
        turn: Some(1),
        winner: None,
        last: Some((6, 21)),
        selected: Some(57),
        targets: &[40, 42, 51],
        check: None,
        status: "Marlowe to move",
        pot: 200,
        theme: Theme::reef(),
    })?;
    Ok(decode(&png))
}

fn connect4(r: &Renderer) -> anyhow::Result<RgbaImage> {
    let rows = [".......", ".......", "...2...", "...1...", "..212..", ".12121."];
    let cells: Vec<u8> = rows
        .iter()
        .flat_map(|row| row.bytes().map(|b| if b == b'.' { 0 } else { b - b'0' }))
        .collect();
    let png = r.connect4_card(&Connect4Card {
        cells: &cells,
        names: ["Captain Pinch", "Marlowe"],
        turn: Some(1),
        winner: None,
        win_cells: &[],
        last: Some(3 * 7 + 3),
        status: "Column 1-7: your move",
        pot: 0,
        theme: Theme::reef(),
    })?;
    Ok(decode(&png))
}

fn wordle(r: &Renderer) -> anyhow::Result<RgbaImage> {
    let row = |word: &str, marks: [u8; 5]| WordleRow {
        letters: word.chars().collect::<Vec<_>>().try_into().expect("five letters"),
        marks,
    };
    let rows = [
        row("crane", [ABSENT, PRESENT, CORRECT, ABSENT, PRESENT]),
        row("pearl", [CORRECT, CORRECT, CORRECT, CORRECT, CORRECT]),
    ];
    let mut keys = [0u8; 26];
    for guess in &rows {
        for (c, m) in guess.letters.iter().zip(guess.marks) {
            let key = &mut keys[(*c as u8 - b'a') as usize];
            *key = (*key).max(m);
        }
    }
    let keyboard: Vec<Vec<(char, u8)>> = pinchora_game::lang::keyboard_rows(Lang::En)
        .iter()
        .map(|row| {
            row.chars()
                .map(|c| (c.to_ascii_uppercase(), keys[(c as u8 - b'a') as usize]))
                .collect()
        })
        .collect();
    let png = r.wordle_card(&WordleCard {
        title: "Daily wordle #63",
        rows: &rows,
        keyboard: &keyboard,
        footer: "Solved in 2/6",
        theme: Theme::reef(),
    })?;
    Ok(decode(&png))
}

// /Link preview and icons

const NAVY_TOP: [u8; 3] = [0x1d, 0x2b, 0x53];
const NAVY_BOTTOM: [u8; 3] = [0x0f, 0x16, 0x31];
const CORAL: Rgba<u8> = Rgba([0xff, 0x6b, 0x4a, 255]);
const INK: Rgba<u8> = Rgba([0xff, 0xf1, 0xe8, 255]);

/// The crab of the cards' footer, as `Crab.tsx` draws it.
const CRAB: [&str; 6] = [
    "#.......#",
    "##.....##",
    ".#.o.o.#.",
    ".#######.",
    "#########",
    ".#.#.#.#.",
];

/// The crab at `scale` pixels to the art pixel, with its top left corner at `(x, y)`.
fn draw_crab(canvas: &mut RgbaImage, x: u32, y: u32, scale: u32) {
    for (row, line) in CRAB.iter().enumerate() {
        for (col, c) in line.chars().enumerate() {
            let colour = match c {
                '#' => CORAL,
                'o' => INK,
                _ => continue,
            };
            for dy in 0..scale {
                for dx in 0..scale {
                    canvas.put_pixel(x + col as u32 * scale + dx, y + row as u32 * scale + dy, colour);
                }
            }
        }
    }
}

fn gradient(w: u32, h: u32) -> RgbaImage {
    RgbaImage::from_fn(w, h, |_, y| {
        let t = y as f32 / (h - 1) as f32;
        let mix = |i: usize| (NAVY_TOP[i] as f32 + (NAVY_BOTTOM[i] as f32 - NAVY_TOP[i] as f32) * t).round() as u8;
        Rgba([mix(0), mix(1), mix(2), 255])
    })
}

/// 1200x630 for link previews: the name, the line under it, a room and a profile card on the cards' own gradient.
fn og_image(r: &Renderer, room: &RgbaImage, profile: &RgbaImage) -> anyhow::Result<RgbaImage> {
    let mut canvas = gradient(1200, 630);
    for (x, y) in [(46, 40), (1120, 56), (1140, 560), (610, 24), (30, 380), (1060, 330)] {
        for dy in 0..4 {
            for dx in 0..4 {
                canvas.put_pixel(x + dx, y + dy, Rgba([255, 255, 255, 90]));
            }
        }
    }
    let room = imageops::resize(room, 520, 373, imageops::FilterType::Triangle);
    let profile = imageops::resize(profile, 380, 214, imageops::FilterType::Triangle);
    imageops::overlay(&mut canvas, &room, 650, 60);
    imageops::overlay(&mut canvas, &profile, 760, 368);
    let text = r##"<svg xmlns="http://www.w3.org/2000/svg" width="1200" height="630" viewBox="0 0 1200 630">
<text x="68" y="246" font-family="Silkscreen" font-weight="700" font-size="72" letter-spacing="2" fill="#1b1d2b">PINCHORA</text>
<text x="64" y="242" font-family="Silkscreen" font-weight="700" font-size="72" letter-spacing="2" fill="#ff6b4a">PINCHORA</text>
<text x="64" y="316" font-family="Noto Sans" font-weight="700" font-size="40" fill="#fff1e8">A cozy pixel world</text>
<text x="64" y="366" font-family="Noto Sans" font-weight="700" font-size="40" fill="#fff1e8">for your Discord server.</text>
<text x="64" y="448" font-family="Silkscreen" font-size="20" letter-spacing="2" fill="#8fa0d8">HOUSES  PETS  FISHING  GAMES  CARDS</text>
</svg>"##;
    imageops::overlay(&mut canvas, &decode(&r.svg_to_png(text)?), 0, 0);
    draw_crab(&mut canvas, 64, 520, 10);
    Ok(canvas)
}

/// The crab on a square of `size` pixels, `scale` pixels to the art pixel, on the page colour or see-through.
fn icon(size: u32, scale: u32, background: bool) -> RgbaImage {
    let mut canvas = if background {
        RgbaImage::from_pixel(size, size, Rgba([NAVY_TOP[0], NAVY_TOP[1], NAVY_TOP[2], 255]))
    } else {
        RgbaImage::new(size, size)
    };
    let (w, h) = (9 * scale, 6 * scale);
    draw_crab(&mut canvas, (size - w) / 2, (size - h) / 2, scale);
    canvas
}

/// A `.ico` holding one PNG per size.
fn ico_file(pictures: &[RgbaImage]) -> anyhow::Result<Vec<u8>> {
    let pngs: Vec<Vec<u8>> = pictures.iter().map(encode_png).collect::<anyhow::Result<_>>()?;
    let mut out = vec![0, 0, 1, 0];
    out.extend((pictures.len() as u16).to_le_bytes());
    let mut offset = 6 + 16 * pictures.len() as u32;
    for (picture, png) in pictures.iter().zip(&pngs) {
        out.extend([picture.width() as u8, picture.height() as u8, 0, 0, 1, 0, 32, 0]);
        out.extend((png.len() as u32).to_le_bytes());
        out.extend(offset.to_le_bytes());
        offset += png.len() as u32;
    }
    for png in pngs {
        out.extend(png);
    }
    Ok(out)
}

// /Everything

/// Every picture, rendered now.
fn assets() -> anyhow::Result<Vec<Asset>> {
    let r = Renderer::new();
    let sprites = Sprites::default();
    let room = room_image(&sprites, &room_view(), 1);
    let profile = profile(&r, &sprites)?;
    let og = og_image(&r, &room, &profile)?;
    let shots = "src/site/showcase";
    Ok(vec![
        asset(&format!("{shots}/profile.png"), profile),
        asset(&format!("{shots}/room.png"), room),
        asset(&format!("{shots}/rank.png"), rank(&r, &sprites)?),
        asset(&format!("{shots}/wallet.png"), wallet(&r, &sprites)?),
        asset(&format!("{shots}/welcome.png"), welcome(&r)?),
        asset(&format!("{shots}/levelup.png"), level_up(&r)?),
        asset(&format!("{shots}/leaderboard.png"), leaderboard(&r)?),
        asset(&format!("{shots}/badges.png"), badges(&r, &sprites)?),
        asset(&format!("{shots}/pet.png"), pet(&r, &sprites)?),
        asset(&format!("{shots}/chess.png"), chess(&r)?),
        asset(&format!("{shots}/connect4.png"), connect4(&r)?),
        asset(&format!("{shots}/wordle.png"), wordle(&r)?),
        asset("public/og.png", og),
        asset("public/favicon-16x16.png", icon(16, 1, false)),
        asset("public/favicon-32x32.png", icon(32, 3, false)),
        asset("public/apple-touch-icon.png", icon(180, 16, true)),
        asset("public/icon-192.png", icon(192, 16, true)),
        asset("public/icon-512.png", icon(512, 48, true)),
    ])
}

/// The smallest PNG of `image` the encoders here make: indexed when it has at most 256 colours, else filtered per
/// row and squeezed as hard as it goes (the renderer's own PNGs are written fast, not small, and the site is
/// mostly these pictures).
fn compact_png(image: &RgbaImage) -> Vec<u8> {
    let mut squeezed = Vec::new();
    PngEncoder::new_with_quality(&mut squeezed, CompressionType::Best, FilterType::Adaptive)
        .write_image(image.as_raw(), image.width(), image.height(), ExtendedColorType::Rgba8)
        .expect("encode a PNG");
    match encode_png(image) {
        Ok(indexed) if indexed.len() < squeezed.len() => indexed,
        _ => squeezed,
    }
}

fn write(path: &Path, bytes: &[u8]) {
    fs::create_dir_all(path.parent().expect("a folder")).expect("create the folder");
    fs::write(path, bytes).expect("write the file");
}

/// Whether two pictures show the same thing: the same size and, pixel by pixel, the same colour give or
/// take what the rasteriser's rounding differs by between machines, on all but a few of every thousand.
fn close(a: &RgbaImage, b: &RgbaImage) -> bool {
    if a.dimensions() != b.dimensions() {
        return false;
    }
    let differing = a
        .pixels()
        .zip(b.pixels())
        .filter(|(p, q)| {
            if p[3] == 0 && q[3] == 0 {
                return false;
            }
            p.0.iter().zip(q.0).any(|(x, y)| x.abs_diff(y) > 8)
        })
        .count();
    differing * 500 <= (a.width() * a.height()) as usize
}

#[test]
fn landing_assets_match() {
    let mut stale = Vec::new();
    for Asset { path, image } in assets().expect("render the pictures") {
        match image::open(web().join(&path)) {
            Ok(committed) if close(&committed.to_rgba8(), &image) => {}
            Ok(_) => stale.push(format!("{path} differs from what the renderer draws")),
            Err(_) => stale.push(format!("{path} is missing")),
        }
    }
    let ico = web().join("public/favicon.ico");
    let expected = ico_file(&[icon(16, 1, false), icon(32, 3, false)]).expect("ico");
    if fs::read(&ico).ok().as_deref() != Some(expected.as_slice()) {
        stale.push("public/favicon.ico is out of date".to_owned());
    }
    assert!(stale.is_empty(), "{}; {REGENERATE}", stale.join("; "));
}

#[test]
#[ignore = "writes web/src/site/showcase and web/public"]
fn write_landing_assets() {
    for Asset { path, image } in assets().expect("render the pictures") {
        write(&web().join(path), &compact_png(&image));
    }
    let ico = ico_file(&[icon(16, 1, false), icon(32, 3, false)]).expect("ico");
    write(&web().join("public/favicon.ico"), &ico);
}

#[test]
fn the_pictures_are_the_sizes_the_site_expects() {
    let sizes: Vec<(String, (u32, u32))> = assets()
        .expect("render the pictures")
        .into_iter()
        .map(|a| (a.path, a.image.dimensions()))
        .collect();
    let size = |name: &str| sizes.iter().find(|(p, _)| p.ends_with(name)).expect(name).1;
    assert_eq!(size("og.png"), (1200, 630), "link previews are 1200x630");
    assert_eq!(size("profile.png"), (960, 540));
    assert_eq!(size("icon-512.png"), (512, 512));
    assert_eq!(size("apple-touch-icon.png"), (180, 180));
}
