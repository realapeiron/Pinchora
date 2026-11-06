//! The `/stats` cards, put together from the stored numbers: the server's, a member's and a
//! channel's, each over 7, 30 or 90 days, with the buttons that switch between them.

use std::collections::{HashMap, HashSet};

use chrono::{Datelike, NaiveDate, NaiveTime};
use futures_util::future::join_all;
use pinchora_core::{
    i18n::Lang,
    server_stats::{self, DayTotals, Period, grouped, minutes_text},
    t,
};
use pinchora_db::ActivityDay;
use pinchora_render::{StatFlows, StatHeat, StatList, StatRow, StatTile, StatsCard, Theme};
use twilight_model::{
    channel::message::{
        Component,
        component::{ActionRow, Button, ButtonStyle},
    },
    http::{attachment::Attachment, interaction::InteractionResponseData},
    id::Id,
};

use crate::{App, util::display_name};

/// What a card is about.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Scope {
    Server,
    Member(u64),
    Channel(u64),
}

impl Scope {
    /// The part of the buttons' custom id that names it: `s`, `m:<user>` or `c:<channel>`.
    fn id(self) -> String {
        match self {
            Self::Server => "s".to_owned(),
            Self::Member(user) => format!("m:{user}"),
            Self::Channel(channel) => format!("c:{channel}"),
        }
    }

    /// Reads [`Scope::id`] back from the parts of a custom id.
    pub(crate) fn parse<'a>(parts: &mut impl Iterator<Item = &'a str>) -> Option<Self> {
        match parts.next()? {
            "s" => Some(Self::Server),
            "m" => parts.next()?.parse().ok().map(Self::Member),
            "c" => parts.next()?.parse().ok().map(Self::Channel),
            _ => None,
        }
    }
}

/// How many rows the server card's lists and the dashboard's show.
pub(crate) const TOP: i64 = 5;

/// `8 Oct`, in `lang`.
fn day_text(day: NaiveDate, lang: Lang) -> String {
    t!(
        lang,
        "stats.card.day",
        day = day.day(),
        month = t!(lang, &format!("stats.month.{}", day.month()))
    )
}

fn pixel(text: String) -> String {
    text.to_uppercase()
}

fn first_moment(day: NaiveDate) -> chrono::DateTime<chrono::Utc> {
    day.and_time(NaiveTime::MIN).and_utc()
}

/// Every day of the period with the member's or channel's numbers, zeros where there were none.
fn fill(rows: &[ActivityDay], first: NaiveDate, days: i64) -> Vec<ActivityDay> {
    let by_day: HashMap<NaiveDate, &ActivityDay> = rows.iter().map(|r| (r.day, r)).collect();
    (0..days)
        .map(|i| {
            let day = first + chrono::Duration::days(i);
            by_day.get(&day).map_or(
                ActivityDay {
                    day,
                    messages: 0,
                    voice_minutes: 0,
                },
                |r| **r,
            )
        })
        .collect()
}

/// The server's name as Discord has it, or a plain stand-in.
async fn guild_name(app: &App, guild: u64, lang: Lang) -> String {
    match app.http.guild(Id::new(guild.max(1))).await {
        Ok(response) => match response.model().await {
            Ok(found) => return found.name,
            Err(error) => tracing::debug!(%error, guild, "server name unreadable"),
        },
        Err(error) => tracing::debug!(%error, guild, "server name unavailable"),
    }
    t!(lang, "stats.card.this-server")
}

/// Channel names as `#name`, asked of Discord one channel at a time (a few at most).
pub(crate) async fn channel_names(app: &App, ids: &[u64]) -> HashMap<u64, String> {
    let asks = ids.iter().map(|id| async move {
        let found = app.http.channel(Id::new((*id).max(1))).await.ok()?.model().await.ok()?;
        Some((*id, found.name?))
    });
    join_all(asks).await.into_iter().flatten().collect()
}

/// Members' names, from the cache of users the bot keeps.
pub(crate) async fn member_names(app: &App, ids: &[u64]) -> HashMap<u64, String> {
    let asks = ids
        .iter()
        .map(|id| async move { app.user(*id).await.map(|u| (*id, display_name(&u, None))) });
    join_all(asks).await.into_iter().flatten().collect()
}

