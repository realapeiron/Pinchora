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
  With nothing held, Floor / Right wall / Left wall buttons put the cursor on the floor or a wall, so hung items can be
  picked up and flipped; a surface-tagged table or shelf cannot be flipped while that would leave something standing on
  it off its top.
- **Visiting** (`/house visit @user [room]`): renders their room; your pet and character appear as guests.
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

- 650 **base designs** × palette **variants** (coral, mint, navy, cream, plum, gold…) ⇒ 1,861
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

## Characters

- `/character` opens an ephemeral editor: one of the 24 **looks** (`content/looks.json`) plus four colours,
  chosen from select menus: skin (8), hair (12), top (12), bottom (12). Every change is saved at once.
- A look is one GPT-drawn sprite (`a_<look id>` in the atlas) in key colours (`assets/sprites/avatar_palette.hex`);
  the renderer swaps the skin, hair, top and bottom ramps for the chosen colours. Until a look has art, a
  placeholder figure stands in.
- Saved as one text value on the user, `"<look>.<skin>.<hair>.<top>.<bottom>"` (`users.avatar`, NULL until
  designed). Only players who designed a character are drawn: standing on a free tile in their house, in the
  room of a house they visit, and in the corner of their profile card.

## Card backgrounds and badges

- 36 **backgrounds** (`content/backgrounds.json`, five rarities, 800 to 45,000 shells) are bought once
  with shells and worn with `/customize background`. One is worn at a time (`users.background`); it sits
  under the profile, rank and wallet cards, behind a dark scrim and translucent plates for the text that is
  not inside a panel, and the cards take the background's accent colour. Art is `bg_<id>`, 240×135, drawn
  ×4; until it exists a gradient from the accent stands in.
- 40 **badges** (`content/badges.json`): 28 achievements and 12 shop badges. Achievements are never stored:
  `/customize badges` works them out from real progress (`PlayerStats`: level, fish species, house likes,
  pets, daily streak, furniture placed, reputation, shells), so a badge that stops counting leaves the
  card until it is earned again. Shop badges are bought once with shells (`user_badges`). Up to six are
  worn, in order (`users.badges`), along the foot of the profile card. Art is `b_<id>`, 24 px wide, drawn
  ×2; until it exists a coloured circle with the badge's first letter stands in.
- Every badge has a **rarity** (common to legendary; a harder threshold or a dearer price is rarer, which
  `content/check.py` and the content tests enforce). The profile card draws each badge on a small pixel
  plate in its rarity colour (`Rarity::color`, the colours furniture and fish already use).
- Badges **show by default**. `users.badges_set` is false until the player saves a choice in the editor;
  while it is false the card shows their best earned badges, rarest first (content order breaks ties), up
  to six. Saving a choice sets it (an empty one then means "no badges"); **Show my best automatically**
  clears it again. Players who already wore badges when this shipped count as having chosen.
- `/customize badges` shows the **collection card** (`BadgeCollectionCard`): the 40 badges in a grid of
  eight, earned ones in colour with the rarity word, locked ones as dark silhouettes with a progress bar
  (shop badges show their price), a worn marker `1`..`6`, a `23/40` counter, and a NEW tag on badges earned
  since the player last opened it (`users.badges_seen`, written when `/customize badges` is run). The
  panel's own buttons carry the NEW set as a hex bit mask in their custom ids, so the tags stay while the
  player works in one message and any pod can serve a click.
