//! `/stats` (a server's, a member's or a channel's activity as a card) and `/counters` (channels
//! whose name shows a live number). The counting, the flush and the renaming are in `crate::stats`.

use std::collections::HashSet;

use pinchora_core::{
    server_stats::{self, CounterKind, MAX_COUNTERS, Period, TEMPLATE_MAX, counter_name},
    t,
};
use pinchora_db::{AuditSource, CounterRow, CounterSave, NewAudit};
use serde_json::json;
use twilight_interactions::command::{CommandModel, CommandOption, CreateCommand, CreateOption};
use twilight_model::{
    application::interaction::InteractionChannel,
    channel::{ChannelType, permission_overwrite::PermissionOverwrite},
    guild::{Permissions, Role},
    id::Id,
};

use crate::{
    Cx,
    stats::{
        cards::{self, Scope},
        counters::{self, Value, permission_name, problem_text},
    },
    util::display_name,
};

fn may_view(base: Permissions, guild: u64, user: u64, held: &[u64], overwrites: &[PermissionOverwrite]) -> bool {
    crate::text::access::channel_permissions(base, guild, user, held, overwrites).contains(Permissions::VIEW_CHANNEL)
}

fn viewer_base(facts: &crate::app::PermissionFacts, guild: u64, user: u64, held: &[u64]) -> Permissions {
    if facts.owner == user {
        return Permissions::all();
    }
    let base = facts
        .roles
        .iter()
        .filter(|r| r.id.get() == guild || held.contains(&r.id.get()))
        .fold(Permissions::empty(), |p, r| p | r.permissions);
    if base.contains(Permissions::ADMINISTRATOR) {
        Permissions::all()
    } else {
        base
    }
}

async fn visible_channels(cx: &Cx, guild: u64) -> anyhow::Result<HashSet<u64>> {
    let member = cx
        .interaction
        .member
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("member unavailable"))?;
    let held: Vec<u64> = member.roles.iter().map(|r| r.get()).collect();
    let user = cx.author()?.id.get();
    let facts = cx.app.permission_facts(guild).await?;
    let base = viewer_base(&facts, guild, user, &held);
    if let Ok(response) = cx.app.http.guild_channels(Id::new(guild)).await
        && let Ok(channels) = response.models().await
    {
        return Ok(channels
            .iter()
            .filter(|ch| {
                !ch.kind.is_thread()
                    && may_view(
                        base,
                        guild,
                        user,
                        &held,
                        ch.permission_overwrites.as_deref().unwrap_or(&[]),
                    )
            })
            .map(|ch| ch.id.get())
            .collect());
    }
    // A guild listing can be unavailable while individual channels remain readable.
    // The fallback still checks every channel's current overwrites and fails closed.
    let mut visible = HashSet::new();
    for rank in cx
        .app
        .db
        .top_channels(
            guild,
            chrono::Utc::now().date_naive() - chrono::Duration::days(90),
            100_000,
        )
        .await?
    {
        let channel = rank.id as u64;
        if can_view_channel(cx, guild, channel).await.unwrap_or(false) {
            visible.insert(channel);
        }
    }
    Ok(visible)
}

async fn can_view_channel(cx: &Cx, guild: u64, channel: u64) -> anyhow::Result<bool> {
    let member = cx
        .interaction
        .member
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("member unavailable"))?;
    let held: Vec<u64> = member.roles.iter().map(|r| r.get()).collect();
    let user = cx.author()?.id.get();
    let facts = cx.app.permission_facts(guild).await?;
    let base = viewer_base(&facts, guild, user, &held);
    let found = cx.app.http.channel(Id::new(channel)).await?.model().await?;
    if found.guild_id.is_none_or(|id| id.get() != guild) || found.kind == ChannelType::PrivateThread {
        return Ok(false);
    }
    let overwrites = if found.kind.is_thread() {
        let Some(parent) = found.parent_id else {
            return Ok(false);
        };
        cx.app
            .http
            .channel(parent)
            .await?
            .model()
            .await?
            .permission_overwrites
            .unwrap_or_default()
    } else {
        found.permission_overwrites.unwrap_or_default()
    };
    Ok(may_view(base, guild, user, &held, &overwrites))
}

