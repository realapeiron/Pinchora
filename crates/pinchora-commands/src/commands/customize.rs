use std::{collections::HashSet, str::Split, sync::Arc};

use pinchora_core::{leveling, stats::PlayerStats};
use pinchora_db::{CustomizeBuy, MAX_BADGES, UserRow};
use pinchora_game::{Background, Badge, BadgeKind, BadgeStat, Content, Rarity, TitleKind, stat_value};
use pinchora_render::{
    Backdrop, BadgeCell, BadgeCollectionCard, BadgeIcon, BadgeState, Frame, Opacity, SpriteArt, Theme, parse_accent,
    sprites::placeholder_badge_color,
};
use twilight_interactions::command::{CommandModel, CreateCommand};
use twilight_model::{
    channel::message::{
        Component,
        component::{ActionRow, Button, ButtonStyle, SelectMenu, SelectMenuOption, SelectMenuType},
    },
    http::interaction::InteractionResponseData,
};
use twilight_util::builder::embed::{EmbedBuilder, EmbedFieldBuilder};

use crate::{
    App, Cx,
    commands::profile,
    game::scene::{ScaledArt, Sprites},
    util::{BRAND, thousands},
};

mod showcase;
mod style;
mod titles;
mod unlocks;

pub(crate) use showcase::{Showcase, on_showcase};
pub(crate) use style::{on_style, on_style_modal};
pub(crate) use titles::on_title;
pub(crate) use unlocks::{announce, announce_in};

/// Discord shows at most this many options in one select menu.
const PER_PAGE: usize = 25;

#[derive(CommandModel, CreateCommand)]
#[command(
    name = "customize",
    desc = "Backgrounds, badges, titles, colours, frames and a showcase for your cards"
)]
pub enum CustomizeCommand {
    #[command(name = "background")]
    Background(CustomizeBackground),
    #[command(name = "badges")]
    Badges(CustomizeBadges),
    #[command(name = "title")]
    Title(CustomizeTitle),
    #[command(name = "style")]
    Style(CustomizeStyle),
    #[command(name = "showcase")]
    Showcase(CustomizeShowcase),
}

#[derive(CommandModel, CreateCommand)]
#[command(
    name = "background",
    desc = "Buy and wear a background for your profile, rank and wallet cards"
)]
pub struct CustomizeBackground;

#[derive(CommandModel, CreateCommand)]
#[command(
    name = "badges",
    desc = "Your badge collection: earn, buy and wear the badges on your profile card"
)]
pub struct CustomizeBadges;

#[derive(CommandModel, CreateCommand)]
#[command(
    name = "title",
    desc = "Pick the title under your name on your profile and rank cards"
)]
pub struct CustomizeTitle;

#[derive(CommandModel, CreateCommand)]
#[command(
    name = "style",
    desc = "Accent colour, panel opacity and a frame for your profile, rank and wallet cards"
)]
pub struct CustomizeStyle;

#[derive(CommandModel, CreateCommand)]
#[command(
    name = "showcase",
    desc = "Put a favourite fish, a pet, a furniture item and your house on your profile card"
)]
pub struct CustomizeShowcase;

impl CustomizeCommand {
    pub async fn run(self, cx: &Cx) -> anyhow::Result<()> {
        let data = match self {
            Self::Background(_) => background_panel(cx, Highlight::Equipped, None, "").await?,
            Self::Badges(_) => badges_panel(cx, 0, "", Fresh::Open).await?,
            Self::Title(_) => titles::panel(cx, 0, "").await?,
            Self::Style(_) => style::panel(cx, "").await?,
            Self::Showcase(_) => showcase::panel(cx, showcase::Slot::Fish, 0, "").await?,
        };
        cx.reply(data).await
    }
}

// /Cards

/// A player's background and badges as the cards draw them. Owned, so it can move to the render thread.
pub(crate) struct Cosmetics {
    /// The art (`None` until it is drawn) and the accent colour.
    background: Option<(Option<Arc<ScaledArt>>, String)>,
    /// Each badge's art, the letter and colour of its placeholder, and the colour of its rarity.
    badges: Vec<(Option<Arc<ScaledArt>>, char, u32, u32)>,
}

impl Cosmetics {
    pub(crate) fn new(sprites: &Sprites, background: Option<&Background>, badges: &[&Badge]) -> Self {
        Self {
            background: background.map(|b| (sprites.background(&b.id), b.accent.clone())),
            badges: badges
                .iter()
                .map(|b| {
                    (
                        sprites.badge(&b.id),
                        letter_of(b),
                        placeholder_badge_color(&b.id),
                        b.rarity.color(),
                    )
                })
                .collect(),
        }
    }

    pub(crate) fn backdrop(&self) -> Option<Backdrop<'_>> {
        self.background.as_ref().map(|(art, accent)| Backdrop {
            art: art.as_ref().map(|a| SpriteArt {
                png: &a.png,
                w: a.w,
                h: a.h,
            }),
            accent,
        })
    }

    pub(crate) fn icons(&self) -> Vec<BadgeIcon<'_>> {
        self.badges
            .iter()
            .map(|(art, letter, color, rarity)| BadgeIcon {
                art: art.as_ref().map(|a| SpriteArt {
                    png: &a.png,
                    w: a.w,
                    h: a.h,
                }),
                letter: *letter,
                color: *color,
                rarity: *rarity,
            })
            .collect()
    }
}

/// The capital the badge's placeholder shows until its art is drawn.
fn letter_of(badge: &Badge) -> char {
    badge
        .name
        .chars()
        .next()
        .map_or('?', |c| c.to_uppercase().next().unwrap_or(c))
}

