# Pinchora game layer: survey of popular Discord bots

Date: 2026-11-05. Purpose: record mechanics (not text or art) of popular Discord bots to design the Pinchora game layer
(Rust bot, server-rendered image cards). All names, art and wording in Pinchora must be original.

Confidence tags used below:
- [V] read directly on the cited page this session.
- [S] taken from a search-result summary of a page that could not be opened directly (redirects, 402/403).
- [R] recalled background knowledge, not re-verified here. Treat as a lead, confirm in-bot before copying a number.

Many official pages blocked automated fetches (fandom 402, top.gg 403, support.tatsu.gg redirect), so numbers marked
[S] or [R] should be checked by playing the bot. Source keys (S1..S31) are listed at the end.

## 0. Bot identity checks

- "Modora": exists. A newer all-in-one bot (site modora.xyz, top.gg id 1416779195605975150) [S23]. Its 35 listed slash
  commands are leveling/achievements/badges, moderation, tickets, giveaways, events, forms, role menus, sticky notes,
  temp voice, server stats, invite rewards, AI chat, and welcome/announce banners. No economy, pets or games listed [V].
  So it is a MEE6/Dyno-class server tool, not a game bot. If the owner meant a game bot, the closest candidates are Mudae
  (character collection) or Tatsu (house/pets).
- "Nightshift": no Discord bot by that name was found on top.gg or discordbotlist [S31]. Nearest name matches: Night Bot
  (Turkish multipurpose bot), Nightbot (stream-chat bot, not a Discord game bot [R]). Nearest feature matches for a
  "games + profile cards" bot: Moneta (jobs, quests, marketplace, lottery), Bociek (economy, leveling, profile cards),
  Narox (animated profile cards), KitsuBot (animal collecting + battles), Shards (server currency and shop layer).
- "Wabbit": WabbitBot, a free customizable economy bot (custom shop/items/chests/stocks/jobs/recipes/role rewards/XP,
  12 casino games with adjustable house edge, about 4.9k servers) [S22]. No rank card found. Confirm this is the intended bot.

## 1. Tatsu (priority 1: house game)

Overview: global leveling bot turned game platform. Free core; "Supporters" are the paid tier. More than 1M communities by
2020 [S10]. Surfaces: slash and `t!` commands, a web dashboard/store at tatsu.gg, and a Discord Activity (Home Studio).

### 1.1 House: data model and rules
- Houses are bought (6 house types as of 2020 launch, each with its own layout and design). Houses are not tradable or
  refundable. Moving house is done from the edit menu and discards the current decoration [S2 summary][S10].
- Placement model is slot-based, not free-form: every house and every room has a unique fixed layout of floor and wall
  slots. Slot types are `wall`, `1x1` (small floor item) and `2x1` (long floor item such as a desk). The fan wiki says three
  of each type per room, nine furniture items per room (possibly outdated) [S4 summary].
- Rooms: default house has 1 room; larger houses (e.g. penthouse) have up to 4. Each room is decorated separately, and
  pets can move between rooms [S2][S4 summary].
- Customization: furniture, wallpaper, flooring, weather, and (seasonal) house background. Seasonal backgrounds change by
  themselves during events (Halloween) and can be switched manually [S8].
- Furniture: 1,000+ pieces [S1][S5]; 300+ planned at the 2020 V2 launch [S10]. Sources: global store (credits), token-only
  items (premium currency, permanent), event store (event currency, limited), furniture boxes, rewards from pet walks
  (credits, tokens, cosmetics or furniture), monthly vote item (60 votes) [S3 summary][S8][S9]. Most furniture is tradable.
- Content cadence: monthly themed patch (e.g. Patch 119 "Botanical Boutique", 40+ new items incl. permanent and pet
  cosmetics) [S9]; older sites say store updates every two weeks [S5][S6]. Every theme splits into permanent credit items,
  permanent token items and time-limited event items [S8].
- Visiting: show a chosen room of another player's house with your own pet standing in it (`t!tg visit`, `t!house @user`,
  now `/pet interact visit user`) [S4 summary][S7]. Houses are visible to everyone in the server [S1].
- Visit loop as a social driver: during the Halloween event "trick or treat" needs a house with an active pet, you visit a
  friend to take one basket, once per target per day, max 10 visits/day; you can only use buttons on people you visited [S8].
