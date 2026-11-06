//! `/surge`: a storm surge the whole server fights together.
//!
//! A storm lasts ten minutes. Everyone presses a button to stack sandbags; the sea washes one away every
//! half a minute and the wall's goal grows with every helper who joins. Hold the wall and everyone who
//! helped is paid a little more than if the sea wins. Managers call a storm with `/surge`, or a server
//! turns on timed storms with `/config surge`. The card is refreshed by the janitor, so clicks never
//! wait for a picture.

use std::{str::Split, sync::Arc};

use chrono::{Duration as Span, Utc};
use pinchora_db::{ACTIVE, GameSession, NewGame, PartySettings, SurgeHelp, SurgeSettled};
use pinchora_game::{
    party::surge::{
        self, DAILY_SHELLS, DURATION_SECS, EVERY_CHOICES, HELP_COOLDOWN_SECS, MAX_HELPS, MIN_EVERY_HOURS,
        SUMMON_COOLDOWN_SECS,
    },
    play::Game,
};
use pinchora_render::{SurgeCard, SurgeState, Theme};
use twilight_interactions::command::{CommandModel, CommandOption, CreateCommand, CreateOption};
use twilight_model::{
    application::interaction::InteractionChannel,
    channel::message::{Component, component::ButtonStyle},
    guild::Permissions,
    http::interaction::InteractionResponseData,
};
use twilight_util::builder::embed::EmbedBuilder;

use super::{
    games::{channel_of, location, quiet},
    party::{clean_name, count, edit_card, post_card, rest, resting, track_message, unrest, wait_text},
};
use crate::{
    App, Cx,
    server_events::{button, row},
    util::{BRAND, DANGER, SUCCESS, mention, relative, thousands},
};

fn manage_guild() -> Permissions {
    Permissions::MANAGE_GUILD
}

#[derive(CommandModel, CreateCommand)]
#[command(
    name = "surge",
    desc = "Call a storm surge the whole server has to hold back (managers)",
    default_permissions = "manage_guild",
    contexts = "guild"
)]
pub struct SurgeCommand;

#[derive(CommandOption, CreateOption, Clone, Copy, Debug, PartialEq, Eq)]
pub enum Every {
    #[option(name = "Every 6 hours", value = 6)]
    Six,
    #[option(name = "Every 12 hours", value = 12)]
    Twelve,
    #[option(name = "Every day", value = 24)]
    Day,
    #[option(name = "Every 2 days", value = 48)]
    TwoDays,
    #[option(name = "Every week", value = 168)]
    Week,
}

impl Every {
    fn hours(self) -> i32 {
        match self {
            Self::Six => 6,
            Self::Twelve => 12,
            Self::Day => 24,
            Self::TwoDays => 48,
            Self::Week => 168,
        }
    }
}

#[derive(CommandModel, CreateCommand)]
#[command(
    name = "surge",
    desc = "Timed storm surges: switch them on, pick the channel and how often one comes"
)]
pub struct ConfigSurge {
    /// Turn timed storms on or off (managers can always call one with /surge)
    enabled: Option<bool>,
    /// The channel storms come to
    #[command(channel_types = "guild_text guild_announcement")]
    channel: Option<InteractionChannel>,
    /// How often a storm comes (default: every day)
    every: Option<Every>,
}

/// What `/config surge` says about the settings.
fn settings_text(settings: &PartySettings) -> String {
    let when = settings
        .surge_next_at
        .map(|at| format!(" The next one comes {}.", relative(at)))
        .unwrap_or_default();
    match (settings.surge_enabled, settings.surge_channel_id) {
        (true, Some(channel)) => format!(
            "⛈️ Timed storm surges are **on**: one comes to <#{channel}> every **{}** hours.{when}",
            settings.surge_every_hours
        ),
        (true, None) => {
            "⛈️ Timed storm surges are **on**, but they need a channel: pick one with `channel:`.".to_owned()
        }
        (false, _) => "⛈️ Timed storm surges are **off**. Managers can still call a storm with `/surge`.".to_owned(),
    }
}