/// How far back a card looks.
#[derive(CommandOption, CreateOption, Clone, Copy, Debug, PartialEq, Eq)]
pub enum Days {
    #[option(name = "Last 7 days", value = "7d")]
    Week,
    #[option(name = "Last 30 days", value = "30d")]
    Month,
    #[option(name = "Last 90 days", value = "90d")]
    Quarter,
}

impl Days {
    fn period(choice: Option<Self>) -> Period {
        match choice {
            Some(Self::Week) => Period::Week,
            Some(Self::Quarter) => Period::Quarter,
            Some(Self::Month) | None => Period::Month,
        }
    }
}

#[derive(CommandModel, CreateCommand)]
#[command(
    name = "stats",
    desc = "Server activity: messages, voice time, joins and leaves, the busiest hours",
    contexts = "guild"
)]
pub enum StatsCommand {
    #[command(name = "server")]
    Server(StatsServer),
    #[command(name = "me")]
    Me(StatsMe),
    #[command(name = "channel")]
    Channel(StatsChannel),
}

#[derive(CommandModel, CreateCommand)]
#[command(name = "server", desc = "This server's activity, busiest channels and members")]
pub struct StatsServer {
    /// How far back to look (default: 30 days)
    period: Option<Days>,
}

#[derive(CommandModel, CreateCommand)]
#[command(name = "me", desc = "Your messages and voice time in this server")]
pub struct StatsMe {
    /// How far back to look (default: 30 days)
    period: Option<Days>,
}

#[derive(CommandModel, CreateCommand)]
#[command(name = "channel", desc = "One channel's messages and voice time")]
pub struct StatsChannel {
    /// The channel to look at
    #[command(channel_types = "guild_text guild_announcement guild_voice guild_stage_voice guild_forum")]
    channel: InteractionChannel,
    /// How far back to look (default: 30 days)
    period: Option<Days>,
}

impl StatsCommand {
    pub async fn run(self, cx: &Cx) -> anyhow::Result<()> {
        let Some(guild) = cx.guild_id() else {
            return cx.reply_ephemeral(t!(cx.lang(), "error.guild-only")).await;
        };
        let (scope, period, name) = match self {
            Self::Server(c) => (Scope::Server, Days::period(c.period), None),
            Self::Me(c) => (
                Scope::Member(cx.author()?.id.get()),
                Days::period(c.period),
                Some(cx.author_display_name()),
            ),
            Self::Channel(c) => (
                Scope::Channel(c.channel.id.get()),
                Days::period(c.period),
                Some(c.channel.name.clone()),
            ),
        };
        if let Scope::Channel(channel) = scope
            && !can_view_channel(cx, guild, channel).await.unwrap_or(false)
        {
            return cx.reply_ephemeral(t!(cx.lang(), "stats.no-access")).await;
        }
        // The names and the drawing can outlast Discord's first-answer window.
        cx.responder.defer(true).await?;
        let data = render(cx, guild, scope, period, name).await?;
        cx.reply(data).await
    }
}

/// Draws the card for `scope`; `name` is the member's or channel's name when the command knows it.
async fn render(
    cx: &Cx,
    guild: u64,
    scope: Scope,
    period: Period,
    name: Option<String>,
) -> anyhow::Result<twilight_model::http::interaction::InteractionResponseData> {
    let (app, lang) = (&cx.app, cx.lang());
    let card = match scope {
        Scope::Server => {
            let visible = visible_channels(cx, guild).await?;
            cards::server_card_for(app, guild, period, lang, Some(&visible)).await?
        }
        Scope::Member(user) => {
            if user != cx.author()?.id.get() {
                anyhow::bail!("stats member card belongs to another viewer");
            }
            let name = match name {
                Some(name) => name,
                None => app
                    .user(user)
                    .await
                    .map_or_else(|| t!(lang, "stats.card.unknown-member"), |u| display_name(&u, None)),
            };
            cards::member_card(app, guild, user, name, period, lang).await?
        }
        Scope::Channel(channel) => {
            if !can_view_channel(cx, guild, channel).await? {
                anyhow::bail!("viewer cannot see stats channel");
            }
            let name = match name {
                Some(name) => name,
                None => cards::channel_names(app, &[channel])
                    .await
                    .remove(&channel)
                    .unwrap_or_else(|| t!(lang, "stats.card.unknown-channel")),
            };
            let mut visible = visible_channels(cx, guild).await?;
            visible.insert(channel);
            let safe_public = crate::stats::everyone_can_view(app, guild, channel).await;
            cards::channel_card(
                app,
                guild,
                channel,
                &name,
                period,
                lang,
                cards::ChannelAudience {
                    visible: &visible,
                    safe_public,
                },
            )
            .await?
        }
    };
    let png = app.render(move |r| r.stats_card(&card)).await?;
    Ok(cards::answer(png, scope, period, lang))
}