/// The card palette and look of a player's cards: the accent colour they chose, else their
/// background's, else the colour from before accents were a choice, else the reef coral; their
/// panel opacity; and their frame, if their level has unlocked it.
pub(crate) fn theme(row: &UserRow, background: Option<&Background>) -> Theme {
    let chosen = row.accent.as_deref().and_then(|a| parse_accent(a).ok());
    let accent = chosen
        .or_else(|| background.and_then(Background::accent_rgb))
        .or(row.accent_color.map(|c| c as u32));
    Theme::reef()
        .with_accent(accent)
        .with_style(Opacity::from_index(row.panel_opacity), frame_of(row))
}

/// The frame around the player's cards: the one they chose, if it exists and their level has unlocked it.
pub(crate) fn frame_of(row: &UserRow) -> Frame {
    Frame::from_id(&row.frame)
        .filter(|f| f.is_unlocked(leveling::level_of(row.xp)))
        .unwrap_or_default()
}

// /Badges

/// What a player has earned and bought, and which badges they have put on.
struct Wardrobe {
    stats: PlayerStats,
    /// Shop badges they bought.
    bought: Vec<String>,
    /// The ids saved as worn, in order.
    equipped: Vec<String>,
    /// Whether they chose their badges. If not, the card shows their best ones instead.
    chosen: bool,
    /// Titles granted to them outside the stats (`user_titles`); only [`Wardrobe::load_titles`] fills it.
    granted: Vec<String>,
}

impl Wardrobe {
    async fn load(app: &App, user: u64, row: &UserRow) -> anyhow::Result<Self> {
        Ok(Self {
            stats: app.db.player_stats(user).await?,
            bought: app.db.owned_badges(user).await?,
            equipped: row.badges.clone(),
            chosen: row.badges_set,
            granted: Vec::new(),
        })
    }

    /// Like [`Wardrobe::load`], with the titles they were granted too.
    async fn load_titles(app: &App, user: u64, row: &UserRow) -> anyhow::Result<Self> {
        let mut wardrobe = Self::load(app, user, row).await?;
        wardrobe.granted = app.db.granted_titles(user).await?;
        Ok(wardrobe)
    }

    /// Whether the badge is theirs: reached for an achievement, bought for a shop badge.
    fn has(&self, badge: &Badge) -> bool {
        badge.is_earned(&self.stats, &self.bought)
    }

    /// Every badge they could put on now, in the order of `content/badges.json`.
    fn wearable(&self) -> Vec<&'static Badge> {
        Content::get().earned_badges(&self.stats, &self.bought)
    }

    /// What the card shows when they have not chosen: up to six of their badges, the rarest
    /// first (the order of `content/badges.json` breaks ties).
    fn best(&self) -> Vec<&'static Badge> {
        let mut badges = self.wearable();
        badges.sort_by_key(|b| std::cmp::Reverse(b.rarity));
        badges.truncate(MAX_BADGES);
        badges
    }

    /// The badges on their card: the saved ones they may still wear, or, until they choose,
    /// their best. A saved badge that stopped counting (a streak that ended, shells spent)
    /// drops out until it is earned again.
    fn worn(&self) -> Vec<&'static Badge> {
        if !self.chosen {
            return self.best();
        }
        let content = Content::get();
        self.equipped
            .iter()
            .filter_map(|id| content.badge(id))
            .filter(|b| self.has(b))
            .take(MAX_BADGES)
            .collect()
    }

    /// `✅ Name · Reach level 5.` once it is theirs, `🔒 Name · 3/5 levels` before.
    fn line(&self, badge: &Badge, worn: &[&Badge]) -> String {
        let worn = if worn.iter().any(|w| w.id == badge.id) {
            " *(worn)*"
        } else {
            ""
        };
        match badge.kind {
            BadgeKind::Achievement { stat, at } => {
                let have = stat_value(&self.stats, stat);
                if have >= at {
                    format!("✅ **{}** · {}{worn}", badge.name, badge.description)
                } else {
                    format!(
                        "🔒 **{}** · {}/{} {}",
                        badge.name,
                        thousands(have.max(0)),
                        thousands(at),
                        stat.noun()
                    )
                }
            }
            BadgeKind::Shop { .. } if self.has(badge) => format!("✅ **{}** · bought{worn}", badge.name),
            BadgeKind::Shop { price } => format!("🛒 **{}** · {} shells", badge.name, thousands(price)),
        }
    }

    /// Whether the title is theirs: its stat reached, its badge owned, or granted to them.
    fn has_title(&self, title: &pinchora_game::Title) -> bool {
        self.granted.contains(&title.id)
            || match &title.kind {
                TitleKind::Stat { stat, at } => stat_value(&self.stats, *stat) >= *at,
                TitleKind::Badge { badge } => Content::get().badge(badge).is_some_and(|b| self.has(b)),
                TitleKind::Special => false,
            }
    }

    /// `✅ **Tide Angler** · Catch 5 different fish species.` once unlocked, else how far they are.
    fn title_line(&self, title: &pinchora_game::Title, equipped: Option<&str>) -> String {
        let worn = if equipped == Some(title.id.as_str()) {
            " *(worn)*"
        } else {
            ""
        };
        if self.has_title(title) {
            return format!("✅ **{}** · {}{worn}", title.name, title.description);
        }
        match &title.kind {
            TitleKind::Stat { stat, at } => format!(
                "🔒 **{}** · {}/{} {}",
                title.name,
                thousands(stat_value(&self.stats, *stat).max(0)),
                thousands(*at),
                stat.noun()
            ),
            _ => format!("🔒 **{}** · {}", title.name, title.description),
        }
    }
}

