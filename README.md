# Pinchora

A cozy pixel world for Discord servers: an isometric house you build and furnish from 1,800+
items, a pixel character, pets, fishing, a shared server plaza, levels and pixel-art cards.
Written in Rust and built to run sharded on Kubernetes.

![A furnished room](docs/images/room.png)

![Profile card](docs/images/profile.png)

![Character creator](docs/images/character.png)

![Leaderboard](docs/images/leaderboard.png)

## What works today

| Command | What it does |
|---|---|
| `/profile [user]` | Pixel-art profile card: level, XP, shells, reputation, streak, global and server rank, bio, an optional title under the name, up to six badges, each on a plate tinted by its rarity, and, if the player chose any, a **showcase** row (favourite fish, pet, furniture, house thumbnail) and their style (accent colour, panel opacity, frame). Until you choose your badges the card shows your best earned ones (rarest first) on its own. With nothing on the showcase the card keeps its classic layout; with a showcase the about-me panel and the stats give up height to a row of tiles. With a partner (`/partner`), the corner of the about-me panel shows `♥ NAME`, measured as drawn so accented, non-Latin and emoji names keep the heart beside them. |
| `/rank [user]` | Server level card, with the next level role to earn, the title under the name and the player's card style |
| `/customize background` | Private editor with a live preview of your profile card: 36 backgrounds in five rarities, bought once with shells (**Buy**), then worn (**Equip**) or taken off (**Remove**); the menu lists owned ones first, then the shop, 25 per page. The profile, rank and wallet cards draw the background under their panels and take their accent colour from it |
| `/customize badges` | Private editor with your badge collection card: all 40 badges in a grid, earned ones in colour on a rarity plate, locked ones dark with a progress bar, a `23/40` counter and a NEW tag on what you earned since you last looked. 28 achievements worked out on the spot from real progress (level, fish species, house likes, pets, daily streak, furniture placed, reputation, shells) and 12 shop badges bought once with shells; wear up to six, in the order you pick them, take them all off, or press **Show my best automatically** to let the card choose again. A badge you no longer qualify for (a lapsed streak, spent shells) leaves the card until you earn it again |
| `/customize title` | Private editor for the title under your name on the profile and rank cards: 29 earned by stats (level, fish species, house likes, pets, streak, furniture, reputation, shells) or by owning a badge, plus the special *House of the Week* for contest winners; locked titles show what they still need. **Remove** takes it off |
| `/customize style` | Private editor with a live preview of your profile card: an **accent colour** (Auto follows the background; 12 pixel-palette presets; or **Hex code…**, a form that refuses unreadable near-black values), **panel opacity** (Light, Normal, Solid; offered once you wear a background picture, since there is nothing for it to show over before) and a pixel **frame** drawn around the whole card (Plain and Rope free; Coral from level 10, Pearl 20, Gold 30, Neon 40, shown with a lock and the level until then). **Reset style** puts it all back, a frame saved above your level included. The style is on the profile, rank and wallet cards |
| `/customize showcase` | Private editor with a live preview: up to three picks for the profile card, a favourite **fish** (from your fish book), a **pet** and a **furniture** item (owned or placed), chosen from menus of what you have (25 per page), plus a **Show house** switch for a thumbnail of your first room (drawn once and kept until the room changes). Sprites are drawn at whole-number scales only, so every pixel stays a crisp block: small ones are enlarged up to 4×, big ones reduced by a whole divisor without blending colours. **Clear slot** and **Clear all** empty it again; empty slots are simply not drawn |
| Badge unlocked notices | When a command or chat level-up earns you an achievement badge (`/daily`, `/fish`, `/pet`, placing furniture, `/give`, a new chat level), a short "Badge unlocked" embed with the badge art follows the reply, once per badge |
| `/leaderboard [board]` | Server XP, server points, **Weekly XP** (resets Monday 00:00 UTC, with last week's top three in a panel under the board), **Weekly catch** (each player's biggest catch of the week by size for its species, so a 7.9 cm minnow can beat a 200 cm sailfish; boots and tins do not count; same Monday reset and last-week podium), global XP or global shells as a pixel-art card, with page buttons |
| `/daily` | Daily shells; the streak bonus grows for 20 days |
| `/balance [user]`, `/give user amount` | Wallet and transfers |
| `/rep user` | One reputation point per day; a server's reputation roles are given the moment the receiver's total reaches them, and the reply says so |
| `/bio text` | About-me text on the profile card |
| `/privacy [server_dms]` | Your own choice, in any server or in DMs: with `server_dms` off, no server's custom commands, auto-responders, level-ups or suggestion updates reach you by direct message; without it, it shows the current choice. Always available, whatever a server switches off |
| `/config welcome\|leveling\|show` | Welcome cards and level-up messages. `/config leveling` takes `enabled`, `channel`, `message` (placeholders `{user}` `{level}` `{server}` `{role}`, the last being the reward role just earned, empty when none) and `mode`: `channel` (the level-up channel, else where they chatted), `current` (always where they chatted), `dm` (a direct message; closed DMs are skipped quietly) or `off` (no message, roles still given) (Manage Server) |
| `/config level-roles add\|remove\|list\|mode\|sync` | Roles given at a level (up to 25): `mode` keeps every role earned or only the highest, `sync` catches up members who already passed a level (one at a time per server, from Discord or the dashboard, with a 5 minute rest after each). Who may set up which role is in "Who may hand out a role" below (Manage Server) |
| `/config xp multiplier value [channel] [role]` | Scale the server XP and points a channel (text, voice or forum) or category, or a role, earns, from 0 to 5; 0 means no XP, 1 is normal and drops the rule. Threads and forum posts follow their channel, and a channel follows its category unless it has a rule of its own (the nearest rule wins). A channel's multiplier is multiplied by the member's highest role multiplier, a no-XP role always wins, and `@everyone` is a server-wide multiplier. Global XP is never scaled. Up to 50 rules (Manage Server) |
| `/config xp clear [channel] [role]`, `/config xp show` | Remove one multiplier or all of them, or list the multipliers with the voice settings (Manage Server) |
| `/config xp voice [enabled] [amount]` | Voice XP, off by default: members in a voice channel with at least one other person earn `amount` (1 to 100, default 10) server XP a minute; nothing while alone (bots do not count), muted and deafened at once, or in the server's AFK channel. Multipliers apply by the voice channel (or its category) and the member's roles; level-ups follow the leveling mode. Switching it on does not see who is already in voice: they start earning when they next join, move, mute or unmute (Manage Server) |
| `/config rep-roles add\|remove\|list` | Roles given at a reputation count (up to 25, same safety checks as level roles); they always stack and are given when a `/rep` point lands (Manage Server) |
| `/config join-role [role] [off]` | A role every new member gets, bots excepted; with neither option it shows the current setting (Manage Server) |
| `/config goodbye [channel] [message] [off]` | Goodbye message in a channel when a member leaves; placeholders `{user}` `{server}` `{members}` (Manage Server) |
| `/config log [channel] [off]` | Server log: compact embeds with the member, their avatar and the time for joins and leaves, nickname and username changes, bans and unbans, and voice joins, leaves and moves (Manage Server) |
| `/config log-events [members] [names] [bans] [voice]` | Switch kinds of log entries on or off; all start on, so picking a log channel is enough (Manage Server) |
| `/roles panel title [description]` | Start a panel of role buttons (a draft); the same title again only changes the description (Manage Server) |
| `/roles add panel role label [emoji]` | Put a role button on a panel (up to 25); adding a role again changes its label and emoji. `panel` is the title or the number (Manage Server) |
| `/roles remove panel role`, `/roles post panel channel` | Take a button off, or post the panel; posting again in the same channel updates the message in place (Manage Server) |
| `/points show [user]` | Server points balance and rank; the server can rename the points and pick their emoji |
| `/points give\|take user amount` | Adjust a member's server points (Manage Server) |
| `/config points [enabled] [name] [emoji] [min] [max]` | Turn server points on or off, rename them, and set what a message earns (Manage Server) |
| `/store` | The server's role store: pick a role from the menu to buy it with server points |
| `/config store add\|remove\|list` | Put roles on sale for points, with optional stock (up to 25); adding a role again updates its price and stock (Manage Server) |
| `/giveaway start duration winners prize [shells] [channel] [required_role]` | Post a giveaway with an **Enter** button (press again to leave): `duration` like `30m`, `2h`, `1d12h` (1 minute to 30 days), 1 to 20 `winners`, an optional `shells` prize **per winner**, a `channel` (default: here) and a `required_role`. Up to 20 run at once per server (Manage Server) |
| `/giveaway end id`, `/giveaway reroll id [winners]`, `/giveaway list` | End one now and pick its winners; pick new winners from the entrants who have not won yet (a reroll never pays shells); list the running ones with entries and links. `id` is the number in the giveaway's footer (Manage Server) |
| `/config suggestions [channel] [off]` | Where `/suggest` posts; with neither option it shows the setting, and `/config show` lists it (Manage Server) |
| `/config critters [enabled] [channel] [anywhere]` | Let critter drops and bottles wash up while people chat (off until you turn it on): with a `channel`, only chat there counts and the drops land there; with `anywhere`, they land in whichever channel tipped the count. With none of the options it just shows the setting, and `/config show` lists it (Manage Server) |
| `/suggest text` | Post a suggestion (up to 1000 characters) in the server's suggestion channel as an embed with 👍 and 👎 buttons showing live counts. One vote per member: press the other side to switch, the same side again to take the vote back. One suggestion per 30 seconds |
| `/suggestion approve\|deny id [reason]` | Decide on the server's suggestion number `id` (the `#n` in its title): the embed turns green or red with who decided and why, its buttons lock with the final counts, and the author gets a direct message (closed DMs are reported to you, not an error). Decided once (Manage Server) |
| `/poll question options [duration]` | A question with two to five answers separated by `\|`, up to 80 characters each; `duration` like `30m`, `6h`, `1d` (default 1 day, 1 minute to 14 days). One button per answer plus **Results**; one vote per member, the same switch/take-back rule as suggestions. Votes stay hidden while it runs, **Results** shows a private pixel bar-chart card (drawn at most every 10 seconds per poll), and when it closes the channel gets the final card and a line saying who won (or the tie). Three open polls per member |
| `/tag name [args]` | Run one of the server's custom commands (see "Custom commands and auto-responders" below). `name` suggests the server's commands as you type, by name and description; `args` (up to 200 characters) are what the command can use as `{args}` and `{1}` to `{5}`. A command can have a cooldown per member, allowed and denied roles and channels, and answer privately; what the people who ran it typed can never ping anyone |
| `/customcommand create name [description] [private]` | Write a command: a form asks for its reply (up to 2,000 characters, with placeholders), an optional description and cooldown. Names are 1 to 32 characters of `a-z 0-9 - _` and cannot be one of Pinchora's own commands; 50 per server (Manage Server) |
| `/customcommand edit name [private]`, `delete name`, `list`, `info name` | Change a command's first reply, description and cooldown in the same form (and whether it answers privately), remove it, list them with their use counts (paged), or see how one is set up. Embeds, several replies, actions and role or channel limits are on the dashboard (Manage Server) |
| `/ping`, `/help` | Health and command list |

