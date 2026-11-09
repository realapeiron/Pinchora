//! House tiers and furniture placement rules.
//!
//! Rooms are seen isometrically: floor items sit on a `width`×`depth` tile grid, wall items
//! hang on the right wall (along x) or, when flipped, on the left wall (along y), where `x`
//! is the offset along that wall and `y` the level from the top. Flipping a floor item turns
//! it a quarter, so its footprint swaps to `[d, w]`.

use crate::{Content, Layer};

/// Wall items snap to this many rows on each wall.
pub const WALL_LEVELS: u32 = 3;

#[derive(Debug, Clone, Copy)]
pub struct Tier {
    pub tier: i16,
    pub name: &'static str,
    pub rooms: usize,
    pub width: u32,
    pub depth: u32,
    pub price: i64,
}

pub const TIERS: [Tier; 6] = [
    Tier {
        tier: 1,
        name: "Tide Pool Shack",
        rooms: 1,
        width: 8,
        depth: 8,
        price: 0,
    },
    Tier {
        tier: 2,
        name: "Beach Cottage",
        rooms: 2,
        width: 9,
        depth: 8,
        price: 5_000,
    },
    Tier {
        tier: 3,
        name: "Reef House",
        rooms: 3,
        width: 10,
        depth: 9,
        price: 20_000,
    },
    Tier {
        tier: 4,
        name: "Coral Villa",
        rooms: 4,
        width: 11,
        depth: 10,
        price: 60_000,
    },
    Tier {
        tier: 5,
        name: "Lighthouse Loft",
        rooms: 5,
        width: 12,
        depth: 11,
        price: 150_000,
    },
    Tier {
        tier: 6,
        name: "Sunken Palace",
        rooms: 6,
        width: 14,
        depth: 12,
        price: 400_000,
    },
];

pub fn tier(n: i16) -> &'static Tier {
    let i = (n.clamp(1, TIERS.len() as i16) - 1) as usize;
    &TIERS[i]
}

/// Room types handed out with each new room, in order.
pub const ROOM_ORDER: [&str; 6] = ["living", "bedroom", "kitchen", "bathroom", "garden", "studio"];

/// An item already in the room.
#[derive(Debug, Clone)]
pub struct Placed<'a> {
    pub item_id: &'a str,
    pub x: u32,
    pub y: u32,
    pub flipped: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rect {
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub d: u32,
}

impl Rect {
    pub fn overlaps(&self, o: &Rect) -> bool {
        self.x < o.x + o.w && o.x < self.x + self.w && self.y < o.y + o.d && o.y < self.y + self.d
    }

    pub fn contains(&self, o: &Rect) -> bool {
        o.x >= self.x && o.y >= self.y && o.x + o.w <= self.x + self.w && o.y + o.d <= self.y + self.d
    }
}

/// Footprint of an item at (x, y) as placed: floor layers swap `[w, d]` when flipped; wall
/// items keep `[w, h]` (flipping moves them to the other wall).
pub fn footprint(content: &Content, item_id: &str, x: u32, y: u32, flipped: bool) -> Option<(Layer, Rect)> {
    let item = content.item(item_id)?;
    let [w, d] = item.base.footprint;
    let (w, d) = if flipped && item.base.layer != Layer::Wall {
        (d, w)
    } else {
        (w, d)
    };
    Some((item.base.layer, Rect { x, y, w, d }))
}

/// Length in tiles of the wall a wall item hangs on.
pub fn wall_length(width: u32, depth: u32, flipped: bool) -> u32 {
    if flipped { depth } else { width }
}

