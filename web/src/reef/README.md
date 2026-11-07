# Reef client engine and multiplayer (R02–R12)

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

Play now attempts a single-pod room connection; an unavailable socket leaves the local preview usable.
Two separate browser profiles, signed in as different accounts with saved characters, can open
/house/&lt;owner-id&gt;?play=1. The owner can use /me/house?play=1. Hidden or locked houses and
bidirectional guestbook blocks refuse visitors. There is no friends-only policy or global Reef sanction
table in this checkout. Owners bypass visitor privacy checks. Guild plazas are not admitted.

- GET /api/reef/bootstrap: authenticated saved appearance; visitors do not need to own a house.
- POST /api/reef/socket-ticket: authenticated, CSRF-protected, Redis single-use ticket, 30-second TTL.
- GET /api/reef/ws: session cookie plus one exact configured Origin; first frame authenticates within five seconds.
- Every JSON message has protocol_version (1), request_id, room_epoch and sequence.
  Client request IDs are 1–64 characters; unsolicited server events use an empty request ID.
  Each direction has its own increasing connection sequence starting at 1. Connection control messages use an empty epoch;
  snapshots supply the actor's random epoch. Room keys are house:OWNER:INDEX; all database IDs are strings.
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
- House editor, surface and upgrade changes publish on the existing invalidation channel. Actors reload authoritative
  DB layouts before movement/use and every second, so missed notifications also recover. Layout changes stop routes,
  release seats and relocate blocked actors to the deterministic free entry. The client refetches the room.
  Redis/DB failures close rooms, and full outbound queues evict slow clients.
- This is single-pod authority only. No chat, rewards, new personal-data tables, migrations, outbox or cross-pod fencing.
  The existing shared invalidation channel carries only a house ID hint; each actor rereads its own database.
  Reliable environment-isolated outbox notifications and multi-pod ownership remain Wave 6 work.

Rust and TS both read navigation-manifest.json, the seven A* cases in fixtures/navigation.json,
and the three collision/interaction cases in fixtures/navigation-layout.json.
The Rust integration binary reef runs real HTTP/WebSocket clients against the configured test services.
The lead can run the two-browser journey with the regular real-server config, after building the Rust binary
and frontend with the assigned test DB/Redis environment:

    npx playwright test tests/e2e/reef-multiplayer.spec.ts

The spec seeds two stable test accounts (IDs ending 091/092), then checks presence, walking, seat contention,
reconnect and replacement controllers. It uses neither mocked WebSockets nor the preview-only Reef config.
