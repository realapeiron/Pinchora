//! Text commands (`p!profile`, `@Pinchora fish cast`) through the real handlers: the same answers,
//! buttons and refusals as the slash commands, posted as replies that ping nobody. Pictures land in
//! `target/sim/<test>/`.

use std::time::Duration;

use pinchora_commands::{PermissionChange, on_permissions_changed, on_text_command};

use super::*;

/// What a server overrides for one command: (target id, 1 role / 2 user / 3 channel, allowed).
type Overrides<'a> = &'a [(u64, u8, bool)];

/// What `@Pinchora` looks like in a message.
const BOT_MENTION: &str = "<@1>";
/// Administrator and Use Application Commands, as the permission bits Discord sends.
const ADMINISTRATOR: u64 = 1 << 3;
const USE_APPLICATION_COMMANDS: u64 = 1 << 31;

impl Sim {
    /// Adds a role with `permissions` to the mock server and returns its id. Add it before the
    /// server's first text command: the bot keeps the role list for a minute.
    fn permission_role(&self, permissions: u64) -> u64 {
        let id = unique();
        let mut role = role_json(id, "Staff", 1, false);
        role["permissions"] = json!(permissions.to_string());
        self.mock.roles.lock().unwrap().push(role);
        id
    }

    /// Tells the mock about a channel with permission overwrites, each as (id, is a member, allow, deny).
    fn channel_with(&self, id: u64, kind: u8, parent: Option<u64>, overwrites: &[(u64, bool, u64, u64)]) {
        let overwrites: Vec<Value> = overwrites
            .iter()
            .map(|(target, member, allow, deny)| {
                json!({
                    "id": target.to_string(),
                    "type": u8::from(*member),
                    "allow": allow.to_string(),
                    "deny": deny.to_string(),
                })
            })
            .collect();
        let channel = json!({
            "id": id.to_string(),
            "type": kind,
            "guild_id": self.guild.to_string(),
            "name": "sim-channel",
            "parent_id": parent.map(|p| p.to_string()),
            "permission_overwrites": overwrites,
        });
        self.mock.channels.lock().unwrap().insert(id, channel);
    }

    /// Tells the mock which commands the bot has, as (id, name).
    fn registered(&self, commands: &[(u64, &str)]) {
        *self.mock.global_commands.lock().unwrap() = commands
            .iter()
            .map(|(id, name)| {
                json!({
                    "id": id.to_string(),
                    "application_id": APPLICATION_ID.to_string(),
                    "version": "1",
                    "name": name,
                    "description": "a command",
                    "type": 1,
                    "options": [],
                })
            })
            .collect();
    }

    /// Sets what the server says under Integrations: the entries are (command id or the application's,
    /// then each override as (target id, 1 role / 2 user / 3 channel, allowed)).
    fn integrations(&self, entries: &[(u64, Overrides)]) {
        let list: Vec<Value> = entries
            .iter()
            .map(|(id, overrides)| {
                json!({
                    "id": id.to_string(),
                    "application_id": APPLICATION_ID.to_string(),
                    "guild_id": self.guild.to_string(),
                    "permissions": overrides
                        .iter()
                        .map(|(target, kind, allowed)| {
                            json!({ "id": target.to_string(), "type": kind, "permission": allowed })
                        })
                        .collect::<Vec<_>>(),
                })
            })
            .collect();
        *self.mock.command_permissions.lock().unwrap() = Some(Value::Array(list));
    }

    /// How often the bot asked Discord for the server's Integrations settings.
    fn integration_lookups(&self) -> usize {
        self.mock.command_permission_lookups.load(Ordering::SeqCst)
    }

    /// `user`'s chat message saying `content`, with the roles they hold and the members it mentions.
    fn text_message(&self, user: u64, roles: &[u64], content: &str, mentions: &[u64]) -> MessageCreate {
        let mentions: Vec<Value> = mentions
            .iter()
            .map(|id| {
                let mut mention = user_json(*id);
                mention["public_flags"] = json!(0);
                mention["member"] = json!({
                    "roles": [],
                    "joined_at": "2024-01-01T00:00:00.000000+00:00",
                    "deaf": false,
                    "mute": false,
                    "flags": 0,
                });
                mention
            })
            .collect();
        let message = json!({
            "id": unique().to_string(),
            "channel_id": self.channel.to_string(),
            "guild_id": self.guild.to_string(),
            "type": 0,
            "author": user_json(user),
            "member": {
                "roles": roles.iter().map(u64::to_string).collect::<Vec<_>>(),
                "joined_at": "2024-01-01T00:00:00.000000+00:00",
                "deaf": false,
                "mute": false,
                "flags": 0,
            },
            "content": content,
            "timestamp": "2024-01-01T00:00:00.000000+00:00",
            "edited_timestamp": null,
            "tts": false,
            "mention_everyone": false,
            "mention_roles": [],
            "mentions": mentions,
            "pinned": false,
            "attachments": [],
            "embeds": [],
            "flags": 0,
        });
        serde_json::from_value(message).expect("message JSON")
    }

