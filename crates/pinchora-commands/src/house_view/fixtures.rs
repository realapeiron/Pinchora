//! Parity fixtures for the browser's room renderer (`web/src/iso`).
//!
//! The scenes are written as JSON in the shape `GET /api/house/rooms/{idx}` answers, next to the PNG the
//! Rust renderer draws for them at native resolution. The TypeScript tests draw the same scenes and compare
//! pixel for pixel. `house_fixtures_match` fails as soon as the renderer, the art or the catalogue moves
//! away from what is committed, so a Rust change that forgets the browser cannot slip through.
//!
//! Regenerate with `cargo test -p pinchora-commands write_house_fixtures -- --ignored`.

use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
};

use image::{Rgba, RgbaImage};
use pinchora_db::{PlacementRow, RoomRow};
use pinchora_game::{
    Content, Layer,
    character::Character,
    house::{self, Placed},
};
use pinchora_render::{
    character::Paint,
    encode_png,
    room::{Cursor, WallSide},
    sprites::{placeholder_item, placeholder_pet, placeholder_tile, recolor, sleeping_pet, variant_ramp},
};
use serde_json::{Value, json};

use super::{RoomData, catalog};
use crate::game::scene::{RoomPet, RoomView, Sprites, room_image, scene_layer};

fn dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../web/src/iso/fixtures")
}

const REGENERATE: &str =
    "run `cargo test -p pinchora-commands write_house_fixtures -- --ignored` and commit web/src/iso/fixtures";

/// A scene as the tests define it, before it becomes JSON.
struct Def {
    name: &'static str,
    width: u32,
    depth: u32,
    wallpaper: usize,
    floor: usize,
    placements: Vec<(String, i16, i16, bool)>,
    pets: Vec<RoomPet>,
    characters: Vec<Character>,
    seed: u64,
    edit: bool,
    cursor: Option<Cursor>,
    selected: Option<i64>,
}

fn character(content: &Content, skin: u8, top: u8, hair: u8) -> Character {
    let mut c = Character::starter(content);
    c.colors.skin = Paint::Swatch(skin);
    c.colors.top = Paint::Swatch(top);
    c.colors.hair = Paint::Swatch(hair);
    c
}

/// A small deterministic generator, so the crowded rooms are the same on every machine.
struct Lcg(u64);

impl Lcg {
    fn next(&mut self, bound: usize) -> usize {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        (self.0 >> 33) as usize % bound.max(1)
    }
}

/// Fills a room with `count` things the placement rules accept: floor pieces first, then small things on
/// the tables, then rugs and wall hangings.
fn crowd(content: &Content, width: u32, depth: u32, seed: u64, count: usize) -> Vec<(String, i16, i16, bool)> {
    let mut rng = Lcg(seed);
    let mut pool: Vec<String> = content
        .items()
        .filter(|i| i.base.footprint[0] * i.base.footprint[1] <= 4)
        .map(|i| i.id())
        .collect();
    pool.sort();
    let mut placed: Vec<(String, i16, i16, bool)> = Vec::new();
    let fits = |placed: &[(String, i16, i16, bool)], id: &str, x: u32, y: u32, flipped: bool| {
        let others: Vec<Placed<'_>> = placed
            .iter()
            .map(|(item_id, px, py, f)| Placed {
                item_id,
                x: *px as u32,
                y: *py as u32,
                flipped: *f,
            })
            .collect();
        house::can_place(content, width, depth, &others, id, x, y, flipped).is_ok()
    };
    for pass in 0..3 {
        for _ in 0..4000 {
            if placed.len() >= count {
                break;
            }
            let id = pool[rng.next(pool.len())].clone();
            let layer = content.item(&id).map(|i| i.base.layer);
            let wanted = match pass {
                0 => matches!(layer, Some(Layer::Floor)),
                1 => matches!(layer, Some(Layer::Surface)),
                _ => matches!(layer, Some(Layer::Rug | Layer::Wall)),
            };
            if !wanted {
                continue;
            }
            let (x, y) = (rng.next(width as usize) as u32, rng.next(depth as usize) as u32);
            let flipped = rng.next(2) == 1;
            if fits(&placed, &id, x, y, flipped) {
                placed.push((id, x as i16, y as i16, flipped));
            }
        }
        // Between the floor pass and the small things: stand one on each table or shelf.
        if pass == 0 {
            let tops: Vec<(i16, i16)> = placed
                .iter()
                .filter(|(id, ..)| {
                    content
                        .item(id)
                        .is_some_and(|i| i.base.layer == Layer::Floor && i.base.has_tag("surface"))
                })
                .map(|(_, x, y, _)| (*x, *y))
                .collect();
            for (x, y) in tops {
                for _ in 0..40 {
                    let id = pool[rng.next(pool.len())].clone();
                    if content.item(&id).is_some_and(|i| i.base.layer == Layer::Surface)
                        && fits(&placed, &id, x as u32, y as u32, false)
                    {
                        placed.push((id, x, y, false));
                        break;
                    }
                }
            }
        }
    }
    placed
}

