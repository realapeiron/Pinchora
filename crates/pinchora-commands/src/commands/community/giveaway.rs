//! `/giveaway`: prize draws members join with a button, ended by the events ticker.
//!
//! A shells prize is paid by the host: `winners` x `shells` is taken from them when the giveaway starts,
//! winners are paid when it ends and whatever nobody won goes back to the host, so a server can never
//! mint shells. Mentions in the prize are stripped and every message pings nobody.

use std::collections::HashMap;
use std::{sync::Arc, time::Duration};

use chrono::{DateTime, Utc};
use pinchora_core::admin::server_events::GIVEAWAY_MAX;
use pinchora_core::admin::server_events::GIVEAWAY_MIN;
use pinchora_core::admin::server_events::parse_duration;
use pinchora_core::infra::i18n::Lang;
use pinchora_core::t;
use pinchora_db::{EndedGiveaway, EntryToggle, GiveawayOpen, GiveawayRow, MAX_RUNNING_GIVEAWAYS, NewGiveaway, Reroll};
use twilight_interactions::command::{CommandModel, CreateCommand};
use twilight_model::{
    application::interaction::InteractionChannel,
    channel::message::{Component, Embed, MessageFlags, component::ButtonStyle},
    guild::{Permissions, Role},
    http::interaction::InteractionResponseData,
    id::Id,
};
use twilight_util::builder::embed::{EmbedBuilder, EmbedFieldBuilder, EmbedFooterBuilder};

use crate::App;
use crate::Cx;
use crate::commands::games::games::edit_message;
use crate::commands::games::games::quiet;
use crate::community::server_events::button;
use crate::community::server_events::is_permanent;
use crate::community::server_events::message_link;
use crate::community::server_events::post_refusal;
use crate::community::server_events::row;
use crate::community::server_events::tell;
use crate::infra::util::BRAND;
use crate::infra::util::SUCCESS;
use crate::infra::util::relative;
use crate::infra::util::single_line;
use crate::infra::util::strip_mentions;
use crate::infra::util::thousands_in;

/// Entry counts shown on the message change at most this often per giveaway.
const REFRESH_EVERY: Duration = Duration::from_secs(5);
/// Giveaways whose entry count changed inside that window, waiting for the ticker to show it.
const CHANGED: &str = "gw:changed";

fn manage_guild() -> Permissions {
    Permissions::MANAGE_GUILD
}

#[derive(CommandModel, CreateCommand)]
#[command(
    name = "giveaway",
    desc = "Run giveaways: members press a button to enter",
    default_permissions = "manage_guild",
    contexts = "guild"
)]
#[allow(clippy::large_enum_variant)] // parsed once per interaction, never stored
pub enum GiveawayCommand {
    #[command(name = "start")]
    Start(GiveawayStart),
    #[command(name = "end")]
    End(GiveawayEnd),
    #[command(name = "reroll")]
    Reroll(GiveawayReroll),
    #[command(name = "list")]
    List(GiveawayList),
}

#[derive(CommandModel, CreateCommand)]
#[command(name = "start", desc = "Start a giveaway")]
pub struct GiveawayStart {
    /// How long it runs: 30m, 2h, 1d, 1d12h (1 minute to 30 days)
    #[command(min_length = 2, max_length = 20)]
    duration: String,
    /// How many winners (1 to 20)
    #[command(min_value = 1, max_value = 20)]
    winners: i64,
    /// What is being given away
    #[command(min_length = 1, max_length = 100)]
    prize: String,
    /// Shells each winner is paid: taken from you now, whatever nobody wins comes back
    #[command(min_value = 1, max_value = 1_000_000)]
    shells: Option<i64>,
    /// Where to post it (default: here)
    #[command(channel_types = "guild_text guild_announcement")]
    channel: Option<InteractionChannel>,
    /// Only members with this role can enter
    required_role: Option<Role>,
    /// Minimum level in this server
    #[command(min_value = 0, max_value = 1000)]
    min_level: Option<i64>,
    /// Minimum Discord account age in days
    #[command(min_value = 0, max_value = 3650)]
    account_days: Option<i64>,
    /// Minimum time in this server in days
    #[command(min_value = 0, max_value = 3650)]
    server_days: Option<i64>,
    /// Members with this role receive bonus entries
    bonus_role: Option<Role>,
    /// Extra entries for the bonus role, up to 20
    #[command(min_value = 1, max_value = 20)]
    bonus_entries: Option<i64>,
    /// Wait before opening: 1m to 30d
    start_in: Option<String>,
}