/// Checks whether `item_id` fits at (x, y) in a `width`×`depth` room.
#[allow(clippy::too_many_arguments)]
pub fn can_place(
    content: &Content,
    width: u32,
    depth: u32,
    placed: &[Placed<'_>],
    item_id: &str,
    x: u32,
    y: u32,
    flipped: bool,
) -> Result<(), String> {
    let (layer, rect) = footprint(content, item_id, x, y, flipped).ok_or("unknown item")?;
    if layer == Layer::Wall {
        if rect.x + rect.w > wall_length(width, depth, flipped) || rect.y + rect.d > WALL_LEVELS {
            return Err("it does not fit on the wall there".into());
        }
        let taken = placed.iter().any(|p| {
            p.flipped == flipped
                && footprint(content, p.item_id, p.x, p.y, p.flipped)
                    .is_some_and(|(l, r)| l == Layer::Wall && r.overlaps(&rect))
        });
        return if taken {
            Err("something already hangs there".into())
        } else {
            Ok(())
        };
    }
    if rect.x + rect.w > width || rect.y + rect.d > depth {
        return Err("it does not fit inside the room there".into());
    }
    let others = placed.iter().filter_map(|p| {
        let (l, r) = footprint(content, p.item_id, p.x, p.y, p.flipped)?;
        Some((l, r, content.item(p.item_id)?.base.has_tag("surface")))
    });
    match layer {
        Layer::Rug => {
            if others
                .filter(|(l, _, _)| *l == Layer::Rug)
                .any(|(_, r, _)| r.overlaps(&rect))
            {
                return Err("another rug is already there".into());
            }
        }
        Layer::Floor => {
            if others
                .filter(|(l, _, _)| *l == Layer::Floor)
                .any(|(_, r, _)| r.overlaps(&rect))
            {
                return Err("something is already in that spot".into());
            }
        }
        Layer::Surface => {
            let others: Vec<_> = others.collect();
            let on_top_of_holder = others
                .iter()
                .any(|(l, r, holds)| *l == Layer::Floor && *holds && r.contains(&rect));
            let blocked_by_floor = others
                .iter()
                .any(|(l, r, holds)| *l == Layer::Floor && !holds && r.overlaps(&rect));
            if others.iter().any(|(l, r, _)| *l == Layer::Surface && r.overlaps(&rect)) {
                return Err("there is already something on that spot".into());
            }
            if blocked_by_floor && !on_top_of_holder {
                return Err("small items need a free tile or a table/shelf top".into());
            }
        }
        Layer::Wall => unreachable!("handled above"),
    }
    Ok(())
}

/// How high a surface item at (x, y) stands: the top of the table or shelf under it.
pub fn surface_lift(content: &Content, placed: &[Placed<'_>], x: u32, y: u32) -> u32 {
    let spot = Rect { x, y, w: 1, d: 1 };
    placed
        .iter()
        .filter_map(|p| {
            let base = &content.item(p.item_id)?.base;
            let (layer, r) = footprint(content, p.item_id, p.x, p.y, p.flipped)?;
            (layer == Layer::Floor && base.has_tag("surface") && r.contains(&spot))
                .then(|| base.surface_height.unwrap_or(24))
        })
        .max()
        .unwrap_or(0)
}

/// Whether flipping the floor item `item_id` (placed at (x, y), currently `flipped`) would leave
/// a surface item that stands on it outside its new footprint. Only floor items tagged
/// `surface` carry things; everything else flips freely.
pub fn flip_drops_surface_items(
    content: &Content,
    placed: &[Placed<'_>],
    item_id: &str,
    x: u32,
    y: u32,
    flipped: bool,
) -> bool {
    let Some(item) = content.item(item_id) else {
        return false;
    };
    if item.base.layer != Layer::Floor || !item.base.has_tag("surface") {
        return false;
    }
    let (Some((_, now)), Some((_, after))) = (
        footprint(content, item_id, x, y, flipped),
        footprint(content, item_id, x, y, !flipped),
    ) else {
        return false;
    };
    placed.iter().any(|p| {
        footprint(content, p.item_id, p.x, p.y, p.flipped)
            .is_some_and(|(layer, r)| layer == Layer::Surface && now.contains(&r) && !after.contains(&r))
    })
}

/// The pixels a piece covers on screen in the room picture, as `[left, top, right, bottom]` with the
/// room's own pixel grid (the picture before it is scaled up): where the renderer draws the art of
/// `item_id` at (x, y), `height` pixels tall, `lift` pixels above the floor. A rug lies flat, so it
/// covers the diamond of its footprint.
fn art_box(
    content: &Content,
    item_id: &str,
    x: u32,
    y: u32,
    flipped: bool,
    height: u32,
    lift: u32,
) -> Option<[i32; 4]> {
    let (layer, r) = footprint(content, item_id, x, y, flipped)?;
    let (x, y, w, d) = (x as i32, y as i32, r.w as i32, r.d as i32);
    if layer == Layer::Rug {
        return Some([32 * (x - y - d), 16 * (x + y), 32 * (x + w - y), 16 * (x + y + w + d)]);
    }
    let full = (w + d) * 32;
    let base = &content.item(item_id)?.base;
    let width = base
        .iso_width
        .map_or(if layer == Layer::Surface { 24 } else { full }, |w| w as i32)
        .clamp(8, full);
    let centre = 32 * (x - y) + 16 * (w - d);
    let bottom = 16 * (x + y) + 8 * (w + d) + width / 4 - lift as i32;
    Some([
        centre - width / 2,
        bottom - height as i32,
        centre - width / 2 + width,
        bottom,
    ])
}

/// Whether `item_id`, put at (x, y), would be mostly hidden in the room picture: a taller piece is
/// drawn in front of it, over more than half of what it covers. `height_of` says how many pixels tall
/// an item's art is. Pieces on the same tile or behind it never hide it; a rug, which lies under
/// whatever stands on it, is hidden by the pieces that stand on it or in front of it. Wall pieces are
/// never hidden.
pub fn is_hidden(
    content: &Content,
    placed: &[Placed<'_>],
    height_of: &dyn Fn(&str) -> u32,
    item_id: &str,
    x: u32,
    y: u32,
    flipped: bool,
) -> bool {
    let Some((layer, rect)) = footprint(content, item_id, x, y, flipped) else {
        return false;
    };
    if layer == Layer::Wall {
        return false;
    }
    let own = height_of(item_id);
    let lift = if layer == Layer::Surface {
        surface_lift(content, placed, x, y)
    } else {
        0
    };
    let Some(mine) = art_box(content, item_id, x, y, flipped, own, lift) else {
        return false;
    };
    let mut hiding = Vec::new();
    for p in placed {
        let Some((l, r)) = footprint(content, p.item_id, p.x, p.y, p.flipped) else {
            continue;
        };
        let height = height_of(p.item_id);
        if !matches!(l, Layer::Floor | Layer::Surface) || (layer != Layer::Rug && height <= own) {
            continue;
        }
        // Drawn after the held piece: it lies entirely behind the other one along x or y.
        let in_front = (rect.x + rect.w <= r.x && r.y + r.d > rect.y) || (rect.y + rect.d <= r.y && r.x + r.w > rect.x);
        if !(in_front || (layer == Layer::Rug && rect.overlaps(&r))) {
            continue;
        }
        let lift = if l == Layer::Surface {
            surface_lift(content, placed, p.x, p.y)
        } else {
            0
        };
        hiding.extend(art_box(content, p.item_id, p.x, p.y, p.flipped, height, lift));
    }
    if hiding.is_empty() {
        return false;
    }
    // Sample the piece's picture on a grid: a rug counts only the points inside its diamond.
    const GRID: i32 = 8;
    let [left, top, right, bottom] = mine;
    let (width, height) = ((right - left) as f32, (bottom - top) as f32);
    let (mut seen, mut covered) = (0, 0);
    for row in 0..GRID {
        for col in 0..GRID {
            let u = (col as f32 + 0.5) / GRID as f32;
            let v = (row as f32 + 0.5) / GRID as f32;
            if layer == Layer::Rug && (u - 0.5).abs() + (v - 0.5).abs() > 0.5 {
                continue;
            }
            let (px, py) = (left as f32 + u * width, top as f32 + v * height);
            seen += 1;
            if hiding
                .iter()
                .any(|b| px >= b[0] as f32 && px < b[2] as f32 && py >= b[1] as f32 && py < b[3] as f32)
            {
                covered += 1;
            }
        }
    }
    covered * 2 > seen
}

#[cfg(test)]
mod tests {
    use crate::house::*;

    fn content() -> Content {
        let f = r#"[
          {"id":"sofa","name":"Sofa","category":"seating","layer":"floor","footprint":[3,1],"rarity":"common","price":100,"currency":"shells","set":null,"tags":["seat"],"variants":["mint","navy"],"availability":"permanent","description":"","sprite_brief":""},
          {"id":"table","name":"Table","category":"tables","layer":"floor","footprint":[2,1],"rarity":"common","price":100,"currency":"shells","set":null,"tags":["surface"],"variants":[],"availability":"permanent","description":"","sprite_brief":""},
          {"id":"lamp","name":"Lamp","category":"lighting","layer":"surface","footprint":[1,1],"rarity":"common","price":50,"currency":"shells","set":null,"tags":["light"],"variants":[],"availability":"permanent","description":"","sprite_brief":""},
          {"id":"rug","name":"Rug","category":"rugs","layer":"rug","footprint":[3,2],"rarity":"common","price":50,"currency":"shells","set":null,"tags":[],"variants":[],"availability":"permanent","description":"","sprite_brief":""},
          {"id":"frame","name":"Frame","category":"wall_decor","layer":"wall","footprint":[2,1],"rarity":"common","price":50,"currency":"shells","set":null,"tags":[],"variants":[],"availability":"permanent","description":"","sprite_brief":""}
        ]"#;
        Content::from_json("[]", f, r#"{"wallpapers":[],"floors":[]}"#, "[]", "[]", "[]", "[]").unwrap()
    }

    #[test]
    fn items_and_variants() {
        let c = content();
        assert_eq!(c.items().count(), 6);
        assert_eq!(c.item("sofa__mint").unwrap().name(), "Sofa (Mint)");
        assert!(c.item("sofa").is_none(), "a base with variants needs one");
        assert!(c.item("sofa__gold").is_none());
        assert!(c.item("table").is_some());
    }

    #[test]
    fn soulbound_items_are_the_same_piece_with_a_mark() {
        let c = content();
        let bound = c.item("sofa__mint~bound").unwrap();
        assert!(bound.bound && bound.variant == Some("mint"));
        assert_eq!(bound.id(), "sofa__mint~bound");
        assert_eq!(bound.name(), "Sofa (Mint)");
        assert_eq!(c.item("sofa__mint").unwrap().id(), "sofa__mint");
        assert!(c.item("table~bound").unwrap().bound);
        assert!(c.item("sofa~bound").is_none() && c.item("sofa__gold~bound").is_none());
        assert!(c.items().all(|i| !i.bound), "the catalogue sells none of them");
        // The same footprint rules apply, so a soulbound piece can be placed like any other.
        let (w, d) = (6, 6);
        assert_eq!(
            can_place(&c, w, d, &[], "sofa__mint~bound", 0, 0, false),
            can_place(&c, w, d, &[], "sofa__mint", 0, 0, false)
        );
    }

    #[test]
    fn placement_rules() {
        let c = content();
        let fits = |placed: &[Placed], id: &str, x, y| can_place(&c, 10, 8, placed, id, x, y, false);
        assert!(fits(&[], "sofa__mint", 0, 2).is_ok());
        assert!(fits(&[], "sofa__mint", 8, 2).is_err(), "runs off the right edge");
        assert!(fits(&[], "frame", 0, 0).is_ok());
        assert!(fits(&[], "frame", 8, 2).is_ok());
        assert!(fits(&[], "frame", 9, 0).is_err(), "runs off the end of the wall");
        assert!(fits(&[], "frame", 0, 3).is_err(), "below the lowest wall level");
        assert!(
            can_place(&c, 10, 8, &[], "sofa__mint", 9, 0, true).is_ok(),
            "turned, the sofa runs along y"
        );
        assert!(can_place(&c, 10, 8, &[], "sofa__mint", 0, 6, true).is_err());
        let frame = [Placed {
            item_id: "frame",
            x: 0,
            y: 0,
            flipped: false,
        }];
        assert!(fits(&frame, "frame", 1, 0).is_err(), "same wall, overlapping");
        assert!(
            can_place(&c, 10, 8, &frame, "frame", 1, 0, true).is_ok(),
            "the other wall is free"
        );

        let room = [
            Placed {
                item_id: "table",
                x: 4,
                y: 4,
                flipped: false,
            },
            Placed {
                item_id: "sofa__navy",
                x: 0,
                y: 6,
                flipped: false,
            },
        ];
        assert!(fits(&room, "sofa__mint", 3, 4).is_err(), "overlaps the table");
        assert!(fits(&room, "rug", 3, 4).is_ok(), "rugs go under furniture");
        assert!(fits(&room, "lamp", 4, 4).is_ok(), "lamp on the table");
        assert!(fits(&room, "lamp", 1, 6).is_err(), "lamp inside the sofa");
        assert!(fits(&room, "lamp", 8, 3).is_ok(), "lamp on a free tile");

        let with_lamp = [
            Placed {
                item_id: "table",
                x: 4,
                y: 4,
                flipped: false,
            },
            Placed {
                item_id: "lamp",
                x: 4,
                y: 4,
                flipped: false,
            },
        ];
        assert!(fits(&with_lamp, "lamp", 4, 4).is_err(), "one thing per table tile");
        assert!(fits(&with_lamp, "lamp", 5, 4).is_ok());
        assert_eq!(surface_lift(&c, &with_lamp, 5, 4), 24, "default table height");
        assert_eq!(surface_lift(&c, &with_lamp, 7, 4), 0);
    }

    #[test]
    fn flipping_a_table_keeps_what_stands_on_it() {
        let c = content();
        let at = |item_id, x, y| Placed {
            item_id,
            x,
            y,
            flipped: false,
        };
        let drops = |placed: &[Placed], flipped| flip_drops_surface_items(&c, placed, "table", 4, 4, flipped);
        // A 2×1 table at (4, 4) covers (4,4) and (5,4); turned, it covers (4,4) and (4,5).
        assert!(!drops(&[], false), "nothing on it");
        assert!(!drops(&[at("lamp", 4, 4)], false), "the origin tile stays under it");
        assert!(drops(&[at("lamp", 5, 4)], false), "the far tile leaves the table");
        assert!(
            !drops(&[at("lamp", 6, 4)], false),
            "a lamp beside the table is not on it"
        );
        assert!(drops(&[at("lamp", 4, 4), at("lamp", 5, 4)], false));
        // Turned back, the lamp on its second tile (4,5) is dropped the same way.
        let turned = [at("lamp", 4, 5)];
        assert!(flip_drops_surface_items(&c, &turned, "table", 4, 4, true));
        // Only tables and shelves carry things.
        assert!(!flip_drops_surface_items(
            &c,
            &[at("lamp", 1, 6)],
            "sofa__mint",
            0,
            6,
            false
        ));
        assert!(!flip_drops_surface_items(&c, &[at("lamp", 1, 1)], "frame", 1, 1, false));
    }

    #[test]
    fn a_small_piece_behind_a_tall_one_is_hidden() {
        let c = content();
        let at = |item_id, x, y| Placed {
            item_id,
            x,
            y,
            flipped: false,
        };
        // The lamp's art is 20 px tall, the sofa's 80, the table's 10.
        let tall = |id: &str| match id {
            "lamp" => 20,
            "table" => 10,
            _ => 80,
        };
        let hidden = |placed: &[Placed], id, x, y| is_hidden(&c, placed, &tall, id, x, y, false);
        let sofa = [at("sofa__mint", 0, 1)];
        assert!(
            hidden(&sofa, "lamp", 0, 0),
            "the sofa stands right in front of that corner"
        );
        assert!(!hidden(&sofa, "lamp", 5, 0), "beside the sofa the lamp is seen");
        assert!(!hidden(&sofa, "lamp", 0, 3), "in front of the sofa nothing covers it");
        assert!(!hidden(&sofa, "lamp", 4, 4));
        // Only a taller piece hides: a low table in front covers nothing of the lamp's top.
        assert!(!hidden(&[at("table", 0, 1)], "lamp", 0, 0));
        // A lamp on the table is on it, not behind it.
        assert!(!hidden(&[at("table", 4, 4)], "lamp", 4, 4));
        // Wall pieces hang above everything that stands.
        assert!(!hidden(&sofa, "frame", 0, 0));
        // An unknown piece hides nothing and is not hidden.
        assert!(!hidden(&sofa, "nothing", 0, 0));
    }

    #[test]
    fn a_rug_under_furniture_is_hidden_but_an_open_one_is_not() {
        let c = content();
        let tall = |id: &str| if id == "rug" { 0 } else { 80 };
        let sofa_on_it = [Placed {
            item_id: "sofa__mint",
            x: 0,
            y: 0,
            flipped: false,
        }];
        assert!(is_hidden(&c, &sofa_on_it, &tall, "rug", 0, 0, false));
        let far_sofa = [Placed {
            item_id: "sofa__mint",
            x: 6,
            y: 6,
            flipped: false,
        }];
        assert!(
            !is_hidden(&c, &far_sofa, &tall, "rug", 0, 0, false),
            "a sofa far behind it"
        );
        assert!(!is_hidden(&c, &[], &tall, "rug", 0, 0, false), "an empty room");
    }

    #[test]
    fn tiers_grow() {
        assert_eq!(tier(1).rooms, 1);
        assert_eq!(tier(6).rooms, 6);
        assert_eq!(tier(99).tier, 6);
        assert!(
            TIERS
                .windows(2)
                .all(|w| w[1].price > w[0].price && w[1].rooms > w[0].rooms)
        );
    }
}