impl ConfigSurge {
    pub(crate) async fn run(self, cx: &Cx, guild: u64) -> anyhow::Result<()> {
        if !cx.can_manage_guild() {
            return cx
                .reply_ephemeral("Only members with Manage Server can change timed storms.")
                .await;
        }
        let hours = self.every.map(Every::hours);
        debug_assert!(hours.is_none_or(|h| EVERY_CHOICES.contains(&h) && h >= MIN_EVERY_HOURS));
        // With no options it only shows the settings: saving would push the next storm back.
        let settings = if self.enabled.is_none() && self.channel.is_none() && hours.is_none() {
            cx.app.db.party_settings(guild).await?
        } else {
            cx.app
                .db
                .set_surge(
                    guild,
                    self.enabled,
                    self.channel.map(|channel| channel.id.get()),
                    hours,
                    cx.app.now(),
                )
                .await?
        };
        cx.reply_ephemeral(settings_text(&settings)).await
    }
}

/// The wall as it stands: sandbags stacked, helpers, how tall the wall is against its goal.
struct Snapshot {
    wall: i64,
    goal: i64,
    helpers: i64,
    elapsed: i64,
    top: Vec<(String, i64)>,
}

impl Snapshot {
    fn of(session: &GameSession, members: &[pinchora_db::PartyMember]) -> Self {
        let stacked: i64 = members.iter().map(|m| m.amount).sum();
        let helpers = members.len() as i64;
        let start = session.expires_at - Span::seconds(DURATION_SECS);
        let elapsed = (Utc::now() - start).num_seconds().clamp(0, DURATION_SECS);
        let mut best: Vec<&pinchora_db::PartyMember> = members.iter().collect();
        best.sort_by_key(|m| std::cmp::Reverse(m.amount));
        Self {
            wall: surge::wall(stacked, elapsed),
            goal: surge::goal(helpers),
            helpers,
            elapsed,
            top: best.iter().take(3).map(|m| (m.name.clone(), m.amount)).collect(),
        }
    }

    /// What the janitor compares to know the card on screen is out of date.
    fn key(&self, stacked: i64) -> String {
        format!("{stacked}:{}:{}", self.helpers, surge::erosion(self.elapsed))
    }
}

async fn snapshot(app: &App, session: &GameSession) -> anyhow::Result<(Snapshot, i64)> {
    let members = app.db.party_members(session.id).await?;
    let stacked = members.iter().map(|m| m.amount).sum();
    Ok((Snapshot::of(session, &members), stacked))
}

async fn card(app: &App, snap: &Snapshot, state: SurgeState) -> anyhow::Result<Vec<u8>> {
    let (wall, goal, helpers) = (snap.wall, snap.goal, snap.helpers);
    let top = snap.top.clone();
    app.render(move |r| {
        r.surge_card(&SurgeCard {
            wall,
            goal,
            helpers,
            top: &top,
            state,
            theme: Theme::reef(),
        })
    })
    .await
}

fn buttons(id: i64) -> Vec<Component> {
    vec![row(vec![button(
        format!("ss:h:{id}"),
        "Stack a sandbag",
        Some("🧱"),
        ButtonStyle::Primary,
        false,
    )])]
}

fn embed(color: u32) -> EmbedBuilder {
    EmbedBuilder::new().color(color).title("Storm Surge")
}

fn announcement(session: &GameSession) -> String {
    format!(
        "⛈️ **Storm surge!** A huge wave is rolling in. Everyone: stack sandbags to hold the wall! The sea washes one \
         away every {} s, and the wall has to grow with every helper who joins. The storm breaks {}.",
        surge::ERODE_SECS,
        relative(session.expires_at)
    )
}

/// The message of a storm that is running.
async fn rolling(app: &App, session: &GameSession) -> anyhow::Result<InteractionResponseData> {
    let (snap, _) = snapshot(app, session).await?;
    let png = card(app, &snap, SurgeState::Rolling).await?;
    let mut data = Cx::card(embed(BRAND), "surge.png", png, buttons(session.id), false);
    data.content = Some(announcement(session));
    data.allowed_mentions = quiet();
    Ok(data)
}

