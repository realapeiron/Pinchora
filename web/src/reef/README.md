# Reef client engine, social spaces and games (R02–R49)

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
- Rugs/walls are walkable; floor and loose surface items default to solid. Five seats are allowlisted (`pebblerest_stool`, `duneplait_stool`, `mosshem_pouf`, `reedwink_stool`, `hemloop_chair`); game-tagged items play the use animation. Seat approaches and directions transpose when flipped. Unknown seats remain solid. All other furniture offers walking to a reachable adjacent tile.
- All 32 frames per look are composed once with the fixed foot anchor. Key skin/hair/top/bottom ramps are sourced from the bot; custom OKLCH ramps are checked against 136 fixtures generated from the actual Rust palette implementation. The palette file in this checkout is `assets/sprites/avatar_palette.hex`.
- Accessories use original procedural pixels obtained by comparing the existing character endpoint's base and single-accessory images. These layers are positioned by metadata, ordered by slot, mirrored by facing and padded beyond the character cell. Faces are occluded while facing away; use poses put held items away. Missing hand landmarks retain a saved anchor and produce a visible warning; unknown accessories are named in a warning. No replacement art is generated.

## Play presentation

Play fills `100dvh` with the header in its own row and restores page scrolling on exit. Desktop has a 264px controls panel; touch starts with the accessible drawer closed. Opening the drawer focuses its close button; Escape/close returns focus to the trigger. The room picker, keyboard instructions, coordinates and complete furniture action list live there. English/Turkish UI follows the browser-language pattern used elsewhere in the web app.

The camera fits the room with 24 CSS pixels of padding at the largest integer **physical-pixel** scale from 1–4, then eases toward the player without cropping room axes that fit. Walking follows with a 24 CSS pixel dead zone; the centre icon explicitly centres the player, including at the fitted scale. Dragging suspends following; walking or the centre icon resumes it. If the room cannot fit even at ×1 on a low-DPR viewport, native pixels take priority and the camera keeps the player in view. Resize/orientation changes refit. Reduced motion jumps the camera and disables bobbing, marker pulses and dust. Footstep dust reuses 12 particles; shadows retain the room renderer's shape/alpha and draw order. Only Play requests a transparent backdrop; the reference viewer's golden pixels are unchanged.

`play.spec.ts` saves six CSS-resolution screenshots in `test-results/reef/`: `play-phone-{open,walk,seated}.png` at 390×844 and `play-desktop-{open,walk,seated}.png` at 1280×800. It waits for streamed art, fonts and opening camera motion; checks room fit, player visibility, header clearance, no page overflow, focus restoration, integer zoom, reduced motion, gestures and the Turkish seat bubble. `perf.spec.ts` still runs once, in the phone project, with its original 4× throttle and unchanged 16.7ms / 16MiB limits.

## Measurements and limits

`reef:budget` follows the production build manifest to sum entry/Play/worker JS, every static dependency and CSS, the complete manifest, the largest initial texture pages and four-direction look, pixel fonts, a full 650-item catalogue allowance, and 300 KB for API bodies/appearance PNGs/headers. It fails over 1,985,000 bytes. Flagged Play uses system body text to avoid downloading the two ~600 KB Noto TTFs; other routes retain their fonts. Dashboard routes, game panels and optional Decorate/Social sheets load on demand; chat stays on the initial path. This is a conservative static envelope, not a measured HTTP capture.

`tests/reef/play.spec.ts` uses the production build and mocked authenticated API data with the complete furniture catalogue and real original art/fonts. `tests/reef/perf.spec.ts` exercises a 14×12 scene, 60 furnishings, and 16/32 animated actors at 4× Chromium CPU throttling, reporting draw CPU p95 and observed rAF FPS. `perf.test.ts` measures only ordering/anchor CPU cost and must not be reported as browser FPS.

This environment's Chromium launch fails at macOS Mach-port registration (`Permission denied`), so browser visuals, HTTP-transfer measurements, total resident memory, and physical midrange-phone 60 FPS remain unverified. The browser specs are retained for the integration environment.

Local preview remains available when multiplayer cannot connect. Seat heights and directional accessory placements are pilot content awaiting visual integration review; no chair-arm foreground masks are certified. The sheet's eye/shoe colours are retained; four key channels are recoloured. Full rooms without a free entry tile remain viewable. Multiplayer refreshes layouts from server invalidations; offline preview refreshes on reload or room change.

## Private-house multiplayer (R09–R11)

