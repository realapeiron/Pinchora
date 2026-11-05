//! House tiers and furniture placement rules.

use crate::{Content, Layer};

pub const WALL_ROWS: u32 = 2;

#[derive(Debug, Clone, Copy)]
pub struct Tier {
    pub tier: i16,
    pub name: &'static str,
    pub rooms: usize,
    pub width: u32,
    pub height: u32,
    pub price: i64,
}

pub const TIERS: [Tier; 6] = [
    Tier {
        tier: 1,
        name: "Tide Pool Shack",
        rooms: 1,
        width: 10,
        height: 8,
        price: 0,
    },
    Tier {
        tier: 2,
        name: "Beach Cottage",
        rooms: 2,
        width: 11,
        height: 8,
        price: 5_000,
    },
    Tier {
        tier: 3,
        name: "Reef House",
        rooms: 3,
        width: 12,
        height: 9,
        price: 20_000,
    },
    Tier {
        tier: 4,
        name: "Coral Villa",
        rooms: 4,
        width: 12,
        height: 9,
        price: 60_000,
    },
    Tier {
        tier: 5,
        name: "Lighthouse Loft",
        rooms: 5,
        width: 13,
        height: 10,
        price: 150_000,
    },
    Tier {
        tier: 6,
        name: "Sunken Palace",
        rooms: 6,
        width: 14,
        height: 10,
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
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rect {
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub d: u32,
}

impl Rect {
    fn overlaps(&self, o: &Rect) -> bool {
        self.x < o.x + o.w && o.x < self.x + self.w && self.y < o.y + o.d && o.y < self.y + self.d
    }

    fn contains(&self, o: &Rect) -> bool {
        o.x >= self.x && o.y >= self.y && o.x + o.w <= self.x + self.w && o.y + o.d <= self.y + self.d
    }
}

/// Footprint rectangle of an item at (x, y).
pub fn footprint(content: &Content, item_id: &str, x: u32, y: u32) -> Option<(Layer, Rect)> {
    let item = content.item(item_id)?;
    let [w, d] = item.base.footprint;
    Some((item.base.layer, Rect { x, y, w, d }))
}

/// Checks whether `item_id` fits at (x, y) in a `width`×`height` room.
pub fn can_place(
    content: &Content,
    width: u32,
    height: u32,
    placed: &[Placed<'_>],
    item_id: &str,
    x: u32,
    y: u32,
) -> Result<(), String> {
    let (layer, rect) = footprint(content, item_id, x, y).ok_or("unknown item")?;
    if rect.x + rect.w > width || rect.y + rect.d > height {
        return Err("it does not fit inside the room there".into());
    }
    match layer {
        Layer::Wall if rect.y + rect.d > WALL_ROWS => return Err("wall items go on the wall".into()),
        Layer::Floor | Layer::Rug | Layer::Surface if rect.y < WALL_ROWS => {
            return Err("that goes on the floor, not the wall".into());
        }
        _ => {}
    }
    let others = placed.iter().filter_map(|p| {
        let (l, r) = footprint(content, p.item_id, p.x, p.y)?;
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
        Layer::Wall | Layer::Floor => {
            if others
                .filter(|(l, _, _)| *l == layer)
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
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn content() -> Content {
        let f = r#"[
          {"id":"sofa","name":"Sofa","category":"seating","layer":"floor","footprint":[3,1],"rarity":"common","price":100,"currency":"shells","set":null,"tags":["seat"],"variants":["mint","navy"],"availability":"permanent","description":"","sprite_brief":""},
          {"id":"table","name":"Table","category":"tables","layer":"floor","footprint":[2,1],"rarity":"common","price":100,"currency":"shells","set":null,"tags":["surface"],"variants":[],"availability":"permanent","description":"","sprite_brief":""},
          {"id":"lamp","name":"Lamp","category":"lighting","layer":"surface","footprint":[1,1],"rarity":"common","price":50,"currency":"shells","set":null,"tags":["light"],"variants":[],"availability":"permanent","description":"","sprite_brief":""},
          {"id":"rug","name":"Rug","category":"rugs","layer":"rug","footprint":[3,2],"rarity":"common","price":50,"currency":"shells","set":null,"tags":[],"variants":[],"availability":"permanent","description":"","sprite_brief":""},
          {"id":"frame","name":"Frame","category":"wall_decor","layer":"wall","footprint":[2,1],"rarity":"common","price":50,"currency":"shells","set":null,"tags":[],"variants":[],"availability":"permanent","description":"","sprite_brief":""}
        ]"#;
        Content::from_json("[]", f, r#"{"wallpapers":[],"floors":[]}"#, "[]", "[]", "[]").unwrap()
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
    fn placement_rules() {
        let c = content();
        let fits = |placed: &[Placed], id: &str, x, y| can_place(&c, 10, 8, placed, id, x, y);
        assert!(fits(&[], "sofa__mint", 0, 2).is_ok());
        assert!(fits(&[], "sofa__mint", 8, 2).is_err(), "runs off the right edge");
        assert!(fits(&[], "sofa__mint", 0, 1).is_err(), "floor item on the wall");
        assert!(fits(&[], "frame", 0, 0).is_ok());
        assert!(fits(&[], "frame", 0, 2).is_err(), "wall item on the floor");

        let room = [
            Placed {
                item_id: "table",
                x: 4,
                y: 4,
            },
            Placed {
                item_id: "sofa__navy",
                x: 0,
                y: 6,
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
            },
            Placed {
                item_id: "lamp",
                x: 4,
                y: 4,
            },
        ];
        assert!(fits(&with_lamp, "lamp", 4, 4).is_err(), "one thing per table tile");
        assert!(fits(&with_lamp, "lamp", 5, 4).is_ok());
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