/// Starts a storm in `channel`, if the server has none running.
async fn open_storm(app: &App, guild: u64, channel: u64, host: u64) -> anyhow::Result<Option<GameSession>> {
    app.db
        .open_party_game(NewGame {
            kind: "surge",
            guild_id: guild,
            channel_id: channel,
            host,
            guest: None,
            bet: 0,
            status: ACTIVE,
            state: "0:0:0",
            ttl_secs: DURATION_SECS,
        })
        .await
}

impl SurgeCommand {
    pub async fn run(self, cx: &Cx) -> anyhow::Result<()> {
        let Some(guild) = cx.guild_id() else {
            return cx.reply_ephemeral("Storms come to servers.").await;
        };
        if !cx.can_manage_guild() {
            return cx
                .reply_ephemeral("Only members with Manage Server can call a storm.")
                .await;
        }
        if let Some(left) = resting(&cx.app, "surge", guild).await? {
            return cx
                .reply_ephemeral(format!(
                    "The sea needs a breather: the next storm can be called in {}.",
                    wait_text(left)
                ))
                .await;
        }
        let me = cx.author()?.id.get();
        let Some(session) = open_storm(&cx.app, guild, channel_of(cx), me).await? else {
            return cx
                .reply_ephemeral("A storm is already raging on this server. Join the fight!")
                .await;
        };
        rest(&cx.app, "surge", guild, SUMMON_COOLDOWN_SECS).await;
        cx.reply(rolling(&cx.app, &session).await?).await?;
        track_message(cx, session.id);
        Ok(())
    }
}

/// `ss:h:<id>` stacks a sandbag.
pub(crate) async fn on_button(cx: &Cx, mut args: Split<'_, char>) -> anyhow::Result<()> {
    let (Some("h"), Some(id)) = (args.next(), args.next().and_then(|v| v.parse::<i64>().ok())) else {
        return cx.reply_ephemeral("This button has expired.").await;
    };
    let me = cx.author()?.id.get();
    let kind = format!("surgehelp:{id}");
    if let Some(left) = resting(&cx.app, &kind, me).await? {
        return cx
            .reply_ephemeral(format!(
                "Your claws need a rest: stack the next one in {}.",
                wait_text(left)
            ))
            .await;
    }
    rest(&cx.app, &kind, me, HELP_COOLDOWN_SECS).await;
    let helped = cx
        .app
        .db
        .surge_help(id, me, &clean_name(&cx.author_display_name()), MAX_HELPS)
        .await?;
    let (session, mine, stacked, helpers) = match helped {
        SurgeHelp::Helped {
            session,
            mine,
            stacked,
            helpers,
        } => (session, mine, stacked, helpers),
        SurgeHelp::Over => {
            unrest(&cx.app, &kind, me).await;
            return cx.reply_ephemeral("This storm has passed.").await;
        }
        SurgeHelp::Capped => {
            unrest(&cx.app, &kind, me).await;
            return cx
                .reply_ephemeral(format!(
                    "You've stacked **{MAX_HELPS}** sandbags, as many as one crab can. Cheer the others on!"
                ))
                .await;
        }
    };
    let elapsed = (Utc::now() - (session.expires_at - Span::seconds(DURATION_SECS))).num_seconds();
    let (wall, goal) = (surge::wall(stacked, elapsed), surge::goal(helpers));
    if wall < goal {
        return cx
            .reply_ephemeral(format!(
                "🧱 Sandbag **{mine}** stacked! The wall stands at **{wall}** of **{goal}**."
            ))
            .await;
    }
    // The wall holds: end the storm now and pay everyone.
    match finish(&cx.app, &session, true, false).await? {
        Some(data) => cx.update(data).await,
        None => cx.reply_ephemeral("The wall already holds, nice work!").await,
    }
}

/// Ends a storm and builds the message that replaces its card. `None` when somebody ended it first.
async fn finish(
    app: &App,
    session: &GameSession,
    held: bool,
    only_if_expired: bool,
) -> anyhow::Result<Option<InteractionResponseData>> {
    let Some(settled) = app
        .db
        .surge_settle(session.id, held, only_if_expired, DAILY_SHELLS, surge::reward)
        .await?
    else {
        return Ok(None);
    };
    let played: Vec<(u64, bool)> = settled.helpers.iter().map(|(h, _)| (h.user_id as u64, held)).collect();
    count(app, Game::Surge, &played).await;
    Ok(Some(ended(app, session, &settled).await?))
}