#[derive(CommandModel, CreateCommand)]
#[command(name = "end", desc = "End a running giveaway now and pick its winners")]
pub struct GiveawayEnd {
    /// The giveaway's number
    #[command(min_value = 1)]
    id: i64,
}

#[derive(CommandModel, CreateCommand)]
#[command(name = "reroll", desc = "Pick new winners for a giveaway that ended")]
pub struct GiveawayReroll {
    /// The giveaway's number
    #[command(min_value = 1)]
    id: i64,
    /// How many new winners (default 1)
    #[command(min_value = 1, max_value = 20)]
    winners: Option<i64>,
}

#[derive(CommandModel, CreateCommand)]
#[command(name = "list", desc = "The giveaways running in this server")]
pub struct GiveawayList;

fn button_id(id: i64) -> String {
    format!("gw:e:{id}")
}

/// Reads what follows the `gw:e:` of an Enter button.
fn parse_button<'a>(mut parts: impl Iterator<Item = &'a str>) -> Option<i64> {
    let id = (parts.next()? == "e").then(|| parts.next()?.parse().ok()).flatten()?;
    parts.next().is_none().then_some(id)
}

/// Samples without replacement, giving each entrant their weight at entry time.
fn draw_weighted(pool: &[u64], count: usize, weights: &HashMap<u64, i16>) -> Vec<u64> {
    let mut remaining = pool.to_vec();
    let mut picked = Vec::new();
    while !remaining.is_empty() && picked.len() < count {
        let total: u64 = remaining
            .iter()
            .map(|id| u64::from(*weights.get(id).unwrap_or(&1) as u16))
            .sum();
        let mut ticket = rand::random_range(0..total);
        let index = remaining
            .iter()
            .position(|id| {
                let weight = u64::from(*weights.get(id).unwrap_or(&1) as u16);
                if ticket < weight {
                    true
                } else {
                    ticket -= weight;
                    false
                }
            })
            .unwrap_or(0);
        picked.push(remaining.remove(index));
    }
    picked
}

fn mentions(ids: &[i64]) -> String {
    ids.iter().map(|id| format!("<@{id}>")).collect::<Vec<_>>().join(", ")
}

fn timestamp(at: DateTime<Utc>) -> String {
    format!("<t:{}:f>", at.timestamp())
}

/// The giveaway as its message shows it: details and the entry count while it runs, the winners after.
fn giveaway_embed(lang: Lang, g: &GiveawayRow) -> Embed {
    let mut lines = Vec::new();
    let upcoming = g.status == "scheduled";
    if g.is_running() {
        lines.push(t!(lang, "giveaway.press-enter"));
        lines.push(String::new());
    }
    lines.push(t!(lang, "giveaway.hosted-by", host = format!("<@{}>", g.host_id)));
    if upcoming {
        lines.push(t!(
            lang,
            "giveaway.starts",
            when = relative(g.starts_at.unwrap_or(g.ends_at))
        ));
    } else if g.is_running() {
        lines.push(t!(
            lang,
            "giveaway.ends",
            relative = relative(g.ends_at),
            absolute = timestamp(g.ends_at)
        ));
    } else {
        lines.push(t!(
            lang,
            "giveaway.ended",
            relative = relative(g.ended_at.unwrap_or(g.ends_at))
        ));
    }
    lines.push(t!(lang, "giveaway.winners-line", count = g.winners));
    if g.shells > 0 {
        lines.push(t!(lang, "giveaway.prize-line", amount = thousands_in(lang, g.shells)));
    }
    if let Some(role) = g.required_role_id {
        lines.push(t!(lang, "giveaway.requires", role = format!("<@&{role}>")));
    }
    if g.min_level > 0 {
        lines.push(t!(lang, "giveaway.min-level", level = g.min_level));
    }
    if g.min_account_days > 0 {
        lines.push(t!(lang, "giveaway.min-account", days = g.min_account_days));
    }
    if g.min_server_days > 0 {
        lines.push(t!(lang, "giveaway.min-server", days = g.min_server_days));
    }
    if let Some(role) = g.bonus_role_id {
        lines.push(t!(
            lang,
            "giveaway.bonus",
            role = format!("<@&{role}>"),
            entries = g.bonus_entries
        ));
    }
    if g.status == "scheduled" {
        lines.push(t!(lang, "giveaway.scheduled"));
    }
    let mut embed = EmbedBuilder::new()
        .color(if g.is_running() || upcoming { BRAND } else { SUCCESS })
        .title(&g.prize)
        .description(lines.join("\n"));
    embed = if g.is_running() || upcoming {
        embed.field(EmbedFieldBuilder::new(t!(lang, "giveaway.entries"), g.entries.to_string()).inline())
    } else if g.winner_ids.is_empty() {
        embed.field(EmbedFieldBuilder::new(
            t!(lang, "giveaway.winners-field"),
            t!(lang, "giveaway.nobody-entered"),
        ))
    } else {
        embed.field(EmbedFieldBuilder::new(
            t!(lang, "giveaway.winners-field"),
            mentions(&g.winner_ids),
        ))
    };
    embed
        .footer(EmbedFooterBuilder::new(t!(lang, "giveaway.footer", id = g.id)))
        .build()
}

