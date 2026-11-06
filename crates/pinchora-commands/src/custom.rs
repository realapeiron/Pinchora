//! Custom commands and auto-responders at run time: the caches they read, the message a template becomes
//! (every user-written text goes through one scrubbing pipeline, here), and the things they do besides
//! answering. `/tag`, the event responders and the dashboard's "test" all build their messages with
//! [`render_message`], so what the dashboard shows is what the bot sends.

use std::{sync::Arc, time::Duration};

use moka::future::Cache as Moka;
use pinchora_core::{
    modules::Module,
    settings::run_time_refusal,
    tags::{
        ACTION_DMS_PER_HOUR, ACTION_POINTS_DAILY_MAX, Action, EMBED_AUTHOR_MAX, EMBED_DESCRIPTION_MAX,
        EMBED_FIELD_NAME_MAX, EMBED_FIELD_VALUE_MAX, EMBED_FOOTER_MAX, EMBED_TITLE_MAX, EMBED_TOTAL_MAX, EmbedSpec,
        RESPONSE_MAX, Vars, render, truncate,
    },
};
use pinchora_db::TagSet;
use serde_json::{Map, Value};
use twilight_http::request::AuditLogReason;
use twilight_model::{
    channel::message::{AllowedMentions, Embed},
    id::{
        Id,
        marker::{ChannelMarker, MessageMarker, UserMarker},
    },
    user::User,
};
use twilight_util::builder::embed::{
    EmbedAuthorBuilder, EmbedBuilder, EmbedFieldBuilder, EmbedFooterBuilder, ImageSource,
};

use crate::{
    App,
    util::{BRAND, avatar_url, display_name, mention, strip_mentions, thousands},
};

/// How long the bot keeps a server's commands and responders before asking the database again.
const KEEP: Duration = Duration::from_secs(60);
/// How long the bot remembers a member's roles, for the responders that watch for a role.
const ROLES_KEPT: Duration = Duration::from_secs(24 * 3_600);

/// A server's name, size and owner as Discord has them.
pub(crate) struct GuildFacts {
    pub name: String,
    pub members: Option<u64>,
    pub owner: u64,
}

/// What the bot keeps about custom commands and auto-responders.
pub(crate) struct CustomState {
    sets: Moka<u64, Arc<TagSet>>,
    facts: Moka<u64, Arc<GuildFacts>>,
    /// The roles a member last had, to tell what changed on a role update.
    remembered: Moka<(u64, u64), Arc<Vec<u64>>>,
}

impl CustomState {
    pub fn new() -> Self {
        Self {
            sets: Moka::builder().max_capacity(20_000).time_to_live(KEEP).build(),
            facts: Moka::builder()
                .max_capacity(20_000)
                .time_to_live(Duration::from_secs(300))
                .build(),
            remembered: Moka::builder().max_capacity(250_000).time_to_idle(ROLES_KEPT).build(),
        }
    }

    /// Forgets a server's copy, so the next command reads what the dashboard just saved.
    pub async fn forget(&self, guild: u64) {
        self.sets.invalidate(&guild).await;
    }

    /// Notes a member's roles; returns what they were before, if the bot had seen them.
    pub async fn swap_roles(&self, guild: u64, user: u64, roles: Vec<u64>) -> Option<Arc<Vec<u64>>> {
        let before = self.remembered.get(&(guild, user)).await;
        self.remembered.insert((guild, user), Arc::new(roles)).await;
        before
    }

    /// Notes a member's roles unless the bot already knows them.
    pub async fn seed_roles(&self, guild: u64, user: u64, roles: Vec<u64>) {
        if self.remembered.get(&(guild, user)).await.is_none() {
            self.remembered.insert((guild, user), Arc::new(roles)).await;
        }
    }
}

impl App {
    /// A server's custom commands and auto-responders, cached for a minute (the dashboard drops the copy
    /// the moment it saves).
    pub(crate) async fn tag_set(&self, guild: u64) -> anyhow::Result<Arc<TagSet>> {
        self.custom
            .sets
            .try_get_with(guild, async { self.db.tag_set(guild).await.map(Arc::new) })
            .await
            .map_err(|e| anyhow::anyhow!("{e:#}"))
    }