/// The badges a profile card shows for `user`: the ones saved as worn that they still may wear,
/// or their best earned ones until they have chosen.
pub(crate) async fn worn_badges(app: &App, user: u64, row: &UserRow) -> anyhow::Result<Vec<&'static Badge>> {
    if row.badges_set && row.badges.is_empty() {
        return Ok(Vec::new());
    }
    Ok(Wardrobe::load(app, user, row).await?.worn())
}

/// The name of the title under the player's name, if they have one and it still exists.
pub(crate) fn title_name(row: &UserRow) -> Option<&'static str> {
    row.title
        .as_deref()
        .and_then(|id| Content::get().title(id))
        .map(|t| t.name.as_str())
}

// /Components

fn row(components: Vec<Component>) -> Component {
    Component::ActionRow(ActionRow { id: None, components })
}

fn button(custom_id: String, label: &str, style: ButtonStyle, disabled: bool) -> Component {
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

fn menu(
    custom_id: String,
    placeholder: &str,
    options: Vec<SelectMenuOption>,
    min: u8,
    max: u8,
    empty: &str,
) -> Component {
    let disabled = options.is_empty();
    Component::SelectMenu(SelectMenu {
        channel_types: None,
        custom_id,
        default_values: None,
        disabled,
        id: None,
        kind: SelectMenuType::Text,
        max_values: Some(if disabled { 1 } else { max.min(options.len() as u8) }),
        min_values: Some(if disabled { 1 } else { min }),
        options: Some(if disabled {
            vec![SelectMenuOption {
                default: false,
                description: None,
                emoji: None,
                label: empty.to_owned(),
                value: "none".to_owned(),
            }]
        } else {
            options
        }),
        placeholder: Some(placeholder.to_owned()),
        required: None,
    })
}

fn option(label: &str, value: &str, description: String, default: bool) -> SelectMenuOption {
    SelectMenuOption {
        default,
        description: Some(description.chars().take(100).collect()),
        emoji: None,
        label: label.to_owned(),
        value: value.to_owned(),
    }
}

/// `-` stands for "no background" inside custom ids.
fn id_part(background: Option<&Background>) -> &str {
    background.map_or("-", |b| b.id.as_str())
}

// /Background

/// Which background the panel previews.
#[derive(Clone, Copy)]
enum Highlight<'a> {
    /// The one on their cards now.
    Equipped,
    Nothing,
    Id(&'a str),
}

/// The backgrounds the player owns (oldest purchase first), then the rest of the shop, cheapest first.
fn background_order<'a>(content: &'a Content, owned: &[String]) -> Vec<&'a Background> {
    let mut order: Vec<&Background> = owned.iter().filter_map(|id| content.background(id)).collect();
    let mut shop: Vec<&Background> = content.backgrounds.iter().filter(|b| !owned.contains(&b.id)).collect();
    shop.sort_by_key(|b| (b.price, &b.id));
    order.extend(shop);
    order
}

/// The ephemeral background editor: their real profile card with the highlighted background, a
/// menu of every background (owned first), and Buy, Equip and Remove for the highlighted one.
async fn background_panel(
    cx: &Cx,
    highlight: Highlight<'_>,
    page: Option<usize>,
    note: &str,
) -> anyhow::Result<InteractionResponseData> {
    let content = Content::get();
    let user = cx.author()?;
    let id = user.id.get();
    let player = cx.app.db.user(id).await?;
    let owned = cx.app.db.owned_backgrounds(id).await?;
    let equipped = player.background.as_deref().and_then(|e| content.background(e));
    let shown = match highlight {
        Highlight::Equipped => equipped,
        Highlight::Nothing => None,
        Highlight::Id(shown) => content.background(shown),
    };
    let is_equipped = |b: &Background| equipped.is_some_and(|e| e.id == b.id);
    let is_owned = |b: &Background| owned.contains(&b.id);

    let order = background_order(content, &owned);
    let pages = order.len().div_ceil(PER_PAGE).max(1);
    let page = page
        .or_else(|| {
            shown
                .and_then(|s| order.iter().position(|b| b.id == s.id))
                .map(|p| p / PER_PAGE)
        })
        .unwrap_or(0)
        .min(pages - 1);
    let options = order
        .iter()
        .skip(page * PER_PAGE)
        .take(PER_PAGE)
        .map(|b| {
            let status = if is_equipped(b) {
                "Equipped".to_owned()
            } else if is_owned(b) {
                "Owned".to_owned()
            } else {
                format!("{} shells", thousands(b.price))
            };
            option(
                &b.name,
                &b.id,
                format!("{} · {status}", b.rarity.label()),
                shown.is_some_and(|s| s.id == b.id),
            )
        })
        .collect();

    let worn = worn_badges(&cx.app, id, &player).await?;
    let credits = player.credits;
    let png = Box::pin(profile::card(cx, user, cx.author_display_name(), player, shown, &worn)).await?;

    let mut text = String::new();
    if !note.is_empty() {
        text.push_str(note);
        text.push_str("\n\n");
    }
    match shown {
        Some(b) => {
            let status = if is_equipped(b) {
                "on your cards now".to_owned()
            } else if is_owned(b) {
                "owned, not worn".to_owned()
            } else {
                format!("{} shells", thousands(b.price))
            };
            text.push_str(&format!("**{}** · {} · {status}", b.name, b.rarity.label()));
        }
        None => text.push_str("Pick a background below to see it on your profile card."),
    }
    text.push_str(&format!("\nYou have **{}** shells.", thousands(credits)));

    let hid = id_part(shown);
    let can_buy = shown.is_some_and(|b| !is_owned(b));
    let can_equip = shown.is_some_and(|b| is_owned(b) && !is_equipped(b));
    let components = vec![
        row(vec![menu(
            "cz:bs".to_owned(),
            "Pick a background…",
            options,
            1,
            1,
            "Nothing to show",
        )]),
        row(vec![
            button(
                format!("cz:bp:{}:{hid}", page.saturating_sub(1)),
                "◀ Page",
                ButtonStyle::Secondary,
                page == 0,
            ),
            button(
                format!("cz:bp:{}:{hid}", page + 1),
                "Page ▶",
                ButtonStyle::Secondary,
                page + 1 >= pages,
            ),
            button(format!("cz:bb:{hid}"), "Buy", ButtonStyle::Success, !can_buy),
            button(format!("cz:be:{hid}"), "Equip", ButtonStyle::Primary, !can_equip),
            button("cz:br".to_owned(), "Remove", ButtonStyle::Danger, equipped.is_none()),
        ]),
    ];
    let embed = EmbedBuilder::new()
        .color(BRAND)
        .title("Card backgrounds")
        .description(text);
    Ok(Cx::card(embed, "profile.png", png, components, true))
}

