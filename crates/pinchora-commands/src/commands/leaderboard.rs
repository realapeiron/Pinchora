use pinchora_core::leveling;
use twilight_interactions::command::{CommandModel, CommandOption, CreateCommand, CreateOption};
use twilight_model::channel::message::{
    Component, Embed,
    component::{ActionRow, Button, ButtonStyle},
};
use twilight_model::http::interaction::InteractionResponseData;
use twilight_util::builder::embed::{EmbedBuilder, EmbedFooterBuilder};

use crate::{
    App, Cx,
    util::{BRAND, mention, thousands},
};

const PAGE_SIZE: i64 = 10;
const MAX_PAGES: u32 = 50;

#[derive(CommandOption, CreateOption, Clone, Copy, Debug, PartialEq, Eq)]
pub enum Board {
    #[option(name = "This server (XP)", value = "server")]
    Server,
    #[option(name = "Global (XP)", value = "xp")]
    GlobalXp,
    #[option(name = "Global (shells)", value = "shells")]
    GlobalShells,
}

impl Board {
    fn parse(raw: &str) -> Option<Self> {
        match raw {
            "server" => Some(Self::Server),
            "xp" => Some(Self::GlobalXp),
            "shells" => Some(Self::GlobalShells),
            _ => None,
        }
    }
}

#[derive(CommandModel, CreateCommand)]
#[command(name = "leaderboard", desc = "Top members by XP or shells")]
pub struct Leaderboard {
    /// Which board (default: this server)
    board: Option<Board>,
}

impl Leaderboard {
    pub async fn run(self, cx: &Cx) -> anyhow::Result<()> {
        let guild = cx.guild_id();
        let board = match (self.board, guild) {
            (Some(Board::Server) | None, None) => Board::GlobalXp,
            (Some(b), _) => b,
            (None, Some(_)) => Board::Server,
        };
        let (embed, components) = page(&cx.app, board, guild, 0).await?;
        cx.reply_embed_components(embed, components).await
    }
}

/// Handles the ◀ ▶ buttons: `lb:<board>:<page>`.
pub(crate) async fn on_button(cx: &Cx, mut args: std::str::Split<'_, char>) -> anyhow::Result<()> {
    let board = args.next().and_then(Board::parse).unwrap_or(Board::GlobalXp);
    let page_no: u32 = args.next().and_then(|p| p.parse().ok()).unwrap_or(0);
    let (embed, components) = page(&cx.app, board, cx.guild_id(), page_no.min(MAX_PAGES)).await?;
    cx.update(InteractionResponseData {
        embeds: Some(vec![embed]),
        components: Some(components),
        ..Default::default()
    })
    .await
}

async fn page(app: &App, board: Board, guild: Option<u64>, page: u32) -> anyhow::Result<(Embed, Vec<Component>)> {
    let offset = i64::from(page) * PAGE_SIZE;
    // Fetch one extra row to know whether a next page exists.
    let (rows, title) = match (board, guild) {
        (Board::Server, Some(g)) => (
            app.db.guild_leaderboard(g, PAGE_SIZE + 1, offset).await?,
            "Server leaderboard",
        ),
        (Board::GlobalShells, _) => (
            app.db.global_credits_leaderboard(PAGE_SIZE + 1, offset).await?,
            "Richest shell collectors",
        ),
        _ => (
            app.db.global_xp_leaderboard(PAGE_SIZE + 1, offset).await?,
            "Global leaderboard",
        ),
    };
    let has_next = rows.len() as i64 > PAGE_SIZE && page < MAX_PAGES;

    let mut lines = Vec::new();
    for (i, row) in rows.iter().take(PAGE_SIZE as usize).enumerate() {
        let place = offset + i as i64 + 1;
        let badge = match place {
            1 => "🥇".to_owned(),
            2 => "🥈".to_owned(),
            3 => "🥉".to_owned(),
            n => format!("`#{n}`"),
        };
        let stat = match board {
            Board::GlobalShells => format!("**{}** shells", thousands(row.value)),
            _ => format!(
                "**Lv. {}** · {} XP",
                leveling::level_of(row.value),
                thousands(row.value)
            ),
        };
        lines.push(format!("{badge} {} — {stat}", mention(row.user_id)));
    }
    let description = if lines.is_empty() {
        "Nobody here yet. Chat a bit and check again!".to_owned()
    } else {
        lines.join("\n")
    };
    let embed = EmbedBuilder::new()
        .color(BRAND)
        .title(title)
        .description(description)
        .footer(EmbedFooterBuilder::new(format!("Page {}", page + 1)))
        .build();

    let key = match board {
        Board::Server => "server",
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
    Ok((embed, vec![row]))
}
