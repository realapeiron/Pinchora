use super::*;

/// One embed field per thing achievements count, then the shop: a line for every badge.
pub(super) fn badge_fields(wardrobe: &Wardrobe, lang: Lang) -> Vec<(String, String)> {
    let badges = &Content::get().badges;
    let worn = wardrobe.worn();
    let mut fields: Vec<_> = BadgeStat::ALL
        .into_iter()
        .map(|stat| {
            let lines: Vec<_> = badges
                .iter()
                .filter(|b| matches!(b.kind, BadgeKind::Achievement { stat: s, .. } if s == stat))
                .map(|b| wardrobe.line(b, &worn, lang))
                .collect();
            (stat.label_in(lang).to_owned(), lines.join("\n"))
        })
        .collect();
    let shop: Vec<_> = badges
        .iter()
        .filter(|b| matches!(b.kind, BadgeKind::Shop { .. }))
        .map(|b| wardrobe.line(b, &worn, lang))
        .collect();
    fields.push((t!(lang, "customize.badges.shop"), shop.join("\n")));
    fields.retain(|(_, lines)| !lines.is_empty());
    fields
}

/// The badges earned since the player last opened their collection, as a bit per badge of
/// `content/badges.json` (the first 128). The panel's buttons carry it, so its NEW markers
/// stay while the player works in it.
pub(super) fn fresh_mask(fresh: &[String]) -> u128 {
    Content::get()
        .badges
        .iter()
        .take(128)
        .enumerate()
        .filter(|(_, b)| fresh.contains(&b.id))
        .fold(0, |mask, (i, _)| mask | 1 << i)
}

/// Whether badge number `index` of `content/badges.json` is in `mask`.
pub(super) fn is_fresh(mask: u128, index: usize) -> bool {
    index < 128 && mask >> index & 1 == 1
}

/// Which badges the collection marks NEW.
#[derive(Clone, Copy)]
pub(super) enum Fresh {
    /// The player just opened the collection: whatever they earned since last time is new, and
    /// from now on it has been seen.
    Open,
    /// A click in an open panel: the markers its buttons carry.
    Keep(u128),
}

/// A badge of the collection, owned so the card can be drawn on the render thread.
pub(super) struct CollectionCell {
    art: Option<Arc<ScaledArt>>,
    letter: char,
    color: u32,
    pub(super) rarity: u32,
    name: Cow<'static, str>,
    rarity_label: &'static str,
    pub(super) state: BadgeState,
}

/// Every badge in the order of `content/badges.json`, as the collection card shows it.
pub(super) fn collection_cells(
    sprites: &Sprites,
    wardrobe: &Wardrobe,
    worn: &[&Badge],
    fresh: u128,
    lang: Lang,
) -> Vec<CollectionCell> {
    let content = Content::get();
    content
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
                BadgeKind::Granted => BadgeState::Locked { have: 0, at: 1 },
            };
            CollectionCell {
                art: sprites.badge(&b.id),
                letter: letter_of(b),
                color: placeholder_badge_color(&b.id),
                rarity: b.rarity.color(),
                name: content.name(&b.id, lang),
                rarity_label: b.rarity.label_in(lang),
                state,
            }
        })
        .collect()
}

/// The ephemeral badge editor: the collection card (every badge, earned in colour, locked ones
/// dark with their progress, NEW on the latest), what the profile card shows, a menu to put up to
/// six on, a menu to buy shop badges, and buttons to take them off or let the card pick.
pub(super) async fn badges_panel(
    cx: &Cx,
    page: usize,
    note: &str,
    fresh: Fresh,
) -> anyhow::Result<InteractionResponseData> {
    let lang = cx.lang();
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

    let (wear_options, buy_options) = badge_options(&wardrobe, &wearable, &worn, page, lang);
    let background = player.background.as_deref().and_then(|e| content.background(e));
    let (credits, saved_empty) = (player.credits, player.badges.is_empty());
    let theme = theme(&player, background);
    let cells = collection_cells(&cx.app.sprites, &wardrobe, &worn, mask, lang);
    let png = badge_image(&cx.app, cells, theme, lang).await?;
    let embed = badge_embed(&wardrobe, &worn, credits, note, lang);
    let components = badge_controls(
        &wardrobe,
        BadgeMenus {
            page,
            pages,
            mask,
            saved_empty,
            wear_options,
            buy_options,
        },
        lang,
    );
    Ok(Cx::card(embed, "badges.png", png, components, true))
}