async fn expired(cx: &Cx, command: &str) -> anyhow::Result<()> {
    cx.reply_ephemeral(format!(
        "That choice isn't available any more. Run `/customize {command}` again."
    ))
    .await
}

async fn buy_background(cx: &Cx, id: &str) -> anyhow::Result<()> {
    let Some(background) = Content::get().background(id) else {
        return expired(cx, "background").await;
    };
    let outcome = cx
        .app
        .db
        .buy_background(cx.author()?.id.get(), &background.id, background.price)
        .await?;
    let note = match outcome {
        CustomizeBuy::Bought { balance } => format!(
            "🛍️ Bought **{}** for {} shells. {} left. Press **Equip** to put it on your cards.",
            background.name,
            thousands(background.price),
            thousands(balance)
        ),
        CustomizeBuy::AlreadyOwned => format!("You already own **{}**.", background.name),
        CustomizeBuy::Insufficient { balance } => format!(
            "**{}** costs {} shells; you have {}.",
            background.name,
            thousands(background.price),
            thousands(balance)
        ),
    };
    let data = background_panel(cx, Highlight::Id(&background.id), None, &note).await?;
    cx.update(data).await
}

async fn equip_background(cx: &Cx, id: &str) -> anyhow::Result<()> {
    let Some(background) = Content::get().background(id) else {
        return expired(cx, "background").await;
    };
    let note = if cx
        .app
        .db
        .equip_background(cx.author()?.id.get(), &background.id)
        .await?
    {
        format!(
            "Saved. **{}** is on your profile, rank and wallet cards.",
            background.name
        )
    } else {
        format!("You don't own **{}** yet. Press **Buy** first.", background.name)
    };
    let data = background_panel(cx, Highlight::Id(&background.id), None, &note).await?;
    cx.update(data).await
}

// /Badges panel

/// One embed field per thing achievements count, then the shop: a line for every badge.
fn badge_fields(wardrobe: &Wardrobe) -> Vec<(&'static str, String)> {
    let badges = &Content::get().badges;
    let worn = wardrobe.worn();
    let mut fields: Vec<_> = BadgeStat::ALL
        .into_iter()
        .map(|stat| {
            let lines: Vec<_> = badges
                .iter()
                .filter(|b| matches!(b.kind, BadgeKind::Achievement { stat: s, .. } if s == stat))
                .map(|b| wardrobe.line(b, &worn))
                .collect();
            (stat.label(), lines.join("\n"))
        })
        .collect();
    let shop: Vec<_> = badges
        .iter()
        .filter(|b| matches!(b.kind, BadgeKind::Shop { .. }))
        .map(|b| wardrobe.line(b, &worn))
        .collect();
    fields.push(("Shop", shop.join("\n")));
    fields.retain(|(_, lines)| !lines.is_empty());
    fields
}

/// The badges earned since the player last opened their collection, as a bit per badge of
/// `content/badges.json` (the first 128). The panel's buttons carry it, so its NEW markers
/// stay while the player works in it.
fn fresh_mask(fresh: &[String]) -> u128 {
    Content::get()
        .badges
        .iter()
        .take(128)
        .enumerate()
        .filter(|(_, b)| fresh.contains(&b.id))
        .fold(0, |mask, (i, _)| mask | 1 << i)
}

/// Whether badge number `index` of `content/badges.json` is in `mask`.
fn is_fresh(mask: u128, index: usize) -> bool {
    index < 128 && mask >> index & 1 == 1
}

/// Which badges the collection marks NEW.
#[derive(Clone, Copy)]
enum Fresh {
    /// The player just opened the collection: whatever they earned since last time is new, and
    /// from now on it has been seen.
    Open,
    /// A click in an open panel: the markers its buttons carry.
    Keep(u128),
}

/// A badge of the collection, owned so the card can be drawn on the render thread.
struct CollectionCell {
    art: Option<Arc<ScaledArt>>,
    letter: char,
    color: u32,
    rarity: u32,
    name: &'static str,
    rarity_label: &'static str,
    state: BadgeState,
}

/// Every badge in the order of `content/badges.json`, as the collection card shows it.
fn collection_cells(sprites: &Sprites, wardrobe: &Wardrobe, worn: &[&Badge], fresh: u128) -> Vec<CollectionCell> {
    Content::get()
        .badges
        .iter()
        .enumerate()
        .map(|(i, b)| {
            let state = match b.kind {
                _ if wardrobe.has(b) => BadgeState::Earned {
                    new: is_fresh(fresh, i),
                    worn: worn.iter().position(|w| w.id == b.id).map(|p| p as u8 + 1),
                },
                BadgeKind::Achievement { stat, at } => BadgeState::Locked {
                    have: stat_value(&wardrobe.stats, stat),
                    at,
                },
                BadgeKind::Shop { price } => BadgeState::ForSale { price },
            };
            CollectionCell {
                art: sprites.badge(&b.id),
                letter: letter_of(b),
                color: placeholder_badge_color(&b.id),
                rarity: b.rarity.color(),
                name: &b.name,
                rarity_label: b.rarity.label(),
                state,
            }
        })
        .collect()
}

