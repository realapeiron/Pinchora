//! The weekly room contest, per server: enter one room, vote on the others, prizes on Monday.
//! A week closes lazily: the first contest call after it ends pays the prizes, once.

use std::str::Split;

use pinchora_db::{ContestEntry, ContestResult, RoomRow};
use pinchora_game::{
    Content,
    contest::{self, PRIZES, WINNER_TITLE},
    house,
};
use twilight_interactions::command::{CommandModel, CreateCommand};
use twilight_model::{
    channel::message::{
        Component, MessageFlags,
        component::{ActionRow, Button, ButtonStyle},
    },
    http::interaction::InteractionResponseData,
};
use twilight_util::builder::embed::{EmbedBuilder, EmbedFieldBuilder, EmbedFooterBuilder};

use super::{
    guestbook::plain,
    house::{ensure_house, render_room, room_index},
};
use crate::{
    Cx,
    game::scene::RoomView,
    util::{BRAND, SUCCESS, WARN, thousands},
};

const MEDALS: [&str; 3] = ["🥇", "🥈", "🥉"];

#[derive(CommandModel, CreateCommand)]
#[command(name = "contest", desc = "This server's weekly room contest")]
pub enum HouseContest {
    #[command(name = "view")]
    View(ContestView),
    #[command(name = "enter")]
    Enter(ContestEnter),
    #[command(name = "vote")]
    Vote(ContestVote),
    #[command(name = "winners")]
    Winners(ContestWinners),
}

#[derive(CommandModel, CreateCommand)]
#[command(
    name = "view",
    desc = "This week's theme, time left, entries and last week's winners"
)]
pub struct ContestView;

#[derive(CommandModel, CreateCommand)]
#[command(name = "enter", desc = "Enter a room as it looks now (entering again updates it)")]
pub struct ContestEnter {
    /// Room number (default: 1)
    #[command(min_value = 1, max_value = 6)]
    room: Option<i64>,
}

#[derive(CommandModel, CreateCommand)]
#[command(name = "vote", desc = "Judge the rooms other members entered")]
pub struct ContestVote;

#[derive(CommandModel, CreateCommand)]
#[command(name = "winners", desc = "Last week's winning room")]
pub struct ContestWinners;

impl HouseContest {
    pub async fn run(self, cx: &Cx) -> anyhow::Result<()> {
        let Some(guild) = cx.guild_id() else {
            return cx.reply_ephemeral("The room contest runs inside servers.").await;
        };
        let week = settle(cx, guild).await?;
        let user = cx.author()?.id.get();
        let data = match self {
            Self::View(_) => overview(cx, guild, week, user).await?,
            Self::Enter(c) => return enter(cx, guild, week, user, room_index(c.room)).await,
            Self::Vote(_) => ballot(cx, guild, week, user, None, "").await?,
            Self::Winners(_) => winners(cx, guild, week, false).await?,
        };
        cx.reply(data).await
    }
}

/// The current week, after closing any week that ended: the first contest call of a new week
/// pays last week's prizes.
async fn settle(cx: &Cx, guild: u64) -> anyhow::Result<i32> {
    let week = contest::week_of(cx.app.now().timestamp()) as i32;
    cx.app
        .db
        .close_contest_weeks(guild, week, &PRIZES, WINNER_TITLE)
        .await?;
    Ok(week)
}

fn room_name(room_type: &str) -> String {
    Content::get()
        .room_type(room_type)
        .map_or_else(|| room_type.to_owned(), |r| r.name.clone())
}

