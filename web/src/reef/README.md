# Reef client engine and multiplayer (R02–R24)

Open `/me/house?play=1` or `/house/<user-id>?play=1` with an existing authenticated session and a saved Discord character. The player's own appearance is used when visiting. The flag starts Play; View keeps the existing reference viewer and Compare with Discord. The room picker in Controls preserves the flag.

Run from `web/`:

```sh
npm ci
npm run dev
# Existing Rust backend defaults to localhost:8090; Vite serves localhost:5173.
npm run lint
npm run typecheck
npx vitest run
npm run build
npm run reef:budget
npm run e2e:reef
```

No new npm dependencies. `reef:assets` runs automatically before dev/build and uses Python 3's standard library. A source hash skips unchanged builds. It writes disposable `public/assets/reef/` pages and the small checked-in manifest URL / palette tables; originals under `assets/` are never modified. Build output is served by the existing Rust SPA static-file handler. Both page and manifest URLs are content-hashed because that handler marks `/assets/` immutable. Vite explicitly does not load dotenv files.

## Contracts

- Original atlas crops are shelf-packed without resizing or alpha compositing. The builder decodes its output and verifies all 953 crops; tests also compare the 96 directional rows of the 24 provided sheets. JSON distinguishes look IDs from sheet numbers.
- A 16 MiB decoded-page LRU deduplicates requests, retries failures, and counts actual response-body bytes. Only floor/wall textures, own avatar and metadata block interaction. Furniture/pets stream sequentially with visible loading status. Active scene and animation bitmaps are released on room changes/unmount; the LRU measurement is not total browser memory.
- Floor/wall/rug/static-shadow preparation uses the existing software renderer in a worker. All seven Rust scene fixtures, including editing overlays, are pixel-identical through the cached software compositor. Atlas standing sprites have binary alpha; live drawing uses native bitmaps with smoothing disabled.
- Standing objects and fractional actors share the existing footprint dependency graph, overlap rank, stable ready queue and cycle fallback. `fixtures/order.json` adds fractional wide-furniture cases. Alpha picking follows reverse painter order.
- Camera scaling and translations are integers in physical pixels. CSS size derives from the backing resolution and DPR; the floor inverse retains its `-1` boundary. Drag/pinch/cancel consume taps. Keyboard arrows, tile coordinates, and a furniture/action list provide pointer-free control.
- Navigation uses four neighbours, Manhattan distance, `(f, h, insertion)` ties and no diagonals. `fixtures/navigation.json` is the shared Rust/TypeScript contract. Empty/full rooms, blocked and unreachable goals, replan-without-corner-cutting, approach tiles, sit/stand and use completion are covered.
- Rugs/walls are walkable; floor and loose surface items default to solid. Three pilot seats are allowlisted (`pebblerest_stool`, `duneplait_stool`, `mosshem_pouf`); game-tagged items play the use animation. Seat approaches and directions transpose when flipped. Unknown seats remain solid. All other furniture offers walking to a reachable adjacent tile.
- All 32 frames per look are composed once with the fixed foot anchor. Key skin/hair/top/bottom ramps are sourced from the bot; custom OKLCH ramps are checked against 136 fixtures generated from the actual Rust palette implementation. The palette file in this checkout is `assets/sprites/avatar_palette.hex`.
- Accessories use original procedural pixels obtained by comparing the existing character endpoint's base and single-accessory images. These layers are positioned by metadata, ordered by slot, mirrored by facing and padded beyond the character cell. Faces are occluded while facing away; use poses put held items away. Missing hand landmarks retain a saved anchor and produce a visible warning; unknown accessories are named in a warning. No replacement art is generated.

## Play presentation

Play fills `100dvh` with the header in its own row and restores page scrolling on exit. Desktop has a 264px controls panel; touch starts with the accessible drawer closed. Opening the drawer focuses its close button; Escape/close returns focus to the trigger. The room picker, keyboard instructions, coordinates and complete furniture action list live there. English/Turkish UI follows the browser-language pattern used elsewhere in the web app.

