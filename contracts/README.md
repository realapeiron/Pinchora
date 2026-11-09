# Outward-facing contracts

These files pin existing behavior. Review their diffs before accepting an update. The collectors
read production code; instrumentation lives exclusively in integration tests. No live services are
used by the static gates or compiled-definition test.

| File | Source |
| --- | --- |
| `commands.json` | Serialized, localized `command_definitions()` payload, sorted by registration name. Options retain their meaningful registration order. |
| `components.json` | Parsed dispatcher arms, module gates, owners, conditional ownership and automatic deferral rules. Button and modal registrations are distinct. |
| `routes.json` | Parsed router composition, resolving nests, merges, local bindings and module path attributes. Includes Axum's implicit HEAD registrations and ANY catch-all. Unsupported router syntax fails closed. |
| `api-shapes.json` | HTTP responses observed by the API/house helpers and the Reef loopback server. Keys are method, registered route template, status. |
| `api-coverage.json` | Observed and successful method/path counts, with every unobserved API route listed. HEAD and the catch-all are excluded from the denominator. |
| `i18n.json` | Compiled locale key sets, separately for each language. |
| `backup-sections.json` | Compiled backup section table names. |
| `privacy-tables.json` | Compiled privacy deletion, clearing, anonymization, list, retention and special-handling tables. |
| `redis-key-patterns.json` | Cache namespace templates and application key arguments, including local bindings/key helpers. Placeholder values are never evaluated. |
| `ledger-periods.json` | Compiled weekly/monthly reward enum keys used by the ledger. |
| `ledger-reasons.json` | Literal and formatted ledger reasons at credit/movement calls and their reward wrappers. Formatted reasons retain placeholders. |

## Recording and checking

```sh
cargo xtask contracts --prepare
# Run the complete workspace suite with the test service variables configured.
cargo nextest run --workspace --profile ci
cargo xtask check
```

`--prepare` clears `target/contracts/observations` and the compiled-definition receipt. It is mandatory before each independent
suite run: retaining observations could mask a removed response field. CI prepares immediately
before nextest and checks afterward. A missing/empty observation directory fails the check.
Preparation creates a fresh local run receipt with a source-input fingerprint; every observation and the compiled-definition receipt must match it. Old files copied into a new run are rejected. The run identifier stays in target/, never in snapshots. Preparation also builds the current route catalog for instrumentation, so new routes cannot fall into an old catch-all.
Preparation and the compiled-definition test both record source-input fingerprints. A check rejects cached results
when production code, tests, locales, content or manifests changed. Check mode does not compile or
run tests; update mode explicitly runs the compiled-definition test.
Tests atomically replace a file per process, protected by an in-process mutex. Consequently
nextest's separate processes never overwrite one another, and cargo test's concurrent threads
cannot lose observations. Both runners use exactly the same merge operation. Reef observes body frames without changing chunks, headers, trailers, errors or size hints, and records only completed bodies; it adds no response-size limit.

Shapes contain JSON paths and types, not values. Array indices collapse to `[]`; duplicate
observations are set-unioned. Numeric object keys collapse to `*` to remove fixture snowflakes.
Null, empty containers, binary and empty HTTP bodies remain distinct. The union describes all
observed variants at a method/template/status, not a claim that every field is required in every
variant. Removing a field everywhere it was observed removes its path and fails the comparison.
Literal routes take precedence over parameter routes; unknown URLs map to the catch-all without
retaining their request values. Optional fields and nonempty arrays need fixtures that exercise
them; this is an observed contract, not an inferred complete JSON Schema.

Filtered tests never rewrite snapshots. Without a current preparation receipt they skip recording. A prepared partial run is rejected with an incomplete-observation diagnostic, and update preflights completeness before writing any snapshot. Every API route must have an observation; add a fixture when adding a route. To deliberately remove a status variant, first review and remove that entry from api-shapes.json, then run a fresh complete suite.

To intentionally accept reviewed behavior changes, run `cargo xtask contracts --update` after a
fresh complete suite. It updates both source-derived and observed contracts. The command-definition
test uses `PINCHORA_UPDATE_CONTRACTS` only when explicitly invoked by this update command. Ordinary
tests compare their compiled definitions directly against checked-in files.

## Read-only gates

`cargo xtask check` runs `depcheck`, `names`, `size`, and `contracts --check`. The architecture
crate-table command remains available separately.

- `depcheck` reads locked Cargo metadata, resolves path identities (including renamed dependencies),
  and checks normal, dev, build, optional and target-specific edges. Only dev edges may target
  testkit. The five existing adapter-to-storage/cross-adapter violations are exact transitional
  exceptions. New exceptions fail against the comparison revision; deleting an edge requires deleting its exception.
- `names` checks source paths, Rust modules and route handlers for temporary suffixes and plan names, and comments
  for plan labels. String literals, immutable SQL migrations and wire URLs are outside this gate.
- `size` counts physical source-file lines and uses `syn` spans for Rust function bodies, including
  tests, impl methods and trait defaults. Comments and blank lines inside the braces count; functions written in nested macro token bodies. Arbitrary procedural macro expansions and closures are not separately named functions. Existing excesses are recorded by
  path/qualified symbol and maximum length. New entries or growth fail, including unused entries added to the baseline file itself. `size --update` removes
  resolved entries and lowers maxima; it cannot exceed the committed ratchet. The initial baseline
  is deliberately broader than just production Rust: it includes frontend files and test fixtures.

`size-exemptions.json` lists pure-data/generated source exceptions individually with reasons.
Snapshot JSON files in this directory are generated contract data, not source code. No directory
of executable source is exempt. Pure-data exemptions reject function/class declarations and arrow functions. The lead should regenerate the size baseline after pending splits.

The source-analysis dependencies are xtask's `syn`, `quote`, and `proc-macro2` (already locked transitively),
for syntax-aware source analysis and physical spans. The existing workspace regex dependency is
reused. The web test helper also directly names the already-locked `http-body` crate as a dev dependency to forward frames and trailers unchanged. Production dependency declarations and behavior are unchanged.

In local uncommitted work, the ratchet comparison revision defaults to `HEAD`. CI explicitly sets
`PINCHORA_RATCHET_BASE` to the fetched PR base or previous push revision (main for a new branch).
This prevents a committed baseline expansion from approving itself. Gates never fetch or mutate
Git state; only the CI setup shell fetches that comparison object. An unavailable revision fails
closed. A baseline absent from the comparison revision is its initial bootstrap.
