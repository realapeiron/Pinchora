//! Renders every card with sample data: `cargo run -p pinchora-render --example samples -- <out-dir>`.

use std::sync::Arc;

use pinchora_render::{
    LevelUpCard, ProfileCard, RankCard, Renderer, Theme, WelcomeCard, placeholder_avatar,
    room::{Cursor, RoomScene, SceneItem, SceneLayer, render_room},
    sprites::{placeholder_item, placeholder_tile, recolor, variant_ramp},
};

fn main() -> anyhow::Result<()> {
    let out = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "target/render-samples".into());
    std::fs::create_dir_all(&out)?;
    let r = Renderer::new();

    let started = std::time::Instant::now();
    let profile = r.profile_card(&ProfileCard {
        display_name: "Captain Pinch",
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
        theme: Theme::reef(),
    })?;
    let profile_ms = started.elapsed().as_millis();
    std::fs::write(format!("{out}/profile.png"), &profile)?;

    let rank = r.rank_card(&RankCard {
        display_name: "Captain Pinch",
        username: "captain.pinch",
        avatar_png: &placeholder_avatar(7, RankCard::AVATAR_SIZE),
        level: 27,
        xp_current: 3_120,
        xp_needed: 5_095,
        rank: 4,
        theme: Theme::reef().with_accent(Some(0x06d6a0)),
    })?;
    std::fs::write(format!("{out}/rank.png"), &rank)?;

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

    println!("wrote {out}/{{profile,rank,welcome}}.png (profile rendered in {profile_ms} ms)");
    Ok(())
}
