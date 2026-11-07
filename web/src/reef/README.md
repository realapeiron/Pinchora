# Reef client engine (R02–R08, R12)

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
- Navigation uses four neighbours, Manhattan distance, `(f, h, insertion)` ties and no diagonals. `fixtures/navigation.json` is the language-neutral contract for a future Rust implementation. Empty/full rooms, blocked and unreachable goals, replan-without-corner-cutting, approach tiles, sit/stand and use completion are covered.
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

This is local client prediction, with no realtime server, persistent position, inventory mutation, game launch, cooldown or rewards. Seat heights and directional accessory placements are pilot content awaiting visual integration review; no chair-arm foreground masks are certified (the initial seats have no arms). The sheet's own eye/shoe colours are retained; the requested four key channels are recoloured. Reduced motion preserves navigation and facing but replaces interpolated walking with tile steps and a static idle pose. A full room offers viewing until space is cleared. Periodic snapshot refresh is suspended during local Play so it cannot reset an active walk. Reload or switch rooms to refresh the layout; server-driven layout invalidation belongs to the later realtime phases.
