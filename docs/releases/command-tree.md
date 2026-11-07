# Command tree release rehearsal

Slices 13–18 complete the transition from 95 registered roots to **66 roots** and 330 executable actions.
The manifest is the action inventory; all 160 moved actions have one canonical destination. Old typed
models remain internal for in-flight interactions and prefix compatibility. The eight planned feature
families must share the domain roots; only four additional roots are reserved (66 + 4 = 70).
At least four planned families must use existing roots, with both size and 25-child checks. Future packages must allocate a canonical path
and check that group's 25-child limit; they must not automatically allocate a new root.

The merged tree includes `/play memory|minesweeper|higherlower|anagram|typing|guessnumber`,
`/duel othello`, `/party nhie`, and `/party fun` (conch, choose and roll modes). Their existing
custom IDs and command-control owners are retained (`mm`, `mi`, `yl`, `ag`, `yt`, `gn`, `of`, `ni`), along with the redrawn cards.
Reef Web routes, multiplayer, chat and house invalidation remain independent of the command cutover.

## Deployment order

1. Back up the database and export configuration. Drain interaction traffic while applying **0068**
   and replacing all gateway/interaction processes: e814163 cannot evaluate the new policy paths.
   Resume traffic only on this compatible runtime. Apply migration **0068** after **0066**;
   never change the already-applied 0066 checksum. Fresh installations still apply **0066**, including on databases
   that already applied **0067** (quick games).
   The out-of-order migration regression test covers this sequence; neither migration is renumbered.
   It expands split parent rules only over their former actions, retains independent role/channel constraints and cooldown
   identities, and translates alias targets. 0068 remaps former `/setup` paths, flattens inherited constraints without losing their original
   cooldown identities, and preserves conflicting destination rules. Re-importing an old version-1 backup uses mappings v1 and v2 before
   preview, validation and merge/replace. Non-executable aliases such as `config xp` keep their prefix
   target because their next token determines the flattened destination. They do not consume shipped
   compatibility aliases or delete existing custom commands named `setup`, `mod` or `rewards`.
2. Deploy the compatible runtime to **all** gateway and interaction processes before changing Discord's
   registration. Keep traffic on this runtime throughout propagation. Do not register the transition
   tree alongside the final one. No empty future groups are registered.
3. Rehearse in a development guild with the canonical schema. Check all five administration roots with Manage Server and
   without it, `/mod` with each staff permission, autocomplete, private replies, modal-first actions,
   and old role/ticket/verification/help messages. All five administration roots always require Manage Server server-side;
   existing handler-specific permissions still apply. Member-facing buttons retain their custom IDs.
4. Run the **explicit global registration mode once** after rehearsal. Registration validates the
   schema, enumerates Discord guild membership, and reads every affected guild's Integrations overrides
   before bulk overwrite. It stores the original ID/name map and exact override entries in
   `command_permission_snapshots`. Any snapshot read/write failure aborts before deletion. A retry
   cannot overwrite the first snapshot, even if Discord has already removed the old command IDs.
   The registration job now needs database and Redis connectivity as well as Discord credentials.
5. After registration, the application-scoped invalidation event clears command IDs, override caches
   and snapshot caches in every subscribed process. Restart serving processes after registration to
   cover missed pub/sub delivery (fallback TTLs: IDs 10 minutes, overrides 5 minutes, snapshots 1 minute).
   A publish failure after Discord succeeds means restart and retry the registration job; do not assume
   the HTTP write was rolled back. Publish help, generated catalog and dashboard assets together.

## Server admin checklist: Discord Integrations permissions

