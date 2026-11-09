//! `/starboard`: boards, top posts and vote channel setup.

use pinchora_core::admin::server_stats::grouped_in;
use pinchora_core::admin::settings::parse_hex_color;
use pinchora_core::community::starboard::MAX_BOARDS;
use pinchora_core::community::starboard::MAX_ROLE_WEIGHT;
use pinchora_core::t;
use pinchora_db::{Starboard, VoteChannel};
use twilight_interactions::command::{CommandModel, CreateCommand};
use twilight_model::{
    application::interaction::InteractionChannel,
    channel::message::{
        Component, MessageFlags,
        component::{ActionRow, Button, ButtonStyle},
    },
    guild::Permissions,
    http::interaction::InteractionResponseData,
};
use twilight_util::builder::embed::EmbedBuilder;

use crate::Cx;
use crate::infra::util::BRAND;

fn manage_guild() -> Permissions {
    Permissions::MANAGE_GUILD
}

#[derive(CommandModel, CreateCommand)]
#[command(
    name = "starboard",
    desc = "Highlight popular posts and set up vote channels",
    default_permissions = "manage_guild",
    contexts = "guild"
)]
#[allow(clippy::large_enum_variant)]
pub enum StarboardCommand {
    #[command(name = "setup")]
    Setup(BoardSetup),
    #[command(name = "edit")]
    Edit(BoardEdit),
    #[command(name = "remove")]
    Remove(BoardRemove),
    #[command(name = "list")]
    List(BoardList),
    #[command(name = "top")]
    Top(BoardTop),
    #[command(name = "votes")]
    Votes(VotesCommand),
}

#[derive(CommandModel, CreateCommand)]
#[command(name = "setup", desc = "Create or replace a starboard (up to three)")]
pub struct BoardSetup {
    /// Name of this board
    #[command(min_length = 1, max_length = 40)]
    name: String,
    /// The board channel
    #[command(channel_types = "guild_text guild_announcement")]
    channel: InteractionChannel,
    /// Star emoji, including custom emoji
    #[command(max_length = 64)]
    emoji: Option<String>,
    /// Stars needed before a message appears (1 to 50)
    #[command(min_value = 1, max_value = 50)]
    threshold: Option<i64>,
    /// Allow members to star their own posts
    self_star: Option<bool>,
    /// Count stars on bot messages
    allow_bots: Option<bool>,
    /// Require an image
    require_image: Option<bool>,
    /// Delete the board post when its stars fall below the threshold
    remove_below: Option<bool>,
    /// Limit sources to channel IDs or mentions, separated by commas
    #[command(max_length = 500)]
    only_channels: Option<String>,
    /// Exclude channel IDs or mentions, separated by commas
    #[command(max_length = 500)]
    except_channels: Option<String>,
    /// Role weights as role ID=weight pairs, separated by commas
    #[command(max_length = 500)]
    role_weights: Option<String>,
    /// Board colour, for example #ffc857
    #[command(max_length = 9)]
    color: Option<String>,
}

#[derive(CommandModel, CreateCommand)]
#[command(name = "edit", desc = "Change a board's threshold, emoji or rules")]
pub struct BoardEdit {
    /// Board ID from /starboard list
    id: i64,
    /// New name
    #[command(max_length = 40)]
    name: Option<String>,
    /// New star emoji
    #[command(max_length = 64)]
    emoji: Option<String>,
    /// New threshold
    #[command(min_value = 1, max_value = 50)]
    threshold: Option<i64>,
    /// Allow self stars
    self_star: Option<bool>,
    /// Count bot messages
    allow_bots: Option<bool>,
    /// Require an image
    require_image: Option<bool>,
    /// Remove below threshold
    remove_below: Option<bool>,
    /// Source channels, comma separated; use `all` to clear
    #[command(max_length = 500)]
    only_channels: Option<String>,
    /// Excluded channels, comma separated; use `all` to clear
    #[command(max_length = 500)]
    except_channels: Option<String>,
    /// Role ID=weight pairs; use `all` to clear
    #[command(max_length = 500)]
    role_weights: Option<String>,
    /// Board colour
    #[command(max_length = 9)]
    color: Option<String>,
    /// Enable or disable this board
    enabled: Option<bool>,
}

