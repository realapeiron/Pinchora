# Shared service infrastructure

`pinchora-services::Services` holds one `Arc<Infrastructure>`. Infrastructure owns the database
pool, Redis connection, Discord and general HTTP clients, clock, tracked tasks, cancellation token,
and render and preview budgets. It depends on storage, core, game and render, never on either adapter.

`commands::App` retains its constructor and public compatibility handles. Database, cache and HTTP
handles clone the existing pools/connections; they do not connect again. Its clock, task and render
methods delegate to infrastructure. The public HTTP handles remain replaceable by existing mock
fixtures; unmoved adapter code continues to read those handles. WebState exposes the same Services
through its App. Domain caches and their invalidation remain on that shared App until their use
cases move. There are no placeholder domain modules or blanket Deref implementations.

A service domain exposes types and operations through its `mod.rs`, with private implementation
modules. `cargo xtask depcheck` also checks source imports: no service glob imports, upward imports
within services, public child modules, or imports of another domain's implementation, including
module aliases. The existing adapter-to-storage exceptions remain until the corresponding use cases
move; new exceptions cannot be added to the ratchet.

HTTP middleware creates one `http.request` span, including a generated `request_id`, on dashboard,
interaction and operations servers. Discord dispatch creates one `discord.interaction` span with
the interaction ID as `request_id`. Spans surround asynchronous execution. HTTP headers, bodies and
callback tokens are not recorded. ERROR remains reserved for server errors and unexpected failures;
expected refusals use WARN or lower. No existing ERROR site was downgraded and no exporter was added.

`Job` adds `pinchora_job_runs_total{kind,outcome}` and elapsed time without replacing existing row
claims. Outcomes are `ok`, `error`, and `skipped` when an advisory lease is busy. Reminders and tickets
are the ticker pilots. Ticket status refresh retains its existing advisory key and protected work.
The lock-only transaction always awaits rollback, including job errors, early returns and panics.
Its tracked owner continues cleanup when the caller stops waiting. SQL and transaction completion
stay in db; services owns job lifetime and metrics. Existing transaction keys and claim algorithms
are unchanged. This is not a new persistent queue, retry or dead-letter policy.

The remaining ticker entrypoints are workflows, player economy, moderation policy, leveling policy,
server events, automod, invite rewards, verification, anti-nuke, moderation, collection, party, roles,
stats, starboard, utility, birthdays, feeds, Roblox, trackers, event RSVP, community, community tools,
voice XP and temporary voice. Their existing claim mechanisms remain for a later migration.
