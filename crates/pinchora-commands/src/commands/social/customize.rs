use std::{borrow::Cow, collections::HashSet, str::Split, sync::Arc};

use pinchora_core::admin::stats::PlayerStats;
use pinchora_core::infra::i18n::Lang;
use pinchora_core::progression::leveling;
use pinchora_core::t;
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

use crate::App;
use crate::Cx;
use crate::commands::social::profile;
use crate::games::game::scene::ScaledArt;
use crate::games::game::scene::Sprites;
use crate::infra::util::BRAND;
use crate::infra::util::thousands_in;

mod badges;
use badges::{Fresh, badges_panel, buy_badge, wear};
#[cfg(test)]
use badges::{badge_fields, collection_cells, fresh_mask, is_fresh};
mod showcase;
mod style;
mod titles;
mod unlocks;

pub(crate) use crate::commands::social::customize::showcase::Showcase;
pub(crate) use crate::commands::social::customize::showcase::on_showcase;
pub(crate) use crate::commands::social::customize::style::on_style;
pub(crate) use crate::commands::social::customize::style::on_style_modal;
pub(crate) use crate::commands::social::customize::titles::on_title;
pub(crate) use crate::commands::social::customize::unlocks::announce;
pub(crate) use crate::commands::social::customize::unlocks::announce_in;

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
            Self::Title(_) => crate::commands::social::customize::titles::panel(cx, 0, "").await?,
            Self::Style(_) => crate::commands::social::customize::style::panel(cx, "").await?,
            Self::Showcase(_) => {
                crate::commands::social::customize::showcase::panel(
                    cx,
                    crate::commands::social::customize::showcase::Slot::Fish,
                    0,
                    "",
                )
                .await?
            }
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
    fn line(&self, badge: &Badge, worn: &[&Badge], lang: Lang) -> String {
        let content = Content::get();
        let name = content.name(&badge.id, lang);
        let worn = if worn.iter().any(|w| w.id == badge.id) {
            t!(lang, "customize.line.worn")
        } else {
            String::new()
        };
        match badge.kind {
            BadgeKind::Achievement { stat, at } => {
                let have = stat_value(&self.stats, stat);
                if have >= at {
                    format!(
                        "✅ **{name}** · {}{worn}",
                        content.description(&badge.id, lang).unwrap_or_default()
                    )
                } else {
                    format!(
                        "🔒 **{name}** · {}/{} {}",
                        thousands_in(lang, have.max(0)),
                        thousands_in(lang, at),
                        stat.noun_in(lang)
                    )
                }
            }
            BadgeKind::Shop { .. } if self.has(badge) => {
                format!("✅ **{name}** · {}{worn}", t!(lang, "customize.line.bought"))
            }
            BadgeKind::Shop { price } => format!(
                "🛒 **{name}** · {}",
                t!(lang, "customize.line.price", price = thousands_in(lang, price))
            ),
            BadgeKind::Granted if self.has(badge) => format!(
                "✅ **{name}** · {}{worn}",
                content.description(&badge.id, lang).unwrap_or_default()
            ),
            BadgeKind::Granted => format!(
                "🔒 **{name}** · {}",
                content.description(&badge.id, lang).unwrap_or_default()
            ),
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
    fn title_line(&self, title: &pinchora_game::Title, equipped: Option<&str>, lang: Lang) -> String {
        let content = Content::get();
        let name = content.name(&title.id, lang);
        let description = content.description(&title.id, lang).unwrap_or_default();
        let worn = if equipped == Some(title.id.as_str()) {
            t!(lang, "customize.line.worn")
        } else {
            String::new()
        };
        if self.has_title(title) {
            return format!("✅ **{name}** · {description}{worn}");
        }
        match &title.kind {
            TitleKind::Stat { stat, at } => format!(
                "🔒 **{name}** · {}/{} {}",
                thousands_in(lang, stat_value(&self.stats, *stat).max(0)),
                thousands_in(lang, *at),
                stat.noun_in(lang)
            ),
            _ => format!("🔒 **{name}** · {description}"),
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
pub(crate) fn title_name(row: &UserRow, lang: Lang) -> Option<Cow<'static, str>> {
    let content = Content::get();
    row.title
        .as_deref()
        .and_then(|id| content.title(id))
        .map(|t| content.name(&t.id, lang))
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
    let lang = cx.lang();
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
                t!(lang, "customize.equipped")
            } else if is_owned(b) {
                t!(lang, "customize.owned")
            } else {
                t!(lang, "customize.price", price = thousands_in(cx.lang(), b.price))
            };
            option(
                &content.name(&b.id, lang),
                &b.id,
                format!("{} · {status}", b.rarity.label_in(lang)),
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
                t!(lang, "customize.worn-now")
            } else if is_owned(b) {
                t!(lang, "customize.owned-not-worn")
            } else {
                t!(lang, "customize.price", price = thousands_in(cx.lang(), b.price))
            };
            text.push_str(&format!(
                "**{}** · {} · {status}",
                content.name(&b.id, lang),
                b.rarity.label_in(lang)
            ));
        }
        None => text.push_str(&t!(lang, "customize.background.pick-hint")),
    }
    text.push('\n');
    text.push_str(&t!(
        lang,
        "customize.balance",
        balance = thousands_in(cx.lang(), credits)
    ));

    let hid = id_part(shown);
    let can_buy = shown.is_some_and(|b| !is_owned(b));
    let can_equip = shown.is_some_and(|b| is_owned(b) && !is_equipped(b));
    let components = vec![
        row(vec![menu(
            "cz:bs".to_owned(),
            &t!(lang, "customize.background.pick"),
            options,
            1,
            1,
            &t!(lang, "customize.nothing-to-show"),
        )]),
        row(vec![
            button(
                format!("cz:bp:{}:{hid}", page.saturating_sub(1)),
                &t!(lang, "customize.prev-page"),
                ButtonStyle::Secondary,
                page == 0,
            ),
            button(
                format!("cz:bp:{}:{hid}", page + 1),
                &t!(lang, "customize.next-page"),
                ButtonStyle::Secondary,
                page + 1 >= pages,
            ),
            button(
                format!("cz:bb:{hid}"),
                &t!(lang, "customize.buy"),
                ButtonStyle::Success,
                !can_buy,
            ),
            button(
                format!("cz:be:{hid}"),
                &t!(lang, "customize.equip"),
                ButtonStyle::Primary,
                !can_equip,
            ),
            button(
                "cz:br".to_owned(),
                &t!(lang, "customize.remove"),
                ButtonStyle::Danger,
                equipped.is_none(),
            ),
        ]),
    ];
    let embed = EmbedBuilder::new()
        .color(BRAND)
        .title(t!(lang, "customize.background.title"))
        .description(text);
    Ok(Cx::card(embed, "profile.png", png, components, true))
}

