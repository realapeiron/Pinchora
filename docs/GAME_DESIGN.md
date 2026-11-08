# Pinchora game design

The game layer: houses first, then pets, shop, fishing and social play. Everything is
data-driven: items, rooms, pets and fish live in `content/*.json`, validated by tests, and
rendered from sprite atlases in `assets/sprites/`. Mechanics come from the survey in
`research/games-survey.md`; names, art and numbers are ours.

## Currencies

| Name | Kind | Sources | Sinks |
|---|---|---|---|
| **Shells** 🐚 | soft, tradable | chat, `/daily`, fishing, quests, selling, visits | shop, house tiers, capsules, pet care, fees |
| **Pearls** | premium, untradable | quest all-clear, events, capped paid diving, (later) purchase | premium furniture, cosmetics, house crafting |

Shell movements are written to `credit_ledger`. Pearl balance changes are transactional;
the existing schema has no separate pearl ledger. D7 entry costs and craft shell costs use
`reef_dive`, `reef_seed` and `reef_craft` ledger reasons.

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
- Pets live in a house room and show up in renders and on visits (not while they are on a trip).
- Levels 1-30: every care action, walk and trip earns XP; higher levels find more on walks and trips.
- Sleep: hunger, fun and hygiene all at zero for 24 hours puts a pet to sleep (no XP, no care, no trips) until a
  wake-up (one free per player per UTC day, else 75 shells). It never dies or leaves.
- Field trips (1, 4 or 8 hours, `content/pet_trips.json`): the needs freeze while away; the claim pays shells,
  XP, a story line and sometimes a free-to-earn (`reward`) furniture piece. A stored end time makes it restart-proof.
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
  level 1 / 5 / 12 / 20 / 30 (the first is open even at level 0). Each spot has fixed `shares`, in percent,
  of the six kinds a cast rolls first: junk, common, uncommon, rare, epic, legendary (they add up to 100; a
  kind is above 0 exactly where fish of that kind live). The hour and the season never move them. Only after
  the kind is rolled does the cast pick a species among the fish of that kind that are about now (`time` and
  `season` in `content/fish.json` are flavour inside a kind), by `spots`: the spots a fish lives in and its
  draw weight there, relative to the other fish of its kind at that spot. Every kind a spot rolls has a fish
  about at every hour of every month (`fishing::tests` and `content/check.py` check it), so the roll never
  moves; if a kind ever has none about it settles for the nearest kind that has one (the lower on a tie) and
  never for junk, which is only ever rolled by its own share. Rarer kinds are rarer and the rare, epic and
  legendary shares never fall going deeper. Shares in percent, shore to abyss: junk 10 / 10 / 9 / 8 / 7,
  uncommon 7.5 / 7 / 8 / 8 / 8, rare 1.5 / 1.8 / 2.2 / 2.8 / 3.4, epic 0 / 0.1 / 0.2 / 0.3 / 0.45, legendary
  0 / 0 / 0 / 0.1 / 0.16, the rest common.
- **Gear**: five rods (`window_ms`, `luck` multiplies the shares of rare, epic and legendary, 1.0 to 1.4) and
  three baits (a rarity's share ×2.5, or a spot's signature `specials` ×3 among the fish of their own kind); the
  shares are then shared out again. A bait replaces the plain-bait fee. Tables `fishing_gear`, `user_rods`,
  `user_baits`; no row means the shore, the starter rod and plain bait.