/// The 7 / 30 / 90 day buttons under a card: `sc:<scope>:<period>`.
pub(crate) async fn on_component(cx: &Cx, mut parts: std::str::Split<'_, char>) -> anyhow::Result<()> {
    let Some(guild) = cx.guild_id() else {
        return cx.reply_ephemeral(t!(cx.lang(), "error.guild-only")).await;
    };
    let Some(scope) = Scope::parse(&mut parts) else {
        return cx.reply_ephemeral(t!(cx.lang(), "stats.expired")).await;
    };
    let period = parts.next().and_then(Period::parse).unwrap_or_default();
    if let Scope::Member(user) = scope
        && user != cx.author()?.id.get()
    {
        return cx.reply_ephemeral(t!(cx.lang(), "stats.no-access")).await;
    }
    if let Scope::Channel(channel) = scope
        && !can_view_channel(cx, guild, channel).await.unwrap_or(false)
    {
        return cx.reply_ephemeral(t!(cx.lang(), "stats.no-access")).await;
    }
    cx.responder.defer(true).await?;
    let data = render(cx, guild, scope, period, None).await?;
    cx.update(data).await
}

fn manage_guild() -> Permissions {
    Permissions::MANAGE_GUILD
}

/// What a counter counts, as the command offers it.
#[derive(CommandOption, CreateOption, Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    #[option(name = "Members", value = "members")]
    Members,
    #[option(name = "Humans", value = "humans")]
    Humans,
    #[option(name = "Bots", value = "bots")]
    Bots,
    #[option(name = "Online now", value = "online")]
    Online,
    #[option(name = "Boosts", value = "boosts")]
    Boosts,
    #[option(name = "Roles", value = "roles")]
    Roles,
    #[option(name = "Channels", value = "channels")]
    Channels,
    #[option(name = "Members with a role", value = "role_members")]
    RoleMembers,
    #[option(name = "Pinchora houses", value = "houses")]
    Houses,
    #[option(name = "Fish caught", value = "fish")]
    Fish,
    #[option(name = "Average level", value = "level_avg")]
    LevelAvg,
}

impl Kind {
    fn kind(self) -> CounterKind {
        match self {
            Self::Members => CounterKind::Members,
            Self::Humans => CounterKind::Humans,
            Self::Bots => CounterKind::Bots,
            Self::Online => CounterKind::Online,
            Self::Boosts => CounterKind::Boosts,
            Self::Roles => CounterKind::Roles,
            Self::Channels => CounterKind::Channels,
            Self::RoleMembers => CounterKind::RoleMembers,
            Self::Houses => CounterKind::Houses,
            Self::Fish => CounterKind::Fish,
            Self::LevelAvg => CounterKind::LevelAvg,
        }
    }
}

#[derive(CommandModel, CreateCommand)]
#[command(
    name = "counters",
    desc = "Channels whose name shows a live number, like Members: 1,234",
    default_permissions = "manage_guild",
    contexts = "guild"
)]
pub enum CountersCommand {
    #[command(name = "add")]
    Add(CountersAdd),
    #[command(name = "remove")]
    Remove(CountersRemove),
    #[command(name = "list")]
    List(CountersList),
}

#[derive(CommandModel, CreateCommand)]
#[command(name = "add", desc = "Make a channel's name show a number, updated every 10 minutes")]
pub struct CountersAdd {
    /// The channel to rename (a locked voice channel works best)
    #[command(channel_types = "guild_voice guild_stage_voice guild_text guild_announcement guild_category")]
    channel: InteractionChannel,
    /// What it counts
    kind: Kind,
    /// The role to count, for "Members with a role"
    role: Option<Role>,
    /// The name, with {count} where the number goes, e.g. Members: {count}
    #[command(max_length = 80)]
    template: Option<String>,
}

