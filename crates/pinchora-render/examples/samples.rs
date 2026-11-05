//! Renders every card with sample data: `cargo run -p pinchora-render --example samples -- <out-dir>`.

use std::sync::Arc;

use image::{RgbaImage, imageops};
use pinchora_render::{
    Backdrop, BadgeCell, BadgeCollectionCard, BadgeIcon, BadgeState, CharacterCard, Frame, LeaderboardCard,
    LeaderboardEntry, LevelUpCard, NextRole, Opacity, ProfileCard, RankCard, Renderer, ShowcaseKind, ShowcaseSlot,
    SpriteArt, Theme, WalletCard, WelcomeCard,
    character::{Colors, placeholder_character, recolor_character},
    placeholder_avatar,
    room::{Cursor, RoomScene, SceneItem, SceneLayer, render_room},
    sprites::{
        SpriteStore, placeholder_badge_color, placeholder_item, placeholder_pet, placeholder_tile, recolor,
        variant_ramp,
    },
    thousands,
};

/// A sprite scaled up by a whole factor, as PNG, with its new size.
fn scaled_png(sprite: &RgbaImage, factor: u32) -> (Vec<u8>, u32, u32) {
    let (w, h) = (sprite.width() * factor, sprite.height() * factor);
    let big = imageops::resize(sprite, w, h, imageops::FilterType::Nearest);
    let mut png = std::io::Cursor::new(Vec::new());
    big.write_to(&mut png, image::ImageFormat::Png).expect("encode png");
    (png.into_inner(), w, h)
}

/// A scaled sprite as card art.
fn art(sprite: &(Vec<u8>, u32, u32)) -> SpriteArt<'_> {
    SpriteArt {
        png: &sprite.0,
        w: sprite.1,
        h: sprite.2,
    }
}

/// A showcase tile of `sprite` (PNG, width, height).
fn tile<'a>(kind: ShowcaseKind, sprite: &'a (Vec<u8>, u32, u32), name: &'a str) -> ShowcaseSlot<'a> {
    ShowcaseSlot {
        kind,
        art: art(sprite),
        name,
    }
}

/// A bright 240×135 beach for backgrounds until the atlas has real scenes: peach sky, turquoise sea, pale
/// sand. Light art like this is the hardest case for the text on a card.
fn sample_scene() -> RgbaImage {
    RgbaImage::from_fn(240, 135, |x, y| {
        let (dx, dy) = (x as i32 - 190, y as i32 - 34);
        let c = if dx * dx + dy * dy < 15 * 15 {
            [255, 244, 214]
        } else if y < 62 {
            [255, 214 + (y / 4) as u8, 180 + (y / 2) as u8]
        } else if y < 96 {
            if (y / 3 + x / 20) % 4 == 0 {
                [176, 236, 226]
            } else {
                [118, 214, 208]
            }
        } else if (x * 7 + y * 13) % 29 == 0 {
            [214, 190, 140]
        } else {
            [240, 222, 176]
        };
        image::Rgba([c[0], c[1], c[2], 255])
    })
}