Chatting earns 15–25 XP and, by default, 1–3 server points per minute per server. Level roles are
given on level-up, and the bot needs the Manage Roles permission with its role above the roles it
hands out; roles the bot can't give (@everyone, integration-managed roles, roles above the bot's own)
are refused when they are added to the level roles, the store, the join role or a role panel. If
Discord refuses a store role at purchase time, the points and stock are returned. New members get a
welcome card when enabled.

Voice XP keeps who is in voice in Redis, written from the voice-state events, and one ticker on the
gateway pays every guild once a minute behind a per-minute lock, so a restart or a second gateway pod
never pays a minute twice. Servers with voice XP off are not tracked at all (the ticker drops one the
moment it finds it switched off). Discord sends each server's voice states once per session in
GUILD_CREATE, so on every identify the bot replaces what it remembered of a server with that snapshot
(and forgets a server on GUILD_DELETE): a leave missed while the bot was away stops paying, and members
already in voice at startup are paid at once. A member the bot has not heard from for a day is still
dropped, in case a leave event was lost. The snapshot's member list gives the roles (a voice
multiplier by role uses the roles the member had at their last voice event or the snapshot) and the
bot flag; a voice state it has no member for waits for the next event. GUILD_CREATE is the one
heavyweight event the gateway deserializes, for this. The ticker reads each server's settings and
multipliers from a five-minute cache, pays a few servers at a time, pays every minute in a task of
its own (one that was late catches up on the last few), and skips a server for the minute when the AFK
channel cannot be looked up and was never known (it uses the last known one otherwise). Multipliers
look up a channel's parent and category from Discord once and remember them for ten minutes. Weekly XP
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