fn defs() -> Vec<Def> {
    let content = Content::get();
    let pet = |i: usize| content.pets[i % content.pets.len()].id.clone();
    // The room the bot's own preview draws, to see the art in place.
    let showcase: Vec<(String, i16, i16, bool)> = [
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
    .into_iter()
    .map(|(id, x, y, f)| (id.to_owned(), x, y, f))
    .collect();
    vec![
        Def {
            name: "empty_8x8",
            width: 8,
            depth: 8,
            wallpaper: 0,
            floor: 0,
            placements: Vec::new(),
            pets: Vec::new(),
            characters: Vec::new(),
            seed: 0,
            edit: false,
            cursor: None,
            selected: None,
        },
        Def {
            name: "furnished_10x9",
            width: 10,
            depth: 9,
            wallpaper: 0,
            floor: 0,
            placements: showcase.clone(),
            pets: vec![RoomPet::awake(pet(0))],
            characters: vec![character(content, 2, 1, 1)],
            seed: 3,
            edit: false,
            cursor: None,
            selected: None,
        },
        Def {
            name: "edit_grid_cursor",
            width: 9,
            depth: 8,
            wallpaper: 3,
            floor: 5,
            placements: showcase[..16].to_vec(),
            pets: Vec::new(),
            characters: Vec::new(),
            seed: 1,
            edit: true,
            cursor: Some(Cursor {
                x: 3,
                y: 1,
                w: 2,
                d: 1,
                wall: Some(WallSide::Right),
                ok: false,
            }),
            selected: Some(12),
        },
        Def {
            name: "edit_floor_cursor",
            width: 8,
            depth: 8,
            wallpaper: 7,
            floor: 2,
            placements: showcase[10..16].to_vec(),
            pets: Vec::new(),
            characters: Vec::new(),
            seed: 1,
            edit: true,
            cursor: Some(Cursor {
                x: 5,
                y: 2,
                w: 2,
                d: 2,
                wall: None,
                ok: true,
            }),
            selected: Some(3),
        },
        Def {
            name: "sleepers_and_walkers",
            width: 8,
            depth: 8,
            wallpaper: 11,
            floor: 9,
            placements: showcase[10..15].to_vec(),
            pets: vec![
                RoomPet::asleep(pet(0)),
                RoomPet::asleep(pet(5)),
                RoomPet::asleep(pet(11)),
                RoomPet::asleep(pet(17)),
                RoomPet::awake(pet(23)),
                RoomPet::awake(pet(31)),
                RoomPet::awake(pet(37)),
            ],
            characters: vec![character(content, 5, 4, 0), character(content, 0, 2, 3)],
            seed: 7,
            edit: false,
            cursor: None,
            selected: None,
        },
        Def {
            name: "crowded_9x8",
            width: 9,
            depth: 8,
            wallpaper: 20,
            floor: 20,
            placements: crowd(content, 9, 8, 0x5eed_0001, 28),
            pets: vec![RoomPet::awake(pet(2)), RoomPet::asleep(pet(9))],
            characters: vec![character(content, 3, 5, 2)],
            seed: 11,
            edit: false,
            cursor: None,
            selected: None,
        },
        Def {
            name: "crowded_14x12",
            width: 14,
            depth: 12,
            wallpaper: 33,
            floor: 40,
            placements: crowd(content, 14, 12, 0x5eed_0002, 60),
            pets: vec![
                RoomPet::awake(pet(3)),
                RoomPet::awake(pet(14)),
                RoomPet::asleep(pet(21)),
            ],
            characters: vec![character(content, 1, 3, 4), character(content, 4, 0, 1)],
            seed: 12,
            edit: false,
            cursor: None,
            selected: None,
        },
    ]
}

fn room_data(def: &Def) -> RoomData {
    let content = Content::get();
    RoomData {
        width: def.width,
        depth: def.depth,
        room: RoomRow {
            idx: 0,
            room_type: "living".to_owned(),
            wallpaper: content.surfaces.wallpapers[def.wallpaper].id.clone(),
            floor: content.surfaces.floors[def.floor].id.clone(),
        },
        placements: def
            .placements
            .iter()
            .enumerate()
            .map(|(i, (item_id, x, y, flipped))| PlacementRow {
                id: i as i64 + 1,
                room_idx: 0,
                item_id: item_id.clone(),
                x: *x,
                y: *y,
                flipped: *flipped,
            })
            .collect(),
        pets: def.pets.clone(),
        characters: def.characters.clone(),
        seed: def.seed,
    }
}

fn wall_key(wall: Option<WallSide>) -> Value {
    match wall {
        Some(WallSide::Right) => json!("right"),
        Some(WallSide::Left) => json!("left"),
        None => Value::Null,
    }
}

/// The scene file: the room as the API gives it, and how to look at it.
fn scene_json(def: &Def) -> Value {
    json!({
        "name": def.name,
        "room": room_data(def).json(),
        "options": {
            "edit": def.edit,
            "cursor": def.cursor.map(|c| json!({
                "x": c.x, "y": c.y, "w": c.w, "d": c.d, "wall": wall_key(c.wall), "ok": c.ok,
            })),
            "selected": def.selected.map(|id| id.to_string()),
        },
    })
}

/// Rebuilds the view the renderer draws from a scene file, the way the browser reads it.
fn view_from(scene: &Value) -> RoomView {
    let content = Content::get();
    let room = &scene["room"];
    let options = &scene["options"];
    let text = |v: &Value| v.as_str().expect("a string").to_owned();
    let cursor = options["cursor"].as_object().map(|c| Cursor {
        x: c["x"].as_u64().expect("x") as u32,
        y: c["y"].as_u64().expect("y") as u32,
        w: c["w"].as_u64().expect("w") as u32,
        d: c["d"].as_u64().expect("d") as u32,
        wall: match c["wall"].as_str() {
            Some("right") => Some(WallSide::Right),
            Some("left") => Some(WallSide::Left),
            _ => None,
        },
        ok: c["ok"].as_bool().expect("ok"),
    });
    RoomView {
        width: room["width"].as_u64().expect("width") as u32,
        depth: room["depth"].as_u64().expect("depth") as u32,
        room: RoomRow {
            idx: 0,
            room_type: text(&room["room_type"]),
            wallpaper: text(&room["wallpaper"]),
            floor: text(&room["floor"]),
        },
        placements: room["placements"]
            .as_array()
            .expect("placements")
            .iter()
            .map(|p| PlacementRow {
                id: text(&p["id"]).parse().expect("an id"),
                room_idx: 0,
                item_id: text(&p["item_id"]),
                x: p["x"].as_i64().expect("x") as i16,
                y: p["y"].as_i64().expect("y") as i16,
                flipped: p["flipped"].as_bool().expect("flipped"),
            })
            .collect(),
        cursor,
        selected: options["selected"].as_str().map(|id| id.parse().expect("an id")),
        edit: options["edit"].as_bool().expect("edit"),
        pets: room["pets"]
            .as_array()
            .expect("pets")
            .iter()
            .map(|p| RoomPet {
                species: text(&p["species"]),
                asleep: p["asleep"].as_bool().expect("asleep"),
            })
            .collect(),
        characters: room["characters"]
            .as_array()
            .expect("characters")
            .iter()
            .map(|c| Character::decode(content, c.as_str().expect("a code")).expect("a character code"))
            .collect(),
        seed: room["seed"].as_u64().expect("seed"),
    }
}

fn render(scene: &Value) -> RgbaImage {
    room_image(&Sprites::default(), &view_from(scene), 1)
}

/// The catalogue entries the scenes use and nothing else, so an unrelated new chair does not
/// make the fixtures stale.
fn used_catalog(scenes: &[Value]) -> Value {
    let mut bases = BTreeSet::new();
    let (mut wallpapers, mut floors, mut pets, mut rooms, mut variants) = (
        BTreeSet::new(),
        BTreeSet::new(),
        BTreeSet::new(),
        BTreeSet::new(),
        BTreeSet::new(),
    );
    for scene in scenes {
        let room = &scene["room"];
        wallpapers.insert(room["wallpaper"].as_str().expect("wallpaper").to_owned());
        floors.insert(room["floor"].as_str().expect("floor").to_owned());
        rooms.insert(room["room_type"].as_str().expect("room type").to_owned());
        for p in room["placements"].as_array().expect("placements") {
            let item = p["item_id"].as_str().expect("item");
            let plain = item.strip_suffix("~bound").unwrap_or(item);
            let (base, variant) = plain.split_once("__").map_or((plain, None), |(b, v)| (b, Some(v)));
            bases.insert(base.to_owned());
            variants.extend(variant.map(str::to_owned));
        }
        for p in room["pets"].as_array().expect("pets") {
            pets.insert(p["species"].as_str().expect("species").to_owned());
        }
    }
    // Reef navigation fixtures share this catalogue and cover the first five seat bases.
    bases.insert("reedwink_stool".to_owned());
    let all = catalog();
    let keep = |list: &Value, ids: &BTreeSet<String>| -> Vec<Value> {
        list.as_array()
            .expect("a list")
            .iter()
            .filter(|v| ids.contains(v["id"].as_str().expect("an id")))
            .cloned()
            .collect()
    };
    let ramps: serde_json::Map<String, Value> = all["variant_ramps"]
        .as_object()
        .expect("ramps")
        .iter()
        .filter(|(name, _)| variants.contains(*name))
        .map(|(name, ramp)| (name.clone(), ramp.clone()))
        .collect();
    json!({
        "furniture": keep(&all["furniture"], &bases),
        "wallpapers": keep(&all["wallpapers"], &wallpapers),
        "floors": keep(&all["floors"], &floors),
        "room_types": keep(&all["room_types"], &rooms),
        "pets": keep(&all["pets"], &pets),
        "tiers": all["tiers"],
        "base_ramp": all["base_ramp"],
        "variant_ramps": ramps,
    })
}

/// FNV-1a over the pixels, with the colour of see-through pixels ignored.
fn fingerprint(img: &RgbaImage) -> String {
    let mut hash = 0x811c_9dc5_u32;
    for px in img.pixels() {
        let bytes = if px[3] == 0 { [0; 4] } else { px.0 };
        for b in bytes {
            hash = (hash ^ u32::from(b)).wrapping_mul(0x0100_0193);
        }
    }
    format!("{hash:08x}")
}

/// A pet-like block: `body` pixels of orange in a larger `frame`, an eye near its corner so a turn or
/// a mirror shows. The browser tests build the very same sprite.
fn block(frame: (u32, u32), at: (u32, u32), body: (u32, u32)) -> RgbaImage {
    let mut img = RgbaImage::new(frame.0, frame.1);
    for y in at.1..at.1 + body.1 {
        for x in at.0..at.0 + body.0 {
            img.put_pixel(x, y, Rgba([200, 90, 60, 255]));
        }
    }
    img.put_pixel(at.0 + 2, at.1 + 1, Rgba([255, 255, 255, 255]));
    img
}

/// Sprite-level cases: the art the room draws when nothing is drawn yet, the palette swap and the
/// sleeping pet, each as a size and a fingerprint.
fn sprite_cases() -> Value {
    let mut cases = Vec::new();
    let mut add = |case: Value, img: RgbaImage| {
        let mut case = case;
        case["size"] = json!([img.width(), img.height()]);
        case["hash"] = json!(fingerprint(&img));
        cases.push(case);
    };
    for (id, w, d, layer, iso) in [
        ("chair", 1, 1, Layer::Floor, None),
        ("sofa", 2, 1, Layer::Floor, Some(96)),
        ("bed", 2, 2, Layer::Floor, Some(160)),
        ("wide_table", 3, 2, Layer::Floor, None),
        ("vase", 1, 1, Layer::Surface, None),
        ("lamp", 1, 1, Layer::Surface, Some(20)),
        ("poster", 1, 2, Layer::Wall, None),
        ("shelf", 3, 1, Layer::Wall, None),
        ("runner", 4, 1, Layer::Rug, None),
        ("big_rug", 3, 3, Layer::Rug, None),
    ] {
        add(
            json!({ "kind": "placeholder_item", "id": id, "w": w, "d": d, "layer": layer_name(layer), "iso_width": iso }),
            placeholder_item(id, w, d, scene_layer(layer), iso),
        );
    }
    for (id, wall) in [
        ("oak", true),
        ("birch", true),
        ("pine", false),
        ("slate", false),
        ("reef", true),
    ] {
        add(
            json!({ "kind": "placeholder_tile", "id": id, "wall": wall }),
            placeholder_tile(id, wall),
        );
    }
    for id in ["blobfish", "sprite", "moss"] {
        add(json!({ "kind": "placeholder_pet", "id": id }), placeholder_pet(id));
    }
    for (frame, at, body) in [
        ((20, 30), (4, 2), (12, 26)),
        ((30, 20), (2, 4), (26, 16)),
        ((8, 8), (0, 0), (0, 0)),
    ] {
        for left in [true, false] {
            add(
                json!({ "kind": "sleeping_pet", "frame": [frame.0, frame.1], "at": [at.0, at.1],
                         "body": [body.0, body.1], "left": left }),
                sleeping_pet(&block(frame, at, body), left),
            );
        }
    }
    let sprite = placeholder_item("sofa", 2, 1, scene_layer(Layer::Floor), Some(96));
    for name in [
        "mint", "navy", "cream", "plum", "gold", "sky", "moss", "rose", "charcoal", "sand", "lilac", "coral",
    ] {
        let ramp = variant_ramp(name).expect("a known ramp");
        add(
            json!({ "kind": "recolor", "name": name, "ramp": ramp, "id": "sofa", "w": 2, "d": 1, "iso_width": 96 }),
            recolor(&sprite, &ramp),
        );
    }
    json!(cases)
}

fn layer_name(layer: Layer) -> &'static str {
    match layer {
        Layer::Floor => "floor",
        Layer::Rug => "rug",
        Layer::Wall => "wall",
        Layer::Surface => "surface",
    }
}

