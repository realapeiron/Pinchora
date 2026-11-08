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
BADGE_KINDS = ("achievement", "shop", "granted")
# Shop badges are rarer the dearer they are; achievements are rarer the harder they are.
BADGE_SHOP_BANDS = {
    "common": (1, 2500), "uncommon": (2501, 6500), "rare": (6501, 10000),
    "epic": (10001, 18000), "legendary": (18001, 10**9),
}
TITLE_KINDS = ("stat", "badge", "special")
PET_WEIGHTS = {"common": 100, "uncommon": 45, "rare": 18, "epic": 6, "legendary": 1}
PET_COUNTS = {"common": 14, "uncommon": 12, "rare": 9, "epic": 5}
ROOM_IDS = {
    "living", "bedroom", "kitchen", "bathroom", "garden", "workshop",
    "pet_room", "library", "studio", "attic",
}
STARTER_IDS = {"crumbclaw_crab", "drowsepatch_cat", "cloverpuff_bun"}
SPOT_IDS = {"shore", "reef", "kelp_forest", "deep_sea", "abyss"}
TIERS = ("junk",) + RARITIES  # what a cast rolls first; junk is its own kind
DAY_PARTS = ("dawn", "day", "dusk", "night")
SEASONS = ("spring", "summer", "autumn", "winter")
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
TRIVIA_CATEGORIES = ("ocean", "animals", "nature", "science", "geography", "food", "history", "home", "minecraft")
TRIVIA_DIFFICULTIES = ("easy", "medium", "hard")
HANGMAN_CATEGORIES = ("sea_life", "house_items", "food", "nature", "pets")
# Words a family friendly game must never accept or show.
BANNED_WORDS = {
    "bitch", "whore", "penis", "dildo", "porno", "shits", "fucks", "pussy", "cunts", "twats", "wanks",
    "boobs", "sluts", "dicks", "horny", "nazis", "raped", "rapes", "spics", "homos", "fagot", "nigga",
    "negro", "pubic",
}
WORD5 = re.compile(r"[a-z]{5}\Z")
HANGMAN_WORD = re.compile(r"[a-z]+(?: [a-z]+)?\Z")
TR_ALPHABET = "abcçdefgğhıijklmnoöprsştuüvyz"
WORD5_TR = re.compile(f"[{TR_ALPHABET}]{{5}}\\Z")
HANGMAN_WORD_TR = re.compile(f"[{TR_ALPHABET}]+(?: [{TR_ALPHABET}]+)?\\Z")
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
    seen, names = {}, {}
    lived_at = defaultdict(int)  # spot -> how many fish live there
    for item, where in records:
        fields(item, where, ("id", "name", "rarity", "min_cm", "max_cm", "base_value", "time", "season", "spots", "flavor", "sprite_brief"), ("junk",))
        identity(item, where, seen, names)
        enum(item, "rarity", RARITIES, where)
        enum(item, "time", ("any", "day", "night", "dawn", "dusk"), where)
        enum(item, "season", ("any", "spring", "summer", "autumn", "winter"), where)
        string(item, "flavor", where, limit=80)
        string(item, "sprite_brief", where, words=25)
        low_size, high_size = item.get("min_cm"), item.get("max_cm")
        if not number(low_size) or not number(high_size) or not 0 < low_size < high_size:
            problem(where, "sizes must be finite numbers with 0 < min_cm < max_cm")
        rarity, value, item_id = item.get("rarity"), item.get("base_value"), item.get("id")
        is_junk = isinstance(item_id, str) and item_id in JUNK_IDS
        if item.get("junk", False) != is_junk:
            problem(where, "only the six odds and ends carry \"junk\": true")
        if not integer(value):
            problem(where, "base_value must be an integer")
        elif is_junk:
            if rarity != "common" or not 1 <= value <= 5:
                problem(where, "junk must be common and worth 1..5 shells")
        elif isinstance(rarity, str) and rarity in FISH_BANDS:
            low, high = FISH_BANDS[rarity]
            if not low <= value <= high:
                problem(where, f"{rarity} base_value must be {low}..{high}")
        spots = item.get("spots")
        if not isinstance(spots, dict) or not spots:
            problem(where, "spots must name at least one spot with its draw weight")
            continue
        for spot, weight in spots.items():
            if spot not in SPOT_IDS:
                problem(where, f"unknown spot {spot!r}; spots are {sorted(SPOT_IDS)}")
            elif not integer(weight) or weight <= 0:
                problem(where, f"the weight at {spot} must be a positive integer")
            else:
                lived_at[spot] += 1
    if not JUNK_IDS <= seen.keys():
        problem("fish.json", f"missing junk catches: {sorted(JUNK_IDS - seen.keys())}")
    if not TREASURE_IDS <= seen.keys():
        problem("fish.json", f"missing treasures: {sorted(TREASURE_IDS - seen.keys())}")
    counts = tally(records, "rarity")
    for rarity in RARITIES:
        if not counts[rarity]:
            problem("fish.json", f"needs at least one {rarity} catch")
    for spot in sorted(SPOT_IDS):
        if not lived_at[spot]:
            problem("fish.json", f"nothing lives at {spot}")
    return records