struct Frame {
    first: NaiveDate,
    today: NaiveDate,
    days: i64,
}

impl Frame {
    fn new(app: &App, period: Period) -> Self {
        let today = app.now().date_naive();
        Self {
            first: period.first_day(today),
            today,
            days: period.days(),
        }
    }

    fn range(&self, lang: Lang) -> String {
        t!(
            lang,
            "stats.card.range",
            from = day_text(self.first, lang),
            to = day_text(self.today, lang),
            year = self.today.year()
        )
    }
}

fn base(app_lang: Lang, frame: &Frame, kicker: &str, title: String, period: Period) -> StatsCard {
    let lang = app_lang;
    StatsCard {
        kicker: pixel(t!(lang, kicker)),
        title,
        period: pixel(t!(lang, "stats.card.period", count = period.days())),
        range: frame.range(lang),
        tiles: Vec::new(),
        chart_title: pixel(t!(lang, "stats.card.messages-per-day")),
        days: Vec::new(),
        first_day: pixel(day_text(frame.first, lang)),
        last_day: pixel(day_text(frame.today, lang)),
        empty: t!(lang, "stats.card.empty"),
        flows: None,
        heat: None,
        lists: Vec::new(),
        theme: Theme::reef(),
    }
}

fn tile(lang: Lang, label: &str, value: String, note: Option<String>) -> StatTile {
    StatTile {
        label: pixel(t!(lang, label)),
        value,
        note,
    }
}

/// The server card.
pub(crate) async fn server_card(app: &App, guild: u64, period: Period, lang: Lang) -> anyhow::Result<StatsCard> {
    server_card_for(app, guild, period, lang, None).await
}