    /// Sends `content` as `user`'s message and runs it as a text command to the end, past the rate
    /// limit. Gives back whether it was one and the replies it posted, as messages a member can
    /// click on. Every reply must answer the member's message and ping nobody but whom the handler
    /// names.
    async fn text(&mut self, user: u64, roles: &[u64], content: &str, mentions: &[u64]) -> (bool, Vec<Reply>) {
        self.app
            .cache
            .release_cooldown(&format!("text:{user}"))
            .await
            .expect("redis");
        self.send(user, roles, content, mentions).await
    }

    /// [`Sim::text`] within the rate limit.
    async fn send(&mut self, user: u64, roles: &[u64], content: &str, mentions: &[u64]) -> (bool, Vec<Reply>) {
        let (ran, replies, errors) = self.send_logged(user, roles, content, mentions).await;
        assert!(
            errors.is_empty(),
            "step {}: ({content}): failed:\n{}",
            self.step,
            errors.join("\n")
        );
        (ran, replies)
    }

    /// [`Sim::send`] that hands back the errors the bot logged instead of failing on them.
    async fn send_logged(
        &mut self,
        user: u64,
        roles: &[u64],
        content: &str,
        mentions: &[u64],
    ) -> (bool, Vec<Reply>, Vec<String>) {
        self.step += 1;
        let step = self.step;
        // Like Discord's, the server's role list has `@everyone`; the channel is a plain text channel
        // unless the test said otherwise.
        self.mock.everyone_role.store(true, Ordering::SeqCst);
        let (guild, channel) = (self.guild, self.channel);
        self.mock.channels.lock().unwrap().entry(channel).or_insert_with(|| {
            json!({
                "id": channel.to_string(),
                "type": 0,
                "guild_id": guild.to_string(),
                "name": "sim-channel",
                "parent_id": null,
                "permission_overwrites": [],
            })
        });
        let message = self.text_message(user, roles, content, mentions);
        self.mock.posts.lock().unwrap().clear();
        ERRORS.with_borrow_mut(Vec::clear);
        let ran = on_text_command(self.app.clone(), &message).await;
        let errors = ERRORS.take();
        println!("\n== {} step {step:02}: text {content:?}", self.name);
        let mut replies = Vec::new();
        for post in self.posts() {
            // An earlier reply edited later (`/ping` shows its round trip that way).
            if post.method == Method::PATCH {
                continue;
            }
            assert_eq!(post.channel, self.channel);
            assert_eq!(
                post.body["message_reference"]["message_id"],
                json!(message.id.to_string()),
                "step {step}: not a reply to the member's message"
            );
            let mentions = &post.body["allowed_mentions"];
            assert!(
                mentions["replied_user"] != json!(true),
                "step {step}: the reply pings the member"
            );
            assert_eq!(
                mentions["parse"],
                json!([]),
                "step {step}: the reply may ping anyone it names"
            );
            let mut reply = Reply::new(InteractionResponseType::ChannelMessageWithSource);
            reply.message_id = post.message;
            reply.edit(
                post.body["content"].as_str().map(str::to_owned),
                serde_json::from_value(post.body["embeds"].clone()).ok(),
                serde_json::from_value(post.body["components"].clone()).ok(),
            );
            reply.mentions = Some(mentions.clone());
            reply.files = post.files.clone();
            self.report(step, &format!("text {content:?}"), &reply);
            replies.push(reply);
        }
        (ran, replies, errors)
    }

    /// [`Sim::text`] for a command that answers with exactly one reply.
    async fn text_one(&mut self, user: u64, roles: &[u64], content: &str, mentions: &[u64]) -> Reply {
        let (ran, mut replies) = self.text(user, roles, content, mentions).await;
        assert!(ran, "{content:?} was not taken as a command");
        assert_eq!(replies.len(), 1, "{content:?} answered {} times", replies.len());
        replies.remove(0)
    }
}