fn giveaway_components(lang: Lang, g: &GiveawayRow) -> Vec<Component> {
    let (label, style) = if g.is_running() || g.status == "scheduled" {
        (t!(lang, "giveaway.enter"), ButtonStyle::Primary)
    } else {
        (t!(lang, "giveaway.ended-button"), ButtonStyle::Secondary)
    };
    vec![row(vec![button(
        button_id(g.id),
        &label,
        Some("🎉"),
        style,
        !g.is_running(),
    )])]
}

impl GiveawayCommand {
    pub async fn run(self, cx: &Cx) -> anyhow::Result<()> {
        let Some(guild) = cx.guild_id() else {
            return tell(cx, t!(cx.lang(), "giveaway.guild-only")).await;
        };
        if !cx.can_manage_guild() {
            return tell(cx, t!(cx.lang(), "giveaway.needs-manage")).await;
        }
        match self {
            Self::Start(c) => start(cx, guild, c).await,
            Self::End(c) => end_now(cx, guild, c.id).await,
            Self::Reroll(c) => reroll(cx, guild, c).await,
            Self::List(_) => list(cx, guild).await,
        }
    }
}

async fn start(cx: &Cx, guild: u64, c: GiveawayStart) -> anyhow::Result<()> {
    let Some(span) = parse_duration(&c.duration) else {
        return tell(cx, t!(cx.lang(), "giveaway.bad-duration")).await;
    };
    if span < GIVEAWAY_MIN || span > GIVEAWAY_MAX {
        return tell(cx, t!(cx.lang(), "giveaway.duration-range")).await;
    }
    let prize = single_line(&strip_mentions(&c.prize));
    if prize.is_empty() {
        return tell(cx, t!(cx.lang(), "giveaway.needs-prize")).await;
    }
    let channel = c
        .channel
        .map(|c| c.id)
        .or_else(|| cx.interaction.channel.as_ref().map(|c| c.id))
        .ok_or_else(|| anyhow::anyhow!("giveaway without a channel"))?;
    let host = cx.author()?.id.get();
    let shells = c.shells.unwrap_or(0);
    // @everyone is the whole server, so requiring it requires nothing.
    let required_role = c.required_role.map(|r| r.id.get()).filter(|r| *r != guild);
    if c.bonus_entries.is_some() && c.bonus_role.is_none() {
        return tell(cx, t!(cx.lang(), "giveaway.bonus-needs-role")).await;
    }
    let starts_at = match c.start_in.as_deref() {
        Some(value) => match parse_duration(value) {
            Some(delay) if (GIVEAWAY_MIN..=GIVEAWAY_MAX).contains(&delay) => Some(cx.app.now() + delay),
            _ => return tell(cx, t!(cx.lang(), "giveaway.bad-duration")).await,
        },
        None => None,
    };
    let ends_at = starts_at.unwrap_or_else(|| cx.app.now()) + span;

    let opened = cx
        .app
        .db
        .create_giveaway(NewGiveaway {
            guild_id: guild,
            channel_id: channel.get(),
            host,
            prize: &prize,
            shells,
            winners: c.winners as i32,
            required_role,
            ends_at,
        })
        .await?;
    let mut giveaway = match opened {
        GiveawayOpen::Started(giveaway) => *giveaway,
        GiveawayOpen::Broke { balance } => {
            return tell(
                cx,
                t!(
                    cx.lang(),
                    "giveaway.broke",
                    count = c.winners,
                    each = thousands_in(cx.lang(), shells),
                    total = thousands_in(cx.lang(), shells * c.winners),
                    balance = thousands_in(cx.lang(), balance)
                ),
            )
            .await;
        }
        GiveawayOpen::Full => {
            return tell(cx, t!(cx.lang(), "giveaway.full", max = MAX_RUNNING_GIVEAWAYS)).await;
        }
    };
    if let Err(error) = sqlx::query("UPDATE giveaways SET min_level=$2,min_account_days=$3,min_server_days=$4,bonus_role_id=$5,bonus_entries=$6,starts_at=$7,status=CASE WHEN $7::timestamptz IS NULL THEN 'running' ELSE 'scheduled' END WHERE id=$1")
        .bind(giveaway.id).bind(c.min_level.unwrap_or(0) as i32).bind(c.account_days.unwrap_or(0) as i32)
        .bind(c.server_days.unwrap_or(0) as i32).bind(c.bonus_role.map(|r| r.id.get() as i64))
        .bind(c.bonus_entries.unwrap_or(0) as i16).bind(starts_at)
        .execute(cx.app.db.pool()).await {
        let _ = cx.app.db.abort_giveaway(giveaway.id).await;
        return Err(error.into());
    }
    giveaway = cx.app.db.giveaway(giveaway.id).await?.expect("new giveaway");
    // The giveaway message answers nobody in particular: it speaks the server's language.
    let server_lang = cx.app.guild_lang(guild).await;

    let posted = cx
        .app
        .http
        .create_message(channel)
        .embeds(&[cx.app.brand_embed(guild, giveaway_embed(server_lang, &giveaway)).await])
        .components(&giveaway_components(server_lang, &giveaway))
        .allowed_mentions(quiet().as_ref())
        .await;
    let message = match posted {
        Ok(response) => response.model().await?,
        Err(error) => {
            tracing::warn!(guild, giveaway = giveaway.id, %error, "could not post a giveaway");
            cx.app.db.abort_giveaway(giveaway.id).await?;
            return tell(cx, post_refusal(cx.lang(), channel, &error)).await;
        }
    };
    // The message is up; the giveaway has to know it (the entry counter and the ending edit it). One more try
    // if the first fails, and then it runs on without: the giveaway is live and its ending still announces.
    for attempt in 1..=2 {
        match cx
            .app
            .db
            .set_giveaway_message(giveaway.id, channel.get(), message.id.get())
            .await
        {
            Ok(()) => break,
            Err(error) => {
                tracing::warn!(guild, giveaway = giveaway.id, attempt, %error, "could not remember a giveaway's message");
            }
        }
    }
    let held = if giveaway.escrow() > 0 {
        t!(
            cx.lang(),
            "giveaway.held",
            amount = thousands_in(cx.lang(), giveaway.escrow())
        )
    } else {
        String::new()
    };
    tell(
        cx,
        t!(
            cx.lang(),
            "giveaway.live",
            id = giveaway.id,
            channel = format!("<#{channel}>"),
            link = message_link(guild, channel.get(), message.id.get()),
            held = held
        ),
    )
    .await
}