/// Everything the fixture directory holds, keyed by file name: JSON text and PNG bytes.
fn build() -> Vec<(String, Vec<u8>)> {
    let sprites = Sprites::default();
    let defs = defs();
    let mut files = Vec::new();
    let mut scenes = Vec::new();
    for def in &defs {
        let mut scene = scene_json(def);
        // Character sprites travel as files next to the scene, by the code the room lists.
        let mut sprite_files = serde_json::Map::new();
        for (i, c) in def.characters.iter().enumerate() {
            let file = format!("{}.character{i}.png", def.name);
            files.push((file.clone(), encode_png(&sprites.character(c)).expect("encodes")));
            sprite_files.insert(c.encode(), json!(file));
        }
        scene["character_sprites"] = Value::Object(sprite_files);
        let img = render(&scene);
        scene["png"] = json!(format!("{}.png", def.name));
        scene["size"] = json!([img.width(), img.height()]);
        scene["hash"] = json!(fingerprint(&img));
        files.push((format!("{}.png", def.name), encode_png(&img).expect("encodes")));
        scenes.push(scene);
    }
    for scene in &scenes {
        let name = scene["name"].as_str().expect("a name");
        files.push((format!("{name}.json"), pretty(scene)));
    }
    files.push(("content.json".to_owned(), pretty(&used_catalog(&scenes))));
    files.push(("sprites.json".to_owned(), pretty(&sprite_cases())));
    let names: Vec<&str> = scenes.iter().map(|s| s["name"].as_str().expect("a name")).collect();
    files.push(("index.json".to_owned(), pretty(&json!({ "scenes": names }))));
    files
}