def check_shares(spot, where, fish):
    """A spot's fixed share of casts per kind of catch, and that every kind it rolls has a fish about all year."""
    shares = spot.get("shares")
    if not isinstance(shares, dict) or not shares or set(shares) - set(TIERS) or not all(
            number(value) and value >= 0 for value in shares.values()):
        problem(where, f"shares must map some of {list(TIERS)} to non-negative numbers")
        return
    if abs(sum(shares.values()) - 100) > 1e-6:
        problem(where, f"shares must add up to 100; they add up to {sum(shares.values())}")
    living = defaultdict(list)  # kind -> (time, season) of each fish of that kind living here
    for item, _ in fish:
        if spot.get("id") in (item.get("spots") or {}):
            kind = "junk" if item.get("junk") else item.get("rarity")
            living[kind].append((item.get("time"), item.get("season")))
    for kind in TIERS:
        share = shares.get(kind, 0)
        if (share > 0) != bool(living[kind]):
            problem(where, f"the {kind} share must be above 0 exactly when {kind} fish live here")
        if share > 0:
            # Time and season are flavour inside a kind: at every part of the day in every season something
            # of the kind must be about, or the roll would be moved to another kind.
            for part in DAY_PARTS:
                for season in SEASONS:
                    if not any(time in ("any", part) and when in ("any", season) for time, when in living[kind]):
                        problem(where, f"no {kind} fish is about at {part} in {season}")
    rolled = [shares.get(rarity, 0) for rarity in RARITIES if shares.get(rarity, 0) > 0]
    if rolled != sorted(rolled, reverse=True) or len(set(rolled)) != len(rolled):
        problem(where, "each rarity must come up less often than the one below it")
    if shares.get("junk", 0) > 12:
        problem(where, "junk may be at most 12% of a spot's casts")


def check_fishing(raw, fish, backgrounds, badges):
    """fishing_gear.json: spots, rods, baits and the fish-book milestones."""
    if not isinstance(raw, dict):
        problem("fishing_gear.json", "must be an object")
        return
    fields(raw, "fishing_gear.json", ("spots", "rods", "baits", "milestones"))
    fish_spots = {item.get("id"): item.get("spots") for item, _ in fish}
    spots, levels = entries(raw.get("spots"), "fishing_gear.json spots", len(SPOT_IDS)), []
    seen_spots = {}
    for item, where in spots:
        fields(item, where, ("id", "name", "level", "blurb", "shares", "specials"))
        identity(item, where, seen_spots, {})
        check_shares(item, where, fish)
        string(item, "blurb", where, limit=80)
        level = item.get("level")
        if not integer(level) or level < 1:
            problem(where, "level must be a positive integer")
        else:
            levels.append(level)
        specials = item.get("specials")
        if not isinstance(specials, list) or len(specials) < 2:
            problem(where, "specials must list at least two fish")
        else:
            for fish_id in specials:
                if item.get("id") not in (fish_spots.get(fish_id) or {}):
                    problem(where, f"special {fish_id!r} does not live at this spot")
    if set(seen_spots) != SPOT_IDS:
        problem("fishing_gear.json", f"spots must be exactly {sorted(SPOT_IDS)}")
    if levels != sorted(set(levels)) or levels[:1] != [1]:
        problem("fishing_gear.json", "spots must open at rising levels, the first at level 1")

    rods = entries(raw.get("rods"), "fishing_gear.json rods", 5)
    seen, previous = {}, None
    for item, where in rods:
        fields(item, where, ("id", "name", "price", "window_ms", "luck", "blurb"))
        identity(item, where, seen, {})
        string(item, "blurb", where, limit=80)
        price, window, luck = item.get("price"), item.get("window_ms"), item.get("luck")
        if not integer(price) or price < 0 or not integer(window) or window < 1000 or not number(luck) or luck < 1:
            problem(where, "price >= 0, window_ms >= 1000 and luck >= 1 are needed")
        elif previous and not (previous[0] < price and previous[1] < window and previous[2] < luck):
            problem(where, "every rod must cost more, reel longer and be luckier than the one before")
        if not (isinstance(price, int) and isinstance(window, int) and number(luck)):
            continue
        previous = (price, window, luck)
    if rods and rods[0][0].get("price") != 0:
        problem("fishing_gear.json rods", "the first rod is the free starter")
    if rods and rods[0][0].get("window_ms") != 2500:
        problem("fishing_gear.json rods", "the starter rod keeps the 2500 ms window")

    baits = entries(raw.get("baits"), "fishing_gear.json baits", 3)
    seen_baits = {}
    for item, where in baits:
        fields(item, where, ("id", "name", "pack", "price", "boost", "blurb"))
        identity(item, where, seen_baits, {})
        if item.get("id") in seen:
            problem(where, "bait ids must differ from rod ids")
        string(item, "blurb", where, limit=80)
        pack, price = item.get("pack"), item.get("price")
        if not integer(pack) or pack < 1 or not integer(price) or price / max(pack, 1) <= 10:
            problem(where, "pack >= 1 and every piece must cost more than the 10-shell plain bait")
        boost = item.get("boost")
        if not isinstance(boost, dict) or boost.get("kind") not in ("rarity", "specials"):
            problem(where, "boost.kind must be rarity or specials")
        elif boost["kind"] == "rarity":
            fields(boost, where + ".boost", ("kind", "rarity", "factor"))
            enum(boost, "rarity", RARITIES, where + ".boost")
        else:
            fields(boost, where + ".boost", ("kind", "factor"))
        if isinstance(boost, dict) and not (number(boost.get("factor")) and 1 < boost.get("factor") <= 10):
            problem(where, "boost.factor must be above 1 and at most 10")

    milestones = entries(raw.get("milestones"), "fishing_gear.json milestones", 4)
    badge_kinds = {item.get("id"): item.get("kind") for item, _ in badges}
    background_ids = {item.get("id") for item, _ in backgrounds}
    counts = []
    for item, where in milestones:
        fields(item, where, ("species", "shells", "reward"))
        species, shells, reward = item.get("species"), item.get("shells"), item.get("reward")
        if not integer(species) or not 1 <= species <= len(fish):
            problem(where, f"species must be 1..{len(fish)}")
        else:
            counts.append(species)
        if not integer(shells) or shells <= 0:
            problem(where, "shells must be a positive integer")
        if not isinstance(reward, dict) or reward.get("kind") not in ("badge", "background"):
            problem(where, "reward.kind must be badge or background")
        else:
            fields(reward, where + ".reward", ("kind", "id"))
            if reward["kind"] == "badge" and badge_kinds.get(reward.get("id")) != "shop":
                problem(where, "a badge reward must be a shop badge from badges.json")
            if reward["kind"] == "background" and reward.get("id") not in background_ids:
                problem(where, "a background reward must be in backgrounds.json")
    if counts != sorted(set(counts)) or (counts and counts[-1] != len(fish)):
        problem("fishing_gear.json milestones", "counts must rise and the last one is the whole fish book")


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
    records = entries(raw, "badges.json", 41)
    seen, names = {}, {}
    for item, where in records:
        kind = item.get("kind")
        common = ("id", "name", "description", "rarity", "kind", "icon_brief")
        if kind == "achievement":
            fields(item, where, common + ("stat", "at"))
        elif kind == "shop":
            fields(item, where, common + ("price",))
        elif kind == "granted":
            fields(item, where, common)
        else:
            fields(item, where, common, ("stat", "at", "price"))
        identity(item, where, seen, names)
        string(item, "name", where, limit=28)
        string(item, "description", where, limit=80)
        string(item, "icon_brief", where, limit=200)
        enum(item, "kind", BADGE_KINDS, where)
        enum(item, "rarity", RARITIES, where)
        if kind == "achievement":
            enum(item, "stat", BADGE_STATS, where)
            threshold = item.get("at")
            if not integer(threshold) or threshold <= 0:
                problem(where, "at must be a positive integer")
            elif item.get("stat") == "fish_species" and threshold > fish_count:
                problem(where, f"fish_species at {threshold} is more than the {fish_count} catches in fish.json")
        elif kind == "shop":
            amount, rarity = item.get("price"), item.get("rarity")
            if not integer(amount) or amount <= 0:
                problem(where, "price must be a positive integer")
            elif isinstance(rarity, str) and rarity in BADGE_SHOP_BANDS:
                low, high = BADGE_SHOP_BANDS[rarity]
                if not low <= amount <= high:
                    problem(where, f"{rarity} shop badge price must be {low}..{high}; got {amount}")
    for stat in BADGE_STATS:
        thresholds = [item["at"] for item, _ in records
                      if item.get("stat") == stat and integer(item.get("at"))]
        if not thresholds:
            problem("badges.json", f"needs at least one {stat} achievement")
        elif len(set(thresholds)) != len(thresholds):
            problem("badges.json", f"{stat} achievements must have different thresholds")
        # A higher threshold is never a lower rarity.
        ladder = sorted((item["at"], item["rarity"]) for item, _ in records
                        if item.get("stat") == stat and integer(item.get("at")) and item.get("rarity") in RARITIES)
        ranks = [RARITIES.index(rarity) for _, rarity in ladder]
        if ranks != sorted(ranks):
            problem("badges.json", f"{stat} achievements must not get less rare as the threshold grows")
    counts = tally(records, "kind")
    for kind in BADGE_KINDS:
        if not counts[kind]:
            problem("badges.json", f"needs at least one {kind} badge")
    rarities = tally(records, "rarity")
    for rarity in RARITIES:
        if not rarities[rarity]:
            problem("badges.json", f"needs at least one {rarity} badge")
    return records