/// The giveaway numbered `id` in this server, or a reply saying there is none (and `None`).
async fn find(cx: &Cx, guild: u64, id: i64) -> anyhow::Result<Option<GiveawayRow>> {
    let found = cx.app.db.giveaway(id).await?.filter(|g| g.guild_id as u64 == guild);
    if found.is_none() {
        tell(cx, t!(cx.lang(), "giveaway.none", id = id)).await?;
    }
    Ok(found)
}

async fn end_now(cx: &Cx, guild: u64, id: i64) -> anyhow::Result<()> {
    let Some(giveaway) = find(cx, guild, id).await? else {
        return Ok(());
    };
    let Some(ended) = end(&cx.app, giveaway.id, true).await? else {
        return tell(cx, t!(cx.lang(), "giveaway.already-ended", id = id)).await;
    };
    let text = if ended.winners.is_empty() {
        t!(cx.lang(), "giveaway.ended-none", id = id)
    } else {
        t!(
            cx.lang(),
            "giveaway.ended-with",
            id = id,
            winners = mentions(&ended.giveaway.winner_ids)
        )
    };
    tell(cx, text).await
}

async fn reroll(cx: &Cx, guild: u64, c: GiveawayReroll) -> anyhow::Result<()> {
    let count = c.winners.unwrap_or(1) as usize;
    let outcome = cx
        .app
        .db
        .reroll_giveaway_weighted(guild, c.id, count, draw_weighted)
        .await?;
    let (giveaway, picked) = match outcome {
        Reroll::Done(giveaway, picked) => (*giveaway, picked),
        Reroll::Missing => return tell(cx, t!(cx.lang(), "giveaway.none", id = c.id)).await,
        Reroll::StillRunning => {
            return tell(cx, t!(cx.lang(), "giveaway.still-running", id = c.id)).await;
        }
        Reroll::NoOneLeft => {
            return tell(cx, t!(cx.lang(), "giveaway.no-one-left", id = c.id)).await;
        }
    };
    let winners = mentions(&picked.iter().map(|w| *w as i64).collect::<Vec<_>>());
    // A reroll only picks people: the shells of a shells prize were paid when it ended.
    // The announcement answers nobody in particular: it speaks the server's language.
    let server_lang = cx.app.guild_lang(guild).await;
    let text = t!(
        server_lang,
        "giveaway.reroll-announce",
        count = picked.len(),
        prize = giveaway.prize,
        winners = winners,
        note = if giveaway.shells > 0 {
            t!(server_lang, "giveaway.reroll-no-shells")
        } else {
            String::new()
        }
    );
    let channel = Id::new((giveaway.channel_id as u64).max(1));
    let no_pings = quiet();
    let mut post = cx
        .app
        .http
        .create_message(channel)
        .content(&text)
        .allowed_mentions(no_pings.as_ref());
    if let Some(message) = giveaway.message_id {
        post = post.reply(Id::new((message as u64).max(1))).fail_if_not_exists(false);
    }
    let told = match post.await {
        Ok(_) => t!(cx.lang(), "giveaway.announced", channel = format!("<#{channel}>")),
        Err(error) => {
            tracing::warn!(guild, giveaway = giveaway.id, %error, "could not announce a reroll");
            t!(
                cx.lang(),
                "giveaway.announce-failed",
                channel = format!("<#{channel}>"),
                reason = post_refusal(cx.lang(), channel, &error)
            )
        }
    };
    tell(
        cx,
        t!(
            cx.lang(),
            "giveaway.rerolled",
            id = giveaway.id,
            winners = winners,
            told = told
        ),
    )
    .await
}