fn pretty(value: &Value) -> Vec<u8> {
    let mut text = serde_json::to_string_pretty(value).expect("serialises");
    text.push('\n');
    text.into_bytes()
}

/// Writes the fixtures the browser tests read.
#[test]
#[ignore = "writes web/src/iso/fixtures"]
fn write_house_fixtures() {
    let dir = dir();
    fs::create_dir_all(&dir).expect("fixture directory");
    for entry in fs::read_dir(&dir).expect("fixture directory").flatten() {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.ends_with(".png") || name.ends_with(".json") {
            fs::remove_file(entry.path()).expect("old fixture removed");
        }
    }
    for (name, bytes) in build() {
        fs::write(dir.join(name), bytes).expect("fixture written");
    }
}

fn pixels(png: &[u8]) -> RgbaImage {
    image::load_from_memory(png).expect("a PNG").to_rgba8()
}

/// The committed scenes and sprite cases still come out of today's renderer, art and catalogue
/// exactly as committed.
#[test]
fn house_fixtures_match() {
    let dir = dir();
    for (name, bytes) in build() {
        let path = dir.join(&name);
        let committed = fs::read(&path).unwrap_or_else(|_| panic!("{name} is missing: {REGENERATE}"));
        if name.ends_with(".png") {
            let (fresh, old) = (pixels(&bytes), pixels(&committed));
            assert_eq!(
                fresh.dimensions(),
                old.dimensions(),
                "{name} changed size: {REGENERATE}"
            );
            let differing = fresh.pixels().zip(old.pixels()).filter(|(a, b)| a != b).count();
            assert_eq!(differing, 0, "{name} differs in {differing} pixels: {REGENERATE}");
        } else {
            assert_eq!(
                String::from_utf8_lossy(&bytes),
                String::from_utf8_lossy(&committed),
                "{name} is stale: {REGENERATE}"
            );
        }
    }
}

