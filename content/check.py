#!/usr/bin/env python3
"""Validate Pinchora content using only the Python standard library.

Run from any directory: python3 /path/to/check.py
"Roughly" allows +/- 5 percentage points for furniture rarity/availability,
capped at 40% of each target share so small tiers cannot disappear entirely.
"About half" means 40-60% sea pets. "A few" treasures means at least three.
Fish junk is identified by its common rarity and 1-5 shell value; these six
IDs are listed explicitly so a broken normal-catch price cannot become junk.
Art quality and worldwide name originality still require editorial review.
Furniture uses 2:1 isometric floor/surface sprites, frontal wall sprites and
flat rectangular rugs. A base's variants enumerate its playable colours;
the default sprite is not an extra item when variants are present.
"""

import json
import math
import re
import sys
from collections import Counter, defaultdict
from pathlib import Path


ROOT = Path(__file__).resolve().parent  # this script lives in content/
RARITIES = ("common", "uncommon", "rare", "epic", "legendary")
CURRENCIES = ("shells", "pearls")
LAYERS = ("floor", "rug", "wall", "surface")
AVAILABILITY = ("permanent", "rotating", "event", "craft", "reward")
PALETTES = {
    "coral", "mint", "navy", "cream", "plum", "gold", "sky", "moss",
    "rose", "charcoal", "sand", "lilac",
}
CATEGORY_MIN = {
    "seating": 40, "tables": 36, "beds": 20, "storage": 32,
    "lighting": 34, "kitchen": 32, "bathroom": 22, "electronics": 26,
    "rugs": 40, "wall_decor": 48, "windows_doors": 26, "plants": 36,
    "pet_items": 20, "toys_games": 24, "music": 16, "outdoor": 28,
    "seasonal": 28, "office": 16, "cafe": 16, "hobbies": 16,
    "fitness": 14, "partitions": 22, "nautical": 14,
}
FURNITURE_MIN = 650
# The current 40% recolourable expansion yields 1,861 playable items.
# Even four colours on all 140 recolourable additions would yield only 1,945.
PLAYABLE_MIN = 1800
LAYER_MIN = {"floor": 360, "wall": 110, "surface": 115, "rug": 40}
BRIEF_PREFIX = {"floor": "Isometric:", "surface": "Isometric:",
                "wall": "Front view:", "rug": "Top-down flat:"}
SHELL_BANDS = {
    "common": (40, 400), "uncommon": (300, 1200), "rare": (1000, 5000),
    "epic": (4000, 20000), "legendary": (15000, 80000),
}
FISH_BANDS = {
    "common": (5, 40), "uncommon": (30, 120), "rare": (100, 400),
    "epic": (350, 1500), "legendary": (1200, 6000),
}
BACKGROUND_BANDS = {
    "common": (500, 2000), "uncommon": (1500, 4000), "rare": (4000, 9000),
    "epic": (10000, 20000), "legendary": (30000, 60000),
}
BADGE_STATS = (
    "level", "fish_species", "house_likes", "pets", "streak", "placed", "reputation", "shells",
)
BADGE_KINDS = ("achievement", "shop")
PET_WEIGHTS = {"common": 100, "uncommon": 45, "rare": 18, "epic": 6, "legendary": 1}
PET_COUNTS = {"common": 14, "uncommon": 12, "rare": 9, "epic": 5}
ROOM_IDS = {
    "living", "bedroom", "kitchen", "bathroom", "garden", "workshop",
    "pet_room", "library", "studio", "attic",
}
STARTER_IDS = {"crumbclaw_crab", "drowsepatch_cat", "cloverpuff_bun"}
JUNK_IDS = {
    "siltstep_boot", "rattlelip_tin", "frayloop_rope", "driftspoke_wheel",
    "chipwink_cup", "snaggletoe_sock",
}
TREASURE_IDS = {"tidefold_locket", "brinebell_coin", "shellscript_key", "moonkeel_chest"}
SEASONAL_SETS = {"hushwick_hollow", "frostcrumb_gathering", "lanternplait_fete", "budwhistle_meadow"}
# Required thematic coverage includes the original and community-place sets.
THEMED_SETS = {
    "thimblefen_cottage", "pebblewake_porch", "stillfold_home", "blinkberry_arcade",
    "marginmoss_library", "sprigspool_potting", "hushwick_hollow", "frostcrumb_gathering",
    "lanternplait_fete", "budwhistle_meadow", "brinebeam_wreck", "polypocket_reef",
    "blushbutton_nook", "rivetsprout_loft", "gilded_acorn_court", "dewpeg_camp",
    "orbitcrumb_station", "fizzfern_lab", "kettlewink_cafe", "humskein_studio",
    "clickcove_den", "reedpause_retreat", "dunethread_house", "sugarquay_parlor",
    "saltstitch_landing", "bramblebob_sleepover", "foliofoam_commons", "inkkelp_office",
    "crumbcoil_bakery", "glazefin_market", "spoolpetal_atelier", "bouncebrine_club",
    "slategill_bathhouse", "sunspool_boardwalk", "rivetreef_workshop", "copperwake_cutter",
    "tilebud_courtyard", "glimmerpip_midway", "driftchime_hall",
}
FORBIDDEN = re.compile(
    r"\b(?:tatsu|animal crossing|stardew(?: valley)?|pok[eé]mon|sims|ikea|"
    r"nintendo|mario|pikachu|minecraft|lego|playstation|xbox|sonic)\b", re.I,
)
SNAKE = re.compile(r"[a-z][a-z0-9]*(?:_[a-z0-9]+)*\Z")
HEX_COLOR = re.compile(r"#[0-9a-f]{6}\Z")
WORD = re.compile(r"[a-z]+\Z")
ERRORS = []


