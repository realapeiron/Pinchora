# Open-source Rust Discord bots: survey and lessons for Pinchora

Surveyed 2026-11-05 via the GitHub API (trees, key files, commits, issues). Repo state is "HEAD on that date".
Links are `blob/HEAD` URLs. "Inferred" marks my own reasoning; "docs" marks Discord API behaviour recalled from
memory and not re-fetched here, so verify before relying on it.

## 1. Headlines

- No surveyed bot runs a sharded multi-pod (Kubernetes) topology. The only prior art for that layer is twilight's own
  [`bucket()`](https://github.com/twilight-rs/twilight/blob/HEAD/twilight-gateway/src/lib.rs),
  [`gateway-queue`](https://github.com/twilight-rs/gateway-queue) and [`http-proxy`](https://github.com/twilight-rs/http-proxy).
- Every twilight-based bot gets slash commands over the **gateway** (`INTERACTION_CREATE`). None uses an HTTP
  interactions endpoint; the only reference is
  [`examples/model-webhook-slash.rs`](https://github.com/twilight-rs/twilight/blob/HEAD/examples/model-webhook-slash.rs).
- Only Bathbot implements session resume across restarts, and it needed a Redis-backed cache to make it useful.
- Nobody renders with resvg. Bathbot uses skia-safe imperative drawing; assyst2 shells out to a separate tool; the rest do
  no image work. Pinchora's resvg/tiny-skia choice has no precedent here but is consistent with what hurt Bathbot (native libs).
- Test coverage is near zero everywhere. Hourai's DB migration smoke test is the best small pattern.
- Stack concern found: **fred 10.1.0 looks abandoned** (section 6).

## 2. Matrix A: runtime

| Bot | Discord lib | Process layout | Sharding / resume | Cache |
|---|---|---|---|---|
| Bathbot | twilight 0.16 + interactions fork | 1 binary, all shards in-process, optional axum server | `create_recommended`; cold resume via Redis; auto-reshard 23 h | custom Redis cache (bb8-redis + rkyv) |
| assyst2 | twilight git (0.17 line) | 3 binaries on one host: gateway, core, cache; Unix sockets + bincode | `create_recommended`, no resume | moka in core + tiny cache process for guild ids |
| Hourai | twilight 0.16 (simd-json) | 1 binary (`hourai-logger`) + libs | `create_recommended`, no resume, `CloseFrame::NORMAL` | forked in-memory cache (member/guild/presence only) |
| robbb | serenity (fork) + poise | 1 binary, single guild | serenity default | serenity cache |
| ferrisbot | serenity + poise 0.6 | 1 binary, single guild | none configured | serenity cache |
| Spoticord (archived) | serenity 0.12 + poise + songbird | 1 binary | serenity ShardManager | serenity cache |
| scripty (archived) | serenity `next` + poise + songbird | 1 binary + axum | `start_autosharded` | serenity cache; Redis for app state |
| compiler-bot | serenity =0.12.5, custom dispatcher | 1 binary | `start_autosharded` | LRU + serenity cache |

## 3. Matrix B: data, rendering, ops

| Bot | DB / migrations | Images | Interactions / commands | Observability | Deploy | Tests |
|---|---|---|---|---|---|---|
| Bathbot | Postgres, sqlx 0.8, reversible up/down, `.sqlx` offline | skia-safe 0.81 + plotters-skia | gateway; own proc-macros + linkme | `metrics` + Prometheus, tracing-appender | compose (PG+Redis, dev), scp binary | none |
| assyst2 | Postgres, sqlx 0.7 native-tls | external `Flux` CLI | gateway + prefix; own `#[command]` macro | `prometheus` + axum, jemalloc | none (`run.sh`) | none (1 fuzz target) |
| Hourai | Postgres, sqlx 0.6, one initial migration | none | gateway; hand-rolled | `metrics-exporter-prometheus` :9090 | Dockerfile, compose prod (Grafana, nginx) | 1 migration test |
| robbb | SQLite, sqlx, migrations at startup | none | poise slash+prefix | Prometheus, Honeycomb, Pyroscope | Dockerfile (prebuilt binary), Nix | none |
| ferrisbot | SQLite, sqlx 0.8 + Litestream | `image`+`imageproc`+`ab_glyph` deps | poise | tracing files | K8s via Flux, SOPS, cargo-chef | 1 module |
| Spoticord | Postgres, diesel-async, embedded migrations | none | poise | env_logger | Dockerfile + GHCR workflow | none |
| scripty | Postgres, sqlx 0.8, 45 forward-only migrations | none | poise | tokio-metrics, tracing | Dockerfile (nightly, mold) | none |
| compiler-bot | none | none | serenity custom | pretty_env_logger | Dockerfile, release binaries | parser tests |

## 4. Per-bot notes

### Bathbot (MaxOhn/Bathbot) - the closest match in scale and rendering
- Workspace of 9 crates ([Cargo.toml](https://github.com/MaxOhn/Bathbot/blob/HEAD/Cargo.toml)); `bathbot-cards`,
  `-cache`, `-psql`, `-server` (axum: /metrics, OAuth) split cleanly from the bot.
- Boot order ([main.rs](https://github.com/MaxOhn/Bathbot/blob/HEAD/bathbot/src/main.rs)): config, metrics, PG, Redis,
  `defrost` sessions, shards, commands registered **on every boot**, then per-shard runner tasks inside a `JoinSet`.
- Resume: on SIGINT/SIGTERM each shard gets `CloseFrame::RESUME`, `Session`s are serialized with rkyv into one Redis key
  with TTL 240 s ([cold_resume.rs](https://github.com/MaxOhn/Bathbot/blob/HEAD/bathbot-cache/src/cache/cold_resume.rs),
  [shutdown.rs](https://github.com/MaxOhn/Bathbot/blob/HEAD/bathbot/src/core/context/shutdown.rs),
  [discord.rs](https://github.com/MaxOhn/Bathbot/blob/HEAD/bathbot/src/core/context/discord.rs)); a `guild -> shard`
  map is stored too, because resumed shards never re-send GUILD_CREATE.
- Event loop ([events/mod.rs](https://github.com/MaxOhn/Bathbot/blob/HEAD/bathbot/src/core/events/mod.rs)): one runner per
  shard, `next_event(EVENT_FLAGS)`, `cache.update(&event)`, then `tokio::spawn(handle_event)`. Cache misses are modelled
  explicitly (`EventLocation::UncachedGuild/UncachedChannel/Cached`).
- Interactions: [command.rs](https://github.com/MaxOhn/Bathbot/blob/HEAD/bathbot/src/core/events/interaction/command.rs)
  checks owner/guild-only flags, a per-user bucket, then **defers before executing** when the command flag says so.
- Cards: [bathbot-cards](https://github.com/MaxOhn/Bathbot/tree/HEAD/bathbot-cards/src) is a typestate builder
  ([card.rs](https://github.com/MaxOhn/Bathbot/blob/HEAD/bathbot-cards/src/card.rs)) drawing on a skia `Canvas`
  (980x1260, PNG out). Fonts: 12 Roboto TTFs read from disk once into `FontData`
  ([font.rs](https://github.com/MaxOhn/Bathbot/blob/HEAD/bathbot-cards/src/font.rs)). Avatar + flag are fetched in parallel
  with `tokio::join!` and passed in as `&[u8]`
  ([cards.rs](https://github.com/MaxOhn/Bathbot/blob/HEAD/bathbot/src/commands/osu/cards.rs)). Icons are parsed by hand
  from SVG `viewBox` + `d=` ([svg.rs](https://github.com/MaxOhn/Bathbot/blob/HEAD/bathbot-cards/src/svg.rs)); backgrounds are
  `fs::read` per render ([builder/card/mod.rs](https://github.com/MaxOhn/Bathbot/blob/HEAD/bathbot-cards/src/builder/card/mod.rs)).
- Cost of skia: CI installs `libfontconfig1-dev libfreetype-dev libpng-dev`
  ([check.yml](https://github.com/MaxOhn/Bathbot/blob/HEAD/.github/workflows/check.yml)) and the history has a run of
  "ci: begone/come back fontconfig" commits (2026-02-07).
- Config: `.env` + dotenvy into a `OnceLock<BotConfig>`; DB pool is `connect_lazy`; migrations are sqlx-cli files
  ([bathbot-psql/migrations](https://github.com/MaxOhn/Bathbot/tree/HEAD/bathbot-psql/migrations)), the `migrate` feature
  is not compiled into the bot.

### assyst2 (Jacherr/assyst2)
- README documents the split: `assyst-gateway` forwards **raw JSON text frames** to `assyst-core` over a Unix socket; both
  use jemalloc ([README](https://github.com/Jacherr/assyst2/blob/HEAD/README.md),
  [gateway main.rs](https://github.com/Jacherr/assyst2/blob/HEAD/assyst-gateway/src/main.rs),
  [pipe/mod.rs](https://github.com/Jacherr/assyst2/blob/HEAD/assyst-common/src/pipe/mod.rs)).
- The gateway uses `try_send` into a bounded channel; commit
  [f6f3c86](https://github.com/Jacherr/assyst2/commit/f6f3c86) switched `unbounded_channel` to `channel(10)` ("experimental
  buffer limit"), i.e. events are now dropped under load.
- `assyst-cache` exists only to keep guild ids that arrive once per identify, so core restarts do not lose them
  ([README](https://github.com/Jacherr/assyst2/blob/HEAD/assyst-cache/README.md)).
- Image work is delegated to the external Flux CLI through
  [assyst-flux-iface](https://github.com/Jacherr/assyst2/tree/HEAD/assyst-flux-iface); no Docker, Linux-only assert.
- Tracks twilight `git` master in the workspace manifest ([Cargo.toml](https://github.com/Jacherr/assyst2/blob/HEAD/Cargo.toml)).

### Hourai (james7132/Hourai) - rewritten July 2026
- One binary; shards via `create_recommended`; each shard loop does `cache.update` then `spawn(consume_event)`; presence
  events skip the heavy path ([main.rs](https://github.com/james7132/Hourai/blob/HEAD/crates/hourai-logger/src/main.rs)).
- Uses the twilight HTTP proxy exactly as intended: `.proxy(url, true).ratelimiter(None)`
  ([init.rs](https://github.com/james7132/Hourai/blob/HEAD/crates/hourai/src/init.rs)).
- In-memory cache is a stripped fork that stores only guild ids, presences and pending members
  ([cache/mod.rs](https://github.com/james7132/Hourai/blob/HEAD/crates/hourai/src/cache/mod.rs)); online counts are flushed
  to Redis every 60 s (`flush_online`), a good batching pattern for write-heavy counters.
- Postgres migration smoke test gated on `DATABASE_URL`
  ([tests/migrations.rs](https://github.com/james7132/Hourai/blob/HEAD/crates/storage/sql/tests/migrations.rs));
  `clippy::unwrap_used/expect_used/panic = warn` workspace lints ([Cargo.toml](https://github.com/james7132/Hourai/blob/HEAD/Cargo.toml)).
- Resume support (`Cluster::down_resumable`, 2021 commit c835fd1) was dropped in the rewrite.

### robbb, ferrisbot, Spoticord, scripty, compiler-bot (serenity family)
- All single-process; useful for ops, not for gateway design. robbb:
  [monitoring.rs](https://github.com/unixporn/robbb/blob/HEAD/crates/robbb/src/monitoring.rs), Pyroscope + Honeycomb in
  [main.rs](https://github.com/unixporn/robbb/blob/HEAD/crates/robbb/src/main.rs); May 2026 commits "avoid re-running ready
  code on reconnect", "fix high cardinality of ratelimit metric", "improve ratelimit adherence by increased caching".
- ferrisbot has the only Kubernetes manifests in the set: a StatefulSet with a Litestream init container restoring SQLite,
  kustomize + Flux + SOPS ([ferrisbot.yaml](https://github.com/rust-community-discord/ferrisbot-for-discord/blob/HEAD/deploy/prod/ferrisbot/ferrisbot.yaml)),
  a cargo-chef Containerfile with `SQLX_OFFLINE=true`
  ([Containerfile](https://github.com/rust-community-discord/ferrisbot-for-discord/blob/HEAD/Containerfile)), figment
  env+toml config, tag-triggered GHCR deploy ([deploy.yml](https://github.com/rust-community-discord/ferrisbot-for-discord/blob/HEAD/.github/workflows/deploy.yml)).
- scripty: 45 forward-only sqlx migrations include
  `..._add_timezone_to_timestamps_because_sqlx_doesn't_support_without.sql`, `..._fix_table_types.sql`,
  `..._convert_to_not_null.sql` ([migrations](https://github.com/scripty-bot/scripty/tree/HEAD/migrations)); deadpool-redis
  pool of 128 with explicit create/recycle/wait timeouts ([init.rs](https://github.com/scripty-bot/scripty/blob/HEAD/scripty_redis/src/init.rs));
  Dockerfile uses mold and a non-root user ([Dockerfile](https://github.com/scripty-bot/scripty/blob/HEAD/Dockerfile)).
- Spoticord embeds diesel migrations and runs them at startup
  ([migrations.rs](https://github.com/SpoticordMusic/spoticord/blob/HEAD/spoticord_database/src/migrations.rs)).
- compiler-bot: no DB, LRU caches, parser/boilerplate unit tests ([src/tests](https://github.com/Headline/discord-compiler-bot/tree/HEAD/src/tests)).

## 5. twilight services and examples (primary references for Pinchora's multi-pod layer)

- **twilight 0.18.0** was released 2026-11-01 together with twilight-interactions 0.18.0 (`deps: update to twilight 0.18`);
  [CHANGELOG](https://github.com/twilight-rs/twilight/blob/HEAD/twilight-gateway/CHANGELOG.md) deprecates `create_bucket`,
  `create_iterator`, `create_recommended` and the gateway `twilight-http` feature in favour of
  `twilight_gateway::bucket(bucket_id, buckets, total)` + `client.gateway().authed()`. Bathbot, assyst2 and Hourai all still
  call `create_recommended`.
- Sharding API ([lib.rs](https://github.com/twilight-rs/twilight/blob/HEAD/twilight-gateway/src/lib.rs)): `bucket` returns a
  consecutive range of shard ids per bucket (doc example: 2 buckets over 25 shards -> 13 + 12).
  `Shard::session()`, `Shard::resume_url()`, `ConfigBuilder::session()`, `ConfigBuilder::resume_url()`,
  `Shard::close(CloseFrame)` exist ([shard.rs](https://github.com/twilight-rs/twilight/blob/HEAD/twilight-gateway/src/shard.rs),
  [config.rs](https://github.com/twilight-rs/twilight/blob/HEAD/twilight-gateway/src/config.rs)); `Session` is serde
  (de)serializable and documented for restart reuse ([session.rs](https://github.com/twilight-rs/twilight/blob/HEAD/twilight-gateway/src/session.rs)).
  Bathbot (0.16) persists only the session, not the resume url.
- [gateway-reshard.rs](https://github.com/twilight-rs/twilight/blob/HEAD/examples/gateway-reshard.rs): `JoinSet` of runners
  that **own** their `Shard`; new shard set is pre-identified (stops at 75 % identified) before swapping. Single-process only.
- [gateway-queue-http.rs](https://github.com/twilight-rs/twilight/blob/HEAD/examples/gateway-queue-http.rs): implements
  `queue::Queue::enqueue(&self, id) -> oneshot::Receiver<()>` by GETting `?shard={id}`. The server
  ([gateway-queue](https://github.com/twilight-rs/gateway-queue)) is one in-memory instance; with `DISCORD_TOKEN` set it
  reads `max_concurrency`/remaining identifies. Last functional bump was Twilight 0.17 (Nov 2025).
- [http-proxy](https://github.com/twilight-rs/http-proxy): per-token ratelimiter that mocks the Discord API
  ([main.rs](https://github.com/twilight-rs/http-proxy/blob/HEAD/src/main.rs)); `metrics` image variant; `DISABLE_HTTP2`
  for high concurrency; `CLIENT_DECAY_TIMEOUT`/`CLIENT_CACHE_MAX_SIZE`. Still on `twilight-http-ratelimiting 0.17`.
  Open issues: [#99](https://github.com/twilight-rs/http-proxy/issues/99) one metric series per interaction token
  (`/interactions/:id/:token/callback`), [#58](https://github.com/twilight-rs/http-proxy/issues/58) webhooks 502,
  closed [#95](https://github.com/twilight-rs/http-proxy/issues/95) and
  [#93](https://github.com/twilight-rs/http-proxy/issues/93): new routes (pins, `/oauth2/token`) return 501/parse errors
  until the proxy is updated.
- [examples/model-webhook-slash.rs](https://github.com/twilight-rs/twilight/blob/HEAD/examples/model-webhook-slash.rs):
  hyper + ed25519-dalek verification over `timestamp || body`, Pong for Ping, response serialized inline.
- [examples/cache-optimization](https://github.com/twilight-rs/twilight/tree/HEAD/examples/cache-optimization): how to
  replace the stock `InMemoryCache` with a custom slim cache for chosen event types.

## 6. Lessons for Pinchora

### 6.1 Patterns worth copying

1. **Shard worker = StatefulSet pod owning a `bucket`.** Pod ordinal -> `bucket_id`, replica count -> `buckets`,
   `TOTAL_SHARDS` -> config. Each shard runs in its own task that *owns* the `Shard` (twilight
   [gateway-reshard.rs](https://github.com/twilight-rs/twilight/blob/HEAD/examples/gateway-reshard.rs)); no `Arc<Mutex<Shard>>`
   (see pitfall 2). Inferred: pin `TOTAL_SHARDS` in a ConfigMap instead of calling `/gateway/bot` per pod, otherwise two
   pods booting across a recommended-count change disagree on the total.
2. **Shared identify queue**: run `twilight-rs/gateway-queue` as a 1-replica Deployment, give every pod a `Queue` impl copied
   from [gateway-queue-http.rs](https://github.com/twilight-rs/twilight/blob/HEAD/examples/gateway-queue-http.rs), pass the shard
   id (`?shard=`) and set `DISCORD_TOKEN` on the queue so `max_concurrency` bucketing works. Without it, N pods' in-memory
   queues collide on Discord's 5 s identify window (gateway-queue README).
3. **Resume across rolling deploys** (Bathbot
   [cold_resume.rs](https://github.com/MaxOhn/Bathbot/blob/HEAD/bathbot-cache/src/cache/cold_resume.rs),
   [discord.rs `down_resumable`](https://github.com/MaxOhn/Bathbot/blob/HEAD/bathbot/src/core/context/discord.rs)): on SIGTERM
   send `CloseFrame::RESUME`, drain the shard, store `(Session, resume_url)` **per shard id** in Redis with a short TTL
   (Bathbot: 240 s; pods only read their own keys), feed `ConfigBuilder::session/resume_url` at boot. Set
   `terminationGracePeriodSeconds` above the drain time. Keep a fallback: a failed resume must degrade to a normal identify.
4. **Per-event `tokio::spawn` with `EventTypeFlags` filtering** so heavy handlers never block a shard
   ([Bathbot events/mod.rs](https://github.com/MaxOhn/Bathbot/blob/HEAD/bathbot/src/core/events/mod.rs), twilight example comment).
5. **Model cache misses as a first-class case** (`UncachedGuild`/`UncachedChannel` in the same file) instead of
   unwrapping cache reads.
6. **Defer first, render second.** Bathbot defers by command flag before running
   ([command.rs](https://github.com/MaxOhn/Bathbot/blob/HEAD/bathbot/src/core/events/interaction/command.rs)); for HTTP
   interactions this is mandatory because the first response must land within 3 s (docs). Rank cards should always take
   the deferred path, then PATCH `@original`.
7. **Command metadata as flags + per-user buckets** (defer, ephemeral, guild-only, owner-only, bucket id) registered with a
   distributed slice ([slash/mod.rs](https://github.com/MaxOhn/Bathbot/blob/HEAD/bathbot-macros/src/slash/mod.rs)). Pinchora's
   twilight-interactions derive covers parsing; the flags/bucket layer is still worth copying.
8. **Typestate card builder + bytes-in, PNG-out signature** ([card.rs](https://github.com/MaxOhn/Bathbot/blob/HEAD/bathbot-cards/src/card.rs)):
   the renderer takes already-fetched avatar bytes and static assets, does no network or DB work, and is trivially testable.
   Fetch avatar and secondary assets concurrently with `tokio::join!` before rendering.
9. **Batch hot counters**: Hourai flushes derived state to Redis on a 60 s ticker
   ([main.rs `flush_online`](https://github.com/james7132/Hourai/blob/HEAD/crates/hourai-logger/src/main.rs)). XP/message
   counters should follow the same shape (Redis incr per message, periodic flush to Postgres).
10. **Ops kit**: ferrisbot's cargo-chef Containerfile + `SQLX_OFFLINE=true`, tag-triggered GHCR push, kustomize layout
    ([deploy/prod/ferrisbot](https://github.com/rust-community-discord/ferrisbot-for-discord/tree/HEAD/deploy/prod/ferrisbot));
    commit `.sqlx/` (Bathbot, robbb, scripty all do); Hourai's `DATABASE_URL`-gated migration test; Hourai's workspace
    `unwrap/expect/panic = warn` lints; scripty's Redis pool with explicit timeouts.
11. **HTTP proxy wiring**: `Client::builder().proxy(url, true).ratelimiter(None)` as in Hourai
    [init.rs](https://github.com/james7132/Hourai/blob/HEAD/crates/hourai/src/init.rs); the proxy needs long client timeouts
    because it queues (http-proxy README).
12. **jemalloc on Linux** for the gateway/render pods (assyst2 sets it in both binaries with a comment about event rate).
    Inferred: image-heavy pods fragment glibc malloc; use `tikv-jemallocator` (assyst2 uses the older `jemallocator` crate).

### 6.2 Pitfalls the surveyed projects hit

1. **Resume does not replay GUILD_CREATE** (gateway semantics, docs). With an in-memory cache, a resumed process starts
   empty. Bathbot persisted a guild->shard map ([#397](https://github.com/MaxOhn/Bathbot/issues/397),
   [#398](https://github.com/MaxOhn/Bathbot/issues/398)) and built a Redis cache; assyst2 built a separate cache process.
   Decide explicitly (see 6.3-B).
2. **Shutdown ownership bug.** Bathbot panicked with "exactly one strong reference" because runners still held the
   `Arc<Mutex<Shard>>` ([#1000](https://github.com/MaxOhn/Bathbot/issues/1000), fix
   [215b37c](https://github.com/MaxOhn/Bathbot/commit/215b37c), then [71ebd40](https://github.com/MaxOhn/Bathbot/commit/71ebd40)
   "dont immediately wrap shards"). Cancel -> task returns the `Shard` -> only then close/serialize.
3. **Unbounded queues between stages** grow without limit; assyst2 had to bound its gateway->core channel
   ([f6f3c86](https://github.com/Jacherr/assyst2/commit/f6f3c86)) and the bounded version now drops events. XP counting
   tolerates drops, economy events do not: choose per event class.
4. **Deadlocks keep recurring**: Bathbot [2f3a134](https://github.com/MaxOhn/Bathbot/commit/2f3a134) and
   [af53854](https://github.com/MaxOhn/Bathbot/commit/af53854) (Sep 2026), assyst2 [56a04d0](https://github.com/Jacherr/assyst2/commit/56a04d0)
   "fix log deadlock", Hourai 2021 deadlocks. Never hold a lock across `.await`; prefer channels/atomics.
5. **Concurrent-map crate regression**: Bathbot pins `papaya = "=0.1.8"` with a comment about a
   `len.is_power_of_two()` panic in 0.2.1 ([Cargo.toml](https://github.com/MaxOhn/Bathbot/blob/HEAD/Cargo.toml)). Pin and
   canary-test concurrency primitives; Pinchora's workspace uses moka.
6. **Ready/reconnect re-runs init code.** robbb had to "avoid re-running ready code on reconnect" (commit
   [440bd4d](https://github.com/unixporn/robbb/commit/440bd4d)). Keep Ready/GuildCreate/Resumed handlers idempotent.
7. **Metric label cardinality** bit robbb ([4af9f20](https://github.com/unixporn/robbb/commit/4af9f20)) and http-proxy
   (#99). Never label with ids, tokens or raw paths.
8. **Rate limits at the app layer** needed repeated fixes in Bathbot ("fix rate limiting" 2025-04, render limits) and robbb
   (cache more to stay under limits). Budget per feature and per user from the start.
9. **Long-lived side connections need supervision**: Bathbot "supervise o!rdr websocket and reconnect on stalled event feed"
   ([13e421b](https://github.com/MaxOhn/Bathbot/commit/13e421b)). Same applies to any Redis pub/sub subscription
   (fred [#367](https://github.com/aembke/fred.rs/issues/367) reports missing pub/sub events).
10. **Reshard churn**: Bathbot autoresharded every 12 h, then lengthened it to 23 h
    ([b56f5c8](https://github.com/MaxOhn/Bathbot/commit/b56f5c8), [7e34651](https://github.com/MaxOhn/Bathbot/commit/7e34651)).
    On K8s the equivalent is a planned rolling change of `TOTAL_SHARDS`, alerted on guilds-per-shard.
11. **Native deps in CI**: skia/fontconfig toggling in Bathbot's history. resvg + fontdb are pure Rust, but fonts must be
    shipped in the image (ferrisbot copies `assets` in its Containerfile).
12. **Schema regrets**: scripty added timestamptz columns late and several "fix types / not null" migrations. Use
    `timestamptz` and `NOT NULL` from the first migration. Bathbot re-registers all commands on every boot
    ([main.rs](https://github.com/MaxOhn/Bathbot/blob/HEAD/bathbot/src/main.rs)); Pinchora's separate `register-commands`
    subcommand is the better design (add a hash check to skip no-op registrations).
13. **Forks and git deps** raise upgrade cost: Bathbot depends on git forks of twilight-interactions (`with-help-16`), rosu-v2,
    plotters-skia; assyst2 tracks twilight master. Pinchora starts on released 0.18 crates; keep it that way.

### 6.3 Where the pre-decided stack looks wrong or needs adjusting

- **A. fred 10 (Redis) is a risk.** crates.io shows fred 10.1.0 published 2025-02-27 with no release since
  ([repo](https://github.com/aembke/fred.rs)). Open bugs include
  [#383](https://github.com/aembke/fred.rs/issues/383) (command buffer counter leaks after a backpressure error, later
  lone commands are never flushed, 2026-10-12), [#367](https://github.com/aembke/fred.rs/issues/367) (pub/sub events go
  missing), [#363](https://github.com/aembke/fred.rs/issues/363) (failed master retried forever),
  [#307](https://github.com/aembke/fred.rs/issues/307) (disconnect not always detected). A fix PR
  [#388](https://github.com/aembke/fred.rs/pull/388) is unmerged and a 2026-11-02 comment asks whether the project is
  being archived. Alternatives with active releases: `redis` 1.8.1 (2026-10-30, `ConnectionManager`), `deadpool-redis`
  0.23.1 (used by scripty), `bb8-redis` (used by Bathbot). Pinchora's session store, cooldowns and XP counters need only
  GET/SET/INCR/EXPIRE/sorted sets/pub-sub, so a small `pinchora-cache` trait makes the client swappable. Recommend
  switching before code depends on fred specifics. Pub/sub is the weakest point: if you need cross-pod fan-out, plan a
  heartbeat/resync rather than trusting delivery.
- **B. In-memory twilight cache + resume.** If shard pods resume after restart, `twilight-cache-inmemory` stays empty until
  GUILD_CREATE (which resume does not send). Options: (1) keep guild config in Postgres/Redis and treat the Discord cache as
  optional, filling misses lazily over REST behind a moka TTL (inferred; cheapest, recommended); (2) persist the minimal
  cache in Redis like Bathbot (expensive: custom rkyv models); (3) skip resume for process restarts, accept identify
  cost and lost events. Whatever is chosen, restrict cached resources (Hourai keeps only guild/member/presence ids;
  [cache-optimization](https://github.com/twilight-rs/twilight/tree/HEAD/examples/cache-optimization)). Note `GUILD_PRESENCES`,
  `GUILD_MEMBERS` and `MESSAGE_CONTENT` are privileged; XP only needs message metadata, so avoid them unless a feature needs them.
- **C. Workspace manifest uses deprecated features.** The current `/Users/apeiron/Development/pinchora/Cargo.toml` enables
  `twilight-gateway`'s `twilight-http` feature (deprecated in 0.18, removal planned) and a separate
  `twilight-gateway-queue = "0.18"` (already re-exported as `twilight_gateway::queue`; separate dep is fine but optional).
  Fetch shard count and `session_start_limit` with `client.gateway().authed()` and shard via `bucket()`.
- **D. HTTP interactions mode is untested ground and changes how stateful UI works.** Discord stops delivering
  `INTERACTION_CREATE` over the gateway once an endpoint URL is set (docs), so slash commands, buttons, modals and
  autocomplete all hit the HTTP server. Bathbot-style pagination/collectors use in-process state (`twilight-standby`,
  `active_msgs`) which breaks across stateless replicas: encode state in `custom_id` or store it in Redis with TTL.
  Follow-ups still go through REST using the interaction token. Verify ed25519 over `timestamp || body` before parsing
  (twilight example); `ed25519-dalek 3.0.0` is released, the example is on 2.x.
- **E. http-proxy caveats.** Run exactly 1 replica (inferred: ratelimit state is in-process); expect 501s for brand-new
  routes until the proxy is bumped (it is still on ratelimiting 0.17 while the bot is on 0.18); do not ship the `metrics`
  image unpatched if interaction callbacks pass through it (#99). Discord's docs state interaction endpoints are not bound to
  the global limit, so consider a second, **direct** `twilight_http::Client` for interaction callbacks/follow-ups and use
  the proxy for bot-token calls (verify; inferred benefit: fewer hops, no per-token metric series, no proxy SPOF for
  interaction responses). Discord's invalid-request ban (10 000 per 10 min, docs) is per IP, which a shared K8s egress makes
  easy to trip; the proxy centralises 429 handling.
- **F. resvg/tiny-skia is plausible but unproven in this set.** Evidence from resvg's `usvg::Options`
  ([options.rs](https://github.com/linebender/resvg/blob/HEAD/crates/usvg/src/parser/options.rs)): `fontdb: Arc<fontdb::Database>`,
  `font_resolver`, `image_href_resolver`. Applied lessons: load fonts into one shared `fontdb` at startup; supply avatars
  through `image_href_resolver` or a data URI instead of files; XML-escape every user-controlled string placed in a
  template; pre-load backgrounds (Bathbot reads them from disk per render); run rasterisation in `spawn_blocking` behind a
  semaphore; cache avatar bytes by hash and finished cards by (user, level, theme). For the pixel-art house game, composing
  sprites on a `tiny_skia::Pixmap`/`image` buffer with nearest-neighbour scaling is probably simpler than SVG (inferred;
  Bathbot's `draw_image` canvas approach is the analogue). Add golden-PNG tests; no surveyed bot has them.
- **G. sqlx 0.9 is fine** (0.9.0 released 2026-05-06; repo moved to `transact-rs/sqlx`; MSRV 1.94 matches the workspace
  `rust-version`). Surveyed bots are on 0.6-0.8, so little copyable 0.9 code exists. Keep up/down or forward-only consistently
  (Bathbot reversible, scripty forward-only) and run `migrate` as a K8s Job/init step, not from every pod
  (robbb and Spoticord migrate at startup, fine for one replica only).

## 7. Top recommendations (ordered)

1. Replace or abstract **fred** now (`redis` crate or deadpool-redis behind a `pinchora-cache` trait).
2. Shard with `twilight_gateway::bucket()` + `client.gateway().authed()`; drop the deprecated `twilight-http` feature.
3. Run one shared `gateway-queue` Deployment; every shard pod uses an HTTP `Queue` impl with `?shard=`.
4. Implement resume as per-shard `(Session, resume_url)` in Redis with short TTL, `CloseFrame::RESUME` on SIGTERM,
   shard-owning tasks, and a graceful identify fallback.
5. Make guild/permission data not depend on the in-memory gateway cache surviving a resume (lazy REST fill + moka, or
   Postgres/Redis as source of truth).
6. For HTTP interactions: defer-first, stateless component state (`custom_id`/Redis), ed25519 verification, direct client
   for interaction follow-ups, `register-commands` only with change detection.
7. Keep http-proxy at 1 replica, long client timeouts, no raw-path metric labels; track its 0.18 bump.
8. Renderer = pure function (bytes in, PNG out) with shared `fontdb`, preloaded assets, `spawn_blocking` + semaphore,
   avatar and card caches, escaped templates, golden tests.
9. Batch hot writes (Redis counters -> periodic Postgres flush); decide drop-vs-block policy per event class and bound every
   internal channel.
10. Hygiene: `unwrap/expect/panic` lints, `.sqlx/` committed + `SQLX_OFFLINE`, migration smoke test in CI, cargo-chef image,
    jemalloc for render pods, idempotent Ready/GuildCreate handlers, pinned concurrency crates, no id/token metric labels.

## 8. Not verified

- Where ferrisbot actually uses `image`/`imageproc`/`ab_glyph`; Spoticord's exact sharding mode; Bathbot's per-render
  threading model for skia (spawn_blocking not confirmed); gateway-queue behaviour when its single instance restarts.
- Discord-side numbers marked "docs" above.