#[derive(CommandModel, CreateCommand)]
#[command(name = "remove", desc = "Stop updating a counter channel (it keeps its last name)")]
pub struct CountersRemove {
    /// The counter channel
    channel: InteractionChannel,
}

#[derive(CommandModel, CreateCommand)]
#[command(name = "list", desc = "This server's counter channels")]
pub struct CountersList;

impl CountersCommand {
    pub async fn run(self, cx: &Cx) -> anyhow::Result<()> {
        let Some(guild) = cx.guild_id() else {
            return cx.reply_ephemeral(t!(cx.lang(), "error.guild-only")).await;
        };
        if !cx.can_manage_guild() {
            return cx.reply_ephemeral(t!(cx.lang(), "error.needs-manage-server")).await;
        }
        match self {
            Self::Add(c) => add(cx, guild, c).await,
            Self::Remove(c) => remove(cx, guild, c).await,
            Self::List(_) => list(cx, guild).await,
        }
    }
}

async fn audit(cx: &Cx, guild: u64, action: &str, channel: u64, before: Option<String>, after: Option<String>) {
    let Ok(actor) = cx.author() else { return };
    let target = channel.to_string();
    let entry = NewAudit {
        guild,
        actor: actor.id.get(),
        actor_name: &cx.author_display_name(),
        source: AuditSource::Discord,
        action,
        target: Some(&target),
        before: before.as_deref(),
        after: after.as_deref(),
    };
    if let Err(error) = cx.app.db.audit(&entry).await {
        tracing::error!(error = ?error, guild, "a counter change was made but not written to the audit log");
    }
}

fn counter_json(row: &CounterRow) -> String {
    json!({ "kind": row.kind, "role_id": row.role_id.map(|r| r.to_string()), "template": row.template }).to_string()
}

async fn add(cx: &Cx, guild: u64, c: CountersAdd) -> anyhow::Result<()> {
    let (app, lang) = (&cx.app, cx.lang());
    let kind = c.kind.kind();
    let role = if kind.needs_role() {
        match &c.role {
            Some(role) => Some(role.id.get()),
            None => return cx.reply_ephemeral(t!(lang, "stats.counters.needs-role")).await,
        }
    } else {
        None
    };
    let template = match c.template.as_deref() {
        Some(raw) => match server_stats::check_template(raw) {
            Ok(template) => template,
            Err(_) => {
                return cx
                    .reply_ephemeral(t!(lang, "stats.counters.bad-template", max = TEMPLATE_MAX))
                    .await;
            }
        },
        // The channel's name is for the whole server: in the server's language.
        None => kind.default_template(app.guild_lang(guild).await),
    };
    let channel = c.channel.id.get();
    if let Some(missing) = counters::missing_permission(app, guild, channel, c.channel.kind).await {
        let text = t!(
            lang,
            "stats.counters.missing",
            permission = permission_name(missing, lang),
            channel = format!("<#{channel}>")
        );
        return cx.reply_ephemeral(text).await;
    }
    let before = app
        .db
        .counters(guild)
        .await?
        .into_iter()
        .find(|r| r.channel_id == channel as i64);
    let row = match app.db.save_counter(guild, channel, kind.key(), role, &template).await? {
        CounterSave::Full => {
            return cx
                .reply_ephemeral(t!(lang, "stats.counters.full", max = MAX_COUNTERS))
                .await;
        }
        CounterSave::Saved(row) => row,
    };
    audit(
        cx,
        guild,
        if before.is_some() {
            "counters.edit"
        } else {
            "counters.add"
        },
        channel,
        before.as_ref().map(counter_json),
        Some(counter_json(&row)),
    )
    .await;
    let value = counters::values(app, guild, &[(kind, role)])
        .await
        .remove(&(kind, role))
        .unwrap_or(Value::Unknown);
    let used = app.db.counters(guild).await?.len();
    let mention = format!("<#{channel}>");
    let text = match value {
        Value::Number(n) => t!(
            lang,
            "stats.counters.added",
            channel = mention,
            name = counter_name(&template, n),
            used = used,
            max = MAX_COUNTERS
        ),
        Value::Problem(key) => t!(
            lang,
            "stats.counters.added-problem",
            channel = mention,
            problem = problem_text(key, lang)
        ),
        Value::Unknown => t!(lang, "stats.counters.added-later", channel = mention),
    };
    // The first rename goes out now, without keeping the answer waiting.
    let background = cx.app.clone();
    cx.app.spawn(async move {
        if let Err(error) = counters::apply(&background, guild, &row, value).await {
            tracing::warn!(%error, guild, "a new counter was not renamed");
        }
    });
    cx.reply_ephemeral(text).await
}