    /// The server's name, member count and owner, cached for five minutes; `None` when Discord cannot be
    /// asked.
    pub(crate) async fn guild_facts(&self, guild: u64) -> Option<Arc<GuildFacts>> {
        self.custom
            .facts
            .try_get_with(guild, async {
                let found = self
                    .http
                    .guild(Id::new(guild.max(1)))
                    .with_counts(true)
                    .await?
                    .model()
                    .await?;
                anyhow::Ok(Arc::new(GuildFacts {
                    name: found.name,
                    members: found.approximate_member_count,
                    owner: found.owner_id.get(),
                }))
            })
            .await
            .map_err(|error| tracing::debug!(%error, guild, "server lookup failed"))
            .ok()
    }

    /// Whether a server has `module` switched on at all, whatever channel rules it has: for what happens
    /// to the server as a whole. A lookup that fails reads as on.
    pub(crate) async fn module_enabled(&self, guild: u64, module: Module) -> bool {
        self.guild_modules(guild)
            .await
            .map_or(true, |rules| rules.rule(module).enabled)
    }

    /// `channel` followed by what it sits under, when the server's lists name any of them.
    pub(crate) async fn named_lineage(&self, channel: u64, named: &[&[u64]]) -> Vec<u64> {
        let listed = |id: u64| named.iter().any(|list| list.contains(&id));
        self.lineage(channel, !named.iter().any(|list| !list.is_empty()), listed)
            .await
    }
}

// /Scrubbing

/// Text a member typed (or a name), made safe to put into a message: mentions and broadcasts gone, and
/// markdown escaped so it shows as typed.
pub fn scrub_value(text: &str) -> String {
    escape_markdown(&strip_mentions(text))
}

/// Markdown escaped with backslashes, which Discord hides: a name or what a member typed cannot make a
/// masked link (`[Free Nitro](…)`), bold, a spoiler, code, a heading, a list or a quote of its own.
pub fn escape_markdown(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 8);
    for c in text.chars() {
        if matches!(
            c,
            '\\' | '*' | '_' | '~' | '`' | '|' | '>' | '<' | '[' | ']' | '(' | ')' | '#' | '-'
        ) {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

/// `@everyone` and `@here` without their `@`, in any case; nothing else changes.
fn defang(text: &str) -> String {
    let mut out = text.to_owned();
    for word in ["@everyone", "@here"] {
        // ASCII lowercasing keeps byte offsets, so positions carry over to the original.
        while let Some(at) = out.to_ascii_lowercase().find(word) {
            out.replace_range(at..=at, "");
        }
    }
    out
}

/// A template made safe: with mentions off, everything that could ping is dropped; with them on, still
/// never everyone or here.
fn scrub_template(text: &str, allow_mentions: bool) -> String {
    if allow_mentions {
        defang(text)
    } else {
        strip_mentions(text)
    }
}

/// The users and roles a text mentions by `<@id>`, `<@!id>` and `<@&id>`.
pub fn mention_ids(text: &str) -> (Vec<u64>, Vec<u64>) {
    let (mut users, mut roles) = (Vec::new(), Vec::new());
    let mut rest = text;
    while let Some(at) = rest.find("<@") {
        let after = &rest[at + 2..];
        let (role, digits) = match after.strip_prefix('&') {
            Some(d) => (true, d),
            None => (false, after.strip_prefix('!').unwrap_or(after)),
        };
        let end = digits.find(|c: char| !c.is_ascii_digit()).unwrap_or(digits.len());
        if end > 0 && digits[end..].starts_with('>') {
            if let Ok(id) = digits[..end].parse::<u64>() {
                let list = if role { &mut roles } else { &mut users };
                if !list.contains(&id) {
                    list.push(id);
                }
            }
            rest = &digits[end + 1..];
        } else {
            rest = &rest[at + 2..];
        }
    }
    (users, roles)
}

// /Messages

/// What a command or responder says, before it is filled in.
pub struct Spec<'a> {
    pub responses: &'a [String],
    pub embed: Option<&'a EmbedSpec>,
    pub allow_mentions: bool,
}

/// A message after its placeholders were filled and its texts made safe.
#[derive(Debug, Clone, PartialEq)]
pub struct Rendered {
    pub content: String,
    pub embed: Option<EmbedSpec>,
    /// The users and roles the template itself names, which the message may ping when the server allows it.
    pub users: Vec<u64>,
    pub roles: Vec<u64>,
}

/// Every template of a spec, to see which placeholders are in use before they are looked up.
pub fn templates<'a>(spec: &'a Spec<'a>) -> Vec<&'a str> {
    let mut texts: Vec<&str> = spec.responses.iter().map(String::as_str).collect();
    if let Some(embed) = spec.embed {
        texts.extend(
            [&embed.title, &embed.description, &embed.author, &embed.footer]
                .into_iter()
                .flatten()
                .map(String::as_str),
        );
        for field in &embed.fields {
            texts.push(&field.name);
            texts.push(&field.value);
        }
    }
    texts
}

/// The texts of the direct messages among `actions`, which are filled like the reply is.
pub fn action_texts(actions: &[Action]) -> Vec<&str> {
    actions
        .iter()
        .filter_map(|a| match a {
            Action::Dm { text } => Some(text.as_str()),
            _ => None,
        })
        .collect()
}

/// Every template of a spec and of the direct messages among `actions`.
pub(crate) fn all_texts<'a>(spec: &'a Spec<'a>, actions: &'a [Action]) -> Vec<&'a str> {
    let mut texts = templates(spec);
    texts.extend(action_texts(actions));
    texts
}