- Daycare (separate pet-display system): storeys (floors) each with 1-6 rooms (6-room floors supporter-only), floors cost
  more as you buy more, pets placed in a daycare need no upkeep; up to 8 pets shown per place; unlimited decorated floors
  [S1][S11 summary].
- Home hygiene: a dirty house can make the pet sick [S10].

### 1.2 Decorating UX
- Originally text commands: `t!house edit` opens a menu with three options (customize a room, change weather, move house).
  For a customize, you first pick a room (if several), then a slot type (wall / 1x1 / 2x1), then an owned item [S2][S4].
- Newer: "Tatsugotchi Home Studio" is a Discord Activity (embedded web app) usable in any server. Features seen in patch notes:
  zoom tool, WASD camera movement, scrollable storey list, one-click storey reset, compressed assets; a bug note says
  clicking ground furniture in front of wall furniture selected the wall piece (so layered click picking exists). Rename,
  add and delete storeys still need `t!daycare edit` [V][S8].
- The image side: `t!house` returns a bird's-eye view of the pet in the house [S4 summary]. Pets are animated GIFs on the
  marketing page; furniture shown as close-up renders [S1]. Style: cute cartoon, not strict pixel art [S1].
- Takeaway for Pinchora: a Rust bot cannot run an Activity; use image + select menus + buttons for P0, add a web editor
  (link or Activity) later.

### 1.3 Pets (Tatsugotchi)
- 100+ pets (cats, dogs, birds), 50+ cosmetics, nursery view, status cards with level and needs [S1][S10].
- Obtained from capsules (random wrapped pet; `unwrap` to open; `/pet misc chart` shows drop rates), starter kit via mail
  (pick a common pet), or shop capsule [S3 summary][S7].
- Care actions: feed, play, train (XP), walk (may find items, costs 5 fullness, raises fatigue, ~2 h rest when fatigue maxed),
  clean mess, carer, nickname, edit (cosmetics) [S3 summary][S7]. Prestige resets level at level 50 [S7].
- Neglect: pets can run away; retrieval costs credits scaled by rarity; higher level pets take longer to run away [S3][S10].
- Field trips / heists: send pets out for rewards (basket per pet during the event, fee otherwise) [S8].

### 1.4 Economy, shop, social
- Currencies: global credits (chat, daily), server points (per server, renameable by admins), tokens (premium currency from
  all-daily-quests, supporter dailies, store), event currency (e.g. Candy Corn) [S5][S6][S8][S12].
- Global store: 2,000+ items (pets, houses, furniture, backgrounds, badges) [S6]. Server store: admin-made roles,
  accolades (custom badges) and custom items, bought with server points; web item creator [S6][S10]. Hybrid global+server
  inventory; mail inbox to claim rewards (`/mail view|claim|mark|delete`) [S7][S10].
- Faucets: chat credits, `/daily` (streak, supporters get more), quests (all-clear gives tokens, reset 00:00 UTC), votes
  with buffs and monthly rewards, walks, events [S7][S8][S12].
- Sinks: store, houses, capsules, pet retrieval, fishing cast (10 credits), slots bet (max 50k in 2020 wiki), cosmetics
  [S10][S12]. Older wiki: credits convert to a tradable form with a 20% fee (not re-verified) [S12].
- Social: reputation (1 per 24 h, cannot give to self, display only, cannot be spent), cookies (give + stats + upgrade shop),
  dice, slots, fishy (cast/sell/redeem rare fish, rare-fish collection) [S7][S12][S14].
- Leaderboards: server, global, points, by period [S7].

### 1.5 Cards and monetization
- Cards: rank card (server rank, next role, accolades), profile card (level, rep, badges, bg; `profile progress` shows
  messages to next level), wallet card (global + server currency, server icon), level-up card [S6][S10][S13].
- Customization: store backgrounds (credits or tokens), equippable badges/accolades; custom upload is supporter-only [S13].
- Supporter perks seen: custom backgrounds, extra daily rewards (2 baskets vs 1, higher caps), 6-room daycare floors,
  supporter furniture/badges/cosmetic each patch [S8][S9][S11].
- Not found: community places / shared public spaces. Closest is the server-wide visibility of houses and daycares.

## 2. Other bots in the owner's list

