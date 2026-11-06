#!/usr/bin/env python3
"""Rebuild Pinchora's brand kit offline. Requires Pillow >= 10; no API or secrets.

Run from any directory: python3 tools/brand/build.py
AI originals and prompts are retained beside this file. Every resize is NEAREST.
"""
from __future__ import annotations

import base64
import hashlib
import json
import math
from pathlib import Path

from PIL import Image, ImageDraw, ImageFont

ROOT = Path(__file__).resolve().parents[2]
SOURCE = Path(__file__).parent / "sources"
OUT = ROOT / "assets/brand"
WEB = ROOT / "web/public"
NEAREST = Image.Resampling.NEAREST
NAVY = "#1d2b53"
DEEP = "#0f1631"
CREAM = "#fff1e8"
TAGLINE = "Build your reef. Play with your server."
CHANNELS = ("rules", "announcements", "support", "showcase", "games", "fishing")
COLORS = (ROOT / "assets/sprites/palette.hex").read_text().split() + ["141b3d", "ff4d6d"]
PALETTE = Image.new("P", (1, 1))
RGB = [tuple(bytes.fromhex(c)) for c in COLORS]
PALETTE.putpalette([v for c in RGB for v in c] + list(RGB[0]) * (256 - len(RGB)))
RECORDS = []


def save(im, path, origin):
    path.parent.mkdir(parents=True, exist_ok=True)
    im.save(path, optimize=True)
    RECORDS.append({"path": str(path.relative_to(ROOT)), "width": im.width,
                    "height": im.height, "origin": origin,
                    "sha256": hashlib.sha256(path.read_bytes()).hexdigest()})


def palette(im, logo=False):
    alpha = im.convert("RGBA").getchannel("A").point(lambda a: 255 if a >= 128 else 0)
    pal = PALETTE
    if logo:
        colors = [tuple(bytes.fromhex(c)) for c in ("0f1631", "1d2b53", "ff6b4a", "c2413f", "ff9e7a", "fff1e8")]
        pal = Image.new("P", (1, 1))
        pal.putpalette([v for c in colors for v in c] + list(colors[0]) * (256-len(colors)))
    result = im.convert("RGB").quantize(palette=pal, dither=Image.Dither.NONE).convert("RGBA")
    result.putalpha(alpha)
    return result


def fit(im, size):
    """Contain without distortion, rounding each edge to the nearest output pixel."""
    scale = min(size[0] / im.width, size[1] / im.height)
    return im.resize((max(1, round(im.width * scale)), max(1, round(im.height * scale))), NEAREST)


def cover(im, size, anchor=(0.5, 0.5)):
    scale = max(size[0] / im.width, size[1] / im.height)
    big = im.resize((math.ceil(im.width * scale), math.ceil(im.height * scale)), NEAREST)
    x = round((big.width - size[0]) * anchor[0])
    y = round((big.height - size[1]) * anchor[1])
    return big.crop((x, y, x + size[0], y + size[1]))


def scene(name):
    im = Image.open(SOURCE / f"{name}.png").convert("RGBA")
    # One logical pixel becomes two or more export pixels; palette removes AI color noise.
    result = palette(fit(im, (1086, 362)))
    # AI navy texture straddles two almost identical palette colors. Merge them
    # into a flat panel navy so neither grain nor stretched scanlines survive.
    pixels = result.load()
    for y in range(result.height):
        for x in range(result.width):
            if pixels[x, y][:3] == (15, 22, 49):
                pixels[x, y] = (20, 27, 61, 255)
    return result


def text_image(text, scale=3, bold=False):
    font = ImageFont.truetype(str(ROOT / "assets/fonts" / ("Silkscreen-Bold.ttf" if bold else "Silkscreen-Regular.ttf")), 8)
    box = font.getbbox(text)
    mask = Image.new("1", (box[2] - box[0], box[3] - box[1]))
    ImageDraw.Draw(mask).text((-box[0], -box[1]), text, font=font, fill=1)
    im = Image.new("RGBA", mask.size, CREAM)
    im.putalpha(mask.convert("L"))
    return im.resize((im.width * scale, im.height * scale), NEAREST)