#[derive(CommandModel, CreateCommand)]
#[command(name = "remove", desc = "Remove a starboard")]
pub struct BoardRemove {
    /// Board ID from /starboard list
    id: i64,
}
#[derive(CommandModel, CreateCommand)]
#[command(name = "list", desc = "List this server's starboards")]
pub struct BoardList;
#[derive(CommandModel, CreateCommand)]
#[command(name = "top", desc = "See the most starred posts and top stargazers")]
pub struct BoardTop {
    /// Board ID, or the first board if omitted
    board: Option<i64>,
}

#[derive(CommandModel, CreateCommand)]
#[command(name = "votes", desc = "Set up channels with automatic up and down reactions")]
pub enum VotesCommand {
    #[command(name = "add")]
    Add(VoteAdd),
    #[command(name = "remove")]
    Remove(VoteRemove),
    #[command(name = "list")]
    List(VoteList),
}
#[derive(CommandModel, CreateCommand)]
#[command(name = "add", desc = "Add or change a vote channel")]
pub struct VoteAdd {
    /// Channel to react in
    #[command(channel_types = "guild_text guild_announcement")]
    channel: InteractionChannel,
    /// Upvote emoji
    #[command(max_length = 64)]
    up: Option<String>,
    /// Downvote emoji
    #[command(max_length = 64)]
    down: Option<String>,
    /// React only to posts with media
    only_media: Option<bool>,
    /// Start a thread under each post
    thread: Option<bool>,
}
#[derive(CommandModel, CreateCommand)]
#[command(name = "remove", desc = "Stop automatic reactions in a channel")]
pub struct VoteRemove {
    /// Channel to stop reacting in
    #[command(channel_types = "guild_text guild_announcement")]
    channel: InteractionChannel,
}
#[derive(CommandModel, CreateCommand)]
#[command(name = "list", desc = "List vote channels")]
pub struct VoteList;

fn ids(value: &str) -> Option<Vec<i64>> {
    if value.eq_ignore_ascii_case("all") {
        return Some(Vec::new());
    }
    value
        .split(',')
        .map(|part| {
            part.trim()
                .trim_start_matches("<#")
                .trim_end_matches('>')
                .parse::<i64>()
                .ok()
                .filter(|v| *v > 0)
        })
        .collect()
}

fn weights(value: &str) -> Option<serde_json::Value> {
    if value.eq_ignore_ascii_case("all") {
        return Some(serde_json::json!({}));
    }
    let mut out = serde_json::Map::new();
    for part in value.split(',') {
        let (role, weight) = part.trim().split_once('=')?;
        let role = role
            .trim()
            .trim_start_matches("<@&")
            .trim_end_matches('>')
            .parse::<u64>()
            .ok()?;
        let weight = weight.trim().parse::<i64>().ok()?;
        if role == 0 || !(1..=MAX_ROLE_WEIGHT).contains(&weight) {
            return None;
        }
        out.insert(role.to_string(), serde_json::json!(weight));
    }
    Some(serde_json::Value::Object(out))
}

fn valid_emoji(value: &str) -> bool {
    !value.trim().is_empty() && value.chars().count() <= 64 && !value.contains(['\n', '\r'])
}

fn missing_permission(channel: &InteractionChannel, permissions: &[(&str, Permissions)]) -> Option<&'static str> {
    let mine = channel.app_permissions?;
    permissions
        .iter()
        .find(|(_, needed)| !mine.contains(*needed))
        .map(|(name, _)| match *name {
            "view" => "starboard.permission.view",
            "send" => "starboard.permission.send",
            "embed" => "starboard.permission.embed",
            "history" => "starboard.permission.history",
            "react" => "starboard.permission.react",
            _ => "starboard.permission.thread",
        })
}