Play attempts a fenced room connection across pods; an unavailable socket leaves the local preview usable.
Two separate browser profiles, signed in as different accounts with saved characters, can open
/house/&lt;owner-id&gt;?play=1. The owner can use /me/house?play=1. Hidden or locked houses and
bidirectional guestbook blocks refuse visitors. Visit preferences support open, friends/partner, accepted doorbells and closed access. House and global Reef sanctions apply; partners never bypass blocks or visit consent. Guild plaza admission checks live membership and the Plaza module.

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
Renewal runs independently of room work every 500 ms, with a one-second deadline; a failed renewal cancels
in-flight room work. Postgres checks expiry after acquiring the fencing row lock, including after lock waits.
Chat recipients are authorized inside the evidence transaction and checked again at frontend relay delivery,
so a committed remote block or ban also suppresses delayed chat events.

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
Reef subscriptions require Redis 6+ (RESP3), with 128 buffered messages of at most 256 KiB each; overflow,
oversized messages, and disconnects end the subscription and force recovery. The Redis driver's unbounded
RESP2 subscription queue is not used for Reef. Sequence gaps clear chat because a lost policy event cannot
be reconstructed from a room snapshot. Local location indexes contain only locally controlled accounts.

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

## Decorating and social spaces (R25–R37)

Apply migration **0082** before running this code. It adds `houses.edit_revision`,
`reef_edit_receipts`, `reef_space_preferences`, `reef_doorbells`, and `reef_guestbook_evidence`,
a cascading lookup for existing moderation evidence. New personal records are privacy-registered;
receipts expire after seven days. Retention prunes expired doorbells while preserving the latest
minute of abuse counters; both cleanup paths use batches of at most 500 rows.

In Play choose **Decorate**. The sheet shares the existing house operation queue and undo stack.
Choose inventory or an existing piece, position by pointer/tap or numeric coordinates, rotate and
confirm. Canvas arrows move the ghost, R rotates, Enter confirms and Escape cancels selection.
Phones use a bottom sheet; a pan/pinch never places furniture. Owned surfaces and a confirmed
clear are available. Surface purchases remain in the existing house editor. Plaza managers apply
owned surfaces without spending a player's wallet. Five original-art seat foreground bands are
replayed from the same (possibly flipped) sprite after seated actors; ordinary rendering is unchanged.

`POST /api/house/rooms/{idx}/ops` accepts `request_id`, `expected_revision`,
`expected_fingerprint`, and `op`; old `{expected,op}` requests remain compatible. Revision comparison,
inventory changes, a successful retry receipt and outbox writes commit together. Reusing an ID with
another payload is rejected. Identical retries return current authoritative state with `replayed:true`.
The shared PostgreSQL editor lock spans Redis undo advancement. A crash after commit can invalidate
undo history, but cannot repeat the mutation; stale history remains conservatively discarded.

**Social spaces** reuses friends/partners, guestbooks, daily house appreciation and F21 events.
Online presence defaults off and reveals only online status to unblocked friends/partners.
Doorbells require opt-in: pending for two minutes, accepted admission for ten minutes, one request
per pair/minute, six per sender/minute, sixteen pending per host. Privacy and blocks override admission;
closing visits or disabling doorbells revokes grants. A successful in-world visit records a visit with
zero reward; movement/chat/presence never mint currency. Guestbook signing requires chat consent,
safe text, a recent visit and daily uniqueness. Reports use existing evidence/receipts/moderator audit.

Guild plazas use `plaza:GUILD:ROOM:INSTANCE`, a separate fenced lease per instance, and 32 occupants.
`GET /api/reef/guilds/{guild}/plazas` routes to a nonfull instance (up to sixteen ordinary instances).
Passing `instance` pins the group to that instance: full means `409 room_full`, never silent splitting.
Counts are advisory, expire after three seconds, and the actor always enforces capacity.
Every join/command and periodic access check verifies Discord membership; Redis failure closes play.
Editing and scheduling require full management rights or the dashboard **community** access area.
The same scope authorizes guild mute/kick/ban through the existing moderation action endpoint.

Gatherings reuse F21 event rows and RSVP capacity/waitlists. A manager schedules a one-hour room tour
from the panel; the event's location is the exact plaza room key, shared by RSVP and Join links.
The Events module and required roles are checked. This does not create a Discord bridge or public global plaza.

Lead browser acceptance (real backend and mock Discord, no intercepted room APIs):
`npx playwright test tests/e2e/reef-spaces.spec.ts` using `playwright.config.ts` and the assigned
Postgres/Redis environment. Four journeys cover two browsers, desktop/phone placement, shared undo,
doorbell admission, notes/likes, blocking, plaza entry and scheduled RSVP. Screenshots go to
`web/test-results/screens/reef7-*`. Chromium execution and final seat silhouette review remain lead gates.

## Room games and shared economy (R38–R49)

Wave 8 uses migration **0085**, adds no slash roots and adds no dependencies.
`Room activities` is available in house rooms and guild plazas. Usable game furniture opens the
same lobby with a placement binding; the accessible activity menu needs no furniture. The lobby,
Checkers, Chess, Picross, Trivia, game translations and game styles load on demand. The art manifest
omits redundant null seat contacts while preserving all pixels, pose anchors and seated contacts.