#[tokio::test]
async fn text_commands_run_the_slash_handlers() {
    let Some(mut sim) = Sim::new("text_commands_run_the_slash_handlers").await else {
        return;
    };
    let player = unique();
    fund(&sim, player, 500).await;

    // The profile card, by prefix in any case and by @mention.
    let profile = sim.text_one(player, &[], "p!profile", &[]).await;
    assert_eq!(profile.files.len(), 1);
    assert_eq!(profile.files[0].0, "profile.png");
    let shouted = sim.text_one(unique(), &[], "P!PROFILE", &[]).await;
    assert_eq!(shouted.files[0].0, "profile.png");
    let mentioned = sim
        .text_one(unique(), &[], &format!("{BOT_MENTION} profile"), &[])
        .await;
    assert_eq!(mentioned.files[0].0, "profile.png");

    // Aliases.
    let balance = sim.text_one(unique(), &[], "p!bal", &[]).await;
    let slash = sim.command(unique(), "balance", json!([])).await;
    assert_eq!(balance.title(), slash.title());

    // A choice by name, and the same cast as the slash command makes.
    let cast = sim.text_one(player, &[], "p!fish cast Shore", &[]).await;
    assert_eq!(cast.title(), "🎣 Line in the water…");
    assert_eq!(shells(&sim, player).await, 490, "the bait was paid once");

    // Not ours, or not a command: nothing is said and nothing runs.
    let (ran, replies) = sim.text(player, &[], "p!notacommand", &[]).await;
    assert!(!ran && replies.is_empty());
    let (ran, replies) = sim.text(player, &[], "hello p!profile", &[]).await;
    assert!(!ran && replies.is_empty());

    // A wrong call gets the usage line.
    let usage = sim.text_one(unique(), &[], "p!duel connect4", &[]).await;
    assert_eq!(
        usage.content(),
        "`user` is missing.\nUsage: `p!duel connect4 <user> [bet]`"
    );
    let usage = sim.text_one(unique(), &[], "p!fish cast moon", &[]).await;
    assert!(
        usage.content().contains("Usage: `p!fish cast [spot]`"),
        "{}",
        usage.content()
    );

    // A private panel only works as a slash command.
    let editor = sim.text_one(unique(), &[], "p!house edit", &[]).await;
    assert_eq!(
        editor.content(),
        "This one only works as a slash command: use `/house edit`."
    );
    assert!(editor.components.is_empty());
    // So does any other private panel, caught as it is answered: nobody else may click it.
    let gear = sim.text_one(unique(), &[], "p!fish gear", &[]).await;
    assert_eq!(
        gear.content(),
        "This one only works as a slash command: use `/fish gear`."
    );
    assert!(gear.components.is_empty());

    // The server's settings stay slash-only: a reply would show them to the whole channel.
    let settings = sim.text_one(unique(), &[], "p!config show", &[]).await;
    assert_eq!(
        settings.content(),
        "This one only works as a slash command: use `/config show`."
    );
    let settings = sim.text_one(unique(), &[], "<@1> config prefix set z!", &[]).await;
    assert_eq!(
        settings.content(),
        "This one only works as a slash command: use `/config prefix set`."
    );

    // Just the mention: how to start.
    let hello = sim.text_one(unique(), &[], BOT_MENTION, &[]).await;
    assert!(hello.content().contains("`p!help`"), "{}", hello.content());

    // Too fast: the second command in a row is ignored.
    let quick = unique();
    sim.text_one(quick, &[], "p!ping", &[]).await;
    let (ran, replies) = sim.send(quick, &[], "p!ping", &[]).await;
    assert!(ran && replies.is_empty(), "rate limited");
}

#[tokio::test]
async fn a_text_duel_is_accepted_with_the_buttons() {
    let Some(mut sim) = Sim::new("a_text_duel_is_accepted_with_the_buttons").await else {
        return;
    };
    let (host, guest, bystander) = (unique(), unique(), unique());
    let challenge = sim
        .text_one(host, &[], &format!("p!duel connect4 <@{guest}>"), &[guest])
        .await;
    assert!(challenge.content().contains("challenges"), "{}", challenge.content());
    // The owner checks go by the member who typed the command.
    let refused = sim.click(host, &challenge, "Accept").await;
    assert!(
        refused.content().contains("You sent this challenge"),
        "{}",
        refused.content()
    );
    let refused = sim.click(bystander, &challenge, "Accept").await;
    assert!(refused.content().contains("isn't for you"), "{}", refused.content());
    let board = sim.click(guest, &challenge, "Accept").await;
    assert_eq!(board.files[0].0, "connect4.png");
    assert!(
        board.content().contains(&format!("<@{host}>'s turn")),
        "{}",
        board.content()
    );

    // The challenge cooldown holds for text commands as for slash commands.
    let again = sim
        .text_one(host, &[], &format!("p!duel connect4 <@{guest}>"), &[guest])
        .await;
    assert!(again.content().starts_with("Easy there!"), "{}", again.content());

    // By name, with a bet, and a raw id for the member.
    let bettor = unique();
    fund(&sim, bettor, 100).await;
    fund(&sim, guest, 100).await;
    let wager = sim
        .text_one(bettor, &[], &format!("p!duel connect4 bet:20 user:{guest}"), &[])
        .await;
    assert!(wager.content().contains("20"), "{}", wager.content());

    // The help guide is posted for the channel, and its pages turn.
    let help = sim.text_one(unique(), &[], "p!h", &[]).await;
    assert_eq!(help.files[0].0, "help.png");
    assert!(help.description().contains("`p!profile`"), "{}", help.description());
    let next = sim.click(unique(), &help, "Next ▶").await;
    assert!(!next.ephemeral, "a page of a posted guide stays posted");
    assert_eq!(next.files[0].0, "help.png");
}