/// Picks one of the replies and fills it, and the embed, with `vars`. `pick(n)` chooses an index below
/// `n` for the reply and for every `{random:…}`.
pub fn render_message(spec: &Spec<'_>, vars: &Vars, pick: &mut dyn FnMut(usize) -> usize) -> Rendered {
    let allow = spec.allow_mentions;
    let reply = spec
        .responses
        .get(pick(spec.responses.len().max(1)))
        .map_or("", String::as_str);
    let clean = scrub_template(reply, allow);
    let (users, roles) = if allow {
        mention_ids(&clean)
    } else {
        (Vec::new(), Vec::new())
    };
    let content = truncate(render(&clean, vars, pick).trim(), RESPONSE_MAX);
    let embed = spec.embed.map(|embed| {
        let mut embed = embed.clone();
        fill_embed(&mut embed, &mut |text, max| {
            let clean = scrub_template(text, allow);
            *text = truncate(render(&clean, vars, &mut *pick).trim(), max);
        });
        embed
    });
    Rendered {
        content,
        embed,
        users,
        roles,
    }
}

/// Fills every text of an embed with `fill(text, longest allowed)`, then trims the embed to Discord's
/// total if the filled texts ran over it.
fn fill_embed(embed: &mut EmbedSpec, fill: &mut dyn FnMut(&mut String, usize)) {
    if let Some(text) = &mut embed.title {
        fill(text, EMBED_TITLE_MAX);
    }
    if let Some(text) = &mut embed.description {
        fill(text, EMBED_DESCRIPTION_MAX);
    }
    if let Some(text) = &mut embed.author {
        fill(text, EMBED_AUTHOR_MAX);
    }
    if let Some(text) = &mut embed.footer {
        fill(text, EMBED_FOOTER_MAX);
    }
    for field in &mut embed.fields {
        fill(&mut field.name, EMBED_FIELD_NAME_MAX);
        fill(&mut field.value, EMBED_FIELD_VALUE_MAX);
    }
    // Filling can only grow a text to its own limit, but the limits together are above Discord's total.
    let over = embed.total_chars().saturating_sub(EMBED_TOTAL_MAX);
    if over > 0
        && let Some(text) = &mut embed.description
    {
        let keep = text.chars().count().saturating_sub(over);
        *text = truncate(text, keep.max(1));
    }
}

