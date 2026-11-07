//! `/surge`: a storm surge the whole server fights together.
//!
//! A storm lasts ten minutes. Everyone presses a button to stack sandbags; the sea washes one away every
//! half a minute and the wall's goal grows with every helper who joins. Hold the wall and everyone who
//! helped is paid a little more than if the sea wins. Managers call a storm with `/surge`, or a server
//! turns on timed storms with `/config surge`. The card is refreshed by the janitor, so clicks never
//! wait for a picture.

use std::{str::Split, sync::Arc};

use chrono::{Duration as Span, Utc};
use pinchora_core::{i18n::Lang, modules::Module, t};
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
    games::{channel_of, location, quiet, session_lang, state_with_lang},
    party::{clean_name, count, edit_card, post_card, rest, resting, track_message, unrest, wait_text_in},
};
use crate::{
    App, Cx,
    server_events::{button, row},
    util::{BRAND, DANGER, SUCCESS, mention, relative, thousands_in},
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
fn settings_text(settings: &PartySettings, lang: Lang) -> String {
    let when = settings
        .surge_next_at
        .map(|at| t!(lang, "surge.config.next", when = relative(at)))
        .unwrap_or_default();
    match (settings.surge_enabled, settings.surge_channel_id) {
        (true, Some(channel)) => t!(
            lang,
            "surge.config.on",
            channel = channel,
            hours = settings.surge_every_hours,
            when = when
        ),
        (true, None) => t!(lang, "surge.config.on-no-channel"),
        (false, _) => t!(lang, "surge.config.off"),
    }
}

impl ConfigSurge {
    pub(crate) async fn run(self, cx: &Cx, guild: u64) -> anyhow::Result<()> {
        if !cx.can_manage_guild() {
            return cx.reply_ephemeral(t!(cx.lang(), "surge.config.denied")).await;
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
        cx.reply_ephemeral(settings_text(&settings, cx.lang())).await
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

async fn card(app: &App, snap: &Snapshot, state: SurgeState, lang: Lang) -> anyhow::Result<Vec<u8>> {
    let (wall, goal, helpers) = (snap.wall, snap.goal, snap.helpers);
    let top = snap.top.clone();
    app.render(move |r| {
        r.surge_card(&SurgeCard {
            lang,
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

fn buttons(id: i64, lang: Lang) -> Vec<Component> {
    vec![row(vec![button(
        format!("ss:h:{id}"),
        &t!(lang, "surge.button"),
        Some("🧱"),
        ButtonStyle::Primary,
        false,
    )])]
}

fn embed(color: u32, lang: Lang) -> EmbedBuilder {
    EmbedBuilder::new().color(color).title(t!(lang, "surge.title"))
}

fn announcement(session: &GameSession, lang: Lang) -> String {
    t!(
        lang,
        "surge.announcement",
        secs = surge::ERODE_SECS,
        when = relative(session.expires_at)
    )
}

/// The message of a storm that is running.
async fn rolling(app: &App, session: &GameSession, lang: Lang) -> anyhow::Result<InteractionResponseData> {
    let (snap, _) = snapshot(app, session).await?;
    let png = card(app, &snap, SurgeState::Rolling, lang).await?;
    let mut data = Cx::card(embed(BRAND, lang), "surge.png", png, buttons(session.id, lang), false);
    data.content = Some(announcement(session, lang));
    data.allowed_mentions = quiet();
    Ok(data)
}

/// Starts a storm in `channel`, if the server has none running.
async fn open_storm(app: &App, guild: u64, channel: u64, host: u64, lang: Lang) -> anyhow::Result<Option<GameSession>> {
    app.db
        .open_party_game(NewGame {
            kind: "surge",
            guild_id: guild,
            channel_id: channel,
            host,
            guest: None,
            bet: 0,
            status: ACTIVE,
            state: &state_with_lang("0:0:0", lang),
            ttl_secs: DURATION_SECS,
        })
        .await
}

impl SurgeCommand {
    pub async fn run(self, cx: &Cx) -> anyhow::Result<()> {
        let lang = cx.lang();
        let Some(guild) = cx.guild_id() else {
            return cx.reply_ephemeral(t!(lang, "surge.server-only")).await;
        };
        if !cx.can_manage_guild() {
            return cx.reply_ephemeral(t!(lang, "surge.denied")).await;
        }
        if let Some(left) = resting(&cx.app, "surge", guild).await? {
            return cx
                .reply_ephemeral(t!(lang, "surge.breather", wait = wait_text_in(lang, left)))
                .await;
        }
        let me = cx.author()?.id.get();
        // The storm card is public and keeps speaking the server's language to its end.
        let board_lang = cx.app.guild_lang(guild).await;
        let Some(session) = open_storm(&cx.app, guild, channel_of(cx), me, board_lang).await? else {
            return cx.reply_ephemeral(t!(lang, "surge.raging")).await;
        };
        rest(&cx.app, "surge", guild, SUMMON_COOLDOWN_SECS).await;
        cx.reply(rolling(&cx.app, &session, board_lang).await?).await?;
        track_message(cx, session.id);
        Ok(())
    }
}

/// `ss:h:<id>` stacks a sandbag.
pub(crate) async fn on_button(cx: &Cx, mut args: Split<'_, char>) -> anyhow::Result<()> {
    let (Some("h"), Some(id)) = (args.next(), args.next().and_then(|v| v.parse::<i64>().ok())) else {
        return cx.reply_ephemeral(t!(cx.lang(), "arcade.button-expired")).await;
    };
    let me = cx.author()?.id.get();
    let lang = cx.lang();
    let kind = format!("surgehelp:{id}");
    if let Some(left) = resting(&cx.app, &kind, me).await? {
        return cx
            .reply_ephemeral(t!(lang, "surge.claws", wait = wait_text_in(lang, left)))
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
            return cx.reply_ephemeral(t!(lang, "surge.passed")).await;
        }
        SurgeHelp::Capped => {
            unrest(&cx.app, &kind, me).await;
            return cx.reply_ephemeral(t!(lang, "surge.capped", max = MAX_HELPS)).await;
        }
    };
    let elapsed = (Utc::now() - (session.expires_at - Span::seconds(DURATION_SECS))).num_seconds();
    let (wall, goal) = (surge::wall(stacked, elapsed), surge::goal(helpers));
    if wall < goal {
        return cx
            .reply_ephemeral(t!(lang, "surge.stacked", mine = mine, wall = wall, goal = goal))
            .await;
    }
    // The wall holds: end the storm now and pay everyone, on the card in the storm's own language.
    let table = session_lang(&cx.app, &session).await;
    match finish(&cx.app, &session, true, false, table).await? {
        Some(data) => cx.update(data).await,
        None => cx.reply_ephemeral(t!(lang, "surge.holds")).await,
    }
}

/// Ends a storm and builds the message that replaces its card. `None` when somebody ended it first.
async fn finish(
    app: &App,
    session: &GameSession,
    held: bool,
    only_if_expired: bool,
    lang: Lang,
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
    Ok(Some(ended(app, session, &settled, lang).await?))
}

/// The message of a storm that is over: the last card and who got what.
async fn ended(
    app: &App,
    session: &GameSession,
    settled: &SurgeSettled,
    lang: Lang,
) -> anyhow::Result<InteractionResponseData> {
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
    let png = card(app, &snap, state, lang).await;
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
            shells => format!(
                "🧱 {} ×{} **+{}**",
                mention(m.user_id),
                m.amount,
                thousands_in(lang, *shells)
            ),
        })
        .collect();
    let more = match helpers.len().saturating_sub(10) {
        0 => String::new(),
        n => t!(lang, "surge.more", n = n),
    };
    let headline = if settled.held {
        t!(lang, "surge.held")
    } else if helpers.is_empty() {
        t!(lang, "surge.broke-nobody")
    } else {
        t!(lang, "surge.broke")
    };
    let content = format!("{headline}\n{}{more}", lines.join("\n"));
    let mut data = match png {
        Ok(png) => Cx::card(
            embed(if settled.held { SUCCESS } else { DANGER }, lang),
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
    let lang = session_lang(app, session).await;
    let Some(data) = finish(app, session, false, true, lang).await? else {
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
    // Taken first: a storm skipped because party games are switched off waits for the next slot,
    // instead of arriving the moment they are switched on again.
    if !app.module_allowed(guild, Some(channel as u64), Module::Party).await {
        return Ok(());
    }
    let lang = app.guild_lang(guild).await;
    let Some(session) = open_storm(app, guild, channel as u64, 0, lang).await? else {
        // A storm is already raging; the next one comes at the next slot.
        return Ok(());
    };
    let data = rolling(app, &session, lang).await?;
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
    let lang = session_lang(app, session).await;
    let key = state_with_lang(&snap.key(stacked), lang);
    if key == session.state || !app.db.touch_game_state(session.id, &session.state, &key).await? {
        return Ok(());
    }
    let Some(at) = location(session) else {
        return Ok(());
    };
    publish(app, at, rolling(app, session, lang).await?).await;
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
        let text = |settings: &PartySettings| settings_text(settings, Lang::En);
        assert!(text(&settings).contains("**off**") && text(&settings).contains("/surge"));
        settings.surge_enabled = true;
        assert!(text(&settings).contains("need a channel"));
        settings.surge_channel_id = Some(55);
        assert!(text(&settings).contains("<#55>") && text(&settings).contains("24"));
        assert!(settings_text(&settings, Lang::Tr).contains("<#55>"));
    }

    #[test]
    fn every_choice_is_a_legal_interval() {
        for every in [Every::Six, Every::Twelve, Every::Day, Every::TwoDays, Every::Week] {
            assert!(EVERY_CHOICES.contains(&every.hours()) && every.hours() >= MIN_EVERY_HOURS);
        }
    }
}