async fn list(cx: &Cx, guild: u64) -> anyhow::Result<()> {
    let running = cx.app.db.running_giveaways(guild).await?;
    if running.is_empty() {
        return tell(cx, t!(cx.lang(), "giveaway.list-empty")).await;
    }
    let lang = cx.lang();
    let lines: Vec<String> = running
        .iter()
        .map(|g| {
            let prize: String = g.prize.chars().take(40).collect();
            let link = g.message_id.map_or_else(String::new, |m| {
                t!(
                    lang,
                    "giveaway.jump",
                    link = message_link(guild, g.channel_id as u64, m as u64)
                )
            });
            t!(
                lang,
                "giveaway.list-row",
                id = g.id,
                prize = prize,
                entries = t!(lang, "giveaway.entries-count", count = g.entries),
                ends = relative(g.ends_at),
                link = link
            )
        })
        .collect();
    let embed = EmbedBuilder::new()
        .color(BRAND)
        .title(t!(lang, "giveaway.list-title"))
        .description(lines.join("\n"))
        .build();
    cx.reply(InteractionResponseData {
        embeds: Some(vec![embed]),
        flags: Some(MessageFlags::EPHEMERAL),
        allowed_mentions: quiet(),
        ..Default::default()
    })
    .await
}

/// `gw:e:<id>`: a member enters the giveaway, or leaves it when they are in.
pub(crate) async fn on_component<'a>(cx: &Cx, parts: impl Iterator<Item = &'a str>) -> anyhow::Result<()> {
    let Some(id) = parse_button(parts) else {
        return tell(cx, t!(cx.lang(), "social.button-expired")).await;
    };
    let guild = cx.guild_id();
    let giveaway = cx
        .app
        .db
        .giveaway(id)
        .await?
        .filter(|g| Some(g.guild_id as u64) == guild);
    let Some(giveaway) = giveaway else {
        return tell(cx, t!(cx.lang(), "giveaway.gone")).await;
    };
    let user = cx.author()?.id.get();
    if giveaway.host_id as u64 == user {
        return tell(cx, t!(cx.lang(), "giveaway.hosting")).await;
    }
    if let Some(role) = giveaway.required_role_id {
        let has_it = cx
            .interaction
            .member
            .as_ref()
            .is_some_and(|m| m.roles.iter().any(|r| r.get() == role as u64));
        if !has_it {
            return tell(cx, t!(cx.lang(), "giveaway.needs-role", role = format!("<@&{role}>"))).await;
        }
    }
    let rules: (i32,i32,i32,Option<i64>,i16,Option<DateTime<Utc>>) = sqlx::query_as(
        "SELECT min_level,min_account_days,min_server_days,bonus_role_id,bonus_entries,starts_at FROM giveaways WHERE id=$1")
        .bind(id).fetch_one(cx.app.db.pool()).await?;
    let now = cx.app.now();
    if giveaway.status == "scheduled" || rules.5.is_some_and(|at| at > now) {
        return tell(cx, t!(cx.lang(), "giveaway.not-started")).await;
    }
    if rules.0 > 0 {
        let xp: Option<(i64,)> = sqlx::query_as("SELECT xp FROM members WHERE guild_id=$1 AND user_id=$2")
            .bind(giveaway.guild_id)
            .bind(user as i64)
            .fetch_optional(cx.app.db.pool())
            .await?;
        if cx
            .app
            .db
            .leveling_policy(giveaway.guild_id as u64)
            .await?
            .level_of(xp.map_or(0, |x| x.0))
            < rules.0 as u32
        {
            return tell(cx, t!(cx.lang(), "giveaway.needs-level", level = rules.0)).await;
        }
    }
    let account_ms = ((user >> 22) + 1_420_070_400_000) as i64;
    if now.timestamp_millis() - account_ms < i64::from(rules.1) * 86_400_000 {
        return tell(cx, t!(cx.lang(), "giveaway.needs-account-age", days = rules.1)).await;
    }
    if rules.2 > 0 {
        let old_enough = cx
            .interaction
            .member
            .as_ref()
            .and_then(|m| m.joined_at)
            .is_some_and(|joined| now.timestamp_micros() - joined.as_micros() >= i64::from(rules.2) * 86_400_000_000);
        if !old_enough {
            return tell(cx, t!(cx.lang(), "giveaway.needs-server-age", days = rules.2)).await;
        }
    }
    let weight = if rules.3.is_some_and(|role| {
        cx.interaction
            .member
            .as_ref()
            .is_some_and(|m| m.roles.iter().any(|r| r.get() == role as u64))
    }) {
        rules.4 + 1
    } else {
        1
    };
    let entered = cx
        .app
        .db
        .toggle_giveaway_entry_weighted(id, user, weight, cx.app.now())
        .await?;
    let text = match entered {
        EntryToggle::Closed => return tell(cx, t!(cx.lang(), "giveaway.closed")).await,
        EntryToggle::Entered(n) => t!(
            cx.lang(),
            "giveaway.entered",
            prize = giveaway.prize,
            entries = t!(cx.lang(), "giveaway.entries-count", count = n)
        ),
        EntryToggle::Left(_) => t!(cx.lang(), "giveaway.left"),
    };
    tell(cx, text).await?;
    show_count(&cx.app, id).await;
    Ok(())
}