/// The rendered message as Discord takes it: the text, the embed, and who may be pinged (the member the
/// message is about, and when the server allows it the users and roles the template names; never everyone,
/// and never the server's own `@everyone` role).
pub(crate) fn to_discord(rendered: &Rendered, guild: u64, subject: u64) -> (String, Option<Embed>, AllowedMentions) {
    let mut users: Vec<Id<UserMarker>> = vec![Id::new(subject.max(1))];
    users.extend(
        rendered
            .users
            .iter()
            .filter(|u| **u != subject)
            .map(|u| Id::new((*u).max(1))),
    );
    let roles = rendered
        .roles
        .iter()
        .filter(|r| **r != guild)
        .map(|r| Id::new((*r).max(1)))
        .collect();
    let mentions = AllowedMentions {
        parse: Vec::new(),
        replied_user: false,
        roles,
        users,
    };
    (
        rendered.content.clone(),
        rendered.embed.as_ref().map(embed_of),
        mentions,
    )
}

/// An embed spec as a Discord embed.
pub(crate) fn embed_of(spec: &EmbedSpec) -> Embed {
    let mut builder = EmbedBuilder::new().color(spec.color_value().unwrap_or(BRAND));
    if let Some(title) = &spec.title {
        builder = builder.title(title);
    }
    if let Some(description) = &spec.description {
        builder = builder.description(description);
    }
    if let Some(url) = &spec.url {
        builder = builder.url(url);
    }
    if let Some(author) = &spec.author {
        builder = builder.author(EmbedAuthorBuilder::new(author));
    }
    if let Some(footer) = &spec.footer {
        builder = builder.footer(EmbedFooterBuilder::new(footer));
    }
    if let Some(source) = spec.image.as_deref().and_then(|u| ImageSource::url(u).ok()) {
        builder = builder.image(source);
    }
    if let Some(source) = spec.thumbnail.as_deref().and_then(|u| ImageSource::url(u).ok()) {
        builder = builder.thumbnail(source);
    }
    for field in &spec.fields {
        let mut f = EmbedFieldBuilder::new(&field.name, &field.value);
        if field.inline {
            f = f.inline();
        }
        builder = builder.field(f);
    }
    builder.build()
}

// /Placeholders

/// Who a message is about and where it happens: what the placeholders are filled from.
pub(crate) struct Scene {
    pub guild: u64,
    pub channel: Option<u64>,
    pub user: u64,
    pub name: String,
    pub avatar: String,
    pub args: String,
    /// Placeholders only this kind of message has (`level`, `role`, `count`), already worded.
    pub extra: Vec<(&'static str, String)>,
}

impl Scene {
    /// A scene about `user`, as Discord shows them in the server.
    pub fn of(guild: u64, channel: Option<u64>, user: &User, nick: Option<&str>) -> Self {
        Self {
            guild,
            channel,
            user: user.id.get(),
            name: display_name(user, nick),
            avatar: avatar_url(user),
            args: String::new(),
            extra: Vec::new(),
        }
    }
}

/// The values for a scene. The server, its size and the points are only looked up when a template uses
/// them; `balance` is the member's points when an action just changed them.
pub(crate) async fn scene_vars(app: &App, scene: &Scene, templates: &[&str], balance: Option<i64>) -> Vars {
    let uses = |needle: &str| templates.iter().any(|t| t.contains(needle));
    let mut vars = Vars::new();
    vars.set("user", mention(scene.user))
        .set("user.name", scrub_value(&scene.name))
        .set("user.id", scene.user.to_string())
        .set("user.avatar", scene.avatar.clone())
        .set("channel", scene.channel.map(|c| format!("<#{c}>")).unwrap_or_default());
    vars.args(&scrub_value(&scene.args));
    for (key, value) in &scene.extra {
        vars.set(key, value.clone());
    }
    if uses("{server") || uses("{count}") {
        let facts = app.guild_facts(scene.guild).await;
        vars.set(
            "server",
            facts
                .as_ref()
                .map_or_else(|| "this server".to_owned(), |f| escape_markdown(&f.name)),
        );
        let members = facts.and_then(|f| f.members).map(|m| m.to_string()).unwrap_or_default();
        vars.set("server.members", members.clone());
        if !vars.has("count") {
            vars.set("count", members);
        }
    }
    if uses("{points") {
        if let Ok(settings) = app.guild_settings(scene.guild).await {
            vars.set("points.name", escape_markdown(&settings.points_name));
        }
        let points = match balance {
            Some(points) => Some(points),
            None if uses("{points}") => app
                .db
                .member_points(scene.guild, scene.user)
                .await
                .ok()
                .map(|(points, _)| points),
            None => None,
        };
        vars.set("points", points.map(|p| p.to_string()).unwrap_or_default());
    }
    vars
}

/// A fair pick for `render_message`.
pub fn pick(n: usize) -> usize {
    rand::random_range(0..n.max(1))
}

// /Actions

/// How long a role change an auto-responder made is remembered, so the member update it causes does not
/// set role responders off again.
const OWN_CHANGE: Duration = Duration::from_secs(15);

/// What the actions of a command or responder need to know about who they act on.
pub(crate) struct Actor<'a> {
    pub guild: u64,
    pub user: u64,
    /// The roles the member holds, for a toggle.
    pub held: &'a [u64],
    /// The message that set off a keyword responder, to react to.
    pub message: Option<(Id<ChannelMarker>, Id<MessageMarker>)>,
    /// An auto-responder runs them: its role changes must not set other responders off (a chain stops
    /// after one responder).
    pub responder: bool,
}

