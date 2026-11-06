//! Friends: a request the other player accepts, a list, and the shortlist `/house visit` offers.

use std::str::Split;

use pinchora_core::leveling;
use pinchora_db::{BefriendOutcome, FriendRow, MAX_FRIENDS};
use twilight_interactions::command::{CommandModel, CreateCommand, ResolvedUser};
use twilight_model::{
    channel::message::{
        Component, MessageFlags,
        component::{ButtonStyle, SelectMenu, SelectMenuOption, SelectMenuType},
    },
    http::interaction::InteractionResponseData,
};
use twilight_util::builder::embed::{EmbedBuilder, EmbedFooterBuilder};

use super::{
    house,
    social::{self, Request, button, row},
};
use crate::{
    Cx,
    util::{BRAND, SUCCESS, mention},
};

/// How long a friend request can be answered, in seconds.
const REQUEST_SECS: i64 = 24 * 60 * 60;
/// Friends per page of the list.
const PER_PAGE: i64 = 10;
/// Friends the visit shortlist offers (a select menu's limit).
const SHORTLIST: i64 = 25;

#[derive(CommandModel, CreateCommand)]
#[command(name = "friends", desc = "Add friends, see who they are, and visit them quickly")]
pub enum FriendsCommand {
    #[command(name = "add")]
    Add(FriendsAdd),
    #[command(name = "remove")]
    Remove(FriendsRemove),
    #[command(name = "list")]
    List(FriendsList),
}

#[derive(CommandModel, CreateCommand)]
#[command(name = "add", desc = "Ask someone to be your friend")]
pub struct FriendsAdd {
    /// Who to ask
    user: ResolvedUser,
}

#[derive(CommandModel, CreateCommand)]
#[command(name = "remove", desc = "Stop being friends with someone")]
pub struct FriendsRemove {
    /// Who to remove
    user: ResolvedUser,
}

#[derive(CommandModel, CreateCommand)]
#[command(name = "list", desc = "Your friends, or somebody else's")]
pub struct FriendsList {
    /// Whose friends (default: yours)
    user: Option<ResolvedUser>,
}

impl FriendsCommand {
    pub async fn run(self, cx: &Cx) -> anyhow::Result<()> {
        let me = cx.author()?.id.get();
        match self {
            Self::Add(c) => {
                let other = &c.user.resolved;
                let to = other.id.get();
                if other.bot || to == me {
                    return cx
                        .reply_ephemeral("Friends are other people. Try someone from the server!")
                        .await;
                }
                if cx.app.db.are_friends(me, to).await? {
                    return cx
                        .reply_ephemeral(format!("You and {} are already friends.", mention(to)))
                        .await;
                }
                if cx.app.db.friend_count(me).await? >= MAX_FRIENDS {
                    return cx
                        .reply_ephemeral(format!(
                            "You have {MAX_FRIENDS} friends, the most there can be. Make room with `/friends remove`."
                        ))
                        .await;
                }
                if cx.app.db.friend_count(to).await? >= MAX_FRIENDS {
                    return cx
                        .reply_ephemeral(format!(
                            "{} has {MAX_FRIENDS} friends already, the most there can be.",
                            mention(to)
                        ))
                        .await;
                }
                if social::asked_too_soon(cx, "friend", me, to).await? {
                    return Ok(());
                }
                social::remember(cx, other).await?;
                let request = Request {
                    from: me,
                    to,
                    until: cx.app.now().timestamp() + REQUEST_SECS,
                };
                let embed = EmbedBuilder::new()
                    .color(BRAND)
                    .title("🤝 Friend request")
                    .description(format!(
                        "{} would like to be friends with {}.\nFriends get a quick pick in `/house visit` and a place in `/friends list`.\nThis request expires <t:{}:R>.",
                        mention(me),
                        mention(to),
                        request.until
                    ))
                    .build();
                cx.reply(InteractionResponseData {
                    content: Some(mention(to)),
                    embeds: Some(vec![embed]),
                    components: Some(vec![request.buttons("fd", "Accept")]),
                    allowed_mentions: social::ping_only(to),
                    ..Default::default()
                })
                .await
            }
            Self::Remove(c) => {
                let to = c.user.resolved.id.get();
                let text = if cx.app.db.unfriend(me, to).await? {
                    format!("{} is no longer your friend.", mention(to))
                } else {
                    format!("{} isn't one of your friends.", mention(to))
                };
                cx.reply_ephemeral(text).await
            }
            Self::List(c) => {
                if c.user.as_ref().is_some_and(|u| u.resolved.bot) {
                    return cx.reply_ephemeral("Bots don't have friends.").await;
                }
                let owner = c.user.as_ref().map_or(me, |u| u.resolved.id.get());
                if super::privacy::private_profile(cx, owner).await? {
                    return Ok(());
                }
                match &c.user {
                    Some(other) => social::remember(cx, &other.resolved).await?,
                    None => {
                        let user = cx.author()?;
                        cx.app.db.remember_name(me, &social::plain_name(user)).await?;
                    }
                }
                let data = list_page(cx, owner, 0).await?;
                cx.reply(data).await
            }
        }
    }
}