pub(crate) async fn server_card_for(
    app: &App,
    guild: u64,
    period: Period,
    lang: Lang,
    visible: Option<&HashSet<u64>>,
) -> anyhow::Result<StatsCard> {
    let frame = Frame::new(app, period);
    let hours = app.db.guild_hours(guild, first_moment(frame.first)).await?;
    let mut days: Vec<DayTotals> = server_stats::daily(&hours, frame.first, frame.days);
    if let Some(visible) = visible {
        let ids: Vec<i64> = visible.iter().map(|id| *id as i64).collect();
        let safe = app.db.public_visible_channel_days(guild, frame.first, &ids).await?;
        let by_day: HashMap<_, _> = safe.into_iter().map(|d| (d.day, d)).collect();
        for day in &mut days {
            day.messages = by_day.get(&day.day).map_or(0, |d| d.messages);
            day.voice_minutes = by_day.get(&day.day).map_or(0, |d| d.voice_minutes);
        }
    }
    let sum = server_stats::totals(&days);
    let (active, channels, members, settings) = tokio::try_join!(
        app.db.active_members(guild, frame.first),
        async {
            if visible.is_some() {
                app.db.public_top_channels(guild, frame.first, 100_000).await
            } else {
                app.db.top_channels(guild, frame.first, TOP).await
            }
        },
        app.db.top_members(guild, frame.first, TOP),
        app.guild_settings(guild),
    )?;
    let channels: Vec<_> = channels
        .into_iter()
        .filter(|c| visible.is_none_or(|ids| ids.contains(&(c.id as u64))))
        .take(TOP as usize)
        .collect();
    let members = if visible.is_some() { Vec::new() } else { members };
    let active = if visible.is_some() { 0 } else { active };
    let channel_ids: Vec<u64> = channels.iter().map(|c| c.id as u64).collect();
    let member_ids: Vec<u64> = members.iter().map(|m| m.id as u64).collect();
    let (title, channel_names, member_names) = tokio::join!(
        guild_name(app, guild, lang),
        channel_names(app, &channel_ids),
        member_names(app, &member_ids),
    );
    let mut card = base(lang, &frame, "stats.card.server-kicker", title, period);
    let voice_note = (!settings.voice_xp_enabled).then(|| t!(lang, "stats.card.voice-off"));
    card.tiles = vec![
        tile(
            lang,
            "stats.card.messages",
            grouped(sum.messages),
            visible
                .is_none()
                .then(|| t!(lang, "stats.card.members-wrote", count = active)),
        ),
        tile(
            lang,
            "stats.card.voice",
            minutes_text(sum.voice_minutes, lang),
            voice_note,
        ),
        tile(lang, "stats.card.joins", grouped(sum.joins), None),
        tile(
            lang,
            "stats.card.leaves",
            grouped(sum.leaves),
            Some(t!(lang, "stats.card.net", net = signed(sum.joins - sum.leaves))),
        ),
    ];
    card.days = days.iter().map(|d| d.messages).collect();
    card.flows = Some(StatFlows {
        title: pixel(t!(lang, "stats.card.joins-and-leaves")),
        joins: days.iter().map(|d| d.joins).collect(),
        leaves: days.iter().map(|d| d.leaves).collect(),
        joins_label: pixel(t!(lang, "stats.card.joins-total", count = grouped(sum.joins))),
        leaves_label: pixel(t!(lang, "stats.card.leaves-total", count = grouped(sum.leaves))),
    });
    card.heat = visible.is_none().then(|| StatHeat {
        title: pixel(t!(lang, "stats.card.busiest-hours")),
        grid: if visible.is_some() {
            [[0_i64; 24]; 7]
        } else {
            server_stats::heatmap(&hours)
        },
        weekdays: std::array::from_fn(|d| pixel(t!(lang, &format!("stats.weekday.{}", d + 1)))),
        less: pixel(t!(lang, "stats.card.quiet")),
        more: pixel(t!(lang, "stats.card.busy")),
    });
    let unknown_channel = t!(lang, "stats.card.unknown-channel");
    let unknown_member = t!(lang, "stats.card.unknown-member");
    card.lists = vec![
        StatList {
            title: pixel(t!(lang, "stats.card.top-channels")),
            rows: channels
                .iter()
                .map(|c| StatRow {
                    name: channel_names
                        .get(&(c.id as u64))
                        .map_or_else(|| unknown_channel.clone(), |n| format!("#{n}")),
                    value: c.messages,
                    shown: grouped(c.messages),
                })
                .collect(),
            empty: t!(lang, "stats.card.nobody-yet"),
        },
        StatList {
            title: pixel(t!(lang, "stats.card.top-members")),
            rows: members
                .iter()
                .map(|m| StatRow {
                    name: member_names
                        .get(&(m.id as u64))
                        .cloned()
                        .unwrap_or_else(|| unknown_member.clone()),
                    value: m.messages,
                    shown: grouped(m.messages),
                })
                .collect(),
            empty: t!(lang, "stats.card.nobody-yet"),
        },
    ];
    if visible.is_some() {
        card.lists.truncate(1);
    }
    Ok(card)
}

fn signed(n: i64) -> String {
    if n > 0 { format!("+{}", grouped(n)) } else { grouped(n) }
}

/// A member's card: `name` is how the server knows them.
pub(crate) async fn member_card(
    app: &App,
    guild: u64,
    user: u64,
    name: String,
    period: Period,
    lang: Lang,
) -> anyhow::Result<StatsCard> {
    let frame = Frame::new(app, period);
    let (rows, place) = tokio::try_join!(
        app.db.member_days(guild, user, frame.first),
        app.db.member_place(guild, user, frame.first),
    )?;
    let days = fill(&rows, frame.first, frame.days);
    let messages: i64 = days.iter().map(|d| d.messages).sum();
    let voice: i64 = days.iter().map(|d| d.voice_minutes).sum();
    let active = days.iter().filter(|d| d.messages > 0 || d.voice_minutes > 0).count();
    let mut card = base(lang, &frame, "stats.card.member-kicker", name, period);
    card.tiles = vec![
        tile(lang, "stats.card.messages", grouped(messages), None),
        tile(lang, "stats.card.voice", minutes_text(voice, lang), None),
        rank_tile(lang, place, "stats.card.of-members"),
        tile(
            lang,
            "stats.card.active-days",
            grouped(active as i64),
            Some(t!(lang, "stats.card.of-days", count = frame.days)),
        ),
    ];
    card.days = days.iter().map(|d| d.messages).collect();
    Ok(card)
}