/// The cache key that marks a role change an auto-responder made.
pub(crate) fn own_change_key(guild: u64, user: u64, role: u64) -> String {
    format!("auto-own:{guild}:{user}:{role}")
}

fn utc_day() -> i64 {
    chrono::Utc::now().timestamp().div_euclid(86_400)
}

/// How many of `amount` points the member may still get from custom actions today, counted against
/// [`ACTION_POINTS_DAILY_MAX`]; `0` when the count cannot be kept.
async fn points_allowance(app: &App, guild: u64, user: u64, amount: i64) -> i64 {
    let key = format!("custom-points:{guild}:{user}:{}", utc_day());
    match app.cache.incr_by(&key, amount, Duration::from_secs(2 * 86_400)).await {
        Ok(total) => (ACTION_POINTS_DAILY_MAX - (total - amount)).clamp(0, amount),
        Err(error) => {
            tracing::warn!(guild, user, %error, "could not count custom action points");
            0
        }
    }
}

/// Whether one more direct message from a custom action or auto-responder may go to the member of
/// `guild` this hour (at most [`ACTION_DMS_PER_HOUR`]). A count that cannot be kept sends nothing.
pub(crate) async fn dm_allowed(app: &App, guild: u64, user: u64) -> bool {
    let hour = chrono::Utc::now().timestamp().div_euclid(3_600);
    let key = format!("custom-dm:{guild}:{user}:{hour}");
    match app.cache.incr(&key, Duration::from_secs(2 * 3_600)).await {
        Ok(sent) => sent <= ACTION_DMS_PER_HOUR,
        Err(error) => {
            tracing::warn!(guild, user, %error, "could not count custom direct messages");
            false
        }
    }
}

/// What the points actions did: the member's balance if one ran, and a word when today's cap held some
/// back.
#[derive(Debug, Default)]
pub(crate) struct PointsRun {
    pub balance: Option<i64>,
    pub capped: Option<String>,
}

/// Runs the points actions, which only touch the database. A server with points off skips them; what they
/// give one member a day is capped at [`ACTION_POINTS_DAILY_MAX`] (taking is not).
pub(crate) async fn run_points(app: &App, guild: u64, user: u64, actions: &[Action]) -> PointsRun {
    let mut run = PointsRun::default();
    for action in actions {
        let Action::GivePoints { amount } = action else {
            continue;
        };
        let settings = match app.guild_settings(guild).await {
            Ok(settings) if settings.points_enabled => settings,
            _ => continue,
        };
        let done = if *amount > 0 {
            let allowed = points_allowance(app, guild, user, *amount).await;
            if allowed < *amount {
                run.capped = Some(format!(
                    "Custom commands give at most {} {} a day; you reached today's limit.",
                    thousands(ACTION_POINTS_DAILY_MAX),
                    settings.points_name
                ));
            }
            if allowed == 0 {
                continue;
            }
            app.db.give_points(guild, user, allowed).await
        } else {
            app.db
                .take_points(guild, user, amount.abs())
                .await
                .map(|(_, balance)| balance)
        };
        match done {
            Ok(points) => run.balance = Some(points),
            Err(error) => tracing::warn!(guild, user, %error, "a points action failed"),
        }
    }
    run
}

