# Web dashboard

The web process is the `pinchora web` role. It serves the Vite build, Discord OAuth login and the JSON API on the configured bind address. The SPA lives in `web/`, and Axum routes live in `crates/pinchora-web/src/routes/`.

## Run locally

1. Set `PINCHORA_WEB__PUBLIC_URL` to the public origin (typically `http://localhost:8090`) and register `<origin>/auth/callback` as a Discord OAuth redirect. Configure the Discord client ID and secret through the normal Pinchora configuration. Keep secrets outside source control.
2. Start PostgreSQL and Redis, apply migrations with `pinchora migrate`, then run `cargo run -p pinchora -- web`.
3. For SPA development, run `cd web && npm ci && npm run dev`. Point `PINCHORA_WEB__PUBLIC_URL` at the Vite origin, register its callback, and use Vite's API proxy.
4. Run `npm run lint`, `npm run typecheck`, `npx vitest run`, and `npm run build` in `web/`. Rust tests use `PINCHORA_TEST_DATABASE_URL` and `PINCHORA_TEST_REDIS_URL` on an isolated test database.

`PINCHORA_WEB__DEV_LOGIN` enables a test login only on a loopback bind address. Keep it off for public service. `PINCHORA_WEB__COOKIE_SECURE` forces Secure cookies; HTTPS public URLs set them automatically. `PINCHORA_WEB__PUBLIC_URL` is also the CSRF Origin comparison and should be the exact browser origin.

| Setting | Purpose |
|---|---|
| `PINCHORA_WEB__BIND` | Address and port for the Axum listener. |
| `PINCHORA_WEB__PUBLIC_URL` | Exact browser origin for OAuth callbacks, CSRF and secure cookies. |
| `PINCHORA_WEB__CLIENT_SECRET` | Discord OAuth client secret. |
| `PINCHORA_WEB__STATIC_DIR` | Optional built SPA directory; otherwise embedded assets are served. |
| `PINCHORA_WEB__COOKIE_SECURE` | Force Secure cookies behind an HTTPS proxy. |
| `PINCHORA_WEB__DEV_LOGIN` | Loopback-only development login. |

The web process also needs the shared Discord application ID and bot token, PostgreSQL URL and Redis URL from Pinchora's normal configuration.

## Routes and ownership

- `/auth/login`, `/auth/callback`, `/auth/logout`: Discord OAuth, Redis sessions and an HttpOnly cookie.
- `/api/me`: login identity and CSRF token. `/api/me/home`, `/profile`, `/character`, `/wallet`, `/inventory`, `/pets`, `/stall`, `/fish-book`, `/quests`, `/guestbook`, `/friends`, `/partner` are personal pages. Requests use the session user ID; clients cannot choose whose player rows they mutate.
- `/api/house`: own and visited homes. `/api/guilds/{g}`: server dashboard, guarded by `ManagedGuild`. `/api/public`: limited public data.
- `/me/*`: player SPA pages. `/g/:guildId/*`: managed server pages. Static asset and SPA fallback routes are last.

Inventory gifts use `Db::gift_item`; stall listing and withdrawal use `Db::list_item` and `Db::unlist_item`. Pet care uses the same `Care`, cooldown key, `Db::care_pet` transaction (needs, XP and shell payment commit together), quest and stat counters as Discord. Trips, claims and wake use the corresponding Db methods and game functions. Do not duplicate economy transactions in a route. Stall purchases, guestbook signing and friend requests remain Discord-only; the web exposes stall listing/withdrawal, guestbook moderation and read-only friends. Friend responses contain only identity and friendship dates, never profile or house statistics.

## Security

Redis sessions have a seven day idle TTL and a thirty day absolute lifetime. A creation timestamp up to five minutes ahead is tolerated for clock skew between replicas; older sessions still expire exactly at thirty days. Every write requires a session CSRF header. JSON payloads require a JSON content type; Fetch Metadata and Origin, when sent, must identify the same origin. The CSP explicitly permits only the configured site’s WebSocket origin for Reef, plus same-origin workers and blob/data images. The server sends an enforcing CSP, `nosniff`, frame denial and no-store on dynamic routes. OAuth and refresh tokens stay server-side. Session checks precede all personal routes; management checks precede server writes. Use the shared API client in the SPA so CSRF and error handling stay consistent.

## Add a settings page

Add a focused route module under `crates/pinchora-web/src/routes/` and nest it in the matching router. Extract `CurrentSession` for personal routes or `ManagedGuild` for server routes. Validate the incoming payload with `ApiJson` and shared game/core rules; call existing Db transactions for writes. Add a matching page, route and navigation link in `web/src`, and put every visible phrase in both i18n catalogs. Test session, CSRF, unauthorized access, the actual write and its audit entry, then run the Rust and web checks above.