### MEE6 (leveling + server tools) [S18]
- XP per message about 15-25 with a 60 s cooldown [R]; levels plugin, `/rank` card, web leaderboard, role rewards.
- Free users can only recolor text on the card. Premium (server-wide, around $11.95/mo per a competitor blog) unlocks
  premium plugins and a server-default card design; Pro (personal) gives +50% XP and personal card customization on any server [V].
- Takeaway: monetize cosmetics and convenience; the personal-vs-server split is a clean model.

### Arcane (leveling-first) [S19]
- Message XP, level-up messages, role rewards, leaderboard (web viewable), rank card; admin cooldowns and multipliers.
- Premium (around $7/server/mo, competitor-sourced): voice XP, higher role-reward limits, custom card backgrounds.

### Carl-bot (utility) [S20]
- Signature: reaction roles (up to 250 per message), deep logging, automod, tags (custom commands), starboard. Leveling is
  premium-only (around $7.99/mo). Not a game bot.

### Dyno (moderation + utility) [S21]
- Moderation/automod, autoban, custom commands, autoroles, welcome messages, announcements, starboard, joinable ranks,
  reminders; modular web dashboard. Not a game bot.

### Wabbit (see section 0) [S22]
- Server-owner-configurable economy: custom shop and items, chests, stocks, jobs, recipes, role rewards, XP, 12 casino games
  with house-edge setting. Free. Pattern worth copying: admin-configurable items and recipes.

### Modora (see section 0) [S23]
- Leveling with achievements/badges, invite rewards, giveaways, welcome and announcement banners, temp voice, server stats.

### OwO bot (priority 4: pets/zoo/collection) [S15][S16]
- Loop: `owo hunt` catches random animals into a zoo (each with rarity, sell value, zoo points); an animal becomes a battle
  pet once used. Teams of up to 3 (no duplicates), 2 saved teams; stats HP/WP 500, STR/MAG 100 at base, scale with level;
  pet level capped by profile level; weapons cost WP [V].
- Rarity: 5-tier ladder in one guide (Common..Legendary, 100+ animals, sell values from about 500 to 100k); other sources
  name extra tiers (Mythic, Fabled, Patreon, bot animals) and a gem system for hunting buffs [V low reliability][S16].
- Idle: Huntbot autohunt, 5 upgradable traits paid with essence (Efficiency, Duration, Cost, Gain, Experience); max about
  240 animals/hour and 24 h runs; 10 cowoncy per animal (5 at max Cost) [V].
- Currency "cowoncy". Faucets: daily with streak (24 h, doubled for Patreon), vote (twice/day), saying "owo" (small, about
  every 10 s), selling animals/weapons, quests, daily checklist, level-ups (5,000 x new level plus lootboxes and crates)
  [V]. Sinks: shop, huntbot runs, gambling. No direct animal/weapon trading, only `owo give` for cowoncy [V].
- Lootboxes and weapon crates from level-ups, votes, checklist. `sacrifice` animal -> essence; `dismantle` weapon [V].
- Social: marry/divorce with couples leaderboard (no gameplay effect), cookie, pray/curse luck, battle other players [V].
- Gambling: slots, coinflip, lottery; admins can disable gambling per channel [V].
- Profile XP about 10-15 per message with 1 min cooldown [V].
- Monetization: Patreon $1-$125/mo; doubled daily, extra team slots, exclusive pets, custom pet design at top tier;
  tickets resellable for in-game currency [V].
- Visuals: mostly text and emoji embeds [R]. Lesson: depth of loop and cooldown design, not art, drives retention.

### ProBot (priority 4: welcome-card designer) [S17]
- Welcome image: dashboard toggle "send an image when a user joins"; image composes member avatar, name/welcome text and a
  custom background; layout and text can be repositioned [S17 summary].
- Profile: global card with level, rep and credits; `/profile`, `rep` (once per 24 h), `credits` (check/transfer; transfer
  tax around 5% [R]), `#daily` (24 h) [S17 summary][R]; `rank` card and `top` (text or voice activity), `title` sets a
  profile title, `setxp/setlevel` admin tools [V].
- Dashboard modules: welcome/goodbye, self roles, embed creator, levels; many logs and moderation tools [V].
- Premium: protection features, tier 2 custom bot [V]. Customers also buy credits-based backgrounds and roles [S17 summary].

