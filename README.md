# Pinchora

A cozy pixel world for Discord servers: an isometric house you build and furnish from 1,800+
items, a pixel character, pets, fishing, a shared server plaza, levels and pixel-art cards.
Written in Rust and built to run sharded on Kubernetes.

![A furnished room](docs/images/room.png)

![Profile card](docs/images/profile.png)

![Character creator](docs/images/character.png)

![Leaderboard](docs/images/leaderboard.png)

## What works today

Command-tree consolidation (slices 1–18): **62 registered roots**, covering all **330 executable actions**.
The eight planned feature families project **70 roots**. `/setup` has 21 groups;
`/mod` keeps each action's own permission checks, and `/rewards` groups weekly/monthly claims.
Old prefix paths and server aliases resolve to the new paths, with a daily English/Turkish notice.
Private setup actions redirect to slash. Existing colliding server aliases keep their text meaning;
new collisions are rejected, and existing custom commands remain available through `/tag`.
Migration `0066` rewrites policy keys and alias targets, expanding split parents while retaining
independent restrictions. Old version-1 backups normalize before validation and import.


| Command | What it does |
|---|---|
| `/profile [user]` | Pixel-art profile card: level, XP, shells, reputation, streak, global and server rank, bio, an optional title under the name, up to six badges, each on a plate tinted by its rarity, and, if the player chose any, a **showcase** row (favourite fish, pet, furniture, house thumbnail) and their style (accent colour, panel opacity, frame). Until you choose your badges the card shows your best earned ones (rarest first) on its own. With nothing on the showcase the card keeps its classic layout; with a showcase the about-me panel and the stats give up height to a row of tiles. With a partner (`/partner`), the corner of the about-me panel shows `♥ NAME`, measured as drawn so accented, non-Latin and emoji names keep the heart beside them. |
| `/rank [user]` | Server level card, with the next level role to earn, the title under the name and the player's card style |
| `/customize background` | Private editor with a live preview of your profile card: 36 backgrounds in five rarities, bought once with shells (**Buy**), then worn (**Equip**) or taken off (**Remove**); the menu lists owned ones first, then the shop, 25 per page. The profile, rank and wallet cards draw the background under their panels and take their accent colour from it |
| `/customize badges` | Private editor with your badge collection card: all 41 badges in a grid, earned ones in colour on a rarity plate, locked ones dark with a progress bar, a `23/41` counter and a NEW tag on what you earned since you last looked. 28 achievements worked out on the spot from real progress (level, fish species, house likes, pets, daily streak, furniture placed, reputation, shells), 12 shop badges bought once with shells, and Cake Day granted on a celebrated birthday; wear up to six, in the order you pick them, take them all off, or press **Show my best automatically** to let the card choose again. A badge you no longer qualify for (a lapsed streak, spent shells) leaves the card until you earn it again |
| `/customize title` | Private editor for the title under your name on the profile and rank cards: 29 earned by stats (level, fish species, house likes, pets, streak, furniture, reputation, shells) or by owning a badge, plus the special *House of the Week* for contest winners; locked titles show what they still need. **Remove** takes it off |
| `/customize style` | Private editor with a live preview of your profile card: an **accent colour** (Auto follows the background; 12 pixel-palette presets; or **Hex code…**, a form that refuses unreadable near-black values), **panel opacity** (Light, Normal, Solid; offered once you wear a background picture, since there is nothing for it to show over before) and a pixel **frame** drawn around the whole card (Plain and Rope free; Coral from level 10, Pearl 20, Gold 30, Neon 40, shown with a lock and the level until then). **Reset style** puts it all back, a frame saved above your level included. The style is on the profile, rank and wallet cards |
| `/customize showcase` | Private editor with a live preview: up to three picks for the profile card, a favourite **fish** (from your fish book), a **pet** and a **furniture** item (owned or placed), chosen from menus of what you have (25 per page), plus a **Show house** switch for a thumbnail of your first room (drawn once and kept until the room changes). Sprites are drawn at whole-number scales only, so every pixel stays a crisp block: small ones are enlarged up to 4×, big ones reduced by a whole divisor without blending colours. **Clear slot** and **Clear all** empty it again; empty slots are simply not drawn |
| Badge unlocked notices | When a command or chat level-up earns you an achievement badge (`/daily`, `/fish`, `/pet`, placing furniture, `/give`, a new chat level), a short "Badge unlocked" embed with the badge art follows the reply, once per badge |
| `/leaderboard [board]` | Server XP, server points, **Weekly XP** (resets Monday 00:00 UTC, with last week's top three in a panel under the board), **Weekly catch** (each player's biggest catch of the week by size for its species, so a 7.9 cm minnow can beat a 200 cm sailfish; boots and tins do not count; same Monday reset and last-week podium), global XP or global shells as a pixel-art card, with page buttons |
| `/invites user [user]`, `/invites top [all\|week\|month]` | Pixel invite card and a paged server leaderboard; counts eligible joins, accounts too new to count, leavers and manager bonuses |
| `/invites temp` | Private expiring, limited-use friend link when enabled; the bot needs Create Instant Invite in the selected channel |
| `/integrations invites settings`, `bonus-add`, `bonus-remove`, `reward-add`, `reward-remove`, `post` | Manage invite tracking, bonus counts, reward roles and the friend-link button (Manage Server) |
| `/daily` | Daily shells; the streak bonus grows for 20 days |
| `/rewards weekly`, `/rewards monthly` | A bigger reward each, on a cooldown of their own: 1,500 shells every 7 days and 6,500 every 30. The cooldown counts from the claim, and a claim made at the same moment as another pays once |
| `/balance [user]`, `/give user amount` | Wallet and transfers. The wallet card shows the time left on the daily, weekly and monthly reward side by side (`ready` in green once it can be collected) |
| `/remind me when daily\|weekly\|monthly\|pet\|trip on\|off` | Opt in to a direct message when a reward is ready again (scheduled when you collect it, or at once for one already counting down), when your active pet needs looking after (hunger, fun or hygiene down to 30, no sooner than two hours after you last looked after it; not while it is away or asleep) or when a pet is home from a trip. One message each time, then it waits for the next claim, trip or care |
| `/remind me in duration text` | A note to yourself by direct message: `45m`, `2h30m`, `1d 12h` (a minute to a year; units w d h m s). The text is up to 200 characters and loses any mention (users, roles, channels, `@everyone`, `@here`). Up to 25 open at a time |
| `/remind list`, `/remind cancel id` | Which notifications are on and the reminders still to come, with a menu to cancel one; or cancel by the number the list shows |
| `/privacy [server_dms]` | Private panel with four switches and a delete button (the `server_dms` option sets the last switch directly). **Profile** private: others who look you up with `/profile`, `/rank`, `/balance`, `/friends list`, `/partner view`, `/play stats` or `/dex` see "This profile is private." (your own cards are never hidden from you). **Boards** hidden: you are left off the global XP and shell leaderboards, and the ranks of others count without you. **House** hidden: not offered in the `/house visit` shortlist and refused by name too, and its guestbook (notes and visitors) cannot be read or signed and the house cannot be liked (stronger than `/house lock`). **Server DMs** off: no server's custom commands, auto-responders, level-ups, suggestion updates or moderation notices (warns, timeouts, kicks, bans) send you direct messages (your own `/remind` messages still arrive). A private profile also hides your `/points show` and `/stall view`. **Delete my data…** asks twice (the second button works for two minutes) and removes every row about you in every table: account, house, pets, friends, notes, votes, suggestions, polls, giveaway entries, history and the rest (a server's custom commands you wrote stay, no longer saying who wrote them). A running duel's wager goes back to the other player, a derby you host gives its racers their stakes back, a giveaway you host is called off, and likes you gave come off the houses. You come back with nothing but not with fresh waiting times: see below |
| `/info wrapped [year]` | A tall card with your year: the busiest month and a bar for each, messages, server XP (chat and voice), shells earned (a duel counts what you won, not your own stake), fish caught, houses visited, pets cared for, likes given and received, your favourite fish of the year, the house you visited most (a friend's first) and a fun title. Open from 1 December to 31 January; outside that it says when it opens. Pinchora has counted per month since this feature began, so earlier months of your year only fill in as you play |
| `/rep user` | One reputation point per day; a server's reputation roles are given the moment the receiver's total reaches them, and the reply says so |
| `/customize bio text` | About-me text on the profile card |
| `/setup welcome message`, `/progression leveling settings`, `/setup server show` | Welcome cards and level-up messages. `/progression leveling settings` takes `enabled`, `channel`, `message` (placeholders `{user}` `{level}` `{server}` `{role}`, the last being the reward role just earned, empty when none) and `mode`: `channel` (the level-up channel, else where they chatted), `current` (always where they chatted), `dm` (a direct message; closed DMs are skipped quietly) or `off` (no message, roles still given) (Manage Server) |
| `/progression leveling roles-add\|roles-remove\|roles-list\|roles-mode\|roles-sync` | Roles given at a level (up to 25): `mode` keeps every role earned or only the highest, `sync` catches up members who already passed a level (one at a time per server, from Discord or the dashboard, with a 5 minute rest after each). Who may set up which role is in "Who may hand out a role" below (Manage Server) |
| `/progression leveling xp-multiplier value [channel] [role]` | Scale the server XP and points a channel (text, voice or forum) or category, or a role, earns, from 0 to 5; 0 means no XP, 1 is normal and drops the rule. Threads and forum posts follow their channel, and a channel follows its category unless it has a rule of its own (the nearest rule wins). A channel's multiplier is multiplied by the member's highest role multiplier, a no-XP role always wins, and `@everyone` is a server-wide multiplier. Global XP is never scaled. Up to 50 rules (Manage Server) |
| `/progression leveling xp-clear [channel] [role]`, `/progression leveling xp-show` | Remove one multiplier or all of them, or list the multipliers with the voice settings (Manage Server) |
| `/progression leveling xp-voice [enabled] [amount]` | Voice XP, off by default: members in a voice channel with at least one other person earn `amount` (1 to 100, default 10) server XP a minute; nothing while alone (bots do not count), muted and deafened at once, or in the server's AFK channel. Multipliers apply by the voice channel (or its category) and the member's roles; level-ups follow the leveling mode. Switching it on does not see who is already in voice: they start earning when they next join, move, mute or unmute (Manage Server) |
| `/progression economy rep-roles-add\|rep-roles-remove\|rep-roles-list` | Roles given at a reputation count (up to 25, same safety checks as level roles); they always stack and are given when a `/rep` point lands (Manage Server) |
| `/setup roles join-role [role] [off]` | A role every new member gets, bots excepted; with neither option it shows the current setting (Manage Server) |
| `/setup welcome goodbye [channel] [message] [off]` | Goodbye message in a channel when a member leaves; placeholders `{user}` `{server}` `{members}` (Manage Server) |
| `/safety moderation log [channel] [off] [category]` | The server log's channel (this turns it on), or with `category` a channel of its own for one of the nine categories (members, messages, roles, channels and threads, voice, moderation, server, invites, emojis and stickers); `off` with a category sends it back to the log channel (Manage Server) |
| `/safety moderation log-events [members] [names] [bans] [voice] [messages] [roles] [channels] [moderation] [server] [invites] [emojis]` | Switch kinds of log entries on or off; all start on except self mutes, so picking a log channel is enough (a server that logged before keeps only its four old kinds on). Single events are switched on the dashboard (Manage Server) |
| `/safety moderation log-ignore [channel] [role] [user] [bots] [remove]` | Members, members with a role, channels (a category covers its channels, a channel its threads) and bots the server log leaves alone, up to 50 of each; `remove` takes one off; no options shows the lists (Manage Server) |
| `/safety moderation settings [channel] [off] [dm]` | The mod-log channel every case is posted to (`off`: the server log's moderation channel), and whether members are told by direct message (Manage Server) |
| `/safety moderation warn-ladder [warns] [action] [duration] [remove]` | What a number of active warns brings: a timeout (with a duration), a kick, or a ban (temporary with a duration); up to 10 rungs, the same count again replaces one; `remove` takes one off; no options shows the ladder (Manage Server) |
| `/setup roles panel-create title [description] [style] [exclusive] [required] [min] [max]` | Start a role panel (a draft); the same title again changes only what is given. `style` is `buttons` (the default), `select` (a menu) or `reactions`; `exclusive` makes picking a role swap the member's other role of the panel; `required` is a role members need to use it (@everyone takes the need away); `min` and `max` (0 to 25, 0 is no limit) are the fewest and most roles of the panel a member holds (Manage Server) |
| `/setup roles panel-add panel role label [emoji] [description]` | Put a role on a panel (up to 25); adding a role again changes its label, emoji and line. `panel` is the title or the number; `description` is the line under the label in a menu; a reactions panel needs an emoji on every role (Manage Server) |
| `/setup roles panel-remove panel role`, `/setup roles panel-post panel channel` | Take a role off, or post the panel; posting again in the same channel updates the message in place, and a reactions panel gets its emojis put under the message (Manage Server) |
| `/setup roles panel-temp role duration [panel]` | Make a role last only `duration` (`12h`, `7d`; a minute to a year; `off` for good) after a member takes it from a panel, on one panel or on every panel that has it (Manage Server) |
| `/setup roles auto-add role [kind] [delay] [remove_after]` | Give a role to everyone who joins (up to 10 a server): `kind` is `join` (members, the default), `bot` or `after_verify` (when the join gate lets a member in); `delay` waits (30 seconds to 30 days) and `remove_after` takes it back again (a minute to a year); adding the same role and kind again changes it. Same safety checks as level roles (Manage Server) |
| `/setup roles auto-remove role [kind]`, `/setup roles auto-list` | Stop giving a role (of one kind, or every kind), or list the auto roles with whether sticky roles are on (Manage Server) |
| `/setup roles auto-sticky enabled` | Sticky roles: a member who leaves and comes back within 60 days gets back the roles they held, as far as the bot can give them (not roles above its own, roles with a dangerous permission, roles an integration owns, or roles whose time ran out while they were away). Switching it on starts from the roles members hold now (Manage Server) |
| `/color pick`, `/color list`, `/color remove` | Colour your name with a role: the menu offers the palette (the card colours that read on Discord's dark chat) and the server's own colour roles, with a card showing the pick on a name; a member holds one colour at a time. Roles are made the first time somebody picks a colour |
| `/color set hex` | Any colour by hex code (`#ff6b4a`), when the server's mode is `hex`; colours too dark to read are refused |
| `/setup roles color-setup [enabled] [mode] [allow] [disallow] [anchor] [roles]` | Switch colour roles on, choose `palette` or `hex` mode, name the roles allowed to pick (none named means everyone), the anchor role new colour roles are made under, and own roles to offer as colours (mention them, up to 10). A server holds up to 32 colour roles (Manage Server) |
| `/points show [user]` | Server points balance and rank; the server can rename the points and pick their emoji |
| `/points give\|take user amount` | Adjust a member's server points (Manage Server) |
| `/progression economy points-settings [enabled] [name] [emoji] [min] [max]` | Turn server points on or off, rename them, and set what a message earns (Manage Server) |
| `/store` | The server's role store: pick a role from the menu to buy it with server points |
| `/progression economy store-add\|store-remove\|store-list` | Put roles on sale for points, with optional stock (up to 25); adding a role again updates its price and stock (Manage Server) |
| `/giveaway start duration winners prize [shells] [channel] [required_role]` | Post a giveaway with an **Enter** button (press again to leave): `duration` like `30m`, `2h`, `1d12h` (1 minute to 30 days), 1 to 20 `winners`, an optional `shells` prize **per winner**, a `channel` (default: here) and a `required_role`. Up to 20 run at once per server (Manage Server) |
| `/giveaway end id`, `/giveaway reroll id [winners]`, `/giveaway list` | End one now and pick its winners; pick new winners from the entrants who have not won yet (a reroll never pays shells, and the announcement of a shells giveaway says so); list the running ones with entries and links. `id` is the number in the giveaway's footer (Manage Server) |
| `/community server suggestions [channel] [off]` | Where `/suggest` posts; with neither option it shows the setting, and `/setup server show` lists it (Manage Server) |
| `/progression economy critters [enabled] [channel] [anywhere]` | Let critter drops and bottles wash up while people chat (off until you turn it on): with a `channel`, only chat there counts and the drops land there; with `anywhere`, they land in whichever channel tipped the count. With none of the options it just shows the setting, and `/setup server show` lists it (Manage Server) |
| `/suggest text` | Post a suggestion (up to 1000 characters) in the server's suggestion channel as an embed with 👍 and 👎 buttons showing live counts. One vote per member: press the other side to switch, the same side again to take the vote back. One suggestion per 30 seconds |
| `/mod suggestion approve\|deny id [reason]` | Decide on the server's suggestion number `id` (the `#n` in its title): the embed turns green or red with who decided and why, its buttons lock with the final counts, and the author gets a direct message (closed DMs are reported to you, not an error). Decided once (Manage Server) |
| `/poll question options [duration]` | A question with two to five answers separated by `\|`, up to 80 characters each; `duration` like `30m`, `6h`, `1d` (default 1 day, 1 minute to 14 days). One button per answer plus **Results**; one vote per member, the same switch/take-back rule as suggestions. Votes stay hidden while it runs, **Results** shows a private pixel bar-chart card (drawn at most every 10 seconds per poll), and when it closes the channel gets the final card and a line saying who won (or the tie). Three open polls per member |
| `/ban user [reason] [duration] [delete_days]` | Ban a member, or anyone by user id: for good, or for a while (`12h`, `7d`, a minute to a year; the ban is lifted on its own), deleting 0 to 7 days of their messages (Ban Members) |
| `/mod unban user [reason]`, `/kick user [reason]` | Lift a ban (paste the user id); remove a member, who can join again (Ban Members, Kick Members) |
| `/timeout user duration [reason]`, `/mod untimeout user [reason]` | Time a member out for a minute to 28 days, or end it early; administrators can't be timed out (Timeout Members) |
| `/warn user reason`, `/mod warnings user`, `/mod unwarn case [reason]` | Warn a member (the answer says how many warns they have, and the warn ladder acts when a count is reached); a private list of the warns that still count; take a warn back so it stops counting, over a member and a warn's moderator below you (Timeout Members) |
| `/purge count [user] [bots] [contains] [links] [attachments] [reason]` | Delete up to 100 recent messages here, never pinned ones or ones older than 14 days, matching every filter given; `contains`, `links` and `attachments` need the Message Content intent (Manage Messages) |
| `/mod slowmode duration [channel] [reason]` | One message every `5s` to `6h` per member in a channel, or `off` (Manage Channels) |
| `/mod lock [channel] [reason]`, `/mod unlock [channel] [reason]` | Take sending messages, sending in threads and making threads from @everyone in a channel, or give back exactly what @everyone had before (Manage Channels; the bot needs Manage Roles there) |
| `/mod nick user [nickname] [reason]` | Change or reset a member's nickname (Manage Nicknames) |
| `/mod role add\|remove user role [reason]` | Give or take a role below both your highest role and the bot's; managed roles and @everyone are refused, and so is a role with a dangerous permission (Administrator, Manage Server, Ban Members and the like) unless you own or administer the server (Manage Roles) |
| `/mod case id`, `/mod reason id reason` | Look a case up privately; give it a new reason, which edits its mod-log entry too: your own cases, or those of a moderator below you (any of Ban, Kick or Timeout Members, Manage Messages or Manage Server) |
| `/tag name [args]` | Run one of the server's custom commands (see "Custom commands and auto-responders" below). `name` suggests the server's commands as you type, by name and description; `args` (up to 200 characters) are what the command can use as `{args}` and `{1}` to `{5}`. A command can have a cooldown per member, allowed and denied roles and channels, and answer privately; what the people who ran it typed can never ping anyone |
| `/setup commands custom-create name [description] [private]` | Write a command: a form asks for its reply (up to 2,000 characters, with placeholders), an optional description and cooldown. Names are 1 to 32 characters of `a-z 0-9 - _` and cannot be one of Pinchora's own commands; 50 per server (Manage Server) |
| `/setup commands custom-edit name [private]`, `custom-delete name`, `custom-list`, `custom-info name` | Change a command's first reply, description and cooldown in the same form (and whether it answers privately), remove it, list them with their use counts (paged), or see how one is set up. Embeds, several replies, actions and role or channel limits are on the dashboard (Manage Server) |
| `/ping` | Health: how fast the interaction, the database and Redis answer |
| `/help` | A private guide drawn as pixel cards, one area at a time (profile, shells, shops, house, pets, fishing, friends, duels, puzzles and party games, crowd games and collecting, community tools, moderation, four pages of server setup). Every command and subcommand has its own line with what it does, and a star marks the ones for members who can manage the server. Jump to nearby pages with the menu (up to 25 at a time) or reach any page with the previous/next buttons; the pages are built from the registered command tree, and a unit test fails until a new command is given an area |
| `/language set language` | The language Pinchora answers you in, on every server: English, Türkçe, or Automatic (the default: your Discord client's language). Private. Without a pick, a member whose Discord is in Turkish gets Turkish replies, cards and slash-command descriptions; the server's `/setup server language` only fills in where Discord sends no language. Adding a language: `crates/pinchora-core/locales/CONTRIBUTING.md` |

Chatting earns 15–25 XP and, by default, 1–3 server points per minute per server. Level roles are
given on level-up, and the bot needs the Manage Roles permission with its role above the roles it
hands out; roles the bot can't give (@everyone, integration-managed roles, roles above the bot's own)
are refused when they are added to the level roles, the store, the join role or a role panel. If
Discord refuses a store role at purchase time, the points and stock are returned. New members get a
welcome card when enabled.

Reminders wait in a table (`reminders`), one row per message with the time it is due. One ticker on
every pod looks every 30 seconds, takes the due rows in batches with `FOR UPDATE SKIP LOCKED`, leases them
for two minutes while the messages go out and deletes each one once sent, so a restart delays a message
and two pods never send the same one. A player with closed direct messages is logged at debug level and the
reminder is dropped. A message about a reward, a pet or a trip is checked again just before it is sent: if
the player switched it off, claimed the trip or looked after the pet meanwhile, it is dropped or moved
instead of sent.

Wrapped counts per user and calendar month (UTC) in `activity_monthly`, bumped where things already
happen: chat and voice XP, every positive ledger entry that is not a transfer from another player or a refund,
fish caught, the first visit to a house each day, house likes, and pet care. XP is the server XP of chat
and voice alike (a message counts its server XP, not the global share). The favourite fish of a year comes from
`activity_fish`, which counts catches per year, and the most visited house from the visit log, which has
history of its own.

"Delete my data" leaves a tombstone: a row in `deleted_users` with the player's id and when they last used
the daily, weekly and monthly rewards and rep, which houses they liked and how many paid visits they made
today, whether they had played today's daily wordle, the last daily picross they solved, the day of their last
free pet wake-up, a 45 minute walk lock when they had a pet (its cooldown is kept in Redis under the pet's id,
which a new pet does not share) and the beachcomb wait they had left. A trigger on `users` gives a returning
player the daily, rep, free wake-up, weekly and monthly times back when their row is created again, so every
reader of those sees them; the paid-visit and like checks, the wordle and picross payouts, the free wake-up
check, the pet walk and the beachcomb read the tombstone themselves. The games' limits of the day
(`collection_daily`: paid critter grabs, bottles, beachcombs and pot hauls; `party_daily`: derby and shell game
losses and plays, heist, storm and share-or-grab payouts, paid word party games) are not deleted for today,
only for the days before, so a returning player picks the day up where it was; the game win caps of
`pay_win` live in Redis and are untouched. The dex goes with the data, so a returning player discovers again,
but only as many plays pay as the day allows. Racers who staked shells in a derby the player hosts get them
back, word party games and lobbies they host are called off, their critter-drop cards become free, and the
dashboard's audit log keeps their changes under "Deleted user". A guest's paid visits of
the day outlive a host who deletes their data (the host's id on the visit becomes a number from a sequence that
is nobody's). A delete and a claim of the same player never overlap: the delete, the trigger that creates a
player's row, and every claim of a per-player limit take the player's advisory lock (`lock_user(id)`) and keep
it to the end of their transaction, so a claim that lands while a delete is still in flight waits for it and
then finds the tombstone.

One lock order holds for every transaction that touches a player's rows (written down in
`crates/pinchora-db/src/locks.rs`): first the locks of all its players at once, in id order (a duel's two
players, a giveaway's host and winners, a listing's buyer and seller, everyone a delete pays back or takes a
like from), then the object it is about (session, giveaway, pet, listing, poll), then the writes. Chat XP
stays one statement that writes `users` first, so the trigger takes the lock before anything else. A delete
that finds a player it must lock only after it holds its first locks (a like landed in between) does not wait
for them: it tries the lock, and gives up and starts over with the full set when somebody holds it. The sweeps
that drop old rows of many players (games, giveaways, polls, weekly boards, tombstones) skip the rows somebody
holds and take them next time. Postgres still breaks any deadlock this misses by rolling one side back (two
friends who delete their data at once cross on the rows they share); every transaction that takes player
locks, and the sweeps, run again when they lose like that (up to three times, after a few random milliseconds;
a delete that met a player out of order starts over up to twelve more), which is safe because the lost
attempt left nothing behind and every claim checks what was already used inside its own transaction.
Tombstones are dropped after 35 days, by the reminder ticker. A test inserts into every table that names a
player and one reads the schema, so a new table or an id column (`*_by`, id arrays included) must be listed
in `crates/pinchora-db/src/privacy.rs` before it can ship.

Voice XP keeps who is in voice in Redis, written from the voice-state events, and one ticker on the
gateway pays every guild once a minute behind a per-minute lock, so a restart or a second gateway pod
never pays a minute twice. Servers with voice XP off are not tracked at all (the ticker drops one the
moment it finds it switched off), and the switch lives in Redis too: turning voice XP off (`/progression leveling
voice` or the dashboard) forgets everyone and makes every pod refuse to write who is in voice for ten
minutes, so a pod whose settings are a minute old cannot write a just-forgotten member back. Settings and
multiplier changes made with the slash commands are announced through Redis like the dashboard's, so every
pod forgets its copy at once (the five-minute voice view included). Discord sends each server's voice
states once per session in GUILD_CREATE: the gateway turns it into a small snapshot on the spot (who is in
voice, with the roles and bot flag of their member entries, the AFK channel, and where the channels and
threads sit) and drops the guild at once, so what waits is small; rebuilds queue for 16 slots of their
own instead of the 1,024 event slots, so an identify storm cannot make messages or joins drop. The
rebuild merges the snapshot into what Redis holds: a member a voice event wrote down after the snapshot was
taken (joined, moved, or left: a leave is a note in channel 0 for 15 minutes, not a hole) stays as the
event left it, and one the snapshot alone knows is added, so a rebuild that waited for its settings never
undoes an event that came in meanwhile. A leave missed while the bot was away stops paying, and members
already in voice at startup are paid at once (a server is also forgotten on GUILD_DELETE). A member the bot
has not heard from for a day is still dropped, in case a leave event was lost. A voice state the snapshot
has no member for waits for the next event. The ticker reads each server's settings and multipliers from a
five-minute cache, pays a few servers at a time, and runs one round at a time: a ticker that woke late pays
the minutes it missed, one after the other, and a minute that comes while the last round is still paying
is skipped, so rounds never stack on a slow database. A server's AFK channel that cannot be looked up and
was never known skips the server for the minute (the last known one is used otherwise). Multipliers look up
a channel's parent and category from Discord once and remember them for six hours, a channel Discord does
not know for ten minutes, and GUILD_CREATE fills them in for every channel and thread it lists. Weekly XP
counts every chat and voice XP grant under the Monday of its week; the hourly prune of weeks older
than six uses an index on the week and takes a per-hour lock, so only one gateway pod runs it.

Giveaways, suggestions and polls: a shells prize is paid by the **host**. `winners` x `shells` comes off
the host's balance in the transaction that starts the giveaway (so a server can never mint shells), each
winner is paid when it ends, and whatever nobody won (fewer entrants than winners, or none) goes back to
the host. A host who cannot cover it starts nothing. Winners are drawn uniformly without replacement,
the host cannot enter their own giveaway, and a required role is checked when a member presses Enter.
One ticker on every pod (gateway and interactions) wakes every 5 seconds; a giveaway ends and a poll
closes inside one database transaction that only one pod wins, so each ends exactly once, payouts
included, even across restarts, and the announcement is claimed before it is sent (a pod that dies
between the two is covered by the next tick, which tells the channel once the claim lapses after 2
minutes). The entrant count on a giveaway message changes at most every 5 seconds: the first entry shows
at once, quicker ones are shown by the ticker once the window is over. Everything members type
(prizes, questions, answers, suggestions, reasons) has user and role mentions removed and `@everyone` and
`@here` defused, and every message and reply these commands send sets `allowed_mentions` to none, so
winners are named but not pinged. Ended giveaways and closed polls are forgotten after 60 days.

Role panel buttons toggle the role and answer privately ("Added @Role" / "Removed @Role"). A button
only works while its role is still on that panel, so removing a role from a panel stops old
messages from handing it out. The server log remembers each member's last-seen nickname and username
in memory to show "old → new"; after a restart, or for a member it has not seen yet, a nickname
change is logged as just the new value. Voice moves work the same way: a member already in a voice
channel when the bot starts is only noticed once their state changes.

Moderation commands check the moderator's own permission (the one Discord would ask for: Ban Members for
`/ban`, Manage Channels in the channel named for `/mod lock`, and so on; Administrator stands for all of them)
from the member permissions Discord sends with the interaction, and the role hierarchy the way Discord does
(`pinchora_core::moderation::may_act`): nobody acts on the owner, on themselves or on the bot, and both the
moderator (unless they own the server) and the bot must sit strictly above the member. A refusal is private
and nothing reaches Discord. Every action opens a case numbered per server (`mod_cases`; the counter row in
`mod_case_counters` is taken by each insert and held to the end of its transaction, so moderators acting at
once get consecutive numbers, and the warn count read after it sees every earlier warn, so a ladder rung
fires exactly once), writes a line to the staff log, tells the member by direct message (warns, timeouts,
kicks and bans; before a kick or ban, since afterwards the member shares no server with the bot, and a kick
or ban Discord then refuses turns that message into one saying it did not go through; a server can switch
this off, a member's `/privacy` "Server DMs" switch is honoured, and a closed DM is noted on the case) and
posts the case to the mod-log (its own channel, else the server log's moderation channel), in the server's
language. Audit log reasons read `moderator: reason`. `/mod unwarn` follows the hierarchy against both the
warned member and the moderator who gave the warn, and `/mod reason` is for the case's own moderator or someone
above them (a case the bot opened: someone above its member); both look the members up on Discord. The warn
ladder (`mod_warn_thresholds`) acts as the bot, as a case of its own, when a warn brings a member's active
warns to a rung's count. `/mod role add|remove` uses the same rule as every role Pinchora hands out
(`settings::grant_refusal`): a role the bot can give, below the moderator's highest role, and none with a
dangerous permission unless they own or administer the server. `/mod lock` remembers what @everyone's
overwrite allowed and denied of the locked permissions, and `/mod unlock` puts exactly that back (a channel
locked by hand goes back to neutral). Temporary bans are lifted by a ticker on every pod every 30 seconds: due
bans are leased for two minutes with `FOR UPDATE SKIP LOCKED`, taken off with `UPDATE … WHERE active
RETURNING` right before Discord is asked (so a ban that was lifted or replaced meanwhile is left alone and
none is lifted twice), lifted, and closed with an unban case; a ban Discord no longer has counts as lifted,
one it refuses to lift stands again and waits an hour, and a new ban or an unban ends the old ban's clock.
A ban lifted on Discord by anyone else (by hand, another bot) ends the member's ban cases too: on the
gateway's unban event unless the member is banned again by then, and on the audit log's unban entry for the
cases opened before it, so the ticker never lifts a newer ban. Cases stay when a member deletes their data (they are the server's record, so warns and bans
cannot be wiped that way); the moderator's name is replaced by "Deleted user", as in the dashboard audit log.

The server log routes every event through `pinchora_core::logging`: an event is logged when its group's
older switch (members, names, bans, voice) is on and the event is not switched off on its own, into its
category's channel or the log channel, unless the ignore lists (members, roles, channels with what sits under
them, bots) leave it alone. Joins, leaves, names, avatars, voice (joins, leaves, moves, server mutes and
deafens, self mutes and deafens, the last off by default), bans and message edits, deletes and bulk deletes
come from the gateway events; roles, member roles, channels and threads (permission overwrites included),
the server's settings, invites, emojis, stickers, kicks and timeouts come from Discord's audit log as it
happens (`GUILD_AUDIT_LOG_ENTRY_CREATE`, which needs the bot to have View Audit Log), with who did it and what
changed; a ban waits a moment for the audit log to name who did it. What Pinchora did itself is left to its
case in the mod-log. Entries are written in the server's language (`/setup server language`). Without the
privileged Message Content intent the log says who edited a message, where and when, and for a deleted
message only where and when it was sent: Discord does not say whose it was, and nothing was kept to tell.
With the intent on (`PINCHORA_GATEWAY__MESSAGE_CONTENT=true`, the same switch the keyword auto-responders
use) recent messages are kept in Redis for a day (only for servers that log message edits or deletes, 1,000
characters each), and the entries show the text before and after, who sent a deleted message, its text and
attachment names, and the kept lines of a bulk delete. The staff log (`staff_actions`, 180 days) holds every
moderation command and every audit log entry made by hand by a member with a staff permission (Administrator,
Manage Server, Roles, Channels, Messages, Nicknames, Threads, Webhooks, Events or Expressions, Kick, Ban or
Timeout Members; not Pinchora's own, not a member's own nickname), and the dashboard reads it together with
its own audit log. A server that already had a log channel before the detailed log came keeps exactly what
it logged (joins and leaves, names, bans, voice joins, leaves and moves); every newer event starts off there
until it is switched on, while a server that picks its first log channel now gets them all. No new gateway
intent is needed: Guild Moderation (bans and audit log entries), Guild Messages (edits and deletes) and Guild
Voice States were there already; Server Members and Message Content are privileged and each has its own
switch. Invites and emoji or sticker changes come from the audit log too: the bot asks for neither the Guild
Invites nor the Guild Emojis and Stickers intent, so it needs View Audit Log for them.

Admin commands check the permission themselves. `/setup` (every subcommand), `/setup roles`, `/giveaway`,
`/mod suggestion`, `/progression economy points-give|take` and `/community server plaza-edit` (with the buttons and menus of its editor) need
Manage Server or Administrator, read from the member permissions Discord sends with each interaction,
on top of the command's default visibility: a server owner can change who sees a command in the
integration settings, so hiding or showing one never decides who may use it. A member without the
permission gets a private refusal and nothing changes.

### The pixel world

| Command | What it does |
|---|---|
| `/character` | Design your pixel character in one private editor with a big live preview on a floor tile: a **category** menu (Look, Skin, Hair, Eyes, Top, Bottom, Shoes, Accessories, Presets, Outfits), **◀ ▶** that step through the open category one click at a time, a menu that jumps straight to a choice, **Undo** (20 steps), **Randomise** (only from what you own) and **Save** / **Cancel**: everything is a draft until Save. 24 looks; six colour channels with 20 swatches each (eyes and shoes start as drawn) plus **Hex…**, a form for your own colour, nudged lighter or darker when the shading would not show; 31 **accessories** in five slots (head: straw sun hat, beanie, bow, sailor cap, flower crown, top hat, witch hat, captain's hat, crown; face: glasses, freckles, rosy cheeks, sunglasses, eyepatch, heart shades; neck: scarf, bandana, bow tie, shell and pearl necklaces; back: backpack, cape, surfboard, fairy and bat wings; hand: ice cream, balloon, flower, lantern, star wand, fishing rod), tried on freely, some free, some bought with shells (**Buy**), some opened by a level or a badge, most dyeable in 16 colours (**Dye ▶**); 10 ready-made **presets** (Beach Surfer, Ship Captain, Coral Witch…) and three **outfit** slots to keep looks in (**Keep in 1-3**, **Rename…**) and put back on. Save refuses an accessory that is not yours yet. The character, accessories included, stands in your house, joins you on visits and is on your profile, the partner card and the hug, pat, wave and high-five GIFs |
| `/house view [room]` | Your house, rendered tile by tile; your active pet wanders around; says when your guestbook has new notes |
| `/house edit [room] [owner]` | Decorate inside Discord: pick furniture, move the cursor with the ↖↗↙↘ arrows (on the floor or either wall), place, turn, flip, pick up. **↶ Undo** reverses the last 10 changes to the room (place, move, store, turn, flip, clear), even after reopening the editor, until the room is changed some other way; **Clear the room…** (first in the Go to menu, with a confirm step) stores every piece at once and is one undo step. With `owner` set to your partner you decorate their house: the pieces come from your own inventory, a piece you place belongs to the house's owner from then on (the editor says so), and anything stored goes to their inventory; moving their pieces is theirs to do, and your undo history is separate from theirs. A picked piece jumps to the nearest free spot that shows: a lamp or a rug is not put straight behind a bed or wardrobe that would hide most of it (the Go to menu lists those spots last, marked "mostly hidden"), unless nothing else is free |
| `/house visit [user] [room]` | Visit a house with your pet and character (first visit each day pays shells, 10 a day); **Sign guestbook** leaves the owner a note. Without a name it offers a menu of your friends whose doors are open, the most liked first |
| `/house guestbook [user]` | A house's guestbook, newest first (5 notes a page), and its last 10 visitors. Visitors sign after a visit: one plain-text note (140 characters, mentions and links left out) per house per day, not while the house is locked or the owner blocked them. Your own book is private and marks new notes; delete notes and block or unblock signers from its menus |
| `/house contest view\|enter\|vote\|winners` | The server's weekly room contest (weeks start Monday 00:00 UTC, one theme a week for everyone): `enter [room]` snapshots a room as it is (entering again updates it), `vote` shows other members' rooms one at a time with **Vote** and **Skip** (one vote per room), `view` shows the theme, time left, entries, your entry and last week's podium, `winners` draws last week's winning room. The first contest call after a week ends pays 500/250/100 shells to the three most-voted rooms and makes the winner **House of the Week** |
| `/house like user`, `/house lock` | Daily likes; close your doors to visitors |
| `/house decorate [room]` | 44 wallpapers and 44 floors, bought on first use, swapped freely |
| `/house upgrade` | Six tiers from Tide Pool Shack to Sunken Palace; furniture stays put |
| `/shop market` | Eight pieces every hour with stock shared by every player |
| `/shop catalog [category]` | 358 permanent designs in 23 aisles; 650 designs and 1,861 items with colour variants in all |
| `/inventory`, `/gift user` | What you own, and sending furniture to friends |
| `/stall sell\|view\|manage` | Your own player shop (8 listings, 5% market fee) |
| `/plaza view\|donate\|edit` | The server's shared plaza: members donate furniture, managers arrange it |
| `/partner propose user` | Ask someone to be your partner; they get **Accept** and **Decline** buttons for 10 minutes (and the ping: nobody else is pinged, and the same person cannot be asked again for a minute). You both must be free: a player has one partner at a time |
| `/partner view [user]` | The pair's picture with both characters, together since, days together, hugs, pats, waves and high fives they sent each other, and their houses' likes and pieces |
| `/partner leave` | End the partnership, after a confirm step |
| `/friends add user` | Ask someone to be your friend (**Accept** or **Decline**, good for a day; the same person cannot be asked again for a minute); up to 100 friends each |
| `/friends remove user`, `/friends list [user]` | Remove a friend, or page through a player's friends with their level and house likes (10 a page) |
| `/hug user`, `/pat user`, `/wave user`, `/highfive user` | A looping GIF (14 frames, about 120 KB) of your two pixel characters facing each other, drawn from the looks, colours and accessories you both chose (hats stay on through every hop and lean; a held item is in the hand the animation leaves alone) (a player without a character gets a stand-in made from their id): they step together with hearts, a hand bobs on a head, an arm waves, palms meet in a flash. "That's hug #12 from you to them", with the sender's and receiver's totals; the receiver gets a **Hug back** button, and is the only one pinged. One every 5 seconds per player |
| `/pet adopt\|view\|capsule\|rename` | Pets with needs that drift over time; feed, play, bathe, walk (a walk finds shells, more at a higher level); 42 species. Every care action, walk and trip earns XP: levels 1-30, a bar on the pet card, level-ups (and the trips they open) announced in the reply. A pet whose hunger, fun and hygiene all sit at zero for 24 hours falls asleep: its card shows it asleep, it earns no XP and cannot be looked after or sent away until **Wake up** (one free a day, else 75 shells), and in the house it lies on its side with a small "z z" above it instead of wandering. A care write is checked against the pet as it was read, so a click that races the start of a trip, a souvenir claim or another click is refused instead of unfreezing the needs or undoing the XP (the food is refunded). Never permanent, never lost. Unknown button actions answer with an error instead of bathing the pet |
| `/pet list` | The nursery: one card with every pet in a grid (portrait, name, level and XP bar, the four needs as icons, a star on the active pet, ASLEEP / AWAY / BACK! badges), 8 to a page (the card is only as tall as its rows: 1-4 pets get a half-height card), with a menu that opens one pet's card (care buttons, **Make active**, a way back to its page). A **Nursery** click on a public pet card opens the nursery as a private message of its own |
| `/pet trip go destination [pet]`, `/pet trip claim` | Field trips: send a pet (default: your active one) for 1, 4 or 8 hours to one of 8 cosy places from `content/pet_trips.json` (a place opens at a pet level, 1 to 26; the card's **Trip** button lists them). The pet leaves the house, its needs wait for it and it cannot be looked after meanwhile. Back, **Claim souvenir** (or `/pet trip claim`, which collects every pet that is home and otherwise says who is still away) pays shells, XP and a short story, and sometimes a free-to-earn furniture piece (`reward` items only, weighted by rarity, never shop stock). Higher levels find more shells and rarer pieces. A trip is a stored end time, so it survives restarts, and a claim pays exactly once |
| `/fish cast [spot]` | Cast, wait for the bite (a random 2 to 11 s), press **Reel!** in time; pressing early scares the fish away. Five spots open by global level (Shore 1, Reef 5, Kelp Forest 12, Deep Sea 20, Abyss 30) and the last one you fished is remembered; each has its own fish. A cast first rolls the kind of catch (junk, common, uncommon, rare, epic, legendary) with fixed shares per spot that the hour and the season never change, then picks among the fish of that kind that are about now, so the wage is the same at dawn, noon and midnight, in every month; time and season only choose which species of the kind bites. Every catch rolls a size in cm (normal around the species' usual size), is added to the fish book (72 species, with a record size each) and goes into your **bucket**, which holds 60 fish and stops the next cast until you sell some. A plain cast costs 10 shells of bait; a better bait on the hook is used up instead. The reply tells of a new species, a new record, a big one for the week, and any book milestone |
| `/fish shop` | Private tackle shop on a card: five rods (free starter, then 800, 3,500, 12,000 and 40,000 shells; each gives a longer Reel! window, 2.5 s up to 3.5 s, and a bigger chance at rare, epic and legendary fish) and three baits in packs of five (Glow Worm for uncommon fish, 65 shells; Silver Lure for rare ones, 140; Tide Crumbs for the spot's signature fish, 120; the last two are for hunting, not income: they lose shells at the shallow spots and Tide Crumbs cuts the Abyss wage, and the shop says so) |
| `/fish gear` | Private panel to pick the spot (closed ones show a lock and their level), the rod you own and the bait on the hook; a bait that runs out hands the hook back to plain bait |
| `/fish sell` | Your bucket on a card, with **Sell all** and **Sell duplicates** (keeps the biggest of each species), priced by rarity and size (fish of a species the game no longer has are not counted and **Sell all** clears them for nothing). The fish book never loses an entry or a record |
| `/fish book` | Your fish book on a card, eight species a page: record size and catch count for the ones you have, a shadow and where they live for the ones you have not, and the milestone rewards: **10 / 25 / 50 / 72** species pay 300 / 1,000 / 3,000 / 10,000 shells and a badge or profile background (Pocket Puffer, Brinebell Market, Lantern Jelly, Moonwhale Crossing). Each pays once; players who passed a milestone earlier are paid at their next catch |
| `/beachcomb` | Walk the shore (once every 20 minutes) for a find: shells, sea glass, driftwood, coins, now and then a critter; some only turn up at a time of day or in a season. A first find is a discovery for your dex and pays well, a spare is recycled for a few shells and fragments. A card shows what you found |
| `/critterdrop` | Drop three pixel critters in the channel on one picture with a numbered button each: the first click takes a card, nobody takes two, they drift away after 90 seconds. It follows `/progression economy critters` (off in a server until a manager turns it on, and kept to the set channel if there is one). You can call one every 30 minutes, a channel gets one every 10 minutes whoever calls, and one waits at a time per channel; chat can make them wash up too, but never on top of a drop that is still open in the channel |
| `/crabpot` | A crab pot you set for 1, 4 or 8 hours and haul in later (a stored end time, so it survives restarts): 1 to 1.8 catches an hour, more for a longer soak, drawn from the critters of the shallows and the finds. Five levels, bought with shells: a better pot catches more and finds rarer things. The haul is a card of everything it brought |
| `/dex [user] [tab]` | The Tide Pool Dex: a paged picture book of all 48 critters and 36 finds with a progress bar, how many you met of each, and the ones you have not found as dark silhouettes. **Spend fragments** trades what spares leave behind for a free-to-earn decor piece of the tier you pick (12 / 65 / 370 / 1,950 / 8,900 fragments from common to legendary); the piece is **soulbound**: it can be placed in your own house but never sold at a stall, gifted, donated to a plaza or placed in a partner's house |
| `/quests` | Three daily quests; clear all three for pearls. The **Claim rewards** button belongs to the day of its board: a button from yesterday says it expired and shows today's quests |
| `/duel tictactoe user` | Button tic-tac-toe; the winner earns shells |
| `/duel connect4 user [bet]` | Connect four on a pixel-art board: the challenged player accepts, then each turn is a column button (1-7) within 90 s; the winner earns shells |
| `/duel rps user [rounds] [bet]` | Rock paper scissors, best of 1, 3 or 5: each player gets private Rock / Paper / Scissors buttons, the channel sees each round revealed; ties replay |
| `/duel share user [bet]` | Share or Grab: the challenged player accepts, then each of you picks **Share** or **Grab** in secret (private buttons, two minutes). Both share: the pool is split. One grabs: they take it all. Both grab: nobody gets anything. The pool is the sea's 60 shells without a wager, or the two stakes with one (shares give the stakes back, a grab takes the pot, two grabs sink it: **in a wagered game both players grabbing loses both stakes, that is the game**); a player who never picks forfeits. The sea pays a pair once an hour, and the hour starts when a payout has gone through |
| `/duel seafleet user [bet]` | **Sea Fleet**, battleship on a 10×10 sea. After the challenge is accepted each captain places five ships in private (**Shuffle** draws a layout from every legal one with the bot's secure random source, as often as you like, then **Lock in fleet**; there are no fixed layouts to learn, and the layout you are looking at stays on the server, never in a button or a picture; fleets are never shown to the other captain). Then they take turns: pick a column and a row from two menus under the shared radar, which shows both players' waters, every hit and miss and the ships that have gone down. **My fleet** is your own waters, privately. 5 minutes to place, 2 minutes a shot |
| `/duel chess user [bet]` | **Reef Chess** with a rules engine written for the bot (castling, en passant, promotion, check, checkmate, stalemate, fifty-move rule, threefold repetition, insufficient material; checked against the standard perft counts). Pieces are pixel sea creatures (pearl, seahorse, conch, lighthouse, octopus queen, crab king); a move is two picks from menus under the board, the piece then where it goes (promotions list all four pieces, the queen first). Coral or sky is drawn at random; 5 minutes a move |
| `/duel checkers user [bet]` | **Pearl Checkers**, English draughts with pearls: compulsory jumps, multiple jumps, kings, a draw after 40 quiet moves each. The same two menus (piece, then move, where a jump chain is one entry); 3 minutes a move |
| `/play picross [mode]` | **Pixel Picross**: a 10×10 nonogram of a sea or house picture (92 pictures, each unique-solution checked, each also mirrored sideways, never upside down). `daily` (default) is the same puzzle for everyone each day, with a streak: 35 shells, plus 5 for each day of the streak up to a week; `practice` is a random puzzle (10 shells, 5 a day) from the pictures the daily will not show for a week. Pick a row and tick its filled cells from a menu, or **Mark cells** and type `A1 B3-B7 -C2` (a dash clears); **Check** says how many marks are wrong, never which, three times a puzzle, and a checked puzzle pays less (daily a flat 15 and no streak bonus, practice 5). The solved picture is revealed in colour, with a spoiler-free **Share result** |
| `/play 2048` | **Tide 2048** with a fish from the atlas on every tile (shrimp up to a leviathan at 2048): four arrow buttons, the game is kept between sessions, **Finish** counts it. The best score shows in `/play stats`; a finished game pays 5 to 60 shells for a tile of 256 or more (5 paid games a day) |
| `/play memory`, `/play minesweeper [size]`, `/play higherlower`, `/play anagram`, `/play typing`, `/play guessnumber` | Private pixel-card rounds: match pairs, clear an 8×8 or 10×10 minefield, predict cards, unscramble a word, type a sentence, or find a secret number. Saved sessions, deadlines and game stats; English and Turkish. |
| `/duel othello user [bet]` | A pearl-and-coral Othello board with legal-move hints, coordinate entry, timed turns and the shared duel wager rules. |
| `/party nhie` | A family-friendly Never Have I Ever question with private, changeable votes and a five-minute reveal. |
| `/party fun mode [prompt] [sides]` | Ask the magic conch, choose from comma-separated options, or roll a die with 2–100 sides. |
| `/play hangman` | A cozy word (780 in five categories, and 500 in Turkish with all 29 letters of the Turkish alphabet) guessed from two letter menus while the tide washes a pixel sandcastle away; six misses lose |
| `/play wordle [mode]` | Five letters, six tries, typed into a form, shown as private pixel tiles with a keyboard summary. `daily` (default) is one word the same for everyone, kept per player per day, with a spoiler-free **Share result**; `practice` is a random word, as many as you like (**New word** on a board you had started, with a guess in, counts it as a lost game). A player whose Discord is set to Turkish gets the **Turkish wordle**: 1,247 answers and 6,001 guesses, the Turkish alphabet with a Turkish Q keyboard (Ğ Ü İ Ö Ş Ç, dotted `i`/`İ` and dotless `ı`/`I` kept apart), its own daily word and board (`daily-tr`), and one daily reward a day across both languages |
| `/play trivia [category]` | A question for the channel from 464 (8 topics, three difficulties; 564 in Turkish, 464 translated and 100 about Turkey): four answer buttons, 20 seconds, anyone answers once; the message then reveals the answer and who got it right |
| `/play derby [bet]` | Hermit Crab Derby: a lobby for the channel with six crabs (the hermit crab pet in six shell colours); everyone picks a crab with a button (and can switch or **Leave** until the start) and **Start the race** (host) or the 75 s timer runs it as an animated GIF of 480x288 under 600 KB that plays once (the lane plates show each crab's number and name, and the finished crabs fan out by place). With a `bet` (10 to 500) each racer puts that many shells on their crab and those who back the winner share the pool; nobody backing it, or fewer than two racers, gives every stake back, so the house never takes a shell. A staked race is pure chance, so it is a gambling-style game: `bet` is refused until a manager runs `/progression economy gambling`, and nobody can be more than 1,000 shells down on a day's staked races (a stake held in a lobby counts as down until it comes back, counted on the day it was put up). Without a bet the backers of the winner get 20 shells (five wins a day). The result is hidden in a spoiler under the GIF |
| `/play heist` | Treasure Heist: a crew of 2 to 8 picks roles (Lookout, Locksmith, Diver) in a lobby; three stages (slip past the guards, crack the lock, haul out the loot) each go better with a fitting role, and the card shows the crew's live chances. All three stages pay each member 60 shells, two pay 25, one pays 8, none is busted. Nobody ever loses shells; at most 150 shells a day and a 5 minute rest after a heist |
| `/play shells bet` | Shell Game: the pearl is under one of three shells, a right guess pays two and a half times the stake (5 to 100), which is a sixth less than fair. A gambling-style game, so it is off until a manager runs `/progression economy gambling`, with 20 games, a 300 shell loss and a 600 shell win limit per player a day; an unanswered table gives the stake back |
| `/surge` | Managers call a Storm Surge: for 10 minutes everyone presses **Stack a sandbag** (one every 6 s, at most 25 each) while the sea washes one away every 30 s; the wall's goal is 24 plus 8 per helper, so one player cannot hold it alone. A shared card shows the wall, the helpers and the best three; hold it and every helper gets 20 shells plus 2 per sandbag (up to 50), lose and they get 6 plus 1 per sandbag; at most 120 shells a day |
| `/play stats [user]` | Games played and won for each game |
| `/progression economy gambling enabled` | Switch gambling-style games (the shell game and staked derbies) on or off for the server; off by default (Manage Server) |
| `/progression economy surge [enabled] [channel] [every]` | Timed storm surges, off by default: one comes to the channel every 6, 12 or 24 hours, 2 days or a week; with no options it shows the setting. `/surge` always works for managers (Manage Server) |
| `/party hotpearl [lives]` | Hot Pearl, 2 to 8 players: a lobby message with **Join**, **Leave**, **Start** (host only) and **Cancel** (closed by the clock after 3 minutes). Each turn the pearl shows a syllable (from 2 and 3 letter fragments, gentle at first, harder later) and its holder presses **Type a word** and types a real word that contains it (176,000 words, plus the usual plurals, `ed`, `ing`, `er`, `est`, `ly` forms; a word can only be played once) before the pearl pops: 25 seconds, shrinking to 15. A pop costs a life (2 each, 1 to 3 with `lives`), a word passes the pearl on with a new syllable, and the last player with lives wins; a click that arrives after the clock pops the pearl itself. A win only pays shells when at least 4 words were played in the game and 2 of them by the winner, so waiting out an empty table earns nothing. The table is a picture with hearts, whose turn it is and the last word |
| `/party acronym [rounds]` | Acronym Tide, 3 to 10 players, 1 to 5 rounds: 3, 4 or 5 letters come up and everyone writes a phrase with one word per letter, in order (75 s, in a form; a rewrite replaces the first). The phrases go up anonymously and numbered, everybody votes with a number button (not for their own; the vote can be changed; 40 s), then the reveal shows who wrote what and the votes (1 point a vote, 2 more for the round's winner, a tie shares it). An early finish (everyone wrote, everyone voted) moves a phase on sooner; fewer than two phrases skip the vote |
| `/party postcard [rounds]` | Caption the Postcard, 3 to 10 players, 1 to 5 rounds: a pixel postcard (one of 11 scenes: a profile backdrop with pets, fish and characters standing on it at the backdrop's own pixel size, stamped and postmarked) comes up, everyone writes a caption (60 s), then the same anonymous vote and reveal as Acronym Tide with the postcard above the captions |
| `/party wordrace` | Wordle Race, 2 to 4 players: one secret word for all, six guesses each typed into a form. A guess comes back to you alone as your private board with the keyboard; the channel sees a progress card with everyone's colours and no letters. The first to solve it wins and the word is shown; seven minutes without a winner ends the race too |
| `/party wouldyourrather [topic]` | A would-you-rather question from 212 (six topics: silly, food, adventure, ocean, brainy, cozy) with a card and an **A** and a **B** button: one vote each, changeable, the split hidden until the poll closes after 10 minutes, when the card shows the percentages. One poll every 20 seconds per channel |
| `/party truthordare [topic]` | Truth or Dare, 2 to 10 players, family friendly (102 truths and 102 dares in six topics): the bot picks whose turn it is, that player presses **Truth** or **Dare**, gets one nobody had lately, and presses **Done** or **Pass** (passing is always fine). The host ends the game with **End game**; a table that lets two turns in a row run out of time ends it by itself. Each player gets one game in `/play stats` when it ends |
| `/party stop` | Ends the lobby or game running in this channel: its host or a member with Manage Server can. The message is closed, nobody wins and nothing is paid or refunded (the party games have no wagers); the answer is private |
| `/progression economy partygames [strict]` | How strict the filter on what members type is (Manage Server). The base filter always blocks strong words and slurs (it reads whole words, undoes disguises like `sh!t`, `f u c k`, `f*ck`, `s1ut` and common misspellings, and lets names like Matsushita through); `strict:true` adds mild words and name-calling. Mentions, links, invites and formatting are always taken out of typed text |
| `/setup server modules-show\|modules-toggle\|modules-channels` | Switch parts of Pinchora off for the whole server (`toggle module enabled`) or per channel (`channels module where [channel] [remove]`: `where` is all channels, only these, or everywhere except these, and the list grows one channel at a time; a category covers the channels and threads under it; up to 50 channels). A switched-off module answers its commands, buttons and forms with a private "This feature is turned off on this server" (or "in this channel"), stops its chat XP, points, critter drops, goodbyes, log lines or timed storms, and `/setup`, `/help`, `/language`, `/ping`, `/privacy` and `/remind` (with their buttons) are never switched off; `/rewards weekly` and `/rewards monthly` belong to the economy module and `/info wrapped` to profiles. The 28 modules are listed with `show`, and `module` suggests them as you type (start with part of a name, or of a game such as `chess`); leveling, points, welcome and gambling keep their switch where it already was (`/progression leveling settings`, `points`, `welcome`, `gambling`). The dashboard's Modules page does the same (Manage Server) |
| `/setup welcome card` | The classic welcome card (theme, headline, accent colour) with a preview: any option makes the classic card the server's welcome image again (a design made in the dashboard's editor is put aside, and the reply says so; how the image is sent stays), and without options it shows the welcome image as it is now, the editor's design included |
| `/setup server language language` | The server's default language (English, Türkçe, or Automatic for English) for messages that answer nobody in particular: welcome cards, level-ups, logs, giveaways. Replies to a member are still in that member's own language (Manage Server) |
| `/setup server prefix-set\|prefix-reset\|prefix-show` | What text commands start with here: 1 to 5 characters without spaces, `@`, `#` or backticks; `reset` goes back to `p!`. `/setup server show` and the dashboard's Overview show it too (Manage Server) |

Duel wagers (`bet`, 10 to 500 shells each) are optional and off by default. Nothing is held until the
challenged player accepts; then both bets leave the balances in one transaction and sit in the database
row of the game. The winner takes both; a draw, a decline or a no-show (a challenge nobody answered) gives
them back, and whoever ends the game first pays out, exactly once, so a restart cannot lose or double shells
(a janitor on every pod ends games whose clock ran out every 10 seconds). In a wagered game a player who runs
out of time *during the game* forfeits the pot to the other, so stalling the clock cannot cancel a bet
(connect four, rock paper scissors, Sea Fleet, Reef Chess and Pearl Checkers; both slow at once, as in rock
paper scissors before either has picked or while placing fleets, gives the bets back), settled only if nobody
has moved since the janitor looked; in the three board duels resigning works the same way, behind a
confirmation. Without a wager the system pays small rewards
(connect four 25, rock paper scissors 15, Sea Fleet 25, Pearl Checkers 20, Reef Chess 30, hangman 15,
wordle 10 and up to 45 for the daily, trivia 8 to 18 by difficulty, picross and 2048 as above) with a daily
limit per player and game. The three board duels pay only for a game that was really played (Reef Chess
10 plies, Pearl Checkers 6, Sea Fleet 10 shots between the captains): a win by an earlier resignation still
counts in `/play stats` but pays nothing. Every game is counted in `/play stats`.

Party games (`/party`) keep everything in Postgres (`party_games`, `party_players`, `party_entries`,
`party_votes`), so a restart loses nothing: a lobby or game is one message that every phase edits, each
phase has a deadline (who has written or voted so far changes only the message's text, at most every 3
seconds, without redrawing the picture), and a ticker on every pod (every 2 seconds) moves the late ones
on. A move made by a click and one made by the clock both go through a compare-and-swap on the game's
sequence number, so only one of them applies and announces it, and the payout rides in the same
transaction as the move that ends the game. Rewards in shells, with a daily limit per player and game: Hot Pearl pays the winner 20 + 4 for
each other player (24 to 48, 6 games a day), Acronym Tide and Caption the Postcard pay everyone who wrote
at least once 4 + 3 per point up to 60 (6 games a day), Wordle Race pays the winner 15 + 4 for each try
under six (15 to 35, 6 a day), Would You Rather pays 1 shell for a first vote (10 a day) and Truth or Dare
4 for a **Done** turn (5 a day). The best any player can make from one game is 60 shells and from one
day about 1,250 across all six games (288 + 360 + 360 + 210 + 10 + 20), far under 1,500 an hour. Games
count in `/play stats` (Would You Rather: a win is being on the bigger side; Truth or Dare: one game per
player when the session ends). There are no wagers in the party games.

Collecting: critters (living things from five habitats) and finds (what the tide leaves) share one
catalogue of 84 entries in five rarities, `content/critters.json` and `content/finds.json`, and one dex per
player. The first of an entry is a **discovery** (15 shells for a common one up to 500 for a legendary);
every later one is a **spare**, recycled at once for 6 to 250 shells and 1 to 30 fragments, and salvage finds
(planks, cleats) leave extra fragments. Draws pick a rarity first (common 60, uncommon 25, rare 10, epic 4,
legendary 1; a better crab pot lifts the rare half) and then an entry of it, so how many entries a tier holds
never moves its odds. Only the first plays of the day pay shells (20 grabs, 10 bottles, 24 walks, 3 hauls);
later ones still fill the dex. A play is counted, filed and paid in one database transaction, so a double
click, two pods or a restart never take a card twice or pay twice; the best sustained source is a level 5 pot,
about 40 shells an hour over its eight-hour soak (a level 1 pot about 19), and a walk or a spare of a common
critter pays 6, far below the 1,500 an hour the games may pay.
A spare pays 16.7 shells and 2.4 fragments on average (the odds against the pay table), and the fragments are
worth as much as the shells beside them, so **one fragment is worth about 7 shells** of catalogue price. The
decor fragments buy is priced at that rate: a tier costs the mean catalogue price of its pieces over 7 (83
shells for a common piece is 12 fragments, 458 is 65, 2,599 is 370, 13,658 is 1,950 and 62,166 is 8,900), about
5, 27, 155, 819 and 3,739 spares, and a unit test fails when the catalogue drifts from the table. Those pieces
are **soulbound**: the inventory row carries a `~bound` mark on the item id (`sofa__mint~bound`, see
`pinchora_core::items`), which placing, storing, undoing and clearing keep, and the three ways a piece can
leave its owner refuse it, in the database as well as in the menus: the stall listing, a gift or plaza
donation, and a partner's house (the partner editor leaves them off its menu).
Time-tied critters and finds (a night jelly, a summer urchin, the moon pearl) only turn up in their time of day
or season wherever they are drawn: beachcombs, drops (at the time of the drop) and crab pots (each catch at
its own hour of the soak).
Chat can make a critter drop or a bottle (a short cosy note and a small prize for whoever opens it first)
wash up: once a server has turned it on, every 25 messages from at least two people, a person's messages
counting once every 20 seconds, at most one every 12 minutes per server (a drop Discord would not take, because the bot cannot post in that channel, gives the minutes back); message text is never read. Drops
and bottles are rows in `collection_drops`, closed by a ticker on every pod when their time is up. Until the
atlas has `cr_<id>` sprites for them, critters and finds borrow a fish, pet or furniture sprite or are drawn
from small pixel shapes (`pinchora_render::critters`).

The party games (`/duel share`, `/play derby`, `/play heist`, `/play shells`, `/surge`) keep the same promise:
every move of shells happens in one transaction with the row that causes it, and a game ends exactly once.
Stakes (derby and share-or-grab wagers, shell game stakes) are held when they are put up and given back on
a decline, a draw, a timeout nobody is to blame for or a race with nobody to pay; a player who abandons a
wagered share-or-grab forfeits to the one who picked. What the system pays out of thin air is capped per
player and day in `party_daily`: 180 shells for share or grab, 150 for the heist, 120 for storm surges (and
the derby's 20-shell reward five times a day), well under what fishing pays an hour. The two games of
chance a player can lose shells at, the shell game and staked derbies, sit behind the per-server gambling
switch and a daily loss limit each (300 and 1,000 shells), kept in `party_daily` on the day the stake was put
up, so a refund after midnight still lands on the right day. The derby, heist and
storm all run from a seed or a clock kept in the database, so any pod can finish them after a restart. One
derby or heist lobby can be open per channel and one storm per server.

Game content (furniture, rooms, pets, pet trips, fish, fishing gear, critters, finds, bottle messages, looks, accessories, character presets, card backgrounds,
badges, titles, trivia, hangman words and the wordle lists) lives in
`content/*.json` (`python3 content/check.py` validates it); see
[`docs/GAME_DESIGN.md`](docs/GAME_DESIGN.md). Sprites are drawn by GPT and imported with
`tools/art/` (see below).

Content names speak more than English. `content/i18n/tr/` holds the Turkish text of every content id (650 furniture
designs, wallpapers, floors, sets, rooms, pets, fish, looks, accessories, presets, backgrounds, badges, titles, contest
themes, critters, finds, pet trips with their stories, fishing spots, rods and baits) as `id → {name, description?}`
files, plus `terms.json` (rarities, categories, slots, habitats, stats, house tiers, daily quests), `colors.json` and
`_glossary.json` (one Turkish word per English term). Colour variants are not stored: `sofa__mint` is the translated
base name plus the translated colour word, as in English. Ask `Content::name(id, lang)` and
`Content::description(id, lang)` (or `Item::name_in`, `Rarity::label_in`, `Category::label_in`, `Quest::text_in`,
`Tier::name_in`, `Trip::stories_in`): a missing text or language falls back to English. `python3 content/check.py`
and `cargo test -p pinchora-game overlay` hold the overlay to the English content: every id translated, length limits,
no duplicate names, the glossary, the furniture noun table (`_furniture_nouns.json`). To add a language, copy
`content/i18n/tr/`, translate it, add the files to `TR_FILES`-style tables in `crates/pinchora-game/src/overlay.rs` and
an arm to `overlay()`.

**Turkish social and server tools.** `/partner`, `/friends`, `/hug` `/pat` `/wave` `/highfive`, `/rep`, `/customize bio`, `/ping`,
the house guestbook and room contest, `/giveaway`, `/suggest`, `/poll`, `/setup roles`, `/tag` and `/setup commands`, every
`/setup` subcommand's replies (`show`, `xp`, `level-roles`, `rep-roles`, `modules`, `join-role`, `goodbye`, `log`,
`suggestions`, `welcome-card`) and the messages a server sends on its own (welcome card, goodbye, level-up banner, server
log, giveaway, suggestion and poll messages) speak Turkish: the catalogs are `locales/<code>/social.toml`, `guestbook.toml`
(+ contest), `giveaway.toml`, `suggest.toml` (+ poll), `roles.toml`, `tags.toml`, `modules.toml`, `serverconfig.toml`
and `settings.toml`. Replies use the member's language; what answers nobody in particular (the giveaway, suggestion, poll and
role-panel messages, welcome, goodbye, level-ups in a channel, the server log) uses the server's default (`/setup
language`), and a direct message (a level-up by DM, a suggestion's verdict) uses its recipient's own pick. The checks that
refuse a setting (`pinchora_core::settings::FieldError`) carry a catalog key beside their English text: ask
`error.text(lang)`; role refusals have `*_in(lang, …)` twins (`role_refusal_in`, `grant_refusal_in`, …) and the pixel cards
(`PollCard`, `LevelUpCard`) take a `lang`. The classic welcome and goodbye card (`templates::classic`, what a server that
never opened the card editor sends) draws its default headline (`HOŞ GELDİN`, Turkish capitals by `Lang::upper`) and its
line in the server's language; a design saved in the editor keeps the words it was given. A server's own
welcome/goodbye/level-up text and custom commands stay as the server wrote them; only the defaults (`events.default-*`)
are translated.

**Turkish games.** The word and quiz games speak Turkish to a player whose Discord locale is Turkish (the games
ask `cx.lang()`, so a `/language` choice and the server default count too). Their content sits beside
the English files: `words_tr.json` (wordle answers and guesses), `hangman_tr.json`, `trivia_tr.json` (ids
`<category>_tr_NNN`), `party_tr.json` (would you rather, truths, dares), `pearl_words_tr.txt` (Hot Pearl, with
the Turkish endings of `party/hotpearl_tr.rs` so "kitaplarımızdan" counts for "kitap") and `bottles_tr.json`.
Wordle, Wordle Race, hangman, Hot Pearl and Acronym Tide use the Turkish alphabet (`pinchora_game::lang`: dotted
`i`/`İ` and dotless `ı`/`I` are kept apart; Acronym Tide draws no Ğ and no dotless I); a running game keeps its
language (hangman's state ends in `|tr`, the party games' JSON state holds `lang`, a wordle board is stored as
mode `daily-tr`/`practice-tr`), so no migration is needed. The party filter reads Turkish with its own list and
disguises (`party/filter_tr.rs`: `ş`/`s`, `ğ`/`g` swaps, numbers, spacing, stars), and never reads the Turkish
`gün` as the English `gun`. `tests/play_content_tr.rs` and `content/check.py` check every list.

**Turkish game chrome.** Everything a game says besides its content (replies, embeds, buttons, menus, forms, refusals and
the words on its pixel cards) is in the catalogs `locales/{en,tr}/`: `games.toml` (the games' names, `Game::name(lang)`,
also what `/play stats` lists), `play.toml` (`/play stats`), `duel.toml` (every `/duel` game and the board helpers),
`solo.toml` (wordle, hangman, trivia, Pixel Picross, Tide 2048), `collect.toml` (critter drops, bottles, `/dex`,
`/beachcomb`, `/crabpot`), `party.toml` (`/party`) and `arcade.toml` (the crab derby, treasure heist, shell game and storm
surge, including the derby GIF's own 3x5 font, which folds Ç Ğ İ Ö Ş Ü to plain letters). A public message in a channel
(a challenge, a duel or board game's board, a party, derby, heist or storm lobby, the shell game's table, a critter drop or
bottle) is drawn in the server's language (`/setup server language`, English when unset) for its whole life, whoever clicks it:
the duel and board-game boards ask `App::guild_lang`, and a lobby locks the server's language when it opens (the derby,
heist, storm and shell game states end in `|<code>`, the party games' JSON state holds `lang`) so the janitor does not
switch it. What only the clicker sees (a refusal, a menu, a private picker, "not your turn") is in the clicker's own
language; what the bot edits in by itself (a time-out, a secret pick) is in the server's. Pixel-font labels are kept in the
catalog already in capitals, and text a card capitalises is capitalised with `Lang::upper` first (Turkish
`i` is `İ`). The Turkish simulator flows are `tests/simulate/turkish_{duel,solo,party,arcade}.rs`.
**Turkish pets, fishing, shops and cards.** `/pet` (with trips and the nursery), `/fish`, `/shop`, `/inventory`, `/gift`,
`/stall`, `/plaza`, `/quests`, `/points`, `/store`, `/leaderboard`, `/info wrapped`, `/remind` and the `/customize` title, style
and showcase editors answer in the player's language: their texts are the keys of `locales/{en,tr}/{pets,fish,shop,
quests,points,remind,boards,cosmetics}.toml`, and the pet, nursery, wrapped, leaderboard and badge-collection cards take
a `lang` for the words they draw. A new pet is named after its species in the adopter's language. `/remind` also
understands Turkish units (`2sa30dk`, `3g`, `1hf`). The direct messages a reminder sends have no interaction to take a
language from: they use the player's `/language` pick, else the language the notification was switched on or last
refreshed in (kept in the reminder's `body`), else English. `tests/simulate/turkish_{pets,fish,shop,remind}.rs` play
each group in Turkish.

### Text commands

Every command also works as a message: `p!profile`, `p!fish cast reef`, `p!duel connect4 @Sam bet:50`, or the
same after the bot's mention (`@Pinchora fish cast reef`), which always works whatever the prefix. The message
is read against the registered slash commands and runs the very same handler, so the module switches, the
Manage Server checks (from the member's roles), cooldowns and wagers are those of the slash command.

Servers control them like any other part of the bot. The **Text commands** module (`/setup server`, the
dashboard's Modules page) switches every text command off for the server or in some channels, the `@Pinchora`
form included: where it is off, a command typed with the prefix is ignored without a word and one typed after
the mention is answered with "This feature is turned off". On top of that a text command is allowed only where
the slash command would be: the member needs **Use Application Commands** in the channel (channel
permission overwrites and a thread's parent are applied), and the server's **Server Settings, Integrations**
overrides for the bot's commands (a user, role or channel switched off for a command, or for all of them)
are honoured; administrators and the owner are never held back by those. Commands that ask for default
permissions (`default_member_permissions`, where `"0"` means administrators only) are checked the same way.

- Command and subcommand names are any case; `bal`, `lb` (or `top`), `h` and `inv` stand for `balance`,
  `leaderboard`, `help` and `inventory`.
- Options go in their order, or as `name:value` in any order. Words with spaces go in quotes
  (`text:"two words"`); the last free-text option takes the rest of the line. Members, roles and channels are
  mentions or raw ids; a choice is its value or its name (`kelp`, `"Kelp Forest"`); `yes`/`no` for switches.
- A wrong call answers with the problem and a usage line from the schema, e.g.
  ``Usage: `p!duel connect4 <user> [bet]` ``. Words that name no command (another bot's) get no answer.
- Answers are replies to the message that ping nobody (a challenge still pings the member it names). A
  private answer (a refusal, a short notice) is posted the same way and deleted after 15 seconds, or at
  once when the bot shuts down. Buttons and menus on a reply work as on the slash command's message and
  check the member who typed the command.
- A message carries no Discord language, so a text command answers in the member's `/language` pick, else the
  server's default (`/setup server language`), else English. The notices, usage lines and `/setup server` are in the
  `text` catalogs (`crates/pinchora-core/locales/<code>/text.toml`); a switch also takes `evet`/`hayır`.
- Private panels, forms and server settings stay slash-only: `/setup` (which would show the server's
  settings to the whole channel), `/house edit`, `/house decorate`, `/customize`, `/character`,
  `/community server plaza-edit`, `/stall manage`, `/play picross`, `/play wordle` and `/play 2048` (and any other command
  whose first answer is a private panel) answer "This one only works as a slash command: use `/…`".
- Bots, webhooks and direct messages are ignored, and a member's text commands count at most once every
  1.5 seconds. If the rate limit cannot be read (Redis is down) text commands are ignored rather than let
  through unlimited. Whole numbers are brought within ±2^53, the most a slash option can carry.

The bot owner turns this on in two places: **Message Content Intent** on the Bot page of the Discord Developer
Portal, and `PINCHORA_GATEWAY__MESSAGE_CONTENT=true`. Without them Discord only sends the text of messages that
mention the bot, so text commands work as `@Pinchora profile` and `/help` says so.

### Consolidated command tree (slices 1–18)

The original 95 roots now occupy **62 registered roots** including `/setup`, `/mod`, and `/rewards`.
Every one of the 330 actions is reachable, including the 9 quick-game actions. The 160 moved actions call their original
typed handlers. `/setup` adds Manage Server authorization on every action; `/mod` has no restrictive
root default and preserves each action's existing authorization and feature module.
Member panel buttons keep their custom IDs and authorization. Old prefix paths use longest-token
matching, after one server alias expansion, and show a rate-limited English/Turkish migration notice.
Private configuration redirects to the new slash path. Existing colliding aliases retain their
meaning; existing custom content remains available through `/tag`. New collisions are rejected.

| New command group | Actions (typed arguments are unchanged) |
| --- | --- |
| `/progression achievements` | `add`, `edit`, `grant`, `list`, `remove` |
| `/safety antinuke` | `disable`, `enable`, `events`, `limits`, `status`, `trust-add`, `trust-remove`, `vip-add`, `vip-remove` |
| `/safety automod` | `exempt-channel`, `exempt-role`, `profile`, `show`, `test`, `toggle` |
| `/setup backup` | `export`, `import`, `list`, `restore` |
| `/community birthdays` | `preview`, `settings` |
| `/setup commands` | `alias`, `reset`, `set`, `show`, `custom-create`, `custom-delete`, `custom-edit`, `custom-info`, `custom-list` |
| `/community server` | `suggestions`, `plaza-edit` |
| `/progression economy` | `critters`, `gambling`, `partygames`, `points-settings`, `rep-roles-add`, `rep-roles-list`, `rep-roles-remove`, `store-add`, `store-list`, `store-remove`, `surge`, `points-give`, `points-take` |
| `/community events` | `template` |
| `/integrations feeds` | `add`, `edit`, `list`, `remove`, `test` |
| `/integrations invites` | `bonus-add`, `bonus-remove`, `post`, `reward-add`, `reward-remove`, `settings` |
| `/progression leveling` | `roles-add`, `roles-list`, `roles-mode`, `roles-remove`, `roles-sync`, `settings`, `xp-clear`, `xp-multiplier`, `xp-show`, `xp-voice`, `xp-add`, `xp-import-mee6`, `xp-remove`, `xp-reset`, `xp-reset-all`, `xp-set` |
| `/safety moderation` | `log`, `log-events`, `log-ignore`, `settings`, `warn-ladder` |
| `/setup roles` | `auto-add`, `auto-list`, `auto-remove`, `auto-sticky`, `color-setup`, `join-role`, `panel-add`, `panel-create`, `panel-post`, `panel-remove`, `panel-temp` |
| `/setup server` | `language`, `modules-channels`, `modules-show`, `modules-toggle`, `prefix-reset`, `prefix-set`, `prefix-show`, `show` |
| `/community starboard` | `edit`, `list`, `remove`, `setup`, `top`, `votes-add`, `votes-list`, `votes-remove` |
| `/integrations stats` | `counter-add`, `counter-list`, `counter-remove` |
| `/community tickets` | `panel-create`, `panel-delete`, `panel-edit`, `panel-list`, `panel-post`, `settings` |
| `/community utility` | `schedule-add`, `schedule-list`, `schedule-pause`, `schedule-preview`, `schedule-remove`, `schedule-resume`, `schedule-run-now`, `sticky-remove`, `sticky-set`, `sticky-show`, `voice-add`, `voice-list`, `voice-remove` |
| `/safety verification` | `message`, `mode`, `post`, `setup` |
| `/setup welcome` | `goodbye`, `message`, `card` |

`/mod` contains `unban`, `untimeout`, `warnings`, `unwarn`, `slowmode`, `lock`, `unlock`, `nick`,
`case`, `reason`, `role add|remove`, `raid lockdown|unlock|status`, and `suggestion approve|deny`.
`/rewards weekly|monthly`, `/customize bio`, and `/info wrapped` retain their original arguments
and module ownership. The player leaves under points, voice, color, plaza and event remain available.

The route manifest in `crates/pinchora-commands/src/routing/manifest.json` records every original
leaf, final path and rollout slice. Registration validates the 100-root budget, depth and 25-option
limits. Migration `0066` expands old parent rules onto their exact replacement actions and preserves
independent role/channel restrictions. Recovery protection is path-aware. Help lists canonical paths
and maps old `hp:<page-index>` buttons to the same action. Registration snapshots Discord overrides
before replacing command IDs and invalidates caches afterward; see the [release rehearsal and admin
checklist](docs/releases/command-tree.md). Old slash definitions remain internal only.

### Command control and configuration backup (D2)

| Command (Manage Server) | What it does |
| --- | --- |
| `/setup commands show` | Show override and alias counts. |
| `/setup commands set command [enabled] [cooldown] [delete_reply] [delete_invocation] [ephemeral] [allow_role] [deny_role] [allow_channel] [deny_channel]` | Add restrictions to a command or full subcommand path. Parent restrictions and the command's original permission/cooldown checks always apply. Channel lists include categories and threads. |
| `/setup commands reset command` | Remove an override. |
| `/setup commands alias name [command]` | Set a prefix shortcut (up to 50); omit the command to remove it. For example `w` → `play wordle`. |
| `/setup backup export`, `/setup backup list`, `/setup backup restore id`, `/setup backup import` | Download configuration, list the last ten restore points, preview a restore before confirming, or open the import dashboard. |

The **Commands** dashboard searches the full registered command tree, edits access and reply behavior in a drawer,
and shows Discord integration permission rows when available. Discord's integration permissions and all server-side
Manage Server checks remain authoritative. Autocomplete applies the same access restrictions without consuming a
command cooldown. Command policies use the guild cache and invalidation bus.
The `command_control` Server module and recovery commands always remain available.
Aliases retain the target command's prefix support; private-panel commands such as Wordle still direct members to slash.
Reply deletion supports 1–840 seconds; prefix invocation deletion supports up to 24 hours.

**Backup** exports version 1 JSON from an explicit configuration allowlist, including modules, command rules, aliases,
welcome designs and uploaded backgrounds, moderation, AutoMod, role panels, tickets, feeds, starboards, counters,
invites, verification/raid, leveling, store, temporary voice, custom commands and responders. Member data, activity,
transcripts, author ids, user exemption lists and runtime state are excluded. Credential query parameters in feed URLs
are redacted and those exported subscriptions are disabled; re-enter their credentials after restoring. Merge overwrites matching natural keys;
replace also removes absent entries. Preview does not write. Apply validates again and atomically saves a restore point
before importing, retaining ten points per server. Cross-server imports remap role/channel ids by unique names and report
missing or ambiguous matches; resolve those before applying. Posted panel messages are not copied to another server.
Responders may share names: same-server imports match their ids; cross-server merges append them as separate records.
Files support up to 32 MiB including card uploads. Exports over Discord's attachment limit use the dashboard download.
Backup operations share per-server budgets across Discord and web: ten previews/exports and five applies, reset after
60 seconds without another attempt of that operation. Imports check current Discord ownership, role permissions and
channel types; AutoMod role actions and escalation grants use the shared role-safety rules. Noncanonical custom command
and responder values must be corrected before import. Switching dashboard servers clears drafts and backup previews.
New configuration tables: `command_control`, `config_restore_points` (migration `0064`). Recovery buttons use `cf:`;
`kc:` handles command-control interactions.

Command-control backups include utility timezones, sticky messages, scheduled-message definitions
and event settings. AFK access settings travel with module and command policies; member AFK status,
event attendance and delivery history are not configuration and are excluded. Restored schedules
must be edited by a manager before they can send broad mentions again.

Reef consent, sanctions, chat evidence and recipients, reports/appeals, and moderator audit records
are player data and are excluded from configuration exports, restore points and imports. Reef chat
moderation stays at `/reef/moderation` (linked from the room chat); it is house/operator scoped, not a
server command-control setting. The server sidebar retains Commands and Backup under Server.

### AutoMod

| Command | What it does |
|---|---|
| `/safety automod show` | Current profile, active rules and observation deadline |
| `/safety automod profile name` | Apply `relaxed`, `standard`, `strict` or `off`; standard only logs for its first 48 hours |
| `/safety automod toggle rule enabled` | Enable or pause one named rule |
| `/safety automod exempt-role action role [rule]` | `add` or `remove` a role exemption, for one rule or all rules |
| `/safety automod exempt-channel action channel [rule]` | `add` or `remove` a channel exemption |
| `/safety automod test text` | Private matcher preview; no strikes or actions |

The **AutoMod** dashboard under Safety has profile cards, up to 30 rule cards, ordered actions,
role/channel exemptions, channel scopes, a message test, the last 50 hits and per-member strike reset.
Rules cover message floods, duplicates, repetition, blocked words (de-disguise and bounded regex),
invites, domains, capitals, emoji, mentions, attachments, newlines, zalgo, media-only, YouTube-only,
honeypot channels and Discord word presets. Actions run in configured order: delete, warn (a moderation
case and the existing warn ladder), timeout, kick, ban, log, DM and safe role assignment. Escalation
thresholds share a configurable strike window. Bots, webhooks, the owner, staff with moderation powers,
and members at or above the bot are excluded. Add-role actions refuse dangerous roles at save and execution.

Standard's first 48 hours and each rule's **Log only** switch record hits without warnings, strikes or
punishments. Content-dependent local rules are locked when Message Content is off. Discord-native
mirrors detect spam, mentions and presets without that flag; keyword, invitation and link rules
stay local because Discord's keyword and regex matching cannot exactly mirror Pinchora's matcher;
mirrors only send alerts, leaving ordered actions and safety checks to Pinchora. Native sync is debounced
five seconds, reconciles drift, and respects other applications' rules and Discord's guild limits.
Native mirrors require a moderation log/alert channel; channel-only rules
stay local. Native spam uses Discord's detector; numeric flood thresholds also run locally.

Alerts use `am:del|to|ban|ok:<hit>` and `ax:r|c:<rule>:<hit>` with Manage Server checked on every click.
Migration **0045** adds `automod_settings`, `automod_rules`, `automod_strikes`, `automod_hits` and durable alert-action claims.
Hit history and inactive strikes expire after 30 days; player deletion clears hits, strikes and rule authors.
English and Turkish text comes from `locales/{en,tr}/automod.toml`.

### Roles pack

Auto roles, role panels in three styles, roles that expire, roles kept for members who come back, and colour
roles. The commands are in the table above (`/setup roles`, `/setup roles`, `/color`); the dashboard's **Roles** page
(sidebar group Roles) does the same with a live preview of every panel.

- **Auto roles** (module `autoroles`) are given by the join hook, after what sticky roles bring back, and respect a
  member held at the join gate (the gate gives the `after_verify` ones with `give_after_verify` when it lets them
  in). A role with a delay waits in `delayed_roles`, a role that lasts waits in `timed_roles`; one ticker (every 30
  seconds, on every pod, claimed with leases) gives and takes them, and also serves a panel's expiring roles. A
  change Discord refuses for good (the member left, the role is gone, the bot may not) is dropped; one with no
  answer is tried again. Members who leave lose the roles still waiting for them.
- **Sticky roles** need to know what a member held when they left, which Discord's leave event does not say. The
  bot notes a member's roles from each member update while the switch is on (this is why `hooks.rs` has a
  `on_member_update` entry next to the six), reads the current members once when it is switched on (up to 10,000,
  one run per server every 10 minutes) and forgets members who left more than 60 days ago.
- **Role panels** (module `role_panels`, now under the Roles group): *buttons* (`rr:`/`rb:`: a panel with no
  rules keeps `rr:` exactly as before, one with rules sends `rb:`; both buttons follow the panel's rules, so a
  message posted before the rules still obeys), a *menu* (`rs:`: what the member picks is what they hold of the panel,
  limited by the fewest and most; picking nothing takes them all off when the fewest is 0) and *reactions* (through
  `hooks::on_reaction`: a reaction gives the role, taking it back takes it, a refused one is taken off the message).
  A required role closes a panel to everyone else. Reactions panels need an emoji on every role, none repeated, and
  the bot needs Add Reactions and Read Message History in the channel. The dashboard posts through the same code.
- **Colour roles** (module `color_roles`, prefix `cr:`): the palette is `ACCENT_PRESETS` plus the outfit dyes of the
  character editor, only the ones readable on Discord's dark chat; roles are made on demand (name, colour, no
  permissions) right under the anchor role when the server set one. One role per colour, at most 32 a server, a
  member holds one. The bot needs Manage Roles with its role above the anchor.
- The bot needs **Manage Roles** (its role above every role it gives), and Add Reactions for reaction panels. The
  intents it listens to are the ones already on (Server Members for joins and sticky roles, reactions).

### Tickets

Private support tickets, opened from a panel and run by the server's staff (module **Tickets**, in the
dashboard's Community group).

| Command | What it does |
|---|---|
| `/community tickets panel-create name channel [title] [description] [topics] [style] [support_role]` | Make a panel (up to 10): a title, a text, and its topics as buttons (up to 5) or a menu (up to 25), typed as `Support, Billing, Report a member`. The support role sees and answers its tickets (Manage Server) |
| `/community tickets panel-edit panel [name] [title] [description] [topics] [style] [support_role] [color] [welcome] [channel]` | Change a panel; `welcome` is the ticket's first message, with `{user}` `{server}` `{number}` `{topic}`. Post it again to update it (Manage Server) |
| `/community tickets panel-post\|panel-delete\|panel-list panel` | Post a panel (posting again edits it where it is), delete one (its tickets stay), or list them with how many tickets are open (Manage Server) |
| `/community tickets settings [category] [closed_category] [log_channel] [auto_close_hours] [dm_transcript] [ratings] [mode] [naming] [max_open] [clear]` | Where ticket channels go and where closed ones move, the log channel, closing tickets nobody wrote in for 1 to 720 hours, the transcript by direct message, 1 to 5 star ratings, channels or private threads (automatic: threads once the server has more than 400 channels), the channel name (`ticket-{number}`, also `{user}` `{topic}`) and how many tickets one member may have open (1 to 10). No options shows them (Manage Server) |
| `/ticket close [reason]`, `/ticket transcript` | In a ticket: close it, or get its messages as a web page only you see. The member who opened it may do both |
| `/ticket claim\|unclaim\|add user\|remove user\|rename name\|reopen\|move category` | In a ticket, for staff (a support role, Manage Channels or Manage Server): say you handle it or hand it back, let someone in or take them out, rename the channel, reopen a closed ticket, move the channel to another category |

- **Opening**: a member presses a topic (or picks it from the menu); a panel with a form asks up to five questions
  first and the answers open the ticket. The bot makes `ticket-0042` in the ticket category, seen only by the
  member, the panel's support roles and the bot, or a private thread off the panel's channel, says hello with
  the member's answers and pings only the member and the support roles. One ticket a minute per member, and at
  most the server's number open at once; two members opening at the same moment get different numbers.
- **Inside**: Close (asks first), Claim, Transcript and Lock (only staff can write) buttons; the ticket says who
  claimed, locked, added or removed whom. A bot without a permission it needs answers with the exact one
  (Manage Channels and Manage Roles for channels, Create Private Threads for threads), and nothing is left half made.
- **Closing** happens once however many pods see it: the bot reads the ticket's history (up to 2,000 messages)
  into a transcript, the member loses access (a thread is archived and locked), the channel moves to the closed
  category, the log channel gets the close with `ticket-0042.html`, and the member gets the page by direct
  message with five star buttons when ratings are on (not if they switched server DMs off in `/privacy`). A
  closed ticket's channel has Reopen, Transcript and Delete buttons. Without the Message Content intent the
  transcript keeps who wrote, when, and the file names, and says why the words are missing.
- **Auto-close**: every five minutes the bot asks Discord for the last message of tickets idle longer than
  the server allows, and closes the ones that really are (a switched-off module pauses it). If a ticket
  channel or thread is deleted directly in Discord, the bot closes its ticket record when it receives the
  delete event or the member next tries to open a ticket.
- **Transcripts** are self-contained pages (no scripts, nothing loaded from elsewhere) with every word members
  wrote escaped. HTML downloads stay below 7 MiB and note how many older messages were left out. "Delete my data"
  removes the tickets a member opened and turns their name on other tickets'
  transcripts into "Deleted user"; staff who claimed or closed tickets are cleared from them.
- **Dashboard** (Tickets page): settings, panels with tabs (Buttons, Behaviour, Form, Log), a live preview and a
  **Post** button, the tickets table (open, claimed, closed; by staff, member or words) with each ticket's
  transcript, a Close button in the list and detail, and the last 7 and 30 days: opened, closed, median first reply and ratings.

### Temp voice

A voice channel a server sets up as a **creator** gives whoever joins it a voice room of their own: the bot makes
`Mira's room` in the creator's category, moves the member in, and deletes the room when the last person leaves.
Module `temp_voice` (Community group, switched on or off for the whole server). Needs **Manage Channels**,
**Move Members** and **Manage Roles** (it writes the channel's permission overwrites); when one is missing,
`/community utility voice-add`, the dashboard and the creator's own chat name it.

| Command | What it does |
|---|---|
| `/community utility voice-add channel [category] [limit] [template]` | Make a voice channel a creator, or change one: `category` is where rooms go (default: the creator's own), `limit` how many a room takes at first (0: no limit), `template` what rooms are called (`{user}` is the owner's name, `{n}` the room's number; default `{user}'s room`). Up to 10 per server (Manage Server) |
| `/community utility voice-remove channel`, `/community utility voice-list` | Stop a channel making rooms (its rooms stay until they empty), or list the creators with their rooms alive (Manage Server) |
| `/voice lock`, `/voice unlock` | Keep new people out of your room (@everyone loses Connect; the owner and the bot keep their seats), or open it again. A room goes back to what its category says, so a private category stays private |
| `/voice hide`, `/voice show` | Hide the room from people who are not in it (View Channel), or show it again |
| `/voice limit people` | How many people the room takes, 0 to 99 (0: no limit) |
| `/voice rename name` | Rename the room; at most once every 5 minutes (Discord allows two renames in ten) |
| `/voice kick user` | Send someone out and keep them out: they are disconnected, put on the room's kick list and denied Connect, and the bot sends them out again if they get back in |
| `/voice invite user` | Let someone in even when the room is locked, and lift a kick |
| `/voice transfer user` | Give the room to someone who is in it (never a bot) |
| `/voice claim` | Take a room whose owner is not in it any more; the first member to claim gets it. A room whose owner deleted their data has no owner and can be claimed too |
| `/voice panel` | The buttons that run the room (private). With a creator's **panel** on, each new room also gets them as a message in its own chat (or in a private thread, below): **Lock**, **Unlock**, **Limit** and **Rename** (forms), **Kick** and **Transfer** (a member picker), **Hide / show**, **Claim** |

Only the owner (or a member with Manage Server) runs a room; `/voice` works from anywhere and finds the room you are
in, else the one you own, and a panel finds the room it sits in. Answers are private and ping nobody.

- **How it works.** The voice events tell the bot where a member is now; it keeps where they were last seen in Redis
  (`tvoice:at:{guild}:{user}`, the members of each room in `tvoice:in:{channel}`), but only for servers that have a
  creator or a room, so other servers cost one cached lookup per half minute. Leaving a room's last seat deletes the
  room; a **sweep every 30 seconds** deletes rooms seen empty twice in a row (a leave the events missed), and a server's
  `GUILD_CREATE` after a restart puts each room's members right, deletes rooms nobody is in, and forgets rooms whose
  channel is gone. Voice XP is untouched: members of a temp room earn it like anyone else.
- **Limits.** 10 creators and 60 rooms per server, 1 to 5 rooms per member per creator (a member over it is taken back
  to their oldest room), one room request per member every 4 seconds. A full category (Discord allows 50 channels in
  one) or a server at 500 channels is told so in the creator's chat.
- **Paired thread.** A creator can name a text channel (the dashboard's *Private chat thread*): each room then gets a
  private thread there with the panel in it; members are added as they join the room and taken out as they leave, and
  the thread goes with the room.
- **Data** (migration 0048): `voice_creators`, `temp_voice_channels` (owner, lock, hide, limit, last rename, panel
  message, thread) and `temp_voice_bans`. "Delete my data" removes a member's kicks and leaves their rooms without an
  owner. Custom ids `vc:` (buttons, forms) and `vo:` (the kick and transfer pickers).
- **Dashboard.** *Temp voice* under COMMUNITY: creators with their room name (live preview), category, people per
  room, rooms per member, starts locked, button panel, active and the thread channel, each saved on its own and
  written to the audit log; the live list of rooms (owner, people inside, locked/hidden, age; refreshes every 15
  seconds, read-only); a warning when the module is off or the bot lacks a permission.

### Starboard and vote channels

`/community starboard setup|edit|remove|list|top` manages up to three boards per server. Each board has an emoji,
threshold (1–50), self-star and bot rules, source channel allow/deny lists, image requirement, colour,
role weights and an option to remove posts that fall below the threshold. A reaction is counted once per
member and board after a short debounce against Discord's current reactions and membership. A qualifying
post is claimed in Postgres before sending; stale senders clean up any losing copy. Removing stars
updates the count, and deleted or edited sources are reconciled with the board copy. NSFW sources only
copy to NSFW boards. Private sources are excluded even when selected in a source allow list.

`/community starboard votes-add|votes-remove|votes-list` sets channels where each new message gets the up reaction followed
by the down reaction; the server may choose emojis, restrict to media posts and open discussion threads.
Discussion threads have per-member and per-channel cooldowns and a bounded worker pool.
The **Starboard** dashboard page edits boards and vote channels and previews the board card. With the
Message Content intent off, board copies show the author and source link but omit message
text and attachments; the dashboard shows why. The module key is `starboard` (Community). Old star records are swept
after 180 days. The bot needs View Channel, Read Message History, Add Reactions and Send Messages in
the relevant channels; thread creation additionally needs Create Public Threads.

### Invite tracker

`/invites user [user]` draws a pixel card with regular invitations, members who left, accounts younger than the configured fake window, bonus, and the counted total. `/invites top [all|week|month]` shows the leaderboard with `in:` page buttons. `/invites temp` gives a private expiring link when the server enables friend links; the same action is available from an “Invite a friend” button. A member can request only the configured number of links per UTC day. The bot needs **Create Instant Invite** in the selected channel.

Managers use `/integrations invites settings` to enable tracking, choose the log and invite channels, set the fake age window and daily quota; `/integrations invites bonus-add|bonus-remove`, `/integrations invites reward-add|reward-remove`, and `/integrations invites post` manage bonuses, role milestones and the friend button. A reward role is checked against the bot hierarchy and dangerous permissions when configured and again before grant. The **Invites** dashboard page edits these settings, shows reward roles, leaderboards and member details, and exports counts as CSV.

Attribution compares Discord invite uses on each join, with a Redis snapshot per server and a database lock to prevent pods crediting one use twice. **Manage Server** is required to list server invites; if the bot lacks it, the join remains unknown. A used-up temporary invite may disappear before it can be matched, so it is labeled temporary/unknown rather than credited by guess. Vanity and widget joins are not credited to a person. Rejoining restores the original member record and does not earn a second invite. No invite channel is shown to ordinary members.

### Birthdays

Members opt in with `/birthday set <day> <month> [year] [timezone]`. The IANA time zone is suggested as they type and defaults to the server zone. The optional year stays private. A confirmation button saves the date; `public` and `show_age` are separate choices, both off by default. `/birthday me`, `/birthday view <member>`, `/birthday next`, `/birthday list [month]` and `/birthday remove` read or remove it. Only shared dates from public profiles appear in other members' views and lists. Date and time zone changes are locked for 30 days after setting, including after removal; privacy choices remain editable. `/privacy delete` removes the active birthday role and clears birthday rows and yearly claims. A minimal cooldown expiry in the existing deletion tombstone prevents re-setting a claimed birthday until the next UTC year (and preserves any longer original 30-day date lock); no birthday date, birth year, time zone or guild is retained there.

Managers use `/community birthdays settings` for the announcement channel, message (`{user}`, `{age}`), 24-hour role, IANA server zone, optional Cake Day collectible badge and card ribbon, and a system gift of 0–500 shells. `/community birthdays preview` renders the pixel cake card privately. The Community dashboard has the same settings, a preview, and the next 30 days of shared dates. The `birthdays` module can switch the feature off.

The minute ticker starts at each member's local midnight and catches up throughout that local day. February 29 is celebrated on February 28 in a non-leap year. Claims are unique per guild/member/local year and UTC year; system gifts are capped to one per player globally per UTC year, even across servers. Current Discord membership is checked before awarding a gift. The 24-hour removal timer is committed with the claim, and grants re-check live role safety. Pending announcements survive restarts and are grouped into at most 25 members per message with restricted mentions. Retries use a stable Discord nonce; Discord only deduplicates recent nonces, so a crash after sending but before recording success can still duplicate an announcement after a long outage. A member must have a `members` row (the bot must have seen their activity) before the server can celebrate them.

### Achievements

Servers can configure up to 25 achievements. Members see earned and locked milestones on a paged pixel card, with a progress bar for each visible rule. Secret achievements appear only after they are earned. Accepted chat, voice XP ticks and level-ups share a once-per-member-per-minute evaluation limit (at most 100 members per server per minute and 32 concurrent evaluations per process). Rules are cached for 60 seconds and edits invalidate every subscribed pod. A milestone can give server points, a safe role, and an announcement. It never mints shells.

| Command | What it does |
|---|---|
| `/achievements [user]` | Show the member's server achievement card. Private profiles remain private. |
| `/progression achievements add name kind target [description] [icon] [role] [points] [announce_channel] [secret]` | Add a milestone (Manage Server). |
| `/progression achievements edit id […]`, `/progression achievements remove id`, `/progression achievements list` | Change, remove, or list milestones (Manage Server). |
| `/progression achievements grant id user` | Grant a custom achievement once (Manage Server). |

Kinds: messages, voice minutes, level, active days, invites, stars received, tickets closed, reputation, games won, fish caught, and custom. A kind whose source module is off is hidden and does not grant. The dashboard's **Achievements** page edits cards, picks icons from the existing glyphs and badge atlas, previews a card, and lists members who earned one. Grants are unique in `member_achievements`; point rewards and the grant commit together under the player lock. `server_achievements` holds the server's rules. Announcement batches post one summary per channel per minute, including when existing progress earns a new rule. Invites exclude self, fake and under-seven-day accounts and count each eligible live invitee once. Stars count distinct external reactions from boards that disallow bots, excluding accounts under seven days old and role weight multipliers; closing one's own ticket earns no progress.

### Stats

Pinchora counts what happens in a server without reading any message text: messages per channel and per member,
minutes in voice (where voice XP follows who is in voice), joins, leaves and commands. A message costs one Redis round
trip; once a minute one pod seals the counts and writes them to Postgres (`guild_activity_hourly`,
`channel_activity_daily`, `member_activity_daily`), and a write cut short is finished by the next one without counting
anything twice (`stats_flushes`). The numbers are kept 90 days; an hourly sweep forgets older ones. A thread counts in
its channel (once the bot has seen it in the server's channel list), days and hours are UTC, and the `stats` module switches it all off (or off in some channels).

| Command | What it does |
|---|---|
| `/stats server [period]` | A pixel card of the last 7, 30 or 90 days: messages, voice time, joins, leaves, a column a day, joins against leaves, the busiest hours (weekday × hour) and the top channels and members (players hiding from leaderboards are left out), with buttons to switch the period |
| `/stats me [period]` | Your messages and voice time here, your place among the members and your active days |
| `/stats channel <channel> [period]` | One channel's messages and voice time, its place and its share of the server |
| `/integrations stats counter-add <channel> <kind> [role] [template]` | Makes a channel's name show a live number (`👥 Members: 1,234`; `{count}` marks the number): members, humans, bots, online now, boosts, roles, channels, members with a role, and the server's Pinchora houses, fish caught or average level. Renamed when the number changes, never twice within 10 minutes; at most 10 per server. Answers with the exact permission the bot lacks in that channel (View Channel, Connect, Manage Channels). Manage Server |
| `/integrations stats counter-remove <channel>`, `/integrations stats counter-list` | Stop a counter (the channel keeps its last name); list them with what each shows and what keeps one from updating. Manage Server |

Members, online and boosts come from Discord's approximate counts (one request per server every 10 minutes); humans,
bots and role counters list the members, so they work in servers of up to 10,000 members.

On the dashboard the **Stats** page (Server group) shows the same numbers for 7, 30 or 90 days with the change from the
period before: messages and voice time per day, joins against leaves, the busiest hours, the top channels and members,
each chart with a hover and keyboard readout and a table of its numbers. It also adds, edits and removes counter
channels (with a preview of the name), draws the `/stats` card as the bot does, and switches on a **public page**,
`/stats/<server id>`, that shows anyone with the link the last 30 days without naming a member. `/stats` on the public
site has the whole bot's totals (servers, players, houses, pets, fish, games) and the messages counted each day.

| Address | What it is |
|---|---|
| `GET /api/guilds/{g}/stats?days=7\|30\|90` | The period's totals, the period before, each day, the heatmap, top channels and members |
| `PUT /api/guilds/{g}/stats/settings` | `{ public }` |
| `GET /api/guilds/{g}/stats/card?days=` | The `/stats server` card PNG |
| `GET/POST /api/guilds/{g}/stats/counters`, `POST …/counters/preview`, `DELETE …/counters/{channel}` | Counter channels |
| `GET /api/public/stats`, `GET /api/public/stats/{g}` | The bot's totals and history; a public server's last 30 days (404 unless it is public). Cached five minutes |

### Raid protection and verification

| Command | What it does |
|---|---|
| `/verify` | Pass the join gate: button, five-character pixel captcha with a private answer modal, or a member-bound web link. Captchas last 10 minutes and each answer consumes its image; three wrong answers trigger a 15-minute cooldown. **Ask a moderator** sends a staff alert. |
| `/safety verification setup enabled [verified_role] [unverified_role] [channel] [kick_after]` | Configure the gate and optional timeout kick (Manage Server). Holds join roles, welcome and join responders until verified; then grants join and `after_verify` auto roles. |
| `/safety verification mode mode`, `/safety verification message text`, `/safety verification post` | Choose `button`, `captcha` or `web`, edit the message and post its `vf:go` panel (Manage Server). |
| `/mod raid lockdown [minutes]`, `/mod raid unlock`, `/mod raid status` | Start, end or inspect a raid (Manage Server). Lockdown forces captcha, raises Discord verification, and optionally pauses invites, sets slowmode or locks selected channels. Automatic expiry restores the captured settings while preserving unrelated staff edits. |

**Safety** in the dashboard configures account-age/avatar/name checks, safe roles, exemptions, join-rate and similarity thresholds, automatic alert/lockdown/kick/ban, and displays captcha previews, live raid state and recent joins/events. Use a verified role to unlock member channels alongside Discord onboarding; configure role/channel permissions in Discord. The owner, staff, bots, whitelisted members and anyone above the bot are protected; roles are checked on save and again when applied. Raid alert buttons (`rd:unlock|kick|ban:<event>`) recheck Manage Server and member hierarchy. Modal IDs use `cq:a:<nonce>`. Actions appear in moderation cases/logs and the staff log. Web mode uses `PINCHORA_WEB__PUBLIC_URL` for its private `/verify/<token>` link and requires the same signed-in Discord account.

Held joins still count in server stats and invite tracking. Verification releases the delayed roles and welcome without attributing the join a second time.

Join volume alone alerts; automatic lockdown/kick/ban requires correlated names (young accounts by default) or matching custom avatars. A longer correlation window catches slow drips. Safety REST is paced across pods. Successful lockdown writes are journaled individually; interrupted writes with an unknown Discord outcome keep the raid open for moderator reconciliation instead of guessing ownership.

Migration **0054** adds `verification`, `verification_pending`, `raid_settings`, `raid_events` and `raid_joins`; recent join and closed raid data are retained for 30 days. Modules: `verification` and `anti_raid` in Safety.

### Anti-nuke

`/safety antinuke` and **Safety → Anti-nuke** watch destructive actions per actor in rolling Redis windows.
Protection starts disabled, with alert-only responses. The owner always stays exempt; trusted users
are exempt too, and only the owner can edit trust or weaken existing protections. Nobody can add themselves.
Discord system users and owner/trusted-installed integrations are exempt; unknown integration owners fail closed.

| Command | What it does |
|---|---|
| `/safety antinuke status` | Protection, response and trusted/VIP usage |
| `/safety antinuke enable [channel] [response] [quarantine] [restore] [revert_vip] [remove_bots] [owner_only]` | Enable monitoring and select recovery/response options |
| `/safety antinuke disable` | Pause destructive-action monitoring |
| `/safety antinuke limits kind count seconds` | Allow 1–100 actions per actor in 5–300 seconds; the next action triggers |
| `/safety antinuke trust-add\|trust-remove user` | Owner-managed exemptions, up to 50 members |
| `/safety antinuke vip-add\|vip-remove role` | Protect holders of up to 50 VIP roles |
| `/safety antinuke events` | Recent actions and outcomes |

Watches channel/role create and delete bursts, bans, kicks/prunes, webhook creation, dangerous role/member
permission grants, bot additions, guild/vanity changes and emoji deletion. Gateway audit entries and
guild-wide REST audit batches over five seconds share durable audit IDs, so replays count once.
Counts use the action timestamp, not delivery time. Disabled guilds use an invalidated negative cache. Responses:
alert, strip roles and assign a safe quarantine role, kick, ban, and optional removal of attack bots.
Pinchora checks its permissions and live role hierarchy; if it cannot act it says why in the alert.
VIP members cannot be banned, kicked, timed out or role-stripped by Pinchora moderation; AutoMod and
warn escalations honor the same guard. External VIP bans alert immediately and can optionally be undone.

`an:undo:<event>:<guild>` and `vp:roles:<event>:<guild>` open a private confirmation; `an:confirm` and
`vp:confirm` recheck owner/trust at execution. The web log also confirms recovery. Recreated roles/channels
receive new IDs, with role/category ID remapping, overwrites and positions restored from bounded snapshots.
Every newer ban is preserved, including one by the same moderator; recovery uses the original incident window. Safe VIP roles can return when an unbanned member rejoins.
Messages, original IDs and integration roles cannot be restored; dangerous role permissions require an
authorized manual recovery. Incomplete/deidentified channel ACL snapshots are refused, and deleted role permissions
and positions require deletion-audit evidence. Recreated roles need staff review of channel permissions before
membership restoration. Uncertain REST outcomes are recorded for review and never blindly retried.

Migration **0055** adds `antinuke`, `antinuke_events`, `antinuke_role_backups`, `antinuke_snapshots` and
`antinuke_overwrites`; module **anti_nuke**, group **Safety**. Snapshots: at most 750 objects/server,
64 KiB/object, seven-day retention; member backups: 5,000 snapshots/server. Events retain 30 days.
Privacy deletion clears actor/target rows, trust, role backups and member-specific snapshot overwrites.
All feature text is in `locales/{en,tr}/anti_nuke.toml`. The dashboard includes a safe draft preview.

### Social feeds

New YouTube videos, RSS and podcast episodes, Reddit posts and Bluesky posts, posted in a channel as they
appear: the **Social feeds** module (group Feeds), `/integrations feeds` in Discord and the dashboard's **Social feeds** page.
A server follows up to 10 sources.

| Command | What it does |
|---|---|
| `/integrations feeds add kind source channel [message] [role]` | Follow a source. `kind` is YouTube, RSS or podcast, Reddit, Bluesky, or X/Instagram/TikTok (only with a bridge, below). `source` is a YouTube channel link, `@handle` or `UC…` id (a handle is read from the channel's page, best effort; the channel link always works), a feed address (`https://` is added when missing), `r/name` or a subreddit link, or a Bluesky handle, DID or profile link. The source is fetched once to show what it is and its latest post; what it already shows is never posted, only what comes after. `message` takes `{title}` `{url}` `{author}` `{account}` `{published}` (up to 500 characters; empty uses the kind's default, in the server's language); `role` is pinged with every new post (never `@everyone`) (Manage Server) |
| `/integrations feeds list` | The feeds with their number, channel, role and health: when the source was last checked and the last post went out, or why it fails (the site's error, “I need View Channel, Send Messages and Embed Links in that channel”), five per page (Manage Server) |
| `/integrations feeds edit id [channel] [message] [role] [no_role] [embed] [include] [exclude] [enabled]` | Change a feed: another channel, the message (`-` back to the default), the role, Pinchora's card, filter words (comma-separated, `-` clears; `include` posts only titles with one of them, `exclude` never titles with one; up to 10 words of 40 characters each, any case), or switch it off and on (Manage Server) |
| `/integrations feeds test id` | Post the source's latest item in the feed's channel now, the way new posts look; nobody is pinged (Manage Server) |
| `/integrations feeds remove id` | Stop following a source (Manage Server) |

**How a post looks.** The message, then (when the card is on, the default for everything but YouTube)
Pinchora's card: the title linking to the post, the account, a summary for articles, the picture (a video's
thumbnail, a post's first image, an episode's cover), an episode's length and audio file, the kind and the
time. With the card on the link is posted in `<…>` so Discord adds no second preview; off, Discord shows its
own (YouTube's player). Titles, names and summaries from the source lose their mentions and `@everyone`, and
their markdown is escaped; links are kept only when they are plain http(s). A post pings nobody but the
feed's role (`allowed_mentions`), and a test post pings nobody.

**How sources are polled.** Every pod runs the feeds ticker every 30 seconds; it leases the sources that are
due (`FOR UPDATE SKIP LOCKED`), so a source is fetched by one pod however many servers follow it. YouTube's
channel feed (`videos.xml?channel_id=`) and Reddit's `/r/name/new/.rss` are read every 5 minutes, Bluesky's
public `getAuthorFeed` (no replies, no reposts, no pinned post) every 2, an RSS feed every 5 to 15 minutes by its
own `ttl` (10 by default). Requests carry `If-None-Match`/`If-Modified-Since` and a descriptive user agent. An item
is new for exactly the transaction that first records it in `feed_items_seen`, which queues one post per
subscription in `feed_outbox`; posts are leased, renewed while Discord handles them, and completed only by the
current lease holder. Each announcement uses a stable Discord nonce with `enforce_nonce`, which deduplicates
retries within Discord's recent-message window. A crash after Discord accepts a post but before the database
records it can still cause a duplicate after that window; delivery is at least once in that case. At most the
5 newest new items are posted per check, none older than 3 days, none
published before the feed was added, so a feed that comes back after an outage or changes its ids does not
flood the channel. A source that fails waits twice as long each time (up to 6 hours; a `Retry-After` is
honoured) and the reason shows on `/integrations feeds list` and the dashboard. A post Discord refuses (no permission, the
channel gone) is reported on the feed; after 10 in a row the feed switches itself off. With the module off,
new items are recorded but not posted. Seen item ids expire after 7 days without a fresh full response, sources nobody
follows after a day.

**What may be fetched.** Every address is checked before a connection is made: http or https only, no
password in it, and the host must resolve to public addresses only (loopback, private, link-local, carrier
NAT, multicast, documentation and reserved ranges are refused, IPv4-mapped and NAT64/6to4 forms included).
The connection goes to exactly the checked addresses, so a name that changes its answer gains nothing.
Redirects are followed by hand, at most 3, each checked again and never from https down to http; a body is read
up to 1 MB and a fetch, redirects included, gets 10 seconds; no proxy from the environment is used. A server
may look up 20 sources a minute (adding, checking, testing).

**Dashboard.** The page, previews and feed-specific errors follow the server’s language (English or Turkish).
The Social feeds page lists the feeds with their health (healthy, waiting for the first check,
failing and why, can't post and why, off), `n/10 used`, an on/off switch, **Test** and **Edit**. Adding a feed:
pick the kind, type the source and the page looks it up as you type (“FOUND: Reef Crew, latest: …”), then the
channel, the role, the message with placeholder chips and a character count, the card switch and the filters;
the preview on the right is drawn by the bot's own function from the source's real latest post. An existing
feed's editor has **Send a test post**. The kind and source of a feed never change: add another feed instead.
API: `GET/POST /api/guilds/{g}/feeds`, `POST …/feeds/check`, `POST …/feeds/preview`, `PUT/DELETE …/feeds/{id}`,
`POST …/feeds/{id}/test`; every change is in the audit log (`feeds.add`, `feeds.edit`, `feeds.remove`, from
Discord too).

**Instance settings.** `PINCHORA_FEEDS__BRIDGE_URL` (an RSSHub-style address with `{route}`, e.g.
`https://rss.example/{route}`) turns on X, Instagram and TikTok: the source is the route the bridge serves,
read as RSS. Without it those stay greyed out (“not available on this instance”). The bridge's own host is
trusted (it may be on a private network); where it redirects is checked like any other address. Reddit's RSS is
for non-commercial use; `PINCHORA_FEEDS__REDDIT=false` stops polling existing Reddit subscriptions and drops
their pending posts as well as preventing new ones on an instance Reddit blocks. Debug
builds only: `PINCHORA_FEEDS__DEV_ENDPOINT=127.0.0.1:<port>` sends every feed request to a local server
(`http://127.0.0.1:<port>/<host><path>`), which the e2e tests use.

Data: `feed_sources`, `feeds`, `feed_items_seen`, `feed_outbox` (migration `0056_feeds.sql`; phase 2 only
alters them). A feed's `created_by` becomes 0 when its author deletes their data. Code: `pinchora_core::feeds`
(sources, addresses, filters, timing), `pinchora-commands/src/feeds/` (fetcher, parsers, message, poller),
`commands/feeds.rs`, `pinchora-db/src/feeds.rs`, `pinchora-web/src/routes/feeds.rs`, `web/src/pages/Feeds.tsx`.

### Workflows

<!-- F13: the Workflows package replaces this heading's section; keep it one `###` section. -->

### Branding

<!-- F14: the Branding package replaces this heading's section; keep it one `###` section. -->

### AI characters

<!-- F15: the AI characters package replaces this heading's section; keep it one `###` section. -->

### Roblox verification

<!-- F16: the Roblox verification package replaces this heading's section; keep it one `###` section. -->

### Game trackers

<!-- F17: the Game trackers package replaces this heading's section; keep it one `###` section. -->

### Info commands

`/info user [user]`, `/info server`, `/info avatar [user] [server]`, `/info banner [user]`,
`/info role <role>`, `/info channel [channel]`, `/info members`, `/info emoji [emoji]` and
`/info invite <code>` show pixel cards. Role lists have pages. Private profiles hide Pinchora
levels, badges and titles on the user card, including when the owner requests it. Channel and
invite cards are private and check the viewer’s channel permissions; role pages belong to their requester.

### Utility

`/afk set [reason] [global] [nickname]` marks you away until your next message. Mentions get one short notice per channel every five minutes. `/community utility sticky-set|sticky-remove|sticky-show` keeps a channel message visible after both a chosen number of human messages and a minimum time interval, with batched counters; the bot needs Manage Messages. `/community utility schedule-add|schedule-list|schedule-remove|schedule-pause|schedule-resume|schedule-run-now|schedule-preview` posts once or on an interval, daily, weekly, or with a five-field cron pattern. Each server stores a default IANA time zone such as `Europe/Istanbul`, which a schedule can override; daylight saving changes are automatic. Nonexistent local times are skipped, and repeated local times run once at the first occurrence. Up to 20 schedules fit. Missed recurrences coalesce into one send. A failed or uncertain delivery pauses with a reason; check the channel before resuming. Delivery intent is stored before sending, so a crash cannot automatically replay a potentially delivered message. Managers can edit sticky and scheduled messages on the Messages dashboard page.

### Community pack

<!-- F20: the Community pack package replaces this heading's section; keep it one `###` section. -->

### Events

`/event create` opens a form for an event in the server time zone. Times accept `2026-11-12 20:00`, `tomorrow 20:00`, `yarın 20:00`, `cuma 21:00`, `in 2h` and `2 saat sonra`. `/event list|edit|cancel|attendees|remind|template` manage events, RSVPs, reminders and optional Discord Scheduled Event sync. The public card has Going, Maybe and Declined buttons, capacity and a first-come waitlist. The dashboard Events page can create, edit, duplicate and inspect attendees. Set the server time zone there. `today 8:00 pm` and `bugün 20:00` also work; past, ambiguous and skipped DST times are rejected. Recurring events skip dates whose local time is ambiguous or missing, then continue on the next valid date. Reminder delivery attempts are recorded before Discord I/O: each member gets at most one attempt per event and offset across pods/restarts; a process crash during delivery can lose that DM. Pending reminders follow edits and cancellations, and respect Server DMs.

### XP admin

Managers can `/progression leveling xp-add|remove|set <user> <amount>`, `/progression leveling xp-reset <user>`,
`/progression leveling xp-reset-all` (two confirmations), or `/progression leveling xp-import-mee6` (merge or replace,
once per server per day). The Leveling dashboard has member XP search and an
import job with progress. Edits affect server XP only, resync level roles, and are audited.
Confirmations expire after five minutes and can be used once. Failed or partial imports also
consume the daily allowance; replace changes only members present in the imported leaderboard.

### Brand kit

<!-- F23: the Brand kit package replaces this heading's section; keep it one `###` section. -->

### More games

<!-- F24: the More games package replaces this heading's section; keep it one `###` section. -->

## Quick start (Docker)

1. Create an application at <https://discord.com/developers/applications>, add a bot, and turn
   on **Server Members Intent** (for welcome cards, join roles, goodbye messages and the member, nickname and
   username entries of the server log; or set `PINCHORA_GATEWAY__MEMBER_EVENTS=false`). The server log's ban, audit
   log, message and voice entries use the non-privileged Guild Moderation, Guild Messages and Guild Voice States
   intents. Keyword auto-responders, text commands with a prefix (`p!profile`), the text of edited and deleted
   messages in the log and `/purge contains|links|attachments` need the privileged **Message Content Intent** as
   well: turn it on in the Developer Portal and set `PINCHORA_GATEWAY__MESSAGE_CONTENT=true` (Discord asks for
   verification above 100 servers). Without it the dashboard shows keyword triggers as locked, the bot never reads
   chat text, and text commands only work after an @mention of the bot. Give the bot **View Audit Log** for the
   role, channel, server, invite, emoji, kick and timeout entries, and Ban Members, Kick Members, Moderate Members,
   Manage Messages, Manage Channels, Manage Nicknames and Manage Roles for the moderation commands it should carry
   out.
2. `cp .env.example .env` and fill in the token and application id.
3. Start everything:

   ```bash
   docker compose up -d
   ```

   For a deploy with the source commit on `/info`, build the image with
   `GIT_SHA=$(git rev-parse --short HEAD) docker compose build`, then run
   `docker compose up -d`. Compose uses `dev` when `GIT_SHA` is unset.

4. Register the slash commands (to `PINCHORA_DISCORD__DEV_GUILD_ID` if set, else globally):

   ```bash
   docker compose run --rm gateway register
   ```

5. Invite the bot with the `bot` and `applications.commands` scopes.

To serve commands over HTTPS instead of the gateway, run the `endpoint` profile
(`docker compose --profile endpoint up -d`), expose port 8080 at a public HTTPS URL, and set
`https://<host>/interactions` as the Interactions Endpoint URL in the Developer Portal.

Kubernetes manifests and scaling notes: [`deploy/k8s/README.md`](deploy/k8s/README.md).

## Web dashboard

`pinchora web` (the `web` compose service, <http://localhost:8090>) is a browser dashboard for server
managers: log in with Discord, pick a server you have **Manage Server** in (servers without the bot get an
"Add bot" invite), and change its settings. Every change is written to an audit log and announced on Redis,
so every bot pod uses it from the next message instead of after its one-minute cache.

| Page | What it sets | Same as |
|---|---|---|
| Overview | Chat XP on/off, the text command prefix, every page's state at a glance, every module's state, the latest changes | `/setup server show`, `/progression leveling settings enabled`, `/setup server`, `/setup server modules-show` |
| Modules | Each part of Pinchora (leveling, points, welcome, goodbye, log, moderation, role panels, auto roles, colour roles, store, suggestions, polls, giveaways, custom commands, profiles, economy, shops, quests, social, houses, plaza, pets, fishing, duels, board and puzzle games, word and trivia games, party games, word party games, critter drops, gambling, text commands) on or off, and where it works: all channels, only some, or all but some | `/setup server` |
| Welcome & goodbye | Welcome cards (channel, text with placeholder chips), the welcome image the bot sends now (drawn by the bot) with a link to the editor, join role, goodbye channel and text | `/setup welcome message`, `join-role`, `goodbye` |
| Welcome image | The image editor for welcome and goodbye pictures (below) | `/setup welcome card` (the classic card) |
| Leveling | Leveling, where level-ups go, level-up channel and text, voice XP, XP multipliers, level roles (stack/replace, **Sync members**), reputation roles | `/progression leveling settings`, `xp`, `level-roles`, `rep-roles` |
| Points & store | Points on/off, name, emoji, points per message; roles on sale with price and stock | `/progression economy points-settings`, `/progression economy` |
| Moderation | The mod-log channel, direct messages to members, and the warn ladder | `/safety moderation settings`, `/safety moderation warn-ladder` |
| Cases | Every case, searchable by name, reason or id and filtered by kind or what still stands; a case opens whole with the member's other cases, and its reason can be changed (the mod-log entry follows) | `/mod case`, `/mod reason` |
| Server log | The log channel, a channel per category, every event on or off (marked when it needs View Audit Log), and the ignore lists | `/safety moderation log`, `/safety moderation log-events`, `/safety moderation log-ignore` |
| Staff log | What staff did with Pinchora's commands, on the dashboard and by hand in Discord, searchable and filtered by where (kept 180 days) | |
| Roles | Auto roles (who gets which role on joining, with a delay or an expiry), sticky roles, role panels as buttons, a menu or reactions with a live preview and posting from the page, and colour roles (palette, mode, who may pick, the anchor role, making the missing roles) | `/setup roles`, `/setup roles`, `/color` |
| Custom commands | The commands members run with `/tag`: name, description, one to five replies (one picked at random), an embed, up to three actions (give, take or toggle a role, give or take server points, send a DM), who may use it and where, a cooldown, a live preview drawn by the bot, import and export as a JSON file | `/setup commands` (replies only) |
| Auto-responders | Messages the bot sends by itself when a member joins, leaves, levels up, gets or loses a role, or says a keyword (locked unless the bot has the Message Content intent): where it answers, how often, a cooldown, actions, a preview, import and export | |
| Community | The `/suggest` channel; critter drops on/off and where they land; timed storm surges (on/off, channel, how often) | `/community server suggestions`, `/progression economy critters`, `/progression economy surge` |
| Audit log | Every dashboard change: who, what, before and after (kept 180 days) | |

Modules (`pinchora_core::modules`) are the one place that decides what a switched-off part does: the bot
asks `App::module_access` before every command, button and form (`gate.rs`, with tests that fail when a
command or button prefix has no module) and in the chat, welcome, goodbye, log and voice paths. A server
that never touched them has everything on (welcome cards and gambling stay off until they are switched on), and a module back to
"on everywhere" keeps no row in `guild_modules`.

A module switched off pauses what runs on a timer instead of ending it: giveaways and polls whose time passes
while their module is off (in their server, or in the channel they live in) are neither drawn nor closed and
post nothing, and end on the next tick after the module is switched back on. Party rounds are different: one
that is already running when its module is switched off still settles through the janitor, paying out or giving
back its escrow, so no stake stays locked; the switch only refuses new commands and clicks. Chat XP spends its
one-minute cooldown only where leveling or points count, chat counts towards quests only while quests are on, and
a level-up is not posted in a level-up channel the leveling module is off in (it is told where it was earned).

### Who may hand out a role

Every place a server sets up a role for the bot to hand out (level, reputation, store and join roles, role panels,
custom command and auto-responder actions; on Discord and on the dashboard) checks it with one rule
(`pinchora_core::settings::grant_refusal`), live when it is saved:

- The bot must be able to give it: not `@everyone`, not managed by an integration, below the bot's highest role.
- A role with Administrator, Manage Server, Manage Roles, Manage Channels, Manage Webhooks, Ban, Kick or Timeout
  Members, Mention @everyone or Manage Messages is only set up by the server owner or an administrator.
- Nobody but the owner sets up a role at or above their own highest role.
- Custom command and auto-responder actions, which any member sets off whenever they like, never hand out a role
  with those permissions, whoever wrote them, and the bot checks the role's permissions again each time it gives it.
  On the dashboard, editing, switching on or overwriting (import) a command or responder checks all its role
  actions against the person doing it.

The dashboard's role pickers grey out, for the manager looking, what they may not pick, with the reason.

### Custom commands and auto-responders

Servers write their own commands (`/tag name`) and messages the bot sends by itself, on the dashboard or, for
the simple case, with `/setup commands`. Both go through the same rules (`pinchora_core::tags`: limits, the
sentences that refuse a draft, the placeholder language) and the same writes, and every change is audited and
announced to the bot. A server has up to 50 commands and 25 auto-responders; a reply is up to 2,000 characters,
an embed up to 6,000 in all with at most 25 fields, and a command does at most three actions.

- **Placeholders**: `{user}` (a mention), `{user.name}`, `{user.id}`, `{user.avatar}`, `{server}`,
  `{server.members}`, `{channel}`, `{random:a|b|c}`, `{points}` and `{points.name}` (after a points action); a
  command also has `{args}`, `{1}` to `{5}` and `{1:default}`; responders have `{count}` (join), `{level}` and
  `{role}` (level-up) or `{role}` (role changes). Anything else is left as written, and a value that looks like a
  placeholder is not filled again.
- **Safety**: everything goes through one pipeline (`custom.rs`). What a member typed or a name is stripped of
  mentions and `@everyone`/`@here` before it is placed in a message; with "Allow mentions" off (the default) the
  mentions the author wrote are dropped too, and only the member it is about is pinged; with it on, the users and
  roles the author wrote can be pinged, never everyone or here. Markdown in what a member typed, in names and in
  the server's name is escaped, so `{args}` cannot become a masked link, bold text or a heading. Actions can only
  move **server points**, never shells; role actions follow "Who may hand out a role" above.
- **Action limits**: a command or responder with actions waits at least 30 seconds per member between uses (a
  shorter cooldown is refused, and an older row runs with 30), points actions give one member at most 1,000 a day
  per server (taking is not capped; the member is told), and direct messages from actions and DM responders stop
  at 5 an hour per member per server and never reach a member who switched them off with `/privacy`. A role
  change an auto-responder makes sets off no other role responder: a chain stops after one responder.
- **Auto-responders**: member join, leave, level-up (from a level on), role gained or lost (a member's first update
  after a restart only teaches the bot their roles), and keyword (contains, exact, starts with, ends with; never
  case-sensitive; at most one answer per message). Each has a chance, a cooldown per member, and answers where it
  happened, in one channel, or by direct message (closed DMs are skipped quietly). Keywords need
  `PINCHORA_GATEWAY__MESSAGE_CONTENT=true`; without it they cannot be switched on and the dashboard says why.
- **Modules**: the *Custom commands* module covers `/tag`, `/setup commands` and the responders, and can be kept
  to channels like any other.
- **Export and import**: a JSON file (`{"version": 1, "items": [...]}`) per list. An import checks every item like
  a new one, leaves out roles and channels the other server lacks (and role actions the importer may not set up),
  skips (or with "replace" overwrites) names that exist, and reports what it refused. The audit entry of a
  replacing import keeps the commands it overwrote.

The dashboard and the slash commands check values with the same rules (`pinchora_core::settings`): the same
limits and the same sentences, and a role the bot cannot hand out is greyed out with the reason `/setup`
gives. `web/src/lib/rules.json` carries those limits and sample answers to the browser (`cargo test -p
pinchora-web rules -- --ignored write_rules` rewrites it; a test fails while it is out of date).

To turn the login on:

1. Developer Portal -> your application -> **OAuth2** -> **Redirects**: add
   `<PINCHORA_WEB__PUBLIC_URL>/auth/callback`, i.e. `http://localhost:8090/auth/callback` by default (and
   `http://localhost:5173/auth/callback` if you work on the UI with `npm run dev`).
2. Copy the **Client Secret** from the same page into `.env` as `PINCHORA_WEB__CLIENT_SECRET`.
3. `docker compose up -d --build web`, open <http://localhost:8090> and log in.

Until the secret is set the dashboard serves a setup page that lists what is missing and the redirect to
register; the rest of the stack runs either way. Other settings (`PINCHORA_WEB__PUBLIC_URL`, `__CLIENT_ID`,
`__BIND`, `__COOKIE_SECURE`, `__STATIC_DIR`, `__TRUSTED_PROXIES`) are described in [`.env.example`](.env.example).
Behind a reverse proxy, list it in `PINCHORA_WEB__TRUSTED_PROXIES` so the login rate limit counts each client
instead of the proxy.

The login asks Discord for `identify guilds` only. Sessions live in Redis behind an HttpOnly cookie (7 days
idle, 30 days at most; logging in again ends the old one); every write needs the session's CSRF token and the
dashboard's own origin. Over HTTPS the dashboard sends HSTS.

The UI is a Vite + React app in `web/`, built into the binary (`crates/pinchora-web/build.rs` embeds
`web/dist`; without Node a placeholder page is embedded, so `cargo build` never needs npm):

```bash
cd web && npm ci
npm run dev        # http://localhost:5173, proxies /api and /auth to the Rust server on :8090
npm run lint && npm run typecheck && npm test && npm run build
cargo build -p pinchora && PINCHORA_TEST_DATABASE_URL=... PINCHORA_TEST_REDIS_URL=... npm run e2e
                   # Playwright smoke against the real server and a mock Discord; screenshots in test-results/screens/
```

For UI work against a running server, set `PINCHORA_WEB__PUBLIC_URL=http://localhost:5173` on the Rust side
(`cargo run -p pinchora -- web`).

### Welcome and goodbye images

The **Welcome image** page is a canvas editor for the picture a new member gets and the one that says goodbye.
A design (`pinchora_core::card_design`, versioned JSON) is a canvas of 300 to 1600 by 150 to 1000 px, a
background (none, a colour, a gradient with or without the pixel dither and sparkles, one of the 36 pixel-art
backgrounds scaled up in whole steps, or an uploaded picture), a darkening scrim, and up to 20 layers from
bottom to top: the **avatar** (circle, rounded, square or pixel-notched, a border, chunky pixels or smooth),
**text** with `{user}` `{username}` `{server}` `{members}` `{ordinal}` (Noto Sans or the pixel font, bold, size,
colour, alignment, capitals, up to 4 wrapped lines cut with an ellipsis, a hard shadow, an outline), **stickers**
(crab, heart, star and shell glyphs in any colour, or any pet, critter, fish, badge or furniture sprite from the
atlas, resized in whole pixel steps) and **shapes** (box, rounded, pixel panel, ellipse, the dashed stage edge).
Each layer can be hidden, locked and faded. One validator checks a design for the API, the bot and (through
`rules.json`) the editor's inputs; a setting it does not know is refused by name, and every problem is listed
with its layer.

A design is also kept cheap to draw, so a card takes under 400 ms (release build) however it is made: a layer is
never bigger than the canvas, an avatar is at most 1024 px across, a design has at most 4 avatars, and the
visible layers together may use at most `LAYER_COST_MAX` (30 million pixel-equivalents): a shape or sticker costs
its width times height, an avatar five times that, and a text layer 80,000 per character it can show, a
placeholder counted at the longest it can be (a name 32, a server 40). The editor shows the share used as the
**draw cost** bar over the layer list, holds sizes to the canvas, and refuses a layer that would go over.
`cargo test --release -p pinchora-render --test cost -- --nocapture` builds the heaviest designs the validator
allows (all text in emoji and CJK, four avatars, twenty faded layers, a noisy upload) and times them; the
slowest must take 400 ms (it measured 270 ms on a loaded laptop; set the numbers again if the costs change).
Uploads are kept at 1 MiB of PNG at most (a noisy picture is shrunk until it fits) and designed cards are
written with the fast PNG level, because squeezing a photo at the default level took longer than drawing it.

In the editor: drag layers with a mouse, pen or finger, pull the handles to resize (avatars stay square, text
corners scale the font, side handles set its width), snap to the canvas middle and edges and to other layers
(guides show; Alt or the Snap switch turns it off), nudge with the arrow keys, undo and redo (Ctrl+Z / Ctrl+Shift+Z,
a whole drag is one step), duplicate (Ctrl+D) and delete; reorder, hide and lock in the layer list; zoom or fit.
A template gallery holds eight starts: the classic card in each of the five themes (layer for layer the card
Pinchora always drew; a server that never opened the editor gets it in its theme, headline and accent),
*Starlit banner*, *Clean card* (transparent) and *Pet party*. **Send** picks how it goes out: with the text in one
message, the image first then the text, the image alone, the text alone, or (welcomes) by direct message to the
new member (closed DMs are skipped). **Send test** posts it to a channel you pick, with you as the member and
pinging nobody (the note under it is in the server's language, like the `/setup welcome card` preview's text, which follows the member's own). The canvas is drawn by the browser so every drag is instant; **Try it** below holds the test and, behind
**Show the bot's picture**, the bot's own PNG of the same design (`POST /api/guilds/{g}/preview/card`, debounced),
which is exactly what members get, because the member events, the preview and the test all draw through the same
`pinchora_commands` code. Previews (and test messages) are drawn in their own lane: two at a time, waiting at
most 3 s for a turn (then 503 `busy`) and given up on after 6 s (503 `slow`), so an editor being dragged around
cannot crowd the member events' cards out of the render slots; a session may ask for 60 a minute and a server for
20 (429, which the editor waits out and asks again). From 1024 px the canvas stays in view beside one column
holding the layers and the properties; tablets get the canvas over the two panels side by side; phones a single
column with the settings a thumb needs.

Uploads (`POST /api/guilds/{g}/cards/uploads`, the file's own bytes): PNG, JPEG, WebP or GIF (first frame) up to
4 MB and 4096 px a side, decoded within those limits, shrunk to fit the largest canvas and 1 MiB and stored as PNG in
`card_uploads`; a server keeps its 12 newest plus any a saved design uses, and a design can only name the
server's own. Designs live in `card_designs` (migration 0043; `design` NULL is the classic card), saving is
audited (`card.put`, `card.upload`) and announced like other settings. The API: `GET /cards` (both images, the
templates, the uploads), `PUT /cards/{welcome|goodbye}` (`design` or `null`, `delivery`), `POST
/cards/{kind}/test` and `GET /cards/uploads/{id}`. `cargo test -p pinchora-render --test designed` checks the
classic template against the old card pixel for pixel and every template against its golden in
`crates/pinchora-render/tests/golden` (`PINCHORA_UPDATE_GOLDENS=1` rewrites them).

### Public website

The same server (`pinchora web`) is the bot's public site. `/` is the landing page for everyone (a logged-in visitor
sees a **Dashboard** button in the header instead of **Log in**; the dashboard keeps its own paths: `/servers`,
`/g/<id>`, `/me/house`, `/login`), `/commands` is the searchable command list, `/features/<area>` has a page for each
of the six areas, `/privacy` and `/terms` are plain-language drafts to review before launch, and anything else is a
404 page. The pages are in `web/src/site` (the words and links of the landing page in `content.ts`, the areas in
`features.ts`, the styles in `web/src/styles/site.css`).

| Address | What it is |
|---|---|
| `/invite` | Redirects to Discord's page for adding the bot, with the `bot applications.commands` scopes and the permissions it uses; every **Add to Discord** button points here |
| `/api/public/landing` | `{ stats, viewer }`: servers, players, houses and pets counted in the database (cached in Redis for ten minutes, one refresh at a time), and the logged-in visitor's name and avatar, or `null`. Rate limited per address |
| `/api/public/status` | Public bot status from live gateway shard heartbeats and database counters, cached in Redis for 30 seconds |
| `/robots.txt`, `/sitemap.xml`, `/og.png`, the favicon set, `/site.webmanifest` | Written by the build or generated (below); `__ORIGIN__` in them becomes `PINCHORA_WEB__PUBLIC_URL` when served, so one build serves any domain |

Search engines and link previews do not run scripts, so the build writes each public page's own title, description,
canonical link and Open Graph / Twitter tags into its HTML (`commands/index.html`, `features/games/index.html`, ...,
from `web/src/site/routes.ts`; the Vite plugin in `web/vite.config.ts` does it) and the server answers
`/commands` with that file. The dashboard's pages get the shell with `X-Robots-Tag: noindex`. The public pages work
before login is set up; only the dashboard shows the setup page. The support server link in the footer is a placeholder:
build with `VITE_SUPPORT_URL=https://discord.gg/<invite>` to set it. Set
`PINCHORA_SUPPORT_URL` to the same Discord invite for the `/info bot` card and `/status` page.

The showcase cards and rooms are drawn by the bot's own renderer from sample data
(`crates/pinchora-commands/src/landing_assets.rs`: the profile card with real backgrounds, badges, showcase tiles and
character, an isometric room, chess, connect four, wordle, the rank, wallet, welcome, level-up, leaderboard, pet and
badge cards), committed in `web/src/site/showcase`. The [brand kit](docs/BRAND.md) supplies the mascot,
favicons and 1200x630 link preview in `web/public`, built with `python3 tools/brand/build.py`.
Refreshing showcase cards leaves those brand exports alone.
`landing_assets_match` fails when a card, the art or the catalogue changes what these look like; regenerate with
`cargo test -p pinchora-commands write_landing_assets -- --ignored`. The command list and the content counts on the
landing page come from the registered command tree and the content files, in `web/src/lib/commands.json` and
`facts.json`; `committed_site_data_matches` fails while they are out of date and
`cargo test -p pinchora-web commands_catalog -- --ignored write_site_data` rewrites them. A new command needs an area in
`/help`'s `AREAS`, and then shows on the site by itself.

### House viewer (read-only)

Players open **My house** (`/me/house`, in the top bar) to look at their rooms, and any player's unlocked
house at `/house/<player id>` (a locked or hidden house answers with the same sentence `/house visit` does, hidden
first; a server's plaza is never served by id). Pick a room, pan,
zoom in whole steps, hover or tap a piece to see its name, switch the tile grid on, and press **Compare
with Discord** to lay the bot's own picture of the room over the browser's and see how many pixels differ
(none). Looking never moves anyone in: no house means a 404 telling the player to run `/house`, and nothing
is paid for visiting from the web.

The browser draws the room itself, in `web/src/iso`: a TypeScript port of `pinchora-render/src/room.rs`
(projection, walls, floor, rugs, shadows, wall hangings, painter's order, pets, characters) on plain pixel
buffers, fed by the same sprite sheet the bot embeds, served as `/assets/atlas.png` and `/assets/atlas.json`.
Characters are the one thing it does not draw: the server draws each one (`/api/house/character.png`) and the
browser places the sprite. The API is `GET /api/house`, `/api/house/rooms/{idx}` (+ `/render.png`, the bot's
picture), `/api/house/visit/{player}` (+ `/rooms/{idx}`), `/api/house/content` (the catalogue the browser
draws with) and `/api/house/character.png?code=`.

The two renderers are kept equal by fixtures the Rust code writes: scenes (empty, furnished, crowded 14x12 with 60
pieces, sleeping and walking pets, grid and cursors on both walls) as JSON next to the PNG the bot draws at native
resolution, plus placeholder art and sleeping pets as fingerprints, in `web/src/iso/fixtures`. `npm test`
draws every scene in TypeScript and requires every pixel to match; `cargo test -p pinchora-commands
house_fixtures_match` fails when the renderer, the art or the catalogue moves away from what is committed.
After changing the room renderer or the sprite sheet, regenerate them and commit the result:

```bash
cargo test -p pinchora-commands write_house_fixtures -- --ignored
```

## Development

```bash
cargo run -p pinchora -- gateway          # needs PINCHORA_* env vars or pinchora.toml
cargo run -p pinchora -- web              # the web dashboard on :8090 (see "Web dashboard")
cargo run -p pinchora -- commands         # print slash command JSON
cargo run -p pinchora-render --example samples   # render sample cards to target/render-samples
                                                 # (profile-background/-badges/-placeholder, rank-background, wallet-background show /customize;
                                                 #  the profile and rank samples carry a title; profile-showcase-*, profile-bg-<white|black|stripes|beach>-<opacity>,
                                                 #  profile-nobg-*, profile-unicode/-wide/-cyrillic/-shout and profile-frame-* are the stress variants;
                                                 #  wrapped and wrapped-quiet are the year card for a busy player and for one with nothing)
cargo run -p pinchora-render --example games     # connect four, hangman and wordle sample cards
cargo run -p pinchora-render --example board_games   # chess, checkers, Sea Fleet radar and fleet, picross and 2048 sample cards
cargo run --release -p pinchora-render --example characters   # characters-looks.png (all 24 looks with a hat, glasses and a
                                                 # hand item), characters-<slot>.png, characters-colours.png, character-editor.png
cargo run -p pinchora-render --example collection # critter drop, bottle, dex, haul and crab pot sample cards
cargo run -p pinchora-render --example party_games    # party-*.png: lobby, Hot Pearl table, standings, letters, votes, race, postcard, would you rather, prompts
```

Tests: unit tests run with plain `cargo test --workspace`. Database and Redis integration
tests run when these are set:

```bash
PINCHORA_TEST_DATABASE_URL=postgres://pinchora:pinchora@localhost:5432/pinchora \
PINCHORA_TEST_REDIS_URL=redis://localhost:6379 \
cargo test --workspace
```

The database waits up to a minute for a free connection in the tests (five seconds in the bot), so a
machine running several suites at once does not fail them with a timeout. The tests that depend on
real time (a countdown such as "back in 1 h", the five-second window between a giveaway's message
edits) allow for the time the test itself has used or hold the window open by hand, and the ones that
run the game janitor first close the late games earlier runs left in the database.

## Layout

```
crates/
  pinchora/           binary: gateway | interactions | web | migrate | register | commands
  pinchora-core/      config, telemetry, level/economy/cluster math, settings, module and custom-command rules,
                      languages (`i18n`, catalogs in `locales/<code>/*.toml`)
  pinchora-db/        PostgreSQL (sqlx) + migrations runner
  pinchora-cache/     Redis: cooldowns, gateway sessions, cross-pod identify queue, invalidation pub/sub
  pinchora-render/    pixel-art cards (SVG → PNG with resvg)
  pinchora-commands/  slash commands, buttons, gateway event handlers; `hooks.rs` is where feature packages listen
                      to messages, joins, leaves, reactions, DMs and level-ups
  pinchora-web/       web dashboard API (axum): Discord OAuth, sessions, CSRF, server settings, modules, the house viewer, the public site's data and invite link, the SPA
web/                  dashboard and public site UI (Vite + React + TypeScript), embedded into the binary
migrations/           SQL migrations
assets/fonts/         Silkscreen + Noto Sans, and Noto Sans CJK + Noto Emoji subsets as fallback (SIL OFL)
deploy/               Kubernetes (kustomize)
docs/                 architecture, research, game design
```

Design notes: [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md). Research behind the choices:
[`docs/research/rust-bots.md`](docs/research/rust-bots.md) and
[`docs/research/games-survey.md`](docs/research/games-survey.md).

## Art pipeline

```bash
cd tools/art
python3 plan_sheets.py     # content → 40 sheet prompts + cut manifests in sheets/
./run_queue.sh 3           # Codex draws them (3 at a time, resumable)
./import.sh                # cut, de-scale, palette-quantize, pack assets/sprites/atlas.*
```

Until a sprite exists, the renderer draws a tidy placeholder in its place, so every feature
works before the art lands. Characters are drawn by one function, `pinchora_render::character::render_character`
(a serde-serialisable `CharacterSpec`: look, colours, accessories, pose), which finds the eyes, brow, collar, torso and
hands in each look's art and draws the accessories there in code. An atlas sprite `acc_<id>` replaces an accessory's
drawing when it exists (tintable ones drawn in the coral base ramp, like furniture). Backgrounds are `bg_<id>` (240×135, drawn ×4 to 960×540 and
cropped to cover the rank and wallet cards) and badges `b_<id>` (24 px wide, drawn ×2); until
they are in the atlas a background is a gradient from its accent colour and a badge is a
coloured circle with its first letter.

## Fonts

Silkscreen and Noto Sans are under the SIL Open Font License; the license texts are in
`assets/fonts/`.

Silkscreen is a modified version: `tools/fonts/silkscreen_tr.py` adds the Turkish letters it lacked, İ ı Ş ş Ğ ğ,
to both weights, on the font's own 125-unit pixel grid (İ is an I with a dot, Ş an S with Ç's cedilla, Ğ a G with a
breve in the rows of Â's circumflex; Silkscreen has no lower case, so ı is the I bar), and moves the dots of Ö and Ü
(and the circumflex of Regular's Û) onto that grid. Ç Ö Ü Â were already there. The family name stays
`Silkscreen`: its copyright line declares no Reserved Font Name. The change is noted in the font's version and
description fields and in `OFL-Silkscreen.txt`. Re-run the script (fontTools) to rebuild; it is idempotent. The
profile, rank and wallet cards capitalise Turkish text with `Lang::upper` (`i` becomes `İ`, as `lang::upper` does
for the wordle board), so the dots land where Turkish puts them.

Names, bios and titles in emoji or CJK draw as letters, not empty boxes, because two fallback fonts follow the
card fonts in the renderer's font database (`pinchora_render::FONT_FILES` for the cards and the dashboard,
`FALLBACK_FONT_FILES` for these). Both are SIL OFL, made from the official releases with `pyftsubset` (fontTools;
no hinting, no layout features except the emoji sequences):

| File | From | Holds | Size |
|---|---|---|---|
| `NotoSansCJKsc-Subset.otf` | Noto Sans CJK SC Regular 2.004 ([notofonts/noto-cjk](https://github.com/notofonts/noto-cjk)) | 7,174 common hanzi (GB 2312 level 1, Big5 frequently used, JIS X 0208 level 1), kana, the 2,350 hangul syllables of KS X 1001 and compatibility jamo, bopomofo, CJK and full-width punctuation | 2.0 MB |
| `NotoEmoji-Subset.ttf` | Noto Emoji 3.002 ([google/fonts](https://github.com/google/fonts/tree/main/ofl/notoemoji)), monochrome, Regular weight | every emoji code point of the font (1,489) and its sequences (flags, ZWJ, skin tones) | 0.9 MB |

A character no font has (a rare hanzi, another script) still draws as an empty box.


### Reef private-house multiplayer

The web Play view (/me/house?play=1 or /house/OWNER?play=1) supports up to 16 players in a
server-authoritative room on one web pod. Separate logged-in browser profiles can walk and use the pilot seats,
with saved appearances and filtered name tags. Hidden/locked houses and bidirectional social blocks restrict visitor access.
House chat requires versioned 13+ self-acknowledgment (Discord login does not verify age). EN/TR text checks,
200-grapheme messages, 1/second burst 3 and 20/minute limits, plain-text bubbles and an accessible transcript apply.
Hosts moderate their house; comma-separated `PINCHORA_WEB__OPERATOR_IDS` grants site moderation at `/reef/moderation`.
Reports have receipts, dispositions and appeals; evidence reads and actions are audited. Evidence expires after
7 days, or 90 days if reported. Migration 0065 extends guestbook blocks to general social exclusion and adds
privacy-registered safety records. Active sanctions survive account deletion only until their expiry.
Public plaza chat remains disabled. No automatic Discord chat bridge exists.
A second tab for an account replaces its earlier controller. An unavailable socket falls back to local preview.
Chat and movement give no rewards. See [the Reef protocol and test instructions](web/src/reef/README.md#private-house-multiplayer-r09r11).