/// `cz:ws:<page>:<new>`: the menu's picks replace what was worn from that page; badges worn from other pages stay.
pub(super) async fn wear(cx: &Cx, page: usize, fresh: u128, values: &[String]) -> anyhow::Result<()> {
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
        t!(cx.lang(), "customize.badges.too-many", max = MAX_BADGES)
    } else {
        cx.app.db.set_badges(id, &next).await?;
        t!(cx.lang(), "customize.saved")
    };
    let data = badges_panel(cx, page, &note, Fresh::Keep(fresh)).await?;
    cx.update(data).await
}

pub(super) async fn buy_badge(cx: &Cx, id: &str, fresh: u128) -> anyhow::Result<()> {
    let Some(badge) = Content::get().badge(id) else {
        return expired(cx, "badges").await;
    };
    let BadgeKind::Shop { price } = badge.kind else {
        return cx.reply_ephemeral(t!(cx.lang(), "customize.badges.not-sold")).await;
    };
    let note = match cx.app.db.buy_badge(cx.author()?.id.get(), &badge.id, price).await? {
        CustomizeBuy::Bought { balance } => t!(
            cx.lang(),
            "customize.badges.bought",
            item = Content::get().name(&badge.id, cx.lang()),
            price = thousands_in(cx.lang(), price),
            balance = thousands_in(cx.lang(), balance)
        ),
        CustomizeBuy::AlreadyOwned => t!(
            cx.lang(),
            "customize.already-own",
            item = Content::get().name(&badge.id, cx.lang())
        ),
        CustomizeBuy::Insufficient { balance } => t!(
            cx.lang(),
            "customize.costs",
            item = Content::get().name(&badge.id, cx.lang()),
            price = thousands_in(cx.lang(), price),
            balance = thousands_in(cx.lang(), balance)
        ),
    };
    let data = badges_panel(cx, 0, &note, Fresh::Keep(fresh)).await?;
    cx.update(data).await
}

fn badge_options(
    wardrobe: &Wardrobe,
    wearable: &[&Badge],
    worn: &[&Badge],
    page: usize,
    lang: Lang,
) -> (Vec<SelectMenuOption>, Vec<SelectMenuOption>) {
    let content = Content::get();
    let wear_options = wearable
        .iter()
        .skip(page * PER_PAGE)
        .take(PER_PAGE)
        .map(|b| {
            option(
                &content.name(&b.id, lang),
                &b.id,
                format!(
                    "{} · {}",
                    b.rarity.label_in(lang),
                    content.description(&b.id, lang).unwrap_or_default()
                ),
                worn.iter().any(|w| w.id == b.id),
            )
        })
        .collect();
    let buy_options = content
        .badges
        .iter()
        .filter_map(|b| match b.kind {
            BadgeKind::Shop { price } if !wardrobe.has(b) => Some(option(
                &content.name(&b.id, lang),
                &b.id,
                t!(
                    lang,
                    "customize.badges.buy-option",
                    price = thousands_in(lang, price),
                    description = content.description(&b.id, lang).unwrap_or_default()
                ),
                false,
            )),
            _ => None,
        })
        .collect();

    (wear_options, buy_options)
}

