# Pinchora

A cozy pixel world for Discord servers: an isometric house you build and furnish from 1,800+
items, a pixel character, pets, fishing, a shared server plaza, levels and pixel-art cards.
Written in Rust and built to run sharded on Kubernetes.

![A furnished room](docs/images/room.png)

![Profile card](docs/images/profile.png)

![Character creator](docs/images/character.png)

![Leaderboard](docs/images/leaderboard.png)

## What works today

| Command | What it does |
|---|---|
| `/profile [user]` | Pixel-art profile card: level, XP, shells, reputation, streak, global and server rank, bio, an optional title under the name, up to six badges, each on a plate tinted by its rarity, and, if the player chose any, a **showcase** row (favourite fish, pet, furniture, house thumbnail) and their style (accent colour, panel opacity, frame). Until you choose your badges the card shows your best earned ones (rarest first) on its own. With nothing on the showcase the card keeps its classic layout; with a showcase the about-me panel and the stats give up height to a row of tiles. With a partner (`/partner`), the corner of the about-me panel shows `♥ NAME`, measured as drawn so accented, non-Latin and emoji names keep the heart beside them. |
| `/rank [user]` | Server level card, with the next level role to earn, the title under the name and the player's card style |
| `/customize background` | Private editor with a live preview of your profile card: 36 backgrounds in five rarities, bought once with shells (**Buy**), then worn (**Equip**) or taken off (**Remove**); the menu lists owned ones first, then the shop, 25 per page. The profile, rank and wallet cards draw the background under their panels and take their accent colour from it |
| `/customize badges` | Private editor with your badge collection card: all 40 badges in a grid, earned ones in colour on a rarity plate, locked ones dark with a progress bar, a `23/40` counter and a NEW tag on what you earned since you last looked. 28 achievements worked out on the spot from real progress (level, fish species, house likes, pets, daily streak, furniture placed, reputation, shells) and 12 shop badges bought once with shells; wear up to six, in the order you pick them, take them all off, or press **Show my best automatically** to let the card choose again. A badge you no longer qualify for (a lapsed streak, spent shells) leaves the card until you earn it again |
| `/customize title` | Private editor for the title under your name on the profile and rank cards: 29 earned by stats (level, fish species, house likes, pets, streak, furniture, reputation, shells) or by owning a badge, plus the special *House of the Week* for contest winners; locked titles show what they still need. **Remove** takes it off |
| `/customize style` | Private editor with a live preview of your profile card: an **accent colour** (Auto follows the background; 12 pixel-palette presets; or **Hex code…**, a form that refuses unreadable near-black values), **panel opacity** (Light, Normal, Solid; offered once you wear a background picture, since there is nothing for it to show over before) and a pixel **frame** drawn around the whole card (Plain and Rope free; Coral from level 10, Pearl 20, Gold 30, Neon 40, shown with a lock and the level until then). **Reset style** puts it all back, a frame saved above your level included. The style is on the profile, rank and wallet cards |
| `/customize showcase` | Private editor with a live preview: up to three picks for the profile card, a favourite **fish** (from your fish book), a **pet** and a **furniture** item (owned or placed), chosen from menus of what you have (25 per page), plus a **Show house** switch for a thumbnail of your first room (drawn once and kept until the room changes). Sprites are drawn at whole-number scales only, so every pixel stays a crisp block: small ones are enlarged up to 4×, big ones reduced by a whole divisor without blending colours. **Clear slot** and **Clear all** empty it again; empty slots are simply not drawn |
| Badge unlocked notices | When a command or chat level-up earns you an achievement badge (`/daily`, `/fish`, `/pet`, placing furniture, `/give`, a new chat level), a short "Badge unlocked" embed with the badge art follows the reply, once per badge |
| `/leaderboard [board]` | Server XP, server points, **Weekly XP** (resets Monday 00:00 UTC, with last week's top three in a panel under the board), global XP or global shells as a pixel-art card, with page buttons |
| `/daily` | Daily shells; the streak bonus grows for 20 days |
| `/weekly`, `/monthly` | A bigger reward each, on a cooldown of their own: 1,500 shells every 7 days and 6,500 every 30. The cooldown counts from the claim, and a claim made at the same moment as another pays once |
| `/balance [user]`, `/give user amount` | Wallet and transfers. The wallet card shows the time left on the daily, weekly and monthly reward side by side (`ready` in green once it can be collected) |
| `/remind me when daily\|weekly\|monthly\|pet\|trip on\|off` | Opt in to a direct message when a reward is ready again (scheduled when you collect it, or at once for one already counting down), when your active pet needs looking after (hunger, fun or hygiene down to 30, no sooner than two hours after you last looked after it; not while it is away or asleep) or when a pet is home from a trip. One message each time, then it waits for the next claim, trip or care |
| `/remind me in duration text` | A note to yourself by direct message: `45m`, `2h30m`, `1d 12h` (a minute to a year; units w d h m s). The text is up to 200 characters and loses any mention (users, roles, channels, `@everyone`, `@here`). Up to 25 open at a time |
| `/remind list`, `/remind cancel id` | Which notifications are on and the reminders still to come, with a menu to cancel one; or cancel by the number the list shows |
| `/privacy` | Private panel with three switches and a delete button. **Profile** private: others who look you up with `/profile`, `/rank`, `/balance`, `/friends list`, `/partner view` or `/play stats` see "This profile is private." (your own cards are never hidden from you). **Boards** hidden: you are left off the global XP and shell leaderboards, and the ranks of others count without you. **House** hidden: not offered in the `/house visit` shortlist and refused by name too (stronger than `/house lock`). **Delete my data…** asks twice (the second button works for two minutes) and removes every row about you in every table: account, house, pets, friends, notes, votes, history and the rest. A running duel's wager goes back to the other player |
| `/wrapped [year]` | A tall card with your year: the busiest month and a bar for each, messages, XP, shells earned, fish caught, houses visited, pets cared for, likes given and received, your favourite fish, the house you visited most (a friend's first) and a fun title. Open from 1 December to 31 January; outside that it says when it opens. Pinchora has counted per month since this feature began, so earlier months of your year only fill in as you play |
| `/rep user` | One reputation point per day; a server's reputation roles are given the moment the receiver's total reaches them, and the reply says so |
| `/bio text` | About-me text on the profile card |
| `/config welcome\|leveling\|show` | Welcome cards and level-up messages. `/config leveling` takes `enabled`, `channel`, `message` (placeholders `{user}` `{level}` `{server}` `{role}`, the last being the reward role just earned, empty when none) and `mode`: `channel` (the level-up channel, else where they chatted), `current` (always where they chatted), `dm` (a direct message; closed DMs are skipped quietly) or `off` (no message, roles still given) (Manage Server) |
| `/config level-roles add\|remove\|list\|mode\|sync` | Roles given at a level (up to 25): `mode` keeps every role earned or only the highest, `sync` catches up members who already passed a level (Manage Server) |
| `/config xp multiplier value [channel] [role]` | Scale the server XP and points a channel (text or voice) or role earns, from 0 to 5; 0 means no XP, 1 is normal and drops the rule. A channel's multiplier is multiplied by the member's highest role multiplier, a no-XP role always wins, and `@everyone` is a server-wide multiplier. Global XP is never scaled. Up to 50 rules (Manage Server) |
| `/config xp clear [channel] [role]`, `/config xp show` | Remove one multiplier or all of them, or list the multipliers with the voice settings (Manage Server) |
| `/config xp voice [enabled] [amount]` | Voice XP, off by default: members in a voice channel with at least one other person earn `amount` (1 to 100, default 10) server XP a minute; nothing while alone (bots do not count), muted and deafened at once, or in the server's AFK channel. Multipliers apply by the voice channel and the member's roles; level-ups follow the leveling mode (Manage Server) |
| `/config rep-roles add\|remove\|list` | Roles given at a reputation count (up to 25, same safety checks as level roles); they always stack and are given when a `/rep` point lands (Manage Server) |
| `/config join-role [role] [off]` | A role every new member gets, bots excepted; with neither option it shows the current setting (Manage Server) |
| `/config goodbye [channel] [message] [off]` | Goodbye message in a channel when a member leaves; placeholders `{user}` `{server}` `{members}` (Manage Server) |
| `/config log [channel] [off]` | Server log: compact embeds with the member, their avatar and the time for joins and leaves, nickname and username changes, bans and unbans, and voice joins, leaves and moves (Manage Server) |
| `/config log-events [members] [names] [bans] [voice]` | Switch kinds of log entries on or off; all start on, so picking a log channel is enough (Manage Server) |
| `/roles panel title [description]` | Start a panel of role buttons (a draft); the same title again only changes the description (Manage Server) |
| `/roles add panel role label [emoji]` | Put a role button on a panel (up to 25); adding a role again changes its label and emoji. `panel` is the title or the number (Manage Server) |
| `/roles remove panel role`, `/roles post panel channel` | Take a button off, or post the panel; posting again in the same channel updates the message in place (Manage Server) |
| `/points show [user]` | Server points balance and rank; the server can rename the points and pick their emoji |
| `/points give\|take user amount` | Adjust a member's server points (Manage Server) |
| `/config points [enabled] [name] [emoji] [min] [max]` | Turn server points on or off, rename them, and set what a message earns (Manage Server) |
| `/store` | The server's role store: pick a role from the menu to buy it with server points |
| `/config store add\|remove\|list` | Put roles on sale for points, with optional stock (up to 25); adding a role again updates its price and stock (Manage Server) |
| `/ping`, `/help` | Health and command list |

Chatting earns 15–25 XP and, by default, 1–3 server points per minute per server. Level roles are
given on level-up, and the bot needs the Manage Roles permission with its role above the roles it
hands out; roles the bot can't give (@everyone, integration-managed roles, roles above the bot's own)
are refused when they are added to the level roles, the store, the join role or a role panel. If
Discord refuses a store role at purchase time, the points and stock are returned. New members get a
welcome card when enabled.

Reminders wait in a table (`reminders`), one row per message with the time it is due. One ticker on
every pod looks every 30 seconds, takes the due rows in batches with `FOR UPDATE SKIP LOCKED`, leases them
for two minutes while the messages go out and deletes each one once sent, so a restart delays a message
and two pods never send the same one. A player with closed direct messages is logged at debug level and the
reminder is dropped. A message about a reward, a pet or a trip is checked again just before it is sent: if
the player switched it off, claimed the trip or looked after the pet meanwhile, it is dropped or moved
instead of sent.

Wrapped counts per user and calendar month (UTC) in `activity_monthly`, bumped where things already
happen: chat and voice XP, every positive ledger entry that is not a transfer from another player or a refund,
fish caught, the first visit to a house each day, house likes, and pet care. The favourite fish comes from
the fish book and the most visited house from the visit log, which has history of its own.

Voice XP keeps who is in voice in Redis, written from the voice-state events, and one ticker on the
gateway pays every guild once a minute behind a per-minute lock, so a restart or a second gateway pod
never pays a minute twice. Members already in voice when the bot first starts are noticed once their
state changes, and a member the bot has not heard from for eight hours is dropped, in case it was
away when they left. A voice multiplier by role uses the roles the member had at their last voice
event. Weekly XP counts every chat and voice XP grant under the Monday of its week.

Role panel buttons toggle the role and answer privately ("Added @Role" / "Removed @Role"). A button
only works while its role is still on that panel, so removing a role from a panel stops old
messages from handing it out. The server log remembers each member's last-seen nickname and username
in memory to show "old → new"; after a restart, or for a member it has not seen yet, a nickname
change is logged as just the new value. Voice moves work the same way: a member already in a voice
channel when the bot starts is only noticed once their state changes.

### The pixel world

| Command | What it does |
|---|---|
| `/character` | Design your pixel character: 24 looks, skin, hair, top and bottom colours; it stands in your house, joins you on visits and shows on your profile card |
| `/house view [room]` | Your house, rendered tile by tile; your active pet wanders around; says when your guestbook has new notes |
| `/house edit [room] [owner]` | Decorate inside Discord: pick furniture, move the cursor with the ↖↗↙↘ arrows (on the floor or either wall), place, turn, flip, pick up. **↶ Undo** reverses the last 10 changes to the room (place, move, store, turn, flip, clear), even after reopening the editor, until the room is changed some other way; **Clear the room…** (first in the Go to menu, with a confirm step) stores every piece at once and is one undo step. With `owner` set to your partner you decorate their house: the pieces come from your own inventory, a piece you place belongs to the house's owner from then on (the editor says so), and anything stored goes to their inventory; moving their pieces is theirs to do, and your undo history is separate from theirs |
| `/house visit [user] [room]` | Visit a house with your pet and character (first visit each day pays shells, 10 a day); **Sign guestbook** leaves the owner a note. Without a name it offers a menu of your friends whose doors are open, the most liked first |
| `/house guestbook [user]` | A house's guestbook, newest first (5 notes a page), and its last 10 visitors. Visitors sign after a visit: one plain-text note (140 characters, mentions and links left out) per house per day, not while the house is locked or the owner blocked them. Your own book is private and marks new notes; delete notes and block or unblock signers from its menus |
| `/house contest view\|enter\|vote\|winners` | The server's weekly room contest (weeks start Monday 00:00 UTC, one theme a week for everyone): `enter [room]` snapshots a room as it is (entering again updates it), `vote` shows other members' rooms one at a time with **Vote** and **Skip** (one vote per room), `view` shows the theme, time left, entries, your entry and last week's podium, `winners` draws last week's winning room. The first contest call after a week ends pays 500/250/100 shells to the three most-voted rooms and makes the winner **House of the Week** |
| `/house like user`, `/house lock` | Daily likes; close your doors to visitors |
| `/house decorate [room]` | 44 wallpapers and 44 floors, bought on first use, swapped freely |
| `/house upgrade` | Six tiers from Tide Pool Shack to Sunken Palace; furniture stays put |
| `/shop market` | Eight pieces every hour with stock shared by every player |
| `/shop catalog [category]` | 358 permanent designs in 23 aisles; 650 designs and 1,861 items with colour variants in all |
| `/inventory`, `/gift user` | What you own, and sending furniture to friends |
| `/stall sell\|view\|manage` | Your own player shop (8 listings, 5% market fee) |
| `/plaza view\|donate\|edit` | The server's shared plaza: members donate furniture, managers arrange it |
| `/partner propose user` | Ask someone to be your partner; they get **Accept** and **Decline** buttons for 10 minutes. You both must be free: a player has one partner at a time |
| `/partner view [user]` | The pair's picture with both characters, together since, days together, hugs, pats, waves and high fives they sent each other, and their houses' likes and pieces |
| `/partner leave` | End the partnership, after a confirm step |
| `/friends add user` | Ask someone to be your friend (**Accept** or **Decline**, good for a day); up to 100 friends each |
| `/friends remove user`, `/friends list [user]` | Remove a friend, or page through a player's friends with their level and house likes (10 a page) |
| `/hug user`, `/pat user`, `/wave user`, `/highfive user` | A looping GIF (14 frames, about 120 KB) of your two pixel characters, drawn from the looks and colours you both chose (a player without a character gets a stand-in made from their id): they step together with hearts, a hand bobs on a head, an arm waves, palms meet in a flash. "That's hug #12 from you to them", with the sender's and receiver's totals; the receiver gets a **Hug back** button. One every 5 seconds per player |
| `/pet adopt\|view\|capsule\|rename` | Pets with needs that drift over time; feed, play, bathe, walk (a walk finds shells, more at a higher level); 42 species. Every care action, walk and trip earns XP: levels 1-30, a bar on the pet card, level-ups (and the trips they open) announced in the reply. A pet whose hunger, fun and hygiene all sit at zero for 24 hours falls asleep: its card shows it asleep, it earns no XP and cannot be looked after or sent away until **Wake up** (one free a day, else 75 shells). Never permanent, never lost. Unknown button actions answer with an error instead of bathing the pet |
| `/pet list` | The nursery: one card with every pet in a grid (portrait, name, level and XP bar, the four needs as icons, a star on the active pet, ASLEEP / AWAY / BACK! badges), 8 to a page, with a menu that opens one pet's card (care buttons, **Make active**, a way back to its page) |
| `/pet trip go destination [pet]`, `/pet trip claim` | Field trips: send a pet (default: your active one) for 1, 4 or 8 hours to one of 8 cosy places from `content/pet_trips.json` (a place opens at a pet level, 1 to 26; the card's **Trip** button lists them). The pet leaves the house, its needs wait for it and it cannot be looked after meanwhile. Back, **Claim souvenir** (or `/pet trip claim`, which collects every pet that is home and otherwise says who is still away) pays shells, XP and a short story, and sometimes a free-to-earn furniture piece (`reward` items only, weighted by rarity, never shop stock). Higher levels find more shells and rarer pieces. A trip is a stored end time, so it survives restarts, and a claim pays exactly once |
| `/fish` | Cast, wait for the bite, press **Reel!** in time; 72 catches for the fish book |
| `/quests` | Three daily quests; clear all three for pearls |
| `/duel tictactoe user` | Button tic-tac-toe; the winner earns shells |
| `/duel connect4 user [bet]` | Connect four on a pixel-art board: the challenged player accepts, then each turn is a column button (1-7) within 90 s; the winner earns shells |
| `/duel rps user [rounds] [bet]` | Rock paper scissors, best of 1, 3 or 5: each player gets private Rock / Paper / Scissors buttons, the channel sees each round revealed; ties replay |
| `/play hangman` | A cozy word (780 in five categories) guessed from two letter menus while the tide washes a pixel sandcastle away; six misses lose |
| `/play wordle [mode]` | Five letters, six tries, typed into a form, shown as private pixel tiles with a keyboard summary. `daily` (default) is one word the same for everyone, kept per player per day, with a spoiler-free **Share result**; `practice` is a random word, as many as you like |
| `/play trivia [category]` | A question for the channel from 464 (8 topics, three difficulties): four answer buttons, 20 seconds, anyone answers once; the message then reveals the answer and who got it right |
| `/play stats [user]` | Games played and won for each game |
| `/config welcome-card` | Design the welcome card (theme, headline, accent colour) with a live preview |

Duel wagers (`bet`, 10 to 500 shells each) are optional and off by default. Nothing is held until the
challenged player accepts; then both bets leave the balances in one transaction and sit in the database
row of the game. The winner takes both; a draw, a decline, a no-show or a timeout gives them back, and
whoever ends the game first pays out, exactly once, so a restart cannot lose or double shells (a janitor on
every pod ends games whose clock ran out every 10 seconds). Without a wager the system pays small rewards
(connect four 25, rock paper scissors 15, hangman 15, wordle 10 and up to 45 for the daily, trivia 8 to 18
by difficulty) with a daily limit per player and game.

Game content (furniture, rooms, pets, pet trips, fish, looks, card backgrounds, badges, titles, trivia, hangman words and
the wordle lists) lives in
`content/*.json` (`python3 content/check.py` validates it); see
[`docs/GAME_DESIGN.md`](docs/GAME_DESIGN.md). Sprites are drawn by GPT and imported with
`tools/art/` (see below).

## Quick start (Docker)

1. Create an application at <https://discord.com/developers/applications>, add a bot, and turn
   on **Server Members Intent** (for welcome cards, join roles, goodbye messages and the member, nickname and
   username entries of the server log; or set `PINCHORA_GATEWAY__MEMBER_EVENTS=false`). The server log's ban and voice
   entries use the non-privileged Guild Moderation and Guild Voice States intents.
2. `cp .env.example .env` and fill in the token and application id.
3. Start everything:

   ```bash
   docker compose up -d
   ```

4. Register the slash commands (to `PINCHORA_DISCORD__DEV_GUILD_ID` if set, else globally):

   ```bash
   docker compose run --rm gateway register
   ```

5. Invite the bot with the `bot` and `applications.commands` scopes.

To serve commands over HTTPS instead of the gateway, run the `endpoint` profile
(`docker compose --profile endpoint up -d`), expose port 8080 at a public HTTPS URL, and set
`https://<host>/interactions` as the Interactions Endpoint URL in the Developer Portal.

Kubernetes manifests and scaling notes: [`deploy/k8s/README.md`](deploy/k8s/README.md).

## Development

```bash
cargo run -p pinchora -- gateway          # needs PINCHORA_* env vars or pinchora.toml
cargo run -p pinchora -- commands         # print slash command JSON
cargo run -p pinchora-render --example samples   # render sample cards to target/render-samples
                                                 # (profile-background/-badges/-placeholder, rank-background, wallet-background show /customize;
                                                 #  the profile and rank samples carry a title; profile-showcase-*, profile-bg-<white|black|stripes|beach>-<opacity>,
                                                 #  profile-nobg-*, profile-unicode/-wide/-cyrillic/-shout and profile-frame-* are the stress variants;
                                                 #  wrapped and wrapped-quiet are the year card for a busy player and for one with nothing)
cargo run -p pinchora-render --example games     # connect four, hangman and wordle sample cards
```

Tests: unit tests run with plain `cargo test --workspace`. Database and Redis integration
tests run when these are set:

```bash
PINCHORA_TEST_DATABASE_URL=postgres://pinchora:pinchora@localhost:5432/pinchora \
PINCHORA_TEST_REDIS_URL=redis://localhost:6379 \
cargo test --workspace
```

## Layout

```
crates/
  pinchora/           binary: gateway | interactions | migrate | register | commands
  pinchora-core/      config, telemetry, level/economy/cluster math
  pinchora-db/        PostgreSQL (sqlx) + migrations runner
  pinchora-cache/     Redis: cooldowns, gateway sessions, cross-pod identify queue
  pinchora-render/    pixel-art cards (SVG → PNG with resvg)
  pinchora-commands/  slash commands, buttons, gateway event handlers
migrations/           SQL migrations
assets/fonts/         Silkscreen + Noto Sans (SIL OFL)
deploy/               Kubernetes (kustomize)
docs/                 architecture, research, game design
```

Design notes: [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md). Research behind the choices:
[`docs/research/rust-bots.md`](docs/research/rust-bots.md) and
[`docs/research/games-survey.md`](docs/research/games-survey.md).

## Art pipeline

```bash
cd tools/art
python3 plan_sheets.py     # content → 40 sheet prompts + cut manifests in sheets/
./run_queue.sh 3           # Codex draws them (3 at a time, resumable)
./import.sh                # cut, de-scale, palette-quantize, pack assets/sprites/atlas.*
```

Until a sprite exists, the renderer draws a tidy placeholder in its place, so every feature
works before the art lands. Backgrounds are `bg_<id>` (240×135, drawn ×4 to 960×540 and
cropped to cover the rank and wallet cards) and badges `b_<id>` (24 px wide, drawn ×2); until
they are in the atlas a background is a gradient from its accent colour and a badge is a
coloured circle with its first letter.

## Fonts

Silkscreen and Noto Sans are under the SIL Open Font License; the license texts are in
`assets/fonts/`.