Command permissions in **Server Settings → Integrations → Pinchora** are attached to command IDs.
Discord [permanently deletes overrides when a command is deleted or renamed](https://docs.discord.com/developers/interactions/application-commands#edit-application-command-permissions).
It cannot carry separate `/lock`, `/unban` and `/warnings` overrides into a single `/mod` ID.
Re-check the application-wide restrictions and the new `/setup`, `/safety`, `/community`, `/integrations`, `/progression`, `/mod`, `/rewards`, `/customize` and
`/info` roots, plus roots that lost admin leaves (`color`, `event`, `plaza`, `points`, `voice`). Check:

- Individual member grants/denials, role grants/denials, `@everyone`, channel exceptions and all-channel
  restrictions; test threads as well as their parent channels.
- All five administration roots’ visibility/default Manage Server permission. `/mod` deliberately has no single restrictive
  default permission mask; each action retains its original server-side checks.
- The emergency recovery paths: `/setup server`, command controls, backups, `/help` and `/privacy`.
- Existing panel buttons and text aliases, including aliases that previously targeted a parent group.

Snapshots remain an additional compatibility boundary for moved slash/text actions. The evaluator uses
Discord's original user → held-role allow precedence → everyone rules, with channel denial first and
original default permissions when silent; it does **not** approximate them with additive bot allowlists.
Live overrides on the new root still apply. Administrator bypass and built-in action checks are unchanged.
Snapshots do not add Manage Server to member panel buttons and are excluded from portable backups.
The dashboard's command rules are separate and cumulative; resetting a migrated rule explicitly removes
its inherited restrictions. An Integrations edit alone does not clear a historical snapshot.

Before retiring a snapshot, export its `snapshot` JSON from `command_permission_snapshots` for the exact
application/guild, reproduce and test the intended restrictions on canonical actions, and only then have
an operator remove that guild/application snapshot and restart/invalidate serving processes. If the
new root cannot express the old distinctions, retain the snapshot. Do not globally clear snapshots.
Use separate applications for global production and development-guild rehearsals so their snapshots
cannot conflict. Restrict snapshot access like the existing command-control configuration.

## Compatibility and acceptance

`!feeds list` and `@Pinchora feeds list` resolve to `/integrations feeds list`; removed names no longer appear
in the slash picker. The existing once-per-user/path/day notice remains. Private forms/panels redirect
to the canonical slash destination instead of exposing configuration in channel messages. `/tag setup`
continues to execute existing custom content; new collisions with current or removed built-in names
are rejected. Old `hp:<index>` help messages resolve their original first action to its canonical page;
new help uses versioned controls. The public catalog searches old names but lists only canonical paths.

Run the workspace Rust checks with the assigned real PostgreSQL/Redis URLs and `PINCHORA_TEST_MIX_SECS=60`,
then the web lint/typecheck/Vitest/build checks. Re-run failing binaries alone. The manifest test requires
66 roots and one reachable canonical leaf for every pre-migration action, preserves typed arguments,
permissions/module metadata and defer behavior. Simulator feature fixtures rewrite both the root and
nested Discord options to canonical paths; explicit legacy fixtures separately compare original handlers.
Regenerate rules, command/fact catalogs and landing images through their ignored writer tests when stale.
No registration, deployment, commit or production data changes are part of the local rehearsal.

## Registration size incident and domain split

Discord allows **8,000 characters per command**, including nested option/group/subcommand names,
descriptions and choice names/values. For each localized field it counts the longest translation or
default, not their sum ([Discord documentation](https://docs.discord.com/developers/interactions/application-commands#slash-commands)).
The validator and regression test cap every command at **7,200** (10% headroom), counting Unicode
characters and numeric choice values conservatively, and the test prints the ten largest commands.
After the split the largest roots are `/community` **6,445**, `/setup` **6,234**,
`/progression` **5,567**, `/safety` **4,493**, and `/integrations` **2,772**.
The rejected payload's item 59 was `/setup`: **25,276** characters. `/mod` was **1,772**.

| Root | Groups |
| --- | --- |
| `/setup` | server, welcome, roles, commands, backup, branding |
| `/safety` | moderation, automod, verification, antinuke |
| `/community` | tickets, starboard, events, birthdays, utility (including voice), server (suggestions/plaza) |
| `/integrations` | feeds, stats, invites |
| `/progression` | leveling, economy, achievements |

All 330 actions remain reachable. Old 95-root slash registrations are handled by the e814163
runtime: `commands::dispatch` retains every legacy typed model and `routing::adapt` passes old
payloads through. Failed bulk registration therefore does not make old slash names unknown.
This patch also accepts the prior consolidated `/setup` paths during propagation and from text aliases.
No production registration or deployment is performed by this fix.
