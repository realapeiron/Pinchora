# Wave 7 review

Scope: `8cc50e8..HEAD` and the uncommitted fixes in `feat/reef-wave7`.

## Guild access route matrix

All entries require a session. Writes also require the shared CSRF/origin checks.
The Reef routes deliberately permit ordinary guild members to enter, read events,
and RSVP; those are player actions, not dashboard management grants.

| Method and route | Area | Additional authorization |
| --- | --- | --- |
| GET `/api/reef/guilds/{guild}/plazas` | community (`plazas`) | Fresh membership, bot present, Plaza enabled; routing hint only; actor independently enforces 32 slots |
| GET `/api/reef/guilds/{guild}/gatherings` | community (`gatherings`) | Fresh membership, Plaza + Events enabled; required event role filters |
| POST `/api/reef/guilds/{guild}/gatherings` | community (`gatherings`) | Fresh membership plus Manage Server/Administrator/owner or dashboard community grant |
| POST `/api/reef/guilds/{guild}/gatherings/{event}/rsvp` | community (`gatherings`) | Fresh membership, Events enabled, required role; existing transactional event capacity/waitlist |
| POST `/api/house/rooms/{idx}/ops?scope=plaza&guild={guild}` (existing route, extended authority) | community (`plazas`) | Fresh membership plus manager/community grant, guild inventory; only surface extra-op permitted |
| GET `/api/reef/moderation?guild={guild}` (existing route, extended scope) | community (`plazas/moderation`) | Manager/community grant; guild-scoped evidence only |
| POST `/api/reef/moderation` with guild owner (existing route, extended scope) | community (`plazas/moderation`) | Manager/community grant; cannot escalate to personal-house/global moderation |

Proof: `wave7_guild_actions_have_explicit_community_area_mapping` checks every
area grant against these prefixes. The API regression
`guild_plaza_membership_delegated_management_capacity_and_scheduled_rsvp`
checks ordinary-member refusal for every edit operation, rejection of unrelated
configuration/safety grants, acceptance of community grants, role revocation,
membership removal, CSRF, moderation scope, capacity routing, RSVP and audit.
The room actor repeats membership/admission checks on every incoming command.

## Other new HTTP routes

Personal/account scope, no guild area: GET/PUT `/api/reef/preferences`, GET
`/api/reef/social`, GET `/api/reef/doorbells`, POST
`/api/reef/houses/{owner}/doorbells`, POST `/api/reef/doorbells/{id}/decision`,
GET/POST `/api/reef/houses/{owner}/guestbook`, POST
`/api/reef/houses/{owner}/guestbook/{note}/report`, POST
`/api/reef/houses/{owner}/appreciate`.
These resolve the session actor independently of path/body IDs. Decisions require
the host; house reads/writes require current consent, privacy and bilateral blocks.
Presence opt-in is separate from visit permission and is exposed only to existing
friends/partners, never with their location.

## Fixes and regression coverage

- Discord placement and grouped store/lift now use the same expected-state and
  partner/bond-checked transaction as the browser. Removing a support and its items
  commits all refunds together; stale groups leave every item in place. Blocked
  partners cannot open or reuse an existing editor (EN/TR refusal text).
- Partner block checks now run under the shared edit transaction's player locks;
  a previously resolved partner cannot spend inventory after a block.
- Room snapshots, presence, movement, posture and chat suppress blocked identities
  in either direction. Delivery rechecks access; policy changes clear cached client
  rosters and request a fresh authorized snapshot. Complete correction rosters also
  remove cached identities when blocks originate on another transport. HTTP and socket regressions cover
  these paths, with an event-shape unit test and a client network test.
- Hidden guestbook notes cannot acquire evidence/recipient grants through a guessed
  ID. Repeated reports share evidence, and deleting a note preserves reported evidence.
- Strict Redis quotas: reports 10/min (shared across both report endpoints), likes
  20/min, RSVP 20/min, scheduling 6/min, doorbell attempts 12/min, signing 10/min.
  Existing pair/recipient doorbell caps, daily likes/signing uniqueness and transactional
  RSVP capacity remain in force. Quota tests seed adjacent windows to avoid clock flakes.
- Binary socket messages are rejected; control frames share the frame budget.
  Parser checks the 4 KiB bound itself. Control-flood and oversized-envelope regressions.
- Outbox maintenance prunes receipts and old doorbells in batches of 500; replay
  after receipt expiry still fails its original monotonic revision.
- Expired doorbells are deleted in batches of 500 while preserving the last minute
  needed by abuse counters; retention regression checks both cases.
- Migration 0082 uses 2-second lock waits and 15-second statement bounds, a metadata-only
  constant house revision default, and indexes only new empty tables. Guestbook evidence
  uses a new cascading lookup rather than indexing the existing evidence table.
  Guest-created and host-pending indexes support abuse queries. The populated 0081→0082
  migration regression verifies physical table files stay unchanged, defaults, data and indexes.
- Long social names and multi-action rows wrap inside the 390 px sheet; a Playwright
  regression checks internal overflow, a single title and keyboard open/close.
- Editor/social/gathering panels remain lazy. Legacy editor, viewer-only widgets and
  dashboard shell/moderation load on demand. The cold-budget script enforces 1,970,000
  bytes and verifies editor/social modules have distinct deferred chunks.

## Validation environment

The provided test database already contained the original 0082 checksum. Validation
uses its isolated `reef7_review` schema (`options=-csearch_path=reef7_review`), leaving
the existing public schema and migration history unchanged. Redis uses the supplied
DB 10. Tests that create temporary migration schemas roll them back.

Chromium execution is excluded as requested. Playwright discovery/typechecking are
used to verify the specifications compile; this is not a claim of a rendered 390 px audit.

## Final verification

All Rust runs used `CARGO_BUILD_JOBS=4`; integration runs used
`PINCHORA_TEST_MIX_SECS=60`, the supplied Redis DB 10, and the isolated schema
in the supplied Postgres database described above.

| Check | Result |
| --- | --- |
| `cargo fmt --all --check` | Passed |
| `cargo clippy --workspace --all-targets -- -D warnings` | Passed |
| `cargo test --workspace --no-fail-fast -- --test-threads=2` | **2,257 passed, 0 failed, 14 ignored**, 71 result groups including doc tests |
| Reef socket binary rerun alone | 35 passed, 0 failed, 2 seed tests ignored; final full run also passed |
| Discord grouped-removal regression | Passed; final full run also passed |
| Discord partner regressions | 5 passed, including blocked editor, soulbound and Turkish flows |
| Migration regression binary | 5 passed, including populated 0081→0082 upgrade |
| `cargo build -p pinchora` | Passed; macOS linker emitted its existing large unwind-section warning |
| `npm run lint` / `npm run typecheck` | Passed, including e2e TypeScript |
| `npx vitest run` | **457 passed** in 59 files |
| `npm run build` | Passed |
| `node scripts/measure-reef.mjs` | **1,966,972 / 1,970,000 bytes**, down 31,953 from the supplied baseline |
| Slash tree and command-size regression | **78 slash roots + Report = 79**; largest `/progression` **6,653 / 6,800** characters; child limits passed |
| `npx playwright test --list` | **146 tests in 41 files** discovered; Chromium not launched as instructed |
| `git diff --check` | Passed |

No additional migration was created. No commits, branch changes or Git index writes
were requested or performed. No `.env` file was read.