impl StarboardCommand {
    pub async fn run(self, cx: &Cx) -> anyhow::Result<()> {
        let Some(guild) = cx.guild_id() else {
            return cx.reply_ephemeral(t!(cx.lang(), "starboard.guild-only")).await;
        };
        if !cx.can_manage_guild() {
            return cx.reply_ephemeral(t!(cx.lang(), "starboard.manage-needed")).await;
        }
        match self {
            Self::Setup(c) => setup(cx, guild, c).await,
            Self::Edit(c) => edit(cx, guild, c).await,
            Self::Remove(c) => {
                let removed = cx.app.db.delete_starboard(guild, c.id).await?;
                cx.reply_ephemeral(t!(
                    cx.lang(),
                    if removed {
                        "starboard.removed"
                    } else {
                        "starboard.missing"
                    }
                ))
                .await
            }
            Self::List(_) => list(cx, guild).await,
            Self::Top(c) => top(cx, guild, c.board, 0, false).await,
            Self::Votes(c) => match c {
                VotesCommand::Add(c) => vote_add(cx, guild, c).await,
                VotesCommand::Remove(c) => {
                    let removed = cx.app.db.delete_vote(guild, c.channel.id.get()).await?;
                    cx.reply_ephemeral(t!(
                        cx.lang(),
                        if removed {
                            "starboard.vote.removed"
                        } else {
                            "starboard.missing"
                        }
                    ))
                    .await
                }
                VotesCommand::List(_) => vote_list(cx, guild).await,
            },
        }
    }
}

async fn setup(cx: &Cx, guild: u64, c: BoardSetup) -> anyhow::Result<()> {
    if let Some(key) = missing_permission(
        &c.channel,
        &[
            ("view", Permissions::VIEW_CHANNEL),
            ("send", Permissions::SEND_MESSAGES),
            ("embed", Permissions::EMBED_LINKS),
            ("history", Permissions::READ_MESSAGE_HISTORY),
        ],
    ) {
        return cx.reply_ephemeral(t!(cx.lang(), key)).await;
    }
    let current = cx.app.db.starboards(guild).await?;
    let exists = current.iter().any(|b| b.channel_id == c.channel.id.get() as i64);
    if current.len() >= MAX_BOARDS && !exists {
        return cx.reply_ephemeral(t!(cx.lang(), "starboard.full")).await;
    }
    let mut board = Starboard {
        id: 0,
        guild_id: guild as i64,
        name: c.name,
        channel_id: c.channel.id.get() as i64,
        emoji: c.emoji.unwrap_or_else(|| "⭐".to_owned()),
        threshold: c.threshold.unwrap_or(3) as i32,
        self_star: c.self_star.unwrap_or(false),
        allow_bots: c.allow_bots.unwrap_or(false),
        only_channels: Vec::new(),
        except_channels: Vec::new(),
        require_image: c.require_image.unwrap_or(false),
        color: 0xffc857,
        role_weights: serde_json::json!({}),
        remove_below: c.remove_below.unwrap_or(true),
        enabled: true,
    };
    if let Some(value) = c.only_channels {
        let Some(v) = ids(&value) else { return invalid(cx).await };
        board.only_channels = v;
    }
    if let Some(value) = c.except_channels {
        let Some(v) = ids(&value) else { return invalid(cx).await };
        board.except_channels = v;
    }
    if let Some(value) = c.role_weights {
        let Some(v) = weights(&value) else {
            return invalid(cx).await;
        };
        board.role_weights = v;
    }
    if let Some(value) = c.color {
        let Some(v) = parse_hex_color(&value) else {
            return invalid(cx).await;
        };
        board.color = v as i32;
    }
    if !valid_emoji(&board.emoji) {
        return invalid(cx).await;
    }
    let saved = cx.app.db.save_starboard(&board).await?;
    cx.reply_ephemeral(t!(
        cx.lang(),
        "starboard.saved",
        id = saved.id,
        channel = format!("<#{}>", saved.channel_id)
    ))
    .await
}