fn rank_tile(lang: Lang, place: Option<(i64, i64)>, of: &str) -> StatTile {
    match place {
        Some((place, out_of)) => tile(
            lang,
            "stats.card.rank",
            format!("#{}", grouped(place)),
            Some(t!(lang, of, count = out_of)),
        ),
        None => tile(
            lang,
            "stats.card.rank",
            "-".to_owned(),
            Some(t!(lang, "stats.card.no-rank")),
        ),
    }
}

/// Channels the viewer may see, and whether this channel's public tally is safe to show.
pub(crate) struct ChannelAudience<'a> {
    pub visible: &'a HashSet<u64>,
    pub safe_public: bool,
}

/// A channel's card: `name` is the channel's name without the `#`.
pub(crate) async fn channel_card(
    app: &App,
    guild: u64,
    channel: u64,
    name: &str,
    period: Period,
    lang: Lang,
    audience: ChannelAudience<'_>,
) -> anyhow::Result<StatsCard> {
    let frame = Frame::new(app, period);
    let rows = if audience.safe_public {
        app.db
            .public_visible_channel_days(guild, frame.first, &[channel as i64])
            .await?
    } else {
        app.db.channel_days(guild, channel, frame.first).await?
    };
    let days = fill(&rows, frame.first, frame.days);
    let messages: i64 = days.iter().map(|d| d.messages).sum();
    let voice: i64 = days.iter().map(|d| d.voice_minutes).sum();
    let ids: Vec<i64> = audience.visible.iter().map(|id| *id as i64).collect();
    let server: i64 = if audience.safe_public {
        app.db
            .public_visible_channel_days(guild, frame.first, &ids)
            .await?
            .iter()
            .map(|d| d.messages)
            .sum()
    } else {
        0
    };
    let share = if server > 0 {
        (messages * 100 + server / 2) / server
    } else {
        0
    };
    let mut card = base(lang, &frame, "stats.card.channel-kicker", format!("#{name}"), period);
    card.tiles = vec![
        tile(lang, "stats.card.messages", grouped(messages), None),
        tile(lang, "stats.card.voice", minutes_text(voice, lang), None),
        rank_tile(lang, None, "stats.card.of-channels"),
        tile(
            lang,
            "stats.card.share",
            format!("{share}%"),
            Some(t!(lang, "stats.card.of-server")),
        ),
    ];
    card.days = days.iter().map(|d| d.messages).collect();
    Ok(card)
}

/// The card as an answer, with the 7 / 30 / 90 day buttons under it (the one showing is greyed out).
pub(crate) fn answer(png: Vec<u8>, scope: Scope, period: Period, lang: Lang) -> InteractionResponseData {
    let buttons = Period::ALL
        .into_iter()
        .map(|p| {
            Component::Button(Button {
                custom_id: Some(format!("sc:{}:{}", scope.id(), p.key())),
                disabled: p == period,
                emoji: None,
                id: None,
                label: Some(t!(lang, "stats.button.days", count = p.days())),
                sku_id: None,
                style: if p == period {
                    ButtonStyle::Primary
                } else {
                    ButtonStyle::Secondary
                },
                url: None,
            })
        })
        .collect();
    InteractionResponseData {
        attachments: Some(vec![Attachment::from_bytes("stats.png".to_owned(), png, 1)]),
        embeds: Some(Vec::new()),
        components: Some(vec![Component::ActionRow(ActionRow {
            id: None,
            components: buttons,
        })]),
        ..Default::default()
    }
}

/// The server card as a PNG, for the dashboard's preview.
pub async fn server_card_png(app: &App, guild: u64, days: i64) -> anyhow::Result<Vec<u8>> {
    let period = Period::of_days(days).unwrap_or_default();
    let lang = app.guild_lang(guild).await;
    let card = server_card(app, guild, period, lang).await?;
    app.render(move |r| r.stats_card(&card)).await
}