Admin commands check the permission themselves. `/config` (every subcommand), `/roles`, `/giveaway`,
`/suggestion`, `/points give|take` and `/plaza edit` (with the buttons and menus of its editor) need
Manage Server or Administrator, read from the member permissions Discord sends with each interaction,
on top of the command's default visibility: a server owner can change who sees a command in the
integration settings, so hiding or showing one never decides who may use it. A member without the
permission gets a private refusal and nothing changes.

### The pixel world

| Command | What it does |
|---|---|
| `/character` | Design your pixel character in one private editor with a big live preview on a floor tile: a **category** menu (Look, Skin, Hair, Eyes, Top, Bottom, Shoes, Accessories, Presets, Outfits), **◀ ▶** that step through the open category one click at a time, a menu that jumps straight to a choice, **Undo** (20 steps), **Randomise** (only from what you own) and **Save** / **Cancel**: everything is a draft until Save. 24 looks; six colour channels with 20 swatches each (eyes and shoes start as drawn) plus **Hex…**, a form for your own colour, nudged lighter or darker when the shading would not show; 31 **accessories** in five slots (head: straw sun hat, beanie, bow, sailor cap, flower crown, top hat, witch hat, captain's hat, crown; face: glasses, freckles, rosy cheeks, sunglasses, eyepatch, heart shades; neck: scarf, bandana, bow tie, shell and pearl necklaces; back: backpack, cape, surfboard, fairy and bat wings; hand: ice cream, balloon, flower, lantern, star wand, fishing rod), tried on freely, some free, some bought with shells (**Buy**), some opened by a level or a badge, most dyeable in 16 colours (**Dye ▶**); 10 ready-made **presets** (Beach Surfer, Ship Captain, Coral Witch…) and three **outfit** slots to keep looks in (**Keep in 1-3**, **Rename…**) and put back on. Save refuses an accessory that is not yours yet. The character, accessories included, stands in your house, joins you on visits and is on your profile, the partner card and the hug, pat, wave and high-five GIFs |
| `/house view [room]` | Your house, rendered tile by tile; your active pet wanders around; says when your guestbook has new notes |
| `/house edit [room] [owner]` | Decorate inside Discord: pick furniture, move the cursor with the ↖↗↙↘ arrows (on the floor or either wall), place, turn, flip, pick up. **↶ Undo** reverses the last 10 changes to the room (place, move, store, turn, flip, clear), even after reopening the editor, until the room is changed some other way; **Clear the room…** (first in the Go to menu, with a confirm step) stores every piece at once and is one undo step. With `owner` set to your partner you decorate their house: the pieces come from your own inventory, a piece you place belongs to the house's owner from then on (the editor says so), and anything stored goes to their inventory; moving their pieces is theirs to do, and your undo history is separate from theirs |
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
| `/partner propose user` | Ask someone to be your partner; they get **Accept** and **Decline** buttons for 10 minutes. You both must be free: a player has one partner at a time |
| `/partner view [user]` | The pair's picture with both characters, together since, days together, hugs, pats, waves and high fives they sent each other, and their houses' likes and pieces |
| `/partner leave` | End the partnership, after a confirm step |
| `/friends add user` | Ask someone to be your friend (**Accept** or **Decline**, good for a day); up to 100 friends each |
| `/friends remove user`, `/friends list [user]` | Remove a friend, or page through a player's friends with their level and house likes (10 a page) |
| `/hug user`, `/pat user`, `/wave user`, `/highfive user` | A looping GIF (14 frames, about 120 KB) of your two pixel characters facing each other, drawn from the looks, colours and accessories you both chose (hats stay on through every hop and lean; a held item is in the hand the animation leaves alone) (a player without a character gets a stand-in made from their id): they step together with hearts, a hand bobs on a head, an arm waves, palms meet in a flash. "That's hug #12 from you to them", with the sender's and receiver's totals; the receiver gets a **Hug back** button. One every 5 seconds per player |
| `/pet adopt\|view\|capsule\|rename` | Pets with needs that drift over time; feed, play, bathe, walk (a walk finds shells, more at a higher level); 42 species. Every care action, walk and trip earns XP: levels 1-30, a bar on the pet card, level-ups (and the trips they open) announced in the reply. A pet whose hunger, fun and hygiene all sit at zero for 24 hours falls asleep: its card shows it asleep, it earns no XP and cannot be looked after or sent away until **Wake up** (one free a day, else 75 shells), and in the house it lies on its side with a small "z z" above it instead of wandering. A care write is checked against the pet as it was read, so a click that races the start of a trip, a souvenir claim or another click is refused instead of unfreezing the needs or undoing the XP (the food is refunded). Never permanent, never lost. Unknown button actions answer with an error instead of bathing the pet |
| `/pet list` | The nursery: one card with every pet in a grid (portrait, name, level and XP bar, the four needs as icons, a star on the active pet, ASLEEP / AWAY / BACK! badges), 8 to a page (the card is only as tall as its rows: 1-4 pets get a half-height card), with a menu that opens one pet's card (care buttons, **Make active**, a way back to its page). A **Nursery** click on a public pet card opens the nursery as a private message of its own |
| `/pet trip go destination [pet]`, `/pet trip claim` | Field trips: send a pet (default: your active one) for 1, 4 or 8 hours to one of 8 cosy places from `content/pet_trips.json` (a place opens at a pet level, 1 to 26; the card's **Trip** button lists them). The pet leaves the house, its needs wait for it and it cannot be looked after meanwhile. Back, **Claim souvenir** (or `/pet trip claim`, which collects every pet that is home and otherwise says who is still away) pays shells, XP and a short story, and sometimes a free-to-earn furniture piece (`reward` items only, weighted by rarity, never shop stock). Higher levels find more shells and rarer pieces. A trip is a stored end time, so it survives restarts, and a claim pays exactly once |
| `/fish cast [spot]` | Cast, wait for the bite (a random 2 to 11 s), press **Reel!** in time; pressing early scares the fish away. Five spots open by global level (Shore 1, Reef 5, Kelp Forest 12, Deep Sea 20, Abyss 30) and the last one you fished is remembered; each has its own fish. A cast first rolls the kind of catch (junk, common, uncommon, rare, epic, legendary) with fixed shares per spot that the hour and the season never change, then picks among the fish of that kind that are about now, so the wage is the same at dawn, noon and midnight, in every month; time and season only choose which species of the kind bites. Every catch rolls a size in cm (normal around the species' usual size), is added to the fish book (72 species, with a record size each) and goes into your **bucket**, which holds 60 fish and stops the next cast until you sell some. A plain cast costs 10 shells of bait; a better bait on the hook is used up instead. The reply tells of a new species, a new record, a big one for the week, and any book milestone |
| `/fish shop` | Private tackle shop on a card: five rods (free starter, then 800, 3,500, 12,000 and 40,000 shells; each gives a longer Reel! window, 2.5 s up to 3.5 s, and a bigger chance at rare, epic and legendary fish) and three baits in packs of five (Glow Worm for uncommon fish, 65 shells; Silver Lure for rare ones, 140; Tide Crumbs for the spot's signature fish, 120) |
| `/fish gear` | Private panel to pick the spot (closed ones show a lock and their level), the rod you own and the bait on the hook; a bait that runs out hands the hook back to plain bait |
| `/fish sell` | Your bucket on a card, with **Sell all** and **Sell duplicates** (keeps the biggest of each species), priced by rarity and size (fish of a species the game no longer has are not counted and **Sell all** clears them for nothing). The fish book never loses an entry or a record |
| `/fish book` | Your fish book on a card, eight species a page: record size and catch count for the ones you have, a shadow and where they live for the ones you have not, and the milestone rewards: **10 / 25 / 50 / 72** species pay 300 / 1,000 / 3,000 / 10,000 shells and a badge or profile background (Pocket Puffer, Brinebell Market, Lantern Jelly, Moonwhale Crossing). Each pays once; players who passed a milestone earlier are paid at their next catch |
| `/beachcomb` | Walk the shore (once every 20 minutes) for a find: shells, sea glass, driftwood, coins, now and then a critter; some only turn up at a time of day or in a season. A first find is a discovery for your dex and pays well, a spare is recycled for a few shells and fragments. A card shows what you found |
| `/critterdrop` | Drop three pixel critters in the channel on one picture with a numbered button each: the first click takes a card, nobody takes two, they drift away after 90 seconds. It follows `/config critters` (off in a server until a manager turns it on, and kept to the set channel if there is one). You can call one every 30 minutes, a channel gets one every 10 minutes whoever calls, and one waits at a time per channel; chat can make them wash up too |
| `/crabpot` | A crab pot you set for 1, 4 or 8 hours and haul in later (a stored end time, so it survives restarts): 1 to 1.8 catches an hour, more for a longer soak, drawn from the critters of the shallows and the finds. Five levels, bought with shells: a better pot catches more and finds rarer things. The haul is a card of everything it brought |
| `/dex [user] [tab]` | The Tide Pool Dex: a paged picture book of all 48 critters and 36 finds with a progress bar, how many you met of each, and the ones you have not found as dark silhouettes. **Spend fragments** trades what spares leave behind for a free-to-earn decor piece of the tier you pick (12 / 65 / 370 / 1,950 / 8,900 fragments from common to legendary); the piece is **soulbound**: it can be placed in your own house but never sold at a stall, gifted, donated to a plaza or placed in a partner's house |
| `/quests` | Three daily quests; clear all three for pearls |
| `/duel tictactoe user` | Button tic-tac-toe; the winner earns shells |
| `/duel connect4 user [bet]` | Connect four on a pixel-art board: the challenged player accepts, then each turn is a column button (1-7) within 90 s; the winner earns shells |
| `/duel rps user [rounds] [bet]` | Rock paper scissors, best of 1, 3 or 5: each player gets private Rock / Paper / Scissors buttons, the channel sees each round revealed; ties replay |
| `/duel share user [bet]` | Share or Grab: the challenged player accepts, then each of you picks **Share** or **Grab** in secret (private buttons, two minutes). Both share: the pool is split. One grabs: they take it all. Both grab: nobody gets anything. The pool is the sea's 60 shells without a wager, or the two stakes with one (shares give the stakes back, a grab takes the pot, two grabs sink it: **in a wagered game both players grabbing loses both stakes, that is the game**); a player who never picks forfeits. The sea pays a pair once an hour, and the hour starts when a payout has gone through |
| `/duel seafleet user [bet]` | **Sea Fleet**, battleship on a 10×10 sea. After the challenge is accepted each captain places five ships in private (**Shuffle** draws a layout from every legal one with the bot's secure random source, as often as you like, then **Lock in fleet**; there are no fixed layouts to learn, and the layout you are looking at stays on the server, never in a button or a picture; fleets are never shown to the other captain). Then they take turns: pick a column and a row from two menus under the shared radar, which shows both players' waters, every hit and miss and the ships that have gone down. **My fleet** is your own waters, privately. 5 minutes to place, 2 minutes a shot |
| `/duel chess user [bet]` | **Reef Chess** with a rules engine written for the bot (castling, en passant, promotion, check, checkmate, stalemate, fifty-move rule, threefold repetition, insufficient material; checked against the standard perft counts). Pieces are pixel sea creatures (pearl, seahorse, conch, lighthouse, octopus queen, crab king); a move is two picks from menus under the board, the piece then where it goes (promotions list all four pieces, the queen first). Coral or sky is drawn at random; 5 minutes a move |
| `/duel checkers user [bet]` | **Pearl Checkers**, English draughts with pearls: compulsory jumps, multiple jumps, kings, a draw after 40 quiet moves each. The same two menus (piece, then move, where a jump chain is one entry); 3 minutes a move |
| `/play picross [mode]` | **Pixel Picross**: a 10×10 nonogram of a sea creature (26 pictures, each unique-solution checked, each also mirrored sideways, never upside down). `daily` (default) is the same puzzle for everyone each day, with a streak: 35 shells, plus 5 for each day of the streak up to a week; `practice` is a random puzzle (10 shells, 5 a day) from the pictures the daily will not show for a week. Pick a row and tick its filled cells from a menu, or **Mark cells** and type `A1 B3-B7 -C2` (a dash clears); **Check** says how many marks are wrong, never which, three times a puzzle, and a checked puzzle pays less (daily a flat 15 and no streak bonus, practice 5). The solved picture is revealed in colour, with a spoiler-free **Share result** |
| `/play 2048` | **Tide 2048** with a fish from the atlas on every tile (shrimp up to a leviathan at 2048): four arrow buttons, the game is kept between sessions, **Finish** counts it. The best score shows in `/play stats`; a finished game pays 5 to 60 shells for a tile of 256 or more (5 paid games a day) |
| `/play hangman` | A cozy word (780 in five categories) guessed from two letter menus while the tide washes a pixel sandcastle away; six misses lose |
| `/play wordle [mode]` | Five letters, six tries, typed into a form, shown as private pixel tiles with a keyboard summary. `daily` (default) is one word the same for everyone, kept per player per day, with a spoiler-free **Share result**; `practice` is a random word, as many as you like |
| `/play trivia [category]` | A question for the channel from 464 (8 topics, three difficulties): four answer buttons, 20 seconds, anyone answers once; the message then reveals the answer and who got it right |
| `/play derby [bet]` | Hermit Crab Derby: a lobby for the channel with six crabs (the hermit crab pet in six shell colours); everyone picks a crab with a button (and can switch or **Leave** until the start) and **Start the race** (host) or the 75 s timer runs it as an animated GIF of 480x288 under 600 KB that plays once (the lane plates show each crab's number and name, and the finished crabs fan out by place). With a `bet` (10 to 500) each racer puts that many shells on their crab and those who back the winner share the pool; nobody backing it, or fewer than two racers, gives every stake back, so the house never takes a shell. A staked race is pure chance, so it is a gambling-style game: `bet` is refused until a manager runs `/config gambling`, and nobody can be more than 1,000 shells down on a day's staked races (a stake held in a lobby counts as down until it comes back, counted on the day it was put up). Without a bet the backers of the winner get 20 shells (five wins a day). The result is hidden in a spoiler under the GIF |
| `/play heist` | Treasure Heist: a crew of 2 to 8 picks roles (Lookout, Locksmith, Diver) in a lobby; three stages (slip past the guards, crack the lock, haul out the loot) each go better with a fitting role, and the card shows the crew's live chances. All three stages pay each member 60 shells, two pay 25, one pays 8, none is busted. Nobody ever loses shells; at most 150 shells a day and a 5 minute rest after a heist |
| `/play shells bet` | Shell Game: the pearl is under one of three shells, a right guess pays two and a half times the stake (5 to 100), which is a sixth less than fair. A gambling-style game, so it is off until a manager runs `/config gambling`, with 20 games, a 300 shell loss and a 600 shell win limit per player a day; an unanswered table gives the stake back |
| `/surge` | Managers call a Storm Surge: for 10 minutes everyone presses **Stack a sandbag** (one every 6 s, at most 25 each) while the sea washes one away every 30 s; the wall's goal is 24 plus 8 per helper, so one player cannot hold it alone. A shared card shows the wall, the helpers and the best three; hold it and every helper gets 20 shells plus 2 per sandbag (up to 50), lose and they get 6 plus 1 per sandbag; at most 120 shells a day |
| `/play stats [user]` | Games played and won for each game |
| `/config gambling enabled` | Switch gambling-style games (the shell game and staked derbies) on or off for the server; off by default (Manage Server) |
| `/config surge [enabled] [channel] [every]` | Timed storm surges, off by default: one comes to the channel every 6, 12 or 24 hours, 2 days or a week; with no options it shows the setting. `/surge` always works for managers (Manage Server) |
| `/party hotpearl [lives]` | Hot Pearl, 2 to 8 players: a lobby message with **Join**, **Leave**, **Start** (host only) and **Cancel** (closed by the clock after 3 minutes). Each turn the pearl shows a syllable (from 2 and 3 letter fragments, gentle at first, harder later) and its holder presses **Type a word** and types a real word that contains it (176,000 words, plus the usual plurals, `ed`, `ing`, `er`, `est`, `ly` forms; a word can only be played once) before the pearl pops: 25 seconds, shrinking to 15. A pop costs a life (2 each, 1 to 3 with `lives`), a word passes the pearl on with a new syllable, and the last player with lives wins; a click that arrives after the clock pops the pearl itself. A win only pays shells when at least 4 words were played in the game and 2 of them by the winner, so waiting out an empty table earns nothing. The table is a picture with hearts, whose turn it is and the last word |
| `/party acronym [rounds]` | Acronym Tide, 3 to 10 players, 1 to 5 rounds: 3, 4 or 5 letters come up and everyone writes a phrase with one word per letter, in order (75 s, in a form; a rewrite replaces the first). The phrases go up anonymously and numbered, everybody votes with a number button (not for their own; the vote can be changed; 40 s), then the reveal shows who wrote what and the votes (1 point a vote, 2 more for the round's winner, a tie shares it). An early finish (everyone wrote, everyone voted) moves a phase on sooner; fewer than two phrases skip the vote |
| `/party postcard [rounds]` | Caption the Postcard, 3 to 10 players, 1 to 5 rounds: a pixel postcard (one of 11 scenes: a profile backdrop with pets, fish and characters standing on it at the backdrop's own pixel size, stamped and postmarked) comes up, everyone writes a caption (60 s), then the same anonymous vote and reveal as Acronym Tide with the postcard above the captions |
| `/party wordrace` | Wordle Race, 2 to 4 players: one secret word for all, six guesses each typed into a form. A guess comes back to you alone as your private board with the keyboard; the channel sees a progress card with everyone's colours and no letters. The first to solve it wins and the word is shown; seven minutes without a winner ends the race too |
| `/party wouldyourrather [topic]` | A would-you-rather question from 212 (six topics: silly, food, adventure, ocean, brainy, cozy) with a card and an **A** and a **B** button: one vote each, changeable, the split hidden until the poll closes after 10 minutes, when the card shows the percentages. One poll every 20 seconds per channel |
| `/party truthordare [topic]` | Truth or Dare, 2 to 10 players, family friendly (102 truths and 102 dares in six topics): the bot picks whose turn it is, that player presses **Truth** or **Dare**, gets one nobody had lately, and presses **Done** or **Pass** (passing is always fine). The host ends the game with **End game**; a table that lets two turns in a row run out of time ends it by itself. Each player gets one game in `/play stats` when it ends |
| `/party stop` | Ends the lobby or game running in this channel: its host or a member with Manage Server can. The message is closed, nobody wins and nothing is paid or refunded (the party games have no wagers); the answer is private |
| `/config partygames [strict]` | How strict the filter on what members type is (Manage Server). The base filter always blocks strong words and slurs (it reads whole words, undoes disguises like `sh!t`, `f u c k`, `f*ck`, `s1ut` and common misspellings, and lets names like Matsushita through); `strict:true` adds mild words and name-calling. Mentions, links, invites and formatting are always taken out of typed text |
| `/config modules show\|toggle\|channels` | Switch parts of Pinchora off for the whole server (`toggle module enabled`) or per channel (`channels module where [channel] [remove]`: `where` is all channels, only these, or everywhere except these, and the list grows one channel at a time; a category covers the channels and threads under it; up to 50 channels). A switched-off module answers its commands, buttons and forms with a private "This feature is turned off on this server" (or "in this channel"), stops its chat XP, points, critter drops, goodbyes, log lines or timed storms, and `/config`, `/help` and `/ping` are never switched off. The 27 modules are listed with `show`, and `module` suggests them as you type (start with part of a name, or of a game such as `chess`); leveling, points, welcome and gambling keep their switch where it already was (`/config leveling`, `points`, `welcome`, `gambling`). The dashboard's Modules page does the same (Manage Server) |
| `/config welcome-card` | Design the welcome card (theme, headline, accent colour) with a live preview |

Duel wagers (`bet`, 10 to 500 shells each) are optional and off by default. Nothing is held until the
challenged player accepts; then both bets leave the balances in one transaction and sit in the database
row of the game. The winner takes both; a draw, a decline, a no-show or a timeout gives them back, and
whoever ends the game first pays out, exactly once, so a restart cannot lose or double shells (a janitor on
every pod ends games whose clock ran out every 10 seconds). In Sea Fleet, Reef Chess and Pearl Checkers a
wagered game is stricter: a player who runs out of time *during the game* forfeits the pot to the other
(both slow while placing fleets gives the bets back), settled only if nobody has moved since the janitor
looked; resigning works the same way, behind a confirmation. Without a wager the system pays small rewards
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

## Quick start (Docker)

1. Create an application at <https://discord.com/developers/applications>, add a bot, and turn
   on **Server Members Intent** (for welcome cards, join roles, goodbye messages and the member, nickname and
   username entries of the server log; or set `PINCHORA_GATEWAY__MEMBER_EVENTS=false`). The server log's ban and voice
   entries use the non-privileged Guild Moderation and Guild Voice States intents. Keyword auto-responders
   need the privileged **Message Content Intent** as well: turn it on in the Developer Portal and set
   `PINCHORA_GATEWAY__MESSAGE_CONTENT=true` (Discord asks for verification above 100 servers); without it the
   dashboard shows keyword triggers as locked and the bot never reads chat text.
2. `cp .env.example .env` and fill in the token and application id.
3. Start everything:

   ```bash
   docker compose up -d
   ```

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
| Overview | Chat XP on/off, every page's state at a glance, every module's state, the latest changes | `/config show`, `/config leveling enabled`, `/config modules show` |
| Modules | Each part of Pinchora (leveling, points, welcome, goodbye, log, role panels, store, suggestions, polls, giveaways, custom commands, profiles, economy, shops, quests, social, houses, plaza, pets, fishing, duels, board and puzzle games, word and trivia games, party games, word party games, critter drops, gambling) on or off, and where it works: all channels, only some, or all but some | `/config modules` |
| Welcome & goodbye | Welcome cards (channel, text with placeholder chips), the card designer (theme, headline, accent, live preview drawn by the bot), join role, goodbye channel and text | `/config welcome`, `welcome-card`, `join-role`, `goodbye` |
| Leveling | Leveling, where level-ups go, level-up channel and text, voice XP, XP multipliers, level roles (stack/replace, **Sync members**), reputation roles | `/config leveling`, `xp`, `level-roles`, `rep-roles` |
| Points & store | Points on/off, name, emoji, points per message; roles on sale with price and stock | `/config points`, `/config store` |
| Server log | Log channel and the four kinds of events | `/config log`, `/config log-events` |
| Custom commands | The commands members run with `/tag`: name, description, one to five replies (one picked at random), an embed, up to three actions (give, take or toggle a role, give or take server points, send a DM), who may use it and where, a cooldown, a live preview drawn by the bot, import and export as a JSON file | `/customcommand` (replies only) |
| Auto-responders | Messages the bot sends by itself when a member joins, leaves, levels up, gets or loses a role, or says a keyword (locked unless the bot has the Message Content intent): where it answers, how often, a cooldown, actions, a preview, import and export | |
| Community | The `/suggest` channel; critter drops on/off and where they land; timed storm surges (on/off, channel, how often) | `/config suggestions`, `/config critters`, `/config surge` |
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
the simple case, with `/customcommand`. Both go through the same rules (`pinchora_core::tags`: limits, the
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
- **Modules**: the *Custom commands* module covers `/tag`, `/customcommand` and the responders, and can be kept
  to channels like any other.
- **Export and import**: a JSON file (`{"version": 1, "items": [...]}`) per list. An import checks every item like
  a new one, leaves out roles and channels the other server lacks (and role actions the importer may not set up),
  skips (or with "replace" overwrites) names that exist, and reports what it refused. The audit entry of a
  replacing import keeps the commands it overwrote.

The dashboard and the slash commands check values with the same rules (`pinchora_core::settings`): the same
limits and the same sentences, and a role the bot cannot hand out is greyed out with the reason `/config`
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

## Development

```bash
cargo run -p pinchora -- gateway          # needs PINCHORA_* env vars or pinchora.toml
cargo run -p pinchora -- web              # the web dashboard on :8090 (see "Web dashboard")
cargo run -p pinchora -- commands         # print slash command JSON
cargo run -p pinchora-render --example samples   # render sample cards to target/render-samples
                                                 # (profile-background/-badges/-placeholder, rank-background, wallet-background show /customize;
                                                 #  the profile and rank samples carry a title; profile-showcase-*, profile-bg-<white|black|stripes|beach>-<opacity>,
                                                 #  profile-nobg-*, profile-unicode/-wide/-cyrillic/-shout and profile-frame-* are the stress variants)
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

## Layout

```
crates/
  pinchora/           binary: gateway | interactions | web | migrate | register | commands
  pinchora-core/      config, telemetry, level/economy/cluster math, settings, module and custom-command rules
  pinchora-db/        PostgreSQL (sqlx) + migrations runner
  pinchora-cache/     Redis: cooldowns, gateway sessions, cross-pod identify queue, invalidation pub/sub
  pinchora-render/    pixel-art cards (SVG → PNG with resvg)
  pinchora-commands/  slash commands, buttons, gateway event handlers
  pinchora-web/       web dashboard API (axum): Discord OAuth, sessions, CSRF, server settings, modules, the SPA
web/                  dashboard UI (Vite + React + TypeScript), embedded into the binary
migrations/           SQL migrations
assets/fonts/         Silkscreen + Noto Sans (SIL OFL)
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
