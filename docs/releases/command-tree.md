# Command tree release rehearsal

The integrated Wave 8 tree has **78 slash roots plus Report and Open ticket (80 registrations)**
and **454 executable entries**. The largest roots measure `/progression` **6,642**,
`/setup` **6,624**, and `/community` **6,367** characters, with no node exceeding 25 children.
Migrations **0085**, **0086**, and **0087** cover room games, ticket operations, and workflows.

D12 extends `/ticket` with seven staff leaves (`priority`, `escalate`, `transfer`, `request-close`,
`force-open`, `note`, `snippet`). The root has 16 children and remains below the 6,800-character
limit. No slash root is added. Migration **0086** adds ticket operation state, panel and support
settings, private notes, snippets and form drafts. Dashboard routes remain beneath `/tickets`
and map to W7's community access area. Claim and close use database compare-and-set updates.
F13 adds `/setup workflows list|toggle|runs` and `/tag wf:<name>` autocomplete. Editing and button posting are web-only. Registration has **78 slash roots plus Report and Open ticket (80)**, `/setup` now has **7** groups, and each root remains within **6,800 characters**. Migration **0087** only.

D6 collection depth extends the existing `/dex` panel and `/critterdrop` interactions; it adds no slash root or child. The `/me/collection` and `/me/wishlist` player pages share the same database state. Migration **0084** only. No registration or deployment is performed here.

D10 adds `/highlight`, `/checklist`, `/autopost`, `/anniversary` and `/confess` as feature roots. Existing `/giveaway`, `/poll`, `/suggestion` and `/remind` grow within their budgets. Registration now has **78 slash roots plus Report**. `/community` gains no children and stays under its 6,400-character budget. D10 uses migration 0079 only.

F20 adds four member roots (`/qotd`, `/counting`, `/form`, `/modmail`). Registration has
**72 slash roots plus the Report message action** (73 total). `/ban`, `/kick`, `/warn`,
`/timeout` and `/purge` remain direct roots with their D11 options and original Ban Members,
Kick Members, Moderate Members and Manage Messages permissions. Secondary actions, including
softban, stay under `/mod`. Old `/mod ban|kick|warn|timeout|purge` interactions and text aliases
resolve during propagation; saved `/mod` parent restrictions still cover the restored roots.
No migration is needed: runtime policy matching and backup normalization preserve old restrictions.

F20 configuration now uses admin-only leaves on its four feature roots:
`/qotd settings|queue`, `/counting setup|reset`, `/form create|post|close`, and
`/modmail settings`. The shared router preserves the former `/community` payloads,
text aliases, private replies, module ownership and saved parent/leaf restrictions.
Manage Server is enforced per action; member leaves retain their existing permissions.
Registration enforces **6,400 characters for `/community`** and **6,800 for every other root**.
The command-size test reports the five largest commands and rejects any root above its budget.

F16 adds `/roblox verify|unverify|whois|update`, with administration under
`/integrations roblox settings|bind-add|bind-remove|bind-list`. Migration **0073** is
imported unchanged from the reviewed branch. The combined tree has **381 executable entries**.

F14 adds `/setup branding show|apply|reset` and migration **0076**. F17 adds `/game` and
`/integrations trackers`, with migration **0075**. Both migrations retain their reviewed numbers.
`/setup` has **6** children and `/integrations` has **5**, within the 25-child limit.

Slices 13–18 originally completed the transition from 95 registered roots to **66 roots** and 330 executable actions.
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
72 registrations and one reachable canonical leaf for every pre-migration action, preserves typed arguments,
permissions/module metadata and defer behavior. Simulator feature fixtures rewrite both the root and
nested Discord options to canonical paths; explicit legacy fixtures separately compare original handlers.
Regenerate rules, command/fact catalogs and landing images through their ignored writer tests when stale.
No registration, deployment or production data changes are part of the local rehearsal.

## Registration size incident and domain split

