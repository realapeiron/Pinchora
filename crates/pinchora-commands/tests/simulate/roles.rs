//! The roles pack: auto roles, sticky roles, role panels in three styles (buttons, a menu, reactions),
//! roles that expire, and colour roles.
//!
//! The shared mock Discord answers the role changes and the member lookups; what the pack adds (making
//! and moving roles, reactions, listing members) is answered here, through [`mock`], and written to a log
//! these tests read back by the id of the message or the server they are about.

use std::{
    collections::HashMap,
    future::Future,
    sync::{LazyLock, Mutex},
    time::Duration,
};

use axum::http::{Method, StatusCode, Uri};
use pinchora_commands::{ReactionEvent, give_after_verify, on_reaction, roles_tick};
use twilight_model::gateway::payload::incoming::{ReactionAdd, ReactionRemove};

use super::*;

/// Calls the shared mock does not keep, as (the server or the message they are about, what happened).
static LOG: LazyLock<Mutex<Vec<(u64, String)>>> = LazyLock::new(Mutex::default);
/// The members `GET /guilds/{id}/members` lists, by server: (member, roles).
static MEMBERS: LazyLock<Mutex<HashMap<u64, Listed>>> = LazyLock::new(Mutex::default);
/// The members of one server, as (member, roles).
type Listed = Vec<(u64, Vec<u64>)>;
/// The ticker claims what is due in any server of the shared database, so the tests that make something
/// come due take turns.
static TICKS: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

fn note(key: u64, text: String) {
    LOG.lock().unwrap().push((key, text));
}

/// What was logged for `key` so far (and forgotten).
fn seen(key: u64) -> Vec<String> {
    let mut log = LOG.lock().unwrap();
    let (mine, rest): (Vec<_>, Vec<_>) = std::mem::take(&mut *log).into_iter().partition(|(k, _)| *k == key);
    *log = rest;
    mine.into_iter().map(|(_, text)| text).collect()
}