/// The ephemeral badge editor: the collection card (every badge, earned in colour, locked ones
/// dark with their progress, NEW on the latest), what the profile card shows, a menu to put up to
/// six on, a menu to buy shop badges, and buttons to take them off or let the card pick.
async fn badges_panel(cx: &Cx, page: usize, note: &str, fresh: Fresh) -> anyhow::Result<InteractionResponseData> {
    let content = Content::get();
    let user = cx.author()?;
    let id = user.id.get();
    let player = cx.app.db.user(id).await?;
    let wardrobe = Wardrobe::load(&cx.app, id, &player).await?;
    let worn = wardrobe.worn();
    let wearable = wardrobe.wearable();

    let mask = match fresh {
        Fresh::Keep(mask) => mask,
        Fresh::Open => {
            let earned: Vec<String> = wearable.iter().map(|b| b.id.clone()).collect();
            // A player seen before notices existed has no list yet: nothing is new to them.
            let new: Vec<String> = match &player.badges_seen {
                Some(seen) => earned.iter().filter(|b| !seen.contains(b)).cloned().collect(),
                None => Vec::new(),
            };
            cx.app.db.mark_badges_seen(id, &earned).await?;
            fresh_mask(&new)
        }
    };
    let pages = wearable.len().div_ceil(PER_PAGE).max(1);
    let page = page.min(pages - 1);

    let wear_options = wearable
        .iter()
        .skip(page * PER_PAGE)
        .take(PER_PAGE)
        .map(|b| {
            option(
                &b.name,
                &b.id,
                format!("{} · {}", b.rarity.label(), b.description),
                worn.iter().any(|w| w.id == b.id),
            )
        })
        .collect();
    let buy_options = content
        .badges
        .iter()
        .filter_map(|b| match b.kind {
            BadgeKind::Shop { price } if !wardrobe.has(b) => Some(option(
                &b.name,
                &b.id,
                format!("{} shells · {}", thousands(price), b.description),
                false,
            )),
            _ => None,
        })
        .collect();

    let background = player.background.as_deref().and_then(|e| content.background(e));
    let (credits, saved_empty) = (player.credits, player.badges.is_empty());
    let theme = theme(&player, background);
    let cells = collection_cells(&cx.app.sprites, &wardrobe, &worn, mask);
    let png = cx
        .app
        .render(move |r| {
            let boxed: Vec<BadgeCell<'_>> = cells
                .iter()
                .map(|c| BadgeCell {
                    icon: BadgeIcon {
                        art: c.art.as_ref().map(|a| SpriteArt {
                            png: &a.png,
                            w: a.w,
                            h: a.h,
                        }),
                        letter: c.letter,
                        color: c.color,
                        rarity: c.rarity,
                    },
                    name: c.name,
                    rarity: c.rarity_label,
                    state: c.state,
                })
                .collect();
            let legend: Vec<(&str, u32)> = Rarity::ALL.iter().map(|r| (r.label(), r.color())).collect();
            r.badge_collection(&BadgeCollectionCard {
                cells: &boxed,
                legend: &legend,
                theme,
            })
        })
        .await?;

    let mut text = String::new();
    if !note.is_empty() {
        text.push_str(note);
        text.push_str("\n\n");
    }
    let names: Vec<_> = worn.iter().map(|b| format!("**{}**", b.name)).collect();
    if worn.is_empty() && wardrobe.chosen {
        text.push_str("You are not wearing any badges.");
    } else if worn.is_empty() {
        text.push_str("Your best badges will show on your card on their own once you have earned some.");
    } else if wardrobe.chosen {
        text.push_str(&format!("Wearing {}/{MAX_BADGES}: {}.", worn.len(), names.join(", ")));
    } else {
        text.push_str(&format!(
            "Your card shows your best badges on its own ({}/{MAX_BADGES}): {}.",
            worn.len(),
            names.join(", ")
        ));
    }
    text.push_str(&format!(
        " Pick up to {MAX_BADGES} below.\nYou have **{}** shells.",
        thousands(credits)
    ));
    let mut embed = EmbedBuilder::new().color(BRAND).title("Your badges").description(text);
    for (name, lines) in badge_fields(&wardrobe) {
        embed = embed.field(EmbedFieldBuilder::new(name, lines));
    }

    let mut buttons = Vec::new();
    if pages > 1 {
        buttons.push(button(
            format!("cz:wp:{}:{mask:x}", page.saturating_sub(1)),
            "◀ Page",
            ButtonStyle::Secondary,
            page == 0,
        ));
        buttons.push(button(
            format!("cz:wp:{}:{mask:x}", page + 1),
            "Page ▶",
            ButtonStyle::Secondary,
            page + 1 >= pages,
        ));
    }
    buttons.push(button(
        format!("cz:wc:{mask:x}"),
        "Take all off",
        ButtonStyle::Danger,
        wardrobe.chosen && saved_empty,
    ));
    buttons.push(button(
        format!("cz:wa:{mask:x}"),
        "Show my best automatically",
        ButtonStyle::Secondary,
        !wardrobe.chosen,
    ));
    let components = vec![
        row(vec![menu(
            format!("cz:ws:{page}:{mask:x}"),
            "Badges to wear…",
            wear_options,
            0,
            MAX_BADGES as u8,
            "Nothing to wear yet",
        )]),
        row(vec![menu(
            format!("cz:wb:{mask:x}"),
            "Buy a shop badge…",
            buy_options,
            1,
            1,
            "Nothing to buy",
        )]),
        row(buttons),
    ];
    Ok(Cx::card(embed, "badges.png", png, components, true))
}