def check_titles(raw, badges, fish_count):
    records = entries(raw, "titles.json", 30)
    seen, names = {}, {}
    badge_ids = {item.get("id") for item, _ in badges}
    for item, where in records:
        kind = item.get("kind")
        common = ("id", "name", "description", "kind")
        if kind == "stat":
            fields(item, where, common + ("stat", "at"))
        elif kind == "badge":
            fields(item, where, common + ("badge",))
        elif kind == "special":
            fields(item, where, common)
        else:
            fields(item, where, common, ("stat", "at", "badge"))
        identity(item, where, seen, names)
        string(item, "name", where, limit=24)
        string(item, "description", where, limit=80)
        enum(item, "kind", TITLE_KINDS, where)
        if kind == "stat":
            enum(item, "stat", BADGE_STATS, where)
            threshold = item.get("at")
            if not integer(threshold) or threshold <= 0:
                problem(where, "at must be a positive integer")
            elif item.get("stat") == "fish_species" and threshold > fish_count:
                problem(where, f"fish_species at {threshold} is more than the {fish_count} catches in fish.json")
        elif kind == "badge":
            if item.get("badge") not in badge_ids:
                problem(where, f"badge {item.get('badge')!r} is not in badges.json")
    for stat in BADGE_STATS:
        thresholds = [item["at"] for item, _ in records
                      if item.get("stat") == stat and integer(item.get("at"))]
        if not thresholds:
            problem("titles.json", f"needs at least one {stat} title")
        elif len(set(thresholds)) != len(thresholds):
            problem("titles.json", f"{stat} titles must have different thresholds")
    counts = tally(records, "kind")
    for kind in TITLE_KINDS:
        if not counts[kind]:
            problem("titles.json", f"needs at least one {kind} title")
    special = next((item for item, _ in records if item.get("id") == "house_of_the_week"), None)
    if special is None:
        problem("titles.json", "missing the special title 'house_of_the_week'")
    elif special.get("kind") != "special":
        problem("titles.json", "house_of_the_week must be a special title")
    return records