/// `%F0%9F%8E%AE` as the emoji it spells.
fn decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && i + 2 < bytes.len()
            && let Ok(byte) = u8::from_str_radix(&text[i + 1..i + 3], 16)
        {
            out.push(byte);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Answers what the roles pack asks Discord besides the role changes the shared mock knows; `None` for
/// everything else.
pub(super) fn mock(
    state: &Mock,
    method: &Method,
    uri: &Uri,
    parts: &[&str],
    body: &[u8],
) -> Option<(StatusCode, String)> {
    let number = |part: &str| part.parse::<u64>().unwrap_or(0);
    let n = parts.len();
    // A role made, moved or deleted.
    if let (&Method::POST, [.., "guilds", guild, "roles"]) = (method, parts) {
        let asked: Value = serde_json::from_slice(body).unwrap_or_default();
        let (id, color) = (unique(), asked["color"].as_u64().unwrap_or(0));
        let mut role = role_json(id, asked["name"].as_str().unwrap_or("role"), 1, false);
        role["color"] = json!(color);
        role["colors"]["primary_color"] = json!(color);
        state.roles.lock().unwrap().push(role.clone());
        note(
            number(guild),
            format!("create {id} {} {color}", asked["name"].as_str().unwrap_or_default()),
        );
        return Some((StatusCode::OK, role.to_string()));
    }
    if let (&Method::PATCH, [.., "guilds", guild, "roles"]) = (method, parts) {
        let asked: Value = serde_json::from_slice(body).unwrap_or_default();
        for entry in asked.as_array().into_iter().flatten() {
            let (id, position) = (
                entry["id"].as_str().unwrap_or_default(),
                entry["position"].as_i64().unwrap_or(0),
            );
            for role in state.roles.lock().unwrap().iter_mut().filter(|r| r["id"] == id) {
                role["position"] = json!(position);
            }
            note(number(guild), format!("move {id} {position}"));
        }
        return Some((
            StatusCode::OK,
            Value::Array(state.roles.lock().unwrap().clone()).to_string(),
        ));
    }
    if let (&Method::DELETE, [.., "guilds", guild, "roles", role]) = (method, parts) {
        state.roles.lock().unwrap().retain(|r| r["id"] != *role);
        note(number(guild), format!("delete {role}"));
        return Some((StatusCode::NO_CONTENT, String::new()));
    }
    // Reactions: `.../messages/{m}/reactions/{emoji}/@me`, `.../{emoji}/{user}`, `.../{emoji}` and all.
    if n >= 4 && parts[n - 2] == "reactions" && n >= 5 && parts[n - 4] == "messages" {
        // `.../messages/{m}/reactions/{emoji}`: every reaction of one emoji.
        if method == Method::DELETE {
            note(number(parts[n - 3]), format!("clear {}", decode(parts[n - 1])));
            return Some((StatusCode::NO_CONTENT, String::new()));
        }
    }
    if n >= 6 && parts[n - 3] == "reactions" && parts[n - 5] == "messages" {
        let (message, emoji, who) = (number(parts[n - 4]), decode(parts[n - 2]), parts[n - 1]);
        match *method {
            Method::PUT => note(message, format!("react {emoji}")),
            Method::DELETE => note(message, format!("unreact {emoji} {who}")),
            _ => return None,
        }
        return Some((StatusCode::NO_CONTENT, String::new()));
    }
    if n >= 4 && parts[n - 1] == "reactions" && parts[n - 3] == "messages" && method == Method::DELETE {
        note(number(parts[n - 2]), "clear all".to_owned());
        return Some((StatusCode::NO_CONTENT, String::new()));
    }
    // The member list, a page at a time.
    if let (&Method::GET, [.., "guilds", guild, "members"]) = (method, parts) {
        let query = uri.query().unwrap_or_default();
        let asked = |name: &str| {
            query
                .split('&')
                .find_map(|pair| pair.strip_prefix(&format!("{name}=")))
                .and_then(|v| v.parse::<u64>().ok())
        };
        let (limit, after) = (asked("limit").unwrap_or(1), asked("after").unwrap_or(0));
        let members = MEMBERS.lock().unwrap().get(&number(guild)).cloned().unwrap_or_default();
        let page: Vec<Value> = members
            .iter()
            .filter(|(user, _)| *user > after)
            .take(limit as usize)
            .map(|(user, roles)| member_json(*user, roles))
            .collect();
        return Some((StatusCode::OK, Value::Array(page).to_string()));
    }
    None
}

/// Waits (up to five seconds) for something a spawned task does.
async fn eventually<F, Fut>(what: &str, check: F)
where
    F: Fn() -> Fut,
    Fut: Future<Output = bool>,
{
    for _ in 0..100 {
        if check().await {
            return;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("never happened: {what}");
}

impl Sim {
    /// The role changes made to `user` since the last look (other people's are left out).
    fn changes_of(&self, user: u64) -> Vec<String> {
        let mine = format!(" {user} ");
        self.role_changes()
            .into_iter()
            .filter(|c| format!("{c} ").contains(&mine))
            .collect()
    }

    /// A member update that says what roles the member holds now.
    async fn update_roles(&self, user: u64, roles: &[u64]) {
        let event: MemberUpdate = serde_json::from_value(json!({
            "guild_id": self.guild.to_string(),
            "user": person_json(user, "Member", Some("Member"), false),
            "nick": null,
            "roles": roles.iter().map(u64::to_string).collect::<Vec<_>>(),
            "avatar": null,
            "communication_disabled_until": null,
            "joined_at": null,
            "deaf": null,
            "mute": null,
            "pending": false,
            "premium_since": null,
        }))
        .expect("member update JSON");
        on_member_update(self.app.clone(), event)
            .await
            .expect("the update handler failed");
    }

    /// Sets what the mock lists for `GET /guilds/{id}/members`.
    fn list_members(&self, members: &[(u64, &[u64])]) {
        MEMBERS
            .lock()
            .unwrap()
            .insert(self.guild, members.iter().map(|(u, r)| (*u, r.to_vec())).collect());
    }

    /// Asks the bot to run the slash command `name` as a manager: `options` is its `data.options`.
    async fn manage(&mut self, user: u64, name: &str, options: Value, roles: &[&Value], channels: &[u64]) -> Reply {
        self.permissions = MANAGE_GUILD;
        let mut known = resolved(&[], roles);
        known["channels"] = resolved_channels(channels)["channels"].clone();
        self.command_resolved(user, name, options, known).await
    }

    /// A reaction on `message`, as the gateway delivers it, with the roles the reacting member holds.
    async fn react(&self, add: bool, user: u64, message: u64, emoji: Value, member_roles: &[u64]) {
        let mut event = json!({
            "burst": false,
            "channel_id": CHANNEL_ID.to_string(),
            "emoji": emoji,
            "guild_id": self.guild.to_string(),
            "message_id": message.to_string(),
            "user_id": user.to_string(),
        });
        if add {
            event["member"] = member_json(user, member_roles);
        }
        let event = if add {
            ReactionEvent::Add(Box::new(
                serde_json::from_value::<ReactionAdd>(event).expect("reaction JSON"),
            ))
        } else {
            ReactionEvent::Remove(Box::new(
                serde_json::from_value::<ReactionRemove>(event).expect("reaction JSON"),
            ))
        };
        on_reaction(self.app.clone(), event)
            .await
            .expect("the reaction handler failed");
    }
}

/// Every option of every menu of a message.
fn all_options(reply: &Reply) -> Vec<SelectMenuOption> {
    reply
        .controls()
        .filter_map(|c| match c {
            Component::SelectMenu(menu) => menu.options.clone(),
            _ => None,
        })
        .flatten()
        .collect()
}

/// Picks `value` in whichever menu of `on` offers it.
async fn pick_colour(sim: &mut Sim, user: u64, on: &Reply, value: &str) -> Reply {
    let menu = on
        .controls()
        .find_map(|c| match c {
            Component::SelectMenu(menu) if menu.options.iter().flatten().any(|o| o.value == value) => {
                Some(menu.custom_id.clone())
            }
            _ => None,
        })
        .unwrap_or_else(|| panic!("no menu offers {value:?}"));
    sim.component(user, &menu, &[value], on).await
}

fn unicode(name: &str) -> Value {
    json!({ "id": null, "name": name })
}

fn custom(id: u64, name: &str) -> Value {
    json!({ "id": id.to_string(), "name": name, "animated": false })
}

/// `/autoroles add` for `role` with more options.
fn add_auto(role: &Value, extra: Vec<Value>) -> Value {
    let mut options = vec![role_option("role", role)];
    options.extend(extra);
    json!([sub("add", options)])
}

#[tokio::test]
async fn auto_roles_are_given_at_once_after_a_delay_and_taken_back() {
    let Some(mut sim) = Sim::new("auto_roles_are_given_at_once_after_a_delay_and_taken_back").await else {
        return;
    };
    let _ticks = TICKS.lock().await;
    let (admin, alice, bob, robot, late) = (unique(), unique(), unique(), unique(), unique());
    sim.own_server(admin);
    let (member_id, waiting_id, bots_id, gate_id) = (unique(), unique(), unique(), unique());
    let member_role = sim.guild_role(member_id, "Member", 2);
    let waiting_role = sim.guild_role(waiting_id, "Newcomer", 3);
    let bots_role = sim.guild_role(bots_id, "Bots", 4);
    let gate_role = sim.guild_role(gate_id, "Verified", 5);
    let boss = role_json(unique(), "Boss", BOT_ROLE_POSITION + 5, false);

    // Only managers set auto roles up.
    sim.permissions = 0;
    let denied = sim
        .command_resolved(
            alice,
            "autoroles",
            add_auto(&member_role, vec![]),
            resolved(&[], &[&member_role]),
        )
        .await;
    assert!(
        denied.ephemeral && denied.content().contains("Manage Server"),
        "{}",
        denied.content()
    );
    assert!(sim.app.db.auto_roles(sim.guild).await.unwrap().is_empty());

    // Three auto roles: one at once, one that waits ten minutes and goes after a day, one for bots.
    let at_once = sim
        .manage(admin, "autoroles", add_auto(&member_role, vec![]), &[&member_role], &[])
        .await;
    assert!(
        at_once
            .content()
            .contains(&format!("<@&{member_id}> · Members who join"))
            && at_once.content().contains("(1/10)"),
        "{}",
        at_once.content()
    );
    let waiting = sim
        .manage(
            admin,
            "autoroles",
            add_auto(
                &waiting_role,
                vec![text_option("delay", "10m"), text_option("remove_after", "1d")],
            ),
            &[&waiting_role],
            &[],
        )
        .await;
    assert!(
        waiting.content().contains("after 10m") && waiting.content().contains("lasts 1d"),
        "{}",
        waiting.content()
    );
    sim.manage(
        admin,
        "autoroles",
        add_auto(&bots_role, vec![text_option("kind", "bot")]),
        &[&bots_role],
        &[],
    )
    .await;

    // What it will not take: a role above the bot, a time nobody can read, one too short.
    for (role, extra, expect) in [
        (&boss, vec![], "above my highest role"),
        (
            &member_role,
            vec![text_option("delay", "soon")],
            "couldn't read the delay",
        ),
        (&member_role, vec![text_option("delay", "5s")], "as short as 30s"),
        (&member_role, vec![text_option("remove_after", "10s")], "as short as 1m"),
        (
            &member_role,
            vec![text_option("remove_after", "2y")],
            "couldn't read the remove_after",
        ),
    ] {
        let refused = sim
            .manage(admin, "autoroles", add_auto(role, extra), &[role], &[])
            .await;
        assert!(refused.content().contains(expect), "{expect}: {}", refused.content());
    }
    assert_eq!(
        sim.app.db.auto_roles(sim.guild).await.unwrap().len(),
        3,
        "nothing refused was saved"
    );

    let listed = sim
        .manage(admin, "autoroles", json!([sub("list", vec![])]), &[], &[])
        .await;
    assert_eq!(listed.title(), "Auto roles");
    assert!(
        listed
            .description()
            .contains(&format!("<@&{bots_id}> · Bots that join"))
            && listed
                .description()
                .contains(&format!("<@&{waiting_id}> · Members who join · after 10m · lasts 1d")),
        "{}",
        listed.description()
    );
    assert!(listed.embeds[0].footer.as_ref().unwrap().text.starts_with("3/10"));

    // A member who joins gets the first role at once; the second waits.
    sim.join(alice, "Alice", false, None).await;
    assert_eq!(sim.changes_of(alice), [format!("PUT {alice} {member_id}")]);
    sim.join(robot, "Beep", true, None).await;
    assert_eq!(
        sim.changes_of(robot),
        [format!("PUT {robot} {bots_id}")],
        "a bot gets the bots' role only"
    );
    roles_tick(&sim.app).await.unwrap();
    assert!(sim.changes_of(alice).is_empty(), "ten minutes have not passed");
    sim.app.shift_clock(chrono::Duration::minutes(11));
    roles_tick(&sim.app).await.unwrap();
    assert_eq!(sim.changes_of(alice), [format!("PUT {alice} {waiting_id}")]);
    roles_tick(&sim.app).await.unwrap();
    assert!(sim.changes_of(alice).is_empty(), "given once");
    // A day after it was given, it is taken back.
    sim.app.shift_clock(chrono::Duration::hours(23));
    roles_tick(&sim.app).await.unwrap();
    assert!(sim.changes_of(alice).is_empty(), "not a day yet");
    sim.app.shift_clock(chrono::Duration::hours(2));
    roles_tick(&sim.app).await.unwrap();
    assert_eq!(sim.changes_of(alice), [format!("DELETE {alice} {waiting_id}")]);

    // Somebody who leaves while their role waits never gets it.
    sim.join(bob, "Bob", false, None).await;
    sim.changes_of(bob);
    sim.leave(bob, "Bob", false).await;
    sim.app.shift_clock(chrono::Duration::minutes(11));
    roles_tick(&sim.app).await.unwrap();
    assert!(sim.changes_of(bob).is_empty(), "a leaver's waiting roles are forgotten");

    // The roles for members who pass the join gate are given when the gate lets them in, not on joining.
    sim.manage(
        admin,
        "autoroles",
        add_auto(&gate_role, vec![text_option("kind", "after_verify")]),
        &[&gate_role],
        &[],
    )
    .await;
    sim.join(late, "Late", false, None).await;
    assert_eq!(
        sim.changes_of(late),
        [format!("PUT {late} {member_id}")],
        "joining gives the join roles only"
    );
    give_after_verify(&sim.app, sim.guild, late).await;
    assert_eq!(sim.changes_of(late), [format!("PUT {late} {gate_id}")]);

    // Switched off with the module, nothing is given.
    let toggle = |on: bool| {
        json!([group(
            "modules",
            sub(
                "toggle",
                vec![text_option("module", "autoroles"), bool_option("enabled", on)]
            )
        )])
    };
    sim.manage(admin, "config", toggle(false), &[], &[]).await;
    let (quiet, quiet_two) = (unique(), unique());
    sim.join(quiet, "Quiet", false, None).await;
    give_after_verify(&sim.app, sim.guild, quiet_two).await;
    assert!(sim.changes_of(quiet).is_empty() && sim.changes_of(quiet_two).is_empty());
    sim.manage(admin, "config", toggle(true), &[], &[]).await;

    // At most ten, and the one at the limit can still change.
    for n in 0..6 {
        let extra = sim.guild_role(unique(), &format!("Extra {n}"), 2);
        let reply = sim
            .manage(admin, "autoroles", add_auto(&extra, vec![]), &[&extra], &[])
            .await;
        assert!(
            reply.content().contains(&format!("({}/10)", 5 + n)),
            "{}",
            reply.content()
        );
    }
    let one_too_many = sim.guild_role(unique(), "Too many", 2);
    let full = sim
        .manage(
            admin,
            "autoroles",
            add_auto(&one_too_many, vec![]),
            &[&one_too_many],
            &[],
        )
        .await;
    assert!(
        full.content().contains("A server can have 10 auto roles"),
        "{}",
        full.content()
    );
    let again = sim
        .manage(
            admin,
            "autoroles",
            add_auto(&member_role, vec![text_option("delay", "1h")]),
            &[&member_role],
            &[],
        )
        .await;
    assert!(
        again.content().contains("after 1h") && again.content().contains("(1"),
        "{}",
        again.content()
    );

    // Taking one away, by kind or all kinds.
    let removed = sim
        .manage(
            admin,
            "autoroles",
            json!([sub("remove", vec![role_option("role", &bots_role)])]),
            &[],
            &[],
        )
        .await;
    assert!(
        removed.content().contains(&format!("<@&{bots_id}> is no longer given")),
        "{}",
        removed.content()
    );
    let gone = sim
        .manage(
            admin,
            "autoroles",
            json!([sub("remove", vec![role_option("role", &bots_role)])]),
            &[],
            &[],
        )
        .await;
    assert!(gone.content().contains("isn't an auto role"), "{}", gone.content());
}

#[tokio::test]
async fn sticky_roles_come_back_when_a_member_returns() {
    let Some(mut sim) = Sim::new("sticky_roles_come_back_when_a_member_returns").await else {
        return;
    };
    let _ticks = TICKS.lock().await;
    let (admin, dana, carol, newcomer) = (unique(), unique(), unique(), unique());
    sim.own_server(admin);
    let (ok_one, ok_two, high, powerful, managed, gone) = (unique(), unique(), unique(), unique(), unique(), unique());
    sim.guild_role(ok_one, "Gamer", 2);
    sim.guild_role(ok_two, "Artist", 3);
    sim.guild_role(high, "Above me", BOT_ROLE_POSITION + 5);
    sim.guild_role_with(powerful, "Admin", 4, 1 << 3);
    sim.mock
        .roles
        .lock()
        .unwrap()
        .push(role_json(managed, "Booster", 5, true));

    // Off, nothing is kept.
    sim.update_roles(dana, &[ok_one]).await;
    sim.leave(dana, "Dana", false).await;
    sim.join(dana, "Dana", false, None).await;
    assert!(sim.changes_of(dana).is_empty());

    // Carol held a role before sticky roles were switched on and never changed since: the snapshot knows.
    sim.list_members(&[(carol, &[ok_one, ok_two]), (newcomer, &[])]);
    let on = sim
        .manage(
            admin,
            "autoroles",
            json!([sub("sticky", vec![bool_option("enabled", true)])]),
            &[],
            &[],
        )
        .await;
    assert!(on.content().contains("Sticky roles are **on**"), "{}", on.content());
    let pool = sim.app.db.pool().clone();
    let guild = sim.guild;
    eventually("the snapshot notes the members already here", || {
        let pool = pool.clone();
        async move {
            let rows: Vec<(Vec<i64>,)> =
                sqlx::query_as("SELECT role_ids FROM sticky_roles WHERE guild_id = $1 AND user_id = $2")
                    .bind(guild as i64)
                    .bind(carol as i64)
                    .fetch_all(&pool)
                    .await
                    .unwrap();
            rows.first()
                .is_some_and(|(roles,)| roles == &vec![ok_one as i64, ok_two as i64])
        }
    })
    .await;
    // A second `on` the same minute does not start another snapshot, and says the same.
    let again = sim
        .manage(
            admin,
            "autoroles",
            json!([sub("sticky", vec![bool_option("enabled", true)])]),
            &[],
            &[],
        )
        .await;
    assert!(again.content().contains("Sticky roles are **on**"));

    // Dana's roles change: everything she holds is what comes back, as far as the bot can give it.
    sim.update_roles(dana, &[ok_one, ok_two, high, powerful, managed, gone])
        .await;
    sim.leave(dana, "Dana", false).await;
    sim.leave(carol, "Carol", false).await;
    sim.join(dana, "Dana", false, None).await;
    assert_eq!(
        sim.changes_of(dana),
        [format!("PUT {dana} {ok_one}"), format!("PUT {dana} {ok_two}")],
        "not the role above the bot, the one with admin powers, the integration's, or the deleted one"
    );
    sim.join(carol, "Carol", false, None).await;
    assert_eq!(
        sim.changes_of(carol),
        [format!("PUT {carol} {ok_one}"), format!("PUT {carol} {ok_two}")]
    );
    // Taken once: the same member leaving and returning with nothing new holds nothing.
    sim.leave(dana, "Dana", false).await;
    sim.join(dana, "Dana", false, None).await;
    assert!(
        sim.changes_of(dana).is_empty(),
        "nothing was noted since the first return"
    );

    // A role whose time ran out while the member was away is not given back.
    sim.update_roles(dana, &[ok_one, ok_two]).await;
    sim.app
        .db
        .schedule_removal(sim.guild, dana, ok_two, sim.app.now() - chrono::Duration::minutes(5))
        .await
        .unwrap();
    sim.leave(dana, "Dana", false).await;
    sim.join(dana, "Dana", false, None).await;
    assert_eq!(sim.changes_of(dana), [format!("PUT {dana} {ok_one}")]);

    // Roles kept for 60 days only.
    sim.update_roles(dana, &[ok_one]).await;
    sim.leave(dana, "Dana", false).await;
    sim.app.shift_clock(chrono::Duration::days(61));
    sim.join(dana, "Dana", false, None).await;
    assert!(sim.changes_of(dana).is_empty(), "too long ago");
    sim.app.shift_clock(-chrono::Duration::days(61));

    // Off again forgets everything and keeps nothing new.
    sim.update_roles(carol, &[ok_one]).await;
    let off = sim
        .manage(
            admin,
            "autoroles",
            json!([sub("sticky", vec![bool_option("enabled", false)])]),
            &[],
            &[],
        )
        .await;
    assert!(off.content().contains("Sticky roles are **off**"), "{}", off.content());
    let (left_over,): (i64,) = sqlx::query_as("SELECT count(*) FROM sticky_roles WHERE guild_id = $1")
        .bind(sim.guild as i64)
        .fetch_one(sim.app.db.pool())
        .await
        .unwrap();
    assert_eq!(left_over, 0);
    sim.update_roles(carol, &[ok_one]).await;
    sim.leave(carol, "Carol", false).await;
    sim.join(carol, "Carol", false, None).await;
    assert!(sim.changes_of(carol).is_empty());
}

/// Posts the panel `title` in `channel` and returns the message as a member sees it.
async fn post_panel(sim: &mut Sim, admin: u64, title: &str, channel: u64) -> (Post, Reply) {
    let reply = sim
        .manage(
            admin,
            "roles",
            json!([sub(
                "post",
                vec![text_option("panel", title), channel_option("channel", channel)]
            )]),
            &[],
            &[channel],
        )
        .await;
    assert!(reply.content().starts_with("Posted"), "{}", reply.content());
    let mut posts = sim.posts();
    let post = posts.remove(0);
    let message = posted_reply(&post);
    (post, message)
}

fn panel_cmd(title: &str, extra: Vec<Value>) -> Value {
    let mut options = vec![text_option("title", title)];
    options.extend(extra);
    json!([sub("panel", options)])
}

fn add_role_cmd(panel: &str, role: &Value, label: &str, extra: Vec<Value>) -> Value {
    let mut options = vec![
        text_option("panel", panel),
        role_option("role", role),
        text_option("label", label),
    ];
    options.extend(extra);
    json!([sub("add", options)])
}

#[tokio::test]
async fn a_menu_panel_follows_its_minimum_maximum_and_required_role() {
    let Some(mut sim) = Sim::new("a_menu_panel_follows_its_minimum_maximum_and_required_role").await else {
        return;
    };
    let (admin, alice) = (unique(), unique());
    sim.own_server(admin);
    let lounge = unique();
    let (chess_id, go_id, shogi_id, vip_id) = (unique(), unique(), unique(), unique());
    let chess = sim.guild_role(chess_id, "Chess", 2);
    let go = sim.guild_role(go_id, "Go", 3);
    let shogi = sim.guild_role(shogi_id, "Shogi", 4);
    let vip = sim.guild_role(vip_id, "VIP", 5);

    let made = sim
        .manage(
            admin,
            "roles",
            panel_cmd(
                "Games",
                vec![
                    text_option("style", "select"),
                    int_option("min", 1),
                    int_option("max", 2),
                    text_option("description", "What do you play?"),
                ],
            ),
            &[],
            &[],
        )
        .await;
    assert!(
        made.content().contains("**Games**") && made.content().contains("Style **menu**"),
        "{}",
        made.content()
    );
    assert!(made.content().contains("at most 2"), "{}", made.content());
    // Rules that make no sense are refused before anything is saved.
    let bad = sim
        .manage(admin, "roles", panel_cmd("Games", vec![int_option("min", 3)]), &[], &[])
        .await;
    assert!(
        bad.content().contains("fewest roles can't be more than the most"),
        "{}",
        bad.content()
    );
    let panel = sim.app.db.find_panel(sim.guild, "Games").await.unwrap().unwrap();
    assert_eq!((panel.min_picks, panel.max_picks), (1, 2));

    for (role, label, extra) in [
        (
            &chess,
            "Chess",
            vec![text_option("emoji", "♟️"), text_option("description", "Plays chess")],
        ),
        (&go, "Go", vec![text_option("description", "Plays go")]),
        (&shogi, "Shogi", vec![]),
    ] {
        sim.manage(admin, "roles", add_role_cmd("Games", role, label, extra), &[role], &[])
            .await;
    }
    let (post, message) = post_panel(&mut sim, admin, "Games", lounge).await;
    assert!(
        post.body["components"][0]["components"][0]["type"] == 3,
        "a menu, not buttons"
    );
    let menu = message.menu("rs:");
    assert_eq!(menu.custom_id, format!("rs:{}", panel.id));
    assert_eq!((menu.min_values, menu.max_values), (Some(1), Some(2)));
    let options = message.options("rs:");
    assert_eq!(options.len(), 3);
    assert_eq!(options[0].description.as_deref(), Some("Plays chess"));
    assert!(options[0].emoji.is_some() && options[2].description.is_none());
    assert!(post.description().contains("**Chess** · "), "{}", post.description());
    let id = menu.custom_id.clone();

    // Picking sets what the member holds of the panel.
    let first = sim.component(alice, &id, &[&chess_id.to_string()], &message).await;
    assert!(
        first.ephemeral && first.content() == format!("Added <@&{chess_id}>."),
        "{}",
        first.content()
    );
    assert_eq!(sim.role_changes(), [format!("PUT {alice} {chess_id}")]);
    sim.roles = vec![chess_id];
    let swapped = sim
        .component(alice, &id, &[&go_id.to_string(), &shogi_id.to_string()], &message)
        .await;
    assert!(
        swapped
            .content()
            .contains(&format!("Added <@&{go_id}>, <@&{shogi_id}>."))
            && swapped.content().contains(&format!("Removed <@&{chess_id}>.")),
        "{}",
        swapped.content()
    );
    let mut changes = sim.role_changes();
    changes.sort();
    let mut wanted = vec![
        format!("DELETE {alice} {chess_id}"),
        format!("PUT {alice} {go_id}"),
        format!("PUT {alice} {shogi_id}"),
    ];
    wanted.sort();
    assert_eq!(changes, wanted);
    sim.roles = vec![go_id, shogi_id];

    // Past the limit, under the floor, and a role that is not on the panel.
    let many = sim
        .component(
            alice,
            &id,
            &[&chess_id.to_string(), &go_id.to_string(), &shogi_id.to_string()],
            &message,
        )
        .await;
    assert!(many.content().contains("at most 2 roles"), "{}", many.content());
    let none = sim.component(alice, &id, &[], &message).await;
    assert!(none.content().contains("at least 1"), "{}", none.content());
    let forged = sim.component(alice, &id, &[&unique().to_string()], &message).await;
    assert!(forged.content().contains("isn't on this panel"), "{}", forged.content());
    let elsewhere = sim
        .component(alice, "rs:999999999", &[&go_id.to_string()], &message)
        .await;
    assert!(elsewhere.content().contains("panel is gone"), "{}", elsewhere.content());
    let junk = sim.component(alice, "rs:x", &["y"], &message).await;
    assert!(junk.content().contains("expired"), "{}", junk.content());
    assert!(sim.role_changes().is_empty(), "refused picks change nothing");

    // A required role closes the panel to everyone else.
    sim.manage(
        admin,
        "roles",
        panel_cmd("Games", vec![role_option("required", &vip)]),
        &[&vip],
        &[],
    )
    .await;
    sim.roles = vec![];
    let shut = sim.component(alice, &id, &[&chess_id.to_string()], &message).await;
    assert!(
        shut.content()
            .contains(&format!("You need <@&{vip_id}> to use this panel")),
        "{}",
        shut.content()
    );
    assert!(sim.role_changes().is_empty());
    sim.roles = vec![vip_id];
    sim.component(alice, &id, &[&chess_id.to_string()], &message).await;
    assert_eq!(sim.role_changes(), [format!("PUT {alice} {chess_id}")]);
    // @everyone takes the need away again.
    let everyone = role_json(sim.guild, "@everyone", 0, false);
    sim.manage(
        admin,
        "roles",
        panel_cmd("Games", vec![role_option("required", &everyone)]),
        &[&everyone],
        &[],
    )
    .await;
    assert!(
        sim.app
            .db
            .find_panel(sim.guild, "Games")
            .await
            .unwrap()
            .unwrap()
            .required_role
            .is_none()
    );

    // The message says what it asks.
    sim.manage(
        admin,
        "roles",
        panel_cmd("Games", vec![role_option("required", &vip)]),
        &[&vip],
        &[],
    )
    .await;
    sim.manage(
        admin,
        "roles",
        json!([sub(
            "post",
            vec![text_option("panel", "Games"), channel_option("channel", lounge)]
        )]),
        &[],
        &[lounge],
    )
    .await;
    let posts = sim.posts();
    assert!(
        posts[0].description().ends_with(&format!("Needs <@&{vip_id}>")),
        "{}",
        posts[0].description()
    );
}

#[tokio::test]
async fn button_panels_with_rules_swap_and_cap() {
    let Some(mut sim) = Sim::new("button_panels_with_rules_swap_and_cap").await else {
        return;
    };
    let (admin, alice) = (unique(), unique());
    sim.own_server(admin);
    let lounge = unique();
    let (red_id, blue_id, green_id) = (unique(), unique(), unique());
    let (red, blue, green) = (
        sim.guild_role(red_id, "Red", 2),
        sim.guild_role(blue_id, "Blue", 3),
        sim.guild_role(green_id, "Green", 4),
    );

    // One at a time, and always one.
    sim.manage(
        admin,
        "roles",
        panel_cmd("Teams", vec![bool_option("exclusive", true), int_option("min", 1)]),
        &[],
        &[],
    )
    .await;
    for (role, label) in [(&red, "Red"), (&blue, "Blue"), (&green, "Green")] {
        sim.manage(admin, "roles", add_role_cmd("Teams", role, label, vec![]), &[role], &[])
            .await;
    }
    let (post, message) = post_panel(&mut sim, admin, "Teams", lounge).await;
    let ids: Vec<_> = post.body["components"][0]["components"]
        .as_array()
        .unwrap()
        .iter()
        .map(|b| b["custom_id"].as_str().unwrap().to_owned())
        .collect();
    assert!(
        ids.iter().all(|id| id.starts_with("rb:")),
        "a panel with rules sends the newer buttons: {ids:?}"
    );
    assert!(
        post.embeds_footer().contains("one at a time"),
        "{}",
        post.embeds_footer()
    );

    let first = sim.click(alice, &message, "Red").await;
    assert_eq!(first.content(), format!("Added <@&{red_id}>."));
    assert_eq!(sim.role_changes(), [format!("PUT {alice} {red_id}")]);
    sim.roles = vec![red_id];
    let swap = sim.click(alice, &message, "Blue").await;
    assert!(
        swap.content().contains(&format!("Added <@&{blue_id}>."))
            && swap.content().contains(&format!("Removed <@&{red_id}>.")),
        "{}",
        swap.content()
    );
    assert_eq!(
        sim.role_changes(),
        [format!("DELETE {alice} {red_id}"), format!("PUT {alice} {blue_id}")],
        "the old one goes first"
    );
    sim.roles = vec![blue_id];
    let floor = sim.click(alice, &message, "Blue").await;
    assert!(floor.content().contains("at least 1"), "{}", floor.content());
    assert!(sim.role_changes().is_empty());

    // The old `rr` buttons follow the panel's rules too, so a message posted before the rules still obeys.
    let old = format!(
        "rr:{}:{green_id}",
        sim.app.db.find_panel(sim.guild, "Teams").await.unwrap().unwrap().id
    );
    sim.component(alice, &old, &[], &message).await;
    assert_eq!(
        sim.role_changes(),
        [format!("DELETE {alice} {blue_id}"), format!("PUT {alice} {green_id}")]
    );

    // A cap instead: up to two of three.
    sim.manage(admin, "roles", panel_cmd("Perks", vec![int_option("max", 2)]), &[], &[])
        .await;
    for (role, label) in [(&red, "Red"), (&blue, "Blue"), (&green, "Green")] {
        sim.manage(admin, "roles", add_role_cmd("Perks", role, label, vec![]), &[role], &[])
            .await;
    }
    let (_, perks) = post_panel(&mut sim, admin, "Perks", unique()).await;
    sim.roles = vec![red_id, blue_id];
    let capped = sim.click(alice, &perks, "Green").await;
    assert!(capped.content().contains("at most 2 roles"), "{}", capped.content());
    assert!(sim.role_changes().is_empty());
    let drop = sim.click(alice, &perks, "Red").await;
    assert_eq!(
        drop.content(),
        format!("Removed <@&{red_id}>."),
        "taking one off is always fine"
    );
}

impl Post {
    fn embeds_footer(&self) -> String {
        self.embed()["footer"]["text"].as_str().unwrap_or_default().to_owned()
    }
}

#[tokio::test]
async fn panel_roles_can_expire() {
    let Some(mut sim) = Sim::new("panel_roles_can_expire").await else {
        return;
    };
    let _ticks = TICKS.lock().await;
    let (admin, alice) = (unique(), unique());
    sim.own_server(admin);
    let lounge = unique();
    let (trial_id, other_id) = (unique(), unique());
    let (trial, other) = (
        sim.guild_role(trial_id, "Trial", 2),
        sim.guild_role(other_id, "Other", 3),
    );
    sim.manage(admin, "roles", panel_cmd("Passes", vec![]), &[], &[]).await;
    sim.manage(
        admin,
        "roles",
        add_role_cmd("Passes", &trial, "Trial", vec![]),
        &[&trial],
        &[],
    )
    .await;
    sim.manage(
        admin,
        "roles",
        add_role_cmd("Passes", &other, "Other", vec![]),
        &[&other],
        &[],
    )
    .await;

    let temp = |role: &Value, duration: &str, panel: Option<&str>| {
        let mut options = vec![role_option("role", role), text_option("duration", duration)];
        options.extend(panel.map(|p| text_option("panel", p)));
        json!([sub("temp", options)])
    };
    for (duration, expect) in [
        ("soon", "couldn't read that time"),
        ("10s", "at least 1m"),
        ("3y", "couldn't read"),
    ] {
        let refused = sim.manage(admin, "roles", temp(&trial, duration, None), &[], &[]).await;
        assert!(refused.content().contains(expect), "{duration}: {}", refused.content());
    }
    let missing = sim
        .manage(admin, "roles", temp(&trial, "1h", Some("Nope")), &[], &[])
        .await;
    assert!(missing.content().contains("no panel called"), "{}", missing.content());
    let stray = sim.guild_role(unique(), "Stray", 2);
    let nowhere = sim.manage(admin, "roles", temp(&stray, "1h", None), &[], &[]).await;
    assert!(
        nowhere.content().contains("isn't on any panel"),
        "{}",
        nowhere.content()
    );
    let set = sim
        .manage(admin, "roles", temp(&trial, "12h", Some("Passes")), &[], &[])
        .await;
    assert!(set.content().contains("now lasts 12h"), "{}", set.content());

    let (post, message) = post_panel(&mut sim, admin, "Passes", lounge).await;
    assert!(post.description().contains("· ⏳ 12h"), "{}", post.description());
    let taken = sim.click(alice, &message, "Trial").await;
    assert!(
        taken.content().contains(&format!("Added <@&{trial_id}>."))
            && taken.content().contains("goes away again after 12h"),
        "{}",
        taken.content()
    );
    sim.click(alice, &message, "Other").await;
    assert_eq!(
        sim.changes_of(alice),
        [format!("PUT {alice} {trial_id}"), format!("PUT {alice} {other_id}")]
    );

    // Twelve hours later the role is taken back, the other one stays.
    sim.app.shift_clock(chrono::Duration::hours(11));
    roles_tick(&sim.app).await.unwrap();
    assert!(sim.changes_of(alice).is_empty());
    sim.app.shift_clock(chrono::Duration::hours(2));
    roles_tick(&sim.app).await.unwrap();
    assert_eq!(sim.changes_of(alice), [format!("DELETE {alice} {trial_id}")]);

    // Taking it off by hand cancels the timer.
    sim.app.shift_clock(-chrono::Duration::hours(13));
    sim.roles = vec![];
    sim.click(alice, &message, "Trial").await;
    sim.roles = vec![trial_id];
    sim.click(alice, &message, "Trial").await;
    sim.changes_of(alice);
    sim.app.shift_clock(chrono::Duration::hours(13));
    roles_tick(&sim.app).await.unwrap();
    assert!(sim.changes_of(alice).is_empty(), "nothing left to take back");

    // `off` makes it last for good again.
    let off = sim.manage(admin, "roles", temp(&trial, "off", None), &[], &[]).await;
    assert!(off.content().contains("lasts for good"), "{}", off.content());
    assert_eq!(
        sim.app
            .db
            .panel_role(
                sim.guild,
                sim.app.db.find_panel(sim.guild, "Passes").await.unwrap().unwrap().id,
                trial_id
            )
            .await
            .unwrap()
            .unwrap()
            .temp_secs,
        None
    );
}

#[tokio::test]
async fn reaction_panels_give_and_take_roles_with_the_emojis() {
    let Some(mut sim) = Sim::new("reaction_panels_give_and_take_roles_with_the_emojis").await else {
        return;
    };
    let (admin, alice, bob) = (unique(), unique(), unique());
    sim.own_server(admin);
    let lounge = unique();
    let (bell_id, game_id, blob_id, vip_id) = (unique(), unique(), unique(), unique());
    let (bell, game, blob, vip) = (
        sim.guild_role(bell_id, "Pings", 2),
        sim.guild_role(game_id, "Gamers", 3),
        sim.guild_role(blob_id, "Blobs", 4),
        sim.guild_role(vip_id, "VIP", 5),
    );
    sim.manage(
        admin,
        "roles",
        panel_cmd("Pings", vec![text_option("style", "reactions")]),
        &[],
        &[],
    )
    .await;
    sim.manage(
        admin,
        "roles",
        add_role_cmd("Pings", &bell, "Pings", vec![text_option("emoji", "🔔")]),
        &[&bell],
        &[],
    )
    .await;
    // A reactions panel needs an emoji on every role.
    sim.manage(
        admin,
        "roles",
        add_role_cmd("Pings", &game, "Gamers", vec![]),
        &[&game],
        &[],
    )
    .await;
    let refused = sim
        .manage(
            admin,
            "roles",
            json!([sub(
                "post",
                vec![text_option("panel", "Pings"), channel_option("channel", lounge)]
            )]),
            &[],
            &[lounge],
        )
        .await;
    assert!(
        refused.content().contains("needs an emoji on every role") && refused.content().contains("Gamers"),
        "{}",
        refused.content()
    );
    assert!(sim.posts().is_empty());
    sim.manage(
        admin,
        "roles",
        add_role_cmd("Pings", &game, "Gamers", vec![text_option("emoji", "🎮")]),
        &[&game],
        &[],
    )
    .await;
    sim.manage(
        admin,
        "roles",
        add_role_cmd("Pings", &blob, "Blobs", vec![text_option("emoji", "<:blob:555>")]),
        &[&blob],
        &[],
    )
    .await;

    let (post, _) = post_panel(&mut sim, admin, "Pings", lounge).await;
    assert!(
        post.body
            .get("components")
            .is_none_or(|c| c.as_array().is_none_or(Vec::is_empty)),
        "a reactions panel has no buttons"
    );
    assert!(
        post.embeds_footer().starts_with("React with an emoji"),
        "{}",
        post.embeds_footer()
    );
    let message = post.message;
    let mut put = seen(message);
    put.sort();
    assert_eq!(
        put,
        ["react blob:555", "react 🎮", "react 🔔"],
        "the bot puts every emoji under the message"
    );
    let stored = sim
        .app
        .db
        .panel_reactions(sim.app.db.find_panel(sim.guild, "Pings").await.unwrap().unwrap().id)
        .await
        .unwrap();
    assert_eq!(stored.len(), 3);

    // A reaction gives the role, taking it back takes the role.
    sim.react(true, alice, message, unicode("🎮"), &[]).await;
    assert_eq!(sim.role_changes(), [format!("PUT {alice} {game_id}")]);
    sim.react(true, bob, message, custom(555, "blob"), &[]).await;
    assert_eq!(
        sim.role_changes(),
        [format!("PUT {bob} {blob_id}")],
        "a custom emoji is known by its id"
    );
    sim.react(false, alice, message, unicode("🎮"), &[]).await;
    assert_eq!(sim.role_changes(), [format!("DELETE {alice} {game_id}")]);
    // The same emoji with its variation selector is the same emoji.
    sim.react(true, alice, message, unicode("🔔"), &[]).await;
    sim.role_changes();

    // Other emojis, the bot's own reactions, and other messages change nothing.
    sim.react(true, alice, message, unicode("🍕"), &[]).await;
    sim.react(true, APPLICATION_ID, message, unicode("🔔"), &[]).await;
    sim.react(true, alice, unique(), unicode("🔔"), &[]).await;
    assert!(sim.role_changes().is_empty());
    assert!(seen(message).is_empty());

    // One at a time: the new role replaces the old one, and the old emoji comes off the message.
    sim.manage(
        admin,
        "roles",
        panel_cmd("Pings", vec![bool_option("exclusive", true)]),
        &[],
        &[],
    )
    .await;
    sim.manage(
        admin,
        "roles",
        json!([sub(
            "post",
            vec![text_option("panel", "Pings"), channel_option("channel", lounge)]
        )]),
        &[],
        &[lounge],
    )
    .await;
    let edited = sim.posts();
    assert_eq!(edited[0].method, Method::PATCH, "the same message is edited in place");
    assert_eq!(edited[0].message, message);
    seen(message);
    sim.react(true, alice, message, unicode("🎮"), &[bell_id]).await;
    assert_eq!(
        sim.role_changes(),
        [format!("DELETE {alice} {bell_id}"), format!("PUT {alice} {game_id}")]
    );
    assert_eq!(seen(message), [format!("unreact 🔔 {alice}")]);

    // A required role: without it the reaction comes off and nothing is given.
    sim.manage(
        admin,
        "roles",
        panel_cmd("Pings", vec![role_option("required", &vip)]),
        &[&vip],
        &[],
    )
    .await;
    sim.manage(
        admin,
        "roles",
        json!([sub(
            "post",
            vec![text_option("panel", "Pings"), channel_option("channel", lounge)]
        )]),
        &[],
        &[lounge],
    )
    .await;
    sim.posts();
    seen(message);
    sim.react(true, bob, message, unicode("🎮"), &[]).await;
    assert!(sim.role_changes().is_empty());
    assert_eq!(seen(message), [format!("unreact 🎮 {bob}")]);
    sim.react(true, bob, message, unicode("🎮"), &[vip_id]).await;
    assert_eq!(sim.role_changes(), [format!("PUT {bob} {game_id}")]);

    // Moving a role off the panel takes its emoji off the message when the panel is posted again.
    sim.manage(
        admin,
        "roles",
        json!([sub(
            "remove",
            vec![text_option("panel", "Pings"), role_option("role", &blob)]
        )]),
        &[],
        &[],
    )
    .await;
    seen(message);
    sim.manage(
        admin,
        "roles",
        json!([sub(
            "post",
            vec![text_option("panel", "Pings"), channel_option("channel", lounge)]
        )]),
        &[],
        &[lounge],
    )
    .await;
    sim.posts();
    let after = seen(message);
    assert!(after.contains(&"clear e:555".to_owned()), "{after:?}");
}

#[tokio::test]
async fn colour_roles_are_one_at_a_time_and_made_on_demand() {
    let Some(mut sim) = Sim::new("colour_roles_are_one_at_a_time_and_made_on_demand").await else {
        return;
    };
    let (admin, alice, bob) = (unique(), unique(), unique());
    sim.own_server(admin);
    let guild = sim.guild;
    let (booster_id, anchor_id) = (unique(), unique());
    let booster = sim.guild_role(booster_id, "Booster", 2);
    let anchor = sim.guild_role(anchor_id, "Colours below", 6);
    let above = role_json(unique(), "Too high", BOT_ROLE_POSITION + 3, false);

    // Off until a manager switches it on.
    sim.permissions = 0;
    let off = sim.command(alice, "color", json!([sub("pick", vec![])])).await;
    assert!(
        off.ephemeral && off.content().contains("off on this server"),
        "{}",
        off.content()
    );
    assert!(
        !off.content().contains("/color setup"),
        "members are not told how to switch it on"
    );
    sim.permissions = MANAGE_GUILD;
    let off = sim.command(admin, "color", json!([sub("pick", vec![])])).await;
    assert!(off.content().contains("/color setup enabled:true"), "{}", off.content());
    let denied = {
        sim.permissions = 0;
        sim.command(
            alice,
            "color",
            json!([sub("setup", vec![bool_option("enabled", true)])]),
        )
        .await
    };
    assert!(
        denied.ephemeral && denied.content().contains("Manage Server"),
        "{}",
        denied.content()
    );

    let on = sim
        .manage(
            admin,
            "color",
            json!([sub("setup", vec![bool_option("enabled", true)])]),
            &[],
            &[],
        )
        .await;
    assert!(
        on.content().contains("**on**") && on.content().contains("0/32"),
        "{}",
        on.content()
    );
    // An anchor above the bot cannot hold new roles.
    let refused = sim
        .manage(
            admin,
            "color",
            json!([sub("setup", vec![role_option("anchor", &above)])]),
            &[&above],
            &[],
        )
        .await;
    assert!(
        refused.content().contains("above my highest role"),
        "{}",
        refused.content()
    );

    // The menu offers the palette; picking one makes its role, gives it, and shows the pick.
    sim.permissions = 0;
    let menu = sim.command(alice, "color", json!([sub("pick", vec![])])).await;
    assert!(menu.ephemeral && !menu.files.is_empty(), "a private card");
    assert_eq!(menu.title(), "Colour your name");
    let offered = all_options(&menu);
    assert!(offered.len() >= 16 && offered.iter().any(|o| o.value == "ff6b4a" && o.label == "Coral"));
    assert!(menu.has_button("Take my colour off") && menu.button("Take my colour off").disabled);
    let coral = pick_colour(&mut sim, alice, &menu, "ff6b4a").await;
    assert!(
        coral.description().contains("Your name is now **Coral**"),
        "{}",
        coral.description()
    );
    assert!(!coral.files.is_empty(), "the card is drawn again with the pick marked");
    assert!(!coral.button("Take my colour off").disabled);
    let made = seen(guild);
    assert_eq!(made.len(), 1, "{made:?}");
    let coral_id: u64 = made[0].split(' ').nth(1).unwrap().parse().unwrap();
    assert!(made[0].ends_with(&format!("Coral {}", 0xff6b4a_u32)), "{}", made[0]);
    assert_eq!(sim.changes_of(alice), [format!("PUT {alice} {coral_id}")]);
    let rows = sim.app.db.color_roles(guild).await.unwrap();
    assert_eq!((rows[0].role_id as u64, rows[0].hex.as_str()), (coral_id, "#ff6b4a"));

    // Another colour replaces the first: the old role comes off.
    sim.roles = vec![coral_id];
    let menu = sim.command(alice, "color", json!([sub("pick", vec![])])).await;
    assert!(
        all_options(&menu).iter().any(|o| o.default && o.value == "ff6b4a"),
        "the member's colour is marked"
    );
    let sky = pick_colour(&mut sim, alice, &menu, "8fd3ff").await;
    assert!(sky.description().contains("**Sky**"), "{}", sky.description());
    let made = seen(guild);
    let sky_id: u64 = made[0].split(' ').nth(1).unwrap().parse().unwrap();
    assert_eq!(
        sim.changes_of(alice),
        [format!("DELETE {alice} {coral_id}"), format!("PUT {alice} {sky_id}")]
    );
    // A colour somebody else has already made is not made twice.
    sim.roles = vec![];
    let again = sim.command(bob, "color", json!([sub("pick", vec![])])).await;
    pick_colour(&mut sim, bob, &again, "ff6b4a").await;
    assert!(seen(guild).is_empty(), "Coral's role exists");
    assert_eq!(sim.changes_of(bob), [format!("PUT {bob} {coral_id}")]);

    // The button takes the colour off.
    sim.roles = vec![sky_id];
    let menu = sim.command(alice, "color", json!([sub("pick", vec![])])).await;
    let removed = sim.click(alice, &menu, "Take my colour off").await;
    assert!(
        removed.description().contains("Your colour is off"),
        "{}",
        removed.description()
    );
    assert_eq!(sim.changes_of(alice), [format!("DELETE {alice} {sky_id}")]);
    sim.roles = vec![];
    let none = sim.command(alice, "color", json!([sub("remove", vec![])])).await;
    assert!(none.content().contains("no colour to take off"), "{}", none.content());

    // `/color list` shows the card without a menu.
    let list = sim.command(alice, "color", json!([sub("list", vec![])])).await;
    assert!(list.ephemeral && !list.files.is_empty() && list.components.is_empty());

    // A hex code needs the any-colour mode, a readable code, and a real one.
    let palette_only = sim
        .command(alice, "color", json!([sub("set", vec![text_option("hex", "#2288ff")])]))
        .await;
    assert!(
        palette_only.content().contains("only offers its own colours"),
        "{}",
        palette_only.content()
    );
    sim.manage(
        admin,
        "color",
        json!([sub("setup", vec![text_option("mode", "hex")])]),
        &[],
        &[],
    )
    .await;
    sim.permissions = 0;
    for (code, expect) in [("zzz", "isn't a colour code"), ("#050505", "too dark")] {
        let bad = sim
            .command(alice, "color", json!([sub("set", vec![text_option("hex", code)])]))
            .await;
        assert!(bad.content().contains(expect), "{code}: {}", bad.content());
    }
    let any = sim
        .command(alice, "color", json!([sub("set", vec![text_option("hex", "#2288ff")])]))
        .await;
    assert!(
        any.content().contains("Your name is now **#2288FF**"),
        "{}",
        any.content()
    );
    let made = seen(guild);
    assert!(
        made[0].contains("#2288FF") && made[0].ends_with(&format!("{}", 0x2288ff_u32)),
        "{made:?}"
    );
    let menu = sim.command(alice, "color", json!([sub("pick", vec![])])).await;
    assert!(
        all_options(&menu).iter().any(|o| o.value == "2288ff"),
        "a typed colour joins the menu"
    );
    sim.changes_of(alice);

    // Only members with an allowed role may pick.
    sim.manage(
        admin,
        "color",
        json!([sub("setup", vec![role_option("allow", &booster)])]),
        &[],
        &[],
    )
    .await;
    sim.permissions = 0;
    sim.roles = vec![];
    let shut = sim.command(alice, "color", json!([sub("pick", vec![])])).await;
    assert!(
        shut.content().contains(&format!("Only members with <@&{booster_id}>")),
        "{}",
        shut.content()
    );
    sim.roles = vec![booster_id];
    let open = sim.command(alice, "color", json!([sub("pick", vec![])])).await;
    assert!(!open.files.is_empty());
    sim.manage(
        admin,
        "color",
        json!([sub("setup", vec![role_option("disallow", &booster)])]),
        &[],
        &[],
    )
    .await;

    // New roles are made right under the anchor.
    let set = sim
        .manage(
            admin,
            "color",
            json!([sub("setup", vec![role_option("anchor", &anchor)])]),
            &[&anchor],
            &[],
        )
        .await;
    assert!(set.content().contains(&format!("<@&{anchor_id}>")), "{}", set.content());
    sim.permissions = 0;
    sim.roles = vec![];
    let menu = sim.command(bob, "color", json!([sub("pick", vec![])])).await;
    pick_colour(&mut sim, bob, &menu, "06d6a0").await;
    let made = seen(guild);
    let mint_id: u64 = made[0].split(' ').nth(1).unwrap().parse().unwrap();
    assert!(made.contains(&format!("move {mint_id} 6")), "{made:?}");
    sim.changes_of(bob);

    // The server's own roles can be colours too: they keep their name and colour.
    let ruby_id = unique();
    let mut ruby = sim.guild_role(ruby_id, "Ruby", 3);
    ruby["colors"]["primary_color"] = json!(0xe0115f);
    sim.mock.roles.lock().unwrap().retain(|r| r["id"] != ruby["id"]);
    sim.mock.roles.lock().unwrap().push(ruby.clone());
    let colourless = sim.guild_role(unique(), "Plain", 3);
    let adopted = sim
        .manage(
            admin,
            "color",
            json!([sub(
                "setup",
                vec![text_option(
                    "roles",
                    &format!(
                        "<@&{ruby_id}> <@&{}> <@&{}>",
                        colourless["id"].as_str().unwrap(),
                        unique()
                    )
                )]
            )]),
            &[],
            &[],
        )
        .await;
    assert!(
        adopted
            .content()
            .contains(&format!("<@&{ruby_id}> is a colour role now (#e0115f)")),
        "{}",
        adopted.content()
    );
    assert!(adopted.content().contains("has no colour"), "{}", adopted.content());
    assert!(
        adopted.content().contains("couldn't find a role"),
        "{}",
        adopted.content()
    );
    sim.permissions = 0;
    let menu = sim.command(alice, "color", json!([sub("pick", vec![])])).await;
    assert!(
        all_options(&menu)
            .iter()
            .any(|o| o.label == "Ruby" && o.value == "e0115f"),
        "the server's own colour is on the menu"
    );
    let ruby_pick = pick_colour(&mut sim, alice, &menu, "e0115f").await;
    assert!(
        ruby_pick.description().contains("**Ruby**"),
        "{}",
        ruby_pick.description()
    );
    assert!(seen(guild).is_empty(), "its role already exists");
    sim.changes_of(alice);

    // A server holds 32 colour roles at most.
    let rows = sim.app.db.color_roles(guild).await.unwrap().len();
    for n in rows..32 {
        sim.app
            .db
            .add_color_role(guild, 1_000 + n as u64, &format!("#10{n:02x}30"), "Filler", 32)
            .await
            .unwrap();
    }
    let full = sim
        .command(alice, "color", json!([sub("set", vec![text_option("hex", "#99aa11")])]))
        .await;
    assert!(full.content().contains("32 colour roles"), "{}", full.content());
    assert!(seen(guild).is_empty(), "nothing was made");
}

#[tokio::test]
async fn colour_roles_say_what_discord_refused() {
    let Some(mut sim) = Sim::new("colour_roles_say_what_discord_refused").await else {
        return;
    };
    let (admin, alice) = (unique(), unique());
    sim.own_server(admin);
    sim.manage(
        admin,
        "color",
        json!([sub("setup", vec![bool_option("enabled", true)])]),
        &[],
        &[],
    )
    .await;
    sim.permissions = 0;
    // The member is asked for a role the bot cannot give: Discord says no and the member is told why.
    let menu = sim.command(alice, "color", json!([sub("pick", vec![])])).await;
    sim.allow_role_changes(false);
    let refused = pick_colour(&mut sim, alice, &menu, "ff6b4a").await;
    assert!(
        refused.content().contains("couldn't give you") && refused.content().contains("highest role"),
        "{}",
        refused.content()
    );
    sim.allow_role_changes(true);
    // Stale or forged menu values give nothing.
    let junk = sim.component(alice, "cr:s:0", &["zzzzzz"], &menu).await;
    assert!(junk.content().contains("expired"), "{}", junk.content());
    let upper = sim.component(alice, "cr:s:0", &["FF6B4A"], &menu).await;
    assert!(upper.content().contains("expired"), "{}", upper.content());
}