/// `cz:ws:<page>:<new>`: the menu's picks replace what was worn from that page; badges worn from other pages stay.
async fn wear(cx: &Cx, page: usize, fresh: u128, values: &[String]) -> anyhow::Result<()> {
    let id = cx.author()?.id.get();
    let saved = cx.app.db.user(id).await?;
    let wardrobe = Wardrobe::load(&cx.app, id, &saved).await?;
    let wearable = wardrobe.wearable();

    let mut chosen: Vec<&str> = Vec::new();
    for value in values {
        match wearable.iter().find(|b| &b.id == value) {
            Some(b) => chosen.push(&b.id),
            None => return expired(cx, "badges").await,
        }
    }
    let in_view: HashSet<&str> = wearable
        .iter()
        .skip(page * PER_PAGE)
        .take(PER_PAGE)
        .map(|b| b.id.as_str())
        .collect();
    let mut next: Vec<String> = wardrobe
        .worn()
        .iter()
        .map(|b| b.id.clone())
        .filter(|b| !in_view.contains(b.as_str()) || chosen.contains(&b.as_str()))
        .collect();
    for pick in chosen {
        if !next.iter().any(|n| n == pick) {
            next.push(pick.to_owned());
        }
    }
    let note = if next.len() > MAX_BADGES {
        format!("You can wear {MAX_BADGES} badges at most. Take one off first.")
    } else {
        cx.app.db.set_badges(id, &next).await?;
        "Saved.".to_owned()
    };
    let data = badges_panel(cx, page, &note, Fresh::Keep(fresh)).await?;
    cx.update(data).await
}

async fn buy_badge(cx: &Cx, id: &str, fresh: u128) -> anyhow::Result<()> {
    let Some(badge) = Content::get().badge(id) else {
        return expired(cx, "badges").await;
    };
    let BadgeKind::Shop { price } = badge.kind else {
        return cx.reply_ephemeral("That badge is earned, not sold.").await;
    };
    let note = match cx.app.db.buy_badge(cx.author()?.id.get(), &badge.id, price).await? {
        CustomizeBuy::Bought { balance } => format!(
            "🛍️ Bought **{}** for {} shells. {} left. Pick it in the first menu to wear it.",
            badge.name,
            thousands(price),
            thousands(balance)
        ),
        CustomizeBuy::AlreadyOwned => format!("You already own **{}**.", badge.name),
        CustomizeBuy::Insufficient { balance } => format!(
            "**{}** costs {} shells; you have {}.",
            badge.name,
            thousands(price),
            thousands(balance)
        ),
    };
    let data = badges_panel(cx, 0, &note, Fresh::Keep(fresh)).await?;
    cx.update(data).await
}

/// `cz:<what>:...` selects and buttons of the editors.
pub(crate) async fn on_component(cx: &Cx, mut args: Split<'_, char>, values: &[String]) -> anyhow::Result<()> {
    let content = Content::get();
    let value = values.first().map(String::as_str);
    let page = |arg: Option<&str>| arg.and_then(|p| p.parse().ok()).unwrap_or(0);
    // The badges NEW on the collection, as the hex bit mask the panel's ids carry.
    let fresh = |arg: Option<&str>| arg.and_then(|m| u128::from_str_radix(m, 16).ok()).unwrap_or(0);
    // `-` is "no background": the buttons act on nothing then.
    fn shown(arg: Option<&str>) -> Option<&str> {
        arg.filter(|a| *a != "-")
    }
    match args.next() {
        Some("bs") => match value.and_then(|v| content.background(v)) {
            Some(b) => {
                let data = background_panel(cx, Highlight::Id(&b.id), None, "").await?;
                cx.update(data).await
            }
            None => expired(cx, "background").await,
        },
        Some("bp") => {
            let (target, id) = (page(args.next()), shown(args.next()));
            let highlight = id.map_or(Highlight::Nothing, Highlight::Id);
            let data = background_panel(cx, highlight, Some(target), "").await?;
            cx.update(data).await
        }
        Some("bb") => match shown(args.next()) {
            Some(id) => buy_background(cx, id).await,
            None => expired(cx, "background").await,
        },
        Some("be") => match shown(args.next()) {
            Some(id) => equip_background(cx, id).await,
            None => expired(cx, "background").await,
        },
        Some("br") => {
            cx.app.db.remove_background(cx.author()?.id.get()).await?;
            let note = "Removed. Your cards are plain again.";
            let data = background_panel(cx, Highlight::Nothing, None, note).await?;
            cx.update(data).await
        }
        Some("ws") => {
            let target = page(args.next());
            wear(cx, target, fresh(args.next()), values).await
        }
        Some("wb") => match value.filter(|v| *v != "none") {
            Some(id) => buy_badge(cx, id, fresh(args.next())).await,
            None => expired(cx, "badges").await,
        },
        Some("wp") => {
            let (target, mask) = (page(args.next()), fresh(args.next()));
            let data = badges_panel(cx, target, "", Fresh::Keep(mask)).await?;
            cx.update(data).await
        }
        Some("wc") => {
            cx.app.db.set_badges(cx.author()?.id.get(), &[]).await?;
            let note = "Took every badge off. Your card shows none.";
            let data = badges_panel(cx, 0, note, Fresh::Keep(fresh(args.next()))).await?;
            cx.update(data).await
        }
        Some("wa") => {
            cx.app.db.reset_badges(cx.author()?.id.get()).await?;
            let note = "Done. Your card shows your best badges again, the rarest first.";
            let data = badges_panel(cx, 0, note, Fresh::Keep(fresh(args.next()))).await?;
            cx.update(data).await
        }
        _ => cx.reply_ephemeral("This button has expired.").await,
    }
}

