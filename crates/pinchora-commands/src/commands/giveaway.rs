//! `/giveaway`: prize draws members join with a button, ended by the events ticker.
//!
//! A shells prize is paid by the host: `winners` x `shells` is taken from them when the giveaway starts,
//! winners are paid when it ends and whatever nobody won goes back to the host, so a server can never
//! mint shells. Mentions in the prize are stripped and every message pings nobody.

use std::{sync::Arc, time::Duration};

use chrono::{DateTime, Utc};
use pinchora_core::server_events::{GIVEAWAY_MAX, GIVEAWAY_MIN, parse_duration, pick_winners};
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

use crate::{
    App, Cx,
    commands::games::{edit_message, quiet},
    server_events::{button, is_permanent, message_link, post_refusal, row, tell},
    util::{BRAND, SUCCESS, relative, single_line, strip_mentions, thousands},
};

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

/// Picks winners uniformly from `pool`.
fn draw(pool: &[u64], count: usize) -> Vec<u64> {
    pick_winners(pool, count, |n| rand::random_range(0..n))
}

fn mentions(ids: &[i64]) -> String {
    ids.iter().map(|id| format!("<@{id}>")).collect::<Vec<_>>().join(", ")
}

fn timestamp(at: DateTime<Utc>) -> String {
    format!("<t:{}:f>", at.timestamp())
}

/// The giveaway as its message shows it: details and the entry count while it runs, the winners after.
fn giveaway_embed(g: &GiveawayRow) -> Embed {
    let mut lines = Vec::new();
    if g.is_running() {
        lines.push("Press **Enter** to join. Press it again to leave.".to_owned());
        lines.push(String::new());
    }
    lines.push(format!("Hosted by <@{}>", g.host_id));
    if g.is_running() {
        lines.push(format!("Ends {} ({})", relative(g.ends_at), timestamp(g.ends_at)));
    } else {
        lines.push(format!("Ended {}", relative(g.ended_at.unwrap_or(g.ends_at))));
    }
    lines.push(format!("Winners: **{}**", g.winners));
    if g.shells > 0 {
        lines.push(format!("Prize: **{}** shells each", thousands(g.shells)));
    }
    if let Some(role) = g.required_role_id {
        lines.push(format!("Requires <@&{role}>"));
    }
    let mut embed = EmbedBuilder::new()
        .color(if g.is_running() { BRAND } else { SUCCESS })
        .title(&g.prize)
        .description(lines.join("\n"));
    embed = if g.is_running() {
        embed.field(EmbedFieldBuilder::new("Entries", g.entries.to_string()).inline())
    } else if g.winner_ids.is_empty() {
        embed.field(EmbedFieldBuilder::new("Winners", "Nobody entered"))
    } else {
        embed.field(EmbedFieldBuilder::new("Winners", mentions(&g.winner_ids)))
    };
    embed
        .footer(EmbedFooterBuilder::new(format!("Giveaway #{}", g.id)))
        .build()
}

fn giveaway_components(g: &GiveawayRow) -> Vec<Component> {
    let (label, style) = if g.is_running() {
        ("Enter", ButtonStyle::Primary)
    } else {
        ("Ended", ButtonStyle::Secondary)
    };
    vec![row(vec![button(
        button_id(g.id),
        label,
        Some("🎉"),
        style,
        !g.is_running(),
    )])]
}

impl GiveawayCommand {
    pub async fn run(self, cx: &Cx) -> anyhow::Result<()> {
        let Some(guild) = cx.guild_id() else {
            return tell(cx, "Giveaways only exist inside a server.").await;
        };
        if !cx.can_manage_guild() {
            return tell(cx, "You need the Manage Server permission to run giveaways.").await;
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
        return tell(cx, "I couldn't read that duration. Try `30m`, `2h`, `1d` or `1d12h`.").await;
    };
    if span < GIVEAWAY_MIN || span > GIVEAWAY_MAX {
        return tell(cx, "A giveaway runs from 1 minute to 30 days.").await;
    }
    let prize = single_line(&strip_mentions(&c.prize));
    if prize.is_empty() {
        return tell(cx, "Give the giveaway a prize.").await;
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
            ends_at: cx.app.now() + span,
        })
        .await?;
    let giveaway = match opened {
        GiveawayOpen::Started(giveaway) => *giveaway,
        GiveawayOpen::Broke { balance } => {
            return tell(
                cx,
                format!(
                    "A shells prize is paid by you: {} winner{} x {} shells = **{}** shells, taken now. You have {}.",
                    c.winners,
                    if c.winners == 1 { "" } else { "s" },
                    thousands(shells),
                    thousands(shells * c.winners),
                    thousands(balance)
                ),
            )
            .await;
        }
        GiveawayOpen::Full => {
            return tell(
                cx,
                format!("This server already runs {MAX_RUNNING_GIVEAWAYS} giveaways. End one first."),
            )
            .await;
        }
    };