- **Unlock notices.** After a command that moves one of the eight stats for the acting player (`/daily`,
  the final `/fish` reel, `/pet adopt` and capsules, placing furniture in the own house, `/give`) and after
  a chat level-up, `customize::announce` reads the stats in one query and compares the earned achievement
  badges with `users.badges_notified`. Whatever is new is claimed in a row-locked transaction (so two
  commands finishing together announce it once) and posted as a "Badge unlocked: <name>" embed with the
  badge art (96 px) as thumbnail, three at most per message. Because it compares the whole set, a badge
  earned by something that never checks (a stall sale, a quest, someone's `/give`) is announced at the
  player's next checking command. Players who existed before notices have `badges_notified = NULL`: the
  first check fills it with what they already have and says nothing. New players start with `{}`.
- 30 **titles** (`content/titles.json`): 24 unlocked by one of the same eight stats (three each), five by
  owning a badge (earned or bought), and the special `house_of_the_week` that no stat grants. A row in
  `user_titles` (`user_id`, `title_id`, `granted_at`) unlocks any title; a later contest feature grants
  `house_of_the_week` that way. The player wears one (`users.title`), picked in `/customize title` (custom
  ids `ti:`), and the profile and rank cards print it under the name. The pick is checked against what
  they have unlocked when they choose it; the cards then show it without re-checking.
- **Card style** (`/customize style`, custom ids `cs:`): `users.accent` (`#rrggbb`, NULL follows the
  background, else the reef coral), `users.panel_opacity` (0 light, 1 normal, 2 solid: the veil over a
  background picture, the plates under loose text, and how see-through the panels are) and `users.frame`
  (`plain`, `rope`, `coral`, `pearl`, `gold`, `neon`). Frames are procedural SVG, 8 px thick, drawn over the
  finished card; Coral/Pearl/Gold/Neon need level 10/20/30/40, checked when picked and again when drawn.
  The profile card lifts its footer 6 px under a frame so the badges clear the band. A typed colour must
  have 3:1 contrast against the darkest panel colour (`#0b1029`). Hex input is a modal (`Cx::modal`, answered in
  `components::dispatch_modal`, which updates the message the modal came from).
- **Showcase** (`/customize showcase`, custom ids `sw:`): `users.showcase_fish`, `showcase_pet` (the pet's
  id), `showcase_item` and `showcase_house`. A pick is checked against what the player has when saved and
  again when the card is drawn (a pet that left is skipped quietly). The profile card has two row layouts:
  the classic one (no showcase, byte-for-byte the card before this existed) and a showcase one that
  shrinks the about-me panel (still three bio lines) and the stat tiles to fit a row of up to four tiles.
  The house thumbnail draws the first room without pets or characters at native size, scales it to fit
  and is cached in memory by a hash of the room's look and placements.
- A purchase is one transaction: the ownership row, the shells and the ledger entry.

## Fishing

`/fish cast` costs a bait (10 shells plain, or one piece of a bought bait), shows a cast message, and
a **Reel!** button works once the fish has bitten, a random 2–11 s later (`BITE_MIN_MS`..`BITE_MAX_MS`, spread
evenly); reacting inside the rod's window (2.5 s for the starter rod, up to 3.5 s) catches a fish, pressing before
the bite is an early miss and the bite is then never shown. The spread is well over twice the longest window, so
pressing blind at one fixed moment catches at most 39% of casts with the best rod and 28% with the starter rod (a
game test checks it stays at or under 40%). The bite message goes out at the bite time itself rather than the delay
after the cast's reply, so the time the reply took does not eat into the window. The cast keeps what it was made with in
Redis (`cast:<user>:<nonce>` = `<bite ms>:<spot>:<rod>:<bait>`), so the reel does not depend on the
player's gear at that moment.

- **Spots** (`content/fishing_gear.json`): Shore, Reef, Kelp Forest, Deep Sea and Abyss open at global
  level 1 / 5 / 12 / 20 / 30 (the first is open even at level 0). Each fish in `content/fish.json` has
  `spots`: the spots it lives in and its draw weight at each. Within one spot a rarer fish always has a
  smaller weight than a commoner one (`content/check.py` and the game tests check it); the depth shifts the
  mix, not the order. Time of day and season still filter who bites, so a pool is never empty (junk lives
  everywhere). Weights run on a scale where a common is 1000 so a legendary can be 8. Base odds work out to
  roughly 17 / 20 / 23 / 27 / 32 shells of fish per cast at the five spots with the starter rod.
- **Gear**: five rods (`window_ms`, `luck` multiplies the weight of rare and better fish, 1.0 to 1.4) and three
  baits (a rarity tier's weight ×2.5, or a spot's signature `specials` ×3); a bait replaces the plain-bait fee.
  Tables `fishing_gear`, `user_rods`, `user_baits`; no row means the shore, the starter rod and plain bait.
- **Economy**: shells an hour is worked out from the real tables as casting every 30 s (120 casts), every reel
  caught, every fish sold at its usual size and the bait paid for, averaged over the hours and months. The
  starter rod with plain bait pays about 810 / 1,150 / 1,575 / 2,030 / 2,625 an hour at the five spots; the best
  rod and bait about 1,085 / 1,530 / 2,180 / 2,760 / 3,815. `fishing::tests` fails if any rod and bait pays more
  than 1,200 / 1,700 / 2,300 / 3,000 / 4,000 at the five spots, if the Shore starter wage leaves 700–900, if a
  deeper spot or better gear stops being clearly better, if the 40,000-shell rod pays for itself in under 15 hours
  of Abyss fishing (it takes 27–41) or if legendaries pass 0.3% of Abyss catches with the best rod (about 0.2%).
  For comparison `/daily` pays 250–750 a day. Baits mostly shift the odds rather than the wage: they cost
  13 / 28 / 24 shells a cast against 10 for plain bait.
- **Sizes and selling**: a size is normal around the middle of the species' range, three deviations to either
  end of it. A catch goes to `fish_bucket` (60 fish at most) and to the fish book, which keeps the count and the
  record size for good; `/fish sell` sells all or all but the biggest of each species, for the species' worth
  scaled by size. `fish_bucket` rows are deleted and paid in one transaction, so a double click pays once. Fish of a
  species later removed from `fish.json` are not counted towards the 60 (the count takes the known ids) and
  Sell all clears them for 0 shells.
- **Book milestones**: 10 / 25 / 50 / 72 species pay shells and a shop badge or a background that already
  exists. `fish_milestones` makes each pay once; the claim runs in the catch's transaction and also catches up
  players who passed a milestone before it existed.
- **Weekly catch** (`weekly_catch`, custom ids of the board `lb:catch:<page>`): one row per player per server
  and week, the catch with the biggest size over the species' middle; junk never counts. Weeks are the Monday
  (UTC) like `weekly_xp`, and the hourly janitor forgets weeks older than the kept ones.
- Custom ids: `fr:` reel, `fx:` shop and gear menus, `fz:` bucket and book buttons.

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
| Character editor | 860×420 | done |
| Leaderboard | 900×716 | done |
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
(pearl all-clear), duels (tic-tac-toe, connect four and rock paper scissors, with optional escrowed
wagers), calm games (hangman, daily and practice wordle, trivia, `/play stats`), profile / rank / wallet / pet / level-up / welcome
cards, welcome card designer, pixel characters (`/character`), leaderboard card, card backgrounds and badges (`/customize`).

Next, in order: wallpaper/floor selection inside the editor, pet walks that find items,
seasonal events with an event currency, furniture crafting, a web editor for free placement.