fn refresh_key(id: i64) -> String {
    format!("gw-refresh:{id}")
}

/// Shows a giveaway's new entry count on its message, unless one was shown in the last five seconds;
/// then the ticker shows the latest count once that window is over.
async fn show_count(app: &Arc<App>, id: i64) {
    match app.cache.try_cooldown(&refresh_key(id), REFRESH_EVERY).await {
        Ok(None) => refresh_message(app, id).await,
        Ok(Some(_)) => {
            if let Err(error) = app.cache.set_add(CHANGED, &id.to_string()).await {
                tracing::debug!(%error, giveaway = id, "entry count not queued");
            }
        }
        Err(error) => tracing::debug!(%error, giveaway = id, "entry count not shown"),
    }
}

/// Edits the giveaway message to the entry count the database has now.
pub(crate) async fn refresh_message(app: &App, id: i64) {
    let giveaway = match app.db.giveaway(id).await {
        Ok(Some(g)) if g.is_running() => g,
        Ok(_) => return,
        Err(error) => return tracing::debug!(%error, giveaway = id, "giveaway not loaded"),
    };
    if let Some(message) = giveaway.message_id {
        let lang = app.guild_lang(giveaway.guild_id as u64).await;
        let refreshed = edit_message(
            app,
            giveaway.channel_id as u64,
            message as u64,
            None,
            Some(&[giveaway_embed(lang, &giveaway)]),
            Some(&giveaway_components(lang, &giveaway)),
        )
        .await;
        if refreshed {
            let _ = sqlx::query("UPDATE giveaways SET start_announced=TRUE WHERE id=$1 AND status='running'")
                .bind(id)
                .execute(app.db.pool())
                .await;
        }
    }
}

