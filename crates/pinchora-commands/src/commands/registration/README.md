# Domain registration

`commands/mod.rs` lists the ten domains once. Each domain has two inputs:

- `registration.rs`: typed commands and context handlers, plus executable Rust match arms.
- `registration.json`: ordinary arrays of routes, help coverage, response modes, text rules and aliases.

`commands!` emits a const slice pairing each typed definition with its parser and boxed handler. `compatibility!` retains
Rust patterns, bindings and async expressions that must compile in the receiving adapter's scope.
It does not wrap plain data in a custom syntax. Neither input takes global sequence numbers.
Entries normally follow domain composition order and then their order within the domain.
Component and owner patterns must be disjoint, including bound alternatives and tuple patterns.
An unconditional wildcard must say `fallback _ => ...`; generation places it last regardless of
which domain declares it. Overlapping undeclared wildcards and multiple fallbacks fail generation.

`cargo xtask registrations` writes the runtime compatibility tables, routing manifest, historical
leaf fixture and command-tree documentation. `--check` renders in memory and rejects missing or
stale output; `cargo xtask check` includes it in CI. No build script or runtime discovery is used.
Generated regions are marked `registrations`; edit their domain sources. Use fully qualified
handler paths for new component and autocomplete entries.

`xtask/src/registrations/order.json` is a generated ordering lock. It preserves the existing command
and help-page order (old help buttons use page indices), each area’s catalogue order, synthetic root
order and the order embedded
in frozen defer/owner contracts. The generator retains surviving entries and appends new entries in
domain order, assigning their compatibility positions itself. Do not select numbers or edit this
file manually. The generated `command_order.rs` applies those positions to typed command entries.
Unordered tables are not added to this lock.

The generated component dispatcher remains in `infra/components.rs`, so the gate tests and contract
scanner inspect executable code. Redis-dependent ownership expressions stay executable, and the
central authorization and boundary-matching algorithms remain in their original adapters. Core
receives generated metadata without depending on the commands crate.

For a new house slash command with one button:

1. Add its implementation, for example `commands/house/tour.rs`, and declare `pub(crate) mod tour;` in that domain.
2. Add its type to `registration.rs`; add component, owner and module-gate arms in that file.
3. Add its route and legacy leaf to `registration.json`, and give it help coverage. Public responses
   and text availability are defaults; add private/form/slash-only entries only when needed.
4. Run `cargo xtask registrations`, then review generated output and the intended frozen-contract,
   i18n and behavioral-fixture changes for the new feature.

Historical count assertions in routing and contract tests still require explicit review when adding
a feature. They are retained in this behavior-preserving refactor. A new help area belongs in the
same domain's `help_areas` data; existing area keys and positions preserve old buttons.

The test domain uses the same typed macro, JSON reader and compatibility writer. Its command runs
through a real Responder; its generated button, owner, module, defer, help and text surfaces are
asserted. Removing required fixture metadata or individual owner/gate entries fails validation.
Frozen command/component contracts, existing custom IDs and independent domain-file discovery also
protect completeness. Test services must be configured with a test Postgres database and nonzero
Redis database.