#[cfg(test)]
mod tests {
    use pinchora_render::ProfileCard;

    use super::*;

    fn wardrobe(stats: PlayerStats, bought: &[&str], equipped: &[&str]) -> Wardrobe {
        let owned = |ids: &[&str]| ids.iter().map(|s| (*s).to_owned()).collect();
        Wardrobe {
            stats,
            bought: owned(bought),
            equipped: owned(equipped),
            chosen: true,
            granted: Vec::new(),
        }
    }

    /// A player who never chose their badges: the card picks.
    fn automatic(stats: PlayerStats, bought: &[&str]) -> Wardrobe {
        Wardrobe {
            chosen: false,
            ..wardrobe(stats, bought, &[])
        }
    }

    fn ids(badges: &[&Badge]) -> Vec<String> {
        badges.iter().map(|b| b.id.clone()).collect()
    }

    #[test]
    fn badges_follow_real_progress() {
        let content = Content::get();
        let stats = PlayerStats {
            level: 7,
            fish_species: 12,
            ..Default::default()
        };
        let w = wardrobe(
            stats,
            &["tiny_tide"],
            &["bright_beacon", "tiny_tide", "little_beacon", "crab_cap"],
        );
        assert_eq!(
            ids(&w.worn()),
            ["tiny_tide", "little_beacon"],
            "level 7 has not reached 10, and the crab cap was never bought; the order stays as saved"
        );
        let wearable = w.wearable();
        assert!(
            wearable.iter().any(|b| b.id == "fin_finder"),
            "12 species earn the 10-species badge"
        );
        assert!(
            !wearable.iter().any(|b| b.id == "reef_regular"),
            "but not the 24-species one"
        );

        let line = |id: &str| w.line(content.badge(id).expect("a badge"), &w.worn());
        assert_eq!(line("bright_beacon"), "🔒 **Bright Beacon** · 7/10 levels");
        assert_eq!(line("reef_regular"), "🔒 **Reef Regular** · 12/24 fish species");
        assert_eq!(line("little_beacon"), "✅ **Little Beacon** · Reach level 5. *(worn)*");
        assert_eq!(
            line("fin_finder"),
            "✅ **Fin Finder** · Catch 10 different fish species."
        );
        assert_eq!(line("tiny_tide"), "✅ **Tiny Tide** · bought *(worn)*");
        assert_eq!(line("crab_cap"), "🛒 **Crab Cap** · 1,500 shells");
    }

    #[test]
    fn a_badge_that_stopped_counting_leaves_the_card() {
        // A seven-day streak badge, saved while the streak was alive.
        let saved = ["week_of_waves"];
        assert_eq!(
            ids(&wardrobe(
                PlayerStats {
                    streak: 9,
                    ..Default::default()
                },
                &[],
                &saved
            )
            .worn()),
            ["week_of_waves"]
        );
        assert!(wardrobe(PlayerStats::default(), &[], &saved).worn().is_empty());
        // Ids the content no longer has are ignored too.
        assert!(wardrobe(PlayerStats::default(), &[], &["gone_badge"]).worn().is_empty());
    }

    #[test]
    fn until_they_choose_the_card_shows_the_best_earned_badges_rarest_first() {
        // Nothing earned, nothing shown.
        assert!(automatic(PlayerStats::default(), &[]).worn().is_empty());

        // Level 25 (rare), 12 species (common), a 7-day streak (common), 60 reputation (uncommon),
        // a bought common badge and a bought legendary one.
        let stats = PlayerStats {
            level: 25,
            fish_species: 12,
            streak: 7,
            reputation: 60,
            ..Default::default()
        };
        let w = automatic(stats, &["tiny_tide", "pearl_wyrm"]);
        assert_eq!(
            ids(&w.worn()),
            [
                "pearl_wyrm",
                "harbour_beacon",
                "bright_beacon",
                "harbour_helper",
                "little_beacon",
                "fin_finder"
            ],
            "legendary, rare, the two uncommon ones in content order, then the first commons"
        );
        assert_eq!(w.worn().len(), MAX_BADGES);

        // Once they chose, only the choice counts, however good the rest is.
        let chosen = wardrobe(stats, &["tiny_tide", "pearl_wyrm"], &["tiny_tide"]);
        assert_eq!(ids(&chosen.worn()), ["tiny_tide"]);
        assert!(
            wardrobe(stats, &[], &[]).worn().is_empty(),
            "an explicit empty choice is no badges"
        );
    }

    #[test]
    fn the_collection_shows_every_badge_in_its_state() {
        let stats = PlayerStats {
            level: 7,
            fish_species: 3,
            ..Default::default()
        };
        let w = wardrobe(stats, &["tiny_tide"], &["tiny_tide", "little_beacon"]);
        let new = fresh_mask(&[
            "little_beacon".to_owned(),
            "tiny_tide".to_owned(),
            "not_a_badge".to_owned(),
        ]);
        let cells = collection_cells(&Sprites::default(), &w, &w.worn(), new);
        let content = Content::get();
        assert_eq!(cells.len(), content.badges.len(), "every badge has a cell");
        let state = |id: &str| {
            let i = content.badges.iter().position(|b| b.id == id).expect("a badge");
            cells[i].state
        };
        assert_eq!(
            state("little_beacon"),
            BadgeState::Earned {
                new: true,
                worn: Some(2)
            },
            "worn second: the order is the saved one"
        );
        assert_eq!(
            state("tiny_tide"),
            BadgeState::Earned {
                new: true,
                worn: Some(1)
            }
        );
        assert_eq!(state("bright_beacon"), BadgeState::Locked { have: 7, at: 10 });
        assert_eq!(state("fin_finder"), BadgeState::Locked { have: 3, at: 10 });
        assert_eq!(state("crab_cap"), BadgeState::ForSale { price: 1_500 });
        // The plate colour is the rarity's.
        let i = content
            .badges
            .iter()
            .position(|b| b.id == "pearl_wyrm")
            .expect("a badge");
        assert_eq!(cells[i].rarity, Rarity::Legendary.color());

        // The NEW markers survive a round trip through the buttons' hex ids.
        assert_eq!(fresh_mask(&[]), 0);
        let hex = format!("{new:x}");
        assert!(format!("cz:ws:1:{hex}").len() <= 100);
        assert_eq!(u128::from_str_radix(&hex, 16).ok(), Some(new));
        assert!(is_fresh(new, 0) && !is_fresh(new, 1));
        assert!(!is_fresh(new, 500), "out of range is never new");
    }

