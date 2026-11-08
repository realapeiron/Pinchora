use std::sync::Arc;

use pinchora_core::{
    i18n::Lang,
    leveling::{self, week_start},
    t,
};
use pinchora_db::{CatchBoardRow, LeaderboardRow};
use pinchora_game::Content;
use pinchora_render::{LastWeekEntry, LeaderboardCard, LeaderboardEntry, Theme, placeholder_avatar};
use twilight_interactions::command::{CommandModel, CommandOption, CreateCommand, CreateOption};
use twilight_model::channel::message::{
    Component,
    component::{ActionRow, Button, ButtonStyle},
};
use twilight_model::http::{attachment::Attachment, interaction::InteractionResponseData};

use crate::{
    App, Cx,
    util::{display_name, thousands_in},
};

const PAGE_SIZE: i64 = LeaderboardCard::ROWS as i64;
const MAX_PAGES: u32 = 50;
/// The most characters a place of last week's catch podium has room for (the longest English one fits).
const PODIUM_CHARS: usize = 29;

#[derive(CommandOption, CreateOption, Clone, Copy, Debug, PartialEq, Eq)]
pub enum Board {
    #[option(name = "This server (XP)", value = "server")]
    Server,
    #[option(name = "This server (points)", value = "points")]
    ServerPoints,
    #[option(name = "Weekly XP", value = "weekly")]
    Weekly,
    #[option(name = "Voice XP", value = "voice")]
    Voice,
    #[option(name = "Weekly catch", value = "catch")]
    WeeklyCatch,
    #[option(name = "Global (XP)", value = "xp")]
    GlobalXp,
    #[option(name = "Global (shells)", value = "shells")]
    GlobalShells,
}

impl Board {
    fn parse(raw: &str) -> Option<Self> {
        match raw {
            "server" => Some(Self::Server),
            "points" => Some(Self::ServerPoints),
            "weekly" => Some(Self::Weekly),
            "voice" => Some(Self::Voice),
            "catch" => Some(Self::WeeklyCatch),
            "xp" => Some(Self::GlobalXp),
            "shells" => Some(Self::GlobalShells),
            _ => None,
        }
    }
}

#[derive(CommandModel, CreateCommand)]
#[command(
    name = "leaderboard",
    desc = "Top members by XP, shells, server points or biggest catch"
)]
pub struct Leaderboard {
    /// Which board (default: this server)
    board: Option<Board>,
}

impl Leaderboard {
    pub async fn run(self, cx: &Cx) -> anyhow::Result<()> {
        let guild = cx.guild_id();
        let board = match (self.board, guild) {
            (
                Some(Board::Server | Board::ServerPoints | Board::Weekly | Board::WeeklyCatch | Board::Voice) | None,
                None,
            ) => Board::GlobalXp,
            (Some(b), _) => b,
            (None, Some(_)) => Board::Server,
        };
        // Up to ten names and avatars are fetched, which can outlast Discord's first-answer window.
        cx.responder.defer(false).await?;
        let data = page(&cx.app, board, guild, 0, cx.lang()).await?;
        cx.reply(data).await
    }
}

/// Handles the ◀ ▶ buttons: `lb:<board>:<page>`.
pub(crate) async fn on_button(cx: &Cx, mut args: std::str::Split<'_, char>) -> anyhow::Result<()> {
    let board = args.next().and_then(Board::parse).unwrap_or(Board::GlobalXp);
    let page_no: u32 = args.next().and_then(|p| p.parse().ok()).unwrap_or(0);
    cx.responder.defer(false).await?;
    let data = page(&cx.app, board, cx.guild_id(), page_no.min(MAX_PAGES), cx.lang()).await?;
    cx.update(data).await
}

/// The top three of the week starting `week`, for the recap under the weekly board.
async fn last_week_podium(
    app: &Arc<App>,
    guild: u64,
    week: chrono::NaiveDate,
    lang: Lang,
) -> anyhow::Result<Vec<LastWeekEntry>> {
    let rows = app.db.weekly_leaderboard(guild, week, 3, 0).await?;
    let mut podium = Vec::with_capacity(rows.len());
    for (i, row) in rows.iter().enumerate() {
        let name = app
            .user(row.user_id as u64)
            .await
            .map_or_else(|| t!(lang, "leaderboard.unknown-user"), |u| display_name(&u, None));
        podium.push(LastWeekEntry {
            rank: i as i64 + 1,
            name,
            value: t!(lang, "leaderboard.xp", xp = thousands_in(lang, row.value)),
        });
    }
    Ok(podium)
}