def problem(where, message):
    ERRORS.append(f"{where}: {message}")


def integer(value):
    return type(value) is int


def number(value):
    return type(value) is int or (type(value) is float and math.isfinite(value))


def nonempty(value):
    return isinstance(value, str) and bool(value.strip())


def read_file(filename):
    def object_pairs(pairs):
        result = {}
        for key, value in pairs:
            if key in result:
                problem(filename, f"duplicate JSON key {key!r}")
            result[key] = value
        return result

    def invalid_constant(value):
        raise ValueError(f"non-JSON number {value}")

    try:
        with (ROOT / filename).open(encoding="utf-8") as handle:
            return json.load(handle, object_pairs_hook=object_pairs, parse_constant=invalid_constant)
    except (OSError, UnicodeError, ValueError) as exc:
        problem(filename, str(exc))
        return None


def entries(value, where, minimum):
    if not isinstance(value, list):
        problem(where, "must be an array")
        return []
    if len(value) < minimum:
        problem(where, f"needs at least {minimum} entries; found {len(value)}")
    result = []
    for index, item in enumerate(value):
        location = f"{where}[{index}]"
        if not isinstance(item, dict):
            problem(location, "must be an object")
        else:
            result.append((item, location))
    return result


def fields(item, where, required, optional=()):
    for key in sorted(set(required) - item.keys()):
        problem(where, f"missing field {key!r}")
    for key in sorted(item.keys() - set(required) - set(optional)):
        problem(where, f"unexpected field {key!r}")


def string(item, key, where, limit=None, words=None):
    value = item.get(key)
    if not nonempty(value):
        problem(where, f"{key} must be a nonempty string")
        return
    if value != value.strip() or any(ord(char) < 32 for char in value):
        problem(where, f"{key} must be trimmed single-line text")
    if limit is not None and len(value) > limit:
        problem(where, f"{key} is {len(value)} characters; maximum is {limit}")
    if words is not None and len(value.split()) > words:
        problem(where, f"{key} is {len(value.split())} words; maximum is {words}")
    if FORBIDDEN.search(value):
        problem(where, f"{key} contains an existing game or brand reference")


def enum(item, key, allowed, where):
    value = item.get(key)
    if not isinstance(value, str) or value not in allowed:
        problem(where, f"{key} must be one of {', '.join(allowed)}; got {value!r}")