/// The permission bits of the server's roles as Discord has them now, for the roles a run would give.
async fn live_role_permissions(app: &App, guild: u64) -> anyhow::Result<Vec<(u64, u64)>> {
    let roles = app.http.roles(Id::new(guild.max(1))).await?.models().await?;
    Ok(roles.into_iter().map(|r| (r.id.get(), r.permissions.bits())).collect())
}

/// Runs the actions that talk to Discord (roles, direct messages, reactions), after the answer went
/// out. Returns what to tell the member about the ones that did not work.
pub(crate) async fn run_discord_actions(app: &App, who: &Actor<'_>, actions: &[Action], vars: &Vars) -> Vec<String> {
    let mut notes = Vec::new();
    let (guild_id, user_id) = (Id::new(who.guild.max(1)), Id::new(who.user.max(1)));
    let gives = |action: &Action| match action {
        Action::AddRole { .. } => true,
        Action::ToggleRole { role_id } => !who.held.contains(role_id),
        _ => false,
    };
    // A role is checked again as it is now: its permissions may have changed since it was put here.
    let permissions = if actions.iter().any(gives) {
        live_role_permissions(app, who.guild)
            .await
            .map_err(|error| tracing::warn!(guild = who.guild, %error, "could not check the roles to give"))
            .ok()
    } else {
        None
    };
    for action in actions {
        match action {
            Action::AddRole { .. } | Action::RemoveRole { .. } | Action::ToggleRole { .. } => {
                let role = action.role().unwrap_or(0);
                let give = gives(action);
                if give {
                    let refusal = match &permissions {
                        None => Some(format!("I couldn't check <@&{role}> just now. Try again in a moment.")),
                        Some(list) => list
                            .iter()
                            .find(|(id, _)| *id == role)
                            .and_then(|(_, bits)| run_time_refusal(*bits, &format!("<@&{role}>"))),
                    };
                    if let Some(why) = refusal {
                        tracing::warn!(guild = who.guild, role, "a role action named a role it may not give");
                        notes.push(why);
                        continue;
                    }
                }
                if who.responder {
                    // Marked before the change: Discord may tell of the member update before it answers.
                    let key = own_change_key(who.guild, who.user, role);
                    if let Err(error) = app.cache.put(&key, "1", OWN_CHANGE).await {
                        tracing::warn!(guild = who.guild, %error, "could not mark a responder's role change");
                    }
                }
                let role_id = Id::new(role.max(1));
                let result = if give {
                    app.http
                        .add_guild_member_role(guild_id, user_id, role_id)
                        .reason("Custom command")
                        .await
                } else {
                    app.http
                        .remove_guild_member_role(guild_id, user_id, role_id)
                        .reason("Custom command")
                        .await
                };
                if let Err(error) = result {
                    tracing::warn!(guild = who.guild, role, %error, "a role action failed");
                    notes.push(format!(
                        "I couldn't {} <@&{role}>: {}.",
                        if give { "give you" } else { "take" },
                        crate::roles::refusal(&error)
                    ));
                }
            }
            Action::Dm { text } => {
                if !dm_allowed(app, who.guild, who.user).await {
                    tracing::debug!(user = who.user, "a direct message action was over the hourly limit");
                    continue;
                }
                let clean = scrub_template(text, false);
                let text = truncate(render(&clean, vars, &mut pick).trim(), RESPONSE_MAX);
                if let Some(channel) = crate::leveling::private_channel(app, who.user).await {
                    let none = AllowedMentions::default();
                    if let Err(error) = app
                        .http
                        .create_message(channel)
                        .content(&text)
                        .allowed_mentions(Some(&none))
                        .await
                    {
                        tracing::debug!(user = who.user, %error, "a direct message action was not delivered");
                    }
                }
            }
            Action::React { emoji } => {
                let Some((channel, message)) = who.message else {
                    continue;
                };
                react(app, channel, message, emoji).await;
            }
            Action::GivePoints { .. } => {}
        }
    }
    notes
}