/// The message of a storm that is over: the last card and who got what.
async fn ended(app: &App, session: &GameSession, settled: &SurgeSettled) -> anyhow::Result<InteractionResponseData> {
    let members: Vec<_> = settled.helpers.iter().map(|(m, _)| m.clone()).collect();
    let mut snap = Snapshot::of(session, &members);
    let state = if settled.held {
        snap.wall = snap.goal;
        SurgeState::Held
    } else {
        snap.wall = surge::wall(members.iter().map(|m| m.amount).sum(), DURATION_SECS);
        SurgeState::Broken
    };
    // The storm is over and paid whatever happens to the picture: without one the text still tells it.
    let png = card(app, &snap, state).await;
    if let Err(error) = &png {
        tracing::warn!(%error, session = session.id, "storm card not drawn");
    }
    let mut helpers: Vec<_> = settled.helpers.iter().collect();
    helpers.sort_by_key(|(m, _)| std::cmp::Reverse(m.amount));
    let lines: Vec<String> = helpers
        .iter()
        .take(10)
        .map(|(m, paid)| match paid {
            0 => format!("🧱 {} ×{}", mention(m.user_id), m.amount),
            shells => format!("🧱 {} ×{} **+{}**", mention(m.user_id), m.amount, thousands(*shells)),
        })
        .collect();
    let more = match helpers.len().saturating_sub(10) {
        0 => String::new(),
        n => format!("\n…and {n} more helpers."),
    };
    let headline = if settled.held {
        "🌤️ **The wall held!** The sea gave up and everyone who helped has been paid:"
    } else if helpers.is_empty() {
        "🌊 **The sea broke through.** Nobody came to help this time."
    } else {
        "🌊 **The sea broke through the wall.** Thanks for trying, the helpers got a little something:"
    };
    let content = format!("{headline}\n{}{more}", lines.join("\n"));
    let mut data = match png {
        Ok(png) => Cx::card(
            embed(if settled.held { SUCCESS } else { DANGER }),
            "surge.png",
            png,
            Vec::new(),
            false,
        ),
        Err(_) => InteractionResponseData {
            components: Some(Vec::new()),
            ..Default::default()
        },
    };
    data.content = Some(content);
    data.allowed_mentions = quiet();
    Ok(data)
}

/// Puts a built message on an existing one with the bot's token.
async fn publish(app: &App, at: (u64, u64), data: InteractionResponseData) {
    if let (Some(embeds), Some(files)) = (data.embeds, data.attachments)
        && let (Some(embed), Some(file)) = (embeds.into_iter().next(), files.into_iter().next())
    {
        edit_card(
            app,
            at,
            data.content.as_deref(),
            embed,
            (&file.filename, file.file),
            &data.components.unwrap_or_default(),
        )
        .await;
    } else {
        super::games::edit_message(app, at.0, at.1, data.content.as_deref(), None, Some(&[])).await;
    }
}

/// The janitor's end for a storm whose time ran out: the sea wins.
pub(crate) async fn time_out(app: &Arc<App>, session: &GameSession) -> anyhow::Result<bool> {
    let Some(data) = finish(app, session, false, true).await? else {
        return Ok(false);
    };
    if let Some(at) = location(session) {
        publish(app, at, data).await;
    }
    Ok(true)
}

/// One step of the janitor for storms: starts the timed storms that are due, and refreshes the card of
/// every running storm whose wall changed.
pub(crate) async fn tick(app: &Arc<App>) -> anyhow::Result<()> {
    for settings in app.db.due_surges(app.now(), 20).await? {
        if let Err(error) = start_timed(app, &settings).await {
            tracing::warn!(%error, guild = settings.guild_id, "timed storm not started");
        }
    }
    for session in app.db.running_surges(50).await? {
        if let Err(error) = refresh(app, &session).await {
            tracing::warn!(%error, session = session.id, "storm card not refreshed");
        }
    }
    Ok(())
}