The camera fits the room with 24 CSS pixels of padding at the largest integer **physical-pixel** scale from 1–4, then eases toward the player without cropping room axes that fit. Walking follows with a 24 CSS pixel dead zone; the centre icon explicitly centres the player, including at the fitted scale. Dragging suspends following; walking or the centre icon resumes it. If the room cannot fit even at ×1 on a low-DPR viewport, native pixels take priority and the camera keeps the player in view. Resize/orientation changes refit. Reduced motion jumps the camera and disables bobbing, marker pulses and dust. Footstep dust reuses 12 particles; shadows retain the room renderer's shape/alpha and draw order. Only Play requests a transparent backdrop; the reference viewer's golden pixels are unchanged.

`play.spec.ts` saves six CSS-resolution screenshots in `test-results/reef/`: `play-phone-{open,walk,seated}.png` at 390×844 and `play-desktop-{open,walk,seated}.png` at 1280×800. It waits for streamed art, fonts and opening camera motion; checks room fit, player visibility, header clearance, no page overflow, focus restoration, integer zoom, reduced motion, gestures and the Turkish seat bubble. `perf.spec.ts` still runs once, in the phone project, with its original 4× throttle and unchanged 16.7ms / 16MiB limits.

## Measurements and limits

`reef:budget` sums uncompressed production entry/Play/worker JS and CSS, the complete manifest, the largest initial texture pages and four-direction look, pixel fonts, a full 650-item catalogue allowance, and 300 KB for API bodies/appearance PNGs/headers. It fails over 2,000,000 bytes. Flagged Play uses system body text to avoid downloading the two ~600 KB Noto TTFs; other routes retain their fonts. This is a conservative static envelope, not a measured HTTP capture.

`tests/reef/play.spec.ts` uses the production build and mocked authenticated API data with the complete furniture catalogue and real original art/fonts. `tests/reef/perf.spec.ts` exercises a 14×12 scene, 60 furnishings, and 16/32 animated actors at 4× Chromium CPU throttling, reporting draw CPU p95 and observed rAF FPS. `perf.test.ts` measures only ordering/anchor CPU cost and must not be reported as browser FPS.

This environment's Chromium launch fails at macOS Mach-port registration (`Permission denied`), so browser visuals, HTTP-transfer measurements, total resident memory, and physical midrange-phone 60 FPS remain unverified. The browser specs are retained for the integration environment.

Local preview remains available when multiplayer cannot connect. Seat heights and directional accessory placements are pilot content awaiting visual integration review; no chair-arm foreground masks are certified. The sheet's eye/shoe colours are retained; four key channels are recoloured. Full rooms without a free entry tile remain viewable. Multiplayer refreshes layouts from server invalidations; offline preview refreshes on reload or room change.

## Private-house multiplayer (R09–R11)

Play attempts a fenced room connection across pods; an unavailable socket leaves the local preview usable.
Two separate browser profiles, signed in as different accounts with saved characters, can open
/house/&lt;owner-id&gt;?play=1. The owner can use /me/house?play=1. Hidden or locked houses and
bidirectional guestbook blocks refuse visitors. There is no friends-only policy; house and global Reef sanctions apply. Owners bypass visitor privacy checks. Guild plazas are not admitted.

- GET /api/reef/bootstrap: authenticated saved appearance; visitors do not need to own a house.
- POST /api/reef/socket-ticket: authenticated, CSRF-protected, Redis single-use ticket, 30-second TTL.
- GET /api/reef/ws: session cookie plus one exact configured Origin; first frame authenticates within five seconds.
- Every JSON message has protocol_version (1), request_id, room_epoch and sequence.
  Client request IDs are 1–64 characters; unsolicited server events use an empty request ID.
  Each direction has its own increasing connection sequence starting at 1. Connection control messages use an empty epoch;
  snapshots supply the actor's Postgres fencing token as a string. Room keys are house:OWNER:INDEX; all database IDs are strings.