    let posted = cx
        .app
        .http
        .create_message(channel)
        .embeds(&[giveaway_embed(&giveaway)])
        .components(&giveaway_components(&giveaway))
        .allowed_mentions(quiet().as_ref())
        .await;
    let message = match posted {
        Ok(response) => response.model().await?,
        Err(error) => {
            tracing::warn!(guild, giveaway = giveaway.id, %error, "could not post a giveaway");
            cx.app.db.abort_giveaway(giveaway.id).await?;
            return tell(cx, post_refusal(channel, &error)).await;
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
        format!(
            " {} shells were taken from you to pay the winners; whatever nobody wins comes back.",
            thousands(giveaway.escrow())
        )
    } else {
        String::new()
    };
    tell(
        cx,
        format!(
            "Giveaway **#{}** is live in <#{channel}>: {}{held}",
            giveaway.id,
            message_link(guild, channel.get(), message.id.get())
        ),
    )
    .await
}

/// The giveaway numbered `id` in this server, or a reply saying there is none (and `None`).
async fn find(cx: &Cx, guild: u64, id: i64) -> anyhow::Result<Option<GiveawayRow>> {
    let found = cx.app.db.giveaway(id).await?.filter(|g| g.guild_id as u64 == guild);
    if found.is_none() {
        tell(cx, format!("There is no giveaway #{id} in this server.")).await?;
    }
    Ok(found)
}

async fn end_now(cx: &Cx, guild: u64, id: i64) -> anyhow::Result<()> {
    let Some(giveaway) = find(cx, guild, id).await? else {
        return Ok(());
    };
    let Some(ended) = end(&cx.app, giveaway.id, true).await? else {
        return tell(
            cx,
            format!("Giveaway #{id} already ended. Use `/giveaway reroll` to pick someone else."),
        )
        .await;
    };
    let text = if ended.winners.is_empty() {
        format!("Ended giveaway **#{id}**: nobody had entered, so there is no winner.")
    } else {
        format!("Ended giveaway **#{id}**: {}", mentions(&ended.giveaway.winner_ids))
    };
    tell(cx, text).await
}

async fn reroll(cx: &Cx, guild: u64, c: GiveawayReroll) -> anyhow::Result<()> {
    let count = c.winners.unwrap_or(1) as usize;
    let outcome = cx.app.db.reroll_giveaway(guild, c.id, count, draw).await?;
    let (giveaway, picked) = match outcome {
        Reroll::Done(giveaway, picked) => (*giveaway, picked),
        Reroll::Missing => return tell(cx, format!("There is no giveaway #{} in this server.", c.id)).await,
        Reroll::StillRunning => {
            return tell(
                cx,
                format!(
                    "Giveaway #{} is still running. End it first with `/giveaway end`.",
                    c.id
                ),
            )
            .await;
        }
        Reroll::NoOneLeft => {
            return tell(
                cx,
                format!(
                    "Nobody is left to pick for giveaway #{}: every entrant has already won.",
                    c.id
                ),
            )
            .await;
        }
    };
    let winners = mentions(&picked.iter().map(|w| *w as i64).collect::<Vec<_>>());
    // A reroll only picks people: the shells of a shells prize were paid when it ended.
    let text = format!(
        "🎉 New winner{} for **{}**: {winners}. Congratulations!{}",
        if picked.len() == 1 { "" } else { "s" },
        giveaway.prize,
        if giveaway.shells > 0 {
            " (no shells on a reroll)"
        } else {
            ""
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
        Ok(_) => format!("Announced in <#{channel}>."),
        Err(error) => {
            tracing::warn!(guild, giveaway = giveaway.id, %error, "could not announce a reroll");
            format!(
                "I couldn't announce it in <#{channel}>: {}",
                post_refusal(channel, &error)
            )
        }
    };
    tell(cx, format!("Rerolled giveaway **#{}**: {winners}. {told}", giveaway.id)).await
}

async fn list(cx: &Cx, guild: u64) -> anyhow::Result<()> {
    let running = cx.app.db.running_giveaways(guild).await?;
    if running.is_empty() {
        return tell(cx, "No giveaways are running. Start one with `/giveaway start`.").await;
    }
    let lines: Vec<String> = running
        .iter()
        .map(|g| {
            let prize: String = g.prize.chars().take(40).collect();
            let link = g.message_id.map_or_else(String::new, |m| {
                format!(" · [jump]({})", message_link(guild, g.channel_id as u64, m as u64))
            });
            format!(
                "**#{}** · {prize} · {} {} · ends {}{link}",
                g.id,
                g.entries,
                if g.entries == 1 { "entry" } else { "entries" },
                relative(g.ends_at)
            )
        })
        .collect();
    let embed = EmbedBuilder::new()
        .color(BRAND)
        .title("Running giveaways")
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
        return tell(cx, "This button has expired.").await;
    };
    let guild = cx.guild_id();
    let giveaway = cx
        .app
        .db
        .giveaway(id)
        .await?
        .filter(|g| Some(g.guild_id as u64) == guild);
    let Some(giveaway) = giveaway else {
        return tell(cx, "That giveaway no longer exists.").await;
    };
    let user = cx.author()?.id.get();
    if giveaway.host_id as u64 == user {
        return tell(cx, "You're hosting this giveaway, so you can't enter it.").await;
    }
    if let Some(role) = giveaway.required_role_id {
        let has_it = cx
            .interaction
            .member
            .as_ref()
            .is_some_and(|m| m.roles.iter().any(|r| r.get() == role as u64));
        if !has_it {
            return tell(cx, format!("You need <@&{role}> to enter this giveaway.")).await;
        }
    }
    let entered = cx.app.db.toggle_giveaway_entry(id, user, cx.app.now()).await?;
    let text = match entered {
        EntryToggle::Closed => return tell(cx, "This giveaway has ended.").await,
        EntryToggle::Entered(n) => format!(
            "You're in! **{}** is up for grabs. {n} {} so far.",
            giveaway.prize,
            if n == 1 { "entry" } else { "entries" }
        ),
        EntryToggle::Left(_) => "You left the giveaway. Press Enter again to rejoin.".to_owned(),
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
async fn refresh_message(app: &App, id: i64) {
    let giveaway = match app.db.giveaway(id).await {
        Ok(Some(g)) if g.is_running() => g,
        Ok(_) => return,
        Err(error) => return tracing::debug!(%error, giveaway = id, "giveaway not loaded"),
    };
    if let Some(message) = giveaway.message_id {
        edit_message(
            app,
            giveaway.channel_id as u64,
            message as u64,
            None,
            Some(&[giveaway_embed(&giveaway)]),
            None,
        )
        .await;
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
    let Some(ended) = app.db.end_giveaway(id, app.now(), early, draw).await? else {
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
    if let Some(message) = g.message_id {
        edit_message(
            app,
            g.channel_id as u64,
            message as u64,
            None,
            Some(&[giveaway_embed(g)]),
            Some(&giveaway_components(g)),
        )
        .await;
    }
    let winners = mentions(&g.winner_ids);
    let unpaid = g.shells * (i64::from(g.winners) - g.winner_ids.len() as i64).max(0);
    let mut text = if g.winner_ids.is_empty() {
        format!(
            "Nobody entered the giveaway for **{}**, so there is no winner.",
            g.prize
        )
    } else {
        format!("🎉 Congratulations {winners}! You won **{}**.", g.prize)
    };
    if g.shells > 0 && !g.winner_ids.is_empty() {
        text.push_str(&format!(" {} shells were paid to each winner.", thousands(g.shells)));
    }
    if unpaid > 0 {
        text.push_str(&format!(
            " {} unclaimed shells went back to <@{}>.",
            thousands(unpaid),
            g.host_id
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
    use super::*;

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
        let embed = giveaway_embed(&giveaway(true));
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
        let Component::ActionRow(buttons) = &giveaway_components(&giveaway(true))[0] else {
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
        let embed = giveaway_embed(&ended);
        assert_eq!(embed.fields[0].value, "<@7>, <@8>");
        let Component::ActionRow(buttons) = &giveaway_components(&ended)[0] else {
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
        assert_eq!(giveaway_embed(&nobody).fields[0].value, "Nobody entered");
    }
}