async fn expired(cx: &Cx, command: &str) -> anyhow::Result<()> {
    cx.reply_ephemeral(t!(cx.lang(), "customize.expired", command = command))
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
        CustomizeBuy::Bought { balance } => t!(
            cx.lang(),
            "customize.background.bought",
            item = Content::get().name(&background.id, cx.lang()),
            price = thousands_in(cx.lang(), background.price),
            balance = thousands_in(cx.lang(), balance)
        ),
        CustomizeBuy::AlreadyOwned => t!(
            cx.lang(),
            "customize.already-own",
            item = Content::get().name(&background.id, cx.lang())
        ),
        CustomizeBuy::Insufficient { balance } => t!(
            cx.lang(),
            "customize.costs",
            item = Content::get().name(&background.id, cx.lang()),
            price = thousands_in(cx.lang(), background.price),
            balance = thousands_in(cx.lang(), balance)
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
        t!(
            cx.lang(),
            "customize.background.equipped",
            item = Content::get().name(&background.id, cx.lang())
        )
    } else {
        t!(
            cx.lang(),
            "customize.background.not-owned",
            item = Content::get().name(&background.id, cx.lang())
        )
    };
    let data = background_panel(cx, Highlight::Id(&background.id), None, &note).await?;
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
            let note = t!(cx.lang(), "customize.background.removed");
            let data = background_panel(cx, Highlight::Nothing, None, &note).await?;
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
            let note = t!(cx.lang(), "customize.badges.took-off");
            let data = badges_panel(cx, 0, &note, Fresh::Keep(fresh(args.next()))).await?;
            cx.update(data).await
        }
        Some("wa") => {
            cx.app.db.reset_badges(cx.author()?.id.get()).await?;
            let note = t!(cx.lang(), "customize.badges.auto-done");
            let data = badges_panel(cx, 0, &note, Fresh::Keep(fresh(args.next()))).await?;
            cx.update(data).await
        }
        _ => cx.reply_ephemeral(t!(cx.lang(), "privacy.expired")).await,
    }
}

#[cfg(test)]
mod tests;