/// Reacts to a message with a Unicode emoji or `<:name:id>`.
async fn react(app: &App, channel: Id<ChannelMarker>, message: Id<MessageMarker>, emoji: &str) {
    use twilight_http::request::channel::reaction::RequestReactionType;
    let custom = emoji
        .strip_prefix('<')
        .and_then(|e| e.strip_suffix('>'))
        .map(|e| e.strip_prefix('a').unwrap_or(e))
        .and_then(|e| e.strip_prefix(':'))
        .and_then(|e| e.split_once(':'))
        .and_then(|(name, id)| Some((name, id.parse::<u64>().ok()?)));
    let result = match custom {
        Some((name, id)) => {
            let kind = RequestReactionType::Custom {
                id: Id::new(id.max(1)),
                name: Some(name),
            };
            app.http.create_reaction(channel, message, &kind).await
        }
        None => {
            let kind = RequestReactionType::Unicode { name: emoji };
            app.http.create_reaction(channel, message, &kind).await
        }
    };
    if let Err(error) = result {
        tracing::debug!(%error, "a reaction action failed");
    }
}

// /Changes

/// What changed between two JSON objects: the fields that differ, as the old and the new values.
pub fn def_diff(before: &Value, after: &Value) -> (Value, Value) {
    let (mut old, mut new) = (Map::new(), Map::new());
    if let (Some(b), Some(a)) = (before.as_object(), after.as_object()) {
        for (key, value) in a {
            if b.get(key) != Some(value) {
                old.insert(key.clone(), b.get(key).cloned().unwrap_or(Value::Null));
                new.insert(key.clone(), value.clone());
            }
        }
    }
    (Value::Object(old), Value::Object(new))
}

#[cfg(test)]
mod tests {
    use pinchora_core::tags::EmbedFieldSpec;
    use serde_json::json;

    use super::*;

    fn first(_: usize) -> usize {
        0
    }

    fn spec<'a>(responses: &'a [String], embed: Option<&'a EmbedSpec>, allow: bool) -> Spec<'a> {
        Spec {
            responses,
            embed,
            allow_mentions: allow,
        }
    }

    #[test]
    fn mentions_in_a_template_are_dropped_unless_the_server_allows_them() {
        let responses = vec!["hi <@123> and <@&456>, @everyone @here".to_owned()];
        let mut vars = Vars::new();
        vars.set("user", "<@9>");
        let off = render_message(&spec(&responses, None, false), &vars, &mut first);
        assert_eq!(off.content, "hi and , everyone here");
        assert!(off.users.is_empty() && off.roles.is_empty());
        let on = render_message(&spec(&responses, None, true), &vars, &mut first);
        assert_eq!(
            on.content, "hi <@123> and <@&456>, everyone here",
            "never everyone or here"
        );
        assert_eq!((on.users, on.roles), (vec![123], vec![456]));
    }

    #[test]
    fn what_a_member_typed_cannot_ping_whatever_the_server_allows() {
        let responses = vec!["{user.name} says: {args}".to_owned()];
        let mut vars = Vars::new();
        vars.set("user.name", scrub_value("Sneaky <@7> @everyone"));
        vars.args(&scrub_value("<@&5> look <@!6> @here"));
        for allow in [false, true] {
            let r = render_message(&spec(&responses, None, allow), &vars, &mut first);
            assert_eq!(r.content, "Sneaky everyone says: look here");
            assert!(r.users.is_empty() && r.roles.is_empty(), "allow {allow}");
        }
    }

    #[test]
    fn the_sender_of_the_message_is_the_only_one_pinged_by_default() {
        let responses = vec!["{user}".to_owned()];
        let mut vars = Vars::new();
        vars.set("user", mention(42));
        let r = render_message(&spec(&responses, None, false), &vars, &mut first);
        let (content, _, mentions) = to_discord(&r, 1, 42);
        assert_eq!(content, "<@42>");
        assert!(mentions.parse.is_empty(), "nothing is parsed from the text");
        assert_eq!(mentions.users, [Id::new(42)]);
        assert!(mentions.roles.is_empty());
        // The server's own @everyone role is never pingable.
        let r = Rendered {
            content: "<@&1>".into(),
            embed: None,
            users: vec![42, 7],
            roles: vec![1, 5],
        };
        let (_, _, mentions) = to_discord(&r, 1, 42);
        assert_eq!(mentions.users, [Id::new(42), Id::new(7)]);
        assert_eq!(mentions.roles, [Id::new(5)]);
    }