- **Economy**: shells an hour is worked out from the real tables as casting every 30 s (120 casts), every reel
  caught, every fish sold at its usual size and the bait paid for, for every hour of every month (288 cells), not
  on average. The starter rod with plain bait pays 755 / 1,128 / 1,552 / 2,013 / 2,565 an hour on average at the
  five spots (the weakest hour 638 / 1,079 / 1,469 / 1,977 / 2,523, the best 901 / 1,180 / 1,591 / 2,090 / 2,670);
  the best rod and bait about 1,000 / 1,491 / 2,026 / 2,766 / 3,464. `fishing::tests` fails if any hour of any
  month pays more than 1,200 / 1,700 / 2,300 / 3,000 / 4,000 at the five spots with any rod and bait (the worst
  cell is 1,135 / 1,549 / 2,107 / 2,868 / 3,600), if the Shore starter mean leaves 700-900, if any hour pays under
  half its spot's mean or loses money with a rod and plain bait, if a deeper spot's worst hour stops beating the
  shallower spot's best, if better gear stops being clearly better, if the 40,000-shell rod pays for itself in
  under 15 hours of Abyss fishing or if legendaries pass 0.3% of casts at any hour at any spot with the best rod
  (about 0.22% at the Abyss, 0.14% at the Deep Sea). For comparison `/daily` pays 250-750 a day. Baits mostly
  shift the odds rather than the wage: they cost 13 / 28 / 24 shells a cast against 10 for plain bait. The Silver
  Lure and Tide Crumbs are for hunting rare and signature fish, not for income: with any rod they lose shells at
  the Shore, Reef and Kelp Forest (the starter rod with a Silver Lure at the Shore is about -857 an hour), and
  Tide Crumbs halves the Abyss wage. The tackle shop's card and the bait menus say so in plain words.
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
| Fish catch | 800×400 | done |
| Reef loop / aquarium / season | 960×640 | done (D7) |

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
(pearl all-clear), duels (tic-tac-toe, connect four, rock paper scissors, Sea Fleet, Reef Chess and Pearl
Checkers, with optional escrowed wagers; in all of them but Share or Grab a player who runs out of time forfeits the pot),
calm games (hangman, daily and practice wordle, trivia, Pixel Picross, Tide 2048, `/play stats`), profile / rank / wallet / pet / level-up / welcome
cards, welcome card designer, pixel characters (`/character`), leaderboard card, card backgrounds and badges (`/customize`).

Next, in order: wallpaper/floor selection inside the editor, pet walks that find items,
seasonal events with an event currency and a web editor for free placement.
Furniture crafting, seasonal fishing and the aquarium/garden loop are described in D7 below.

## D7 — fish–house–pet long loop (migration 0078)

All actions live under existing member roots: `/fish dock|contest|season|dive`,
`/house aquarium|garden|craft`, `/pet streak`, `/partner streak`. No new root, module,
admin setting or configurable currency mint. Diving reuses the fishing component namespace
as `fz:loop:<owner>:<nonce>:<depth>:push|surface`; ownership, deadline and expected state
are checked in the database. New commands answer privately; the aquarium also accompanies
ordinary authorized house views/visits. Web house overviews show aquarium fish and, only
for the owner, garden status. Editing and tending use the Discord commands; existing web
house placement supports the crafted items without a separate editor or new mutation API.

- **Mallowfin's Dock** rotates at 00:00/08:00/16:00 UTC. It requests two common, non-junk,
  shore fish available all day in the current season. One delivery per rotation, three per
  UTC day globally; consumes the two smallest matching bucket fish and gives two seaweed.
  No shells or pearls are paid, and aquarium specimens are never consumed.
- **Daily Ripple Cup** automatically records the best non-junk catch in each server per
  UTC day, inside the catch transaction. Score = size / species' typical size (same metric
  as weekly fishing). At midnight the closed day stops changing; `/fish contest yesterday:true`
  shows its final standings. **Tidebook Season** sums daily bests; ties use earliest catch,
  then user ID. Northern meteorological seasons begin March/June/September/December 1 UTC,
  matching the existing seasonal fish, odds and art. `/fish season previous:true` displays
  a finished season card, and `claim:true` grants one bound `reefbubble_lamp__sky~bound`
  after at least three fishing days in that server. Claims are global per player/season,
  so fishing in extra servers cannot multiply the reward. No daily cash prizes. Hidden
  profiles/boards are excluded from standings. Results stay for 366 days; the previous
  season remains claimable throughout the current season.
- **Ripple Nook** is a six-slot, named aquarium. Moving a caught specimen removes the
  bucket row; returning it creates one bucket row with the exact species and size. A full
  60-fish bucket refuses returns. Names are 1–24 characters without control characters.
  The house visit's existing privacy/lock/social checks guard its accompanying aquarium;
  the web snapshot uses the same house authorization before exposing fish. Bucket browsing
  has five pages of 12. No fish are cloned and the permanent fish book is unchanged.
- **Brinebud Garden** has one bed. Planting costs **40 shells**; water once at least
  **1 hour** after planting; harvest at least **8 hours** after planting, only if watered,
  for **4 seaweed**. Plants never wither. There is no offline multiplier or catch-up harvest.
  Earliest sustained output: **12 seaweed/day for 120 shells/day**. All garden timestamps
  persist in PostgreSQL; neither a process restart nor a replay accelerates the clock.