fn plural(n: i64, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

fn button(label: &str, custom_id: String, style: ButtonStyle, disabled: bool) -> Component {
    Component::Button(Button {
        custom_id: Some(custom_id),
        disabled,
        emoji: None,
        id: None,
        label: Some(label.to_owned()),
        sku_id: None,
        style,
        url: None,
    })
}

fn row(components: Vec<Component>) -> Component {
    Component::ActionRow(ActionRow { id: None, components })
}

/// A text-only answer that replaces whatever the message showed.
fn text(text: String, ephemeral: bool) -> InteractionResponseData {
    InteractionResponseData {
        content: Some(text),
        embeds: Some(Vec::new()),
        components: Some(Vec::new()),
        attachments: Some(Vec::new()),
        flags: ephemeral.then_some(MessageFlags::EPHEMERAL),
        ..Default::default()
    }
}

/// An entry drawn from its snapshot.
async fn render_entry(cx: &Cx, guild: u64, week: i32, entry: &ContestEntry) -> anyhow::Result<Vec<u8>> {
    let placements = cx.app.db.contest_pieces(guild, week, entry.user_id as u64).await?;
    let tier = house::tier(entry.tier);
    render_room(
        &cx.app,
        RoomView {
            width: tier.width,
            depth: tier.depth,
            room: RoomRow {
                idx: entry.room_idx,
                room_type: entry.room_type.clone(),
                wallpaper: entry.wallpaper.clone(),
                floor: entry.floor.clone(),
            },
            placements,
            cursor: None,
            selected: None,
            edit: false,
            pets: Vec::new(),
            characters: Vec::new(),
            seed: 0,
        },
    )
    .await
}

/// "🥇 **Sim** · 3 votes · +500 shells", one line per winner.
async fn podium(cx: &Cx, guild: u64, result: &ContestResult) -> anyhow::Result<String> {
    if result.winner_ids.is_empty() {
        return Ok(format!(
            "{} entered but nobody got a vote, so nobody won.",
            plural(result.entries.into(), "room", "rooms")
        ));
    }
    let entries = cx
        .app
        .db
        .contest_entries_of(guild, result.week, &result.winner_ids)
        .await?;
    let lines: Vec<String> = result
        .winner_ids
        .iter()
        .zip(&result.winner_votes)
        .zip(MEDALS.iter().zip(PRIZES))
        .map(|((id, votes), (medal, prize))| {
            let name = entries
                .iter()
                .find(|e| e.user_id == *id)
                .map_or_else(|| "Someone".to_owned(), |e| plain(&e.owner_name));
            format!(
                "{medal} **{name}** · {} · +{} shells",
                plural((*votes).into(), "vote", "votes"),
                thousands(prize)
            )
        })
        .collect();
    Ok(lines.join("\n"))
}

async fn overview(cx: &Cx, guild: u64, week: i32, user: u64) -> anyhow::Result<InteractionResponseData> {
    let db = &cx.app.db;
    let theme = contest::theme(week.into());
    let ends = contest::week_start(i64::from(week) + 1);
    let (entries, mine, last) = tokio::try_join!(
        db.contest_entry_count(guild, week),
        db.contest_entry(guild, week, user),
        db.last_contest_result(guild, week),
    )?;
    let mine = match mine {
        Some(e) => format!(
            "Room {} · {} · 💖 {}",
            e.room_idx + 1,
            room_name(&e.room_type),
            plural(e.votes, "vote", "votes")
        ),
        None => "Not entered yet: `/house contest enter [room]`".to_owned(),
    };
    let (last_title, last_text) = match &last {
        Some(r) if r.week + 1 == week => ("Last week".to_owned(), podium(cx, guild, r).await?),
        Some(r) => (format!("Week of {}", short_date(r.week)), podium(cx, guild, r).await?),
        None => ("Last week".to_owned(), "No contest has finished here yet.".to_owned()),
    };
    let embed = EmbedBuilder::new()
        .color(BRAND)
        .title(format!("🏆 Room contest: {}", theme.name))
        .description(format!(
            "{}\n\nEnter one room with `/house contest enter`, judge the others with `/house contest vote`. \
             Prizes: 🥇 {} · 🥈 {} · 🥉 {} shells, and the winner becomes **House of the Week**.",
            theme.description, PRIZES[0], PRIZES[1], PRIZES[2]
        ))
        .field(EmbedFieldBuilder::new("Ends", format!("<t:{ends}:R>")).inline())
        .field(EmbedFieldBuilder::new("Entries", entries.to_string()).inline())
        .field(EmbedFieldBuilder::new("Your entry", mine))
        .field(EmbedFieldBuilder::new(last_title, last_text))
        .build();
    let components = vec![row(vec![
        button("Vote", "rc:o".to_owned(), ButtonStyle::Success, false),
        button(
            "Last winners",
            "rc:w".to_owned(),
            ButtonStyle::Secondary,
            last.is_none(),
        ),
    ])];
    Ok(InteractionResponseData {
        embeds: Some(vec![embed]),
        components: Some(components),
        ..Default::default()
    })
}

/// "Nov 2", the Monday a week starts.
fn short_date(week: i32) -> String {
    chrono::DateTime::from_timestamp(contest::week_start(week.into()), 0)
        .map_or_else(String::new, |d| d.format("%b %-d").to_string())
}

async fn enter(cx: &Cx, guild: u64, week: i32, user: u64, room: i16) -> anyhow::Result<()> {
    let (_, rooms) = ensure_house(&cx.app, user).await?;
    let room = room.clamp(0, rooms.len().saturating_sub(1) as i16);
    let db = &cx.app.db;
    let again = db.contest_entry(guild, week, user).await?.is_some();
    let name = cx.author_display_name();
    let pieces = db.enter_contest(guild, week, user, &name, room).await?;
    if pieces == 0 {
        return cx
            .reply_ephemeral(format!(
                "Room {} is empty. Decorate it with `/house edit` first.",
                room + 1
            ))
            .await;
    }
    let entry = db
        .contest_entry(guild, week, user)
        .await?
        .ok_or_else(|| anyhow::anyhow!("contest entry missing right after entering"))?;
    let png = render_entry(cx, guild, week, &entry).await?;
    let theme = contest::theme(week.into());
    let ends = contest::week_start(i64::from(week) + 1);
    let embed = EmbedBuilder::new()
        .color(SUCCESS)
        .title(if again {
            format!("{} updated their contest entry", plain(&name))
        } else {
            format!("{} entered the room contest!", plain(&name))
        })
        .description(format!(
            "**{}** for the theme **{}** · {}\nVoting is open until <t:{ends}:f>. Changed the room? Enter again to update it.",
            room_name(&entry.room_type),
            theme.name,
            plural(pieces as i64, "piece", "pieces")
        ));
    let components = vec![row(vec![button(
        "Vote on entries",
        "rc:o".to_owned(),
        ButtonStyle::Success,
        false,
    )])];
    cx.reply(Cx::card(embed, "contest.png", png, components, false)).await
}

/// A random entry the voter has not judged and that is not theirs, with Vote and Skip.
async fn ballot(
    cx: &Cx,
    guild: u64,
    week: i32,
    voter: u64,
    skip: Option<u64>,
    note: &str,
) -> anyhow::Result<InteractionResponseData> {
    let db = &cx.app.db;
    let (entry, left) = db.next_ballot(guild, week, voter, skip).await?;
    let Some(entry) = entry else {
        let entries = db.contest_entry_count(guild, week).await?;
        let own = db.contest_entry(guild, week, voter).await?.is_some();
        let why = if entries - i64::from(own) == 0 {
            "There are no other entries to judge yet. Check back later, or enter your own room with `/house contest enter`."
        } else {
            "You have judged every entry this week. The winners are announced on Monday!"
        };
        let note = if note.is_empty() {
            String::new()
        } else {
            format!("{note}\n")
        };
        return Ok(text(format!("{note}{why}"), true));
    };
    let png = render_entry(cx, guild, week, &entry).await?;
    let theme = contest::theme(week.into());
    let mut lines = Vec::new();
    if !note.is_empty() {
        lines.push(note.to_owned());
    }
    lines.push(format!(
        "Theme: **{}** · {}",
        theme.name,
        plural(left, "entry left to judge", "entries left to judge")
    ));
    let embed = EmbedBuilder::new()
        .color(WARN)
        .title(format!(
            "{}'s {}",
            plain(&entry.owner_name),
            room_name(&entry.room_type)
        ))
        .description(lines.join("\n"))
        .footer(EmbedFooterBuilder::new("One vote per room. Skip to see another."));
    let owner = entry.user_id;
    let components = vec![row(vec![
        button("💖 Vote", format!("rc:v:{week}:{owner}"), ButtonStyle::Success, false),
        button("Skip", format!("rc:k:{week}:{owner}"), ButtonStyle::Secondary, left < 2),
    ])];
    Ok(Cx::card(embed, "contest.png", png, components, true))
}

/// The newest finished week's winning room, drawn from its snapshot, with the podium.
async fn winners(cx: &Cx, guild: u64, week: i32, ephemeral: bool) -> anyhow::Result<InteractionResponseData> {
    let db = &cx.app.db;
    let Some(result) = db.last_contest_result(guild, week).await? else {
        return Ok(text(
            format!(
                "No contest week has finished in this server yet. This week's theme is **{}**.",
                contest::theme(week.into()).name
            ),
            ephemeral,
        ));
    };
    let podium_text = podium(cx, guild, &result).await?;
    let first = result.winner_ids.first().and_then(|id| u64::try_from(*id).ok());
    let winner = match first {
        Some(id) => db.contest_entry(guild, result.week, id).await?,
        None => None,
    };
    let theme = contest::theme(result.week.into());
    let Some(winner) = winner else {
        return Ok(text(
            format!(
                "**{}** (week of {}): {podium_text}",
                theme.name,
                short_date(result.week)
            ),
            ephemeral,
        ));
    };
    let png = render_entry(cx, guild, result.week, &winner).await?;
    let embed = EmbedBuilder::new()
        .color(SUCCESS)
        .title(format!("🏆 House of the Week: {}", plain(&winner.owner_name)))
        .description(format!(
            "{} · theme **{}** · week of {}\n\n{podium_text}",
            room_name(&winner.room_type),
            theme.name,
            short_date(result.week)
        ))
        .footer(EmbedFooterBuilder::new(format!(
            "{} entered that week",
            plural(result.entries.into(), "room", "rooms")
        )));
    Ok(Cx::card(embed, "winner.png", png, Vec::new(), ephemeral))
}

/// `rc:o` opens a ballot, `rc:w` shows the winners, `rc:v:<week>:<entry>` votes and
/// `rc:k:<week>:<entry>` skips to another entry.
pub(crate) async fn on_component(cx: &Cx, mut args: Split<'_, char>) -> anyhow::Result<()> {
    let Some(guild) = cx.guild_id() else {
        return cx.reply_ephemeral("The room contest runs inside servers.").await;
    };
    let week = settle(cx, guild).await?;
    let voter = cx.author()?.id.get();
    match args.next() {
        Some("o") => {
            let data = ballot(cx, guild, week, voter, None, "").await?;
            cx.reply(data).await
        }
        Some("w") => {
            let data = winners(cx, guild, week, true).await?;
            cx.reply(data).await
        }
        Some(kind @ ("v" | "k")) => {
            let shown: i32 = args.next().and_then(|v| v.parse().ok()).unwrap_or(-1);
            let entry: u64 = args.next().and_then(|v| v.parse().ok()).unwrap_or(0);
            if shown != week {
                return cx
                    .update(text(
                        "That contest week is over. See who won with `/house contest winners`.".to_owned(),
                        true,
                    ))
                    .await;
            }
            let note = match kind {
                "v" if cx.app.db.contest_vote(guild, week, entry, voter).await? => "💖 Vote counted!",
                "v" => "You already voted for that room.",
                _ => "Skipped.",
            };
            let skip = (kind == "k").then_some(entry);
            let data = ballot(cx, guild, week, voter, skip, note).await?;
            cx.update(data).await
        }
        _ => cx.reply_ephemeral("This button has expired.").await,
    }
}