def identity(item, where, seen, names):
    value = item.get("id")
    if not isinstance(value, str) or not SNAKE.fullmatch(value):
        problem(where, "id must be snake_case")
    elif value in seen:
        problem(where, f"duplicate id {value!r}; first used at {seen[value]}")
    else:
        seen[value] = where
    string(item, "name", where)
    name = item.get("name")
    if nonempty(name):
        normalized = name.casefold()
        if normalized in names:
            problem(where, f"duplicate name {name!r}; first used at {names[normalized]}")
        else:
            names[normalized] = where


def set_reference(item, where, set_ids):
    value = item.get("set")
    if value is not None and (not isinstance(value, str) or value not in set_ids):
        problem(where, f"unknown set {value!r}")


def price(item, where):
    enum(item, "rarity", RARITIES, where)
    enum(item, "currency", CURRENCIES, where)
    amount, rarity, currency = item.get("price"), item.get("rarity"), item.get("currency")
    if not integer(amount):
        problem(where, "price must be an integer")
        return
    if currency == "pearls":
        if rarity not in ("rare", "epic", "legendary"):
            problem(where, "pearl purchases must be rare or better")
        if not 10 <= amount <= 250:
            problem(where, "pearl price must be 10..250")
    elif currency == "shells" and isinstance(rarity, str) and rarity in SHELL_BANDS:
        low, high = SHELL_BANDS[rarity]
        if not low <= amount <= high:
            problem(where, f"{rarity} shell price must be {low}..{high}; got {amount}")


def tally(records, key):
    return Counter(item[key] for item, _ in records if isinstance(item.get(key), str))


def share(records, where):
    if records:
        proportion = sum(item.get("currency") == "pearls" for item, _ in records) / len(records)
        if not 0.05 <= proportion <= 0.12:
            problem(where, f"pearl currency share {proportion:.1%} must be 5%..12%")


def mix(records, key, expected):
    counts = tally(records, key)
    if records:
        for value, target in expected.items():
            actual = counts[value] / len(records)
            tolerance = min(.05, .4 * target)
            if abs(actual - target) > tolerance + 1e-9:
                problem("furniture.json", f"{key} {value} share {actual:.1%}; target {target:.0%} +/- {100 * tolerance:g}pp")


def check_sets(raw):
    records = entries(raw, "sets.json", 39)
    seen, names = {}, {}
    for item, where in records:
        fields(item, where, ("id", "name", "description", "mood"))
        identity(item, where, seen, names)
        string(item, "description", where, limit=100)
        string(item, "mood", where, limit=60)
    for missing in sorted(THEMED_SETS - seen.keys()):
        problem("sets.json", f"missing required themed set {missing!r}")
    return records, set(seen)