### Dank Memer (economy leader) [S24]
- Mostly slash commands. Earn commands: work, hunt, crime, fish, dig, beg, pm (post meme) [S][R]; `/daily` with streak;
  bank deposit; `/bankrob` group heist (join, split or steal) [V][S].
- Hundreds of tradable items, global marketplace, prestige, badges, achievements, quests, skill upgrades [V].
- Fishing is a full sub-game: 115 creatures, baits and tools, fish guide book, leaderboards, customizable fish tanks
  (a house-like display for fish), idle fishing mode [V].
- Drops: time-limited sales with limited stock; some "supporters only" [S24 summary]. Omega shop: one discounted skin per
  day [S24 summary]. This is the closest real example to Pinchora's limited rotating stock.
- Premium (subscription): extra coins on `/daily` that scale with streak, weekly reward cycle, supporter role, premium
  drops [S24 summary]. Cosmetic "skins" for the bot UI exist [S24].

### Mudae (character gacha) [S25]
- Roll characters into a channel; any user can claim with a reaction, one claim per 3-hour window, limited rolls per hour;
  claimed characters form a harem. Wishlist (`$wish`) raises roll chance for wished characters, `$wishprotect`-style
  protection blocks others claiming your wished character, sniping others' rolls is part of the game [V][S].
- Kakera: currency from claims/burns and reactions, spent mainly on Kakera Badges (Bronze, Silver, Gold, Ruby, Emerald,
  Sapphire) that give rolls, wish chance, discounts; Silver level 4 pays 200 kakera when someone else claims your wish [V].
- `$vote` resets rolls for everyone on the server; premium lets an admin raise server roll count (`$setrolls`) [V].
- Visual: one embed per roll (character portrait plus kakera value and claim rank) [R].

### Karuta (card trading) [S26]
- `k!drop` drops 3 cards on a 30 min cooldown (item upgrades raise count and lower cooldown); activity also triggers auto
  drops. Grab by reacting; first reaction within about 1 s wins, ties resolved randomly (anti-latency, anti-bot); users
  always win contests on their own manual drops [V].
- 160,000+ characters; unique print numbers and visible card quality; every card and item is tradable [V]. `k!burn`
  destroys a card for resources; `k!gemshop`; gems fund a server chest that unlocks server perks [V].
- Visual: composite image of the dropped cards side by side with print number and quality [R]. Verification step for new
  users (anti-alt) [V].

### Pokétwo (spawn + collect) [S27]
- Spawns driven by chat: about one per 24 non-bot messages (about 36 s solo, about 24 s with 2+ users). First player to
  type the correct name with `catch` wins; failed guesses do not despawn; hint command reveals letters [V].
- Incense (shard shop) spawns every 20 s for 1 h; shiny chance 1/4096 rolled after catch; Shiny Charm +20% for a week
  (150 shards); redirect sets spawn channels [V].
- Currencies: pokecoins (PC), shards (200 PC each, can be bought with money, untradable), redeems (bought with shards, pick
  a Pokemon with random IV) [V]. Faucets: catches (35 PC first, 350 at 10th, 3,500 at 100th...), release 2 PC, quests,
  region quests (badges at 20/50/100/200/500) [V]. Sinks: shop (XP boosters, evolution stones, mints, megas), shards,
  market/auction fees [V].
- Player-run market and auctions, trading, favorite/mass-favorite tools [V].

### EPIC RPG (cooldown RPG loop) [S28]
- Text RPG: hunt, adventure, chop, fish, pickup, mine for items and XP; craft swords/armor; dungeons (20, two co-op boss
  attempts per day); areas 1-15; later unlocks (enchant, time travel, pets, horse, guild) gated by dungeon boss wins [S28].
- Lesson: a long unlock ladder keeps one command set interesting; time travel is a prestige reset.

### Virtual Fisher (single-theme idle/loop) [S29]
- `/fish`, `/sell`, `/shop`, `/bait`, `/rod`, boats, `/quests`, `/daily`, coinflip, global and server tops, settings;
  donor perks plus gifting of donation perks [V]. Pattern: one verb, deep gear upgrades.

### UnbelievaBoat (admin-run economy) [S30]
- Per-server economy, 100+ slash commands: work and crime income, role income, taxes, customisable shop (admins sell
  roles and items), blackjack, roulette, animal racing, chicken fighting, leaderboards, web dashboard [V].
- Pattern: the server owner is the economy designer; the bot is the engine.

