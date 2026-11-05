#!/usr/bin/env python3
"""Plans GPT sprite sheets from content/*.json.

For every sheet it writes `sheets/<name>.prompt.txt` (what Codex draws) and
`sheets/<name>.manifest.json` (how `pinchora-assets cut` slices it). Run from tools/art/.
"""
import json
import pathlib

ROOT = pathlib.Path(__file__).resolve().parents[2]
CONTENT = ROOT / "content"
OUT = pathlib.Path(__file__).resolve().parent / "sheets"
PALETTE = (ROOT / "assets/sprites/palette.hex").read_text().split()

STYLE = f"""Use your built-in image generation tool to draw ONE sprite sheet for "Pinchora", a cozy seaside pixel-art life game where players build and furnish rooms seen in an ISOMETRIC view.
The attached image is the style reference: match its camera, pixel size, outline, shading and mood exactly.

Rules:
- True pixel art: crisp square pixels at one consistent size, no anti-aliasing, no blur, no gradients, no noise. Fine detail: one floor tile is about 64 art pixels wide.
- 1-pixel dark outline #1b1d2b around every object, light from the upper-left, 2-3 tone shading.
- Use only these colours: {' '.join(PALETTE)}.
- Background: flat pure #00FF00 everywhere around the objects (a fully transparent background is equally fine). Nothing else on the background: no floor, no tiles, no shadows, no text, no labels, no numbers, no grid lines.
- Generate the image once and save it. Only regenerate (at most once) if objects are missing, merged together or drawn from the wrong angle; do not regenerate for background, colour or size nitpicks, our importer fixes those.
- Draw the objects in a grid in exactly the order listed (row by row, left to right), each one complete and isolated with wide green gaps (at least 40 px) between them; parts of one object must touch each other.
"""

ISO = ("View: 2:1 isometric exactly like the reference (floor diamonds twice as wide as tall). "
       "Every object's FRONT faces the LOWER-LEFT: we see its front on the lower-left, its right side on the "
       "lower-right and its top. Keep relative sizes: a 1x1-tile object is about as wide as the reference chair.")
FLAT_WALL = ("View: FLAT, seen straight from the front with no perspective at all (our engine slants it onto the wall). "
             "Each object fills its own rectangle with the exact proportions given.")
FLAT_RUG = ("View: FLAT, seen straight from above with no perspective at all (our engine lays it on the floor). "
            "Each rug fills its own rectangle with the exact proportions given; round rugs touch the rectangle edges.")

RECOLOR = ("Its main colour (fabric, paint or plastic) must use the coral ramp "
           "#7a2a3a (shadow), #c2413f, #ff6b4a (base), #ff9e7a, #ffd2c0 (highlight); keep wood brown and metal grey-blue.")


LOOKS = (ISO + " Character sheet: full-body player characters, all the SAME height and the SAME body "
         "(a friendly chibi-ish adult, head about a quarter of the height), standing relaxed, arms down, in the "
         "same pose, facing the lower-left. For characters the colour list above does NOT apply; use exactly "
         "this colour code instead, required because our engine recolours these regions: "
         "skin only in #8a4b3b #c97a5c #f0a98a #ffd8c2; hair only in the purple ramp #3b1f6b #5e35a8 #8a5ad8 "
         "#b994f0; top, shirt, jacket or dress only in the turquoise ramp #0b5e6b #0f8a9a #19b8c8 #6fe0ea #c0f6fa; "
         "trousers, shorts or skirt only in the blue ramp #1d2b53 #2e3d75 #4a5ba3 #8fa0d8; shoes in browns "
         "#3b2a24 #6b4a3a #a0705a; small details (buttons, glasses, laces) in greys #45495c #6a6f86 #a3a8bd or "
         "gold #c98a1e #ffd166; eyes #1b1d2b with a #fff1e8 highlight. No hats, no held items.")


def iso_width(f):
    w, d = f["footprint"]
    if f.get("iso_width"):
        return min(f["iso_width"], (w + d) * 32)
    return 24 if f["layer"] == "surface" else (w + d) * 32


def run(w, d):
    if w > d:
        return f"{w}x{d} tiles, its long side runs from upper-left to lower-right"
    if d > w:
        return f"{w}x{d} tiles, its long side runs from upper-right to lower-left"
    return f"{w}x{d} tiles"


def chunks(items, n):
    for i in range(0, len(items), n):
        yield items[i:i + n]


def write_sheet(name, header, entries, cols, manifest_items, extra=None):
    rows = (len(entries) + cols - 1) // cols
    lines = [STYLE, header, f"Layout: {cols} columns x {rows} rows, {len(entries)} objects in total.", ""]
    for i, text in enumerate(entries, 1):
        lines.append(f"{i}. {text}")
    lines.append("")
    lines.append(f"Save the final PNG to ./out/{name}.png (create ./out if needed). Exactly {len(entries)} objects.")
    (OUT / f"{name}.prompt.txt").write_text("\n".join(lines))
    (OUT / f"{name}.manifest.json").write_text(json.dumps({**(extra or {}), "items": manifest_items}, indent=1))


