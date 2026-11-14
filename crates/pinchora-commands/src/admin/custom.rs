//! Custom commands and auto-responders at run time: the caches they read, the message a template becomes
//! (every user-written text goes through one scrubbing pipeline, here), and the things they do besides
//! answering. `/tag`, the event responders and the dashboard's "test" all build their messages with
//! [`render_message`], so what the dashboard shows is what the bot sends.

mod render;
pub use render::*;
mod actions;
pub(crate) use actions::*;
#[cfg(test)]
mod tests;

use std::{sync::Arc, time::Duration};

use moka::future::Cache as Moka;
use pinchora_core::admin::modules::Module;
use pinchora_core::admin::settings::run_time_refusal_in;
use pinchora_core::community::tags::ACTION_DMS_PER_HOUR;
use pinchora_core::community::tags::ACTION_POINTS_DAILY_MAX;
use pinchora_core::community::tags::Action;
use pinchora_core::community::tags::EMBED_AUTHOR_MAX;
use pinchora_core::community::tags::EMBED_DESCRIPTION_MAX;
use pinchora_core::community::tags::EMBED_FIELD_NAME_MAX;
use pinchora_core::community::tags::EMBED_FIELD_VALUE_MAX;
use pinchora_core::community::tags::EMBED_FOOTER_MAX;
use pinchora_core::community::tags::EMBED_TITLE_MAX;
use pinchora_core::community::tags::EMBED_TOTAL_MAX;
use pinchora_core::community::tags::EmbedSpec;
use pinchora_core::community::tags::RESPONSE_MAX;
use pinchora_core::community::tags::Vars;
use pinchora_core::community::tags::render;
use pinchora_core::community::tags::truncate;
use pinchora_core::infra::i18n::Lang;
use pinchora_core::t;
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

use crate::App;
use crate::infra::util::BRAND;
use crate::infra::util::avatar_url;
use crate::infra::util::display_name;
use crate::infra::util::mention;
use crate::infra::util::strip_mentions;
use crate::infra::util::thousands_in;

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
pub(crate) async fn scene_vars(app: &App, scene: &Scene, templates: &[&str], balance: Option<i64>, lang: Lang) -> Vars {
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
                .map_or_else(|| t!(lang, "customcommand.this-server"), |f| escape_markdown(&f.name)),
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
