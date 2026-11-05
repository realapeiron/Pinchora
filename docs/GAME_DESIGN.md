# Pinchora game design

The game layer: houses first, then pets, shop, fishing and social play. Everything is
data-driven: items, rooms, pets and fish live in `content/*.json`, validated by tests, and
rendered from sprite atlases in `assets/sprites/`. Mechanics come from the survey in
`research/games-survey.md`; names, art and numbers are ours.

## Currencies

| Name | Kind | Sources | Sinks |
|---|---|---|---|
| **Shells** 🐚 | soft, tradable | chat, `/daily`, fishing, quests, selling, visits | shop, house tiers, capsules, pet care, fees |
| **Pearls** | premium, untradable | quest all-clear, events, (later) purchase | premium furniture, cosmetics |

Every movement is written to `credit_ledger` (pearls get their own ledger).

## House

- Every player gets a **starter house** on first use. Houses have **tiers**; upgrading adds rooms
  and enlarges existing ones, and **keeps all placed furniture** (Tatsu discards it; we don't).
- A **room** is an isometric tile grid (2:1 pixel art, classic isometric room-builder view):
  a `width`×`depth` floor of 64×32 px diamonds with two back walls, rendered ×2 with
  nearest-neighbour scaling.
  - Floor tile (x, y) has its top corner at screen (32·(x−y), 16·(x+y)); x runs down-right,
    y runs down-left. The right wall stands on the y = 0 edge, the left wall on the x = 0 edge.
  - Walls are 3 **levels** of 32 px. A wall item sits at an offset along its wall and a level.
  - Each room has a **type** (living, bedroom, kitchen, bathroom, garden, workshop, pet room,
    library, studio, attic) that sets its default wallpaper/floor and a style bonus for matching
    categories.
- **Placement is free on the grid**, not fixed slots. An item has a footprint `[w, d]` in tiles
  and a layer:
  - `rug` — flat top-down art projected onto the floor, under everything;
  - `floor` — isometric sprite facing the lower-left; occupies its footprint, no overlap;
  - `surface` — small decor standing on a free tile or on top of a `surface`-tagged floor item
    (raised by that item's `surface_height`);
  - `wall` — flat front-view art `[w, h]` (tiles × levels), sheared onto the wall.
  Draw order: walls → wall items → floor → rugs → contact shadows → everything standing on the
  floor in painter's order (an item is drawn first when it lies entirely behind another along
  x or y) → cursor overlay.
- **Flipping** mirrors the sprite, which in this projection turns a floor item a quarter
  (footprint `[w, d]` becomes `[d, w]`) and moves a wall item to the other wall.
- **Editing in Discord** (`/house edit`): an ephemeral message with the room render, a grid
  cursor, ↖↗↙↘ buttons that follow the isometric axes, an inventory select (paged), and Place / Flip / Pick up / Room buttons.
  All state rides in `custom_id`; the database is the truth.
- **Visiting** (`/house visit @user [room]`): renders their room; your pet appears as a guest.
  One small shell reward per target per day, max 10/day; the host's guestbook logs visits.
  Houses can be locked.
- **Community plaza** (later): one shared room per server; members donate furniture, admins
  arrange it.

### House tiers

| Tier | Name | Rooms | Room size (tiles) | Price |
|---|---|---|---|---|
| 1 | Tide Pool Shack | 1 | 8×8 | free |
| 2 | Beach Cottage | 2 | 9×8 | 5,000 |
| 3 | Reef House | 3 | 10×9 | 20,000 |
| 4 | Coral Villa | 4 | 11×10 | 60,000 |
| 5 | Lighthouse Loft | 5 | 12×11 | 150,000 |
| 6 | Sunken Palace | 6 | 14×12 | 400,000 |

## Furniture catalog

- ~250 **base designs** × palette **variants** (coral, mint, navy, cream, plum, gold…) ⇒ 1,000+
  items. A variant is the base sprite with a palette swap, so one drawing yields 3–5 items.
- Fields (`content/furniture.json`): `id`, `name`, `category`, `layer`, `footprint`, `rarity`,
  `price`, `currency`, `set`, `tags`, `variants`, `availability`, `description`.
- Categories: seating, tables, beds, storage, lighting, kitchen, bathroom, electronics, rugs,
  wall_decor, windows_doors, plants, pet_items, toys_games, music, outdoor, seasonal.
- Rarity drives price bands and shop weights: common, uncommon, rare, epic, legendary.
- Availability: `permanent` (always in the catalog shop), `rotating` (only via the hourly shop),
  `event`, `craft`, `reward`.

## Shop

- **Catalog shop**: permanent items, browsable by category with page images.
- **Hourly market**: every UTC hour a seeded draw of 8 items from the rotating pool, weighted by
  rarity, each with a **global stock** (common 500 … legendary 3). Stock is decremented atomically
  in Postgres; the shop card shows price, rarity frame, stock left and a countdown.
- **Player stalls** (later): list owned items at your price; 5% fee is a sink.

## Pets

- Starter pet from three commons; more from **capsules** with a published drop chart.
- Needs: hunger, fun, energy, hygiene (0–100), decaying on read from timestamps (no ticking
  jobs). Care actions with cooldowns; neglect makes the pet sleep and need a small revive fee,
  never permanent loss.
- Pets live in a house room and show up in renders and on visits.
- Species, rarity weights and personalities: `content/pets.json`.

## Fishing (first mini game)

`/fish` costs a bait, shows a cast image, and a **Reel!** button appears after a random 1–4 s;
reacting in time catches a fish weighted by rarity and the rod tier. Catches fill a fish book
(`content/fish.json`: size ranges, value, rarity, flavour); duplicates sell for shells.

## Cards

| Card | Size | Status |
|---|---|---|
| Profile | 960×540 | done |
| Rank | 934×282 | done |
| Welcome | 1100×500 | done |
| Wallet | 900×300 | done |
| Room / visit | 64·W × 64·H | done |
| Shop (hourly market) | 1000×600 | done |
| Catalog page | 1000×600 | done |
| Pet status | 860×420 | done |
| Fish catch | 800×400 | later |

## Art direction

- Isometric 2:1 pixel art, 64×32 px floor tiles, **1 px dark outline** (`#1b1d2b`), light from
  the upper-left, cozy and bright. Native resolution in the atlas; all scaling is integer and
  nearest-neighbour. Floor sprites are `iso_width` px wide (by default the footprint's
  `(w+d)·32`); wall art is `w·32 × h·32`, rugs `w·32 × d·32`, both flat.
- One shared **32-colour palette** (`assets/sprites/palette.hex`); the import pipeline quantizes
  every sprite to it so GPT-drawn sheets stay consistent. Variants swap palette ramps.
- Sprites are drawn by GPT (Codex image generation) as sheets on a flat `#00FF00` background,
  then cut, de-scaled to their native pixel grid, quantized and packed by
  `tools/asset-import`. Every sheet prompt lists exactly which items it contains and in which
  grid cell, and attaches the style reference sheet so all sheets match.

## Status (2026-11-05)

Built: houses (6 tiers, tile-grid editor in Discord, visits with rewards, likes, lock),
hourly market with shared stock, catalog, inventory, gifts, player stalls (5% fee), server
plaza (donations + manager editing), pets (adopt, capsules with published odds, care with
decaying needs, pets wander in rooms), fishing (timed Reel button, fish book), daily quests
(pearl all-clear), tic-tac-toe duels, profile / rank / wallet / pet / level-up / welcome
cards, welcome card designer.

Next, in order: pixel character creator (layered paper-doll sprites), wallpaper/floor
selection inside the editor, leaderboard image, pet walks that find items, seasonal events
with an event currency, furniture crafting, a web editor for free placement.
