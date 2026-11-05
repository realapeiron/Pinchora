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

STYLE = f"""Use your built-in image generation tool to draw ONE sprite sheet for "Pinchora", a cozy pixel-art life game.
The attached image is the style reference: match its pixel size, outline, shading and mood exactly.

Rules:
- True pixel art with chunky square pixels, no anti-aliasing, no blur, no gradients, no texture noise.
- Top-down 3/4 view: we see the top and the front of each object, like the reference.
- 1-pixel dark outline #1b1d2b around every object, light from the top-left, 2-3 tone shading.
- Use only these colours: {' '.join(PALETTE)}.
- Background: flat pure #00FF00 everywhere around the objects. Nothing else on the background, no floor, no shadows on the background, no text, no labels, no numbers, no grid lines.
- Draw the objects in a grid in exactly the order listed (row by row, left to right), each one complete and isolated with wide green gaps (at least 40 px) between them; parts of one object must touch each other.
- Keep relative sizes: a 1-tile object is about as wide as a small chair; a 3-tile object is three times as wide.
"""

RECOLOR = ("Its main colour (fabric, paint or plastic) must use the coral ramp "
           "#7a2a3a (shadow), #c2413f, #ff6b4a (base), #ff9e7a, #ffd2c0 (highlight); keep wood brown and metal grey-blue.")


def width_for(layer, footprint):
    w = footprint[0]
    if layer == "surface":
        return 12
    return w * 16 - 2


def chunks(items, n):
    for i in range(0, len(items), n):
        yield items[i:i + n]


def write_sheet(name, header, entries, cols, manifest_items):
    rows = (len(entries) + cols - 1) // cols
    lines = [STYLE, header, f"Layout: {cols} columns x {rows} rows, {len(entries)} objects in total.", ""]
    for i, text in enumerate(entries, 1):
        lines.append(f"{i}. {text}")
    lines.append("")
    lines.append(f"Save the final PNG to ./out/{name}.png (create ./out if needed). Exactly {len(entries)} objects.")
    (OUT / f"{name}.prompt.txt").write_text("\n".join(lines))
    (OUT / f"{name}.manifest.json").write_text(json.dumps({"items": manifest_items}, indent=1))


def main():
    OUT.mkdir(exist_ok=True)
    furniture = json.loads((CONTENT / "furniture.json").read_text())
    surfaces = json.loads((CONTENT / "surfaces.json").read_text())
    pets = json.loads((CONTENT / "pets.json").read_text())
    fish = json.loads((CONTENT / "fish.json").read_text())
    sheets = []

    furniture.sort(key=lambda f: (f["category"], f["id"]))
    for n, group in enumerate(chunks(furniture, 12), 1):
        name = f"furniture_{n:02d}"
        entries, manifest = [], []
        for f in group:
            w, d = f["footprint"]
            size = ("small decoration that sits on a table (about half a tile)" if f["layer"] == "surface"
                    else f"{w} tile{'s' if w > 1 else ''} wide" + (f", {d} tiles deep" if f["layer"] != "wall" else f", {d} tile{'s' if d > 1 else ''} tall, hangs on a wall (front view)"))
            text = f"{f['name']} — {f['sprite_brief']} ({size})"
            if f["variants"]:
                text += " " + RECOLOR
            if f["layer"] == "rug":
                text += " Flat rug lying on the floor, seen from above."
            entries.append(text)
            manifest.append({"name": f"f_{f['id']}", "width": width_for(f["layer"], f["footprint"])})
        write_sheet(name, "Furniture sheet.", entries, 4, manifest)
        sheets.append(name)

    for kind in ("wallpapers", "floors"):
        for n, group in enumerate(chunks(surfaces[kind], 20), 1):
            name = f"{kind}_{n:02d}"
            what = "wallpaper" if kind == "wallpapers" else "floor"
            entries = [f"{s['name']} — square seamless {what} tile: {s['pattern_brief']}" for s in group]
            prefix = "wp_" if kind == "wallpapers" else "fl_"
            manifest = [{"name": prefix + s["id"], "width": 16, "height": 16} for s in group]
            header = (f"Tile sheet: {len(group)} separate SQUARE swatches, each one 16x16 art pixels drawn big, "
                      "a repeating seamless pattern that fills its whole square edge to edge (no outline around the square).")
            write_sheet(name, header, entries, 5, manifest)
            sheets.append(name)

    for n, group in enumerate(chunks(pets, 12), 1):
        name = f"pets_{n:02d}"
        entries = [f"{p['name']} — {p['sprite_brief']} (cute pet, sitting, facing the viewer, about 1 tile big)" for p in group]
        manifest = [{"name": f"p_{p['id']}", "width": 20} for p in group]
        write_sheet(name, "Pet sheet.", entries, 4, manifest)
        sheets.append(name)

    for n, group in enumerate(chunks(fish, 16), 1):
        name = f"fish_{n:02d}"
        entries = [f"{f['name']} — {f['sprite_brief']} (side view, facing right)" for f in group]
        manifest = [{"name": f"c_{f['id']}", "width": 24} for f in group]
        write_sheet(name, "Fish and catches sheet.", entries, 4, manifest)
        sheets.append(name)

    (OUT / "queue.txt").write_text("\n".join(sheets) + "\n")
    print(f"planned {len(sheets)} sheets -> {OUT}")


if __name__ == "__main__":
    main()
