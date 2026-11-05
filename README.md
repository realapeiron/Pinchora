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
| `/profile [user]` | Pixel-art profile card: level, XP, shells, reputation, streak, global and server rank, bio, up to six badges |
| `/rank [user]` | Server level card, with the next level role to earn |
| `/customize background` | Private editor with a live preview of your profile card: 36 backgrounds in five rarities, bought once with shells (**Buy**), then worn (**Equip**) or taken off (**Remove**); the menu lists owned ones first, then the shop, 25 per page. The profile, rank and wallet cards draw the background under their panels and take their accent colour from it |
| `/customize badges` | Private editor for the badges on your profile card: 28 achievements worked out on the spot from real progress (level, fish species, house likes, pets, daily streak, furniture placed, reputation, shells; locked ones show e.g. `12/36 fish species`) and 12 shop badges bought once with shells; wear up to six, in the order you pick them. A badge you no longer qualify for (a lapsed streak, spent shells) leaves the card until you earn it again |
| `/leaderboard [board]` | Server XP, server points, global XP or global shells as a pixel-art card, with page buttons |
| `/daily` | Daily shells; the streak bonus grows for 20 days |
| `/balance [user]`, `/give user amount` | Wallet and transfers |
| `/rep user` | One reputation point per day |
| `/bio text` | About-me text on the profile card |
| `/config welcome\|leveling\|show` | Welcome cards and level-up messages (Manage Server) |
| `/config level-roles add\|remove\|list\|mode\|sync` | Roles given at a level (up to 25): `mode` keeps every role earned or only the highest, `sync` catches up members who already passed a level (Manage Server) |
| `/points show [user]` | Server points balance and rank; the server can rename the points and pick their emoji |
| `/points give\|take user amount` | Adjust a member's server points (Manage Server) |
| `/config points [enabled] [name] [emoji] [min] [max]` | Turn server points on or off, rename them, and set what a message earns (Manage Server) |
| `/store` | The server's role store: pick a role from the menu to buy it with server points |
| `/config store add\|remove\|list` | Put roles on sale for points, with optional stock (up to 25); adding a role again updates its price and stock (Manage Server) |
| `/ping`, `/help` | Health and command list |

Chatting earns 15–25 XP and, by default, 1–3 server points per minute per server. Level roles are
given on level-up, and the bot needs the Manage Roles permission with its role above the roles it
hands out; roles the bot can't give are refused when they are added. If Discord refuses a store
role at purchase time, the points and stock are returned. New members get a welcome card when enabled.

### The pixel world

| Command | What it does |
|---|---|
| `/character` | Design your pixel character: 24 looks, skin, hair, top and bottom colours; it stands in your house, joins you on visits and shows on your profile card |
| `/house view [room]` | Your house, rendered tile by tile; your active pet wanders around |
| `/house edit [room]` | Decorate inside Discord: pick furniture, move the cursor with the ↖↗↙↘ arrows (on the floor or either wall), place, turn, flip, pick up |
| `/house visit user` | Visit a house with your pet and character (first visit each day pays shells, 10 a day) |
| `/house like user`, `/house lock` | Daily likes; close your doors to visitors |
| `/house decorate [room]` | 44 wallpapers and 44 floors, bought on first use, swapped freely |
| `/house upgrade` | Six tiers from Tide Pool Shack to Sunken Palace; furniture stays put |
| `/shop market` | Eight pieces every hour with stock shared by every player |
| `/shop catalog [category]` | 358 permanent designs in 23 aisles; 650 designs and 1,861 items with colour variants in all |
| `/inventory`, `/gift user` | What you own, and sending furniture to friends |
| `/stall sell\|view\|manage` | Your own player shop (8 listings, 5% market fee) |
| `/plaza view\|donate\|edit` | The server's shared plaza: members donate furniture, managers arrange it |
| `/pet adopt\|view\|capsule\|list\|rename` | Pets with needs that drift over time; feed, play, bathe; 42 species |
| `/fish` | Cast, wait for the bite, press **Reel!** in time; 72 catches for the fish book |
| `/quests` | Three daily quests; clear all three for pearls |
| `/duel tictactoe user` | Button tic-tac-toe; the winner earns shells |
| `/config welcome-card` | Design the welcome card (theme, headline, accent colour) with a live preview |

Game content (furniture, rooms, pets, fish, looks, card backgrounds and badges) lives in
`content/*.json` (`python3 content/check.py` validates it); see
[`docs/GAME_DESIGN.md`](docs/GAME_DESIGN.md). Sprites are drawn by GPT and imported with
`tools/art/` (see below).

## Quick start (Docker)

1. Create an application at <https://discord.com/developers/applications>, add a bot, and turn
   on **Server Members Intent** (for welcome cards; or set `PINCHORA_GATEWAY__MEMBER_EVENTS=false`).
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
                                                 # (profile-background/-badges/-placeholder, rank-background, wallet-background show /customize)
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