def placement(item, where):
    layer, category = item.get("layer"), item.get("category")
    footprint = item.get("footprint")
    valid = isinstance(footprint, list) and len(footprint) == 2 and all(integer(v) for v in footprint)
    if not valid:
        problem(where, "footprint must be an array of two integers")
    else:
        width, depth = footprint
        # Wall footprints are [width, height]; the others are [width, depth].
        bounds = {"floor": (1, 4, 1, 4), "rug": (1, 4, 1, 4), "wall": (1, 4, 1, 3), "surface": (1, 1, 1, 1)}
        if isinstance(layer, str) and layer in bounds:
            min_w, max_w, min_d, max_d = bounds[layer]
            if not (min_w <= width <= max_w and min_d <= depth <= max_d):
                problem(where, f"invalid {layer} footprint {footprint}")
    raw_tags = item.get("tags")
    tags = set(tag for tag in raw_tags if isinstance(tag, str)) if isinstance(raw_tags, list) else set()
    if layer in ("floor", "surface"):
        iso_width = item.get("iso_width")
        if not integer(iso_width):
            problem(where, "iso_width is required and must be an integer")
        elif valid and not 12 <= iso_width <= sum(footprint) * 32:
            problem(where, f"iso_width must be 12..{sum(footprint) * 32} for footprint {footprint}")
        if layer == "surface" and integer(iso_width) and not 16 <= iso_width <= 36:
            problem(where, "small surface sprites must be 16..36 pixels wide")
    elif "iso_width" in item:
        problem(where, "iso_width is only allowed on floor and surface items")
    if "surface" in tags:
        height = item.get("surface_height")
        if not integer(height) or not 10 <= height <= 70:
            problem(where, "surface-tagged furniture needs integer surface_height in 10..70")
    elif "surface_height" in item:
        problem(where, "surface_height is only allowed on surface-tagged furniture")
    brief = item.get("sprite_brief")
    if isinstance(brief, str) and isinstance(layer, str) and layer in BRIEF_PREFIX:
        if not brief.startswith(BRIEF_PREFIX[layer]):
            problem(where, f"{layer} sprite_brief must start with {BRIEF_PREFIX[layer]!r}")
        if layer in ("floor", "surface") and ("2:1" not in brief or "front facing lower-left" not in brief):
            problem(where, "isometric sprite_brief must specify 2:1 and front facing lower-left")
        if layer == "rug" and ("rectangle" not in brief.casefold() or "fills footprint" not in brief.casefold()):
            problem(where, "rug sprite_brief must describe a rectangle that fills footprint")
        if item.get("variants") and not re.search(r"\bcoral\b", brief, re.I):
            problem(where, "recolourable sprite_brief must retain a coral base colour")
    name = item.get("name", "")
    name = name.casefold() if isinstance(name, str) else ""
    if (layer == "floor" and valid and integer(item.get("iso_width"))
            and (category in ("beds", "tables") or re.search(
                r"\b(bed|bunk|cot|sofa|loveseat|sectional|table|desk|cabinet|counter|nightstand|dresser|wardrobe|bookcase|cupboard|armoire|island)\b", name))
            and item["iso_width"] != sum(footprint) * 32):
        problem(where, "furniture that fills its footprint needs iso_width = (w+d)*32")
    if category == "rugs" and layer != "rug":
        problem(where, "rugs must use the rug layer")
    if layer == "rug" and category != "rugs":
        problem(where, "rug layer is reserved for rugs")
    if category in ("seating", "tables", "beds") and layer != "floor":
        problem(where, f"{category} must use the floor layer")
    if category == "windows_doors" and layer != "wall":
        problem(where, "windows and doors must use the wall layer")
    if re.search(r"\b(painting|portrait|clock|window|poster|mural|tapestry|print|pennant|wreath|sconce)\b", name) and layer != "wall":
        problem(where, "mounted decor must use the wall layer")
    if re.search(r"\bwall (shelf|rack)\b", name) and layer != "wall":
        problem(where, "wall shelves and racks must use the wall layer")
    if re.search(r"\b(vase|mug|books|candle|candles|teacup)\b", name) and layer != "surface":
        problem(where, "small tabletop decor must use the surface layer")
    if "lamp" in name and not re.search(r"\b(floor|standing|tripod|arc)\b", name) and layer != "surface":
        problem(where, "small lamps must use the surface layer")
    if category == "seating" and "seat" not in tags:
        problem(where, "seating needs the seat tag")
    if re.search(r"\b(chair|sofa|stool|bench|armchair|loveseat|ottoman|lounger|pouf|hammock|beanbag)\b", name) and "seat" not in tags:
        problem(where, "sittable furniture needs the seat tag")
    # "Desk lamp" and "desk calendar" are tabletop objects, not desks.
    if (category == "tables" or re.search(r"\b(desk|counter|shelf|nightstand)$", name)) and "surface" not in tags:
        problem(where, "tables, desks, counters, shelves and nightstands need the surface tag")
    if category == "lighting" or re.search(r"\b(lamp|lantern|candle|candles|sconce|firepit|campfire)\b", name):
        if "light" not in tags:
            problem(where, "light sources need the light tag")
    if "surface" in tags and layer not in ("floor", "wall"):
        problem(where, "only floor furniture or wall shelves may carry the surface tag")