    #[test]
    fn a_reply_is_picked_filled_and_cut_to_discords_limit() {
        let responses = vec!["one".to_owned(), "two {1:x}".to_owned()];
        let mut vars = Vars::new();
        vars.args("");
        let mut last = |n: usize| n - 1;
        assert_eq!(
            render_message(&spec(&responses, None, false), &vars, &mut last).content,
            "two x"
        );
        let long = vec!["é{args}".to_owned()];
        let mut vars = Vars::new();
        vars.args(&"é".repeat(RESPONSE_MAX));
        let r = render_message(&spec(&long, None, false), &vars, &mut first);
        assert_eq!(r.content.chars().count(), RESPONSE_MAX);
        assert!(r.content.ends_with('…'));
    }

    #[test]
    fn an_embed_is_filled_scrubbed_and_kept_under_the_total() {
        let embed = EmbedSpec {
            title: Some("Hello {user.name} <@5>".into()),
            description: Some("d".repeat(4_096)),
            footer: Some("{args}".into()),
            fields: vec![EmbedFieldSpec {
                name: "n".into(),
                value: "v".repeat(1_024),
                inline: true,
            }],
            ..Default::default()
        };
        let responses = vec![String::new()];
        let mut vars = Vars::new();
        vars.set("user.name", "Sam");
        vars.args(&"f".repeat(2_000));
        let r = render_message(&spec(&responses, Some(&embed), false), &vars, &mut first);
        let e = r.embed.expect("an embed");
        assert_eq!(e.title.as_deref(), Some("Hello Sam"), "the mention is gone");
        assert!(e.footer.as_deref().unwrap().chars().count() <= EMBED_FOOTER_MAX);
        assert!(e.total_chars() <= EMBED_TOTAL_MAX, "{}", e.total_chars());
        assert!(e.fields[0].inline);
        let built = embed_of(&e);
        assert_eq!(built.fields.len(), 1);
        assert_eq!(built.title.as_deref(), Some("Hello Sam"));
    }

    #[test]
    fn mention_ids_find_users_and_roles() {
        assert_eq!(mention_ids("<@1> <@!2> <@&3> <@1>"), (vec![1, 2], vec![3]));
        assert_eq!(mention_ids("<@abc> <@> <@12 <#5> @1"), (vec![], vec![]));
        assert_eq!(mention_ids("<@<@4>"), (vec![4], vec![]));
    }

    #[test]
    fn defang_removes_only_the_at_sign() {
        assert_eq!(defang("hi @Everyone and @HERE"), "hi Everyone and HERE");
        assert_eq!(
            defang("@@everyone @@@here"),
            "everyone here",
            "removing one cannot make another"
        );
        assert_eq!(
            defang("mail me@here.com"),
            "mail mehere.com",
            "the same tradeoff as the guestbook"
        );
        assert!(!defang("@eve@everyoneryone").contains("@everyone"));
    }

    #[test]
    fn templates_lists_every_text() {
        let embed = EmbedSpec {
            title: Some("t".into()),
            fields: vec![EmbedFieldSpec {
                name: "n".into(),
                value: "v".into(),
                inline: false,
            }],
            ..Default::default()
        };
        let responses = vec!["a".to_owned(), "b".to_owned()];
        let spec = spec(&responses, Some(&embed), false);
        assert_eq!(templates(&spec), ["a", "b", "t", "n", "v"]);
    }

    #[test]
    fn a_diff_keeps_only_what_changed() {
        let before = json!({ "name": "a", "cooldown_secs": 0, "responses": ["x"] });
        let after = json!({ "name": "a", "cooldown_secs": 5, "responses": ["x", "y"], "enabled": true });
        let (old, new) = def_diff(&before, &after);
        assert_eq!(old, json!({ "cooldown_secs": 0, "responses": ["x"], "enabled": null }));
        assert_eq!(
            new,
            json!({ "cooldown_secs": 5, "responses": ["x", "y"], "enabled": true })
        );
        let (old, new) = def_diff(&before, &before);
        assert_eq!((old, new), (json!({}), json!({})));
    }
}
