//! Friends: a request the other player accepts, a list, and the shortlist `/house visit` offers.

use std::str::Split;

use pinchora_core::{i18n::Lang, leveling, t};
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
                    return cx.reply_ephemeral(t!(cx.lang(), "friends.not-self")).await;
                }
                if cx.app.db.are_friends(me, to).await? {
                    return cx
                        .reply_ephemeral(t!(cx.lang(), "friends.already", user = mention(to)))
                        .await;
                }
                if cx.app.db.friend_count(me).await? >= MAX_FRIENDS {
                    return cx
                        .reply_ephemeral(t!(cx.lang(), "friends.you-full", max = MAX_FRIENDS))
                        .await;
                }
                if cx.app.db.friend_count(to).await? >= MAX_FRIENDS {
                    return cx
                        .reply_ephemeral(t!(
                            cx.lang(),
                            "friends.they-full",
                            user = mention(to),
                            max = MAX_FRIENDS
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
                let public_lang = match cx.guild_id() {
                    Some(guild) => cx.app.guild_lang(guild).await,
                    None => cx.lang(),
                };
                let embed = EmbedBuilder::new()
                    .color(BRAND)
                    .title(t!(public_lang, "friends.request-title"))
                    .description(t!(
                        public_lang,
                        "friends.request",
                        from = mention(me),
                        to = mention(to),
                        until = request.until
                    ))
                    .build();
                cx.reply(InteractionResponseData {
                    content: Some(mention(to)),
                    embeds: Some(vec![embed]),
                    components: Some(vec![request.buttons(
                        public_lang,
                        "fd",
                        &t!(public_lang, "friends.accept"),
                    )]),
                    allowed_mentions: social::ping_only(to),
                    ..Default::default()
                })
                .await
            }
            Self::Remove(c) => {
                let to = c.user.resolved.id.get();
                let text = if cx.app.db.unfriend(me, to).await? {
                    t!(cx.lang(), "friends.removed", user = mention(to))
                } else {
                    t!(cx.lang(), "friends.not-a-friend", user = mention(to))
                };
                cx.reply_ephemeral(text).await
            }
            Self::List(c) => {
                if c.user.as_ref().is_some_and(|u| u.resolved.bot) {
                    return cx.reply_ephemeral(t!(cx.lang(), "friends.bot")).await;
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
    let lang = cx.lang();
    let name = cx.app.db.remembered_name(owner).await?.filter(|n| !n.is_empty());
    let title = match (owner == me, name) {
        (true, _) => t!(lang, "friends.title-own"),
        (false, Some(name)) => t!(lang, "friends.title-of", name = name),
        (false, None) => t!(lang, "friends.title"),
    };
    let body = if friends.is_empty() {
        if owner == me {
            t!(lang, "friends.empty-own")
        } else {
            t!(lang, "friends.empty")
        }
    } else {
        friends
            .iter()
            .enumerate()
            .map(|(n, f)| {
                t!(
                    lang,
                    "friends.row",
                    number = page * PER_PAGE + n as i64 + 1,
                    user = mention(f.friend_id),
                    level = leveling::level_of(f.xp),
                    likes = f.likes,
                    locked = if f.locked { " · 🔒" } else { "" }
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    };
    let footer = match pages {
        1 => t!(lang, "friends.footer", total = total, max = MAX_FRIENDS),
        p => t!(
            lang,
            "friends.footer-paged",
            page = page + 1,
            pages = p,
            total = total,
            max = MAX_FRIENDS
        ),
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
            nav(&t!(lang, "friends.newer"), page - 1, page == 0),
            nav(&t!(lang, "friends.older"), page + 1, page + 1 >= pages),
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
        return cx.reply_ephemeral(t!(cx.lang(), "friends.shortlist-empty")).await;
    }
    let lang = cx.lang();
    let options = friends.iter().map(|f| option(lang, f)).collect();
    cx.reply(InteractionResponseData {
        content: Some(t!(lang, "friends.shortlist-ask")),
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
            placeholder: Some(t!(lang, "friends.shortlist-placeholder")),
            required: None,
        })])]),
        flags: Some(MessageFlags::EPHEMERAL),
        ..Default::default()
    })
    .await
}

fn option(lang: Lang, friend: &FriendRow) -> SelectMenuOption {
    let fallback = t!(lang, "friends.a-friend");
    let name = if friend.name.is_empty() {
        &fallback
    } else {
        &friend.name
    };
    SelectMenuOption {
        default: false,
        description: Some(t!(
            lang,
            "friends.option",
            level = leveling::level_of(friend.xp),
            likes = friend.likes
        )),
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
            None => closed(cx, t!(cx.lang(), "friends.expired")).await,
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
                return cx.reply_ephemeral(t!(cx.lang(), "friends.pick")).await;
            };
            // Somebody nobody remembered still gets a name: "{name} has locked their house" must not start with a space.
            let name = cx
                .app
                .db
                .remembered_name(host)
                .await?
                .filter(|n| !n.is_empty())
                .unwrap_or_else(|| t!(cx.lang(), "friends.a-friend"));
            let author = cx.author()?.clone();
            house::visit(cx, &author, host, name, room.clamp(0, 5)).await
        }
        _ => cx.reply_ephemeral(t!(cx.lang(), "social.button-expired")).await,
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
    let public_lang = match cx.guild_id() {
        Some(guild) => cx.app.guild_lang(guild).await,
        None => cx.lang(),
    };
    if me != request.to && !(me == request.from && !accept) {
        return cx.reply_ephemeral(t!(cx.lang(), "social.not-for-you")).await;
    }
    if request.expired(&cx.app) {
        return closed(cx, t!(public_lang, "friends.expired")).await;
    }
    if !accept {
        let text = if me == request.from {
            t!(public_lang, "friends.withdrawn", from = mention(request.from))
        } else {
            t!(
                public_lang,
                "social.said-no",
                to = mention(request.to),
                from = mention(request.from)
            )
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
                .title(t!(public_lang, "friends.new-title"))
                .description(t!(public_lang, "friends.new", from = from, to = to))
                .build();
            cx.update(InteractionResponseData {
                content: Some(String::new()),
                embeds: Some(vec![embed]),
                components: Some(Vec::new()),
                ..Default::default()
            })
            .await
        }
        BefriendOutcome::AlreadyFriends => {
            closed(cx, t!(public_lang, "friends.already-pair", from = from, to = to)).await
        }
        BefriendOutcome::Full { user } => {
            closed(
                cx,
                t!(
                    public_lang,
                    "friends.they-full",
                    user = mention(user),
                    max = MAX_FRIENDS
                ),
            )
            .await
        }
    }
}