def check_furniture(raw, set_ids, shared_ids):
    records = entries(raw, "furniture.json", FURNITURE_MIN)
    names = {}
    playable = 0
    size_prices = defaultdict(lambda: defaultdict(list))
    for item, where in records:
        fields(item, where, (
            "id", "name", "category", "layer", "footprint", "rarity", "price", "currency",
            "set", "tags", "variants", "availability", "description", "sprite_brief",
        ), ("iso_width", "surface_height"))
        identity(item, where, shared_ids, names)
        string(item, "name", where, limit=28)
        name = item.get("name")
        if isinstance(name, str) and name != name.title():
            problem(where, "name must be Title Case")
        enum(item, "category", CATEGORY_MIN, where)
        enum(item, "layer", LAYERS, where)
        enum(item, "availability", AVAILABILITY, where)
        price(item, where)
        set_reference(item, where, set_ids)
        string(item, "description", where, limit=80)
        string(item, "sprite_brief", where, limit=200)
        tags = item.get("tags")
        if not isinstance(tags, list) or not 1 <= len(tags) <= 5:
            problem(where, "tags must be an array of 1..5 lowercase words")
        else:
            if any(not isinstance(tag, str) or not WORD.fullmatch(tag) for tag in tags):
                problem(where, "each tag must be a lowercase word")
            if len({tag for tag in tags if isinstance(tag, str)}) != len(tags):
                problem(where, "tags must be unique")
        variants = item.get("variants")
        if not isinstance(variants, list):
            problem(where, "variants must be an array")
        else:
            playable += max(1, len(variants))
            if len(variants) > 5:
                problem(where, "variants must contain 0..5 palettes")
            if any(not isinstance(v, str) or v not in PALETTES for v in variants):
                problem(where, "variants contain an unknown palette")
            if len({v for v in variants if isinstance(v, str)}) != len(variants):
                problem(where, "variants must be unique")
        placement(item, where)
        if item.get("category") == "seasonal":
            if item.get("availability") not in ("event", "rotating"):
                problem(where, "seasonal items must be event or rotating")
            seasonal_set = item.get("set")
            if not isinstance(seasonal_set, str) or seasonal_set not in SEASONAL_SETS:
                problem(where, "seasonal item must reference a seasonal set")
        if item.get("availability") == "event" and item.get("category") != "seasonal":
            problem(where, "event items must belong to the seasonal category")
        fp, rarity, amount = item.get("footprint"), item.get("rarity"), item.get("price")
        if (item.get("currency") == "shells" and item.get("layer") == "floor"
                and isinstance(rarity, str) and rarity in RARITIES and integer(amount)
                and isinstance(fp, list) and len(fp) == 2 and all(integer(v) and v > 0 for v in fp)):
            size_prices[rarity][fp[0] * fp[1]].append(amount)
    counts = tally(records, "category")
    for category, minimum in CATEGORY_MIN.items():
        if counts[category] < minimum:
            problem("furniture.json", f"{category}: needs {minimum}; found {counts[category]}")
    layers = tally(records, "layer")
    for layer, minimum in LAYER_MIN.items():
        if layers[layer] < minimum:
            problem("furniture.json", f"needs at least {minimum} {layer} items; found {layers[layer]}")
    if playable < PLAYABLE_MIN:
        problem("furniture.json", f"needs at least {PLAYABLE_MIN} playable items; found {playable}")
    share(records, "furniture.json")
    mix(records, "rarity", dict(zip(RARITIES, (.40, .28, .18, .10, .04))))
    mix(records, "availability", dict(zip(AVAILABILITY, (.55, .30, .05, .05, .05))))
    # Compare mean shell prices for adjacent footprint sizes within each rarity.
    # A 5% band-width allowance permits hand-priced detail and material differences.
    for rarity, by_area in size_prices.items():
        sizes = sorted(by_area)
        for small, large in zip(sizes, sizes[1:]):
            small_mean = sum(by_area[small]) / len(by_area[small])
            large_mean = sum(by_area[large]) / len(by_area[large])
            low, high = SHELL_BANDS[rarity]
            if large_mean + .05 * (high - low) < small_mean:
                problem("furniture.json", f"{rarity} mean price drops as footprint grows from {small} to {large} tiles")
    return records, playable


