use std::{collections::HashSet, str::Split, sync::Arc};

use pinchora_db::{CustomizeBuy, MAX_BADGES, PlayerStats};
use pinchora_game::{Background, Badge, BadgeKind, BadgeStat, Content};
use pinchora_render::{Backdrop, BadgeIcon, SpriteArt, Theme, sprites::placeholder_badge_color};
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

/// Discord shows at most this many options in one select menu.
const PER_PAGE: usize = 25;

#[derive(CommandModel, CreateCommand)]
#[command(name = "customize", desc = "Backgrounds and badges for your cards")]
pub enum CustomizeCommand {
    #[command(name = "background")]
    Background(CustomizeBackground),
    #[command(name = "badges")]
    Badges(CustomizeBadges),
}

#[derive(CommandModel, CreateCommand)]
#[command(
    name = "background",
    desc = "Buy and wear a background for your profile, rank and wallet cards"
)]
pub struct CustomizeBackground;

#[derive(CommandModel, CreateCommand)]
#[command(name = "badges", desc = "Earn, buy and wear the badges on your profile card")]
pub struct CustomizeBadges;

impl CustomizeCommand {
    pub async fn run(self, cx: &Cx) -> anyhow::Result<()> {
        let data = match self {
            Self::Background(_) => background_panel(cx, Highlight::Equipped, None, "").await?,
            Self::Badges(_) => badges_panel(cx, 0, "").await?,
        };
        cx.reply(data).await
    }
}

// /Cards

/// A player's background and badges as the cards draw them. Owned, so it can move to the render thread.
pub(crate) struct Cosmetics {
    /// The art (`None` until it is drawn) and the accent colour.
    background: Option<(Option<Arc<ScaledArt>>, String)>,
    /// Each badge's art, and the letter and colour of its placeholder.
    badges: Vec<(Option<Arc<ScaledArt>>, char, u32)>,
}

impl Cosmetics {
    pub(crate) fn new(sprites: &Sprites, background: Option<&Background>, badges: &[&Badge]) -> Self {
        Self {
            background: background.map(|b| (sprites.background(&b.id), b.accent.clone())),
            badges: badges
                .iter()
                .map(|b| {
                    let letter = b
                        .name
                        .chars()
                        .next()
                        .map_or('?', |c| c.to_uppercase().next().unwrap_or(c));
                    (sprites.badge(&b.id), letter, placeholder_badge_color(&b.id))
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
            .map(|(art, letter, color)| BadgeIcon {
                art: art.as_ref().map(|a| SpriteArt {
                    png: &a.png,
                    w: a.w,
                    h: a.h,
                }),
                letter: *letter,
                color: *color,
            })
            .collect()
    }
}

/// The card palette: the background's accent colour when there is a background, else the
/// colour the player picked, else the reef coral.
pub(crate) fn theme(accent_color: Option<i32>, background: Option<&Background>) -> Theme {
    Theme::reef().with_accent(
        background
            .and_then(Background::accent_rgb)
            .or(accent_color.map(|c| c as u32)),
    )
}

// /Badges

fn stat_value(stats: &PlayerStats, stat: BadgeStat) -> i64 {
    match stat {
        BadgeStat::Level => stats.level,
        BadgeStat::FishSpecies => stats.fish_species,
        BadgeStat::HouseLikes => stats.house_likes,
        BadgeStat::Pets => stats.pets,
        BadgeStat::Streak => stats.streak,
        BadgeStat::Placed => stats.placed,
        BadgeStat::Reputation => stats.reputation,
        BadgeStat::Shells => stats.shells,
    }
}

/// What a player has earned and bought, and which badges they have put on.
struct Wardrobe {
    stats: PlayerStats,
    /// Shop badges they bought.
    bought: Vec<String>,
    /// The ids saved as worn, in order.
    equipped: Vec<String>,
}

impl Wardrobe {
    async fn load(app: &App, user: u64, equipped: &[String]) -> anyhow::Result<Self> {
        Ok(Self {
            stats: app.db.player_stats(user).await?,
            bought: app.db.owned_badges(user).await?,
            equipped: equipped.to_vec(),
        })
    }

    /// Whether the badge is theirs: reached for an achievement, bought for a shop badge.
    fn has(&self, badge: &Badge) -> bool {
        match badge.kind {
            BadgeKind::Achievement { stat, at } => stat_value(&self.stats, stat) >= at,
            BadgeKind::Shop { .. } => self.bought.contains(&badge.id),
        }
    }

    /// Every badge they could put on now, in the order of `content/badges.json`.
    fn wearable(&self) -> Vec<&'static Badge> {
        Content::get().badges.iter().filter(|b| self.has(b)).collect()
    }

    /// The saved badges they may still wear: one that stopped counting (a streak that
    /// ended, shells spent) drops out until it is earned again.
    fn worn(&self) -> Vec<&'static Badge> {
        let content = Content::get();
        self.equipped
            .iter()
            .filter_map(|id| content.badge(id))
            .filter(|b| self.has(b))
            .take(MAX_BADGES)
            .collect()
    }

    /// `✅ Name · Reach level 5.` once it is theirs, `🔒 Name · 3/5 levels` before.
    fn line(&self, badge: &Badge) -> String {
        let worn = if self.equipped.contains(&badge.id) && self.has(badge) {
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
}

/// The badges a profile card shows for `user`: the ones saved as worn that they still may wear.
pub(crate) async fn worn_badges(app: &App, user: u64, equipped: &[String]) -> anyhow::Result<Vec<&'static Badge>> {
    if equipped.is_empty() {
        return Ok(Vec::new());
    }
    Ok(Wardrobe::load(app, user, equipped).await?.worn())
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

    let worn = worn_badges(&cx.app, id, &player.badges).await?;
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
    let mut fields: Vec<_> = BadgeStat::ALL
        .into_iter()
        .map(|stat| {
            let lines: Vec<_> = badges
                .iter()
                .filter(|b| matches!(b.kind, BadgeKind::Achievement { stat: s, .. } if s == stat))
                .map(|b| wardrobe.line(b))
                .collect();
            (stat.label(), lines.join("\n"))
        })
        .collect();
    let shop: Vec<_> = badges
        .iter()
        .filter(|b| matches!(b.kind, BadgeKind::Shop { .. }))
        .map(|b| wardrobe.line(b))
        .collect();
    fields.push(("Shop", shop.join("\n")));
    fields.retain(|(_, lines)| !lines.is_empty());
    fields
}

/// The ephemeral badge editor: the profile card as it is now, every badge with its progress or
/// price, a menu to put up to six on, a menu to buy shop badges and a button to take them all off.
async fn badges_panel(cx: &Cx, page: usize, note: &str) -> anyhow::Result<InteractionResponseData> {
    let content = Content::get();
    let user = cx.author()?;
    let id = user.id.get();
    let player = cx.app.db.user(id).await?;
    let wardrobe = Wardrobe::load(&cx.app, id, &player.badges).await?;
    let worn = wardrobe.worn();
    let wearable = wardrobe.wearable();
    let pages = wearable.len().div_ceil(PER_PAGE).max(1);
    let page = page.min(pages - 1);

    let wear_options = wearable
        .iter()
        .skip(page * PER_PAGE)
        .take(PER_PAGE)
        .map(|b| option(&b.name, &b.id, b.description.clone(), worn.iter().any(|w| w.id == b.id)))
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
    let (credits, equipped) = (player.credits, player.badges.len());
    let png = Box::pin(profile::card(
        cx,
        user,
        cx.author_display_name(),
        player,
        background,
        &worn,
    ))
    .await?;

    let mut text = String::new();
    if !note.is_empty() {
        text.push_str(note);
        text.push_str("\n\n");
    }
    if worn.is_empty() {
        text.push_str("You are not wearing any badges.");
    } else {
        let names: Vec<_> = worn.iter().map(|b| format!("**{}**", b.name)).collect();
        text.push_str(&format!("Wearing {}/{MAX_BADGES}: {}.", worn.len(), names.join(", ")));
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
            format!("cz:wp:{}", page.saturating_sub(1)),
            "◀ Page",
            ButtonStyle::Secondary,
            page == 0,
        ));
        buttons.push(button(
            format!("cz:wp:{}", page + 1),
            "Page ▶",
            ButtonStyle::Secondary,
            page + 1 >= pages,
        ));
    }
    buttons.push(button(
        "cz:wc".to_owned(),
        "Take all off",
        ButtonStyle::Danger,
        equipped == 0,
    ));
    let components = vec![
        row(vec![menu(
            format!("cz:ws:{page}"),
            "Badges to wear…",
            wear_options,
            0,
            MAX_BADGES as u8,
            "Nothing to wear yet",
        )]),
        row(vec![menu(
            "cz:wb".to_owned(),
            "Buy a shop badge…",
            buy_options,
            1,
            1,
            "Nothing to buy",
        )]),
        row(buttons),
    ];
    Ok(Cx::card(embed, "profile.png", png, components, true))
}