ACCESSORY_SLOTS = ("head", "face", "neck", "back", "hand")
# Draw order of the slots, which is the order a saved character code lists them in.
ACCESSORY_ORDER = ("back", "neck", "face", "head", "hand")


def check_accessories(raw, badges):
    records = entries(raw, "accessories.json", 30)
    seen, names = {}, {}
    badge_ids = {item.get("id") for item, _ in badges}
    for item, where in records:
        fields(item, where, ("id", "name", "slot", "tintable", "price"), ("level", "badge"))
        identity(item, where, seen, names)
        string(item, "name", where, limit=20)
        enum(item, "slot", ACCESSORY_SLOTS, where)
        if not isinstance(item.get("tintable"), bool):
            problem(where, "tintable must be true or false")
        price = item.get("price")
        if not integer(price) or price < 0:
            problem(where, "price must be a whole number of shells, 0 for free")
        level = item.get("level")
        if level is not None and (not integer(level) or not 1 <= level <= 100):
            problem(where, "level must be 1..100")
        badge = item.get("badge")
        if badge is not None and badge not in badge_ids:
            problem(where, f"badge {badge!r} is not in badges.json")
    for slot in ACCESSORY_SLOTS:
        mine = [item for item, _ in records if item.get("slot") == slot]
        if len(mine) < 5:
            problem("accessories.json", f"needs at least five {slot} accessories")
        if not any(item.get("price") == 0 and "level" not in item and "badge" not in item for item in mine):
            problem("accessories.json", f"needs a free {slot} accessory")
    return records


def check_presets(raw, looks, accessories):
    records = entries(raw, "character_presets.json", 10)
    seen, names = {}, {}
    look_ids = {item.get("id") for item in looks if isinstance(item, dict)}
    by_id = {item.get("id"): item for item, _ in accessories}
    for item, where in records:
        fields(item, where, ("id", "name", "code"))
        identity(item, where, seen, names)
        string(item, "name", where, limit=20)
        code = item.get("code")
        if not nonempty(code):
            problem(where, "code must be a character code")
            continue
        base, *worn = code.split("+")
        parts = base.split(".")
        if len(parts) != 7 or parts[0] not in look_ids or not all(p.isdigit() and int(p) < 20 for p in parts[1:]):
            problem(where, f"code {code!r} must be <look>.<six swatch numbers below 20>")
        slots = []
        for piece in worn:
            ident, _, tint = piece.partition("*")
            accessory = by_id.get(ident)
            if accessory is None:
                problem(where, f"unknown accessory {ident!r}")
                continue
            if tint and (not accessory.get("tintable") or not tint.isdigit() or int(tint) >= 16):
                problem(where, f"{ident} cannot take the dye {tint!r}")
            slots.append(accessory.get("slot"))
        if len(set(slots)) != len(slots):
            problem(where, "two accessories in one slot")
        if slots != sorted(slots, key=ACCESSORY_ORDER.index):
            problem(where, f"accessories must be listed in the order {', '.join(ACCESSORY_ORDER)}")
    return records


def check_trivia(raw):
    records = entries(raw, "trivia.json", 300)
    ids, questions = {}, {}
    per_category, per_difficulty, slots = Counter(), Counter(), Counter()
    for item, where in records:
        fields(item, where, ("id", "category", "difficulty", "question", "options", "answer"))
        enum(item, "category", TRIVIA_CATEGORIES, where)
        enum(item, "difficulty", TRIVIA_DIFFICULTIES, where)
        string(item, "question", where, limit=110)
        identifier, category = item.get("id"), item.get("category")
        if not isinstance(identifier, str) or not SNAKE.fullmatch(identifier):
            problem(where, "id must be snake_case")
        elif identifier in ids:
            problem(where, f"duplicate id {identifier!r}; first used at {ids[identifier]}")
        else:
            ids[identifier] = where
            if isinstance(category, str) and not identifier.startswith(category + "_"):
                problem(where, f"id must start with {category + '_'!r}")
        question = item.get("question")
        if nonempty(question):
            key = " ".join(question.casefold().split())
            if key in questions:
                problem(where, f"duplicate question; first used at {questions[key]}")
            else:
                questions[key] = where
        options = item.get("options")
        if not isinstance(options, list) or len(options) != 4:
            problem(where, "options must be exactly four strings")
            continue
        if not all(nonempty(option) and len(option) <= 28 for option in options):
            problem(where, "every option must be a nonempty string of at most 28 characters")
            continue
        if len({option.casefold() for option in options}) != 4:
            problem(where, "options must all differ")
        answer = item.get("answer")
        if options.count(answer) != 1:
            problem(where, "answer must be exactly one of the options")
        else:
            slots[options.index(answer)] += 1
        if nonempty(question) and nonempty(answer) and len(answer) > 3 and answer.casefold() in question.casefold():
            problem(where, "the question gives the answer away")
        per_category[category] += 1
        per_difficulty[(category, item.get("difficulty"))] += 1
    for category in TRIVIA_CATEGORIES:
        if per_category[category] < 40:
            problem("trivia.json", f"needs at least 40 {category} questions; found {per_category[category]}")
        for difficulty in TRIVIA_DIFFICULTIES:
            if per_difficulty[(category, difficulty)] < 3:
                problem("trivia.json", f"needs at least 3 {difficulty} {category} questions")
    total = sum(slots.values())
    if total and min(slots[slot] for slot in range(4)) * 100 < total * 15:
        problem("trivia.json", f"right answers are lopsided by option position: {dict(sorted(slots.items()))}")
    return records