    #[test]
    fn the_badge_list_fits_one_embed_whatever_is_earned() {
        let everything = PlayerStats {
            level: 1_000,
            fish_species: 72,
            house_likes: 1_000_000,
            pets: 99,
            streak: 1_000,
            placed: 1_000_000,
            reputation: 1_000_000,
            shells: 1_000_000_000,
        };
        let bought: Vec<_> = Content::get().badges.iter().map(|b| b.id.as_str()).collect();
        let worn: Vec<_> = bought.iter().copied().take(MAX_BADGES).collect();
        for w in [
            wardrobe(PlayerStats::default(), &[], &[]),
            wardrobe(everything, &bought, &worn),
        ] {
            let fields = badge_fields(&w);
            assert_eq!(fields.len(), BadgeStat::ALL.len() + 1);
            assert!(
                fields.iter().all(|(_, lines)| lines.chars().count() <= 1024),
                "a field value is capped at 1,024"
            );
            let chars: usize = fields
                .iter()
                .map(|(name, lines)| name.chars().count() + lines.chars().count())
                .sum();
            // The title, the description (six badge names and some text) and the fields share Discord's 6,000.
            assert!(chars + 600 < 6_000, "{chars} characters of fields");
        }
    }

    #[test]
    fn custom_ids_and_options_fit_discords_limits() {
        let content = Content::get();
        for b in &content.backgrounds {
            assert!(
                format!("cz:bp:99:{}", b.id).len() <= 100 && b.name.chars().count() <= 100,
                "{}",
                b.id
            );
        }
        for b in &content.badges {
            let option = option(&b.name, &b.id, format!("999,999 shells · {}", b.description), false);
            assert!(b.id.len() <= 100 && option.label.chars().count() <= 100);
            assert!(option.description.is_some_and(|d| d.chars().count() <= 100));
        }
    }

    #[test]
    fn the_card_has_room_for_what_the_database_allows() {
        assert_eq!(MAX_BADGES, ProfileCard::MAX_BADGES);
    }

    /// A player row with the defaults every new player has.
    fn player() -> UserRow {
        UserRow {
            id: 1,
            credits: 0,
            xp: 0,
            reputation: 0,
            daily_streak: 0,
            last_daily_at: None,
            last_rep_at: None,
            bio: String::new(),
            accent_color: None,
            created_at: chrono::Utc::now(),
            pearls: 0,
            character: None,
            background: None,
            badges: Vec::new(),
            badges_set: false,
            badges_seen: None,
            title: None,
            accent: None,
            panel_opacity: 1,
            frame: "plain".to_owned(),
            showcase_fish: None,
            showcase_pet: None,
            showcase_item: None,
            showcase_house: false,
        }
    }

    #[test]
    fn the_accent_follows_the_background_unless_the_player_chose_one() {
        let content = Content::get();
        let background = content.background("shellwash_morning").expect("a background");
        let mut row = player();
        row.accent_color = Some(0x112233);
        assert_eq!(
            theme(&row, Some(background)).accent,
            background.accent,
            "the picture wins over the colour from before accents were a choice"
        );
        assert_eq!(theme(&row, None).accent, "#112233", "else that colour");
        assert_eq!(theme(&player(), None).accent, Theme::reef().accent);

        // A chosen accent beats the background; Auto (NULL) gives the background back.
        row.accent = Some("#06d6a0".to_owned());
        assert_eq!(theme(&row, Some(background)).accent, "#06d6a0");
        assert_eq!(theme(&row, None).accent, "#06d6a0");
        // A value the card could not show is ignored, as if the player had never chosen.
        row.accent = Some("#000000".to_owned());
        assert_eq!(theme(&row, Some(background)).accent, background.accent);
        row.accent = Some("nonsense".to_owned());
        assert_eq!(theme(&row, None).accent, "#112233");
    }

    #[test]
    fn frames_and_opacity_come_from_the_row_and_frames_need_their_level() {
        let mut row = player();
        assert_eq!(theme(&row, None).frame, Frame::Plain);
        assert_eq!(theme(&row, None).opacity, Opacity::Normal);
        row.panel_opacity = 0;
        assert_eq!(theme(&row, None).opacity, Opacity::Light);
        row.panel_opacity = 9;
        assert_eq!(theme(&row, None).opacity, Opacity::Normal, "an unknown value is Normal");

        row.frame = "pearl".to_owned();
        assert_eq!(frame_of(&row), Frame::Plain, "level 1 has not unlocked the pearl frame");
        row.xp = 30_000; // level 20 takes 23,850 XP in all
        assert!(leveling::level_of(row.xp) >= 20);
        assert_eq!(frame_of(&row), Frame::Pearl);
        row.frame = "rope".to_owned();
        row.xp = 0;
        assert_eq!(frame_of(&row), Frame::Rope, "the rope frame is free");
        row.frame = "no_such_frame".to_owned();
        assert_eq!(frame_of(&row), Frame::Plain);
    }
}