/// The ticker's part in entry counts: shows the ones that changed inside a window, as soon as their
/// window is over.
pub(crate) async fn refresh_changed(app: &Arc<App>) {
    let Ok(changed) = app.cache.set_members(CHANGED).await else {
        return;
    };
    for raw in changed {
        let Ok(id) = raw.parse::<i64>() else {
            let _ = app.cache.set_remove(CHANGED, &raw).await;
            continue;
        };
        match app.cache.try_cooldown(&refresh_key(id), REFRESH_EVERY).await {
            Ok(None) => {
                let _ = app.cache.set_remove(CHANGED, &raw).await;
                refresh_message(app, id).await;
            }
            // Still inside the window: the next tick.
            Ok(Some(_)) => {}
            Err(error) => tracing::debug!(%error, giveaway = id, "entry count not shown"),
        }
    }
}

/// Ends giveaway `id` (when it is due, or at once with `early`) and tells the channel. `None` when it
/// was not running, because somebody else ended it first.
pub(crate) async fn end(app: &Arc<App>, id: i64, early: bool) -> anyhow::Result<Option<EndedGiveaway>> {
    let Some(ended) = app
        .db
        .end_giveaway_weighted(id, app.now(), early, draw_weighted)
        .await?
    else {
        return Ok(None);
    };
    metrics::counter!("pinchora_giveaways_ended_total").increment(1);
    if let Err(error) = announce(app, &ended.giveaway).await {
        // The winners are paid and saved; the ticker tries the announcement again.
        tracing::warn!(%error, giveaway = id, "giveaway ended but not announced");
    }
    Ok(Some(ended))
}

/// Shows the result on the giveaway's message and posts it in the channel, once, whichever pod gets
/// there first. A refusal that will never change (the channel is gone) is given up on.
async fn announce(app: &Arc<App>, g: &GiveawayRow) -> anyhow::Result<()> {
    if !app.db.claim_giveaway_announcement(g.id, app.now()).await? {
        return Ok(());
    }
    let channel = Id::new((g.channel_id as u64).max(1));
    let lang = app.guild_lang(g.guild_id as u64).await;
    if let Some(message) = g.message_id {
        edit_message(
            app,
            g.channel_id as u64,
            message as u64,
            None,
            Some(&[giveaway_embed(lang, g)]),
            Some(&giveaway_components(lang, g)),
        )
        .await;
    }
    let winners = mentions(&g.winner_ids);
    let unpaid = g.shells * (i64::from(g.winners) - g.winner_ids.len() as i64).max(0);
    let mut text = if g.winner_ids.is_empty() {
        t!(lang, "giveaway.nobody", prize = g.prize)
    } else {
        t!(lang, "giveaway.congrats", winners = winners, prize = g.prize)
    };
    if g.shells > 0 && !g.winner_ids.is_empty() {
        text.push_str(&t!(lang, "giveaway.paid", amount = thousands_in(lang, g.shells)));
    }
    if unpaid > 0 {
        text.push_str(&t!(
            lang,
            "giveaway.unclaimed",
            amount = thousands_in(lang, unpaid),
            host = format!("<@{}>", g.host_id)
        ));
    }
    let no_pings = quiet();
    let mut post = app
        .http
        .create_message(channel)
        .content(&text)
        .allowed_mentions(no_pings.as_ref());
    if let Some(message) = g.message_id {
        post = post.reply(Id::new((message as u64).max(1))).fail_if_not_exists(false);
    }
    match post.await {
        Ok(_) => {}
        Err(error) if is_permanent(&error) => {
            tracing::warn!(giveaway = g.id, %error, "giveaway result could not be posted; giving up");
        }
        Err(error) => return Err(error.into()),
    }
    app.db.mark_giveaway_announced(g.id).await
}

