# Pinchora

A cozy pixel world for Discord servers: levels, shells, pixel-art profile cards, and soon your
own house, pets and mini games. Written in Rust and built to run sharded on Kubernetes.

![Profile card](docs/images/profile.png)

## What works today

| Command | What it does |
|---|---|
| `/profile [user]` | Pixel-art profile card: level, XP, shells, reputation, streak, global and server rank, bio |
| `/rank [user]` | Server level card |
| `/leaderboard [board]` | Server XP, global XP or global shells, with page buttons |
| `/daily` | Daily shells; the streak bonus grows for 20 days |
| `/balance [user]`, `/give user amount` | Wallet and transfers |
| `/rep user` | One reputation point per day |
| `/bio text` | About-me text on the profile card |
| `/config welcome\|leveling\|show` | Welcome cards and level-up messages (Manage Server) |
| `/ping`, `/help` | Health and command list |

Chatting earns 15–25 XP per minute per server. New members get a welcome card when enabled.

### The pixel world

| Command | What it does |
|---|---|
| `/house view [room]` | Your house, rendered tile by tile; your active pet wanders around |
| `/house edit [room]` | Decorate inside Discord: pick furniture, move the cursor with ⬅⬆⬇➡, place, flip, pick up |
| `/house visit user` | Visit a house with your pet (first visit each day pays shells, 10 a day) |
| `/house like user`, `/house lock` | Daily likes; close your doors to visitors |
| `/house decorate [room]` | 44 wallpapers and 44 floors, bought on first use, swapped freely |
| `/house upgrade` | Six tiers from Tide Pool Shack to Sunken Palace; furniture stays put |
| `/shop market` | Eight pieces every hour with stock shared by every player |
| `/shop catalog [category]` | 165 permanent designs in 17 aisles; 1,175 items with colour variants |
| `/inventory`, `/gift user` | What you own, and sending furniture to friends |
| `/stall sell\|view\|manage` | Your own player shop (8 listings, 5% market fee) |
| `/plaza view\|donate\|edit` | The server's shared plaza: members donate furniture, managers arrange it |
| `/pet adopt\|view\|capsule\|list\|rename` | Pets with needs that drift over time; feed, play, bathe; 42 species |
| `/fish` | Cast, wait for the bite, press **Reel!** in time; 72 catches for the fish book |
| `/quests` | Three daily quests; clear all three for pearls |
| `/duel tictactoe user` | Button tic-tac-toe; the winner earns shells |
| `/config welcome-card` | Design the welcome card (theme, headline, accent colour) with a live preview |

Game content (furniture, rooms, pets, fish) lives in `content/*.json`; see
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
works before the art lands.

## Fonts

Silkscreen and Noto Sans are under the SIL Open Font License; the license texts are in
`assets/fonts/`.