#[tokio::test]
async fn text_commands_keep_permissions_and_modules() {
    let Some(mut sim) = Sim::new("text_commands_keep_permissions_and_modules").await else {
        return;
    };
    let staff = sim.permission_role(MANAGE_GUILD);
    let (manager, member) = (unique(), unique());

    // `/surge` needs Manage Server, as Discord asks of the slash command: by the member's roles.
    let refused = sim.text_one(member, &[], "p!surge", &[]).await;
    assert_eq!(
        refused.content(),
        "You need the Manage Server permission to use `surge`."
    );
    let storm = sim.text_one(manager, &[staff], "p!surge", &[]).await;
    assert!(storm.content().contains("Storm surge"), "{}", storm.content());

    // The prefix is set with the slash command, and the new one works at once; the old one is just chat.
    sim.permissions = MANAGE_GUILD;
    let set = sim
        .command(
            manager,
            "config",
            json!([group("prefix", sub("set", vec![text_option("prefix", "z!")]))]),
        )
        .await;
    assert!(
        set.content().starts_with("Text commands now start with `z!`"),
        "{}",
        set.content()
    );
    sim.permissions = 0;
    let (ran, replies) = sim.text(member, &[], "p!profile", &[]).await;
    assert!(!ran && replies.is_empty());
    let profile = sim.text_one(member, &[], "z!profile", &[]).await;
    assert_eq!(profile.files[0].0, "profile.png");
    let usage = sim.text_one(unique(), &[], "z!duel connect4", &[]).await;
    assert!(
        usage.content().ends_with("`z!duel connect4 <user> [bet]`"),
        "{}",
        usage.content()
    );
    // Even a manager cannot read or change the server's settings from the channel.
    let settings = sim.text_one(manager, &[staff], "z!config show", &[]).await;
    assert_eq!(
        settings.content(),
        "This one only works as a slash command: use `/config show`."
    );

    // The slash command sees the prefix, and refuses what the shared rule refuses.
    sim.permissions = MANAGE_GUILD;
    let shown = sim
        .command(manager, "config", json!([group("prefix", sub("show", vec![]))]))
        .await;
    assert!(
        shown.ephemeral && shown.content().contains("`z!`"),
        "{}",
        shown.content()
    );
    let bad = sim
        .command(
            manager,
            "config",
            json!([group("prefix", sub("set", vec![text_option("prefix", "@x")]))]),
        )
        .await;
    assert_eq!(bad.content(), "A prefix can't hold spaces, @, # or backticks.");
    sim.permissions = 0;

    // A module switched off refuses its text commands with the slash command's words.
    let off = modules(&mut sim, manager, toggle("pets", false), &[]).await;
    assert!(off.content().contains("**Pets**"), "{}", off.content());
    let refused = sim.text_one(member, &[], "z!pet view", &[]).await;
    let slash = sim.command(member, "pet", json!([sub("view", vec![])])).await;
    assert_eq!(refused.content(), slash.content());
    assert_eq!(refused.content(), "This feature is turned off on this server.");

    // Back to the default.
    sim.permissions = MANAGE_GUILD;
    let reset = sim
        .command(manager, "config", json!([group("prefix", sub("reset", vec![]))]))
        .await;
    sim.permissions = 0;
    assert!(reset.content().contains("`p!`"), "{}", reset.content());
    let profile = sim.text_one(unique(), &[], "p!profile", &[]).await;
    assert_eq!(profile.files[0].0, "profile.png");
}