def check_hangman(raw):
    records = entries(raw, "hangman.json", 400)
    seen, per_category = {}, Counter()
    for item, where in records:
        fields(item, where, ("category", "word"))
        enum(item, "category", HANGMAN_CATEGORIES, where)
        word = item.get("word")
        if not isinstance(word, str) or not HANGMAN_WORD.fullmatch(word):
            problem(where, "word must be lowercase a-z with at most one space")
            continue
        letters = len(word.replace(" ", ""))
        if not 4 <= letters <= 16:
            problem(where, f"word has {letters} letters; allowed 4..16")
        if word in seen:
            problem(where, f"duplicate word {word!r}; first used at {seen[word]}")
        seen[word] = where
        if word in BANNED_WORDS:
            problem(where, f"{word!r} is not family friendly")
        per_category[item.get("category")] += 1
    for category in HANGMAN_CATEGORIES:
        if per_category[category] < 80:
            problem("hangman.json", f"needs at least 80 {category} words; found {per_category[category]}")
    return records


def check_words(raw):
    if not isinstance(raw, dict):
        problem("words.json", "must be an object with answers and allowed")
        return [], []
    fields(raw, "words.json", ("answers", "allowed"))
    lists = {}
    for key, minimum in (("answers", 1000), ("allowed", 3000)):
        words = raw.get(key)
        if not isinstance(words, list):
            problem("words.json", f"{key} must be an array")
            lists[key] = []
            continue
        if len(words) < minimum:
            problem("words.json", f"{key} needs at least {minimum} words; found {len(words)}")
        bad = [w for w in words if not isinstance(w, str) or not WORD5.fullmatch(w)]
        if bad:
            problem("words.json", f"{key} must be five lowercase letters a-z; first bad: {bad[0]!r}")
        good = [w for w in words if isinstance(w, str)]
        if good != sorted(set(good)):
            problem("words.json", f"{key} must be sorted with no repeats")
        for word in good:
            if word in BANNED_WORDS:
                problem("words.json", f"{word!r} in {key} is not family friendly")
        lists[key] = good
    missing = sorted(set(lists["answers"]) - set(lists["allowed"]))
    if missing:
        problem("words.json", f"{len(missing)} answers are not allowed guesses, e.g. {missing[0]!r}")
    return lists["answers"], lists["allowed"]


# --- Turkish overlay (content/i18n/tr) -------------------------------------------------------------------------
TR_DIR = ROOT / "i18n" / "tr"
# overlay file -> (English source file, key inside it or None, id field)
TR_SOURCES = {
    "furniture": ("furniture.json", None, "description"),
    "sets": ("sets.json", None, "description"),
    "surfaces": ("surfaces.json", ("wallpapers", "floors"), None),
    "rooms": ("rooms.json", None, "description"),
    "pets": ("pets.json", None, "personality"),
    "fish": ("fish.json", None, "flavor"),
    "looks": ("looks.json", None, None),
    "accessories": ("accessories.json", None, None),
    "presets": ("character_presets.json", None, None),
    "backgrounds": ("backgrounds.json", None, None),
    "badges": ("badges.json", None, "description"),
    "titles": ("titles.json", None, "description"),
    "contest_themes": ("contest_themes.json", None, "description"),
    "critters": ("critters.json", None, "flavor"),
    "finds": ("finds.json", None, "flavor"),
    "pet_trips": ("pet_trips.json", None, "blurb"),
    "fishing_gear": ("fishing_gear.json", ("spots", "rods", "baits"), "blurb"),
}
# every id in terms.json: the words the code asks for by key
TR_TERM_KEYS = (
    {f"rarity.{r}" for r in RARITIES}
    | {f"category.{c}" for c in CATEGORY_MIN}
    | {f"slot.{s}" for s in ("head", "face", "neck", "back", "hand")}
    | {f"habitat.{h}" for h in ("shore", "tidepool", "kelp", "reef", "deep", "washed_up")}
    | {"kind.critter", "kind.find"}
    | {f"stat.{s}" for s in BADGE_STATS} | {f"stat.{s}.noun" for s in BADGE_STATS}
    | {f"time.{t}" for t in ("any",) + DAY_PARTS} | {f"season.{t}" for t in ("any",) + SEASONS}
    | {f"tier.{n}" for n in range(1, 7)}
    | {f"quest.{q}" for q in ("chat", "fish", "visit", "care", "place", "buy", "like", "daily")}
    | {f"currency.{c}" for c in ("shells", "pearls", "fragments")}
)
TR_QUEST_NUMBERS = {"chat": "10", "fish": "3", "visit": "2", "care": "3", "place": "2"}
TR_NAME_SLACK = 2  # characters on top of English's longest name + 30% for the suffix a Turkish noun takes
TR_FIRST_WORDS = {"Revolving": "Dönen"}  # real English adjectives that open a furniture name: translated, not coined


def tr_lower(text):
    """Lower-case with Turkish dotted/dotless I rules."""
    return text.replace("I", "\u0131").replace("\u0130", "i").lower()


def tr_words(text):
    return " " + " ".join(re.findall(r"[^\W\d_]+", text.lower())) + " "