def main():
    OUT.mkdir(exist_ok=True)
    furniture = json.loads((CONTENT / "furniture.json").read_text())
    surfaces = json.loads((CONTENT / "surfaces.json").read_text())
    pets = json.loads((CONTENT / "pets.json").read_text())
    fish = json.loads((CONTENT / "fish.json").read_text())
    looks = json.loads((CONTENT / "looks.json").read_text())
    sheets = []

    def furniture_sheets(prefix, group_items, per_sheet, cols, header, describe, manifest_item):
        for n, group in enumerate(chunks(group_items, per_sheet), 1):
            name = f"{prefix}_{n:02d}"
            entries = []
            for f in group:
                text = f"{f['name']} — {f['sprite_brief']} ({describe(f)})"
                if f["variants"]:
                    text += " " + RECOLOR
                entries.append(text)
            write_sheet(name, header, entries, cols, [manifest_item(f) for f in group])
            sheets.append(name)

    furniture.sort(key=lambda f: (f["category"], f["id"]))
    floor = [f for f in furniture if f["layer"] == "floor"]
    iso_item = lambda f: {"name": f"f_{f['id']}", "width": iso_width(f)}
    for prefix, items, per, cols in (
        ("iso_small", [f for f in floor if iso_width(f) <= 64], 8, 4),
        ("iso_medium", [f for f in floor if 64 < iso_width(f) <= 128], 6, 3),
        ("iso_large", [f for f in floor if iso_width(f) > 128], 4, 2),
    ):
        furniture_sheets(prefix, items, per, cols, "Furniture sheet. " + ISO,
                         lambda f: run(*f["footprint"]), iso_item)
    furniture_sheets("iso_surface", [f for f in furniture if f["layer"] == "surface"], 12, 4,
                     "Small tabletop decorations. " + ISO,
                     lambda f: "small decoration that stands on a table, well under one tile", iso_item)
    furniture_sheets("flat_wall", [f for f in furniture if f["layer"] == "wall"], 8, 4,
                     "Wall decorations. " + FLAT_WALL,
                     lambda f: f"proportions {f['footprint'][0]} wide by {f['footprint'][1]} tall",
                     lambda f: {"name": f"f_{f['id']}", "width": f["footprint"][0] * 32 - 2,
                                "height": f["footprint"][1] * 32 - 2})
    furniture_sheets("flat_rug", [f for f in furniture if f["layer"] == "rug"], 6, 3,
                     "Rugs. " + FLAT_RUG,
                     lambda f: f"proportions {f['footprint'][0]} wide by {f['footprint'][1]} tall",
                     lambda f: {"name": f"f_{f['id']}", "width": f["footprint"][0] * 32,
                                "height": f["footprint"][1] * 32})

    for kind in ("wallpapers", "floors"):
        for n, group in enumerate(chunks(surfaces[kind], 20), 1):
            name = f"{kind}_{n:02d}"
            what = "wallpaper" if kind == "wallpapers" else "floor"
            entries = [f"{s['name']} — square seamless {what} tile: {s['pattern_brief']}" for s in group]
            prefix = "wp_" if kind == "wallpapers" else "fl_"
            manifest = [{"name": prefix + s["id"], "width": 16, "height": 16, "opaque": True} for s in group]
            header = (f"Tile sheet: {len(group)} separate SQUARE swatches, each one 16x16 art pixels drawn big, "
                      "a repeating seamless pattern that fills its whole square edge to edge (no outline around the square).")
            write_sheet(name, header, entries, 5, manifest)
            sheets.append(name)

    pet_width = {"common": 36, "uncommon": 40, "rare": 44, "epic": 48, "legendary": 52}
    for n, group in enumerate(chunks(pets, 8), 1):
        name = f"iso_pets_{n:02d}"
        entries = [f"{p['name']} — {p['sprite_brief']} (cute pet standing on the floor, facing the lower-left, "
                   "a little smaller than one floor tile)" for p in group]
        manifest = [{"name": f"p_{p['id']}", "width": pet_width[p["rarity"]]} for p in group]
        write_sheet(name, "Pet sheet. " + ISO, entries, 4, manifest)
        sheets.append(name)

    for n, group in enumerate(chunks(fish, 16), 1):
        name = f"fish_{n:02d}"
        entries = [f"{f['name']} — {f['sprite_brief']} (side view, facing right)" for f in group]
        manifest = [{"name": f"c_{f['id']}", "width": 24} for f in group]
        write_sheet(name, "Fish and catches sheet.", entries, 4, manifest)
        sheets.append(name)

    for n, group in enumerate(chunks(looks, 8), 1):
        name = f"iso_looks_{n:02d}"
        entries = [f"{l['name']} — {l['sprite_brief']}" for l in group]
        manifest = [{"name": f"a_{l['id']}", "width": 0, "height": 100} for l in group]
        write_sheet(name, LOOKS, entries, 4, manifest, {"palette": "avatar", "uniform_height": 100})
        sheets.append(name)

    (OUT / "queue.txt").write_text("\n".join(sheets) + "\n")
    print(f"planned {len(sheets)} sheets -> {OUT}")


if __name__ == "__main__":
    main()