fn main() -> anyhow::Result<()> {
    let out = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "target/render-samples".into());
    std::fs::create_dir_all(&out)?;
    let r = Renderer::new();

    let colors = Colors {
        skin: 2,
        hair: 4,
        top: 1,
        bottom: 2,
    };
    let look = SpriteStore::embedded()
        .get("a_ponycrest")
        .map_or_else(placeholder_character, |s| (*s).clone());
    let character = recolor_character(&look, colors);
    let (character_png, character_w, character_h) = scaled_png(&character, ProfileCard::CHARACTER_SCALE);
    let (card_png, card_w, card_h) = scaled_png(&character, CharacterCard::SCALE);

    let started = std::time::Instant::now();
    let profile = r.profile_card(&ProfileCard {
        display_name: "Captain Pinch",
        title: None,
        username: "captain.pinch",
        avatar_png: &placeholder_avatar(42, ProfileCard::AVATAR_SIZE),
        bio: "Collector of shiny shells, builder of tiny pixel houses. Ask me about my crab named Sir Clicks-a-lot!",
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
        backdrop: None,
        badges: &[],
        showcase: &[],
        partner: None,
        theme: Theme::reef(),
    })?;
    let profile_ms = started.elapsed().as_millis();
    std::fs::write(format!("{out}/profile.png"), &profile)?;

    // A background and six badges: the atlas art when it exists, placeholders until then.
    let store = SpriteStore::embedded();
    let scene = store
        .get("bg_shellwash_morning")
        .map_or_else(sample_scene, |s| (*s).clone());
    let (scene_png, scene_w, scene_h) = scaled_png(&scene, Backdrop::SCALE);
    let accent = "#8eddd1";
    let backdrop = |art: bool| Backdrop {
        art: art.then_some(SpriteArt {
            png: &scene_png,
            w: scene_w,
            h: scene_h,
        }),
        accent,
    };
    let worn = [
        ("little_beacon", 'L'),
        ("fin_finder", 'F'),
        ("pawshake", 'P'),
        ("week_of_waves", 'W'),
        ("tiny_tide", 'T'),
        ("pearl_wyrm", 'P'),
    ];
    let badge_art: Vec<Option<(Vec<u8>, u32, u32)>> = worn
        .iter()
        .map(|(id, _)| store.get(&format!("b_{id}")).map(|s| scaled_png(&s, BadgeIcon::SCALE)))
        .collect();
    let rarity_color = |id: &str| match id {
        "little_beacon" => 0x8fa0d8,
        "fin_finder" | "pawshake" => 0x5bbf6a,
        "week_of_waves" => 0x3a9bdc,
        "tiny_tide" => 0x8fa0d8,
        _ => 0xffd166,
    };
    let icons = |drawn: bool| -> Vec<BadgeIcon<'_>> {
        worn.iter()
            .zip(&badge_art)
            .map(|((id, letter), art)| BadgeIcon {
                art: art
                    .as_ref()
                    .filter(|_| drawn)
                    .map(|(png, w, h)| SpriteArt { png, w: *w, h: *h }),
                letter: *letter,
                color: placeholder_badge_color(id),
                rarity: rarity_color(id),
            })
            .collect()
    };
    let themed = Theme::reef().with_accent(Some(0x8eddd1));
    let profile_with = |name: &str, backdrop: Option<Backdrop<'_>>, badges: &[BadgeIcon<'_>], theme: Theme| {
        let png = r.profile_card(&ProfileCard {
            display_name: "Captain Pinch",
            title: Some("Tide Angler"),
            username: "captain.pinch",
            avatar_png: &placeholder_avatar(42, ProfileCard::AVATAR_SIZE),
            bio: "Collector of shiny shells, builder of tiny pixel houses. Ask me about my crab named Sir Clicks-a-lot!",
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
            backdrop,
            badges,
            showcase: &[],
            partner: None,
            theme,
        })?;
        std::fs::write(format!("{out}/{name}.png"), png)?;
        anyhow::Ok(())
    };
    profile_with("profile-badges", None, &icons(true), Theme::reef())?;
    profile_with("profile-background", Some(backdrop(true)), &icons(true), themed.clone())?;
    profile_with(
        "profile-placeholder",
        Some(backdrop(false)),
        &icons(false),
        themed.clone(),
    )?;

    // The collection card: the six sample badges earned, new, worn, part-way and for sale.
    let names = [
        "Little Beacon",
        "Fin Finder",
        "Pawshake",
        "Week Of Waves",
        "Tiny Tide",
        "Pearl Wyrm",
    ];
    let states = [
        BadgeState::Earned {
            new: false,
            worn: Some(1),
        },
        BadgeState::Locked { have: 7, at: 10 },
        BadgeState::Earned {
            new: true,
            worn: Some(2),
        },
        BadgeState::Locked { have: 3, at: 7 },
        BadgeState::Earned { new: false, worn: None },
        BadgeState::ForSale { price: 25_000 },
    ];
    let collection_icons = icons(true);
    let cells: Vec<BadgeCell<'_>> = collection_icons
        .into_iter()
        .zip(names)
        .zip(states)
        .map(|((icon, name), state)| BadgeCell {
            icon,
            name,
            rarity: "Common",
            state,
        })
        .collect();
    let legend = [
        ("Common", 0x8fa0d8),
        ("Uncommon", 0x5bbf6a),
        ("Rare", 0x3a9bdc),
        ("Epic", 0x9b6bc4),
        ("Legendary", 0xffd166),
    ];
    std::fs::write(
        format!("{out}/badge-collection.png"),
        r.badge_collection(&BadgeCollectionCard {
            cells: &cells,
            legend: &legend,
            theme: Theme::reef(),
        })?,
    )?;

    let rank = r.rank_card(&RankCard {
        display_name: "Captain Pinch",
        title: Some("Tide Angler"),
        username: "captain.pinch",
        avatar_png: &placeholder_avatar(7, RankCard::AVATAR_SIZE),
        level: 27,
        xp_current: 3_120,
        xp_needed: 5_095,
        rank: 4,
        next_role: Some(NextRole {
            name: "Reef Veteran",
            level: 30,
        }),
        backdrop: None,
        theme: Theme::reef().with_accent(Some(0x06d6a0)),
    })?;
    std::fs::write(format!("{out}/rank.png"), &rank)?;

    let rank_card = |backdrop: Option<Backdrop<'_>>, theme: Theme| {
        r.rank_card(&RankCard {
            display_name: "Captain Pinch",
            title: None,
            username: "captain.pinch",
            avatar_png: &placeholder_avatar(7, RankCard::AVATAR_SIZE),
            level: 27,
            xp_current: 3_120,
            xp_needed: 5_095,
            rank: 4,
            next_role: Some(NextRole {
                name: "Reef Veteran",
                level: 30,
            }),
            backdrop,
            theme,
        })
    };
    std::fs::write(
        format!("{out}/rank-background.png"),
        rank_card(Some(backdrop(true)), themed.clone())?,
    )?;

    let wallet_card = |backdrop: Option<Backdrop<'_>>, theme: Theme| {
        r.wallet_card(&WalletCard {
            display_name: "Captain Pinch",
            avatar_png: &placeholder_avatar(7, WalletCard::AVATAR_SIZE),
            shells: 1_284_500,
            pearls: 42,
            streak: 12,
            next_daily: "3h 20m",
            pets: 5,
            fish_found: 31,
            fish_total: 72,
            backdrop,
            theme,
        })
    };
    std::fs::write(format!("{out}/wallet.png"), wallet_card(None, Theme::reef())?)?;
    std::fs::write(
        format!("{out}/wallet-background.png"),
        wallet_card(Some(backdrop(true)), themed.clone())?,
    )?;

    let welcome = r.welcome_card(&WelcomeCard {
        display_name: "Captain Pinch",
        avatar_png: &placeholder_avatar(99, WelcomeCard::AVATAR_SIZE),
        server_name: "The Tide Pool",
        member_number: Some(12_345),
        title: "Welcome",
        theme: Theme::reef(),
    })?;
    std::fs::write(format!("{out}/welcome.png"), &welcome)?;

    let level_up = r.level_up_card(&LevelUpCard {
        display_name: "Captain Pinch",
        avatar_png: &placeholder_avatar(5, LevelUpCard::AVATAR_SIZE),
        from: 26,
        to: 27,
        theme: Theme::reef(),
    })?;
    std::fs::write(format!("{out}/levelup.png"), &level_up)?;

    let card = r.character_card(&CharacterCard {
        sprite_png: card_png,
        sprite_w: card_w,
        sprite_h: card_h,
        name: "Captain Pinch".into(),
        look: "Tidewalker".into(),
        colors,
        theme: Theme::reef(),
    })?;
    std::fs::write(format!("{out}/character.png"), &card)?;

    let people = [
        ("Captain Pinch", 120_480, 31),
        ("Marlow the Quiet", 98_210, 28),
        ("Shelly", 77_005, 25),
        ("tide_pool_enjoyer_2000", 51_990, 22),
        ("Nori", 40_120, 20),
        ("Bartholomew Fizzlebottom", 33_870, 18),
        ("kelp", 20_450, 15),
        ("Pearl", 12_300, 12),
        ("Bubbles", 9_870, 11),
        ("Anchor", 6_100, 9),
    ];
    let entries = |shells: bool| {
        people
            .iter()
            .enumerate()
            .map(|(i, (name, xp, level))| LeaderboardEntry {
                rank: i as i64 + 1,
                avatar_png: placeholder_avatar(i as u64 * 31 + 5, LeaderboardCard::AVATAR_SIZE),
                name: (*name).into(),
                value: if shells {
                    thousands(xp * 11)
                } else {
                    format!("LV {level}")
                },
                detail: if shells {
                    "SHELLS".into()
                } else {
                    format!("{} XP", thousands(*xp))
                },
            })
            .collect::<Vec<_>>()
    };
    let board = r.leaderboard_card(&LeaderboardCard {
        title: "Server leaderboard".into(),
        entries: entries(false),
        page: 0,
        theme: Theme::reef(),
    })?;
    std::fs::write(format!("{out}/leaderboard.png"), &board)?;
    let shells = r.leaderboard_card(&LeaderboardCard {
        title: "Richest shell collectors".into(),
        entries: entries(true).into_iter().skip(3).collect(),
        page: 4,
        theme: Theme::reef(),
    })?;
    std::fs::write(format!("{out}/leaderboard-shells.png"), &shells)?;

    let item = |id: &str, x, y, w, d, layer, variant: &str| {
        let base = placeholder_item(id, w, d, layer, None);
        let sprite = variant_ramp(variant).map_or(base.clone(), |r| recolor(&base, &r));
        SceneItem {
            sprite: Arc::new(sprite),
            x,
            y,
            w,
            d,
            layer,
            flipped: false,
            selected: false,
            lift: 0,
        }
    };
    let mut lamp = item("lamp", 5, 5, 1, 1, SceneLayer::Surface, "gold");
    lamp.lift = 22;
    let mut mirror = item("mirror", 1, 0, 1, 2, SceneLayer::Wall, "sky");
    mirror.flipped = true;
    let scene = RoomScene {
        width: 10,
        depth: 9,
        wallpaper: Arc::new(placeholder_tile("wp_stripes", true)),
        floor: Arc::new(placeholder_tile("floor_oak", false)),
        items: vec![
            item("rug_round", 3, 4, 4, 3, SceneLayer::Rug, "mint"),
            item("painting", 1, 0, 2, 1, SceneLayer::Wall, "gold"),
            item("window", 5, 0, 2, 2, SceneLayer::Wall, "sky"),
            mirror,
            item("sofa", 3, 2, 3, 1, SceneLayer::Floor, "coral"),
            item("table", 5, 5, 2, 1, SceneLayer::Floor, "cream"),
            lamp,
            item("bookshelf", 8, 0, 2, 1, SceneLayer::Floor, "navy"),
            item("plant", 0, 6, 1, 1, SceneLayer::Floor, "moss"),
            item("bed", 7, 5, 2, 3, SceneLayer::Floor, "plum"),
            SceneItem {
                sprite: Arc::new(character.clone()),
                x: 2,
                y: 6,
                w: 1,
                d: 1,
                layer: SceneLayer::Figure,
                flipped: false,
                selected: false,
                lift: 0,
            },
        ],
        cursor: Some(Cursor {
            x: 1,
            y: 3,
            w: 2,
            d: 1,
            wall: None,
            ok: true,
        }),
        grid: true,
        scale: 2,
    };
    render_room(&scene).save(format!("{out}/room.png"))?;

    // The showcase, partner and frames: a long name and a full bio on the busiest layout.
    let thumb_scene = RoomScene {
        width: scene.width,
        depth: scene.depth,
        wallpaper: scene.wallpaper.clone(),
        floor: scene.floor.clone(),
        items: scene.items.clone(),
        cursor: None,
        grid: false,
        scale: 1,
    };
    let (house_png, house_w, house_h) = ShowcaseSlot::fit_house(&render_room(&thumb_scene))?;
    let tile_art = |key: &str| -> anyhow::Result<(Vec<u8>, u32, u32)> {
        let sprite = store.get(key).map_or_else(|| placeholder_pet(key), |s| (*s).clone());
        ShowcaseSlot::fit_sprite(&sprite)
    };
    let fish = tile_art("c_amberwink_squidlet")?;
    let pet = tile_art("p_auroraclaw_crab")?;
    let bed = tile_art("f_blushbun_bed")?;
    let lamp = tile_art("f_blushcheek_lamp")?;
    let house = (house_png, house_w, house_h);
    let house = |name| tile(ShowcaseKind::House, &house, name);
    let all = [
        tile(ShowcaseKind::Fish, &fish, "Amberwink Squidlet"),
        tile(ShowcaseKind::Pet, &pet, "Sir Clicks-a-lot"),
        tile(ShowcaseKind::Furniture, &bed, "Blushbun Bed (Rose)"),
        house("Tide Pool Shack"),
    ];
    let long_bio = "Collector of shiny shells, builder of tiny pixel houses, feeder of crabs and chaser of rare fish. Ask me about Sir Clicks-a-lot or my 4 AM fishing trips!";
    let long_name = "Bartholomew Fizzlebottom Smythe!";
    let busy_bio = |name: &str,
                    bio: &str,
                    showcase: &[ShowcaseSlot<'_>],
                    partner: Option<&str>,
                    backdrop: Option<Backdrop<'_>>,
                    theme: Theme|
     -> anyhow::Result<Vec<u8>> {
        r.profile_card(&ProfileCard {
            display_name: name,
            title: Some("Lighthouse Keeper"),
            username: "bartholomew.fizzlebottom.smythe.99",
            avatar_png: &placeholder_avatar(42, ProfileCard::AVATAR_SIZE),
            bio,
            level: 41,
            xp_current: 3_120,
            xp_needed: 5_095,
            total_xp: 1_864_820,
            credits: 1_284_500,
            reputation: 1_330,
            streak: 112,
            global_rank: 1_337,
            server_rank: Some(4),
            character: Some(SpriteArt {
                png: &character_png,
                w: character_w,
                h: character_h,
            }),
            backdrop,
            badges: &icons(true),
            showcase,
            partner: partner.map(str::to_owned),
            theme,
        })
    };
    let busy_named = |name: &str,
                      showcase: &[ShowcaseSlot<'_>],
                      partner: Option<&str>,
                      backdrop: Option<Backdrop<'_>>,
                      theme: Theme|
     -> anyhow::Result<Vec<u8>> { busy_bio(name, long_bio, showcase, partner, backdrop, theme) };
    let busy = |showcase: &[ShowcaseSlot<'_>],
                partner: Option<&str>,
                backdrop: Option<Backdrop<'_>>,
                theme: Theme|
     -> anyhow::Result<Vec<u8>> { busy_named(long_name, showcase, partner, backdrop, theme) };
    let write = |name: &str, png: Vec<u8>| std::fs::write(format!("{out}/{name}.png"), png);
    write(
        "profile-showcase",
        busy(&all, Some("Marlow the Quiet"), None, Theme::reef())?,
    )?;
    write(
        "profile-showcase-background",
        busy(&all, Some("Marlow the Quiet"), Some(backdrop(true)), themed.clone())?,
    )?;
    write(
        "profile-showcase-light",
        busy(
            &all,
            None,
            Some(backdrop(true)),
            themed.clone().with_style(Opacity::Light, Frame::Plain),
        )?,
    )?;
    write(
        "profile-showcase-solid",
        busy(
            &all,
            None,
            Some(backdrop(true)),
            themed.clone().with_style(Opacity::Solid, Frame::Plain),
        )?,
    )?;
    let two = [
        tile(ShowcaseKind::Fish, &fish, "Amberwink Squidlet"),
        tile(ShowcaseKind::Furniture, &lamp, "Blushcheek Lamp"),
    ];
    write("profile-showcase-two", busy(&two, None, None, Theme::reef())?)?;
    write(
        "profile-showcase-house",
        busy(&all[3..], Some("Marlow"), None, Theme::reef())?,
    )?;
    write(
        "profile-partner",
        busy(&[], Some("Marlow the Quiet"), None, Theme::reef())?,
    )?;
    write("profile-long", busy(&[], None, None, Theme::reef())?)?;

    // Sprites of every size land on the same grid: the smallest, the biggest, and the in-betweens.
    let sturgeon = tile_art("c_frostlace_sturgeon")?;
    let cat = tile_art("p_drowsepatch_cat")?;
    let dish = tile_art("f_soapbud_dish")?;
    let small = [
        tile(ShowcaseKind::Fish, &sturgeon, "Frostlace Sturgeon"),
        tile(ShowcaseKind::Pet, &cat, "Drowsepatch"),
        tile(ShowcaseKind::Furniture, &dish, "Soapbud Dish"),
    ];
    write(
        "profile-showcase-small",
        busy(&small, Some("Marlow"), None, Theme::reef())?,
    )?;
    let tin = tile_art("c_rattlelip_tin")?;
    let squid = tile_art("p_starstitch_squid")?;
    let rig = tile_art("f_motionharbor_rig")?;
    let big = [
        tile(ShowcaseKind::Fish, &tin, "Rattlelip Tin"),
        tile(ShowcaseKind::Pet, &squid, "Starstitch Squid"),
        tile(ShowcaseKind::Furniture, &rig, "Motion Harbor Rig"),
        house("Tide Pool Shack"),
    ];
    write("profile-showcase-big", busy(&big, Some("Marlow"), None, Theme::reef())?)?;
    let sunfish = tile_art("c_moonbutton_sunfish")?;
    let tortoise = tile_art("p_wishroot_tortoise")?;
    let sofa = tile_art("f_baybloom_circle_sofa")?;
    let mixed = [
        tile(ShowcaseKind::Fish, &sunfish, "Moonbutton Sunfish"),
        tile(ShowcaseKind::Pet, &tortoise, "Wishroot"),
        tile(ShowcaseKind::Furniture, &sofa, "Baybloom Circle Sofa"),
    ];
    write("profile-showcase-mixed", busy(&mixed, None, None, Theme::reef())?)?;

    // Shouting and wide letters in the bio, beside the character.
    write(
        "profile-shout",
        busy_bio(
            "Captain Pinch",
            "WELCOME TO MY WORLD OF SHELLS, MOSAICS AND MIGHTY WAVES! ASK ME ABOUT WWWWW MMMMM AND THE WHOLE TIDE",
            &all,
            Some("Marlow"),
            None,
            Theme::reef(),
        )?,
    )?;
    write(
        "profile-shout-classic",
        busy_bio(
            "Captain Pinch",
            "WELCOME TO MY WORLD OF SHELLS, MOSAICS AND MIGHTY WAVES! ASK ME ABOUT WWWWW MMMMM AND THE WHOLE TIDE",
            &[],
            None,
            None,
            Theme::reef(),
        )?,
    )?;

    // Names the fonts do not cover, and names that are simply wide.
    write(
        "profile-unicode",
        busy_named("Zoë ✨ 李", &all, Some("Zoë ✨ 李"), None, Theme::reef())?,
    )?;
    write(
        "profile-unicode-classic",
        busy_named("Zoë ✨ 李", &[], Some("Zoë ✨ 李"), None, Theme::reef())?,
    )?;
    write(
        "profile-wide",
        busy_named(
            "WWWWWWWWWWWWWWWWWWWWWWWWWWWWWWWW",
            &all,
            Some("MMMMMMMMMMMMMMMMMMMMMM"),
            None,
            Theme::reef(),
        )?,
    )?;
    write(
        "profile-cyrillic",
        busy_named("Александр Пушкин", &[], Some("Анна Каренина"), None, Theme::reef())?,
    )?;
    write(
        "profile-emoji-family",
        busy_named("👨‍👩‍👧‍👦👨‍👩‍👧‍👦👨‍👩‍👧‍👦", &[], Some("👨‍👩‍👧‍👦👨‍👩‍👧‍👦👨‍👩‍👧‍👦👨‍👩‍👧‍👦"), None, Theme::reef())?,
    )?;

    // Backgrounds of every brightness, at every opacity, under the busiest layout.
    let flat = |rgb: [u8; 3]| RgbaImage::from_pixel(240, 135, image::Rgba([rgb[0], rgb[1], rgb[2], 255]));
    let stripes = RgbaImage::from_fn(240, 135, |x, y| {
        if (x / 6 + y / 6) % 2 == 0 {
            image::Rgba([250, 250, 250, 255])
        } else {
            image::Rgba([20, 20, 20, 255])
        }
    });
    for (name, scene) in [
        ("white", flat([255, 255, 255])),
        ("black", flat([0, 0, 0])),
        ("stripes", stripes),
    ] {
        let scaled = scaled_png(&scene, Backdrop::SCALE);
        for opacity in Opacity::ALL {
            write(
                &format!("profile-bg-{name}-{}", opacity.label().to_lowercase()),
                busy(
                    &all,
                    Some("Marlow the Quiet"),
                    Some(Backdrop {
                        art: Some(art(&scaled)),
                        accent: "#8eddd1",
                    }),
                    themed.clone().with_style(opacity, Frame::Plain),
                )?,
            )?;
        }
    }
    // With the real picture the same, and on the classic layout.
    for opacity in Opacity::ALL {
        let label = opacity.label().to_lowercase();
        write(
            &format!("profile-bg-beach-{label}"),
            busy(
                &all,
                Some("Marlow the Quiet"),
                Some(backdrop(true)),
                themed.clone().with_style(opacity, Frame::Plain),
            )?,
        )?;
        write(
            &format!("profile-bg-beach-{label}-classic"),
            busy(
                &[],
                Some("Marlow the Quiet"),
                Some(backdrop(true)),
                themed.clone().with_style(opacity, Frame::Plain),
            )?,
        )?;
        // And no picture at all.
        write(
            &format!("profile-nobg-{label}"),
            busy(&all, None, None, Theme::reef().with_style(opacity, Frame::Plain))?,
        )?;
    }
    let white = scaled_png(&flat([255, 255, 255]), Backdrop::SCALE);
    for (name, opacity) in [("light", Opacity::Light), ("solid", Opacity::Solid)] {
        std::fs::write(
            format!("{out}/rank-bg-white-{name}.png"),
            rank_card(
                Some(Backdrop {
                    art: Some(art(&white)),
                    accent,
                }),
                themed.clone().with_style(opacity, Frame::Plain),
            )?,
        )?;
        std::fs::write(
            format!("{out}/wallet-bg-white-{name}.png"),
            wallet_card(
                Some(Backdrop {
                    art: Some(art(&white)),
                    accent,
                }),
                themed.clone().with_style(opacity, Frame::Plain),
            )?,
        )?;
    }

    for frame in &Frame::ALL[1..] {
        let styled = |opacity| Theme::reef().with_style(opacity, *frame);
        write(
            &format!("profile-frame-{}", frame.id()),
            busy(&all, Some("Marlow the Quiet"), None, styled(Opacity::Normal))?,
        )?;
        write(
            &format!("profile-frame-{}-classic", frame.id()),
            busy(
                &[],
                Some("Marlow the Quiet"),
                Some(backdrop(true)),
                themed.clone().with_style(Opacity::Normal, *frame),
            )?,
        )?;
        write(
            &format!("rank-frame-{}", frame.id()),
            rank_card(Some(backdrop(true)), themed.clone().with_style(Opacity::Normal, *frame))?,
        )?;
        write(
            &format!("wallet-frame-{}", frame.id()),
            wallet_card(None, Theme::reef().with_style(Opacity::Normal, *frame))?,
        )?;
    }

    println!(
        "wrote {out}/{{profile,profile-badges,profile-background,profile-placeholder,badge-collection,rank,rank-background,wallet,wallet-background,welcome,levelup,character,leaderboard,leaderboard-shells,room}}.png (profile rendered in {profile_ms} ms)"
    );
    Ok(())
}