## 3. Cross-bot patterns worth keeping

- Slot-based house placement keeps the UI simple (menus, no collision math); free grids need an editor [Tatsu].
- Visit loops need a reason: visit-gated rewards with daily caps work (Tatsu trick or treat, max 10/day).
- Furniture supply is a tri-split: permanent credit items, permanent premium-currency items, time-limited event items.
- Limited stock is already normal (Dank Memer drops with stock, Tatsu event stores); no surveyed bot was verified to
  rotate hourly. Hourly rotation is a Pinchora differentiator, not a copy.
- Every game loop is a cooldown: claim windows (Mudae 3 h), drops (Karuta 30 min), daily 24 h, spawn per N messages
  (Pokétwo). Cooldown plus reminder is the engagement lever.
- Anti-abuse used in the wild: random tie-break on contested grabs (Karuta), one claim per window (Mudae), daily caps on
  visits (Tatsu), no direct trading of collectibles in OwO (to curb RMT), verification (Karuta).
- Premium split: cosmetics, convenience, extra rewards; pure power is rare but exists (MEE6 Pro +50% XP, OwO Patreon
  teams). Premium currency purchasable by money with fixed soft rate (Pokétwo 200 PC per shard).

## 4. Pinchora feature catalog

Priority: P0 first release, P1 soon after, P2 later. Mechanics only; names and art are ours. "Coins" = soft currency,
"Gems" = premium currency, "Stars" = server points; placeholders.

### 4.1 House (owner priority 1)
| ID | Mechanic | Pri |
|---|---|---|
| H1 | Every user gets a free starter house on first command; house state stored as rows, rendered to a pixel-art PNG | P0 |
| H2 | Typed slot placement per room (wall, floor 1x1, floor 2x1, ceiling) with fixed per-house layouts; no collision code | P0 |
| H3 | Room types (living, bedroom, kitchen, bath, garden, workshop, pet room, attic) each with its own slot layout and a style tag | P0 |
| H4 | Data-driven furniture catalog (TOML/JSON + sprite atlas): id, category, slot type, rarity, set, price, currency, tags; ship ~150, scale to 1000+ | P0 |
| H5 | Furniture categories: seating, tables, beds, storage, lighting, appliances, rugs and floor decor, wall art, plants, windows and doors, pet items, themed sets | P0 |
| H6 | `/house edit`: ephemeral message with live render plus selects (room, slot, owned item) and buttons (place, remove, clear room, undo) | P0 |
| H7 | Wallpaper and flooring swap per room, from owned items | P0 |
| H8 | House upgrade tiers (e.g. 6 tiers): more rooms and slots per tier, bought with coins; layout kept on upgrade (improve on Tatsu, which discards) | P0 |
| H9 | `/house visit @user [room]`: renders their room (with your pet if owned); public, no consent needed unless they lock it | P0 |
| H10 | Pet standing or wandering in the rendered room, moving between rooms | P1 |
| H11 | Window view setting: weather, time of day, season (auto-changes with the real calendar and events) | P1 |
| H12 | Visit rewards: one small reward per target per day, cap 10/day; host gets a guest-log entry | P1 |
| H13 | Guestbook and likes on a house; "most liked" weekly board | P1 |
| H14 | Furniture trading and gifting (houses stay untradable); trade tax as a sink | P1 |
| H15 | Free-grid placement with rotation in a web editor (link or Activity), same data model | P1 |
| H16 | Community places: each server gets a shared plaza of N slots; members donate furniture, admins or voters arrange; server points unlock upgrades | P1 |
| H17 | Room style bonus: matching set pieces give a small cosmetic score; weekly decoration contest voted by members | P2 |
| H18 | Saved layouts and share codes; "copy this room" for visitors who own the items | P2 |
| H19 | Public place directory (browse and teleport-visit popular houses/plazas) | P2 |

### 4.2 Social (priority 2)
| ID | Mechanic | Pri |
|---|---|---|
| SO1 | Reputation: one point per 24 h to another user, display only | P0 |
| SO2 | Gift item or coins to a friend via mail inbox (async claim) | P0 |
| SO3 | Friend list; friends get cheaper trades and appear in a visit shortlist | P1 |
| SO4 | Social action commands (hug, pat, cheer) rendered as small pixel animation or GIF with counters | P1 |
| SO5 | Marriage or partner link: shared room slot and couples leaderboard, no power effect (OwO pattern) | P1 |
| SO6 | Server goals: members donate to a communal chest that unlocks server-wide perks (Karuta chest pattern) | P2 |
| SO7 | Co-op group event (heist or raid) that needs 2-5 players and splits rewards | P2 |
| SO8 | Mentor/newcomer bonus for helping a new player | P2 |