def check_surfaces(raw, set_ids, shared_ids):
    if not isinstance(raw, dict):
        problem("surfaces.json", "must be an object containing wallpapers and floors")
        raw = {}
    fields(raw, "surfaces.json", ("wallpapers", "floors"))
    result, ids, names = {}, {}, {}
    for kind in ("wallpapers", "floors"):
        records = entries(raw.get(kind), f"surfaces.json.{kind}", 40)
        kind_ids = set()
        for item, where in records:
            fields(item, where, ("id", "name", "rarity", "price", "currency", "set", "pattern_brief"))
            identity(item, where, shared_ids, names)
            price(item, where)
            set_reference(item, where, set_ids)
            string(item, "pattern_brief", where, words=25)
            value = item.get("id")
            if isinstance(value, str):
                kind_ids.add(value)
        # Each side of the catalog must offer an affordable plain starter finish.
        if not any(item.get("rarity") == "common" and item.get("currency") == "shells"
                   and integer(item.get("price")) and 40 <= item["price"] <= 100
                   and item.get("set") is None for item, _ in records):
            problem(f"surfaces.json.{kind}", "needs a plain basic finish costing 40..100 shells")
        result[kind], ids[kind] = records, kind_ids
    share(result["wallpapers"] + result["floors"], "surfaces.json")
    return result, ids


def check_rooms(raw, surface_ids):
    records = entries(raw, "rooms.json", 10)
    seen, names = {}, {}
    if len(records) != 10:
        problem("rooms.json", "must contain exactly ten room types")
    for item, where in records:
        fields(item, where, ("id", "name", "description", "default_wallpaper", "default_floor", "bonus_categories"))
        identity(item, where, seen, names)
        string(item, "description", where, limit=80)
        enum(item, "id", sorted(ROOM_IDS), where)
        for key, kind in (("default_wallpaper", "wallpapers"), ("default_floor", "floors")):
            value = item.get(key)
            if not isinstance(value, str) or value not in surface_ids[kind]:
                problem(where, f"{key} must reference an existing {kind} id")
        bonuses = item.get("bonus_categories")
        if not isinstance(bonuses, list) or not 2 <= len(bonuses) <= 4:
            problem(where, "bonus_categories must be an array of 2..4 categories")
        else:
            if any(not isinstance(v, str) or v not in CATEGORY_MIN for v in bonuses):
                problem(where, "unknown bonus category")
            if len({v for v in bonuses if isinstance(v, str)}) != len(bonuses):
                problem(where, "bonus_categories must be unique")
    if set(seen) != ROOM_IDS:
        problem("rooms.json", f"room ids differ from required set; missing {sorted(ROOM_IDS - seen.keys())}")
    return records


def check_pets(raw):
    records = entries(raw, "pets.json", 42)
    seen, names = {}, {}
    starters = []
    for item, where in records:
        fields(item, where, ("id", "name", "rarity", "capsule_weight", "habitat", "personality", "favorite_food", "sprite_brief"), ("starter",))
        identity(item, where, seen, names)
        enum(item, "rarity", RARITIES, where)
        enum(item, "habitat", ("land", "sea", "sky"), where)
        string(item, "personality", where, limit=40)
        string(item, "favorite_food", where, limit=24)
        string(item, "sprite_brief", where, words=30)
        rarity, weight = item.get("rarity"), item.get("capsule_weight")
        if not integer(weight):
            problem(where, "capsule_weight must be an integer")
        elif isinstance(rarity, str) and rarity in PET_WEIGHTS:
            low, high = math.ceil(PET_WEIGHTS[rarity] * .8), math.floor(PET_WEIGHTS[rarity] * 1.2)
            if not low <= weight <= high:
                problem(where, f"{rarity} capsule_weight must be {low}..{high}")
        if "starter" in item and type(item["starter"]) is not bool:
            problem(where, "starter must be a boolean")
        if item.get("starter") is True:
            starters.append(item)
            if rarity != "common":
                problem(where, "starter pets must be common")
    starter_ids = {item.get("id") for item in starters if isinstance(item.get("id"), str)}
    if len(starters) != 3 or starter_ids != STARTER_IDS:
        problem("pets.json", "exactly the common crab, cat and bunny-like starter species must have starter: true")
    counts = tally(records, "rarity")
    for rarity, expected in PET_COUNTS.items():
        if counts[rarity] != expected:
            problem("pets.json", f"needs exactly {expected} {rarity} species; found {counts[rarity]}")
    if counts["legendary"] < 2:
        problem("pets.json", "needs at least two legendary species")
    if records and not .4 <= tally(records, "habitat")["sea"] / len(records) <= .6:
        problem("pets.json", "40%..60% of species must have sea habitat")
    return records