- Client types: authenticate {ticket}, join {room}, leave, snapshot, move_to {tile:[x,y]},
  stop, use {placement_id,revision}, stand, ping.
- Server types: authenticated, snapshot, presence, path, correction, posture,
  layout_changed, left, ping, error {reason}. Command responses echo request IDs.
  Correction updates are compact tuples: [user_id,x,y,tile_x,tile_y,direction,state].
  Snapshots/presence carry saved character codes and plain-text names; appearances are not resent every tick.
- Limits: 4 KiB incoming frames and messages, 64 queued outbound events / actor commands, 16 occupants,
  movement 5/s with burst 10, furniture 2/s with burst 2 and a 500 ms account use cooldown, six joins per rolling minute.
  An extra 30/s burst-60 envelope budget bounds other commands. Account budgets survive reconnects.
- Server movement uses monotonic time at 3 tiles/s; corrections run at 10 Hz and the browser interpolates at rAF frequency
  with a 100 ms buffer. Stop/replan finishes the current edge without crossing a blocked corner.
  Server-validated seats have one occupant; moving/standing/leaving releases the seat.
- Heartbeats run every 15 seconds, expire after 45 seconds without a valid client frame, and revalidate the session.
  A new authenticated controller closes the old one with controller_replaced; the old tab does not reconnect.
  Gaps request snapshots; reconnect uses bounded exponential backoff with jitter.
- House editor, surface and upgrade changes write a transactional Postgres outbox. Actors reload authoritative
  DB layouts before movement/use and every second. Layout changes stop routes, release seats and relocate blocked
  actors to the deterministic free entry. The client refetches the room.
- Redis/DB failures pause gameplay by closing affected sockets. Bounded queues and two-second write deadlines
  evict slow clients with `slow_client`; reconnect starts with a fresh snapshot.

## Multi-pod routing and recovery (R21–R24)

Apply migration `0070` before starting the new web pods. Set **the same `PINCHORA_ENV` on bot and web pods**
(for example `production`, `staging`, or a test-run name). Only letters, digits, `_` and `-` are accepted;
when unset it is `default`. Reef keys/channels use `pinchora:<env>:db<N>:reef:*`. Existing settings/XP/house
invalidation uses `pinchora:<env>:db<N>:invalidate`; the global `pinchora:invalidate` channel is no longer used.
Redis Pub/Sub ignores SELECT, so the database number is explicitly part of these channel names.
Roll bot and web together for the channel-name change; older pods will retain their normal cache TTL fallback.
Use separate Postgres databases for separate deployment environments.

Each subscribed frontend relays intentions to one room actor. A three-second Redis lease gates publication
atomically; a renewable Postgres lease and monotonically increasing fencing sequence survive Redis key loss.
Persistent chat locks and checks the Postgres fence inside its evidence transaction. Stale owners cannot renew,
publish, or persist chat after a successor takes over. Account controllers use compare-and-renew five-second
leases, checked before actor commands and frontend delivery. Expiring room-location records remove ghosts even
when a transition's leave message is lost. Logout, blocks and consent changes reach other pods.
`WebState::drain_reef()` cancels room/relay tasks immediately; ownership becomes available after lease expiry.
There is no persisted footstep stream: failover returns players to deterministic spawn via a new snapshot.

Outbox triggers cover placements, rooms, and house tier changes from both bot and web, including direct surface/
flip updates and bulk restore/clear. Multiple row writes in one transaction produce one house notification;
rollback produces none. Each active room relay polls committed event IDs and publishes missing IDs. Subscribers
and room actors deduplicate IDs; scans do not use `max(id)` because transactions can commit out of order.
`layout_changed` carries a string `event_id` and is applied once per committed transaction per live room epoch,
even if geometry refreshes coalesce. This is logical exactly-once application over retryable delivery, not a claim
that Pub/Sub itself delivers exactly once. New/restarted pods use the current snapshot as their baseline.
The hourly retention task prunes outbox rows older than one day in batches and stale fencing rows.

