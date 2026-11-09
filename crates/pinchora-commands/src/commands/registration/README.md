# Domain registration

`commands/mod.rs` lists the ten domains once. Each domain's `registration.rs` supplies its typed
command factory and matching handler through `commands!`, followed by compatibility slices.
The numeric keys retain the historical order across domains. Add a fresh key in the relevant
slice; duplicate keys and ambiguous prefixes fail generation. Routes and legacy leaf output are
sorted by path, so a new command never requires renumbering another domain's entries. Component
ownership and gating must be declared in the same domain as its handler.

`cargo xtask registrations` combines the slices and writes the runtime compatibility tables,
routing manifest, historical leaf fixture and command-tree documentation. `--check` renders the
same output in memory and rejects drift; `cargo xtask check` includes it. No build script or
runtime discovery is involved. Generated regions are marked `registrations`; edit their domain
sources. Use fully qualified handler paths for new component and autocomplete entries.

The generated component dispatcher remains in `infra/components.rs`, so the existing gate tests
and contract scanner inspect the code the bot actually executes. Ownership overrides retain their
executable expressions, including the Redis-dependent confirmation resolver. Authorization and
prefix matching algorithms remain in their original adapters. Core receives only generated
metadata, with no dependency on the commands crate.

For a new house slash command with one button, edit `commands/house/mod.rs`, add its implementation
in that folder, and extend `commands/house/registration.rs`: typed command, route, component,
owner, module gate, response mode, help and text rules. Public responses and text availability
retain the existing defaults; private replies, forms, slash-only commands and aliases are explicit
slices. Run the generator, review its output and deliberately update the frozen contracts for the
new feature. The historical count assertions in `infra/routing/tests.rs` and `tests/contracts.rs`
also require explicit review for such a feature; they are intentionally retained in this refactor.
The current refactor does not update those contracts or historical assertions. A new help area also belongs
in the domain's `help_areas` slice; existing area keys and ordering preserve old buttons.

The test domain uses the same typed macro and compatibility writer. Its command dispatch runs
against the test services configured by `PINCHORA_TEST_DATABASE_URL` and `PINCHORA_TEST_REDIS_URL`,
with no data writes. Removing a required fixture slice fails generation; its compiled test checks
command and button dispatch, ownership, defer, help and text output. Production completeness also
uses frozen command/component contracts, historical custom IDs, existing simulator fixtures and
independent domain-file discovery.