async fn edit(cx: &Cx, guild: u64, c: BoardEdit) -> anyhow::Result<()> {
    let Some(mut board) = cx.app.db.starboard(c.id).await?.filter(|b| b.guild_id == guild as i64) else {
        return cx.reply_ephemeral(t!(cx.lang(), "starboard.missing")).await;
    };
    if let Some(v) = c.name {
        if v.trim().is_empty() {
            return invalid(cx).await;
        }
        board.name = v;
    }
    if let Some(v) = c.emoji {
        board.emoji = v;
    }
    if let Some(v) = c.threshold {
        board.threshold = v as i32;
    }
    if let Some(v) = c.self_star {
        board.self_star = v;
    }
    if let Some(v) = c.allow_bots {
        board.allow_bots = v;
    }
    if let Some(v) = c.require_image {
        board.require_image = v;
    }
    if let Some(v) = c.remove_below {
        board.remove_below = v;
    }
    if let Some(v) = c.only_channels {
        let Some(parsed) = ids(&v) else {
            return invalid(cx).await;
        };
        board.only_channels = parsed;
    }
    if let Some(v) = c.except_channels {
        let Some(parsed) = ids(&v) else {
            return invalid(cx).await;
        };
        board.except_channels = parsed;
    }
    if let Some(v) = c.role_weights {
        let Some(parsed) = weights(&v) else {
            return invalid(cx).await;
        };
        board.role_weights = parsed;
    }
    if let Some(v) = c.color {
        let Some(parsed) = parse_hex_color(&v) else {
            return invalid(cx).await;
        };
        board.color = parsed as i32;
    }
    if let Some(v) = c.enabled {
        board.enabled = v;
    }
    if !valid_emoji(&board.emoji) {
        return invalid(cx).await;
    }
    cx.app.db.save_starboard(&board).await?;
    cx.reply_ephemeral(t!(cx.lang(), "starboard.changed")).await
}

async fn invalid(cx: &Cx) -> anyhow::Result<()> {
    cx.reply_ephemeral(t!(cx.lang(), "starboard.invalid")).await
}

async fn list(cx: &Cx, guild: u64) -> anyhow::Result<()> {
    let boards = cx.app.db.starboards(guild).await?;
    let lines: Vec<_> = boards
        .iter()
        .map(|b| {
            t!(
                cx.lang(),
                "starboard.list-line",
                id = b.id,
                emoji = &b.emoji,
                name = crate::admin::custom::scrub_value(&b.name),
                channel = format!("<#{}>", b.channel_id),
                threshold = grouped_in(cx.lang(), i64::from(b.threshold))
            )
        })
        .collect();
    cx.reply_ephemeral(if lines.is_empty() {
        t!(cx.lang(), "starboard.empty")
    } else {
        lines.join("\n")
    })
    .await
}

async fn vote_add(cx: &Cx, guild: u64, c: VoteAdd) -> anyhow::Result<()> {
    if c.only_media.unwrap_or(false) && !cx.app.message_content {
        return cx.reply_ephemeral(t!(cx.lang(), "starboard.vote.content-needed")).await;
    }
    let mut needed = vec![
        ("view", Permissions::VIEW_CHANNEL),
        ("history", Permissions::READ_MESSAGE_HISTORY),
        ("react", Permissions::ADD_REACTIONS),
    ];
    if c.thread.unwrap_or(false) {
        needed.push(("thread", Permissions::CREATE_PUBLIC_THREADS));
    }
    if let Some(key) = missing_permission(&c.channel, &needed) {
        return cx.reply_ephemeral(t!(cx.lang(), key)).await;
    }
    let vote = VoteChannel {
        channel_id: c.channel.id.get() as i64,
        guild_id: guild as i64,
        up_emoji: c.up.unwrap_or_else(|| "⬆️".to_owned()),
        down_emoji: c.down.unwrap_or_else(|| "⬇️".to_owned()),
        only_media: c.only_media.unwrap_or(false),
        create_thread: c.thread.unwrap_or(false),
        enabled: true,
    };
    if !valid_emoji(&vote.up_emoji) || !valid_emoji(&vote.down_emoji) || vote.up_emoji == vote.down_emoji {
        return invalid(cx).await;
    }
    cx.app.db.save_vote(&vote).await?;
    cx.reply_ephemeral(t!(
        cx.lang(),
        "starboard.vote.saved",
        channel = format!("<#{}>", vote.channel_id)
    ))
    .await
}

