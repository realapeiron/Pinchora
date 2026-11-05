use std::sync::Arc;

use pinchora_core::leveling;
use pinchora_render::{LeaderboardCard, LeaderboardEntry, Theme, placeholder_avatar};
use twilight_interactions::command::{CommandModel, CommandOption, CreateCommand, CreateOption};
use twilight_model::channel::message::{
    Component,
    component::{ActionRow, Button, ButtonStyle},
};
use twilight_model::http::{attachment::Attachment, interaction::InteractionResponseData};

use crate::{
    App, Cx,
    util::{display_name, thousands},
};

const PAGE_SIZE: i64 = LeaderboardCard::ROWS as i64;
const MAX_PAGES: u32 = 50;

#[derive(CommandOption, CreateOption, Clone, Copy, Debug, PartialEq, Eq)]
pub enum Board {
    #[option(name = "This server (XP)", value = "server")]
    Server,
    #[option(name = "This server (points)", value = "points")]
    ServerPoints,
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
            "xp" => Some(Self::GlobalXp),
            "shells" => Some(Self::GlobalShells),
            _ => None,
        }
    }
}

#[derive(CommandModel, CreateCommand)]
#[command(name = "leaderboard", desc = "Top members by XP, shells or server points")]
pub struct Leaderboard {
    /// Which board (default: this server)
    board: Option<Board>,
}

impl Leaderboard {
    pub async fn run(self, cx: &Cx) -> anyhow::Result<()> {
        let guild = cx.guild_id();
        let board = match (self.board, guild) {
            (Some(Board::Server | Board::ServerPoints) | None, None) => Board::GlobalXp,
            (Some(b), _) => b,
            (None, Some(_)) => Board::Server,
        };
        // Up to ten names and avatars are fetched, which can outlast Discord's first-answer window.
        cx.responder.defer(false).await?;
        let data = page(&cx.app, board, guild, 0).await?;
        cx.reply(data).await
    }
}

/// Handles the ◀ ▶ buttons: `lb:<board>:<page>`.
pub(crate) async fn on_button(cx: &Cx, mut args: std::str::Split<'_, char>) -> anyhow::Result<()> {
    let board = args.next().and_then(Board::parse).unwrap_or(Board::GlobalXp);
    let page_no: u32 = args.next().and_then(|p| p.parse().ok()).unwrap_or(0);
    cx.responder.defer(false).await?;
    let data = page(&cx.app, board, cx.guild_id(), page_no.min(MAX_PAGES)).await?;
    cx.update(data).await
}

/// One page as a card image with the ◀ ▶ buttons; the image replaces whatever message it lands on.
async fn page(app: &Arc<App>, board: Board, guild: Option<u64>, page: u32) -> anyhow::Result<InteractionResponseData> {
    let offset = i64::from(page) * PAGE_SIZE;
    // The server boards need a server; anywhere else a stale button shows global XP.
    let board = match (board, guild) {
        (Board::Server | Board::ServerPoints, None) => Board::GlobalXp,
        (b, _) => b,
    };
    // Fetch one extra row to know whether a next page exists.
    let mut points_name = String::new();
    let (mut rows, title) = match (board, guild) {
        (Board::Server, Some(g)) => (
            app.db.guild_leaderboard(g, PAGE_SIZE + 1, offset).await?,
            "Server leaderboard".to_owned(),
        ),
        (Board::ServerPoints, Some(g)) => {
            points_name = app.guild_settings(g).await?.points_name.clone();
            (
                app.db.guild_points_leaderboard(g, PAGE_SIZE + 1, offset).await?,
                format!("Top {points_name}"),
            )
        }
        (Board::GlobalShells, _) => (
            app.db.global_credits_leaderboard(PAGE_SIZE + 1, offset).await?,
            "Richest shell collectors".to_owned(),
        ),
        _ => (
            app.db.global_xp_leaderboard(PAGE_SIZE + 1, offset).await?,
            "Global leaderboard".to_owned(),
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
    for (i, (row, person)) in rows.iter().zip(people).enumerate() {
        let (user, avatar) = person.await?;
        let (value, detail) = match board {
            Board::GlobalShells => (thousands(row.value), "SHELLS".to_owned()),
            Board::ServerPoints => (thousands(row.value), points_name.to_uppercase()),
            _ => (
                format!("LV {}", leveling::level_of(row.value)),
                format!("{} XP", thousands(row.value)),
            ),
        };
        entries.push(LeaderboardEntry {
            rank: offset + i as i64 + 1,
            avatar_png: avatar.to_vec(),
            name: user
                .as_deref()
                .map_or_else(|| "Unknown user".to_owned(), |u| display_name(u, None)),
            value,
            detail,
        });
    }
    let png = app
        .render(move |r| {
            r.leaderboard_card(&LeaderboardCard {
                title,
                entries,
                page,
                theme: Theme::reef(),
            })
        })
        .await?;

    let key = match board {
        Board::Server => "server",
        Board::ServerPoints => "points",
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