#[tokio::test]
async fn the_text_commands_module_switches_them_off_by_server_and_channel() {
    let Some(mut sim) = Sim::new("the_text_commands_module_switches_them_off_by_server_and_channel").await else {
        return;
    };
    let staff = sim.permission_role(MANAGE_GUILD);
    let (manager, member) = (unique(), unique());
    let works = |reply: &Reply| assert_eq!(reply.files[0].0, "profile.png");

    // On by default.
    works(&sim.text_one(member, &[], "p!profile", &[]).await);

    // Off for the whole server.
    let off = modules(&mut sim, manager, toggle("text_commands", false), &[]).await;
    assert!(off.content().contains("**Text commands** · off"), "{}", off.content());
    // The prefix form says nothing (another bot may use that prefix there); help is no exception.
    for text in ["p!profile", "p!help", "p!ping", "p!duel connect4", "p!fish gear"] {
        let (ran, replies) = sim.text(member, &[], text, &[]).await;
        assert!(ran && replies.is_empty(), "{text}: {} replies", replies.len());
    }
    // The @mention form answers with the module's words, and a manager is shown the way back.
    for text in ["profile", "help", "ping"] {
        let refused = sim.text_one(member, &[], &format!("{BOT_MENTION} {text}"), &[]).await;
        assert_eq!(
            refused.content(),
            "This feature is turned off on this server.",
            "{text}"
        );
    }
    let way_back = sim
        .text_one(manager, &[staff], &format!("{BOT_MENTION} profile"), &[])
        .await;
    assert!(
        way_back.content().contains("**Text commands**") && way_back.content().contains("/config modules"),
        "{}",
        way_back.content()
    );
    // Slash commands are not touched.
    let slash = sim.command(member, "profile", json!([])).await;
    assert_eq!(slash.files[0].0, "profile.png");

    let on = modules(&mut sim, manager, toggle("text_commands", true), &[]).await;
    assert!(on.content().contains("**Text commands** · on"), "{}", on.content());
    works(&sim.text_one(member, &[], "p!profile", &[]).await);
    works(&sim.text_one(member, &[], &format!("{BOT_MENTION} profile"), &[]).await);

    // Only in a category: its channels and their threads, nowhere else.
    let (category, lounge, thread, loose) = (unique(), unique(), unique(), unique());
    sim.channel_is(category, 4, None);
    sim.channel_is(lounge, 0, Some(category));
    sim.channel_is(thread, 11, Some(lounge));
    sim.channel_is(loose, 0, None);
    modules(
        &mut sim,
        manager,
        limit("text_commands", "only", category, false),
        &[category],
    )
    .await;
    let (ran, replies) = sim.text(member, &[], "p!profile", &[]).await;
    assert!(ran && replies.is_empty(), "this channel is not on the list");
    let refused = sim.text_one(member, &[], &format!("{BOT_MENTION} profile"), &[]).await;
    assert_eq!(refused.content(), "This feature is turned off in this channel.");
    let home = sim.channel;
    for open in [lounge, thread] {
        sim.channel = open;
        works(&sim.text_one(member, &[], "p!profile", &[]).await);
    }
    sim.channel = loose;
    let (ran, replies) = sim.text(member, &[], "p!profile", &[]).await;
    assert!(ran && replies.is_empty(), "a channel outside the category");

    // Everywhere but one channel: its threads follow it.
    sim.channel = home;
    modules(
        &mut sim,
        manager,
        limit("text_commands", "except", lounge, false),
        &[lounge],
    )
    .await;
    for shut in [lounge, thread] {
        sim.channel = shut;
        let (ran, replies) = sim.text(member, &[], "p!profile", &[]).await;
        assert!(ran && replies.is_empty(), "channel {shut}");
    }
    for open in [loose, home] {
        sim.channel = open;
        works(&sim.text_one(member, &[], "p!profile", &[]).await);
    }
}

#[tokio::test]
async fn text_commands_need_use_application_commands_in_the_channel() {
    let Some(mut sim) = Sim::new("text_commands_need_use_application_commands_in_the_channel").await else {
        return;
    };
    let guild = sim.guild;
    let admin = sim.permission_role(ADMINISTRATOR);
    let vip = sim.permission_role(0);
    let muted = sim.permission_role(0);
    let (locked, thread, member, plain) = (unique(), unique(), unique(), unique());
    let no_commands = "You can't use commands in this channel.";

    // `@everyone` may not use commands here; one role may again; one member may not, whatever their roles.
    sim.channel_with(
        locked,
        0,
        None,
        &[
            (guild, false, 0, USE_APPLICATION_COMMANDS),
            (vip, false, USE_APPLICATION_COMMANDS, 0),
            (plain, true, 0, USE_APPLICATION_COMMANDS),
        ],
    );
    sim.channel_with(thread, 11, Some(locked), &[]);
    sim.channel = locked;
    let refused = sim.text_one(member, &[], "p!profile", &[]).await;
    assert_eq!(refused.content(), no_commands);
    // The @mention form is held to the same rule, and so is a thread (it follows the channel it sits in).
    let refused = sim
        .text_one(member, &[muted], &format!("{BOT_MENTION} profile"), &[])
        .await;
    assert_eq!(refused.content(), no_commands);
    sim.channel = thread;
    let refused = sim.text_one(member, &[], "p!profile", &[]).await;
    assert_eq!(refused.content(), no_commands, "a thread follows its channel");
    sim.channel = locked;
    let works = |reply: &Reply| assert_eq!(reply.files[0].0, "profile.png");
    works(&sim.text_one(member, &[vip], "p!profile", &[]).await);
    works(&sim.text_one(member, &[admin], "p!profile", &[]).await);
    // The role that allows beats the one that doesn't ask; the member's own overwrite comes last.
    works(&sim.text_one(member, &[vip, muted], "p!profile", &[]).await);
    let refused = sim.text_one(plain, &[vip], "p!profile", &[]).await;
    assert_eq!(refused.content(), no_commands, "the member's own overwrite wins");

    // The same member is let in anywhere else, and the slash command is not in question.
    sim.channel = unique();
    works(&sim.text_one(member, &[], "p!profile", &[]).await);
}