def tr_has_term(words, term):
    return any(f" {term}{plural} " in words for plural in ("", "s", "es"))


def read_tr(filename):
    try:
        with (TR_DIR / filename).open(encoding="utf-8") as handle:
            return json.load(handle)
    except (OSError, ValueError) as exc:
        problem(f"i18n/tr/{filename}", str(exc))
        return None


def tr_sources():
    """overlay file -> list of (id, English name, English description or None)."""
    result = {}
    for name, (source, keys, desc_key) in TR_SOURCES.items():
        raw = read_file(source)
        if raw is None:
            continue
        lists = [raw] if keys is None else [raw.get(key, []) for key in keys]
        result[name] = [(item["id"], item["name"], item.get(desc_key) if desc_key else None)
                        for items in lists for item in items if isinstance(item, dict) and "id" in item]
    return result


def check_turkish():
    """Every content id has a natural-length, unique, glossary-respecting Turkish name (and description)."""
    sources = tr_sources()
    glossary = read_tr("_glossary.json") or {"terms": [], "exempt": {}}
    nouns = read_tr("_furniture_nouns.json") or {}
    exempt = glossary.get("exempt", {})
    all_ids, checks = {}, 0
    for name, english in sources.items():
        overlay = read_tr(name + ".json")
        if not isinstance(overlay, dict):
            continue
        where = f"i18n/tr/{name}.json"
        en_ids = {item[0] for item in english}
        for missing in sorted(en_ids - overlay.keys()):
            problem(where, f"missing Turkish text for {missing!r}")
        for extra in sorted(overlay.keys() - en_ids):
            problem(where, f"Turkish text for unknown id {extra!r}")
        limit = max((len(item[1]) for item in english), default=0) * 13 // 10 + TR_NAME_SLACK
        seen = {}
        for ident, en_name, en_desc in english:
            text = overlay.get(ident)
            if not isinstance(text, dict):
                continue
            here = f"{where}:{ident}"
            if ident in all_ids:
                problem(here, f"id also in {all_ids[ident]}")
            all_ids[ident] = name
            fields(text, here, ("name",), ("description", "food", "stories"))
            for key in ("name", "description", "food"):
                value = text.get(key)
                if value is None:
                    continue
                if not nonempty(value) or value != value.strip() or "\n" in value:
                    problem(here, f"{key} must be trimmed single-line text")
                elif len(value) > 100:
                    problem(here, f"{key} is {len(value)} characters; Discord labels stop at 100")
            tr_name = text.get("name", "")
            if nonempty(tr_name):
                if len(tr_name) > limit:
                    problem(here, f"name {tr_name!r} is {len(tr_name)} characters; limit {limit} for {name}")
                folded = tr_lower(tr_name)
                if folded in seen:
                    problem(here, f"duplicate name {tr_name!r}; first used by {seen[folded]}")
                seen[folded] = ident
            if (text.get("description") is None) != (en_desc is None):
                problem(here, "description must exist exactly when the English has one")
            elif en_desc is not None and len(text["description"]) > len(en_desc) * 14 // 10 + 12:
                problem(here, "description is far longer than the English one")
            if ident not in exempt:
                for term in glossary["terms"]:
                    pairs = [(en_name, tr_name)]
                    if term.get("scope") == "all" and en_desc is not None:
                        pairs.append((en_desc, text.get("description") or ""))
                    for english_text, turkish in pairs:
                        if tr_has_term(tr_words(english_text), term["en"]):
                            checks += 1
                            if not any(stem in tr_lower(turkish) for stem in term["match"]):
                                problem(here, f"English has {term['en']!r} but {turkish!r} lacks {term['match']}")
        if name == "pets":
            for ident, text in overlay.items():
                if not nonempty(text.get("food")):
                    problem(f"{where}:{ident}", "a pet needs a food")
        if name == "pet_trips":
            trips = {item["id"]: item for item in read_file("pet_trips.json") or []}
            for ident, text in overlay.items():
                stories = text.get("stories", [])
                if ident in trips and len(stories) != len(trips[ident]["stories"]):
                    problem(f"{where}:{ident}", "needs one Turkish story per English story")
                for story in stories:
                    if story.count("{pet}") != 1 or len(story) > 100:
                        problem(f"{where}:{ident}", f"story must hold {{pet}} once and fit 100 characters: {story!r}")
        if name == "furniture":
            for item in read_file("furniture.json") or []:
                first, _, tail = item["name"].partition(" ")
                first = TR_FIRST_WORDS.get(first, first)
                noun = nouns.get(tail)
                if isinstance(noun, dict):
                    noun = noun.get(item["category"], noun.get("*"))
                text = overlay.get(item["id"])
                if noun is None:
                    problem("i18n/tr/_furniture_nouns.json", f"no Turkish noun for {tail!r}")
                elif isinstance(text, dict) and text.get("name") != f"{first} {noun}":
                    problem(f"{where}:{item['id']}", f"name must be {first} {noun}")
    terms = read_tr("terms.json") or {}
    if set(terms) != TR_TERM_KEYS:
        problem("i18n/tr/terms.json", f"keys differ: missing {sorted(TR_TERM_KEYS - set(terms))[:5]}, "
                                      f"extra {sorted(set(terms) - TR_TERM_KEYS)[:5]}")
    for key, line in terms.items():
        if not isinstance(line, dict) or not nonempty(line.get("name")) or len(line["name"]) > 100:
            problem("i18n/tr/terms.json", f"{key} needs a name of at most 100 characters")
    for quest, number in TR_QUEST_NUMBERS.items():
        if number not in str(terms.get(f"quest.{quest}", {}).get("name", "")):
            problem("i18n/tr/terms.json", f"quest.{quest} must keep the number {number}")
    colors = read_tr("colors.json") or {}
    for palette in sorted(PALETTES):
        if palette not in colors:
            problem("i18n/tr/colors.json", f"no Turkish word for the variant colour {palette!r}")
    for key, line in colors.items():
        if not isinstance(line, dict) or not nonempty(line.get("name")):
            problem("i18n/tr/colors.json", f"{key} needs a name")
    return checks


