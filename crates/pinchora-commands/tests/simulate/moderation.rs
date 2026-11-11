//! Moderation through the real handlers: every command opens a numbered case, tells the member and
//! posts to the mod-log; the hierarchy and the permissions refuse what Discord would; the warn ladder
//! acts on its own; temporary bans end on the ticker; and the server log reports message edits and
//! deletes, the audit log's entries and voice, with its ignore lists and switches. Nothing here draws a
//! picture: every answer is an embed.

use std::time::Duration;

use pinchora_commands::{
    lift_expired_bans, on_audit_log_entry, on_message_delete, on_message_delete_bulk, on_message_update,
};
use pinchora_core::admin::logging::LogEvent;
use pinchora_db::{CaseFilter, StaffFilter};
use twilight_model::gateway::payload::incoming::{
    GuildAuditLogEntryCreate, MessageDelete, MessageDeleteBulk, MessageUpdate,
};

use crate::*;
mod revoke_race;

const KICK: u64 = 1 << 1;
const BAN: u64 = 1 << 2;
const MANAGE_CHANNELS: u64 = 1 << 4;
const MANAGE_MESSAGES: u64 = 1 << 13;
const MANAGE_NICKNAMES: u64 = 1 << 27;
const MANAGE_ROLES: u64 = 1 << 28;
const MODERATE: u64 = 1 << 40;
/// Everything a moderator could need, Manage Server included (for the settings).
const STAFF: u64 =
    KICK | BAN | MANAGE_CHANNELS | MANAGE_MESSAGES | MANAGE_NICKNAMES | MANAGE_ROLES | MODERATE | MANAGE_GUILD;

/// `data.resolved` for a user option: `name` as their display name, and, when `member` is given, a
/// member holding those roles with those permissions (and timed out until then, if so).
fn someone(user: u64, name: &str, member: Option<(&[u64], u64, Option<&str>)>) -> Value {
    let mut resolved =
        json!({ "users": { user.to_string(): person_json(user, &name.to_lowercase(), Some(name), false) } });
    if let Some((roles, permissions, timed_out)) = member {
        resolved["members"] = json!({
            user.to_string(): {
                "roles": roles.iter().map(u64::to_string).collect::<Vec<_>>(),
                "joined_at": "2024-01-01T00:00:00.000000+00:00",
                "flags": 0,
                "permissions": permissions.to_string(),
                "pending": false,
                "communication_disabled_until": timed_out,
            },
        });
    }
    resolved
}

/// A plain member, no roles, no permissions.
fn member(user: u64, name: &str) -> Value {
    someone(user, name, Some((&[], 0, None)))
}

fn with_role(mut resolved: Value, role: &Value) -> Value {
    resolved["roles"] = json!({ role["id"].as_str().unwrap(): role });
    resolved
}

impl Sim {
    /// Moderation calls the mock saw since the last look.
    fn mod_calls(&self) -> Vec<String> {
        std::mem::take(&mut *self.mock.mod_calls.lock().unwrap())
    }

    /// A message in `channel` from `author`, sent `age` ago, as Discord lists them.
    fn history_message(&self, channel: u64, author: u64, bot: bool, age: chrono::Duration, pinned: bool) -> Value {
        let sent = chrono::Utc::now() - age;
        let id = (((sent.timestamp_millis() - 1_420_070_400_000) as u64) << 22) | (unique() & 0x3f_ffff);
        let mut message = message_json(channel, id);
        message["author"] = person_json(author, "someone", None, bot);
        message["pinned"] = json!(pinned);
        message
    }

    /// An audit log entry made now (its id is a snowflake of this moment).
    async fn audit(&self, kind: u16, actor: u64, target: u64, changes: Value, reason: Option<&str>) {
        let now = self.app.now().timestamp_millis() - 1_420_070_400_000;
        let id = ((now as u64) << 22) | (unique() & 0x3f_ffff);
        let entry: GuildAuditLogEntryCreate = serde_json::from_value(json!({
            "action_type": kind,
            "changes": changes,
            "guild_id": self.guild.to_string(),
            "id": id.to_string(),
            "target_id": target.to_string(),
            "user_id": actor.to_string(),
            "reason": reason,
        }))
        .expect("audit log entry JSON");
        println!("\n== {}: audit log entry {kind} by {actor} on {target}", self.name);
        on_audit_log_entry(self.app.clone(), entry)
            .await
            .expect("the audit log handler failed");
    }

    /// Delivers a chat message with `content` and returns its id.
    async fn said(&self, user: u64, content: &str) -> u64 {
        let mut message = serde_json::to_value(self.chat(user, &[])).expect("message JSON");
        message["content"] = json!(content);
        let id: u64 = message["id"].as_str().unwrap().parse().unwrap();
        on_message(self.app.clone(), serde_json::from_value(message).unwrap())
            .await
            .expect("the message handler failed");
        id
    }

    async fn edited(&self, user: u64, id: u64, content: &str) {
        let mut message = serde_json::to_value(self.chat(user, &[])).expect("message JSON");
        message["id"] = json!(id.to_string());
        message["content"] = json!(content);
        message["edited_timestamp"] = json!("2024-01-01T00:01:00.000000+00:00");
        let event: MessageUpdate = serde_json::from_value(message).expect("message update JSON");
        on_message_update(self.app.clone(), event)
            .await
            .expect("the edit handler failed");
    }

    async fn deleted(&self, ids: &[u64]) {
        let (channel, guild) = (CHANNEL_ID.to_string(), self.guild.to_string());
        if let [id] = ids {
            let event: MessageDelete =
                serde_json::from_value(json!({ "id": id.to_string(), "channel_id": channel, "guild_id": guild }))
                    .unwrap();
            on_message_delete(self.app.clone(), event)
                .await
                .expect("the delete handler failed");
        } else {
            let ids: Vec<String> = ids.iter().map(u64::to_string).collect();
            let event: MessageDeleteBulk =
                serde_json::from_value(json!({ "ids": ids, "channel_id": channel, "guild_id": guild })).unwrap();
            on_message_delete_bulk(self.app.clone(), event)
                .await
                .expect("the bulk delete handler failed");
        }
    }

    /// The cases this server has, newest first.
    async fn cases(&self) -> Vec<pinchora_db::CaseRow> {
        self.app
            .db
            .cases(
                self.guild,
                &CaseFilter {
                    limit: 100,
                    ..CaseFilter::default()
                },
            )
            .await
            .unwrap()
    }
}

fn in_channel_titled(posts: &[Post], channel: u64) -> Vec<&str> {
    posts.iter().filter(|p| p.channel == channel).map(Post::title).collect()
}

