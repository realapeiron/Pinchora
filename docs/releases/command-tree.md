# Command tree release rehearsal

Slices 13–18 complete the transition from 95 registered roots to **62 roots** and 321 executable actions.
The manifest is the action inventory; all 160 moved actions have one canonical destination. Old typed
models remain internal for in-flight interactions and prefix compatibility. The eight planned feature
families bring the projected future budget to 70 roots. Future packages must allocate a canonical path
and check that group's 25-child limit; they must not automatically allocate a new root.

## Deployment order

1. Back up the database and export configuration. Apply migration **0066** once. It expands split parent
   rules only over their former actions, retains independent role/channel constraints and cooldown
   identities, and translates alias targets. Re-importing an old version-1 backup uses mapping v1 before
   preview, validation and merge/replace. Non-executable aliases such as `config xp` keep their prefix
   target because their next token determines the flattened destination. They do not consume shipped
   compatibility aliases or delete existing custom commands named `setup`, `mod` or `rewards`.
2. Deploy the compatible runtime to **all** gateway and interaction processes before changing Discord's
   registration. Keep traffic on this runtime throughout propagation. Do not register the transition
   tree alongside the final one. No empty future groups are registered.
3. Rehearse in a development guild with the canonical schema. Check `/setup` with Manage Server and
   without it, `/mod` with each staff permission, autocomplete, private replies, modal-first actions,
   and old role/ticket/verification/help messages. `/setup` always requires Manage Server server-side;
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
Re-check the application-wide restrictions and the new `/setup`, `/mod`, `/rewards`, `/customize` and
`/info` roots, plus roots that lost admin leaves (`color`, `event`, `plaza`, `points`, `voice`). Check:

- Individual member grants/denials, role grants/denials, `@everyone`, channel exceptions and all-channel
  restrictions; test threads as well as their parent channels.
- `/setup` visibility/default Manage Server permission. `/mod` deliberately has no single restrictive
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

`!feeds list` and `@Pinchora feeds list` resolve to `/setup feeds list`; removed names no longer appear
in the slash picker. The existing once-per-user/path/day notice remains. Private forms/panels redirect
to the canonical slash destination instead of exposing configuration in channel messages. `/tag setup`
continues to execute existing custom content; new collisions with current or removed built-in names
are rejected. Old `hp:<index>` help messages resolve their original first action to its canonical page;
new help uses versioned controls. The public catalog searches old names but lists only canonical paths.

Run the workspace Rust checks with the assigned real PostgreSQL/Redis URLs and `PINCHORA_TEST_MIX_SECS=60`,
then the web lint/typecheck/Vitest/build checks. Re-run failing binaries alone. The manifest test requires
62 roots and one reachable canonical leaf for every pre-migration action, preserves typed arguments,
permissions/module metadata and defer behavior. Simulator feature fixtures rewrite both the root and
nested Discord options to canonical paths; explicit legacy fixtures separately compare original handlers.
Regenerate rules, command/fact catalogs and landing images through their ignored writer tests when stale.
No registration, deployment, commit or production data changes are part of the local rehearsal.
