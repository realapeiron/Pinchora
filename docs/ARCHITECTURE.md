# Pinchora architecture

Pinchora is a social game bot for Discord: levels, a shell economy, pixel-art profile cards
and (next) houses, pets and mini games. It is built to run on many shards across many pods
from day one. Background research lives in [`research/`](research/).

Rust command handlers, gateway helpers, database modules, core rules and API routes are grouped
by domain: `games`, `economy`, `progression`, `social`, `house`, `moderation`, `community`,
`roles`, `integrations` and `admin`. Shared infrastructure lives in `src/infra/`; command handlers
and API routes use `src/commands/<domain>/` and `src/routes/<domain>/` respectively.
`crates/pinchora-testkit/` supplies fixture macros only through development dependencies. It is
excluded from workspace roots so the normal workspace dependency tree contains production crates only;
its protocol tests expand at their original consumer sites and retain their isolation scopes.

## Stack

| Concern | Choice | Why |
|---|---|---|
| Discord | [twilight](https://github.com/twilight-rs/twilight) 0.18 + twilight-interactions | Modular: gateway, REST and models are separate crates, so shards, the REST proxy and the interaction server can scale apart. Used by the largest Rust bots (Bathbot, Hourai). |
| Database | PostgreSQL via sqlx 0.9 | Transactions with row locks for the economy, partial indexes for leaderboards, mature operators on Kubernetes (CloudNativePG). Runtime-checked queries, so builds need no live database. |
| Cache / coordination | Redis via `redis` 1.x (`ConnectionManager`) | Cooldowns, the cross-pod identify queue, saved gateway sessions. `fred` was rejected: no release since Feb 2025 and open leak bugs (see `research/rust-bots.md`). |
| Images | resvg (SVG → PNG) + `image` | Cards are SVG templates filled with data, so layouts stay readable and diffable; a 960×540 card renders in ~20 ms. Fonts are embedded. |
| Runtime | tokio, axum, tracing, metrics (Prometheus) | Standard, well-supported. |
| Deploy | Docker Compose on the owner's host (cargo-chef, distroless) | The production deploy is gated on a green CI commit; Kubernetes manifests are also available in `deploy/k8s/`. |

## Processes

The **`web`** role serves the React dashboard and JSON API independently of the gateway and
interactions roles. It shares PostgreSQL for player and server state and Redis for OAuth state,
sessions, cooldowns and coordination. Player writes call the same Db transactions and game
functions used by bot commands. Server dashboard writes use the managed-guild extractor and
record audit entries. The web process is stateless apart from those shared stores, so replicas
can receive any request. See [WEB.md](WEB.md) for routes, configuration and the development loop.

One binary, `pinchora`, with roles chosen by subcommand:

```
                 ┌────────────────────┐  slash commands / buttons (optional mode)
 Discord ───────►│ interactions (N)   │◄── HTTPS POST /interactions, Ed25519-verified
   │             └────────┬───────────┘
   │ gateway              │
   ▼                      ▼
 ┌────────────────────┐  ┌──────────┐   ┌─────────────┐
 │ gateway cluster i  │─►│ Postgres │   │ Redis       │
 │ shards bucket(i,C) │─►│          │   │ cooldowns   │
 └────────┬───────────┘  └──────────┘   │ identify q  │
          │ REST                         │ sessions    │
          ▼                              └─────────────┘
   http-proxy (1) ──► Discord REST (shared ratelimits)
```

- **`gateway`** runs one shard cluster. With `C` clusters and `S` total shards, cluster `i`
  owns the consecutive shard range `twilight_gateway::bucket(i, C, S)`. In Kubernetes this is a
  StatefulSet: the pod ordinal (`pinchora-gateway-3`) is the cluster id. It handles chat XP,
  welcome cards, join roles, goodbye messages, the server log (joins, name changes, bans, voice) and —
  while no Interactions Endpoint URL is set — interactions.
- **`interactions`** is a stateless HTTPS endpoint for slash commands and buttons. Once its URL
  is set in the Developer Portal, Discord stops sending interactions over the gateway, so
  command load scales with this Deployment (HPA) instead of with shards.
- **`migrate`** applies `migrations/` (sqlx takes an advisory lock, so every pod can run it as
  an init container). **`register`** bulk-overwrites slash commands.

## Gateway details

- **Identify queue across pods.** Discord allows `max_concurrency` identifies per 5 s, one per
  bucket `shard % max_concurrency`. `RedisQueue` makes that bucket a Redis key with a 5 s TTL;
  whoever sets it identifies. No extra queue service is needed.
- **Resume across deploys.** On SIGTERM each shard closes with `CloseFrame::RESUME`; its session
  id, sequence and resume URL go to Redis for 240 s. The replacement pod resumes instead of
  re-identifying, so a rolling restart replays missed events and costs no session starts.
- **Session budget guard.** The process refuses to start if Discord's remaining daily session
  starts are fewer than the shards it would open (a crash loop cannot burn the token's budget).
- **Cheap event decoding.** Only events we handle are deserialized (`EventTypeFlags`);
  `GUILD_CREATE` is skipped entirely. Nothing depends on a gateway cache, so resumes and
  restarts never leave stale state behind.
- **Back-pressure.** Event handlers run under a 1024-permit semaphore; when saturated, XP ticks
  are dropped and counted (`pinchora_events_dropped_total`) rather than queued without bound.
- **Total shards are pinned** in config when running more than one cluster, so pods never
  disagree on the shard count.

## Interactions

Every command receives a `Cx` and answers through a `Responder` that hides the transport:

- Gateway mode: answers are REST callbacks. A handler still running after 1.8 s is deferred through REST
  (`handle_gateway_interaction`); its answer then becomes an edit of the original message. The defer is
  ephemeral for commands that answer privately (the table in `defer.rs`) and is skipped for buttons that may
  open a modal, which only works as the first answer.
- HTTP mode: the first answer is the HTTP response body. If the handler takes longer than 2 s,
  the endpoint sends a deferral and later answers become edits of the original message.
  Answers with image attachments always take the defer-then-edit path, because files cannot
  ride in a JSON body.

Button state lives entirely in the `custom_id` (e.g. `lb:xp:2`), so any pod can serve any click.
Interaction callbacks use a direct REST client; everything else goes through the shared
http-proxy when one is configured.

## Data

- `users` — global profile: shells (`credits`), global XP, reputation, daily streak, bio.
- `guilds` — per-server settings (leveling, welcome cards, level role mode, server points, join role,
  goodbye message, server log channel and its per-event switches).
- `members` — per-server XP and server points; `(guild_id, xp DESC)` and `(guild_id, points DESC)`
  indexes serve leaderboards and ranks.
- `level_roles` — roles given at a level; `store_items` — roles sold for server points (price, optional stock).
- `role_panels` / `role_panel_roles` — messages of role buttons (up to 25 per panel); a button's custom id is
  `rr:<panel>:<role>` and a click is checked against these tables.
- `giveaways` / `giveaway_entries` — a giveaway and who entered; the shells prize (`winners` x `shells`) is held
  from the host when it starts, and ending, paying and refunding happen in one transaction with the status
  change. `polls` / `poll_votes` — polls of two to five options, one vote per member, closed once by the
  ticker. `suggestion_settings` / `suggestions` / `suggestion_votes` — the suggestion channel and per-server
  numbering, the suggestions and their 👍/👎 votes. Buttons carry `gw:e:<giveaway>`, `pl:v:<poll>:<option>`,
  `pl:r:<poll>` and `sg:v:<suggestion>:<u|d>`; every click is checked against its row and server.
- `credit_ledger` — every shell movement, for audits and economy statistics.

Money moves only inside transactions that lock the touched user rows in id order, so
concurrent transfers can neither overdraw nor deadlock (covered by
`crates/pinchora-db/tests/db.rs`). A store purchase locks the item row first, then takes the
points and one unit of stock in the same transaction; if Discord then refuses the role, the
handler refunds both. Guild settings are cached per pod for 60 s.

Giveaways and polls end on a ticker every pod runs (every 5 s, gateway and interactions alike). Ending is
one transaction that only one pod wins, and the announcement is claimed (`announce_claimed`, good for
2 minutes) before it is sent, so a pod that dies between the two is covered by a later tick and a second
pod never posts it twice.

## Crates

<!-- crate-table:start -->

| Crate | Description | Internal dependencies |
|---|---|---|
| `pinchora` | Application binary and process entry points | `pinchora-cache`, `pinchora-commands`, `pinchora-core`, `pinchora-db`, `pinchora-web` |
| `pinchora-assets` | Tools for preparing Pinchora image assets | — |
| `pinchora-cache` | Redis coordination, cooldowns, and gateway sessions | `pinchora-testkit` |
| `pinchora-commands` | Discord commands, interactions, and gateway handlers | `pinchora-cache`, `pinchora-core`, `pinchora-db`, `pinchora-game`, `pinchora-render`, `pinchora-testkit` |
| `pinchora-core` | Shared configuration, telemetry, and game rules | — |
| `pinchora-db` | PostgreSQL access and migrations | `pinchora-core`, `pinchora-testkit` |
| `pinchora-game` | Shared game logic and presentation data | `pinchora-core`, `pinchora-render` |
| `pinchora-render` | Pixel art cards and image rendering | `pinchora-core` |
| `pinchora-testkit` | Shared test fixture sessions and isolation protocols | — |
| `pinchora-web` | Web dashboard and public API | `pinchora-cache`, `pinchora-commands`, `pinchora-core`, `pinchora-db`, `pinchora-game`, `pinchora-render`, `pinchora-testkit` |
<!-- crate-table:end -->

Architecture choices are recorded in the [decision records](adr/README.md).

## Observability

Every serving pod exposes `:9000/metrics` (Prometheus), `/healthz` and `/readyz`. Key series:
`pinchora_interactions_total{name,outcome}`, `pinchora_interaction_seconds`,
`pinchora_gateway_events_total{event}`, `pinchora_render_seconds`, `pinchora_shard_guilds{shard}`,
`pinchora_events_dropped_total`. Logs are JSON in production (`PINCHORA_LOG__FORMAT=json`).

## Scaling rules of thumb

- Shards: Discord requires one per 2,500 guilds; keep the total a multiple of
  `max_concurrency`. Grow by raising `SHARDS` and the StatefulSet replicas together
  (`CLUSTER_COUNT == replicas`).
- Interactions: scale on CPU; card rendering is the hot path and is bounded to one render per
  CPU per pod.
- Postgres: leaderboards read indexed ranges; per-user rank uses a count over the index, which
  should move to Redis sorted sets once a single guild passes a few hundred thousand members.