/// One page of `owner`'s friends, with page buttons when there is more than one.
async fn list_page(cx: &Cx, owner: u64, page: i64) -> anyhow::Result<InteractionResponseData> {
    let me = cx.author()?.id.get();
    let (mut friends, total) = cx.app.db.friends(owner, page.max(0) * PER_PAGE, PER_PAGE).await?;
    let pages = (total + PER_PAGE - 1).div_euclid(PER_PAGE).max(1);
    let page = page.clamp(0, pages - 1);
    if friends.is_empty() && total > 0 {
        friends = cx.app.db.friends(owner, page * PER_PAGE, PER_PAGE).await?.0;
    }
    let name = cx.app.db.remembered_name(owner).await?.filter(|n| !n.is_empty());
    let title = match (owner == me, name) {
        (true, _) => "👥 Your friends".to_owned(),
        (false, Some(name)) => format!("👥 {name}'s friends"),
        (false, None) => "👥 Friends".to_owned(),
    };
    let body = if friends.is_empty() {
        if owner == me {
            "No friends yet. Ask someone with `/friends add`.".to_owned()
        } else {
            "No friends yet.".to_owned()
        }
    } else {
        friends
            .iter()
            .enumerate()
            .map(|(n, f)| {
                format!(
                    "**{}.** {} · Lv {} · 💖 {}{}",
                    page * PER_PAGE + n as i64 + 1,
                    mention(f.friend_id),
                    leveling::level_of(f.xp),
                    f.likes,
                    if f.locked { " · 🔒" } else { "" }
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    };
    let footer = match pages {
        1 => format!("{total} of {MAX_FRIENDS} friends"),
        p => format!("Page {}/{p} · {total} of {MAX_FRIENDS} friends", page + 1),
    };
    let embed = EmbedBuilder::new()
        .color(BRAND)
        .title(title)
        .description(body)
        .footer(EmbedFooterBuilder::new(footer))
        .build();
    let mut components = Vec::new();
    if pages > 1 {
        let nav = |label: &str, target: i64, disabled: bool| {
            let mut b = button(label, format!("fd:p:{owner}:{target}"), ButtonStyle::Secondary);
            if let Component::Button(inner) = &mut b {
                inner.disabled = disabled;
            }
            b
        };
        components.push(row(vec![
            nav("◀ Newer", page - 1, page == 0),
            nav("Older ▶", page + 1, page + 1 >= pages),
        ]));
    }
    Ok(InteractionResponseData {
        embeds: Some(vec![embed]),
        components: Some(components),
        ..Default::default()
    })
}

/// `/house visit` with nobody named: a menu of the friends whose houses are open.
pub(crate) async fn visit_shortlist(cx: &Cx, room: i16) -> anyhow::Result<()> {
    let me = cx.author()?.id.get();
    let friends = cx.app.db.visitable_friends(me, SHORTLIST).await?;
    if friends.is_empty() {
        return cx
            .reply_ephemeral(
                "Name whose house to visit: `/house visit user:@someone`. Friends you add with `/friends add` show up here as a shortlist (houses they have locked or hidden are left out).",
            )
            .await;
    }
    let options = friends.iter().map(option).collect();
    cx.reply(InteractionResponseData {
        content: Some("Whose house do you want to visit?".to_owned()),
        components: Some(vec![row(vec![Component::SelectMenu(SelectMenu {
            channel_types: None,
            custom_id: format!("fd:v:{room}"),
            default_values: None,
            disabled: false,
            id: None,
            kind: SelectMenuType::Text,
            max_values: Some(1),
            min_values: Some(1),
            options: Some(options),
            placeholder: Some("Visit a friend…".to_owned()),
            required: None,
        })])]),
        flags: Some(MessageFlags::EPHEMERAL),
        ..Default::default()
    })
    .await
}

fn option(friend: &FriendRow) -> SelectMenuOption {
    let name = if friend.name.is_empty() {
        "A friend"
    } else {
        &friend.name
    };
    SelectMenuOption {
        default: false,
        description: Some(format!("Lv {} · 💖 {}", leveling::level_of(friend.xp), friend.likes)),
        emoji: None,
        label: name.chars().take(100).collect(),
        value: friend.friend_id.to_string(),
    }
}

/// `fd:yes|no:<from>:<to>:<until>` buttons, `fd:p:<owner>:<page>` page buttons and the
/// `fd:v:<room>` shortlist.
pub(crate) async fn on_component(cx: &Cx, mut args: Split<'_, char>, value: Option<&str>) -> anyhow::Result<()> {
    match args.next() {
        Some(answer @ ("yes" | "no")) => match Request::parse(&mut args) {
            Some(request) => on_answer(cx, answer == "yes", request).await,
            None => closed(cx, "This friend request has expired.").await,
        },
        Some("p") => {
            let owner: u64 = args.next().and_then(|v| v.parse().ok()).unwrap_or(0);
            let page: i64 = args.next().and_then(|v| v.parse().ok()).unwrap_or(0);
            let data = list_page(cx, owner, page).await?;
            cx.update(data).await
        }
        Some("v") => {
            let room: i16 = args.next().and_then(|v| v.parse().ok()).unwrap_or(0);
            let Some(host) = value.and_then(|v| v.parse::<u64>().ok()) else {
                return cx.reply_ephemeral("Pick a friend to visit.").await;
            };
            // Somebody nobody remembered still gets a name: "{name} has locked their house" must not start with a space.
            let name = cx
                .app
                .db
                .remembered_name(host)
                .await?
                .filter(|n| !n.is_empty())
                .unwrap_or_else(|| "A friend".to_owned());
            let author = cx.author()?.clone();
            house::visit(cx, &author, host, name, room.clamp(0, 5)).await
        }
        _ => cx.reply_ephemeral("This button has expired.").await,
    }
}

/// Replaces the request with a plain line and no buttons.
async fn closed(cx: &Cx, text: impl Into<String>) -> anyhow::Result<()> {
    cx.update(InteractionResponseData {
        content: Some(text.into()),
        embeds: Some(Vec::new()),
        components: Some(Vec::new()),
        ..Default::default()
    })
    .await
}

async fn on_answer(cx: &Cx, accept: bool, request: Request) -> anyhow::Result<()> {
    let me = cx.author()?.id.get();
    if me != request.to && !(me == request.from && !accept) {
        return cx.reply_ephemeral("This request isn't for you.").await;
    }
    if request.expired(&cx.app) {
        return closed(cx, "This friend request has expired.").await;
    }
    if !accept {
        let text = if me == request.from {
            format!("{} took the friend request back.", mention(request.from))
        } else {
            format!("{} said no thanks to {}.", mention(request.to), mention(request.from))
        };
        return closed(cx, text).await;
    }
    let (from, to) = (mention(request.from), mention(request.to));
    match cx.app.db.befriend(request.from, request.to).await? {
        BefriendOutcome::Added => {
            // The names a list shows come from here, so the accepting side's is fresh too.
            let user = cx.author()?;
            cx.app.db.remember_name(me, &social::plain_name(user)).await?;
            let embed = EmbedBuilder::new()
                .color(SUCCESS)
                .title("🤝 New friends")
                .description(format!("{from} and {to} are friends now!"))
                .build();
            cx.update(InteractionResponseData {
                content: Some(String::new()),
                embeds: Some(vec![embed]),
                components: Some(Vec::new()),
                ..Default::default()
            })
            .await
        }
        BefriendOutcome::AlreadyFriends => closed(cx, format!("{from} and {to} are already friends.")).await,
        BefriendOutcome::Full { user } => {
            closed(
                cx,
                format!(
                    "{} has {MAX_FRIENDS} friends already, the most there can be.",
                    mention(user)
                ),
            )
            .await
        }
    }
}