/// "Pearlpatch Koi 61.4 cm": what a weekly catch was. Sizes of 100 cm and up drop the tenth, so the
/// longest names still fit their place on the podium.
fn catch_text(row: &CatchBoardRow, lang: Lang) -> String {
    let name = fish_name(&row.fish_id, lang);
    let size = if row.size_cm >= 100.0 {
        pinchora_core::server_stats::decimal_in(lang, row.size_cm, 0)
    } else {
        pinchora_core::server_stats::decimal_in(lang, row.size_cm, 1)
    };
    let text = t!(lang, "leaderboard.catch-size", name = &name, size = &size);
    // A place of the podium is drawn in a column this wide: a longer fish name gives way, never the size.
    let excess = text.chars().count().saturating_sub(PODIUM_CHARS);
    if excess == 0 {
        return text;
    }
    let keep = name.chars().count().saturating_sub(excess + 1);
    let short = format!("{}…", name.chars().take(keep).collect::<String>().trim_end());
    t!(lang, "leaderboard.catch-size", name = short, size = size)
}

/// A fish's name in `lang`; a fish the content no longer has is "a fish".
fn fish_name(id: &str, lang: Lang) -> String {
    let content = Content::get();
    match content.fish(id) {
        Some(fish) => content.name(&fish.id, lang).into_owned(),
        None => t!(lang, "leaderboard.a-fish"),
    }
}

/// The top three catches of the week starting `week`, for the recap under the weekly catch board.
async fn last_week_catches(
    app: &Arc<App>,
    guild: u64,
    week: chrono::NaiveDate,
    lang: Lang,
) -> anyhow::Result<Vec<LastWeekEntry>> {
    let rows = app.db.weekly_catch_board(guild, week, 3, 0).await?;
    let mut podium = Vec::with_capacity(rows.len());
    for (i, row) in rows.iter().enumerate() {
        let name = app
            .user(row.user_id as u64)
            .await
            .map_or_else(|| t!(lang, "leaderboard.unknown-user"), |u| display_name(&u, None));
        podium.push(LastWeekEntry {
            rank: i as i64 + 1,
            name,
            value: catch_text(row, lang),
        });
    }
    Ok(podium)
}