/// Tells the channels of giveaways that ended without being announced (the pod that ended them
/// died, or Discord was unreachable), once their claim has lapsed.
pub(crate) async fn announce_pending(app: &Arc<App>) {
    let pending = match app.db.unannounced_giveaways(20).await {
        Ok(pending) => pending,
        Err(error) => return tracing::warn!(%error, "unannounced giveaways not loaded"),
    };
    for giveaway in pending {
        if let Err(error) = announce(app, &giveaway).await {
            tracing::warn!(%error, giveaway = giveaway.id, "giveaway result not announced");
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::commands::community::giveaway::*;

    fn giveaway(running: bool) -> GiveawayRow {
        GiveawayRow {
            id: 12,
            guild_id: 1,
            channel_id: 2,
            message_id: Some(3),
            host_id: 4,
            prize: "Golden shell".to_owned(),
            shells: 50,
            winners: 2,
            required_role_id: Some(5),
            ends_at: Utc::now(),
            starts_at: None,
            min_level: 0,
            min_account_days: 0,
            min_server_days: 0,
            bonus_role_id: None,
            bonus_entries: 0,
            ended_at: (!running).then(Utc::now),
            status: if running { "running" } else { "ended" }.to_owned(),
            winner_ids: if running { Vec::new() } else { vec![7, 8] },
            announced: false,
            entries: 9,
        }
    }

    #[test]
    fn button_ids_round_trip() {
        let id = button_id(12);
        let mut parts = id.split(':');
        assert_eq!(parts.next(), Some("gw"));
        assert_eq!(parse_button(parts), Some(12));
        assert!(id.len() <= 100);
    }

    #[test]
    fn bad_button_ids_are_refused() {
        assert_eq!(parse_button("".split(':')), None);
        assert_eq!(parse_button("e".split(':')), None);
        assert_eq!(parse_button("x:1".split(':')), None);
        assert_eq!(parse_button("e:one".split(':')), None);
        assert_eq!(parse_button("e:1:2".split(':')), None);
    }

    #[test]
    fn a_running_giveaway_shows_its_terms_and_entry_count() {
        let embed = giveaway_embed(Lang::En, &giveaway(true));
        let text = embed.description.expect("a description");
        for part in [
            "Hosted by <@4>",
            "Winners: **2**",
            "**50** shells each",
            "Requires <@&5>",
        ] {
            assert!(text.contains(part), "{part} missing from {text}");
        }
        assert_eq!(embed.fields[0].name, "Entries");
        assert_eq!(embed.fields[0].value, "9");
        let Component::ActionRow(buttons) = &giveaway_components(Lang::En, &giveaway(true))[0] else {
            panic!("no row");
        };
        let Component::Button(enter) = &buttons.components[0] else {
            panic!("no button");
        };
        assert!(!enter.disabled);
        assert_eq!(enter.custom_id.as_deref(), Some("gw:e:12"));
    }

    #[test]
    fn an_ended_giveaway_names_its_winners_and_locks_the_button() {
        let ended = giveaway(false);
        let embed = giveaway_embed(Lang::En, &ended);
        assert_eq!(embed.fields[0].value, "<@7>, <@8>");
        let Component::ActionRow(buttons) = &giveaway_components(Lang::En, &ended)[0] else {
            panic!("no row");
        };
        let Component::Button(enter) = &buttons.components[0] else {
            panic!("no button");
        };
        assert!(enter.disabled);
        let nobody = GiveawayRow {
            winner_ids: Vec::new(),
            ..ended
        };
        assert_eq!(giveaway_embed(Lang::En, &nobody).fields[0].value, "Nobody entered");
    }
}