Discord allows **8,000 characters per command**, including nested option/group/subcommand names,
descriptions and choice names/values. For each localized field it counts the longest translation or
default, not their sum ([Discord documentation](https://docs.discord.com/developers/interactions/application-commands#slash-commands)).
The validator and regression test cap every command at **6,800**, with `/community` capped at **6,400**, counting Unicode
characters and numeric choice values conservatively, and the test prints the ten largest commands.
Historically, immediately after the domain split, the largest roots were `/community` **6,445**, `/setup` **6,234**,
`/progression` **5,567**, `/safety` **4,493**, and `/integrations` **2,772**.
The rejected payload's item 59 was `/setup`: **25,276** characters. `/mod` was **1,772**.

| Root | Groups |
| --- | --- |
| `/setup` | server, welcome, roles, commands, backup, branding |
| `/safety` | moderation, automod, verification, antinuke |
| `/community` | tickets, starboard, events, birthdays, utility (including voice), server (suggestions/plaza) |
| `/integrations` | feeds, stats, invites |
| `/progression` | leveling (23 actions including curve, sources, tuning, voice roles and public board), economy, achievements |

All 330 actions remain reachable. Old 95-root slash registrations are handled by the e814163
runtime: `commands::dispatch` retains every legacy typed model and `routing::adapt` passes old
payloads through. Failed bulk registration therefore does not make old slash names unknown.
This patch also accepts the prior consolidated `/setup` paths during propagation and from text aliases.
No production registration or deployment is performed by this fix.

## D7 reef loop

Nine member leaves extend the existing roots: `fish dock|contest|season|dive`,
`house aquarium|garden|craft`, `pet streak`, `partner streak`. The tree remains at
72 slash roots plus Report; the source action inventory becomes 397 entries.
All nine leaves defer privately, retain existing root module ownership and command
controls, and are included in the EN/TR command catalogs. Diving shares the existing
`fz` fishing dispatcher via its `loop` action. Migration 0078 only; no registration
or deployment is performed in this worktree.

## D9 party and team games v2

Adds six `/party` leaves (`reefquiz`, `wordwake`, `driftstory`, `roomwake`,
`crittercompass`, `lastreef`) and three `/play` leaves (`reefmind`, `kelpmaze`,
`gemtide`). `/party` has 15 children and `/play` has 18; registration remains
78 slash roots plus Report. The localized size check measures `/party` at
1,934 characters and `/play` at 2,076, below the 6,800-character limit.
EN/TR descriptions and the route inventory include all nine leaves. No migration, dependencies, registration or deployment.

## D8 player economy v2

Twelve actions extend `/rewards`, `/stall` and `/progression economy`; registration remains
78 slash roots plus Report (443 executable source entries). The largest root is
`/progression` at 6,762 characters; every root remains below 6,800 with at most 25 children.
The internal `economyv2`
model is pruned from registration by the canonical manifest. `sl:eco:*` extends the stall
component namespace with owner-bound shift and trade controls; old stall controls retain
their original payloads. All twelve leaves defer privately, and administration remains
under `/progression economy tools` with the server-side Manage Server check.
Migration 0083 only. Apply before restarting the gateway/web processes, which then share
the 30-second idempotent auction/trade sweeper. Register the regenerated tree only through
the owner's ordinary release process. No registration or deployment is performed here.

## D12 ticket operations v2

The `/ticket` root has 16 children and remains below the 6,800-character limit. Message
**Open ticket** adds a context action that carries the source message link through panel
selection, FAQ and multi-step forms. Registration has 78 slash roots and two message actions.
Migration 0086 only; web ticket routes use the Community access area.

## Registered command paths

Generated by `cargo xtask registrations` from the domain registrations.

<!-- registration-tree:start -->

| Command | Legacy handler |
| --- | --- |
| `Open ticket` | `Open ticket` |
| `Report` | `Report` |
| `achievements` | `achievements` |
| `afk set` | `afk set` |
| `anniversary off` | `anniversary off` |
| `anniversary set` | `anniversary set` |
| `autopost list` | `autopost list` |
| `autopost remove` | `autopost remove` |
| `autopost set` | `autopost set` |
| `balance` | `balance` |
| `ban` | `ban` |
| `beachcomb` | `beachcomb` |
| `birthday list` | `birthday list` |
| `birthday me` | `birthday me` |
| `birthday next` | `birthday next` |
| `birthday remove` | `birthday remove` |
| `birthday set` | `birthday set` |
| `birthday view` | `birthday view` |
| `character` | `character` |
| `checklist add` | `checklist add` |
| `checklist done` | `checklist done` |
| `checklist list` | `checklist list` |
| `checklist remove` | `checklist remove` |
| `color list` | `color list` |
| `color pick` | `color pick` |
| `color remove` | `color remove` |
| `color set` | `color set` |
| `community birthdays preview` | `birthdays preview` |
| `community birthdays settings` | `birthdays settings` |
| `community events template` | `event template` |
| `community server plaza-edit` | `plaza edit` |
| `community server suggestions` | `config suggestions` |
| `community starboard edit` | `starboard edit` |
| `community starboard list` | `starboard list` |
| `community starboard remove` | `starboard remove` |
| `community starboard setup` | `starboard setup` |
| `community starboard top` | `starboard top` |
| `community starboard votes-add` | `starboard votes add` |
| `community starboard votes-list` | `starboard votes list` |
| `community starboard votes-remove` | `starboard votes remove` |
| `community tickets panel-create` | `tickets panel create` |
| `community tickets panel-delete` | `tickets panel delete` |
| `community tickets panel-edit` | `tickets panel edit` |
| `community tickets panel-list` | `tickets panel list` |
| `community tickets panel-post` | `tickets panel post` |
| `community tickets settings` | `tickets settings` |
| `community utility schedule-add` | `schedule add` |
| `community utility schedule-list` | `schedule list` |
| `community utility schedule-pause` | `schedule pause` |
| `community utility schedule-preview` | `schedule preview` |
| `community utility schedule-remove` | `schedule remove` |
| `community utility schedule-resume` | `schedule resume` |
| `community utility schedule-run-now` | `schedule run-now` |
| `community utility sticky-remove` | `sticky remove` |
| `community utility sticky-set` | `sticky set` |
| `community utility sticky-show` | `sticky show` |
| `community utility voice-add` | `voice creators add` |
| `community utility voice-list` | `voice creators list` |
| `community utility voice-remove` | `voice creators remove` |
| `confess ban` | `confess ban` |
| `confess report` | `confess report` |
| `confess reveal` | `confess reveal` |
| `confess review` | `confess review` |
| `confess settings` | `confess settings` |
| `confess submit` | `confess submit` |
| `counting reset` | `community counting reset` |
| `counting setup` | `community counting setup` |
| `counting stats` | `counting stats` |
| `crabpot` | `crabpot` |
| `critterdrop` | `critterdrop` |
| `customize background` | `customize background` |
| `customize badges` | `customize badges` |
| `customize bio` | `bio` |
| `customize showcase` | `customize showcase` |
| `customize style` | `customize style` |
| `customize title` | `customize title` |
| `daily` | `daily` |
| `dex` | `dex` |
| `duel checkers` | `duel checkers` |
| `duel chess` | `duel chess` |
| `duel connect4` | `duel connect4` |
| `duel othello` | `duel othello` |
| `duel rps` | `duel rps` |
| `duel seafleet` | `duel seafleet` |
| `duel share` | `duel share` |
| `duel tictactoe` | `duel tictactoe` |
| `event attendees` | `event attendees` |
| `event cancel` | `event cancel` |
| `event create` | `event create` |
| `event edit` | `event edit` |
| `event list` | `event list` |
| `event remind` | `event remind` |
| `fish book` | `fish book` |
| `fish cast` | `fish cast` |
| `fish contest` | `fish contest` |
| `fish dive` | `fish dive` |
| `fish dock` | `fish dock` |
| `fish gear` | `fish gear` |
| `fish season` | `fish season` |
| `fish sell` | `fish sell` |
| `fish shop` | `fish shop` |
| `form close` | `community forms close` |
| `form create` | `community forms create` |
| `form list` | `form list` |
| `form open` | `form open` |
| `form post` | `community forms post` |
| `friends add` | `friends add` |
| `friends list` | `friends list` |
| `friends remove` | `friends remove` |
| `game lol link` | `game lol link` |
| `game lol profile` | `game lol profile` |
| `game lol unlink` | `game lol unlink` |
| `game mc news` | `game mc news` |
| `game mc status` | `game mc status` |
| `game mc trivia` | `game mc trivia` |
| `game valorant profile` | `game valorant profile` |
| `gift` | `gift` |
| `give` | `give` |
| `giveaway end` | `giveaway end` |
| `giveaway list` | `giveaway list` |
| `giveaway reroll` | `giveaway reroll` |
| `giveaway start` | `giveaway start` |
| `help` | `help` |
| `highfive` | `highfive` |
| `highlight add` | `highlight add` |
| `highlight list` | `highlight list` |
| `highlight remove` | `highlight remove` |
| `house aquarium` | `house aquarium` |
| `house contest enter` | `house contest enter` |
| `house contest view` | `house contest view` |
| `house contest vote` | `house contest vote` |
| `house contest winners` | `house contest winners` |
| `house craft` | `house craft` |
| `house decorate` | `house decorate` |
| `house edit` | `house edit` |
| `house garden` | `house garden` |
| `house guestbook` | `house guestbook` |
| `house like` | `house like` |
| `house lock` | `house lock` |
| `house upgrade` | `house upgrade` |
| `house view` | `house view` |
| `house visit` | `house visit` |
| `hug` | `hug` |
| `info avatar` | `info avatar` |
| `info banner` | `info banner` |
| `info bot` | `info bot` |
| `info channel` | `info channel` |
| `info emoji` | `info emoji` |
| `info invite` | `info invite` |
| `info members` | `info members` |
| `info role` | `info role` |
| `info server` | `info server` |
| `info user` | `info user` |
| `info wrapped` | `wrapped` |
| `integrations feeds add` | `feeds add` |
| `integrations feeds edit` | `feeds edit` |
| `integrations feeds list` | `feeds list` |
| `integrations feeds remove` | `feeds remove` |
| `integrations feeds test` | `feeds test` |
| `integrations invites bonus-add` | `inviteadmin bonus-add` |
| `integrations invites bonus-remove` | `inviteadmin bonus-remove` |
| `integrations invites post` | `inviteadmin post` |
| `integrations invites reward-add` | `inviteadmin reward-add` |
| `integrations invites reward-remove` | `inviteadmin reward-remove` |
| `integrations invites settings` | `inviteadmin settings` |
| `integrations roblox bind-add` | `robloxadmin bind-add` |
| `integrations roblox bind-list` | `robloxadmin bind-list` |
| `integrations roblox bind-remove` | `robloxadmin bind-remove` |
| `integrations roblox settings` | `robloxadmin settings` |
| `integrations stats counter-add` | `counters add` |
| `integrations stats counter-list` | `counters list` |
| `integrations stats counter-remove` | `counters remove` |
| `integrations trackers mc-news-channel` | `trackers mc-news-channel` |
| `integrations trackers mc-unwatch` | `trackers mc-unwatch` |
| `integrations trackers mc-watch` | `trackers mc-watch` |
| `integrations trackers patch-add` | `trackers patch-add` |
| `integrations trackers patch-list` | `trackers patch-list` |
| `integrations trackers patch-remove` | `trackers patch-remove` |
| `inventory` | `inventory` |
| `invites temp` | `invites temp` |
| `invites top` | `invites top` |
| `invites user` | `invites user` |
| `kick` | `kick` |
| `language set` | `language set` |
| `leaderboard` | `leaderboard` |
| `mod case` | `case` |
| `mod lock` | `lock` |
| `mod mass` | `modtools mass` |
| `mod names` | `modtools names` |
| `mod nick` | `nick` |
| `mod notes` | `modtools notes` |
| `mod raid lockdown` | `raid lockdown` |
| `mod raid status` | `raid status` |
| `mod raid unlock` | `raid unlock` |
| `mod reason` | `reason` |
| `mod report` | `modtools report` |
| `mod restrict` | `modtools restrict` |
| `mod role add` | `role add` |
| `mod role remove` | `role remove` |
| `mod roles` | `modtools roles` |
| `mod slowmode` | `slowmode` |
| `mod suggestion approve` | `suggestion approve` |
| `mod suggestion deny` | `suggestion deny` |
| `mod unban` | `unban` |
| `mod unlock` | `unlock` |
| `mod untimeout` | `untimeout` |
| `mod unwarn` | `unwarn` |
| `mod voice` | `modtools voice` |
| `mod warnings` | `warnings` |
| `modmail block` | `modmail block` |
| `modmail close` | `modmail close` |
| `modmail settings` | `community modmail settings` |
| `modmail snippet-add` | `modmail snippet-add` |
| `modmail snippet-list` | `modmail snippet-list` |
| `modmail snippet-use` | `modmail snippet-use` |
| `partner leave` | `partner leave` |
| `partner propose` | `partner propose` |
| `partner streak` | `partner streak` |
| `partner view` | `partner view` |
| `party acronym` | `party acronym` |
| `party crittercompass` | `party crittercompass` |
| `party driftstory` | `party driftstory` |
| `party fun` | `party fun` |
| `party hotpearl` | `party hotpearl` |
| `party lastreef` | `party lastreef` |
| `party nhie` | `party nhie` |
| `party postcard` | `party postcard` |
| `party reefquiz` | `party reefquiz` |
| `party roomwake` | `party roomwake` |
| `party stop` | `party stop` |
| `party truthordare` | `party truthordare` |
| `party wordrace` | `party wordrace` |
| `party wordwake` | `party wordwake` |
| `party wouldyourrather` | `party wouldyourrather` |
| `pat` | `pat` |
| `pet adopt` | `pet adopt` |
| `pet capsule` | `pet capsule` |
| `pet list` | `pet list` |
| `pet rename` | `pet rename` |
| `pet streak` | `pet streak` |
| `pet trip claim` | `pet trip claim` |
| `pet trip go` | `pet trip go` |
| `pet view` | `pet view` |
| `ping` | `ping` |
| `play 2048` | `play 2048` |
| `play anagram` | `play anagram` |
| `play derby` | `play derby` |
| `play gemtide` | `play gemtide` |
| `play guessnumber` | `play guessnumber` |
| `play hangman` | `play hangman` |
| `play heist` | `play heist` |
| `play higherlower` | `play higherlower` |
| `play kelpmaze` | `play kelpmaze` |
| `play memory` | `play memory` |
| `play minesweeper` | `play minesweeper` |
| `play picross` | `play picross` |
| `play reefmind` | `play reefmind` |
| `play shells` | `play shells` |
| `play stats` | `play stats` |
| `play trivia` | `play trivia` |
| `play typing` | `play typing` |
| `play wordle` | `play wordle` |
| `plaza donate` | `plaza donate` |
| `plaza view` | `plaza view` |
| `points show` | `points show` |
| `poll` | `poll` |
| `privacy` | `privacy` |
| `profile` | `profile` |
| `progression achievements add` | `achievementadmin add` |
| `progression achievements edit` | `achievementadmin edit` |
| `progression achievements grant` | `achievementadmin grant` |
| `progression achievements list` | `achievementadmin list` |
| `progression achievements remove` | `achievementadmin remove` |
| `progression economy critters` | `config critters` |
| `progression economy gambling` | `config gambling` |
| `progression economy partygames` | `config partygames` |
| `progression economy points-give` | `points give` |
| `progression economy points-settings` | `config points` |
| `progression economy points-take` | `points take` |
| `progression economy rep-roles-add` | `config rep-roles add` |
| `progression economy rep-roles-list` | `config rep-roles list` |
| `progression economy rep-roles-remove` | `config rep-roles remove` |
| `progression economy store-add` | `config store add` |
| `progression economy store-list` | `config store list` |
| `progression economy store-remove` | `config store remove` |
| `progression economy surge` | `config surge` |
| `progression economy tools` | `economyv2 tools` |
| `progression leveling roles-add` | `config level-roles add` |
| `progression leveling roles-list` | `config level-roles list` |
| `progression leveling roles-mode` | `config level-roles mode` |
| `progression leveling roles-remove` | `config level-roles remove` |
| `progression leveling roles-sync` | `config level-roles sync` |
| `progression leveling settings` | `config leveling` |
| `progression leveling xp-add` | `xp add` |
| `progression leveling xp-clear` | `config xp clear` |
| `progression leveling xp-curve` | `config xp curve` |
| `progression leveling xp-import-mee6` | `xp import mee6` |
| `progression leveling xp-multiplier` | `config xp multiplier` |
| `progression leveling xp-public` | `config xp public` |
| `progression leveling xp-remove` | `xp remove` |
| `progression leveling xp-reset` | `xp reset` |
| `progression leveling xp-reset-all` | `xp reset-all` |
| `progression leveling xp-set` | `xp set` |
| `progression leveling xp-show` | `config xp show` |
| `progression leveling xp-sources` | `config xp sources` |
| `progression leveling xp-tuning` | `config xp tuning` |
| `progression leveling xp-voice` | `config xp voice` |
| `progression leveling xp-voice-role-add` | `config xp voice-role-add` |
| `progression leveling xp-voice-role-list` | `config xp voice-role-list` |
| `progression leveling xp-voice-role-remove` | `config xp voice-role-remove` |
| `purge` | `purge` |
| `qotd queue` | `community qotd queue` |
| `qotd settings` | `community qotd settings` |
| `qotd suggest` | `qotd suggest` |
| `quests` | `quests` |
| `rank` | `rank` |
| `remind cancel` | `remind cancel` |
| `remind list` | `remind list` |
| `remind me in` | `remind me in` |
| `remind me when` | `remind me when` |
| `remind repeat` | `remind repeat` |
| `remind repeat-cancel` | `remind repeat-cancel` |
| `remind repeat-list` | `remind repeat-list` |
| `rep` | `rep` |
| `rewards chest` | `economyv2 chest` |
| `rewards donate` | `economyv2 donate` |
| `rewards jobs` | `economyv2 jobs` |
| `rewards monthly` | `monthly` |
| `rewards prestige` | `economyv2 prestige` |
| `rewards provisions` | `economyv2 provisions` |
| `rewards season` | `economyv2 season` |
| `rewards weekly` | `weekly` |
| `roblox unverify` | `roblox unverify` |
| `roblox update` | `roblox update` |
| `roblox verify` | `roblox verify` |
| `roblox whois` | `roblox whois` |
| `safety antinuke disable` | `antinuke disable` |
| `safety antinuke enable` | `antinuke enable` |
| `safety antinuke events` | `antinuke events` |
| `safety antinuke limits` | `antinuke limits` |
| `safety antinuke status` | `antinuke status` |
| `safety antinuke trust-add` | `antinuke trust add` |
| `safety antinuke trust-remove` | `antinuke trust remove` |
| `safety antinuke vip-add` | `antinuke vip add` |
| `safety antinuke vip-remove` | `antinuke vip remove` |
| `safety automod exempt-channel` | `automod exempt channel` |
| `safety automod exempt-role` | `automod exempt role` |
| `safety automod profile` | `automod profile` |
| `safety automod show` | `automod show` |
| `safety automod test` | `automod test` |
| `safety automod toggle` | `automod toggle` |
| `safety moderation log` | `config log` |
| `safety moderation log-events` | `config log-events` |
| `safety moderation log-ignore` | `config log-ignore` |
| `safety moderation policy` | `modtools policy` |
| `safety moderation settings` | `config moderation` |
| `safety moderation warn-ladder` | `config warn-ladder` |
| `safety verification message` | `verification message` |
| `safety verification mode` | `verification mode` |
| `safety verification post` | `verification post` |
| `safety verification setup` | `verification setup` |
| `setup backup export` | `backup export` |
| `setup backup import` | `backup import` |
| `setup backup list` | `backup list` |
| `setup backup restore` | `backup restore` |
| `setup branding apply` | `branding apply` |
| `setup branding reset` | `branding reset` |
| `setup branding show` | `branding show` |
| `setup commands alias` | `commandcontrol alias` |
| `setup commands custom-create` | `customcommand create` |
| `setup commands custom-delete` | `customcommand delete` |
| `setup commands custom-edit` | `customcommand edit` |
| `setup commands custom-info` | `customcommand info` |
| `setup commands custom-list` | `customcommand list` |
| `setup commands reset` | `commandcontrol reset` |
| `setup commands set` | `commandcontrol set` |
| `setup commands show` | `commandcontrol show` |
| `setup roles auto-add` | `autoroles add` |
| `setup roles auto-list` | `autoroles list` |
| `setup roles auto-remove` | `autoroles remove` |
| `setup roles auto-sticky` | `autoroles sticky` |
| `setup roles color-setup` | `color setup` |
| `setup roles join-role` | `config join-role` |
| `setup roles panel-add` | `roles add` |
| `setup roles panel-create` | `roles panel` |
| `setup roles panel-post` | `roles post` |
| `setup roles panel-remove` | `roles remove` |
| `setup roles panel-temp` | `roles temp` |
| `setup server language` | `config language` |
| `setup server modules-channels` | `config modules channels` |
| `setup server modules-show` | `config modules show` |
| `setup server modules-toggle` | `config modules toggle` |
| `setup server prefix-reset` | `config prefix reset` |
| `setup server prefix-set` | `config prefix set` |
| `setup server prefix-show` | `config prefix show` |
| `setup server show` | `config show` |
| `setup welcome card` | `config welcome-card` |
| `setup welcome goodbye` | `config goodbye` |
| `setup welcome message` | `config welcome` |
| `setup workflows list` | `workflow list` |
| `setup workflows runs` | `workflow runs` |
| `setup workflows toggle` | `workflow toggle` |
| `shop catalog` | `shop catalog` |
| `shop market` | `shop market` |
| `stall auction` | `economyv2 auction` |
| `stall auction-close` | `economyv2 auction-close` |
| `stall bid` | `economyv2 bid` |
| `stall manage` | `stall manage` |
| `stall sell` | `stall sell` |
| `stall trade` | `economyv2 trade` |
| `stall trade-review` | `economyv2 trade-review` |
| `stall view` | `stall view` |
| `stats channel` | `stats channel` |
| `stats me` | `stats me` |
| `stats server` | `stats server` |
| `store` | `store` |
| `suggest` | `suggest` |
| `suggestion consider` | `suggestion consider` |
| `suggestion implement` | `suggestion implement` |
| `suggestion note` | `suggestion note` |
| `suggestion thread` | `suggestion thread` |
| `surge` | `surge` |
| `tag` | `tag` |
| `ticket add` | `ticket add` |
| `ticket claim` | `ticket claim` |
| `ticket close` | `ticket close` |
| `ticket escalate` | `ticket escalate` |
| `ticket force-open` | `ticket force-open` |
| `ticket move` | `ticket move` |
| `ticket note` | `ticket note` |
| `ticket priority` | `ticket priority` |
| `ticket remove` | `ticket remove` |
| `ticket rename` | `ticket rename` |
| `ticket reopen` | `ticket reopen` |
| `ticket request-close` | `ticket request-close` |
| `ticket snippet` | `ticket snippet` |
| `ticket transcript` | `ticket transcript` |
| `ticket transfer` | `ticket transfer` |
| `ticket unclaim` | `ticket unclaim` |
| `timeout` | `timeout` |
| `verify` | `verify` |
| `voice claim` | `voice claim` |
| `voice hide` | `voice hide` |
| `voice invite` | `voice invite` |
| `voice kick` | `voice kick` |
| `voice limit` | `voice limit` |
| `voice lock` | `voice lock` |
| `voice panel` | `voice panel` |
| `voice rename` | `voice rename` |
| `voice show` | `voice show` |
| `voice transfer` | `voice transfer` |
| `voice unlock` | `voice unlock` |
| `warn` | `warn` |
| `wave` | `wave` |

<!-- registration-tree:end -->