/// One page as a card image with the ◀ ▶ buttons; the image replaces whatever message it lands on.
async fn page(
    app: &Arc<App>,
    board: Board,
    guild: Option<u64>,
    page: u32,
    lang: Lang,
) -> anyhow::Result<InteractionResponseData> {
    let offset = i64::from(page) * PAGE_SIZE;
    // The server boards need a server; anywhere else a stale button shows global XP.
    let board = match (board, guild) {
        (Board::Server | Board::ServerPoints | Board::Weekly | Board::WeeklyCatch | Board::Voice, None) => {
            Board::GlobalXp
        }
        (b, _) => b,
    };
    // Fetch one extra row to know whether a next page exists.
    let mut points_name = String::new();
    let mut last_week = Vec::new();
    // The weekly catch board's rows, which carry a fish and a size the plain rows have no room for.
    let mut catches: Vec<CatchBoardRow> = Vec::new();
    let (mut rows, title) = match (board, guild) {
        (Board::WeeklyCatch, Some(g)) => {
            let week = week_start(app.now());
            last_week = last_week_catches(app, g, week - chrono::Duration::weeks(1), lang).await?;
            catches = app.db.weekly_catch_board(g, week, PAGE_SIZE + 1, offset).await?;
            let rows = catches
                .iter()
                .map(|c| LeaderboardRow {
                    user_id: c.user_id,
                    value: (c.ratio * 1000.0).round() as i64,
                })
                .collect();
            (rows, t!(lang, "leaderboard.catch"))
        }
        (Board::Weekly, Some(g)) => {
            let week = week_start(app.now());
            last_week = last_week_podium(app, g, week - chrono::Duration::weeks(1), lang).await?;
            (
                app.db.weekly_leaderboard(g, week, PAGE_SIZE + 1, offset).await?,
                t!(lang, "leaderboard.weekly"),
            )
        }
        (Board::Server, Some(g)) => (
            app.db.guild_leaderboard(g, PAGE_SIZE + 1, offset).await?,
            t!(lang, "leaderboard.server"),
        ),
        (Board::Voice, Some(g)) => (
            app.db.voice_level_board(g, PAGE_SIZE + 1, offset).await?,
            t!(lang, "leaderboard.voice"),
        ),
        (Board::ServerPoints, Some(g)) => {
            points_name = app.guild_settings(g).await?.points_name.clone();
            (
                app.db.guild_points_leaderboard(g, PAGE_SIZE + 1, offset).await?,
                t!(lang, "leaderboard.points", name = points_name),
            )
        }
        (Board::GlobalShells, _) => (
            app.db.global_credits_leaderboard(PAGE_SIZE + 1, offset).await?,
            t!(lang, "leaderboard.shells"),
        ),
        _ => (
            app.db.global_xp_leaderboard(PAGE_SIZE + 1, offset).await?,
            t!(lang, "leaderboard.global"),
        ),
    };
    let has_next = rows.len() as i64 > PAGE_SIZE && page < MAX_PAGES;
    rows.truncate(PAGE_SIZE as usize);

    // Names and avatars for the whole page at once; most come from the caches.
    let people: Vec<_> = rows
        .iter()
        .map(|row| {
            let (app, id) = (app.clone(), row.user_id as u64);
            tokio::spawn(async move {
                let user = app.user(id).await;
                let avatar = match &user {
                    Some(user) => app.avatar(user, LeaderboardCard::AVATAR_SIZE, Some(16)).await,
                    None => Arc::new(placeholder_avatar(id, LeaderboardCard::AVATAR_SIZE)),
                };
                (user, avatar)
            })
        })
        .collect();
    let mut entries = Vec::with_capacity(rows.len());
    let curve = if matches!(board, Board::Server | Board::Voice) {
        Some(app.db.leveling_parity(guild.expect("server board")).await?)
    } else {
        None
    };
    for (i, (row, person)) in rows.iter().zip(people).enumerate() {
        let (user, avatar) = person.await?;
        let (value, detail) = match board {
            Board::GlobalShells => (thousands_in(lang, row.value), t!(lang, "leaderboard.shells-unit")),
            Board::ServerPoints => (thousands_in(lang, row.value), lang.upper(&points_name)),
            Board::Weekly => (thousands_in(lang, row.value), t!(lang, "leaderboard.weekly-unit")),
            Board::WeeklyCatch => {
                let catch = &catches[i];
                let fish = lang.upper(&fish_name(&catch.fish_id, lang));
                (
                    t!(
                        lang,
                        "leaderboard.size",
                        size = pinchora_core::server_stats::decimal_in(lang, catch.size_cm, 1)
                    ),
                    t!(
                        lang,
                        "leaderboard.usual",
                        fish = fish,
                        percent = pinchora_core::server_stats::decimal_in(lang, catch.ratio * 100.0, 0)
                    ),
                )
            }
            _ => (
                t!(
                    lang,
                    "leaderboard.level",
                    level = curve
                        .as_ref()
                        .map_or_else(|| leveling::level_of(row.value), |c| c.level_of(row.value))
                ),
                t!(lang, "leaderboard.xp", xp = thousands_in(lang, row.value)),
            ),
        };
        entries.push(LeaderboardEntry {
            rank: offset + i as i64 + 1,
            avatar_png: avatar.to_vec(),
            name: user
                .as_deref()
                .map_or_else(|| t!(lang, "leaderboard.unknown-user"), |u| display_name(u, None)),
            value,
            detail,
        });
    }
    let png = app
        .render(move |r| {
            r.leaderboard_card(&LeaderboardCard {
                lang,
                title: lang.upper(&title),
                entries,
                page,
                last_week,
                empty: if board == Board::WeeklyCatch {
                    t!(lang, "leaderboard.empty-catch")
                } else {
                    t!(lang, "leaderboard.empty")
                },
                theme: Theme::reef(),
            })
        })
        .await?;

    let key = match board {
        Board::Server => "server",
        Board::ServerPoints => "points",
        Board::Weekly => "weekly",
        Board::Voice => "voice",
        Board::WeeklyCatch => "catch",
        Board::GlobalXp => "xp",
        Board::GlobalShells => "shells",
    };
    let button = |label: &str, target: u32, disabled: bool| {
        Component::Button(Button {
            custom_id: Some(format!("lb:{key}:{target}")),
            disabled,
            emoji: None,
            id: None,
            label: Some(label.to_owned()),
            sku_id: None,
            style: ButtonStyle::Secondary,
            url: None,
        })
    };
    let row = Component::ActionRow(ActionRow {
        id: None,
        components: vec![
            button("◀", page.saturating_sub(1), page == 0),
            button("▶", page + 1, !has_next),
        ],
    });
    Ok(InteractionResponseData {
        attachments: Some(vec![Attachment::from_bytes("leaderboard.png".to_owned(), png, 1)]),
        // Boards posted before the card existed were embeds; the card replaces those too.
        embeds: Some(Vec::new()),
        components: Some(vec![row]),
        ..Default::default()
    })
}