- **Lantern Descent** costs **300 shells**, one entry per UTC day globally. Three descents
  are possible in a **90-second** session. Each success adds one held pearl; failure loses
  all held pearls and ends the dive. Loss probabilities are **15%, 35%, 55%**, published
  before entry. Surface banks 0–3 pearls; timeouts lose held pearls and never refund the fee.
  Pure fixed-depth strategies return **0.85 / 1.105 / 0.745875 pearls per paid entry** at
  depths 1/2/3 respectively; depth 2 is optimal in expectation. Maximum issuance is
  **3 pearls/day (1,095 per 365 days)** for at least **109,500 shells of entry sinks/year**
  when entering daily; optimal expected issuance is **403.325/year**, about **271.49 shells
  per expected pearl**. Existing fishing payouts and rarity shares are unchanged.
- **Little Care Calendar** advances on a successful pet-care write, once per player/UTC
  day across pets, and gives **1 seaweed**. Opening a status card pays nothing. **Two Tides
  Together** advances only when both current partners check in on the same UTC day and
  grants **1 seaweed each**. Both streaks allow one missed day; longer gaps reset the count,
  never possessions. Rewards do not grow with the streak. Re-pairing cannot repeat a day's
  reward. Seaweed is untradeable, unsellable and capped at **999**; overflow care rewards
  stop at the cap, and paid planting/harvesting/deliveries refuse when there is no room.

**Tideweave Workshop sinks** (one bound item per craft, atomic inventory/balance/bucket
transaction; the smallest common non-junk bucket specimens are used):

| Recipe / catalog item | Seaweed | Common fish | Pearls | Shells |
|---|---:|---:|---:|---:|
| Sprigstep / `sprigstep_planter_box__mint` | 8 | 1 | 1 | 250 |
| Reefbubble / `reefbubble_lamp__sky` | 12 | 2 | 2 | 400 |
| Blushpetal / `blushpetal_planter__lilac` | 16 | 3 | 3 | 600 |

These are personal crafting alternatives to existing catalog furniture. The bound suffix
is already understood by inventory, placement, undo and both renderers, and prevents
selling, gifting, plaza donation or placement in a partner's house. Ingredients have
opportunity costs (bait, fish that cannot be sold, seed fees, time); crafting additionally
removes pearls and shells. Combined maximum seaweed supply is **20/player/day** (12 garden,
6 dock, 1 care, 1 partner), with no cash conversion. Finite inventory ingredients and the
crafting pearl costs provide sinks for every new repeatable source. Season keepsakes have
no tradable or sellable value. The existing economy and fishing-budget tests still apply.

**Durability/privacy:** seven tables: `reef_loop`, `reef_aquarium`, `reef_loop_daily`,
`reef_requests`, `reef_catches`, `reef_season_claims`, `reef_partner_streaks`. Player advisory
locks precede row locks; both partner locks are acquired in sorted order before rechecking
the relationship. Failed recipes roll back every cost. Daily limits remain as same-day
privacy tombstones; other player data is deleted. The hourly janitor uses `SKIP LOCKED`,
retains daily counters for 35 days, requests for two days and results for 366 days.

**Art:** 960×640 pixel cards reuse the fish and furniture atlas. Mallowfin, seaweed and dive
pearls are original procedural pixel motifs in `pinchora-render/src/cards/reef_loop.rs`;
no external art, new atlas sheets or owner-supplied placeholder assets are required. Both
EN/TR cards include translated instructions, costs, results and deadlines in the embed.

## D8 player economy v2

All additions use global shells, the existing inventory and `credit_ledger`. No purchases
with real money, premium track, paid chest, stake or gambling mechanic is introduced.
Server administrators control **server points only**, never global shell supply or balances.

- **Reef jobs:** Kelp Keeper (sort, bundle, stock), Coral Courier (map, pack, deliver),
  Beacon Tender (clean, align, light). A shift is three ordered, owner-bound button tasks
  completed within five minutes. At most three paid shifts per UTC day, one hour apart.
  Promotions every ten completed shifts raise pay from 100 to 125 / 150 / 175 / 200 shells.
  Progress follows the player when changing jobs. The repeatable wage ceiling is 600/day.