#[tokio::test]
async fn moderation_commands_open_cases_tell_members_and_post_to_the_mod_log() {
    let Some(mut sim) = Sim::new("moderation_commands_open_cases_tell_members_and_post_to_the_mod_log").await else {
        return;
    };
    let (moderator, mia) = (unique(), unique());
    let (mod_role, helper_id, lead_id, modlog) = (unique(), unique(), unique(), unique());
    sim.guild_role(mod_role, "Moderators", 5);
    let helper = sim.guild_role(helper_id, "Helpers", 2);
    let lead = sim.guild_role(lead_id, "Leads", 8);
    sim.roles = vec![mod_role];
    sim.permissions = STAFF;

    // The mod-log and a rung on the warn ladder.
    let set = sim
        .command_resolved(
            moderator,
            "config",
            json!([sub("moderation", vec![channel_option("channel", modlog)])]),
            resolved_channels(&[modlog]),
        )
        .await;
    assert!(
        set.ephemeral && set.content().contains(&format!("Cases go to <#{modlog}>")),
        "{}",
        set.content()
    );
    let ladder = sim
        .command(
            moderator,
            "config",
            json!([sub(
                "warn-ladder",
                vec![
                    int_option("warns", 2),
                    text_option("action", "timeout"),
                    text_option("duration", "1h")
                ]
            )]),
        )
        .await;
    assert!(
        ladder.content().contains("2 warns → 1h timeout"),
        "{}",
        ladder.content()
    );

    // A warn: a public answer with the case, a direct message and the mod-log entry.
    let warned = sim
        .command_resolved(
            moderator,
            "warn",
            json!([user_option("user", mia), text_option("reason", "Spam in #general")]),
            member(mia, "Mia"),
        )
        .await;
    assert!(!warned.ephemeral, "a warn is said where it happened");
    assert!(
        warned.description().contains("**Mia** was warned · case #1"),
        "{}",
        warned.description()
    );
    assert!(
        warned.description().contains("**1** active warn."),
        "{}",
        warned.description()
    );
    assert!(warned.pings_nobody());
    let posts = sim.posts();
    let dm = posts
        .iter()
        .find(|p| p.channel == mia)
        .expect("a direct message to Mia");
    assert!(
        dm.description()
            .contains(&format!("You were warned in **{GUILD_NAME}**")),
        "{}",
        dm.description()
    );
    assert_eq!(dm.field("Reason"), "Spam in #general");
    let logged = posts.iter().find(|p| p.channel == modlog).expect("a mod-log entry");
    assert_eq!(logged.title(), "Case #1 · Warn");
    assert_eq!(logged.field("Member"), format!("<@{mia}> · Mia"));
    assert_eq!(logged.field("Moderator"), format!("<@{moderator}> · Sim"));
    assert_eq!(logged.body["allowed_mentions"]["parse"], json!([]));

    // The second warn reaches the ladder: the bot times Mia out for an hour, as a case of its own.
    let again = sim
        .command_resolved(
            moderator,
            "warn",
            json!([user_option("user", mia), text_option("reason", "Again")]),
            member(mia, "Mia"),
        )
        .await;
    assert!(
        again
            .description()
            .contains("2 warns reached: timed out for 1h · case #3"),
        "{}",
        again.description()
    );
    let calls = sim.mod_calls();
    assert!(
        calls.len() == 1
            && calls[0].starts_with(&format!("member {mia}"))
            && calls[0].contains("communication_disabled_until"),
        "{calls:?}"
    );
    let posts = sim.posts();
    assert_eq!(
        in_channel_titled(&posts, modlog),
        ["Case #2 · Warn", "Case #3 · Timeout"]
    );
    let auto = posts.iter().find(|p| p.title() == "Case #3 · Timeout").unwrap();
    assert_eq!(auto.field("Moderator"), "<@1> · Pinchora");
    assert_eq!(auto.field("Reason"), "Automatic: 2 warns → 1h timeout");

    let warnings = sim
        .command_resolved(
            moderator,
            "warnings",
            json!([user_option("user", mia)]),
            member(mia, "Mia"),
        )
        .await;
    assert!(warnings.ephemeral);
    assert_eq!(warnings.title(), "Warns of Mia · 2");
    assert!(warnings.description().contains("**#2**") && warnings.description().contains("> Spam in #general"));

    let unwarned = sim.command(moderator, "unwarn", json!([int_option("case", 1)])).await;
    assert!(
        unwarned
            .description()
            .contains("A warn of **Mia** was removed (case #1) · case #4")
            && unwarned.description().contains("**1** active warn now"),
        "{}",
        unwarned.description()
    );
    let twice = sim.command(moderator, "unwarn", json!([int_option("case", 1)])).await;
    assert!(twice.ephemeral && twice.content().contains("not a warn that still counts"));
    sim.posts();

    // Timeouts.
    let timed = sim
        .command_resolved(
            moderator,
            "timeout",
            json!([
                user_option("user", mia),
                text_option("duration", "10m"),
                text_option("reason", "Cool down")
            ]),
            member(mia, "Mia"),
        )
        .await;
    assert!(
        timed.description().contains("**Mia** was timed out for 10m · case #5"),
        "{}",
        timed.description()
    );
    assert_eq!(sim.mod_calls().len(), 1);
    let posts = sim.posts();
    assert!(
        posts
            .iter()
            .any(|p| p.channel == mia && p.description().contains("You were timed out"))
    );
    let not_out = sim
        .command_resolved(
            moderator,
            "untimeout",
            json!([user_option("user", mia)]),
            member(mia, "Mia"),
        )
        .await;
    assert!(not_out.ephemeral && not_out.content().contains("isn't timed out"));
    let lifted = sim
        .command_resolved(
            moderator,
            "untimeout",
            json!([user_option("user", mia)]),
            someone(mia, "Mia", Some((&[], 0, Some("2099-01-01T00:00:00.000000+00:00")))),
        )
        .await;
    assert!(
        lifted.description().contains("**Mia**'s timeout was removed · case #6"),
        "{}",
        lifted.description()
    );
    assert!(sim.mod_calls()[0].contains("\"communication_disabled_until\":null"));

    // A nickname and roles, answered privately.
    let renamed = sim
        .command_resolved(
            moderator,
            "nick",
            json!([user_option("user", mia), text_option("nickname", "Calm Mia")]),
            member(mia, "Mia"),
        )
        .await;
    assert!(renamed.ephemeral && renamed.description().contains("nickname set to `Calm Mia`"));
    assert!(sim.mod_calls()[0].contains("\"nick\":\"Calm Mia\""));
    let given = sim
        .command_resolved(
            moderator,
            "role",
            json!([sub("add", vec![user_option("user", mia), role_option("role", &helper)])]),
            with_role(member(mia, "Mia"), &helper),
        )
        .await;
    assert!(
        given.ephemeral && given.description().contains("**Mia** was given @Helpers"),
        "{}",
        given.description()
    );
    assert_eq!(sim.role_changes(), [format!("PUT {mia} {helper_id}")]);
    let too_high = sim
        .command_resolved(
            moderator,
            "role",
            json!([sub("add", vec![user_option("user", mia), role_option("role", &lead)])]),
            with_role(member(mia, "Mia"), &lead),
        )
        .await;
    assert!(
        too_high.ephemeral && too_high.content().contains("not below your highest role"),
        "{}",
        too_high.content()
    );
    assert!(sim.role_changes().is_empty());
    let taken = sim
        .command_resolved(
            moderator,
            "role",
            json!([sub(
                "remove",
                vec![user_option("user", mia), role_option("role", &helper)]
            )]),
            with_role(someone(mia, "Mia", Some((&[helper_id], 0, None))), &helper),
        )
        .await;
    assert!(
        taken.description().contains("**Mia** lost @Helpers"),
        "{}",
        taken.description()
    );
    assert_eq!(sim.role_changes(), [format!("DELETE {mia} {helper_id}")]);
    sim.posts();

    // A kick tells Mia first, then removes her.
    let kicked = sim
        .command_resolved(
            moderator,
            "kick",
            json!([user_option("user", mia), text_option("reason", "Last chance")]),
            member(mia, "Mia"),
        )
        .await;
    assert!(
        kicked.description().contains("**Mia** was kicked · case #10"),
        "{}",
        kicked.description()
    );
    assert_eq!(sim.mod_calls(), [format!("kick {mia}")]);
    assert!(
        sim.posts()
            .iter()
            .any(|p| p.channel == mia && p.description().contains("You were kicked"))
    );

    // A temporary ban that deletes a day of messages, then the unban.
    let banned = sim
        .command_resolved(
            moderator,
            "ban",
            json!([
                user_option("user", mia),
                text_option("duration", "1d"),
                int_option("delete_days", 1),
                text_option("reason", "Raid"),
            ]),
            member(mia, "Mia"),
        )
        .await;
    assert!(
        banned.description().contains("**Mia** was banned for 1d · case #11"),
        "{}",
        banned.description()
    );
    assert_eq!(sim.mod_calls(), [format!("ban {mia} 86400")]);
    let posts = sim.posts();
    let entry = posts.iter().find(|p| p.title() == "Case #11 · Ban").unwrap();
    assert!(
        entry.field("Duration").starts_with("1d · ends <t:"),
        "{}",
        entry.field("Duration")
    );
    let unbanned = sim
        .command_resolved(
            moderator,
            "unban",
            json!([user_option("user", mia)]),
            someone(mia, "Mia", None),
        )
        .await;
    assert!(
        unbanned.description().contains("**Mia** was unbanned · case #12"),
        "{}",
        unbanned.description()
    );
    assert_eq!(sim.mod_calls(), [format!("unban {mia}")]);
    let not_banned = sim
        .command_resolved(
            moderator,
            "unban",
            json!([user_option("user", mia)]),
            someone(mia, "Mia", None),
        )
        .await;
    assert!(
        not_banned.ephemeral && not_banned.content().contains("isn't banned"),
        "{}",
        not_banned.content()
    );
    assert!(
        !sim.cases().await.iter().any(|c| c.kind == "ban" && c.active),
        "the unban ended the ban"
    );
    sim.posts();

    // Looking a case up, and giving one a new reason: the mod-log entry is edited in place.
    let case = sim.command(moderator, "case", json!([int_option("id", 1)])).await;
    assert!(case.ephemeral);
    assert_eq!(case.title(), "Case #1 · Warn");
    assert_eq!(case.field("Status"), "Removed");
    let reasoned = sim
        .command(
            moderator,
            "reason",
            json!([int_option("id", 2), text_option("reason", "Spam, twice")]),
        )
        .await;
    assert_eq!(reasoned.field("Reason"), "Spam, twice");
    let posts = sim.posts();
    let edit = posts
        .iter()
        .find(|p| p.method == Method::PATCH)
        .expect("the mod-log entry edited");
    assert_eq!((edit.channel, edit.title()), (modlog, "Case #2 · Warn"));
    assert_eq!(edit.field("Reason"), "Spam, twice");
    let missing = sim.command(moderator, "case", json!([int_option("id", 999)])).await;
    assert!(missing.content().contains("There is no case #999"));

    // Purge: the bots' recent messages, not the pinned one, not the one too old to bulk delete.
    let here = sim.channel;
    let bot = unique();
    let history = vec![
        sim.history_message(here, bot, true, chrono::Duration::minutes(1), false),
        sim.history_message(here, mia, false, chrono::Duration::minutes(2), false),
        sim.history_message(here, bot, true, chrono::Duration::minutes(3), true),
        sim.history_message(here, bot, true, chrono::Duration::minutes(4), false),
        sim.history_message(here, bot, true, chrono::Duration::days(15), false),
    ];
    sim.mock.history.lock().unwrap().insert(here, history);
    let purged = sim
        .command(
            moderator,
            "purge",
            json!([int_option("count", 10), bool_option("bots", true)]),
        )
        .await;
    assert!(purged.ephemeral);
    assert!(
        purged
            .description()
            .starts_with(&format!("Deleted 2 messages (from bots) in <#{here}>")),
        "{}",
        purged.description()
    );
    assert_eq!(sim.mod_calls(), ["bulk-delete 2"]);
    let one = sim
        .command_resolved(
            moderator,
            "purge",
            json!([int_option("count", 5), user_option("user", mia)]),
            member(mia, "Mia"),
        )
        .await;
    assert!(
        one.description().starts_with("Deleted 1 message (from mia)"),
        "{}",
        one.description()
    );
    assert!(sim.mod_calls()[0].starts_with("delete "));
    let words = sim
        .command(
            moderator,
            "purge",
            json!([int_option("count", 5), text_option("contains", "x")]),
        )
        .await;
    assert!(words.ephemeral && words.content().contains("need to read messages"));
    assert!(sim.mod_calls().is_empty());

    // Slowmode, lock and unlock of this channel.
    let slow = sim
        .command(moderator, "slowmode", json!([text_option("duration", "30s")]))
        .await;
    assert!(
        slow.description().contains("one message every 30s"),
        "{}",
        slow.description()
    );
    assert_eq!(
        sim.mod_calls(),
        [format!("channel {here} {{\"rate_limit_per_user\":30}}")]
    );
    sim.channel_is(here, 0, None);
    let locked = sim
        .command(moderator, "lock", json!([text_option("reason", "Heated")]))
        .await;
    assert!(!locked.ephemeral && locked.description().contains(&format!("<#{here}> was locked")));
    let calls = sim.mod_calls();
    assert!(
        calls[0].starts_with(&format!("overwrite {here} {}", sim.guild)),
        "{calls:?}"
    );
    assert!(calls[0].contains("deny=\""), "{calls:?}");
    let already = sim.command(moderator, "lock", json!([])).await;
    assert!(already.ephemeral && already.content().contains("already locked"));
    let unlocked = sim.command(moderator, "unlock", json!([])).await;
    assert!(
        unlocked.description().contains("was unlocked"),
        "{}",
        unlocked.description()
    );
    assert_eq!(sim.mod_calls(), [format!("overwrite-delete {here} {}", sim.guild)]);

    // Every case is in the staff log, each with its number; the new reason too.
    let cases = sim.cases().await;
    assert_eq!(cases.first().map(|c| c.number), Some(17));
    let log = sim
        .app
        .db
        .staff_actions(
            sim.guild,
            &StaffFilter {
                limit: 100,
                ..StaffFilter::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(
        log.iter().filter(|l| l.case_number.is_some()).count(),
        18,
        "17 cases and one new reason"
    );
    assert!(log.iter().all(|l| l.source == "command"));

    // A moderator can't take back a warn of their own.
    let own = sim
        .command_resolved(
            mia,
            "warn",
            json!([user_option("user", moderator), text_option("reason", "Rude")]),
            member(moderator, "Sim"),
        )
        .await;
    assert!(own.description().contains("case #18"), "{}", own.description());
    let refused = sim.command(moderator, "unwarn", json!([int_option("case", 18)])).await;
    assert!(
        refused.ephemeral && refused.content().contains("warn of your own"),
        "{}",
        refused.content()
    );
}

#[tokio::test]
async fn the_hierarchy_and_the_permissions_refuse_what_discord_would() {
    let Some(mut sim) = Sim::new("the_hierarchy_and_the_permissions_refuse_what_discord_would").await else {
        return;
    };
    let (moderator, mia, owner) = (unique(), unique(), unique());
    let (mod_role, top_role, managed_role) = (unique(), unique(), unique());
    sim.guild_role(mod_role, "Moderators", 5);
    let top = sim.guild_role(top_role, "Above the bot", 11);
    let managed = role_json(managed_role, "Bot role", 3, true);
    sim.mock.roles.lock().unwrap().push(managed.clone());
    sim.mock.owner.store(owner, Ordering::SeqCst);

    let ban = |user: u64| json!([user_option("user", user)]);
    let peer = someone(mia, "Mia", Some((&[mod_role], 0, None)));
    let above_bot = someone(mia, "Mia", Some((&[top_role], 0, None)));
    let admin = someone(mia, "Mia", Some((&[], ADMINISTRATOR, None)));
    let timeout = |duration: &str| json!([user_option("user", mia), text_option("duration", duration)]);
    // (what, who acts, their permissions, command, options, resolved, what the refusal says)
    let rows: Vec<(&str, u64, u64, &str, Value, Value, &str)> = vec![
        (
            "no Ban Members",
            moderator,
            STAFF & !BAN,
            "ban",
            ban(mia),
            member(mia, "Mia"),
            "Ban Members permission",
        ),
        (
            "no Kick Members",
            moderator,
            STAFF & !KICK,
            "kick",
            ban(mia),
            member(mia, "Mia"),
            "Kick Members permission",
        ),
        (
            "no Timeout Members",
            moderator,
            STAFF & !MODERATE,
            "warn",
            json!([user_option("user", mia), text_option("reason", "x")]),
            member(mia, "Mia"),
            "Timeout Members permission",
        ),
        (
            "no Manage Messages",
            moderator,
            STAFF & !MANAGE_MESSAGES,
            "purge",
            json!([int_option("count", 5)]),
            json!({}),
            "Manage Messages permission",
        ),
        (
            "no Manage Channels",
            moderator,
            STAFF & !MANAGE_CHANNELS,
            "lock",
            json!([]),
            json!({}),
            "Manage Channels permission",
        ),
        (
            "no Manage Nicknames",
            moderator,
            STAFF & !MANAGE_NICKNAMES,
            "nick",
            ban(mia),
            member(mia, "Mia"),
            "Manage Nicknames permission",
        ),
        (
            "no Manage Roles",
            moderator,
            STAFF & !MANAGE_ROLES,
            "role",
            json!([sub("add", vec![user_option("user", mia), role_option("role", &top)])]),
            with_role(member(mia, "Mia"), &top),
            "Manage Roles permission",
        ),
        (
            "an equal role",
            moderator,
            STAFF,
            "ban",
            ban(mia),
            peer.clone(),
            "not below yours",
        ),
        (
            "an equal role, kicked",
            moderator,
            STAFF,
            "kick",
            ban(mia),
            peer,
            "not below yours",
        ),
        (
            "above the bot, by the owner",
            owner,
            STAFF,
            "ban",
            ban(mia),
            above_bot,
            "not below mine",
        ),
        (
            "the owner",
            moderator,
            STAFF,
            "ban",
            ban(owner),
            member(owner, "Boss"),
            "the server owner",
        ),
        (
            "yourself",
            moderator,
            STAFF,
            "warn",
            json!([user_option("user", moderator), text_option("reason", "x")]),
            member(moderator, "Me"),
            "yourself",
        ),
        (
            "the bot",
            moderator,
            STAFF,
            "kick",
            ban(APPLICATION_ID),
            member(APPLICATION_ID, "Pinchora"),
            "myself",
        ),
        (
            "an administrator",
            moderator,
            STAFF,
            "timeout",
            timeout("1h"),
            admin,
            "can't be timed out",
        ),
        (
            "not a member",
            moderator,
            STAFF,
            "kick",
            ban(mia),
            someone(mia, "Mia", None),
            "isn't a member",
        ),
        (
            "no length",
            moderator,
            STAFF,
            "timeout",
            timeout("soon"),
            member(mia, "Mia"),
            "Say how long",
        ),
        (
            "too long",
            moderator,
            STAFF,
            "timeout",
            timeout("30d"),
            member(mia, "Mia"),
            "from a minute to 28 days",
        ),
        (
            "a bad ban length",
            moderator,
            STAFF,
            "ban",
            json!([user_option("user", mia), text_option("duration", "7x")]),
            member(mia, "Mia"),
            "Write the duration",
        ),
        (
            "a role above the bot",
            owner,
            STAFF,
            "role",
            json!([sub("add", vec![user_option("user", mia), role_option("role", &top)])]),
            with_role(member(mia, "Mia"), &top),
            "above my highest role",
        ),
        (
            "a managed role",
            owner,
            STAFF,
            "role",
            json!([sub(
                "add",
                vec![user_option("user", mia), role_option("role", &managed)]
            )]),
            with_role(member(mia, "Mia"), &managed),
            "managed",
        ),
    ];
    for (what, actor, permissions, name, options, resolved, expected) in rows {
        sim.permissions = permissions;
        sim.roles = if actor == owner { Vec::new() } else { vec![mod_role] };
        let reply = sim.command_resolved(actor, name, options, resolved).await;
        assert!(
            reply.ephemeral && reply.content().contains(expected),
            "{what}: {:?}",
            reply.content()
        );
    }
    assert!(sim.mod_calls().is_empty(), "nothing reached Discord");
    assert!(sim.role_changes().is_empty());
    assert!(sim.cases().await.is_empty(), "no case was opened");
    assert!(sim.posts().is_empty(), "nobody was told");

    // Administrator stands for every permission.
    sim.permissions = ADMINISTRATOR;
    sim.roles = vec![mod_role];
    let reply = sim
        .command_resolved(
            moderator,
            "warn",
            json!([user_option("user", mia), text_option("reason", "x")]),
            member(mia, "Mia"),
        )
        .await;
    assert!(
        reply.description().contains("was warned · case #1"),
        "{:?}",
        reply.content()
    );
}

/// The ticker lifts every due temporary ban in the database, whoever's it is (the database tests leave
/// some behind), so tests that run it take turns and only judge their own server's bans.
const BAN_TICKS: fixture_lock::FixtureLock = fixture_lock::FixtureLock(12);

#[tokio::test]
async fn temporary_bans_end_on_the_ticker_with_a_case_of_their_own() {
    let _serial = BAN_TICKS.lock().await;
    let Some(mut sim) = Sim::new("temporary_bans_end_on_the_ticker_with_a_case_of_their_own").await else {
        return;
    };
    let (moderator, mia, modlog, mod_role) = (unique(), unique(), unique(), unique());
    sim.guild_role(mod_role, "Moderators", 5);
    sim.roles = vec![mod_role];
    sim.permissions = STAFF;
    sim.command_resolved(
        moderator,
        "config",
        json!([sub(
            "moderation",
            vec![channel_option("channel", modlog), bool_option("dm", false)]
        )]),
        resolved_channels(&[modlog]),
    )
    .await;
    let banned = sim
        .command_resolved(
            moderator,
            "ban",
            json!([user_option("user", mia), text_option("duration", "1m")]),
            member(mia, "Mia"),
        )
        .await;
    assert!(
        banned.description().contains("banned for 1m · case #1"),
        "{}",
        banned.description()
    );
    assert!(
        sim.posts().iter().all(|p| p.channel != mia),
        "direct messages are off here"
    );
    sim.mod_calls();
    lift_expired_bans(&sim.app).await.unwrap();
    assert!(!sim.mod_calls().contains(&format!("unban {mia}")), "not yet");

    sim.app.shift_clock(chrono::Duration::minutes(2));
    lift_expired_bans(&sim.app).await.unwrap();
    assert!(sim.mod_calls().contains(&format!("unban {mia}")));
    let posts = sim.posts();
    let ended = posts
        .iter()
        .find(|p| p.channel == modlog)
        .expect("the unban in the mod-log");
    assert_eq!(ended.title(), "Case #2 · Unban");
    assert_eq!(ended.field("Reason"), "Temporary ban ended (case #1)");
    assert_eq!(ended.field("Moderator"), "<@1> · Pinchora");
    lift_expired_bans(&sim.app).await.unwrap();
    assert!(!sim.mod_calls().contains(&format!("unban {mia}")), "lifted once");
    assert_eq!(sim.cases().await.len(), 2);
    sim.app.shift_clock(chrono::Duration::minutes(-2));
}
/// `/mod unwarn` and `/mod reason` follow the hierarchy of the case: a junior moderator can neither take back nor
/// reword a senior's warn, the senior can do both to the junior's, and everyone may reword their own.
#[tokio::test]
async fn unwarn_and_reason_follow_the_hierarchy_of_the_case() {
    let Some(mut sim) = Sim::new("unwarn_and_reason_follow_the_hierarchy_of_the_case").await else {
        return;
    };
    let (senior, junior, mia, boss) = (unique(), unique(), unique(), unique());
    let (senior_role, junior_role, boss_role) = (unique(), unique(), unique());
    sim.guild_role(senior_role, "Senior", 8);
    sim.guild_role(junior_role, "Junior", 5);
    sim.guild_role(boss_role, "Boss", 6);
    for (who, role) in [(senior, senior_role), (junior, junior_role), (boss, boss_role)] {
        sim.mock.member_roles.lock().unwrap().insert(who, vec![role]);
    }
    sim.permissions = STAFF;
    let warn = |user: u64| json!([user_option("user", user), text_option("reason", "Spam")]);
    let as_senior = |sim: &mut Sim| sim.roles = vec![senior_role];
    let as_junior = |sim: &mut Sim| sim.roles = vec![junior_role];

    as_senior(&mut sim);
    sim.command_resolved(senior, "warn", warn(mia), member(mia, "Mia"))
        .await;
    let boss_member = someone(boss, "Boss", Some((&[boss_role], 0, None)));
    sim.command_resolved(senior, "warn", warn(boss), boss_member).await;
    as_junior(&mut sim);
    sim.command_resolved(junior, "warn", warn(mia), member(mia, "Mia"))
        .await;

    // The junior and the senior's warn of Mia (case #1), and the warn of someone above the junior (#2).
    let unwarn = |case: i64| json!([int_option("case", case)]);
    let reason = |case: i64| json!([int_option("id", case), text_option("reason", "Reworded")]);
    let refused = sim.command(junior, "unwarn", unwarn(1)).await;
    assert!(
        refused.ephemeral && refused.content().contains(&format!("given by <@{senior}>")),
        "{}",
        refused.content()
    );
    let refused = sim.command(junior, "unwarn", unwarn(2)).await;
    assert!(
        refused.content().contains("their highest role is not below yours"),
        "{}",
        refused.content()
    );
    let refused = sim.command(junior, "reason", reason(1)).await;
    assert!(
        refused.ephemeral
            && refused
                .content()
                .contains(&format!("Only <@{senior}>, who opened case #1")),
        "{}",
        refused.content()
    );
    // Their own case is theirs to reword.
    let own = sim.command(junior, "reason", reason(3)).await;
    assert_eq!((own.ephemeral, own.title()), (true, "Case #3 · Warn"));
    assert_eq!(own.field("Reason"), "Reworded");

    // The senior sits above the junior and both members.
    as_senior(&mut sim);
    let taken = sim.command(senior, "unwarn", unwarn(3)).await;
    assert!(
        taken.description().contains("A warn of **Mia** was removed (case #3)"),
        "{}",
        taken.description()
    );
    let reworded = sim.command(senior, "reason", reason(1)).await;
    assert_eq!(reworded.field("Reason"), "Reworded");
    let warns = sim.cases().await;
    let active: Vec<i32> = warns
        .iter()
        .filter(|c| c.kind == "warn" && c.active)
        .map(|c| c.number)
        .collect();
    assert_eq!(active, [2, 1], "only the junior's warn was taken back");
}

/// A ban Discord refuses after the member was told turns the message into one that says so; a member
/// who switched server DMs off (`/privacy`) is not written to at all.
#[tokio::test]
async fn a_refused_ban_takes_its_message_back_and_server_dms_off_is_honoured() {
    let Some(mut sim) = Sim::new("a_refused_ban_takes_its_message_back_and_server_dms_off_is_honoured").await else {
        return;
    };
    let (moderator, mia, quiet, mod_role) = (unique(), unique(), unique(), unique());
    sim.guild_role(mod_role, "Moderators", 5);
    sim.roles = vec![mod_role];
    sim.permissions = STAFF;
    sim.mock.bans_refused.store(true, Ordering::SeqCst);
    let ban = |user: u64| json!([user_option("user", user), text_option("reason", "Raid")]);
    let refused = sim
        .command_resolved(moderator, "ban", ban(mia), member(mia, "Mia"))
        .await;
    sim.mock.bans_refused.store(false, Ordering::SeqCst);
    assert!(
        refused.ephemeral
            && refused.content().contains("Discord refused the ban")
            && refused.content().contains("told them it did not go through"),
        "{}",
        refused.content()
    );
    let posts = sim.posts();
    let to_mia: Vec<&Post> = posts.iter().filter(|p| p.channel == mia).collect();
    assert_eq!(to_mia.len(), 2, "the message, then its correction");
    assert!(
        to_mia[0].embed()["description"]
            .as_str()
            .unwrap()
            .contains("You were banned from")
    );
    assert_eq!(to_mia[1].method, Method::PATCH);
    assert!(
        to_mia[1].embed()["description"]
            .as_str()
            .unwrap()
            .contains("tried to ban you, but it did not go through")
    );
    assert!(sim.cases().await.is_empty(), "no case for a ban that did not happen");

    sim.command(quiet, "privacy", json!([bool_option("server_dms", false)]))
        .await;
    sim.posts();
    let warned = sim
        .command_resolved(
            moderator,
            "warn",
            json!([user_option("user", quiet), text_option("reason", "Spam")]),
            member(quiet, "Quiet"),
        )
        .await;
    assert!(warned.description().contains("was warned"), "{}", warned.description());
    assert!(
        sim.posts().iter().all(|p| p.channel != quiet),
        "server DMs are off for them"
    );
    assert_eq!(sim.cases().await[0].dm_sent, Some(false));
}

/// `/mod unlock` gives @everyone back exactly what the lock took: here an explicit Send Messages.
#[tokio::test]
async fn unlock_puts_back_what_everyone_had() {
    let Some(mut sim) = Sim::new("unlock_puts_back_what_everyone_had").await else {
        return;
    };
    let moderator = unique();
    sim.permissions = STAFF;
    let here = sim.channel;
    let send = 1u64 << 11;
    let channel = json!({
        "id": here.to_string(), "type": 0, "name": "general", "guild_id": sim.guild.to_string(),
        "permission_overwrites": [{ "id": sim.guild.to_string(), "type": 0, "allow": send.to_string(), "deny": "0" }],
    });
    sim.mock.channels.lock().unwrap().insert(here, channel);
    sim.command(moderator, "lock", json!([])).await;
    let calls = sim.mod_calls();
    assert!(calls[0].contains("allow=\"0\""), "the lock takes the allow: {calls:?}");
    let unlocked = sim.command(moderator, "unlock", json!([])).await;
    assert!(
        unlocked.description().contains("was unlocked"),
        "{}",
        unlocked.description()
    );
    assert_eq!(
        sim.mod_calls(),
        [format!("overwrite {here} {} allow=\"{send}\" deny=\"0\"", sim.guild)],
        "Send Messages is allowed again, nothing denied"
    );
}

/// A temporary ban someone lifted by hand is over: banned again by hand (for good), the member stays
/// banned when the first ban's time comes, whichever of Discord's two signals arrives first.
#[tokio::test]
async fn a_temporary_ban_lifted_by_hand_never_lifts_a_newer_ban() {
    let _serial = BAN_TICKS.lock().await;
    let Some(mut sim) = Sim::new("a_temporary_ban_lifted_by_hand_never_lifts_a_newer_ban").await else {
        return;
    };
    let (moderator, owner, mia, ana, mod_role) = (unique(), unique(), unique(), unique(), unique());
    sim.guild_role(mod_role, "Moderators", 5);
    sim.roles = vec![mod_role];
    sim.permissions = STAFF;
    for who in [mia, ana] {
        sim.command_resolved(
            moderator,
            "ban",
            json!([user_option("user", who), text_option("duration", "1m")]),
            member(who, "Mia"),
        )
        .await;
    }
    let banned = |sim: &Sim, who: u64| sim.mock.bans.lock().unwrap().contains(&who);
    assert!(banned(&sim, mia) && banned(&sim, ana));

    // Mia: unbanned by hand (the gateway's event, then the audit log's entry), then banned again.
    sim.mock.bans.lock().unwrap().retain(|b| *b != mia);
    sim.ban(mia, "mia", false).await;
    sim.audit(23, owner, mia, json!([]), None).await;
    sim.mock.bans.lock().unwrap().push(mia);
    sim.ban(mia, "mia", true).await;
    // Ana: unbanned and banned again before the unban's event arrives; only the audit log can tell.
    sim.mock.bans.lock().unwrap().retain(|b| *b != ana);
    sim.mock.bans.lock().unwrap().push(ana);
    sim.ban(ana, "ana", false).await;
    let standing = sim.cases().await.iter().filter(|c| c.kind == "ban" && c.active).count();
    assert_eq!(standing, 1, "Ana's ban still looks like it stands");
    sim.audit(23, owner, ana, json!([]), None).await;

    sim.mod_calls();
    sim.app.shift_clock(chrono::Duration::minutes(2));
    lift_expired_bans(&sim.app).await.unwrap();
    sim.app.shift_clock(chrono::Duration::minutes(-2));
    assert!(
        sim.cases().await.iter().all(|c| c.kind != "unban"),
        "nothing was lifted here"
    );
    let calls = sim.mod_calls();
    assert!(
        [mia, ana].iter().all(|who| !calls.contains(&format!("unban {who}"))),
        "nobody was unbanned: {calls:?}"
    );
    assert!(banned(&sim, mia) && banned(&sim, ana), "the newer bans stand");
    assert!(sim.cases().await.iter().all(|c| !(c.kind == "ban" && c.active)));
}

#[tokio::test]
async fn the_server_log_reports_messages_audit_entries_and_voice_and_leaves_the_ignored_alone() {
    let Some(mut sim) =
        Sim::new("the_server_log_reports_messages_audit_entries_and_voice_and_leaves_the_ignored_alone").await
    else {
        return;
    };
    let (admin, alice, bob, moderator) = (unique(), unique(), unique(), unique());
    let (log, channels_log, renamed) = (unique(), unique(), unique());
    // The moderator is staff (a role with Manage Channels); Alice is not.
    let staff_role = unique();
    sim.guild_role_with(staff_role, "Staff", 4, MANAGE_CHANNELS);
    sim.mock
        .member_roles
        .lock()
        .unwrap()
        .insert(moderator, vec![staff_role]);
    sim.permissions = MANAGE_GUILD;
    let config = |options: Vec<Value>| json!([sub("log", options)]);
    sim.command_resolved(
        admin,
        "config",
        config(vec![channel_option("channel", log)]),
        resolved_channels(&[log]),
    )
    .await;
    let per_category = sim
        .command_resolved(
            admin,
            "config",
            config(vec![
                channel_option("channel", channels_log),
                text_option("category", "channels"),
            ]),
            resolved_channels(&[channels_log]),
        )
        .await;
    assert!(
        per_category
            .content()
            .contains(&format!("Channels and threads: 7/7 on · <#{channels_log}>")),
        "{}",
        per_category.content()
    );

    // An edit shows the text before and after, a delete what it said, a bulk delete the kept ones.
    let first = sim.said(alice, "hello").await;
    sim.edited(alice, first, "hello there").await;
    let posts = sim.posts();
    let [edit] = &posts[..] else {
        panic!("expected one entry, got {}", posts.len());
    };
    assert_eq!((edit.channel, edit.title()), (log, "Message edited"));
    assert_eq!((edit.field("Before"), edit.field("After")), ("hello", "hello there"));
    assert!(
        edit.description().contains(&format!("/{CHANNEL_ID}/{first})")),
        "a link to it"
    );
    sim.edited(alice, first, "hello there").await;
    assert!(sim.posts().is_empty(), "the same text again is an unfurl, not an edit");
    sim.deleted(&[first]).await;
    let posts = sim.posts();
    assert_eq!(posts[0].title(), "Message deleted");
    assert_eq!(posts[0].field("Message"), "hello there");
    assert!(posts[0].description().contains(&format!("by <@{alice}>")));
    let (one, two) = (sim.said(alice, "first").await, sim.said(bob, "second").await);
    sim.deleted(&[one, two, unique()]).await;
    let posts = sim.posts();
    assert_eq!(posts[0].title(), "Messages bulk deleted");
    let text = posts[0].description();
    assert!(text.starts_with("3 messages were deleted"), "{text}");
    assert!(
        text.contains("**Sim**: first") && text.contains("**Sim**: second"),
        "{text}"
    );

    // The audit log: a channel renamed (to the channels category's own channel), roles given, a
    // timeout by hand (one by Pinchora is left to its case), a reorder (not worth a line).
    sim.audit(
        11,
        moderator,
        renamed,
        json!([{ "key": "name", "old_value": "general", "new_value": "lobby" }]),
        None,
    )
    .await;
    let posts = sim.posts();
    assert_eq!((posts[0].channel, posts[0].title()), (channels_log, "Channel changed"));
    assert!(
        posts[0].description().contains("**Name**: `general` → `lobby`"),
        "{}",
        posts[0].description()
    );
    sim.audit(
        11,
        moderator,
        renamed,
        json!([{ "key": "position", "old_value": 1, "new_value": 2 }]),
        None,
    )
    .await;
    assert!(sim.posts().is_empty());
    sim.audit(
        25,
        moderator,
        alice,
        json!([{ "key": "$add", "new_value": [{ "id": "77", "name": "Helpers" }] }]),
        None,
    )
    .await;
    let posts = sim.posts();
    assert_eq!((posts[0].channel, posts[0].title()), (log, "Member roles changed"));
    assert!(posts[0].description().contains("**Given**: <@&77>"));
    let timeout = json!([{ "key": "communication_disabled_until", "new_value": "2099-01-01T00:00:00.000000+00:00" }]);
    sim.audit(24, moderator, alice, timeout.clone(), Some("cool down"))
        .await;
    let posts = sim.posts();
    assert_eq!(posts[0].title(), "Timeout given or removed");
    assert_eq!(posts[0].field("Reason"), "cool down");
    sim.audit(24, APPLICATION_ID, alice, timeout, None).await;
    assert!(sim.posts().is_empty(), "Pinchora's own timeouts have a case");

    // A ban the audit log saw first names who did it.
    sim.audit(22, moderator, bob, json!([]), Some("raid")).await;
    sim.ban(bob, "bob", true).await;
    let posts = sim.posts();
    assert_eq!(posts[0].title(), "Member banned");
    assert_eq!(
        (posts[0].field("By"), posts[0].field("Reason")),
        (mention(moderator).as_str(), "raid")
    );

    // What the moderator did by hand is in the staff log; Alice's thread is no staff's doing.
    sim.audit(
        110,
        alice,
        unique(),
        json!([{ "key": "name", "new_value": "chat" }]),
        None,
    )
    .await;
    sim.posts();
    let staff = sim
        .app
        .db
        .staff_actions(
            sim.guild,
            &StaffFilter {
                limit: 50,
                ..StaffFilter::default()
            },
        )
        .await
        .unwrap();
    let actions: Vec<&str> = staff.iter().map(|a| a.action.as_str()).collect();
    assert_eq!(
        actions,
        [
            "ban",
            "member_update",
            "member_roles",
            "channel_update",
            "channel_update"
        ],
        "{staff:?}"
    );
    assert!(
        staff
            .iter()
            .all(|a| a.source == "discord" && a.actor_id == moderator as i64)
    );

    // Voice: a server mute is logged; a self mute is off until switched on.
    let lounge = unique();
    sim.voice(alice, "alice", false, Some(lounge), false).await;
    assert_eq!(sim.posts()[0].title(), "Joined voice");
    sim.voice(alice, "alice", false, Some(lounge), true).await;
    assert!(sim.posts().is_empty(), "self mutes start off");
    sim.app
        .db
        .update_log_config(sim.guild, |c| c.toggle(LogEvent::VoiceSelf, true))
        .await
        .unwrap();
    sim.app.announce_guild_change(sim.guild).await;
    sim.voice(alice, "alice", false, Some(lounge), false).await;
    let posts = sim.posts();
    assert!(
        posts[0].description().contains("unmuted themselves"),
        "{}",
        posts[0].description()
    );

    // Ignore lists: a member, and a channel.
    let ignored = sim
        .command_resolved(
            admin,
            "config",
            json!([sub(
                "log-ignore",
                vec![user_option("user", alice), channel_option("channel", renamed)]
            )]),
            json!({
                "users": resolved(&[alice], &[])["users"],
                "channels": resolved_channels(&[renamed])["channels"],
            }),
        )
        .await;
    assert!(
        ignored.content().contains(&format!("<@{alice}>")),
        "{}",
        ignored.content()
    );
    let quiet = sim.said(alice, "psst").await;
    sim.edited(alice, quiet, "psst!").await;
    sim.audit(
        11,
        moderator,
        renamed,
        json!([{ "key": "topic", "new_value": "x" }]),
        None,
    )
    .await;
    assert!(sim.posts().is_empty(), "ignored member and channel");
    let loud = sim.said(bob, "hey").await;
    sim.edited(bob, loud, "hey!").await;
    assert_eq!(sim.posts().len(), 1, "others still are");

    // One event off, as the dashboard's matrix does it; then the whole log module off.
    sim.app
        .db
        .update_log_config(sim.guild, |c| c.toggle(LogEvent::MessageDelete, false))
        .await
        .unwrap();
    sim.app.announce_guild_change(sim.guild).await;
    sim.deleted(&[loud]).await;
    assert!(sim.posts().is_empty());
    sim.command(
        admin,
        "config",
        json!([group(
            "modules",
            sub(
                "toggle",
                vec![text_option("module", "log"), bool_option("enabled", false)]
            )
        )]),
    )
    .await;
    sim.edited(bob, loud, "hey!!").await;
    assert!(sim.posts().is_empty(), "the log module is off");
}

/// With the Message Content intent off the bot reads no chat: Discord still sends the text of a
/// message that mentions it (for text commands), but the server log keeps none of it, and `/purge`
/// refuses the filters that look at what messages say.
#[tokio::test]
async fn without_message_content_the_log_keeps_no_text_and_purge_filters_nothing_by_it() {
    let Some(mut sim) = Sim::with(
        "without_message_content_the_log_keeps_no_text_and_purge_filters_nothing_by_it",
        |app| app.message_content = false,
    )
    .await
    else {
        return;
    };
    let (admin, alice, moderator, log) = (unique(), unique(), unique(), unique());
    sim.permissions = MANAGE_GUILD;
    sim.command_resolved(
        admin,
        "config",
        json!([sub("log", vec![channel_option("channel", log)])]),
        resolved_channels(&[log]),
    )
    .await;

    let first = sim.said(alice, "hello").await;
    // Redis may still hold messages from before Message Content was disabled.
    let cached = json!({
        "author": alice,
        "name": "Alice",
        "content": "private cached message",
        "attachments": ["private-file.txt"]
    });
    let bulk = [unique(), unique()];
    for id in [first, bulk[0], bulk[1]] {
        sim.app
            .cache
            .put(&format!("logmsg:{id}"), &cached.to_string(), Duration::from_secs(60))
            .await
            .unwrap();
    }
    sim.edited(alice, first, "hello there").await;
    let posts = sim.posts();
    let [edit] = &posts[..] else {
        panic!("expected one entry, got {}", posts.len());
    };
    assert_eq!((edit.channel, edit.title()), (log, "Message edited"));
    assert_eq!(
        (edit.field("Before"), edit.field("After")),
        ("", ""),
        "no text either side"
    );
    sim.deleted(&[first]).await;
    let posts = sim.posts();
    assert_eq!(posts[0].title(), "Message deleted");
    assert_eq!(posts[0].field("Message"), "", "nothing was kept");
    assert_eq!(posts[0].field("Attachments"), "", "cached files stay hidden too");
    sim.deleted(&bulk).await;
    let posts = sim.posts();
    assert_eq!(posts.len(), 1);
    assert!(
        !posts[0].description().contains("private cached message"),
        "bulk deletes must not reveal cached text"
    );

    sim.permissions = STAFF;
    let words = sim
        .command(
            moderator,
            "purge",
            json!([int_option("count", 5), text_option("contains", "hello")]),
        )
        .await;
    assert!(words.ephemeral && words.content().contains("need to read messages"));
    assert!(sim.mod_calls().is_empty());
}

/// A gateway unban has no timestamp; neither it nor its late audit entry may close a newer bot ban.
#[tokio::test]
async fn delayed_unban_events_preserve_the_exact_newer_ban() {
    let _serial = BAN_TICKS.lock().await;
    let Some(mut sim) = Sim::new("delayed_unban_events_preserve_the_exact_newer_ban").await else {
        return;
    };
    let (moderator, target, mod_role) = (unique(), unique(), unique());
    sim.guild_role(mod_role, "Moderators", 5);
    sim.roles = vec![mod_role];
    sim.permissions = STAFF;
    sim.command_resolved(
        moderator,
        "ban",
        json!([user_option("user", target), text_option("duration", "1m")]),
        member(target, "Mia"),
    )
    .await;
    let old = sim.cases().await.remove(0);
    // This event belongs to the earlier ban, but is delivered after the replacement is recorded.
    let at = old.created_at;
    sim.mock.bans.lock().unwrap().retain(|b| *b != target);
    sim.command_resolved(
        moderator,
        "ban",
        json!([user_option("user", target)]),
        member(target, "Mia"),
    )
    .await;
    let newer = sim.cases().await.remove(0);
    assert!(newer.created_at > at);
    sim.ban(target, "Mia", false).await;
    let id = (((at.timestamp_millis() - 1_420_070_400_000) as u64) << 22) | 1;
    let event = serde_json::from_value(json!({
        "action_type": 23, "changes": [], "guild_id": sim.guild.to_string(),
        "id": id.to_string(), "target_id": target.to_string(), "user_id": moderator.to_string(),
    }))
    .unwrap();
    on_audit_log_entry(sim.app.clone(), event).await.unwrap();
    assert!(sim.app.db.case(sim.guild, newer.number).await.unwrap().unwrap().active);
    sim.mod_calls();
    sim.app.shift_clock(chrono::Duration::minutes(2));
    lift_expired_bans(&sim.app).await.unwrap();
    sim.app.shift_clock(chrono::Duration::minutes(-2));
    assert!(!sim.mod_calls().iter().any(|call| call == &format!("unban {target}")));
    assert!(sim.mock.bans.lock().unwrap().contains(&target));
}