### 4.3 Economy (priority 4)
| ID | Mechanic | Pri |
|---|---|---|
| E1 | Global wallet: coins (soft), gems (premium, untradable) | P0 |
| E2 | Faucets: chat coins with 60 s cooldown and daily cap, `/daily` with streak, daily quests, walks, mini games | P0 |
| E3 | Sinks: shop, upgrades, capsules, pet food and care, cosmetics, trade tax, rename fees | P0 |
| E4 | Hourly rotating limited-stock store: each hour a seeded pick of N items, finite global stock per item, countdown shown on the shop image | P0 |
| E5 | Mail inbox for rewards, gifts and event payouts; claim all | P0 |
| E6 | User-run shops: any user lists items at own price (and fee) in a stall; buyers use `/shop view @user`; seller fee 5% as sink | P1 |
| E7 | Per-server points ("Stars") with admin-set earn rate, name and emoji, spent in the server store | P1 |
| E8 | Server store: admins create role, badge or custom items for Stars (Tatsu, UnbelievaBoat pattern) | P1 |
| E9 | Direct trade (two-sided confirm, item locks) | P1 |
| E10 | Auction house and global marketplace with listing fee | P2 |
| E11 | Gems purchasable with money at fixed rate; soft-currency to gems exchange with a sink fee (Pokétwo shard rate pattern) | P2 |
| E12 | Optional gambling (coinflip, slots, blackjack) with per-channel toggle and low caps | P2 |

### 4.4 Pets (priority 4)
| ID | Mechanic | Pri |
|---|---|---|
| PE1 | Free starter pet from a short choice of commons | P0 |
| PE2 | Pet needs (hunger, fun, energy, cleanliness) decay slowly; feed, play, walk, clean with cooldowns | P0 |
| PE3 | Rarity tiers (common, uncommon, rare, epic, legendary) with published drop chart for capsules | P0 |
| PE4 | Capsules: random wrapped pet; bought with coins or earned; duplicates convert to pet food or fragments | P0 |
| PE5 | Pet XP and levels; level capped by owner profile level (OwO pattern) | P1 |
| PE6 | Pet cosmetics (hats, collars, colors) as paperdoll layers | P1 |
| PE7 | Walk finds items, coins, furniture; fatigue and rest timer (Tatsu pattern) | P1 |
| PE8 | Nursery view: grid image of all owned pets with stats | P1 |
| PE9 | Field trip: send pets on an idle timer for rewards | P1 |
| PE10 | Neglect makes the pet "sleep" and need a revive fee; never permanent loss | P1 |
| PE11 | Pet battles: team of 3, turn-based with buttons | P2 |
| PE12 | Prestige at max level: reset level for a permanent trait or cosmetic | P2 |
| PE13 | Auto-hunt idle bot with upgrade traits (OwO Huntbot pattern) | P2 |
| PE14 | Daycare floors displaying up to 8 pets as a social showpiece | P2 |

### 4.5 Character (priority 4)
| ID | Mechanic | Pri |
|---|---|---|
| C1 | Default avatar from Discord avatar in cards until character is made | P0 |
| C2 | Pixel character creator: layered body, skin, hair, eyes, outfit slots, chosen via selects and buttons with live preview | P1 |
| C3 | Wardrobe: outfits as items (shop, drops, events), tradable | P1 |
| C4 | Character stands in own house render and on profile card | P1 |
| C5 | Titles and badges equipped from achievements | P1 |
| C6 | Character emotes shown in visits ("waves", "sits on sofa") | P2 |