/// The scenes use more than one of everything the renderer knows: both walls, every layer, variants,
/// rugs, sleepers and walkers, characters, the grid and both cursor kinds.
#[test]
fn the_scenes_cover_the_renderer() {
    let content = Content::get();
    let defs = defs();
    let layers: BTreeSet<&str> = defs
        .iter()
        .flat_map(|d| &d.placements)
        .filter_map(|(id, ..)| content.item(id))
        .map(|i| layer_name(i.base.layer))
        .collect();
    assert_eq!(layers, BTreeSet::from(["floor", "rug", "surface", "wall"]));
    assert!(
        defs.iter()
            .flat_map(|d| &d.placements)
            .any(|(id, _, _, f)| *f && content.item(id).is_some_and(|i| i.base.layer == Layer::Wall)),
        "something hangs on the left wall"
    );
    assert!(
        defs.iter()
            .any(|d| d.edit && d.cursor.is_some_and(|c| c.wall.is_some()))
    );
    assert!(defs.iter().any(|d| d.cursor.is_some_and(|c| c.wall.is_none())));
    assert!(defs.iter().any(|d| d.pets.iter().any(|p| p.asleep)));
    assert!(defs.iter().any(|d| d.pets.iter().any(|p| !p.asleep)));
    assert!(defs.iter().any(|d| d.characters.len() > 1));
    let surface_lifts = defs.iter().any(|d| {
        let placed: Vec<Placed<'_>> = d
            .placements
            .iter()
            .map(|(item_id, x, y, flipped)| Placed {
                item_id,
                x: *x as u32,
                y: *y as u32,
                flipped: *flipped,
            })
            .collect();
        d.placements.iter().any(|(id, x, y, _)| {
            content.item(id).is_some_and(|i| i.base.layer == Layer::Surface)
                && house::surface_lift(content, &placed, *x as u32, *y as u32) > 0
        })
    });
    assert!(surface_lifts, "something stands on a table");
    for d in defs.iter().filter(|d| d.name.starts_with("crowded")) {
        assert!(d.placements.len() >= 25, "{} is crowded", d.name);
    }
}