def check_fish(raw):
    records = entries(raw, "fish.json", 64)
    seen, names, weights = {}, {}, defaultdict(list)
    for item, where in records:
        fields(item, where, ("id", "name", "rarity", "min_cm", "max_cm", "base_value", "time", "season", "weight", "flavor", "sprite_brief"))
        identity(item, where, seen, names)
        enum(item, "rarity", RARITIES, where)
        enum(item, "time", ("any", "day", "night", "dawn", "dusk"), where)
        enum(item, "season", ("any", "spring", "summer", "autumn", "winter"), where)
        string(item, "flavor", where, limit=80)
        string(item, "sprite_brief", where, words=25)
        low_size, high_size = item.get("min_cm"), item.get("max_cm")
        if not number(low_size) or not number(high_size) or not 0 < low_size < high_size:
            problem(where, "sizes must be finite numbers with 0 < min_cm < max_cm")
        rarity, value, weight, item_id = item.get("rarity"), item.get("base_value"), item.get("weight"), item.get("id")
        is_junk = isinstance(item_id, str) and item_id in JUNK_IDS
        if not integer(value):
            problem(where, "base_value must be an integer")
        elif is_junk:
            if rarity != "common" or not 1 <= value <= 5:
                problem(where, "junk must be common and worth 1..5 shells")
        elif isinstance(rarity, str) and rarity in FISH_BANDS:
            low, high = FISH_BANDS[rarity]
            if not low <= value <= high:
                problem(where, f"{rarity} base_value must be {low}..{high}")
        if not integer(weight) or weight <= 0:
            problem(where, "weight must be a positive integer")
        elif isinstance(rarity, str) and rarity in RARITIES:
            weights[rarity].append(weight)
    if not JUNK_IDS <= seen.keys():
        problem("fish.json", f"missing junk catches: {sorted(JUNK_IDS - seen.keys())}")
    if not TREASURE_IDS <= seen.keys():
        problem("fish.json", f"missing treasures: {sorted(TREASURE_IDS - seen.keys())}")
    counts = tally(records, "rarity")
    for rarity in RARITIES:
        if not counts[rarity]:
            problem("fish.json", f"needs at least one {rarity} catch")
    for commoner, rarer in zip(RARITIES, RARITIES[1:]):
        if weights[commoner] and weights[rarer] and min(weights[commoner]) <= max(weights[rarer]):
            problem("fish.json", f"every {rarer} draw weight must be smaller than every {commoner} weight")
    return records


def check_backgrounds(raw):
    records = entries(raw, "backgrounds.json", 36)
    seen, names = {}, {}
    for item, where in records:
        fields(item, where, ("id", "name", "rarity", "price", "accent", "scene_brief"))
        identity(item, where, seen, names)
        string(item, "name", where, limit=28)
        string(item, "scene_brief", where, words=45)
        enum(item, "rarity", RARITIES, where)
        accent = item.get("accent")
        if not isinstance(accent, str) or not HEX_COLOR.fullmatch(accent):
            problem(where, f"accent must be a lowercase #rrggbb colour; got {accent!r}")
        amount, rarity = item.get("price"), item.get("rarity")
        if not integer(amount) or amount <= 0:
            problem(where, "price must be a positive integer")
        elif isinstance(rarity, str) and rarity in BACKGROUND_BANDS:
            low, high = BACKGROUND_BANDS[rarity]
            if not low <= amount <= high:
                problem(where, f"{rarity} background price must be {low}..{high}; got {amount}")
    counts = tally(records, "rarity")
    for rarity in RARITIES:
        if not counts[rarity]:
            problem("backgrounds.json", f"needs at least one {rarity} background")
    return records