### 4.6 Progression
| ID | Mechanic | Pri |
|---|---|---|
| G1 | Global level and per-server level from chat XP (15-25 per message class, 60 s cooldown); curve with rising cost | P0 |
| G2 | Daily quests (3 per day) with all-clear bonus; reset 00:00 UTC | P0 |
| G3 | Leaderboards: server, global, weekly (level, coins, house score, fishing) | P0 |
| G4 | Level-up rewards (coins plus a box) and rank roles (server tool) | P1 |
| G5 | Achievements and badges (first visit, 100 items placed, rarest catch) | P1 |
| G6 | Daily streak with milestone bonuses (7, 14, 21, 28 days) | P1 |
| G7 | Prestige or "rebirth" for long-term players, keeps cosmetics | P2 |
| G8 | Seasonal pass with free and premium tracks (cosmetics only) | P2 |

### 4.7 Mini games
| ID | Mechanic | Pri |
|---|---|---|
| M1 | Fishing: `/fish` cast with cooldown and bait cost; button to reel; rod/bait upgrades; fish collection book; rare-fish bonus | P0 |
| M2 | Wild spawn: bot posts a rare pet/item image every N chat messages (about 24), first button press or correct guess wins; incense item boosts rate | P1 |
| M3 | Dice and coinflip with small stakes | P1 |
| M4 | Work or gather command with branching result (safe vs risky) | P1 |
| M5 | Furniture crafting: combine materials into furniture (recipes admin-extendable) | P1 |
| M6 | Trivia or puzzle with coin prize and per-server channel | P2 |
| M7 | Card or character drop with grab contest and random tie-break (Karuta pattern) | P2 |
| M8 | Co-op heist or boss with join button and split reward (Dank Memer pattern) | P2 |
| M9 | Cooldown reminders: DM or ping when daily, spawn or walk is ready | P1 |

### 4.8 Cards and visuals (priority 3)
Render rule: draw pixel art at native resolution then upscale with nearest-neighbor; cache PNG by a hash of the card
state; suggested sizes (our proposal, not copied): rank about 900x300, profile about 1000x600, wallet about 800x300,
welcome about 1024x450, house room about 768x512 (scaled 3x from 256x170 art).
| ID | Mechanic | Pri |
|---|---|---|
| V1 | Profile card: avatar or character, level, XP bar, rep, coins, badges, title, pet, background | P0 |
| V2 | Rank card: server rank, XP progress, next role | P0 |
| V3 | Wallet card: coins, gems, server Stars with the server icon | P0 |
| V4 | House room render (H1-H9), visit render with guest | P0 |
| V5 | Shop card: grid of the hourly items with price, rarity frame, stock left, countdown | P0 |
| V6 | Furniture catalog page images (6-12 items per page with paging buttons) | P0 |
| V7 | Pet status card: portrait, needs bars, level, rarity | P1 |
| V8 | Level-up card and achievement toast | P1 |
| V9 | Leaderboard image (top 10 rows with avatars) | P1 |
| V10 | Card backgrounds, frames and badge sets as cosmetics (shop, drops, events) | P1 |
| V11 | Custom upload background (moderated, premium) | P2 |
| V12 | Animated GIF pet or house idle loop (few frames, small size) | P2 |

### 4.9 Server tools
| ID | Mechanic | Pri |
|---|---|---|
| T1 | Welcome card designer (ProBot pattern): admin picks background, avatar shape and position, text layers, font, colors; live preview via dashboard or command wizard | P1 |
| T2 | Level role rewards and announcement channel | P1 |
| T3 | Server point naming, emoji, earn rate and blocked roles | P1 |
| T4 | Server store item creator (roles, badges, custom items) | P1 |
| T5 | Per-channel toggles for spawns, gambling, XP | P1 |
| T6 | Autoroles and reaction roles (low priority, many bots do it) | P2 |
| T7 | Logging, automod, tickets: out of scope for the game layer | P2 |

### 4.10 Monetization notes
- Premium subscription or supporter pack: cosmetic and convenience perks (extra house tier slot, extra storage, custom
  background upload, supporter-only furniture each month, higher daily streak bonus, bigger caps). Avoid direct power.
- Personal vs server premium split (MEE6 Pro vs Premium) maps to: personal cosmetics and perks, server-wide extras
  (more server store slots, welcome card templates).
- Gems as premium currency, bought with money; patch cadence monthly themed furniture drop with a permanent coin tier,
  a permanent gem tier and a limited event tier (Tatsu pattern), plus the hourly stock (E4) as the daily hook.

### 4.11 Suggested first-release cut (P0 only)
House H1-H9, Pets PE1-PE4, Economy E1-E5, Progression G1-G3, Mini game M1, Cards V1-V6, Character C1, Social SO1-SO2.
This delivers: own house, a pet, an hourly shop, fishing, and nice image cards. Everything else follows from the same
data model (items with slot type, rarity and shop rules).