/// Starts the timed storm of a server whose slot is due, if this pod is the one to take the slot.
async fn start_timed(app: &Arc<App>, settings: &PartySettings) -> anyhow::Result<()> {
    let (Some(due), Some(channel)) = (settings.surge_next_at, settings.surge_channel_id) else {
        return Ok(());
    };
    let guild = settings.guild_id as u64;
    if !app.db.claim_surge_slot(guild, due, app.now()).await? {
        return Ok(());
    }
    let Some(session) = open_storm(app, guild, channel as u64, 0).await? else {
        // A storm is already raging; the next one comes at the next slot.
        return Ok(());
    };
    let data = rolling(app, &session).await?;
    let (Some(embeds), Some(files)) = (data.embeds, data.attachments) else {
        return Ok(());
    };
    let (Some(embed), Some(file)) = (embeds.into_iter().next(), files.into_iter().next()) else {
        return Ok(());
    };
    let posted = post_card(
        app,
        channel as u64,
        data.content.as_deref().unwrap_or_default(),
        embed,
        (&file.filename, file.file),
        &data.components.unwrap_or_default(),
    )
    .await;
    match posted {
        Ok(message) => app.db.set_game_message(session.id, channel as u64, message).await,
        Err(error) => {
            // Nowhere to put it (the channel is gone or the bot cannot post): close the storm unseen.
            tracing::warn!(guild, channel, %error, "could not post a timed storm");
            app.db
                .surge_settle(session.id, false, false, DAILY_SHELLS, surge::reward)
                .await?;
            Ok(())
        }
    }
}

/// Redraws a running storm's card when its wall, its helpers or the erosion changed since the last draw.
async fn refresh(app: &App, session: &GameSession) -> anyhow::Result<()> {
    let (snap, stacked) = snapshot(app, session).await?;
    let key = snap.key(stacked);
    if key == session.state || !app.db.touch_game_state(session.id, &session.state, &key).await? {
        return Ok(());
    }
    let Some(at) = location(session) else {
        return Ok(());
    };
    publish(app, at, rolling(app, session).await?).await;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn storm(minutes_left: i64) -> GameSession {
        GameSession {
            id: 3,
            kind: "surge".into(),
            guild_id: 1,
            channel_id: 2,
            message_id: None,
            host_id: 4,
            guest_id: None,
            bet: 0,
            status: ACTIVE.into(),
            state: "0:0:0".into(),
            expires_at: Utc::now() + Span::minutes(minutes_left),
        }
    }

    fn helper(user: i64, bags: i64) -> pinchora_db::PartyMember {
        pinchora_db::PartyMember {
            user_id: user,
            choice: 0,
            amount: bags,
            name: format!("H{user}"),
        }
    }

    #[test]
    fn the_snapshot_counts_sandbags_erosion_and_the_goal() {
        // Five minutes in: ten washed away.
        let members = [helper(1, 20), helper(2, 12), helper(3, 3)];
        let snap = Snapshot::of(&storm(5), &members);
        assert_eq!(snap.helpers, 3);
        assert_eq!(snap.goal, surge::goal(3));
        assert_eq!(snap.wall, 35 - 10);
        assert_eq!(snap.top[0], ("H1".to_owned(), 20));
        assert_eq!(snap.top.len(), 3);
        assert_eq!(snap.key(35), "35:3:10");
        // A storm that just began has lost nothing.
        assert_eq!(Snapshot::of(&storm(10), &members).wall, 35);
    }

    #[test]
    fn the_settings_text_names_the_channel_and_the_way_out() {
        let mut settings = PartySettings {
            guild_id: 1,
            surge_enabled: false,
            surge_channel_id: None,
            surge_every_hours: 24,
            surge_next_at: None,
        };
        assert!(settings_text(&settings).contains("**off**") && settings_text(&settings).contains("/surge"));
        settings.surge_enabled = true;
        assert!(settings_text(&settings).contains("need a channel"));
        settings.surge_channel_id = Some(55);
        assert!(settings_text(&settings).contains("<#55>") && settings_text(&settings).contains("24"));
    }

    #[test]
    fn every_choice_is_a_legal_interval() {
        for every in [Every::Six, Every::Twelve, Every::Day, Every::TwoDays, Every::Week] {
            assert!(EVERY_CHOICES.contains(&every.hours()) && every.hours() >= MIN_EVERY_HOURS);
        }
    }
}