def check_trivia_tr(raw, english):
    records = entries(raw, "trivia_tr.json", len(english) + 100)
    ids, questions, slots = {}, {}, Counter()
    per_category = Counter()
    for item, where in records:
        fields(item, where, ("id", "category", "difficulty", "question", "options", "answer"))
        enum(item, "category", TRIVIA_CATEGORIES, where)
        enum(item, "difficulty", TRIVIA_DIFFICULTIES, where)
        string(item, "question", where, limit=110)
        identifier, category = item.get("id"), item.get("category")
        if not isinstance(identifier, str) or not SNAKE.fullmatch(identifier):
            problem(where, "id must be snake_case")
        elif identifier in ids:
            problem(where, f"duplicate id {identifier!r}; first used at {ids[identifier]}")
        else:
            ids[identifier] = where
            if isinstance(category, str) and not identifier.startswith(category + "_tr_"):
                problem(where, f"id must start with {category + '_tr_'!r}")
        question = item.get("question")
        if nonempty(question):
            key = " ".join(question.casefold().split())
            if key in questions:
                problem(where, f"duplicate question; first used at {questions[key]}")
            questions[key] = where
        options = item.get("options")
        if not isinstance(options, list) or len(options) != 4:
            problem(where, "options must be exactly four strings")
            continue
        if not all(nonempty(option) and len(option) <= 28 for option in options):
            problem(where, "every option must be a nonempty string of at most 28 characters")
            continue
        if len({option.casefold() for option in options}) != 4:
            problem(where, "options must all differ")
        answer = item.get("answer")
        if options.count(answer) != 1:
            problem(where, "answer must be exactly one of the options")
        else:
            slots[options.index(answer)] += 1
        per_category[category] += 1
    for item, _ in english:
        twin = item.get("id", "").replace("_", "_tr_", 1)
        if twin not in ids:
            problem("trivia_tr.json", f"{item.get('id')!r} has no Turkish twin {twin!r}")
    for category in TRIVIA_CATEGORIES:
        required = 40 if category == "minecraft" else 55
        if per_category[category] < required:
            problem("trivia_tr.json", f"needs at least {required} {category} questions; found {per_category[category]}")
    total = sum(slots.values())
    if total and min(slots[slot] for slot in range(4)) * 100 < total * 15:
        problem("trivia_tr.json", f"right answers are lopsided by option position: {dict(sorted(slots.items()))}")
    return records


def check_hangman_tr(raw):
    records = entries(raw, "hangman_tr.json", 500)
    seen, per_category = {}, Counter()
    for item, where in records:
        fields(item, where, ("category", "word"))
        enum(item, "category", HANGMAN_CATEGORIES, where)
        word = item.get("word")
        if not isinstance(word, str) or not HANGMAN_WORD_TR.fullmatch(word):
            problem(where, "word must be lowercase Turkish letters with at most one space")
            continue
        if not 4 <= len(word.replace(" ", "")) <= 16:
            problem(where, f"word has {len(word.replace(' ', ''))} letters; allowed 4..16")
        if word in seen:
            problem(where, f"duplicate word {word!r}; first used at {seen[word]}")
        seen[word] = where
        per_category[item.get("category")] += 1
    for category in HANGMAN_CATEGORIES:
        if per_category[category] < 100:
            problem("hangman_tr.json", f"needs at least 100 {category} words; found {per_category[category]}")
    return records


def check_words_tr(raw):
    if not isinstance(raw, dict):
        problem("words_tr.json", "must be an object with answers and allowed")
        return [], []
    fields(raw, "words_tr.json", ("answers", "allowed"))
    lists = {}
    for key, minimum in (("answers", 800), ("allowed", 4000)):
        words = raw.get(key)
        if not isinstance(words, list):
            problem("words_tr.json", f"{key} must be an array")
            lists[key] = []
            continue
        if len(words) < minimum:
            problem("words_tr.json", f"{key} needs at least {minimum} words; found {len(words)}")
        bad = [w for w in words if not isinstance(w, str) or not WORD5_TR.fullmatch(w)]
        if bad:
            problem("words_tr.json", f"{key} must be five lowercase Turkish letters; first bad: {bad[0]!r}")
        good = [w for w in words if isinstance(w, str)]
        if good != sorted(set(good)):
            problem("words_tr.json", f"{key} must be sorted with no repeats")
        lists[key] = good
    missing = sorted(set(lists["answers"]) - set(lists["allowed"]))
    if missing:
        problem("words_tr.json", f"{len(missing)} answers are not allowed guesses, e.g. {missing[0]!r}")
    return lists["answers"], lists["allowed"]


