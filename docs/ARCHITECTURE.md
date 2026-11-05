# Pinchora architecture

Pinchora is a social game bot for Discord: levels, a shell economy, pixel-art profile cards
and (next) houses, pets and mini games. It is built to run on many shards across many pods
from day one. Background research lives in [`research/`](research/).

## Stack

| Concern | Choice | Why |
|---|---|---|
| Discord | [twilight](https://github.com/twilight-rs/twilight) 0.18 + twilight-interactions | Modular: gateway, REST and models are separate crates, so shards, the REST proxy and the interaction server can scale apart. Used by the largest Rust bots (Bathbot, Hourai). |
| Database | PostgreSQL via sqlx 0.9 | Transactions with row locks for the economy, partial indexes for leaderboards, mature operators on Kubernetes (CloudNativePG). Runtime-checked queries, so builds need no live database. |
| Cache / coordination | Redis via `redis` 1.x (`ConnectionManager`) | Cooldowns, the cross-pod identify queue, saved gateway sessions. `fred` was rejected: no release since Feb 2025 and open leak bugs (see `research/rust-bots.md`). |
| Images | resvg (SVG → PNG) + `image` | Cards are SVG templates filled with data, so layouts stay readable and diffable; a 960×540 card renders in ~20 ms. Fonts are embedded. |
| Runtime | tokio, axum, tracing, metrics (Prometheus) | Standard, well-supported. |
| Deploy | Docker (cargo-chef, distroless) + Kubernetes (kustomize) | See `deploy/k8s/README.md`. |

## Processes

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
  welcome cards and — while no Interactions Endpoint URL is set — interactions.
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

- Gateway mode: answers are REST callbacks.
- HTTP mode: the first answer is the HTTP response body. If the handler takes longer than 2 s,
  the endpoint sends a deferral and later answers become edits of the original message.
  Answers with image attachments always take the defer-then-edit path, because files cannot
  ride in a JSON body.

Button state lives entirely in the `custom_id` (e.g. `lb:xp:2`), so any pod can serve any click.
Interaction callbacks use a direct REST client; everything else goes through the shared
http-proxy when one is configured.

## Data

- `users` — global profile: shells (`credits`), global XP, reputation, daily streak, bio.
- `guilds` — per-server settings (leveling, welcome cards).
- `members` — per-server XP; `(guild_id, xp DESC)` index serves leaderboards and ranks.
- `credit_ledger` — every shell movement, for audits and economy statistics.

Money moves only inside transactions that lock the touched user rows in id order, so
concurrent transfers can neither overdraw nor deadlock (covered by
`crates/pinchora-db/tests/db.rs`). Guild settings are cached per pod for 60 s.

## Crates

| Crate | Role |
|---|---|
| `pinchora-core` | config, telemetry/ops server, level and economy math, cluster math |
| `pinchora-db` | PostgreSQL access and migrations |
| `pinchora-cache` | Redis: cooldowns, sessions, identify queue |
| `pinchora-render` | pixel-art cards (SVG templates → PNG) |
| `pinchora-commands` | `App` context, responder, slash commands, components, gateway event handlers |
| `pinchora` | the binary: gateway, interactions server, migrate, register |

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