async fn badge_image(app: &App, cells: Vec<CollectionCell>, theme: Theme, lang: Lang) -> anyhow::Result<Vec<u8>> {
    let png = app
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
                    name: &c.name,
                    rarity: c.rarity_label,
                    state: c.state,
                })
                .collect();
            let legend: Vec<(&str, u32)> = Rarity::ALL.iter().map(|r| (r.label_in(lang), r.color())).collect();
            r.badge_collection(&BadgeCollectionCard {
                lang,
                cells: &boxed,
                legend: &legend,
                theme,
            })
        })
        .await?;

    Ok(png)
}

fn badge_embed(wardrobe: &Wardrobe, worn: &[&Badge], credits: i64, note: &str, lang: Lang) -> EmbedBuilder {
    let content = Content::get();
    let mut text = String::new();
    if !note.is_empty() {
        text.push_str(note);
        text.push_str("\n\n");
    }
    let names: Vec<_> = worn
        .iter()
        .map(|b| format!("**{}**", content.name(&b.id, lang)))
        .collect();
    let (count, names) = (worn.len(), names.join(", "));
    text.push_str(&if worn.is_empty() && wardrobe.chosen {
        t!(lang, "customize.badges.none-worn")
    } else if worn.is_empty() {
        t!(lang, "customize.badges.none-earned")
    } else if wardrobe.chosen {
        t!(
            lang,
            "customize.badges.wearing",
            count = count,
            max = MAX_BADGES,
            names = names
        )
    } else {
        t!(
            lang,
            "customize.badges.auto",
            count = count,
            max = MAX_BADGES,
            names = names
        )
    });
    text.push_str(&t!(lang, "customize.badges.pick-up-to", max = MAX_BADGES));
    text.push('\n');
    text.push_str(&t!(lang, "customize.balance", balance = thousands_in(lang, credits)));
    let mut embed = EmbedBuilder::new()
        .color(BRAND)
        .title(t!(lang, "customize.badges.title"))
        .description(text);
    for (name, lines) in badge_fields(wardrobe, lang) {
        embed = embed.field(EmbedFieldBuilder::new(name, lines));
    }

    embed
}

struct BadgeMenus {
    page: usize,
    pages: usize,
    mask: u128,
    saved_empty: bool,
    wear_options: Vec<SelectMenuOption>,
    buy_options: Vec<SelectMenuOption>,
}

fn badge_controls(wardrobe: &Wardrobe, menus: BadgeMenus, lang: Lang) -> Vec<Component> {
    let BadgeMenus {
        page,
        pages,
        mask,
        saved_empty,
        wear_options,
        buy_options,
    } = menus;
    let mut buttons = Vec::new();
    if pages > 1 {
        buttons.push(button(
            format!("cz:wp:{}:{mask:x}", page.saturating_sub(1)),
            &t!(lang, "customize.prev-page"),
            ButtonStyle::Secondary,
            page == 0,
        ));
        buttons.push(button(
            format!("cz:wp:{}:{mask:x}", page + 1),
            &t!(lang, "customize.next-page"),
            ButtonStyle::Secondary,
            page + 1 >= pages,
        ));
    }
    buttons.push(button(
        format!("cz:wc:{mask:x}"),
        &t!(lang, "customize.badges.take-all-off"),
        ButtonStyle::Danger,
        wardrobe.chosen && saved_empty,
    ));
    buttons.push(button(
        format!("cz:wa:{mask:x}"),
        &t!(lang, "customize.badges.auto-button"),
        ButtonStyle::Secondary,
        !wardrobe.chosen,
    ));
    let components = vec![
        row(vec![menu(
            format!("cz:ws:{page}:{mask:x}"),
            &t!(lang, "customize.badges.wear-menu"),
            wear_options,
            0,
            MAX_BADGES as u8,
            &t!(lang, "customize.badges.nothing-to-wear"),
        )]),
        row(vec![menu(
            format!("cz:wb:{mask:x}"),
            &t!(lang, "customize.badges.buy-menu"),
            buy_options,
            1,
            1,
            &t!(lang, "customize.badges.nothing-to-buy"),
        )]),
        row(buttons),
    ];
    components
}