def paste_center(canvas, im, y):
    canvas.alpha_composite(im, ((canvas.width - im.width) // 2, y))


def mascot():
    im = palette(Image.open(SOURCE / "icon.png").resize((64, 64), NEAREST), logo=True)
    # Remove the generated tile and retain the largest connected mascot silhouette.
    navy = tuple(bytes.fromhex("1d2b53"))
    active = {(x, y) for y in range(64) for x in range(64)
              if im.getpixel((x, y))[3] and im.getpixel((x, y))[:3] != navy}
    components = []
    while active:
        todo = [active.pop()]
        component = set(todo)
        while todo:
            x, y = todo.pop()
            for p in ((x-1, y), (x+1, y), (x, y-1), (x, y+1)):
                if p in active:
                    active.remove(p)
                    component.add(p)
                    todo.append(p)
        components.append(component)
    keep = max(components, key=len)
    # Preserve enclosed facial details, including the tongue.
    outside = set()
    todo = [(0, 0)]
    while todo:
        p = todo.pop()
        x, y = p
        if p in outside or p in keep or not (0 <= x < 64 and 0 <= y < 64):
            continue
        outside.add(p)
        todo.extend(((x-1, y), (x+1, y), (x, y-1), (x, y+1)))
    for y in range(64):
        for x in range(64):
            if (x, y) in outside:
                im.putpixel((x, y), (0, 0, 0, 0))
    return im.crop(im.getbbox())


def tile(mark, size, rounded=True):
    im = Image.new("RGBA", (128, 128), NAVY)
    small = fit(mark, (88, 76))
    im.alpha_composite(small, ((128-small.width)//2, (128-small.height)//2))
    if rounded:
        mask = Image.new("L", (128, 128))
        ImageDraw.Draw(mask).rounded_rectangle((0, 0, 127, 127), radius=24, fill=255)
        im.putalpha(mask)
    return im.resize((size, size), NEAREST)


def channel(im):
    # Fit the whole illustration: do not cut roofs or fishing lines to force 4:1.
    art = fit(im, (1920, 480))
    canvas = Image.new("RGBA", (1920, 480), DEEP)
    # Continue the empty ocean at its left edge; never stretch any scene objects.
    left = 1920 - art.width
    canvas.alpha_composite(art.crop((0, 0, 1, art.height)).resize((left, 480), NEAREST))
    canvas.alpha_composite(art, (left, 0))
    return canvas


def main():
    OUT.mkdir(parents=True, exist_ok=True)
    mark = mascot()
    save(mark.resize((mark.width*8, mark.height*8), NEAREST), OUT / "mascot.png", "AI icon; palette and background extraction")
    save(mark.resize((mark.width*8, mark.height*8), NEAREST), WEB / "mascot.png", "Identical mascot for existing Crab component")
    for size in (1024, 512, 256):
        save(tile(mark, size), OUT / f"icon-{size}.png", "AI icon; scripted tile and nearest-neighbour export")
    round_icon = tile(mark, 1024, rounded=False)
    mask = Image.new("L", (64, 64))
    ImageDraw.Draw(mask).ellipse((0, 0, 63, 63), fill=255)
    round_icon.putalpha(mask.resize((1024, 1024), NEAREST))
    save(round_icon, OUT / "icon-round.png", "AI icon; scripted circular mask")
    mono = Image.new("RGBA", mark.size, CREAM)
    mono.putalpha(mark.getchannel("A"))
    mono_canvas = Image.new("RGBA", (1024, 1024))
    m = fit(mono, (704, 608))
    mono_canvas.alpha_composite(m, ((1024-m.width)//2, (1024-m.height)//2))
    save(mono_canvas, OUT / "icon-monochrome.png", "AI silhouette; scripted one-ink cream variant")

    word = palette(Image.open(SOURCE / "wordmark.png"), logo=True)
    word = word.crop(word.getbbox())
    word = palette(fit(word, (480, 120)), logo=True)
    logo = Image.new("RGBA", (512, 144))
    logo.alpha_composite(word, ((512-word.width)//2, (144-word.height)//2))
    logo = logo.resize((2048, 576), NEAREST)
    save(logo, OUT / "wordmark.png", "AI mascot and PINCHORA lettering; scripted palette, alpha, margins")

    hero = scene("hero")
    save(hero.resize((2172, 724), NEAREST), OUT / "hero.png", "AI reef panorama; scripted palette and pixel grid")
    for name, size in (("discord-app-banner", (680, 240)), ("x-banner", (1500, 500))):
        canvas = cover(hero, size, (0.8, 0.5))
        # The left third was deliberately kept quiet for the logo.
        badge = fit(logo, (round(size[0]*0.34), round(size[1]*0.34)))
        canvas.alpha_composite(badge, (round(size[0]*0.025), round(size[1]*0.38)))
        save(canvas, OUT / f"{name}.png", "Scripted hero crop + AI wordmark")

    server = Image.new("RGBA", (960, 540), DEEP)
    server.alpha_composite(fit(hero, (960, 320)), (0, 220))
    paste_center(server, fit(logo, (600, 170)), 30)
    save(server, OUT / "server-banner.png", "Scripted complete panorama + AI wordmark")

    youtube = Image.new("RGBA", (2560, 1440), hero.getpixel((0, 0)))
    # Full scene remains intact on television; all branding fits mobile's safe rectangle.
    art = fit(hero, (1740, 580))
    youtube.alpha_composite(art.crop((0, 0, 1, art.height)).resize((820, art.height), NEAREST), (0, 1440-art.height))
    youtube.alpha_composite(art, (2560-art.width, 1440-art.height))
    yt_logo = fit(logo, (1080, 304))
    youtube.alpha_composite(yt_logo, (740, 532))
    line = text_image(TAGLINE, 4)
    youtube.alpha_composite(line, ((2560-line.width)//2, 822))
    safe = (507, 508, 2053, 931)  # 1546 × 423, centered to the nearest integer pixel.
    assert (2560+line.width)//2 < safe[2] and 822 + line.height < safe[3]
    assert safe[0] <= 740 and 740 + yt_logo.width <= safe[2]
    assert safe[1] <= 532 and 532 + yt_logo.height <= safe[3]
    save(youtube, OUT / "youtube-banner.png", "Scripted hero + AI wordmark + Silkscreen; centered 1546x423 safe area")

    og = Image.new("RGBA", (1200, 630), DEEP)
    og.alpha_composite(fit(hero, (1200, 400)), (0, 170))
    og.alpha_composite(fit(logo, (640, 180)), (40, 0))
    ImageDraw.Draw(og).rectangle((0, 570, 1199, 629), fill=DEEP)
    paste_center(og, text_image(TAGLINE, 4), 592)
    save(og, OUT / "og.png", "Scripted composition; AI scene/logo; exact Silkscreen tagline")
    save(og, WEB / "og.png", "Identical brand OG image; existing web path")
    for name in CHANNELS:
        save(channel(scene(name)), OUT / "channels" / f"{name}.png", "AI scene; scripted palette/grid and empty-water extension")

    for size, name in ((16, "favicon-16x16.png"), (32, "favicon-32x32.png"), (48, "favicon-48x48.png"),
                       (180, "apple-touch-icon.png"), (192, "icon-192.png"), (512, "icon-512.png")):
        save(tile(mark, size, rounded=False), WEB / name, "AI mascot; scripted opaque navy favicon/PWA tile")
    icon = tile(mark, 48, rounded=False)
    icon.save(WEB / "favicon.ico", sizes=[(16, 16), (32, 32), (48, 48)],
              append_images=[tile(mark, 16, False), tile(mark, 32, False)])
    encoded = base64.b64encode((WEB / "icon-512.png").read_bytes()).decode()
    (WEB / "favicon.svg").write_text('<svg xmlns="http://www.w3.org/2000/svg" width="512" height="512" viewBox="0 0 512 512"><title>Pinchora crab</title><image width="512" height="512" style="image-rendering:pixelated" href="data:image/png;base64,' + encoded + '"/></svg>\n')
    manifest = {"name": "Pinchora", "short_name": "Pinchora", "description": TAGLINE,
                "start_url": "/", "display": "browser", "background_color": DEEP, "theme_color": NAVY,
                "icons": [{"src": f"/icon-{s}.png", "sizes": f"{s}x{s}", "type": "image/png", "purpose": "any maskable"} for s in (192, 512)]}
    (WEB / "site.webmanifest").write_text(json.dumps(manifest, indent=2) + "\n")

    sprites = {"shell": "b_community_pearl", "crab": "p_crumbclaw_crab", "pearl": "b_community_pearl",
               "fish": "c_pearlpatch_koi", "heart": "b_heartfelt_home", "star": "b_snoozy_star",
               "house": "f_buttonburrow_house", "rod": "c_shellscript_key", "coin": "c_brinebell_coin",
               "chest": "c_moonkeel_chest", "key": "c_shellscript_key", "bunny": "p_cloverpuff_bun",
               "seal": "p_pearlwhisk_seal", "squid": "p_starstitch_squid", "wave": "b_tiny_tide",
               "lantern": "b_little_beacon"}
    provenance = {}
    for name, source in sprites.items():
        im = Image.open(ROOT / f"assets/sprites/src/{source}.png").convert("RGBA")
        method = "transparent trim and nearest-neighbour contain"
        if name == "pearl":
            im = im.crop((8, 9, 17, 16))
            pearl_mask = Image.new("L", im.size)
            ImageDraw.Draw(pearl_mask).ellipse((0, 0, 8, 6), fill=255)
            im.putalpha(pearl_mask)
            method = "pearl crop [8,9,17,16] from shell badge; hard pixel ellipse mask"
        elif name == "heart":
            im = im.crop((8, 13, 16, 20))
            bg = im.load()
            for y in range(im.height):
                for x in range(im.width):
                    if bg[x, y][:3] != (255, 107, 74):
                        bg[x, y] = (0, 0, 0, 0)
            method = "coral heart pixels extracted from home badge [8,13,16,20]"
        elif name == "rod":
            # Existing key's gold shaft becomes a fishing pole; add its cream line and reel.
            shaft = im.crop((9, 3, 24, 5)).resize((24, 2), NEAREST).rotate(45, resample=NEAREST, expand=True)
            im = Image.new("RGBA", (28, 28))
            im.alpha_composite(shaft, (3, 2))
            draw = ImageDraw.Draw(im)
            draw.line((22, 3, 22, 23), fill=CREAM, width=1)
            draw.rectangle((20, 23, 24, 25), fill="#ff6b4a")
            draw.rectangle((4, 18, 8, 22), fill="#ffd166")
            draw.rectangle((5, 19, 7, 21), fill=DEEP)
            method = "remix of key shaft [9,3,24,5], nearest 45-degree rotation; scripted line/reel/bobber"
        im = im.crop(im.getbbox())
        im = fit(im, (28, 28))
        canvas = Image.new("RGBA", (32, 32))
        canvas.alpha_composite(im, ((32-im.width)//2, (32-im.height)//2))
        save(canvas, OUT / "emoji" / f"{name}.png", f"Existing sprite {source}; {method}")
        provenance[name] = {"source": f"assets/sprites/src/{source}.png", "method": method}
    (OUT / "emoji/sources.json").write_text(json.dumps(provenance, indent=2) + "\n")
    (OUT / "manifest.json").write_text(json.dumps({"tagline": TAGLINE, "youtube_safe_area": safe,
         "pngs": RECORDS}, indent=2) + "\n")
    verify()
    print(f"Built and verified {len(RECORDS)} PNG exports. See assets/brand/manifest.json.")


def verify():
    for record in RECORDS:
        im = Image.open(ROOT / record["path"])
        assert im.size == (record["width"], record["height"]), record["path"]
        im.verify()
    assert len(list((OUT / "emoji").glob("*.png"))) == 16
    assert Image.open(OUT / "wordmark.png").getchannel("A").getextrema() == (0, 255)
    assert set(Image.open(WEB / "favicon.ico").ico.sizes()) == {(16,16), (32,32), (48,48)}
    # Ensure existing HTML icon links still resolve, without accessing application env files.
    import re
    html = (ROOT / "web/index.html").read_text()
    for href in re.findall(r'href="(/[^" ]+)"', html):
        if "favicon" in href or "apple-touch" in href or "webmanifest" in href:
            assert (WEB / href.lstrip("/")).exists(), href


if __name__ == "__main__":
    main()
