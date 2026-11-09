//! Four-neighbour navigation. Manifest and ordered A* fixtures are shared with the browser.
use pinchora_commands::RoomData;
use pinchora_game::{Content, Layer, house::footprint};
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet},
    sync::LazyLock,
};

pub type Tile = [i32; 2];
#[derive(Clone, Debug)]
pub struct Grid {
    pub width: i32,
    pub depth: i32,
    pub blocked: HashSet<Tile>,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Target {
    #[serde(default)]
    pub id: String,
    pub kind: String,
    pub approaches: Vec<Tile>,
    pub anchor: Tile,
    pub height: i32,
    pub direction: String,
}
#[derive(Deserialize)]
struct Class {
    blocked: Option<Vec<Tile>>,
    interaction: Option<Target>,
}
#[derive(Deserialize)]
struct Manifest {
    version: u32,
    classes: HashMap<String, Class>,
}
static MANIFEST: LazyLock<Manifest> = LazyLock::new(|| {
    let manifest: Manifest = serde_json::from_str(include_str!("../../../../web/src/reef/navigation-manifest.json"))
        .expect("navigation manifest");
    assert_eq!(manifest.version, 1);
    manifest
});
impl Grid {
    pub fn free(&self, p: Tile) -> bool {
        p[0] >= 0 && p[1] >= 0 && p[0] < self.width && p[1] < self.depth && !self.blocked.contains(&p)
    }
    pub fn spawn(&self) -> Option<Tile> {
        (0..self.depth)
            .rev()
            .flat_map(|y| (0..self.width).map(move |x| [x, y]))
            .find(|p| self.free(*p))
    }
    pub fn path(&self, start: Tile, goal: Tile) -> Option<Vec<Tile>> {
        if !self.free(start) || !self.free(goal) {
            return None;
        }
        let distance = |p: Tile| (p[0] - goal[0]).abs() + (p[1] - goal[1]).abs();
        let mut open = vec![(start, 0, distance(start), 0)];
        let mut serial = 1;
        let mut costs = HashMap::from([(start, 0)]);
        let mut parents = HashMap::new();
        while !open.is_empty() {
            open.sort_by_key(|(_, g, h, serial)| (g + h, *h, *serial));
            let (p, g, _, _) = open.remove(0);
            if costs.get(&p) != Some(&g) {
                continue;
            }
            if p == goal {
                let mut path = vec![p];
                let mut cursor = p;
                while cursor != start {
                    cursor = parents[&cursor];
                    path.push(cursor);
                }
                path.reverse();
                return Some(path);
            }
            for [dx, dy] in [[1, 0], [0, 1], [-1, 0], [0, -1]] {
                let next = [p[0] + dx, p[1] + dy];
                if !self.free(next) || costs.get(&next).is_some_and(|cost| g + 1 >= *cost) {
                    continue;
                }
                costs.insert(next, g + 1);
                parents.insert(next, p);
                open.push((next, g + 1, distance(next), serial));
                serial += 1;
            }
        }
        None
    }
}
pub fn navigation(room: &RoomData) -> (Grid, Vec<Target>) {
    let mut grid = Grid {
        width: room.width as i32,
        depth: room.depth as i32,
        blocked: HashSet::new(),
    };
    let mut targets = vec![];
    let content = Content::get();
    for p in &room.placements {
        let Some(item) = content.item(&p.item_id) else {
            continue;
        };
        let Some((layer, rect)) = footprint(content, &p.item_id, p.x as u32, p.y as u32, p.flipped) else {
            continue;
        };
        let class = MANIFEST.classes.get(&item.base.id);
        let transform = |[x, y]: Tile| {
            if p.flipped {
                [i32::from(p.x) + y, i32::from(p.y) + x]
            } else {
                [i32::from(p.x) + x, i32::from(p.y) + y]
            }
        };
        let walkable = matches!(layer, Layer::Rug | Layer::Wall);
        if let Some(cells) = class.and_then(|c| c.blocked.as_ref()) {
            grid.blocked.extend(cells.iter().map(|p| transform(*p)));
        } else if class.is_some() || !walkable {
            for y in rect.y..rect.y + rect.d {
                for x in rect.x..rect.x + rect.w {
                    if x < room.width && y < room.depth {
                        grid.blocked.insert([x as i32, y as i32]);
                    }
                }
            }
        }
        let interaction = class.and_then(|c| c.interaction.clone()).or_else(|| {
            (class.is_none() && !walkable && item.base.has_tag("game")).then(|| Target {
                id: String::new(),
                kind: "usable".into(),
                approaches: vec![
                    [0, item.base.footprint[1] as i32],
                    [item.base.footprint[0] as i32, 0],
                    [-1, 0],
                    [0, -1],
                ],
                anchor: [0, 0],
                height: 0,
                direction: "NW".into(),
            })
        });
        if let Some(mut target) = interaction {
            target.id = p.id.to_string();
            target.anchor = transform(target.anchor);
            target.approaches = target.approaches.into_iter().map(transform).collect();
            if p.flipped {
                target.direction = match target.direction.as_str() {
                    "SW" => "SE",
                    "SE" => "SW",
                    "NW" => "NE",
                    _ => "NW",
                }
                .into();
            }
            targets.push(target);
        }
    }
    (grid, targets)
}
#[cfg(test)]
mod tests {
    use crate::reef::navigation::*;
    #[test]
    fn shared_astar_fixtures() {
        #[derive(Deserialize)]
        struct Case {
            name: String,
            size: Tile,
            blocked: Vec<Tile>,
            start: Tile,
            goal: Tile,
            path: Option<Vec<Tile>>,
        }
        #[derive(Deserialize)]
        struct Fixtures {
            cases: Vec<Case>,
        }
        let fixtures: Fixtures =
            serde_json::from_str(include_str!("../../../../web/src/reef/fixtures/navigation.json")).unwrap();
        for c in fixtures.cases {
            let grid = Grid {
                width: c.size[0],
                depth: c.size[1],
                blocked: c.blocked.into_iter().collect(),
            };
            assert_eq!(grid.path(c.start, c.goal), c.path, "{}", c.name);
        }
    }
    #[test]
    fn shared_layout_fixtures() {
        use pinchora_db::{PlacementRow, RoomRow};
        #[derive(Deserialize)]
        struct Placement {
            id: String,
            item_id: String,
            x: i16,
            y: i16,
            flipped: bool,
        }
        #[derive(Deserialize)]
        struct Case {
            name: String,
            placements: Vec<Placement>,
            blocked: Vec<Tile>,
            targets: serde_json::Value,
        }
        #[derive(Deserialize)]
        struct Fixtures {
            cases: Vec<Case>,
        }
        let fixtures: Fixtures =
            serde_json::from_str(include_str!("../../../../web/src/reef/fixtures/navigation-layout.json")).unwrap();
        for case in fixtures.cases {
            let data = RoomData {
                width: 6,
                depth: 6,
                room: RoomRow {
                    idx: 0,
                    room_type: "living".into(),
                    wallpaper: String::new(),
                    floor: String::new(),
                },
                placements: case
                    .placements
                    .into_iter()
                    .map(|p| PlacementRow {
                        id: p.id.parse().unwrap(),
                        room_idx: 0,
                        item_id: p.item_id,
                        x: p.x,
                        y: p.y,
                        flipped: p.flipped,
                    })
                    .collect(),
                pets: vec![],
                characters: vec![],
                seed: 0,
            };
            let (grid, targets) = navigation(&data);
            assert_eq!(grid.blocked, case.blocked.into_iter().collect(), "{}", case.name);
            assert_eq!(serde_json::to_value(targets).unwrap(), case.targets, "{}", case.name);
        }
    }
    #[test]
    fn spawn_and_full_room() {
        let mut grid = Grid {
            width: 2,
            depth: 1,
            blocked: HashSet::new(),
        };
        assert_eq!(grid.spawn(), Some([0, 0]));
        grid.blocked.insert([0, 0]);
        assert_eq!(grid.spawn(), Some([1, 0]));
        grid.blocked.insert([1, 0]);
        assert_eq!(grid.spawn(), None);
    }
}
