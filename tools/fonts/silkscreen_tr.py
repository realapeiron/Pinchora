#!/usr/bin/env python3
"""Adds the Turkish letters Silkscreen lacks (İ ı Ş ş Ğ ğ) to assets/fonts/Silkscreen-{Regular,Bold}.ttf.

Silkscreen is an all-caps pixel font on a 125-unit pixel grid (eight pixels to the em): the Regular cap is
4 x 5 pixels, the Bold one 5 x 5. Each new glyph is built from the font's own outline plus whole pixels:

    İ  I with a dot one pixel row above the gap, where the dots of Ö and Ü sit (U+0130)
    ı  the plain I bar, as `i` already is in an all-caps font (U+0131)
    Ş ş  S with the font's own cedilla, the way Ç draws it (U+015E U+015F)
    Ğ ğ  G with a one-pixel breve in the two rows where Â has its circumflex (U+011E U+011F)

and puts the dots of Ö ö Ü ü and the circumflex of Regular's Û on the pixel grid: they sat half a pixel off it,
so they blurred.

The script edits the fonts in place and can be re-run: it redefines the glyphs it owns and leaves the rest of
the font alone. Needs fontTools (`python3 -m venv v && v/bin/pip install fonttools`).

    python3 tools/fonts/silkscreen_tr.py [assets/fonts]
"""

import sys
from pathlib import Path

from fontTools.pens.ttGlyphPen import TTGlyphPen
from fontTools.ttLib import TTFont

PX = 125  # one font pixel, in units (the em is 1000)

NOTE = "Modified for Pinchora: added the Turkish letters I-dot, dotless i, S-cedilla and G-breve, on the font's pixel grid."


def rect(x0, y0, x1, y1):
    """A clockwise rectangle, the winding TrueType outlines use, from pixel corners in units."""
    return [(x0, y0), (x0, y1), (x1, y1), (x1, y0)]


def contours_of(font, name):
    """The outline of a simple glyph as a list of contours of (x, y) points."""
    glyf = font["glyf"]
    glyph = glyf[name]
    assert glyph.numberOfContours > 0, f"{name} is not a simple outline"
    coords, ends, _ = glyph.getCoordinates(glyf)
    out, start = [], 0
    for end in ends:
        out.append([(int(x), int(y)) for x, y in coords[start : end + 1]])
        start = end + 1
    return out


def outline(contours):
    pen = TTGlyphPen(None)
    for points in contours:
        pen.moveTo(points[0])
        for point in points[1:]:
            pen.lineTo(point)
        pen.closePath()
    return pen.glyph()


def breve(left, right, wide):
    """A breve over a glyph spanning x `left`..`right`: two arms in the top row, a bowl in the row below.
    `wide` is the arm width in pixels (1 for Regular, 2 for Bold)."""
    arm = wide * PX
    return [
        rect(left, 7 * PX, left + arm, 8 * PX),
        rect(right - arm, 7 * PX, right, 8 * PX),
        rect(left + PX, 6 * PX, right - PX, 7 * PX),
    ]


def add_glyph(font, name, code, contours, advance):
    glyph = outline(contours)
    glyph.recalcBounds(font["glyf"])
    glyf = font["glyf"]
    if name not in font.getGlyphOrder():
        order = font.getGlyphOrder() + [name]
        font.setGlyphOrder(order)
    glyf.glyphs[name] = glyph
    glyf.glyphOrder = font.getGlyphOrder()
    font["hmtx"].metrics[name] = (advance, glyph.xMin)
    for table in font["cmap"].tables:
        if table.isUnicode():
            table.cmap[code] = name
    classes = font["GDEF"].table.GlyphClassDef.classDefs
    classes[name] = 1  # a base glyph, like the letters it is made from


def build(path):
    font = TTFont(path, recalcTimestamp=False)
    # The font's cedilla, snapped to the pixel grid (Regular's is 10 units off it and Ç is shifted back).
    cedilla = [[(round(x / PX) * PX, y) for x, y in c] for c in contours_of(font, "uni0327")]

    # The bar and the dot: Regular is one pixel wide, Bold two.
    bar = contours_of(font, "I")
    bar_w = bar[0][2][0] - bar[0][0][0]
    s, g = contours_of(font, "S"), contours_of(font, "G")
    g_xmin, g_xmax = min(x for c in g for x, _ in c), max(x for c in g for x, _ in c)
    i_adv = font["hmtx"]["I"][0]
    s_adv = font["hmtx"]["S"][0]
    g_adv = font["hmtx"]["G"][0]
    assert s_adv == g_adv

    dot = rect(bar[0][0][0], 6 * PX, bar[0][0][0] + bar_w, 7 * PX)
    add_glyph(font, "Idotaccent", 0x0130, bar + [dot], i_adv)
    add_glyph(font, "dotlessi", 0x0131, bar, i_adv)
    for name, code, base in (("Scedilla", 0x015E, s), ("scedilla", 0x015F, s)):
        add_glyph(font, name, code, base + cedilla, s_adv)
    for name, code in (("Gbreve", 0x011E), ("gbreve", 0x011F)):
        add_glyph(font, name, code, g + breve(g_xmin, g_xmax, bar_w // PX), g_adv)

    # Ö ö Ü ü: the base letter with a dot over its outer columns (Bold's already were there).
    left, right = g_xmin, g_xmax
    dots = [rect(left, 6 * PX, left + bar_w, 7 * PX), rect(right - bar_w, 6 * PX, right, 7 * PX)]
    for name, code, base in (
        ("Odieresis", 0xD6, "O"),
        ("odieresis", 0xF6, "o"),
        ("Udieresis", 0xDC, "U"),
        ("udieresis", 0xFC, "u"),
    ):
        add_glyph(font, name, code, contours_of(font, base) + dots, g_adv)

    # Û (hükûmet): Regular's is U plus a circumflex half a pixel off the grid; û, simple and on the grid, is the
    # same letter in an all-caps font.
    circumflexed = font["glyf"]["Ucircumflex"]
    if circumflexed.isComposite() and any(part.x % PX for part in circumflexed.components):
        add_glyph(font, "Ucircumflex", 0xDB, contours_of(font, "ucircumflex"), g_adv)

    names = font["name"]
    version = names.getName(5, 3, 1, 0x409).toUnicode()
    marker = "; Pinchora Turkish glyphs"
    if marker not in version:
        names.setName(version + marker, 5, 3, 1, 0x409)
    names.setName(NOTE, 10, 3, 1, 0x409)
    font.save(path)


def main():
    folder = Path(sys.argv[1] if len(sys.argv) > 1 else "assets/fonts")
    for style in ("Regular", "Bold"):
        build(folder / f"Silkscreen-{style}.ttf")


if __name__ == "__main__":
    main()
