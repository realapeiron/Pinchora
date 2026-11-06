//! Text commands (`p!profile`, `@Pinchora fish cast`) through the real handlers: the same answers,
//! buttons and refusals as the slash commands, posted as replies that ping nobody. Pictures land in
//! `target/sim/<test>/`.

use pinchora_commands::on_text_command;

use super::*;

/// What `@Pinchora` looks like in a message.
const BOT_MENTION: &str = "<@1>";

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
        self.step += 1;
        let step = self.step;
        let message = self.text_message(user, roles, content, mentions);
        self.mock.posts.lock().unwrap().clear();
        ERRORS.with_borrow_mut(Vec::clear);
        let ran = on_text_command(self.app.clone(), &message).await;
        let errors = ERRORS.take();
        assert!(
            errors.is_empty(),
            "step {step} ({content}): failed:\n{}",
            errors.join("\n")
        );
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
        (ran, replies)
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

    // `/config` needs Manage Server, as Discord asks of the slash command.
    let refused = sim.text_one(member, &[], "p!config prefix set z!", &[]).await;
    assert_eq!(
        refused.content(),
        "You need the Manage Server permission to use `config`."
    );
    let set = sim.text_one(manager, &[staff], "p!config prefix set z!", &[]).await;
    // A private answer without buttons is posted for its 15 seconds.
    let settings = sim.text_one(manager, &[staff], "z!config show", &[]).await;
    assert_eq!(settings.title(), "Server settings");
    assert!(
        set.content().starts_with("Text commands now start with `z!`"),
        "{}",
        set.content()
    );

    // The new prefix works at once; the old one is just chat.
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

    // The slash command sees it, and refuses what the shared rule refuses.
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
    let off = sim
        .text_one(manager, &[staff], "z!config modules toggle pets no", &[])
        .await;
    assert!(off.content().contains("**Pets**"), "{}", off.content());
    let refused = sim.text_one(member, &[], "z!pet view", &[]).await;
    let slash = sim.command(member, "pet", json!([sub("view", vec![])])).await;
    assert_eq!(refused.content(), slash.content());
    assert_eq!(refused.content(), "This feature is turned off on this server.");

    // Back to the default.
    let reset = sim.text_one(manager, &[staff], "z!config prefix reset", &[]).await;
    assert!(reset.content().contains("`p!`"), "{}", reset.content());
    let profile = sim.text_one(unique(), &[], "p!profile", &[]).await;
    assert_eq!(profile.files[0].0, "profile.png");
}