Relay delivery counters preserve gaps through the socket writer. The client pauses intentions, requests one
snapshot, and reconnects if recovery has not completed within three seconds. A missing command acknowledgment
closes the affected connection after three seconds; chat is never automatically replayed. Duplicate routed
command sequences return a snapshot without repeating their side effects. Full queues close with `slow_client`.
Frontend connection rate budgets remain per pod; room ownership and account controller exclusivity are global.

`cargo test -p pinchora-web --test reef -- --test-threads=2` includes two independent app instances sharing the
assigned Postgres/Redis, cross-pod presence/movement/chat, controller and consent replacement, transactional
outbox rollback/deduplication, stopped-owner failover, and Redis restart recovery. The restart test launches a
private `redis-server` child on a free loopback port; it never restarts the configured shared service or containers.

Rust and TS both read navigation-manifest.json, the seven A* cases in fixtures/navigation.json,
and the three collision/interaction cases in fixtures/navigation-layout.json.
The Rust integration binary reef runs real HTTP/WebSocket clients against the configured test services.
The lead can run the two-browser journey with the regular real-server config, after building the Rust binary
and frontend with the assigned test DB/Redis environment:

    npx playwright test tests/e2e/reef-multiplayer.spec.ts

The spec seeds two stable test accounts (IDs ending 091/092), then checks presence, walking, seat contention,
reconnect and replacement controllers. It uses neither mocked WebSockets nor the preview-only Reef config.

## Chat safety (R13–R20)

Migration 0065 adds `reef_consents`, `reef_sanctions`, `reef_chat_evidence`,
`reef_chat_recipients`, `reef_reports`, and `reef_moderator_audit`. `guestbook_blocks`
is the general social block source; both directions apply to chat and visits/signing.
`Db::socially_blocked` is the shared gate for future doorbell/invitation services (these do not yet exist).

- `GET /api/reef/bootstrap`: consent version, eligibility, safe name, operator capability.
- `PUT /api/reef/consent`: version, `under_13`/`13_plus`, accepted. No birth date is stored.
- `GET /api/reef/blocks`, `PUT/DELETE /api/reef/blocks/{user}`.
- `POST /api/reef/reports`: delivered message id and reason; returns a receipt.
- `GET /api/reef/reports`: own receipts and decisions; `POST /reports/{id}/appeal`.
- `GET /api/reef/moderation`: up to 100 oldest pending/appealed reports, restricted to own house or operators, every evidence read audited.
- `POST /api/reef/moderation`: target, owner (null is operator-only site scope), optional room_idx,
  kind (mute/kick/ban/no_action/warned/revoke), seconds (1–7,776,000), optional report id.
  The UI applies one-hour house sanctions; kicks prevent reconnect for 60 seconds.
- Socket `chat {text}` produces an accepted `chat` with server message_id, sender_id, name, room,
  text, created_at, and the sender's request_id. Rejections are `error {reason}`. No optimistic delivery/replay.
- `policy_changed` clears cached chat; logout, consent withdrawal, expiry, sanctions and access revocation
  close with a localized client explanation. Blocks never identify the blocking account to the other user.

The hourly retention worker deletes batches with SKIP LOCKED: 7 days ordinary / 90 days reported,
based on message creation time. There is no indefinite legal-hold feature. Report and receipt rows cascade
with expired evidence. Audit records expire after 90 days. All raw evidence stays out of operational logs.
Deletion removes consent, authored evidence, delivery records, reports and audit identities; minimal active
sanctions retain the target id until expiry to prevent deletion/rejoin bypass, following moderation tombstones.

Run `web/tests/e2e/reef-chat.spec.ts` with the real development server and assigned database/Redis.
The lead owns execution of Playwright; the spec uses two contexts and verifies chat, report receipt, queue,
and block revocation. The existing seed resets the two fixture accounts' blocks and sanctions.