- **Dock auctions:** one unplaced, unbound furniture item, 1–10,000,000 shells, 1–72 hours,
  at most eight active listings. Inventory is removed into escrow immediately. Each bid
  reserves shells; an outbid player is refunded in the same transaction. The increment is
  at least `max(1, floor(current / 20))`. A bid in the last two minutes sets the end to two
  minutes from the bid, capped at 30 extra minutes beyond the original deadline. Sellers
  cannot bid on themselves or cancel after a bid. Unbid cancellations/expiries return the
  item. Settlement credits the seller and deducts a **5% fee, rounded up** (minimum one).
  Gateways attempt a sweep every 30 seconds; a transaction-scoped Postgres lock admits
  one runner, releases on restart, and conditional state changes pay once. Worker receipt
  IDs are reserved from player requests.
- **Swap table:** immutable offers of shells plus up to eight distinct furniture types per
  side, 1–10 of each. The creator reserves their own offer; the partner reserves theirs
  only on confirming. **Both explicitly confirm** the same offer, showing both player IDs,
  quantities, shell amounts, expiry and confirmation state. Final delivery is one atomic
  transaction. Reservations remove items/shells from spendable balances, so shop, stall,
  house placement and gifts cannot spend them. Either participant may cancel before the
  completed swap; after 24 hours the sweeper refunds reserved assets. Changing an offer
  requires cancellation and a fresh trade with fresh confirmations. Privacy deletion
  refunds the surviving participant, including auction bidders/sellers, under sorted locks.
- **Community pool:** permanent shell donations, no withdrawals, payouts or reward farming.
  Admins choose a 100–1,000,000,000-shell goal; progress persists when the goal changes and
  reaching the goal marks a community milestone. Donations are an explicit shell sink.
- **Provisions:** free tending starts a 12-hour supply cycle. Collect ten shells per complete
  hour, capped at **120 per cycle** (240/day at continuous care); excess offline time earns
  nothing. Collect the full accrued cycle before tending again. No compound growth.
- **Earned treasure:** every fifth completed shift grants one untradeable chest, storage cap
  ten. Opening is free. Published odds are **70%: 25 shells; 25%: 50; 5%: 100** (expected 35).
  No source sells chests, and no entry fee is charged. The sustainable supply is at most
  0.6 chest/day (21 expected shells/day); opening previously stored chests may pay more.
- **Prestige:** after 40 shifts, opt in to spend **10,000 shells**, reset job rank/progress,
  and gain one permanent prestige level (cap 100). Inventory, season and daily limits stay.
  Prestige grants no income multiplier. It is a voluntary long-term sink.
- **Free season:** each UTC calendar month, ten tiers at 50 progress each; shifts earn ten
  progress, each tier pays **100 shells**, total **1,000/month**. Claim tiers in order, once
  each. A new month resets progress and expires unclaimed rewards. All tiers are free.
- **Server store:** optional minimum server level and minimum membership tenure, plus a
  store enabled switch. Tenure is conservatively counted from the membership record's
  timestamp when migration 0083 lands, or first record thereafter. Existing role grant and
  hierarchy checks remain. Admin reset/reduction previews count and total server points;
  the hash of exact member balances must still match on confirmation. Keep 0% to reset,
  or 1–100% to reduce inflation; shells and XP are never reset by this tool. A synchronous
  operation is capped at 10,000 members; larger servers need an operator-managed batch.

D8's steady repeatable sources total at most 840 shells/day plus earned chest openings
and the monthly 1,000-shell track. Existing furniture, house, care and crafting sinks
remain, supplemented by auctions, donations and prestige. All shell movements, including
escrow, refunds and fees, produce ledger entries. Transfers/refunds do not count as newly
earned activity. Guarded numeric arithmetic rejects overflow without partial effects.

Migration **0083 only**: `economy_players`, `economy_limits`, `economy_requests`,
`economy_auctions`, `economy_trades`, `economy_settings`, `economy_ledger`, and
`members.economy_joined_at`. Every shell/item movement writes a balanced wallet and
escrow/source/sink ledger pair in the same transaction; privacy removes both entries.
The existing dashboard access constraint gains the `economy` area for the new admin routes.
Admin settings and point changes commit with their audit records; unchanged settings do
not advance the revision or create audit noise. Reset preview hashes include the guild.
The existing privacy registry deletes normal records and retains daily/month-end claim
limits through their period to prevent delete/recreate claims. Request receipts bind the
request ID to the exact submitted action. The `sl:eco:*` namespace extends the existing
stall component dispatcher; old `sl:<price>` menus are unchanged. Job/trade controls use
Economy module gating and their canonical command policies.