def check_party_tr():
    english, turkish = read_file("party.json"), read_file("party_tr.json")
    if not isinstance(english, dict) or not isinstance(turkish, dict):
        problem("party_tr.json", "must be an object like party.json")
        return 0
    for key in ("wyr", "truths", "dares"):
        was, now = english.get(key, []), turkish.get(key, [])
        count = Counter(item.get("category") for item in was)
        if Counter(item.get("category") for item in now) != count:
            problem("party_tr.json", f"{key} must have the same number per category as party.json")
        seen = set()
        for item in now:
            where = f"party_tr.json {item.get('id')}"
            if not isinstance(item.get("id"), str) or f"_{item.get('category')}_tr_" not in item["id"]:
                problem(where, "id must look like wyr_<category>_tr_NN")
            if item.get("id") in seen:
                problem(where, "duplicate id")
            seen.add(item.get("id"))
            for field in ("a", "b") if key == "wyr" else ("text",):
                text = item.get(field)
                if not nonempty(text) or text != text.strip():
                    problem(where, f"{field} must be trimmed text")
                elif key == "wyr" and not text.endswith(("mak", "mek")):
                    problem(where, f"{field} must be an infinitive phrase ending in -mak/-mek")
    return len(turkish.get("wyr", [])) + len(turkish.get("truths", [])) + len(turkish.get("dares", []))


def check_bottles_tr():
    english, turkish = read_file("bottles.json"), read_file("bottles_tr.json")
    if not isinstance(english, list) or not isinstance(turkish, list):
        problem("bottles_tr.json", "must be an array like bottles.json")
        return 0
    if [b.get("id") for b in english] != [b.get("id") for b in turkish]:
        problem("bottles_tr.json", "must have the same ids in the same order as bottles.json")
    for item in turkish:
        if not nonempty(item.get("text")) or len(item["text"]) > 95:
            problem(f"bottles_tr.json {item.get('id')}", "text must be 1..95 characters")
    return len(turkish)


def check_games():
    english_trivia = entries(read_file("trivia.json"), "trivia.json", 0)
    trivia = check_trivia(read_file("trivia.json"))
    hangman = check_hangman(read_file("hangman.json"))
    answers, allowed = check_words(read_file("words.json"))
    trivia_tr = check_trivia_tr(read_file("trivia_tr.json"), english_trivia)
    hangman_tr = check_hangman_tr(read_file("hangman_tr.json"))
    answers_tr, allowed_tr = check_words_tr(read_file("words_tr.json"))
    party_tr = check_party_tr()
    bottles_tr = check_bottles_tr()
    return (trivia, hangman, answers, allowed), (trivia_tr, hangman_tr, answers_tr, allowed_tr, party_tr, bottles_tr)


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
        "sets", "furniture", "surfaces", "rooms", "pets", "fish", "looks", "backgrounds", "badges", "titles",
        "fishing_gear", "accessories", "character_presets")}
    sets, set_ids = check_sets(raw["sets"])
    shared_ids = {}
    furniture, playable = check_furniture(raw["furniture"], set_ids, shared_ids)
    surfaces, surface_ids = check_surfaces(raw["surfaces"], set_ids, shared_ids)
    rooms = check_rooms(raw["rooms"], surface_ids)
    pets = check_pets(raw["pets"])
    fish = check_fish(raw["fish"])
    backgrounds = check_backgrounds(raw["backgrounds"])
    badges = check_badges(raw["badges"], len(fish))
    titles = check_titles(raw["titles"], badges, len(fish))
    check_fishing(raw["fishing_gear"], fish, backgrounds, badges)
    accessories = check_accessories(raw["accessories"], badges)
    character_presets = check_presets(raw["character_presets"], raw["looks"], accessories)
    (trivia, hangman, answers, allowed), turkish = check_games()
    glossary_checks = check_turkish()
    share(furniture + surfaces["wallpapers"] + surfaces["floors"], "all purchasable designs")
    table("Catalog", [("Furniture sets", len(sets)), ("Furniture bases", len(furniture)),
                      ("Playable furniture items", playable), ("Wallpapers", len(surfaces["wallpapers"])),
                      ("Floors", len(surfaces["floors"])), ("Rooms", len(rooms)),
                      ("Pet species", len(pets)), ("Fish / catches", len(fish)),
                      ("Backgrounds", len(backgrounds)), ("Badges", len(badges)),
                      ("Titles", len(titles)), ("Looks", len(raw["looks"])),
                      ("Accessories", len(accessories)), ("Character presets", len(character_presets))])
    table("Games", [("Trivia questions", len(trivia)), ("Hangman words", len(hangman)),
                    ("Wordle answers", len(answers)), ("Wordle allowed guesses", len(allowed))])
    table("Turkish games", [("Trivia questions", len(turkish[0])), ("Hangman words", len(turkish[1])),
                            ("Wordle answers", len(turkish[2])), ("Wordle allowed guesses", len(turkish[3])),
                            ("Would you rather, truths and dares", turkish[4]), ("Bottle messages", turkish[5])])
    for key, order in (("category", CATEGORY_MIN), ("rarity", RARITIES), ("availability", AVAILABILITY), ("layer", LAYERS)):
        if key in ("category", "layer"):
            furniture_table(f"Furniture by {key}", furniture, key, order)
        else:
            counts = tally(furniture, key)
            table(f"Furniture by {key}", [(label, counts[label]) for label in order])
    print(f"\nPearl furniture: {tally(furniture, 'currency')['pearls']} / {len(furniture)} bases")
    print(f"Turkish overlay: {glossary_checks} glossary checks")
    if ERRORS:
        print(f"\n{len(ERRORS)} problem(s):")
        for error in ERRORS:
            print(f"- {error}")
        return 1
    print("\nOK")
    return 0


if __name__ == "__main__":
    sys.exit(main())