async fn remove(cx: &Cx, guild: u64, c: CountersRemove) -> anyhow::Result<()> {
    let (app, lang) = (&cx.app, cx.lang());
    let channel = c.channel.id.get();
    let before = app
        .db
        .counters(guild)
        .await?
        .into_iter()
        .find(|r| r.channel_id == channel as i64);
    let mention = format!("<#{channel}>");
    if !app.db.remove_counter(guild, channel).await? {
        return cx
            .reply_ephemeral(t!(lang, "stats.counters.not-a-counter", channel = mention))
            .await;
    }
    audit(
        cx,
        guild,
        "counters.remove",
        channel,
        before.as_ref().map(counter_json),
        None,
    )
    .await;
    cx.reply_ephemeral(t!(lang, "stats.counters.removed", channel = mention))
        .await
}

async fn list(cx: &Cx, guild: u64) -> anyhow::Result<()> {
    let (app, lang) = (&cx.app, cx.lang());
    let rows = app.db.counters(guild).await?;
    if rows.is_empty() {
        return cx.reply_ephemeral(t!(lang, "stats.counters.none")).await;
    }
    let mut text = t!(lang, "stats.counters.heading", used = rows.len(), max = MAX_COUNTERS);
    for row in &rows {
        let kind = CounterKind::parse(&row.kind).map_or_else(|| row.kind.clone(), |k| k.name(lang));
        let shown = row
            .last_name
            .clone()
            .unwrap_or_else(|| t!(lang, "stats.counters.not-yet"));
        text.push('\n');
        text.push_str(&t!(
            lang,
            "stats.counters.line",
            channel = format!("<#{}>", row.channel_id),
            kind = kind,
            name = crate::escape_markdown(&shown)
        ));
        if let Some(key) = &row.problem {
            text.push_str("\n  ");
            text.push_str(&problem_text(key, lang));
        }
    }
    cx.reply_ephemeral(text).await
}

#[cfg(test)]
mod tests {
    use pinchora_core::i18n::Lang;

    use super::*;
    use crate::stats::counters::problem;

    #[test]
    fn every_kind_has_a_choice_and_text() {
        for kind in [
            Kind::Members,
            Kind::Humans,
            Kind::Bots,
            Kind::Online,
            Kind::Boosts,
            Kind::Roles,
            Kind::Channels,
            Kind::RoleMembers,
            Kind::Houses,
            Kind::Fish,
            Kind::LevelAvg,
        ] {
            assert_eq!(CounterKind::parse(kind.value()), Some(kind.kind()));
        }
        assert_eq!(CounterKind::ALL.len(), 11, "a new kind needs a choice above");
        for key in [
            problem::VIEW_CHANNEL,
            problem::MANAGE_CHANNELS,
            problem::CONNECT,
            problem::TOO_MANY_MEMBERS,
            problem::ROLE_GONE,
        ] {
            for lang in Lang::ALL {
                assert!(
                    !problem_text(key, lang).contains("stats."),
                    "{key} has no text in {lang:?}"
                );
            }
        }
    }

    #[test]
    fn channel_cards_obey_viewer_overwrites() {
        use twilight_model::channel::permission_overwrite::PermissionOverwriteType;
        let guild = 123;
        let hidden = PermissionOverwrite {
            allow: Permissions::empty(),
            deny: Permissions::VIEW_CHANNEL,
            id: Id::new(guild),
            kind: PermissionOverwriteType::Role,
        };
        assert!(!may_view(Permissions::VIEW_CHANNEL, guild, 456, &[], &[hidden]));
        assert!(may_view(Permissions::VIEW_CHANNEL, guild, 456, &[], &[]));
    }
}