- `pinchora-game/src/play/room.rs` adapts the existing Checkers and Chess engines. It owns the
  transport-independent projection; it does not implement alternative movement rules. Checkers
  preserves forced captures and chains; Chess preserves castling, en passant, all promotions,
  repetition, check and draw outcomes. Move clocks are 180/300 seconds, as on Discord.
- `reef_games` holds an explicit `house:…`/`plaza:…` context, persisted deadline and revision.
  `reef_game_players` holds seats, single-use answers and paid amounts. `reef_game_actions` binds
  each account/request ID to its exact payload. Replaying a committed action returns current state;
  reusing that ID for another payload conflicts. Numeric database/user IDs are JSON strings.
- The database session row serializes both transports. It takes player locks first, checks current
  state, writes statistics, spends the shared daily allowance, writes the shell ledger and commits
  the final state/receipt together. WebSocket actions additionally verify the room owner's fence
  inside that transaction. HTTP and WebSocket use the same service and server rules. A one-second
  sweeper settles persisted deadlines without connected browsers; competing pods cannot pay twice.
- Multiplayer **house games never pay shells**, regardless of the number of accounts, host, game
  length or result. Self-join cannot occupy a second seat. Guild-plaza games use the existing reward
  amounts, minimum moves and paid-win caps. This is a private-room farming rule, not a claim to
  identify every alternate account in a public guild. No web wagers or passive income exist.
- `game_reward_daily` replaces the Redis-only `pay_win` allowance for bot and web. Migration seeds
  historical paid wins from the ledger; today's counters survive privacy deletion. A missing counter
  after a same-day deletion stays closed until tomorrow, covering legacy deleted ledgers at cutover. Restart bot and
  web together at cutover so older binaries cannot keep spending the old Redis-only allowance.
- Bot board conclusions now commit rewards and stats with `settle_game`'s state/wager effects.
  Bot Trivia also commits answers' statistics and capped payments with its completed state.
  Picross commits its solved state, stats, daily/streak or capped practice reward in the mark
  transaction. Both clients use `picross_games`; daily tombstones, checks and reduced rewards remain.
  Browser edits carry a monotonic revision as well as the grid, including across practice resets.
  Both clients retire practice pictures that approach the next daily puzzles.
- Trivia reuses EN/TR question content and option permutation. Question IDs, shuffle seeds/order,
  other people's chosen answers, and the correct-slot field are absent before reveal. The server
  allows one answer per participant before the persisted deadline; everyone answering or the
  deadline triggers reveal. Reconnect returns whether your answer was accepted. Room bubbles show
  status only. A room viewer remains a spectator until joining a waiting session.
- Snapshots and actions recheck room access, sanctions, plaza membership/module gating and participant
  blocks. Requests have a 4 KiB ceiling, strict unknown-field parsing and fail-closed Redis budgets:
  creation 12/minute, actions 120/minute, reads 240/minute, Picross marks 180/minute. Existing socket
  frame, sequence, controller, origin and ticket limits remain. Browser POSTs retain CSRF/audit.
- Finished sessions and their action/player children expire after seven days. Reward counters expire
  after the current-day protection window. The privacy registry covers every new personal-data table.
  Removing bound furniture releases the binding on the next action, preserving play in the activity menu.
- Completion invalidates existing `player`, `economy`, `/api/me`, house, inventory and pet caches;
  there is no separate browser wallet or inventory.

Endpoints: `GET/POST /api/reef/rooms/{room}/games`, `GET /api/reef/games/{id}`,
`POST /api/reef/games/{id}/actions`, `POST /api/reef/rooms/{room}/picross`, and
`POST /api/reef/rooms/{room}/picross/marks`. Socket `game_action` carries `game_id` and `payload`
(the same HTTP action body). `game_changed` acknowledges the sender and notifies other viewers by session ID.
The browser's two-second lobby poll also recovers missed events and HTTP changes.

Verification: `crates/pinchora-db/tests/reef_games.rs` injects PostgreSQL failures at the stats and
ledger and final receipt stages, verifies full rollback, retries after a lost committed response, checks shared bot/web
caps and deletion tombstones. `crates/pinchora-web/tests/reef/wave8.rs` exercises real sessions, CSRF,
HTTP/socket parity, stale actions, spectator authorization, Trivia concealment/deadlines and Picross
cross-transport resume. The existing EN/TR bot simulator journeys exercise the changed settlement paths.
`web/tests/e2e/reef-games.spec.ts` contains four real-server two-browser complete-game journeys, including
phone/desktop captures, spectator and reconnect checks, and Picross solved from public clues.
The lead runs Chromium; `npx playwright test tests/e2e/reef-games.spec.ts --list` checks discovery here.
`node web/scripts/measure-reef.mjs` now enforces **1,985,000 bytes** and verifies every game facade stays
outside the cold dependency graph. Its conservative response-body envelope is not a device FPS claim.