async fn vote_list(cx: &Cx, guild: u64) -> anyhow::Result<()> {
    let votes = cx.app.db.votes(guild).await?;
    let lines: Vec<_> = votes
        .iter()
        .map(|v| {
            t!(
                cx.lang(),
                "starboard.vote.line",
                channel = format!("<#{}>", v.channel_id),
                up = &v.up_emoji,
                down = &v.down_emoji
            )
        })
        .collect();
    cx.reply_ephemeral(if lines.is_empty() {
        t!(cx.lang(), "starboard.vote.empty")
    } else {
        lines.join("\n")
    })
    .await
}

async fn top(cx: &Cx, guild: u64, id: Option<i64>, page: i64, update: bool) -> anyhow::Result<()> {
    let boards = cx.app.db.starboards(guild).await?;
    let board = match id
        .and_then(|id| boards.iter().find(|b| b.id == id))
        .or_else(|| boards.first())
    {
        Some(v) => v,
        None => return cx.reply_ephemeral(t!(cx.lang(), "starboard.empty")).await,
    };
    let posts = cx.app.db.top_star_posts(board.id, page * 10).await?;
    let mut lines: Vec<String> = posts
        .iter()
        .enumerate()
        .map(|(i, p)| {
            t!(
                cx.lang(),
                "starboard.top.post-line",
                rank = grouped_in(cx.lang(), page * 10 + i as i64 + 1),
                count = grouped_in(cx.lang(), i64::from(p.star_count)),
                url = format!(
                    "https://discord.com/channels/{}/{}/{}",
                    guild, p.channel_id, p.message_id
                )
            )
        })
        .collect();
    if lines.is_empty() {
        lines.push(t!(cx.lang(), "starboard.top.empty"));
    }
    let gazers = cx.app.db.top_stargazers(board.id).await?;
    if !gazers.is_empty() {
        lines.push(t!(cx.lang(), "starboard.top.stargazers"));
        lines.extend(gazers.iter().map(|(user, count)| {
            t!(
                cx.lang(),
                "starboard.top.stargazer-line",
                user = format!("<@{user}>"),
                count = grouped_in(cx.lang(), *count)
            )
        }));
    }
    let embed = EmbedBuilder::new()
        .title(t!(
            cx.lang(),
            "starboard.top.title",
            board = crate::admin::custom::scrub_value(&board.name)
        ))
        .description(lines.join("\n"))
        .color(BRAND)
        .build();
    let buttons = vec![Component::ActionRow(ActionRow {
        id: None,
        components: vec![
            Component::Button(Button {
                id: None,
                custom_id: Some(format!("sr:{}:{}", board.id, page.saturating_sub(1))),
                disabled: page == 0,
                emoji: None,
                label: Some(t!(cx.lang(), "starboard.top.previous")),
                style: ButtonStyle::Secondary,
                url: None,
                sku_id: None,
            }),
            Component::Button(Button {
                id: None,
                custom_id: Some(format!("sr:{}:{}", board.id, page + 1)),
                disabled: posts.len() < 10,
                emoji: None,
                label: Some(t!(cx.lang(), "starboard.top.next")),
                style: ButtonStyle::Secondary,
                url: None,
                sku_id: None,
            }),
        ],
    })];
    let response = InteractionResponseData {
        embeds: Some(vec![embed]),
        components: Some(buttons),
        flags: Some(MessageFlags::EPHEMERAL),
        ..Default::default()
    };
    if update {
        cx.update(response).await
    } else {
        cx.reply(response).await
    }
}

pub(crate) async fn on_component<'a>(cx: &Cx, mut parts: impl Iterator<Item = &'a str>) -> anyhow::Result<()> {
    let (Some(id), Some(page)) = (
        parts.next().and_then(|v| v.parse::<i64>().ok()),
        parts.next().and_then(|v| v.parse::<i64>().ok()),
    ) else {
        return cx.reply_ephemeral(t!(cx.lang(), "starboard.missing")).await;
    };
    let Some(guild) = cx.guild_id() else {
        return cx.reply_ephemeral(t!(cx.lang(), "starboard.guild-only")).await;
    };
    if !cx.can_manage_guild() {
        return cx.reply_ephemeral(t!(cx.lang(), "starboard.manage-needed")).await;
    }
    top(cx, guild, Some(id), page.clamp(0, 100), true).await
}