/// `cz:ws:<page>`: the menu's picks replace what was worn from that page; badges worn from other pages stay.
async fn wear(cx: &Cx, page: usize, values: &[String]) -> anyhow::Result<()> {
    let id = cx.author()?.id.get();
    let saved = cx.app.db.user(id).await?.badges;
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
    let data = badges_panel(cx, page, &note).await?;
    cx.update(data).await
}

async fn buy_badge(cx: &Cx, id: &str) -> anyhow::Result<()> {
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
    let data = badges_panel(cx, 0, &note).await?;
    cx.update(data).await
}

/// `cz:<what>:...` selects and buttons of both editors.
pub(crate) async fn on_component(cx: &Cx, mut args: Split<'_, char>, values: &[String]) -> anyhow::Result<()> {
    let content = Content::get();
    let value = values.first().map(String::as_str);
    let page = |arg: Option<&str>| arg.and_then(|p| p.parse().ok()).unwrap_or(0);
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
        Some("ws") => wear(cx, page(args.next()), values).await,
        Some("wb") => match value.filter(|v| *v != "none") {
            Some(id) => buy_badge(cx, id).await,
            None => expired(cx, "badges").await,
        },
        Some("wp") => {
            let data = badges_panel(cx, page(args.next()), "").await?;
            cx.update(data).await
        }
        Some("wc") => {
            cx.app.db.set_badges(cx.author()?.id.get(), &[]).await?;
            let data = badges_panel(cx, 0, "Took every badge off.").await?;
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

        let line = |id: &str| w.line(content.badge(id).expect("a badge"));
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

    #[test]
    fn the_accent_follows_the_background() {
        let content = Content::get();
        let background = content.background("shellwash_morning").expect("a background");
        let mine = Some(0x112233);
        assert_eq!(
            theme(mine, Some(background)).accent,
            background.accent,
            "the picture wins"
        );
        assert_eq!(theme(mine, None).accent, "#112233", "else the colour the player picked");
        assert_eq!(theme(None, None).accent, Theme::reef().accent);
    }
}