#[tokio::test]
async fn text_commands_obey_the_integrations_overrides() {
    let Some(mut sim) = Sim::new("text_commands_obey_the_integrations_overrides").await else {
        return;
    };
    let guild = sim.guild;
    let (fish, pet) = (unique(), unique());
    let (banned, friend, member, boss) = (unique(), unique(), unique(), unique());
    let (nofish, fisher, admin) = (
        sim.permission_role(0),
        sim.permission_role(0),
        sim.permission_role(ADMINISTRATOR),
    );
    for angler in [member, banned, friend] {
        fund(&sim, angler, 500).await;
    }
    let (second, third) = (unique(), unique());
    for angler in [second, third] {
        fund(&sim, angler, 500).await;
    }
    let (open, shut) = (unique(), unique());
    let works = |reply: &Reply| assert_eq!(reply.files[0].0, "profile.png");
    let not_here = |reply: &Reply, name: &str| {
        assert_eq!(
            reply.content(),
            format!("This server's command permissions don't let you use `{name}` here.")
        );
    };

    sim.registered(&[(fish, "fish"), (pet, "pet"), (unique(), "profile")]);
    sim.integrations(&[
        // Every command: not in `shut`, and the member `banned` may not use any.
        (
            APPLICATION_ID,
            &[(guild - 1, 3, true), (shut, 3, false), (banned, 2, false)],
        ),
        // `/fish` has its own entry, which replaces that one: a role is off, another is on, a member too.
        (
            fish,
            &[
                (nofish, 1, false),
                (fisher, 1, true),
                (friend, 2, true),
                (guild, 1, true),
            ],
        ),
        // `/pet` is for one role only.
        (pet, &[(guild, 1, false), (fisher, 1, true)]),
    ]);

    // The entry for all commands.
    works(&sim.text_one(member, &[], "p!profile", &[]).await);
    let refused = sim.text_one(banned, &[], "p!profile", &[]).await;
    not_here(&refused, "profile");
    sim.channel = shut;
    let refused = sim.text_one(member, &[], "p!profile", &[]).await;
    not_here(&refused, "profile");
    let refused = sim.text_one(member, &[], &format!("{BOT_MENTION} profile"), &[]).await;
    not_here(&refused, "profile");
    sim.channel = open;
    works(&sim.text_one(member, &[], "p!profile", &[]).await);

    // A command with its own entry: the member beats the role, an allowing role beats a denying one.
    let cast = sim.text_one(member, &[], "p!fish cast Shore", &[]).await;
    assert_eq!(cast.title(), "🎣 Line in the water…", "@everyone is allowed");
    let refused = sim.text_one(second, &[nofish], "p!fish cast Shore", &[]).await;
    not_here(&refused, "fish");
    let again = sim.text_one(second, &[nofish, fisher], "p!fish cast Shore", &[]).await;
    assert_eq!(
        again.title(),
        "🎣 Line in the water…",
        "a role that allows beats one that denies"
    );
    // `banned` is only banned by the entry for all commands, which `/fish` does not use.
    let cast = sim.text_one(banned, &[], "p!fish cast Shore", &[]).await;
    assert_eq!(cast.title(), "🎣 Line in the water…");
    let cast = sim.text_one(friend, &[nofish], "p!fish cast Shore", &[]).await;
    assert_eq!(cast.title(), "🎣 Line in the water…", "the member beats their role");

    // `/pet` is off for `@everyone` and on for a role.
    let refused = sim.text_one(third, &[], "p!pet list", &[]).await;
    not_here(&refused, "pet");
    let pets = sim.text_one(third, &[fisher], "p!pet list", &[]).await;
    assert!(!pets.content().contains("command permissions"), "{}", pets.content());

    // Administrators are never asked.
    sim.channel = shut;
    works(&sim.text_one(boss, &[admin], "p!profile", &[]).await);
    let pets = sim.text_one(boss, &[admin], "p!pet list", &[]).await;
    assert!(!pets.content().contains("command permissions"), "{}", pets.content());

    // Asked once, then remembered for a while; until Discord says it changed.
    assert_eq!(sim.integration_lookups(), 1);
    sim.integrations(&[(APPLICATION_ID, &[(member, 2, false)])]);
    sim.channel = open;
    let refused = sim.text_one(banned, &[], "p!profile", &[]).await;
    not_here(&refused, "profile");
    assert_eq!(sim.integration_lookups(), 1, "still the old settings");
    on_permissions_changed(&sim.app, PermissionChange::CommandPermissions(sim.guild)).await;
    let refused = sim.text_one(member, &[], "p!profile", &[]).await;
    not_here(&refused, "profile");
    works(&sim.text_one(banned, &[], "p!profile", &[]).await);
    assert_eq!(sim.integration_lookups(), 2);
}