def check_badges(raw, fish_count):
    records = entries(raw, "badges.json", 40)
    seen, names = {}, {}
    for item, where in records:
        kind = item.get("kind")
        common = ("id", "name", "description", "kind", "icon_brief")
        if kind == "achievement":
            fields(item, where, common + ("stat", "at"))
        elif kind == "shop":
            fields(item, where, common + ("price",))
        else:
            fields(item, where, common, ("stat", "at", "price"))
        identity(item, where, seen, names)
        string(item, "name", where, limit=28)
        string(item, "description", where, limit=80)
        string(item, "icon_brief", where, limit=200)
        enum(item, "kind", BADGE_KINDS, where)
        if kind == "achievement":
            enum(item, "stat", BADGE_STATS, where)
            threshold = item.get("at")
            if not integer(threshold) or threshold <= 0:
                problem(where, "at must be a positive integer")
            elif item.get("stat") == "fish_species" and threshold > fish_count:
                problem(where, f"fish_species at {threshold} is more than the {fish_count} catches in fish.json")
        elif kind == "shop":
            amount = item.get("price")
            if not integer(amount) or amount <= 0:
                problem(where, "price must be a positive integer")
    for stat in BADGE_STATS:
        thresholds = [item["at"] for item, _ in records
                      if item.get("stat") == stat and integer(item.get("at"))]
        if not thresholds:
            problem("badges.json", f"needs at least one {stat} achievement")
        elif len(set(thresholds)) != len(thresholds):
            problem("badges.json", f"{stat} achievements must have different thresholds")
    counts = tally(records, "kind")
    for kind in BADGE_KINDS:
        if not counts[kind]:
            problem("badges.json", f"needs at least one {kind} badge")
    return records


def table(title, rows):
    print(f"\n{title}")
    width = max(len(str(label)) for label, _ in rows)
    print(f"{'Name':<{width}}  Count")
    print(f"{'-' * width}  -----")
    for label, value in rows:
        print(f"{label:<{width}}  {value:>5}")


def furniture_table(title, records, key, order):
    bases, playable = Counter(), Counter()
    for item, _ in records:
        label, variants = item.get(key), item.get("variants")
        if isinstance(label, str):
            bases[label] += 1
            if isinstance(variants, list):
                playable[label] += max(1, len(variants))
    width = max(len(label) for label in order)
    print(f"\n{title}")
    print(f"{'Name':<{width}}  Bases  Playable")
    print(f"{'-' * width}  -----  --------")
    for label in order:
        print(f"{label:<{width}}  {bases[label]:>5}  {playable[label]:>8}")


def main():
    ERRORS.clear()
    raw = {name: read_file(name + ".json") for name in (
        "sets", "furniture", "surfaces", "rooms", "pets", "fish", "backgrounds", "badges")}
    sets, set_ids = check_sets(raw["sets"])
    shared_ids = {}
    furniture, playable = check_furniture(raw["furniture"], set_ids, shared_ids)
    surfaces, surface_ids = check_surfaces(raw["surfaces"], set_ids, shared_ids)
    rooms = check_rooms(raw["rooms"], surface_ids)
    pets = check_pets(raw["pets"])
    fish = check_fish(raw["fish"])
    backgrounds = check_backgrounds(raw["backgrounds"])
    badges = check_badges(raw["badges"], len(fish))
    share(furniture + surfaces["wallpapers"] + surfaces["floors"], "all purchasable designs")
    table("Catalog", [("Furniture sets", len(sets)), ("Furniture bases", len(furniture)),
                      ("Playable furniture items", playable), ("Wallpapers", len(surfaces["wallpapers"])),
                      ("Floors", len(surfaces["floors"])), ("Rooms", len(rooms)),
                      ("Pet species", len(pets)), ("Fish / catches", len(fish)),
                      ("Backgrounds", len(backgrounds)), ("Badges", len(badges))])
    for key, order in (("category", CATEGORY_MIN), ("rarity", RARITIES), ("availability", AVAILABILITY), ("layer", LAYERS)):
        if key in ("category", "layer"):
            furniture_table(f"Furniture by {key}", furniture, key, order)
        else:
            counts = tally(furniture, key)
            table(f"Furniture by {key}", [(label, counts[label]) for label in order])
    print(f"\nPearl furniture: {tally(furniture, 'currency')['pearls']} / {len(furniture)} bases")
    if ERRORS:
        print(f"\n{len(ERRORS)} problem(s):")
        for error in ERRORS:
            print(f"- {error}")
        return 1
    print("\nOK")
    return 0


if __name__ == "__main__":
    sys.exit(main())