## 5. Open questions for the owner
- Free-grid placement (needs an editor) or typed slots (menus only) for the first release?
- Is gambling allowed at all (age and policy risk)? Default in this catalog: P2, off by default, low caps.
- Is a web editor or Discord Activity in scope for Rust, or image-only?
- Which currency and name set is wanted (coins, gems, server points)?

## Sources

- S1 https://tatsu.gg/tatsugotchi
- S2 https://support.tatsu.gg/hc/en-us/articles/4404004981017 (Customizing your House) [S]
- S3 https://support.tatsu.gg/hc/en-us/articles/4403218031513-How-to-play-Tatsugotchi [S]
- S4 https://tatsu.fandom.com/wiki/House and https://tatsu.fandom.com/wiki/Tatsugotchi [S]
- S5 https://tatsu.gg/llms.txt
- S6 https://tatsu.gg/economy
- S7 https://discordbotlist.com/bots/tatsu/commands
- S8 https://tatsutimes.substack.com/p/patch-108-october-update-annual-phantom
- S9 https://tatsutimes.substack.com/p/patch-119-november-update-botanical
- S10 https://tatsutimes.substack.com/p/its-been-a-long-road (same text at https://medium.com/tatsuworks/its-been-a-long-road-3eb6d25575ed)
- S11 https://community.tatsu.gg/news/updates/tg-daycare-pet-cosmetics-20-new-furni-badges-bg-cards [S]
- S12 https://wikiwiki.jp/werewolf/Discord/bot/Tatsu (Japanese fan wiki, older)
- S13 https://tatsu.gg/leveling and https://support.tatsu.gg/hc/en-us/articles/900005595306-Changing-your-rank-card-background [S]
- S14 https://dev.to/dennismurph/discord-tatsumaki-bot-features-commands-and-setup-2b98
- S15 https://blog.communityone.io/owo-discord-bot-deepdive/
- S16 https://thelinuxcode.com/owo-commands/ (low reliability) and https://owobot.fandom.com/wiki/Fabled_Animals
- S17 https://probot.io , https://docs.probot.io/ , https://techcult.com/how-to-setup-and-use-probot-on-discord , https://alternativeto.net/software/pro-bot/about
- S18 https://wiki.mee6.xyz/plugins/levels , https://help.mee6.xyz/en/articles/616234-how-to-set-up-levels-plugin , https://help.mee6.xyz/en/articles/611742-mee6-is-not-showing-my-personal-rank-card-with-pro
- S19 https://top.gg/bot/437808476106784770 , https://shapes.inc/arcanebot-1gkp/faq , https://peakbot.pro/blog/arcane-vs-tatsu-2026 (competitor blog)
- S20 https://dev.to/dennismurph/discord-carl-bot-features-commands-and-setup-564b , https://peakbot.pro/blog/carl-bot-vs-dyno-2026 (competitor blog)
- S21 https://top.gg/bot/155149108183695360
- S22 https://top.gg/bot/1348370129804591115 , https://discordbotlist.com/bots/wabbitbot
- S23 https://discordbotlist.com/bots/modora/commands , https://top.gg/bot/1416779195605975150 , https://modora.xyz
- S24 https://dankmemer.lol/tutorials/welcome-to-dank-memer , https://dankmemer.lol/tutorial/premium-redemption , https://dankmemer.lol/tutorials/drop-items , https://dankmemer.lol/blogs/rewrite-changelog
- S25 https://www.alphr.com/mude-get-more-rolls/ , https://mudae.fandom.com/wiki/FAQs
- S26 https://top.gg/bot/646937666251915264
- S27 https://docs.poketwo.net/economy/currencies-and-items , https://docs.poketwo.net/basics/spawning-and-catching
- S28 https://top.gg/bot/555955826880413696 , https://discordbotlist.com/bots/epic-rpg
- S29 https://alternative.me/discord/bots/virtualfisher/commands , https://top.gg/bot/574652751745777665
- S30 https://unbelievaboat.com , https://top.gg/bot/292953664492929025
- S31 Nightshift not found; candidates via https://top.gg search results (Moneta, Bociek, Narox, KitsuBot, Shards)