#[tokio::test]
async fn text_commands_without_command_permissions_to_read_still_run() {
    let Some(mut sim) = Sim::new("text_commands_without_command_permissions_to_read_still_run").await else {
        return;
    };
    // A server the bot was added to without the commands scope: Discord has nothing to say about it.
    for status in [404, 403] {
        sim.mock.command_permissions_status.store(status, Ordering::SeqCst);
        sim.app.forget_command_overrides(sim.guild).await;
        let profile = sim.text_one(unique(), &[], "p!profile", &[]).await;
        assert_eq!(profile.files[0].0, "profile.png", "status {status}");
    }

    // Discord failing otherwise is not "no overrides": the command is refused, not run unchecked.
    sim.mock.command_permissions_status.store(500, Ordering::SeqCst);
    sim.app.forget_command_overrides(sim.guild).await;
    let (ran, mut replies, errors) = sim.send_logged(unique(), &[], "p!profile", &[]).await;
    assert!(ran && errors.len() == 1, "{errors:?}");
    assert_eq!(replies.len(), 1);
    let refused = replies.remove(0);
    assert_eq!(
        refused.content(),
        "Something went wrong on our side. Please try again in a moment."
    );
    assert!(refused.files.is_empty());

    // And it recovers when Discord does (nothing was remembered of the failure).
    sim.mock.command_permissions_status.store(0, Ordering::SeqCst);
    let profile = sim.text_one(unique(), &[], "p!profile", &[]).await;
    assert_eq!(profile.files[0].0, "profile.png");
}

#[tokio::test]
async fn permission_changes_reach_text_commands_at_once() {
    let Some(mut sim) = Sim::new("permission_changes_reach_text_commands_at_once").await else {
        return;
    };
    let manager = unique();
    // The first command makes the bot read the server's roles.
    sim.text_one(unique(), &[], "p!ping", &[]).await;
    // A role is made afterwards: the bot still has the old list.
    let staff = sim.permission_role(MANAGE_GUILD);
    let refused = sim.text_one(manager, &[staff], "p!surge", &[]).await;
    assert_eq!(
        refused.content(),
        "You need the Manage Server permission to use `surge`.",
        "the role list is a minute old"
    );
    // A member update (which may have changed roles) makes the bot read them again.
    sim.update_member(manager, "Manager", None).await;
    let storm = sim.text_one(manager, &[staff], "p!surge", &[]).await;
    assert!(storm.content().contains("Storm surge"), "{}", storm.content());

    // So does a role or server update.
    let (other, second) = (unique(), sim.permission_role(MANAGE_GUILD));
    let refused = sim.text_one(other, &[second], "p!surge", &[]).await;
    assert!(
        refused.content().starts_with("You need the Manage Server"),
        "{}",
        refused.content()
    );
    on_permissions_changed(&sim.app, PermissionChange::Guild(sim.guild)).await;
    let allowed = sim.text_one(other, &[second], "p!surge", &[]).await;
    assert!(
        allowed.content().contains("breather"),
        "a manager now, so the handler answers: {}",
        allowed.content()
    );

    // A channel's overwrites are looked at again when the channel changed.
    let channel = unique();
    sim.channel_with(channel, 0, None, &[]);
    sim.channel = channel;
    let profile = sim.text_one(unique(), &[], "p!profile", &[]).await;
    assert_eq!(profile.files[0].0, "profile.png");
    sim.channel_with(channel, 0, None, &[(sim.guild, false, 0, USE_APPLICATION_COMMANDS)]);
    let profile = sim.text_one(unique(), &[], "p!profile", &[]).await;
    assert_eq!(profile.files[0].0, "profile.png", "the overwrites are a minute old");
    on_permissions_changed(&sim.app, PermissionChange::Channel(channel)).await;
    let refused = sim.text_one(unique(), &[], "p!profile", &[]).await;
    assert_eq!(refused.content(), "You can't use commands in this channel.");
}

#[tokio::test]
async fn private_text_answers_are_deleted_when_the_bot_shuts_down() {
    let Some(mut sim) = Sim::new("private_text_answers_are_deleted_when_the_bot_shuts_down").await else {
        return;
    };
    let member = unique();
    // A private answer (a notice) and a public one (the card).
    let usage = sim.text_one(member, &[], "p!duel connect4", &[]).await;
    assert!(usage.content().starts_with("`user` is missing."), "{}", usage.content());
    let profile = sim.text_one(member, &[], "p!profile", &[]).await;
    assert_eq!(profile.files[0].0, "profile.png");
    assert!(
        sim.mock.deleted_messages.lock().unwrap().is_empty(),
        "it is the notice's 15 seconds still"
    );

    // Shutting down does not wait them out: the private one goes at once, the public one stays.
    let began = std::time::Instant::now();
    sim.app.drain(Duration::from_secs(10)).await;
    assert!(
        began.elapsed() < Duration::from_secs(8),
        "drain waited out the 15 seconds"
    );
    let deleted = sim.mock.deleted_messages.lock().unwrap().clone();
    assert_eq!(deleted.len(), 1, "{deleted:?}");
    assert_eq!(deleted[0].0, sim.channel);
}

/// A message carries no Discord language: a text command answers in the member's `/language` pick,
/// else the server's default, else English; `/config prefix` follows the interaction's language.
#[tokio::test]
async fn text_commands_answer_in_the_members_or_the_servers_language() {
    use pinchora_core::i18n::Lang;

    let Some(mut sim) = Sim::new("text_commands_answer_in_the_members_or_the_servers_language").await else {
        return;
    };
    let (member, picker, manager) = (unique(), unique(), unique());

    // Nothing chosen anywhere: English.
    let usage = sim.text_one(member, &[], "p!duel connect4", &[]).await;
    assert_eq!(
        usage.content(),
        "`user` is missing.\nUsage: `p!duel connect4 <user> [bet]`"
    );

    // The server's default is Turkish: notices, usage lines and the guide's hint follow.
    sim.app
        .set_guild_lang(sim.guild, Some(Lang::Tr))
        .await
        .expect("set the server language");
    let usage = sim.text_one(member, &[], "p!duel connect4", &[]).await;
    assert_eq!(
        usage.content(),
        "`user` eksik.\nKullanım: `p!duel connect4 <user> [bet]`"
    );
    let editor = sim.text_one(member, &[], "p!house edit", &[]).await;
    assert_eq!(
        editor.content(),
        "Bu komut yalnızca slash komutu olarak çalışıyor: `/house edit` yaz."
    );
    let refused = sim.text_one(member, &[], "p!surge", &[]).await;
    assert_eq!(
        refused.content(),
        "`surge` komutunu kullanmak için Sunucuyu Yönet iznine ihtiyacın var."
    );
    let tip = sim.text_one(member, &[], "p!help", &[]).await;
    assert!(
        tip.description()
            .contains("Her komut burada yazıyla da çalışır: `p!profile`"),
        "{}",
        tip.description()
    );
    let (ran, hint) = sim.text(member, &[], &format!("{BOT_MENTION} nothing"), &[]).await;
    assert!(ran && hint.len() == 1);
    assert_eq!(hint[0].content(), "Buradaki önekim `p!`: `p!help` ya da `/help` dene.");

    // A member's own pick beats the server's default.
    sim.app
        .set_user_lang(picker, Some(Lang::En))
        .await
        .expect("set the member language");
    let usage = sim.text_one(picker, &[], "p!duel connect4", &[]).await;
    assert!(usage.content().starts_with("`user` is missing."), "{}", usage.content());

    // `/config prefix` is answered in the language of the interaction.
    sim.locale = "tr";
    sim.permissions = MANAGE_GUILD;
    let shown = sim
        .command(manager, "config", json!([group("prefix", sub("show", vec![]))]))
        .await;
    assert!(
        shown.content().starts_with("Metin komutları şununla başlıyor: `p!`"),
        "{}",
        shown.content()
    );
    let bad = sim
        .command(
            manager,
            "config",
            json!([group("prefix", sub("set", vec![text_option("prefix", "@x")]))]),
        )
        .await;
    assert_eq!(bad.content(), "Önekte boşluk, @, # ya da ters tırnak olamaz.");
}
