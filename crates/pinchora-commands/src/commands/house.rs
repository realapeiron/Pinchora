use std::str::Split;

use chrono::Utc;
use pinchora_db::{HouseRow, NewRoom, PlaceOutcome, PlacementRow, RoomRow, UpgradeOutcome, VisitOutcome};
use pinchora_game::{
    Content, Layer,
    house::{self, Placed, ROOM_ORDER},
};
use pinchora_render::room::Cursor;
use twilight_interactions::command::{CommandModel, CreateCommand, ResolvedUser};
use twilight_model::{
    channel::message::{
        Component, MessageFlags,
        component::{ActionRow, Button, ButtonStyle, SelectMenu, SelectMenuOption, SelectMenuType},
    },
    http::interaction::InteractionResponseData,
    user::User,
};
use twilight_util::builder::embed::{EmbedBuilder, EmbedFooterBuilder};

use crate::{
    App, Cx,
    game::scene::{RoomView, render_view},
    util::{BRAND, SUCCESS, WARN, resolved_display_name, thousands},
};

/// Shells a guest earns for the first visit to a house each day.
const VISIT_REWARD: i64 = 25;
/// Paid visits per guest per day.
const VISIT_CAP: i64 = 10;
/// Inventory entries per select-menu page (Discord's limit).
const PAGE: usize = 25;

#[derive(CommandModel, CreateCommand)]
#[command(name = "house", desc = "Your pixel house")]
#[allow(clippy::large_enum_variant)] // parsed once per interaction, never stored
pub enum HouseCommand {
    #[command(name = "view")]
    View(HouseView),
    #[command(name = "visit")]
    Visit(HouseVisit),
    #[command(name = "edit")]
    Edit(HouseEdit),
    #[command(name = "upgrade")]
    Upgrade(HouseUpgrade),
    #[command(name = "lock")]
    Lock(HouseLock),
    #[command(name = "like")]
    Like(HouseLike),
}

#[derive(CommandModel, CreateCommand)]
#[command(name = "like", desc = "Leave a like on someone's house (once a day)")]
pub struct HouseLike {
    /// Whose house you like
    user: ResolvedUser,
}

#[derive(CommandModel, CreateCommand)]
#[command(name = "view", desc = "Look at your house")]
pub struct HouseView {
    /// Room number (default: 1)
    #[command(min_value = 1, max_value = 6)]
    room: Option<i64>,
}

#[derive(CommandModel, CreateCommand)]
#[command(name = "visit", desc = "Visit someone's house (first visit each day pays shells)")]
pub struct HouseVisit {
    /// Whose house
    user: ResolvedUser,
    /// Room number (default: 1)
    #[command(min_value = 1, max_value = 6)]
    room: Option<i64>,
}

#[derive(CommandModel, CreateCommand)]
#[command(name = "edit", desc = "Decorate a room: move the cursor, pick furniture, place it")]
pub struct HouseEdit {
    /// Room number (default: 1)
    #[command(min_value = 1, max_value = 6)]
    room: Option<i64>,
}

#[derive(CommandModel, CreateCommand)]
#[command(name = "upgrade", desc = "Move up to a bigger house (your furniture stays)")]
pub struct HouseUpgrade;

#[derive(CommandModel, CreateCommand)]
#[command(name = "lock", desc = "Lock or unlock your house for visitors")]
pub struct HouseLock {
    /// True stops visits
    locked: bool,
}

impl HouseCommand {
    pub async fn run(self, cx: &Cx) -> anyhow::Result<()> {
        let author = cx.author()?.clone();
        match self {
            Self::View(c) => {
                let data = room_card(cx, &author, &cx.author_display_name(), room_index(c.room), None, None).await?;
                cx.reply(data).await
            }
            Self::Visit(c) => visit(cx, &author, &c.user, room_index(c.room)).await,
            Self::Edit(c) => {
                let (_, rooms) = ensure_house(&cx.app, author.id.get()).await?;
                let room = room_index(c.room).min(rooms.len().saturating_sub(1) as i16);
                let state = EditState {
                    room,
                    x: 1,
                    y: house::WALL_ROWS,
                    page: 0,
                    item: None,
                };
                let note = "Pick furniture from the menu, move with the arrows, then press **Place**.";
                let data = editor(cx, author.id.get(), state, note).await?;
                cx.reply(data).await
            }
            Self::Upgrade(_) => upgrade_offer(cx, author.id.get()).await,
            Self::Like(c) => {
                let host = &c.user.resolved;
                if host.bot || host.id == author.id {
                    return cx.reply_ephemeral("Like someone else's house!").await;
                }
                ensure_house(&cx.app, host.id.get()).await?;
                match cx
                    .app
                    .db
                    .like_house(host.id.get(), author.id.get(), Utc::now().date_naive())
                    .await?
                {
                    Some(total) => {
                        cx.app
                            .quest(author.id.get(), pinchora_game::quests::QuestKind::Like)
                            .await;
                        cx.reply_text(format!(
                            "💖 {} liked {}'s house! It has **{total}** like{} now.",
                            crate::util::mention(author.id),
                            crate::util::mention(host.id),
                            if total == 1 { "" } else { "s" }
                        ))
                        .await
                    }
                    None => cx.reply_ephemeral("You already liked this house today.").await,
                }
            }
            Self::Lock(c) => {
                ensure_house(&cx.app, author.id.get()).await?;
                cx.app.db.set_house_locked(author.id.get(), c.locked).await?;
                let text = if c.locked {
                    "Your house is locked. Visitors will be turned away."
                } else {
                    "Your house is open for visitors again."
                };
                cx.reply_ephemeral(text).await
            }
        }
    }
}

fn room_index(room: Option<i64>) -> i16 {
    (room.unwrap_or(1).clamp(1, 6) - 1) as i16
}

fn starter_room(content: &Content, room_type: &str) -> NewRoom {
    let rt = content.room_type(room_type);
    NewRoom {
        room_type: room_type.to_owned(),
        wallpaper: rt.map_or("plain_cream".to_owned(), |r| r.default_wallpaper.clone()),
        floor: rt.map_or("oak_planks".to_owned(), |r| r.default_floor.clone()),
    }
}

/// Cheapest permanent common from a few cozy categories, so a new house is not empty.
fn starter_kit(content: &Content) -> Vec<String> {
    use pinchora_game::{Availability, Category, Currency, Rarity};
    let wanted = [
        Category::Seating,
        Category::Tables,
        Category::Beds,
        Category::Lighting,
        Category::Rugs,
        Category::WallDecor,
        Category::Plants,
    ];
    wanted
        .iter()
        .filter_map(|cat| {
            content
                .items()
                .filter(|i| {
                    i.base.category == *cat
                        && i.base.rarity == Rarity::Common
                        && i.base.availability == Availability::Permanent
                        && i.base.currency == Currency::Shells
                })
                .min_by_key(|i| i.base.price)
                .map(|i| i.id())
        })
        .collect()
}

/// The player's house, created (with a starter kit) on first use.
pub(crate) async fn ensure_house(app: &App, user_id: u64) -> anyhow::Result<(HouseRow, Vec<RoomRow>)> {
    let content = Content::get();
    let (house, rooms, created) = app.db.house(user_id, &starter_room(content, ROOM_ORDER[0])).await?;
    if created {
        for item in starter_kit(content) {
            app.db.grant_item(user_id, &item, 1).await?;
        }
    }
    Ok((house, rooms))
}

async fn render_room(app: &App, view: RoomView) -> anyhow::Result<Vec<u8>> {
    let sprites = app.sprites.clone();
    app.render(move |_| render_view(&sprites, &view)).await
}

fn room_title(content: &Content, room: &RoomRow) -> String {
    content
        .room_type(&room.room_type)
        .map_or_else(|| room.room_type.clone(), |r| r.name.clone())
}

/// Read-only room card with ◀ ▶ room buttons (`hv:<owner>:<room>`).
async fn room_card(
    cx: &Cx,
    owner: &User,
    owner_name: &str,
    room: i16,
    footer: Option<String>,
    guest: Option<u64>,
) -> anyhow::Result<InteractionResponseData> {
    let owner_id = owner.id.get();
    let (house, rooms) = ensure_house(&cx.app, owner_id).await?;
    let room = room.clamp(0, rooms.len().saturating_sub(1) as i16);
    let row = rooms[room as usize].clone();
    let placements = cx.app.db.placements(owner_id, room).await?;
    let tier = house::tier(house.tier);
    let count = placements.len();
    let content = Content::get();
    let title = room_title(content, &row);
    let mut pets: Vec<String> = Vec::new();
    for who in std::iter::once(owner_id).chain(guest) {
        if let Some(pet) = cx.app.db.active_pet(who).await? {
            pets.push(pet.species);
        }
    }
    let png = render_room(
        &cx.app,
        RoomView {
            width: tier.width,
            height: tier.height,
            room: row,
            placements,
            cursor: None,
            selected: None,
            edit: false,
            pets,
            seed: (Utc::now().timestamp() / 600) as u64,
        },
    )
    .await?;
    let mut embed = EmbedBuilder::new()
        .color(BRAND)
        .title(format!("{owner_name}'s {}", tier.name))
        .description(format!(
            "Room {}/{} · **{title}** · {count} item{} · 💖 {}",
            room + 1,
            rooms.len(),
            if count == 1 { "" } else { "s" },
            house.likes
        ));
    if let Some(footer) = footer {
        embed = embed.footer(EmbedFooterBuilder::new(footer));
    }
    let nav = |label: &str, target: i16, disabled: bool| {
        Component::Button(Button {
            custom_id: Some(format!("hv:{owner_id}:{target}")),
            disabled,
            emoji: None,
            id: None,
            label: Some(label.to_owned()),
            sku_id: None,
            style: ButtonStyle::Secondary,
            url: None,
        })
    };
    let components = if rooms.len() > 1 {
        vec![Component::ActionRow(ActionRow {
            id: None,
            components: vec![
                nav("◀ Room", room - 1, room == 0),
                nav("Room ▶", room + 1, room as usize + 1 >= rooms.len()),
            ],
        })]
    } else {
        Vec::new()
    };
    Ok(Cx::card(embed, "room.png", png, components, false))
}

async fn visit(cx: &Cx, author: &User, target: &ResolvedUser, room: i16) -> anyhow::Result<()> {
    let host = &target.resolved;
    if host.bot {
        return cx.reply_ephemeral("Bots don't have houses.").await;
    }
    let name = resolved_display_name(target);
    if host.id == author.id {
        let data = room_card(cx, host, &name, room, None, None).await?;
        return cx.reply(data).await;
    }
    let (house, _) = ensure_house(&cx.app, host.id.get()).await?;
    if house.locked {
        return cx.reply_ephemeral(format!("{name} has locked their house.")).await;
    }
    let outcome = cx
        .app
        .db
        .record_visit(
            host.id.get(),
            author.id.get(),
            Utc::now().date_naive(),
            VISIT_REWARD,
            VISIT_CAP,
        )
        .await?;
    if matches!(outcome, VisitOutcome::Rewarded { .. }) {
        cx.app
            .quest(author.id.get(), pinchora_game::quests::QuestKind::Visit)
            .await;
    }
    let footer = match outcome {
        VisitOutcome::Rewarded { reward } if reward > 0 => Some(format!("+{reward} shells for visiting today")),
        VisitOutcome::Rewarded { .. } => Some(format!("You've had your {VISIT_CAP} paid visits today")),
        VisitOutcome::AlreadyToday => None,
    };
    let data = room_card(cx, host, &name, room, footer, Some(author.id.get())).await?;
    cx.reply(data).await
}

pub(crate) async fn on_view_nav(cx: &Cx, mut args: Split<'_, char>) -> anyhow::Result<()> {
    let owner: u64 = args.next().and_then(|v| v.parse().ok()).unwrap_or(0);
    let room: i16 = args.next().and_then(|v| v.parse().ok()).unwrap_or(0);
    let owner_user = cx
        .app
        .http
        .user(twilight_model::id::Id::new(owner.max(1)))
        .await?
        .model()
        .await?;
    let name = owner_user
        .global_name
        .clone()
        .unwrap_or_else(|| owner_user.name.clone());
    let data = room_card(cx, &owner_user, &name, room, None, None).await?;
    cx.update(data).await
}

/// Editor state; it all rides in each component's custom id.
#[derive(Debug, Clone)]
struct EditState {
    room: i16,
    x: u32,
    y: u32,
    page: usize,
    item: Option<String>,
}

impl EditState {
    fn id(&self, action: &str) -> String {
        format!(
            "he:{action}:{}:{}:{}:{}:{}",
            self.room,
            self.x,
            self.y,
            self.page,
            self.item.as_deref().unwrap_or("")
        )
    }

    fn parse(args: &mut Split<'_, char>) -> Option<(String, Self)> {
        let action = args.next()?.to_owned();
        let room = args.next()?.parse().ok()?;
        let x = args.next()?.parse().ok()?;
        let y = args.next()?.parse().ok()?;
        let page = args.next()?.parse().ok()?;
        let item = args.next().filter(|s| !s.is_empty()).map(str::to_owned);
        Some((action, Self { room, x, y, page, item }))
    }
}

/// The placement whose footprint covers the cursor tile; smaller layers win.
fn under_cursor<'a>(content: &Content, placements: &'a [PlacementRow], x: u32, y: u32) -> Option<&'a PlacementRow> {
    let rank = |layer: Layer| match layer {
        Layer::Surface => 0,
        Layer::Floor => 1,
        Layer::Wall => 2,
        Layer::Rug => 3,
    };
    placements
        .iter()
        .filter_map(|p| {
            let (layer, r) = house::footprint(content, &p.item_id, p.x as u32, p.y as u32)?;
            (x >= r.x && x < r.x + r.w && y >= r.y && y < r.y + r.d).then_some((rank(layer), p))
        })
        .min_by_key(|(rank, p)| (*rank, -p.id))
        .map(|(_, p)| p)
}

pub(crate) async fn on_edit(cx: &Cx, mut args: Split<'_, char>, value: Option<&str>) -> anyhow::Result<()> {
    let Some((action, mut state)) = EditState::parse(&mut args) else {
        return cx
            .reply_ephemeral("This editor has expired. Run `/house edit` again.")
            .await;
    };
    let user_id = cx.author()?.id.get();
    let content = Content::get();
    let (house_row, rooms) = ensure_house(&cx.app, user_id).await?;
    let tier = house::tier(house_row.tier);
    state.room = state.room.clamp(0, rooms.len().saturating_sub(1) as i16);
    let placements = cx.app.db.placements(user_id, state.room).await?;
    let (w, d) = state
        .item
        .as_deref()
        .and_then(|i| content.item(i))
        .map_or((1, 1), |i| (i.base.footprint[0], i.base.footprint[1]));

    let note = match action.as_str() {
        "s" => {
            state.item = value.map(str::to_owned);
            match state.item.as_deref().and_then(|i| content.item(i)) {
                Some(item) => {
                    let [iw, id] = item.base.footprint;
                    state.x = state.x.min(tier.width.saturating_sub(iw));
                    state.y = state.y.min(tier.height.saturating_sub(id));
                    if item.base.layer == Layer::Wall {
                        state.y = 0;
                    } else if state.y < house::WALL_ROWS {
                        state.y = house::WALL_ROWS;
                    }
                    format!("Selected **{}** ({iw}×{id}). Move it, then **Place**.", item.name())
                }
                None => "Pick something from the menu.".to_owned(),
            }
        }
        "l" | "r" | "u" | "d" => {
            let (dx, dy): (i32, i32) = match action.as_str() {
                "l" => (-1, 0),
                "r" => (1, 0),
                "u" => (0, -1),
                _ => (0, 1),
            };
            state.x = (state.x as i32 + dx).clamp(0, (tier.width - w) as i32) as u32;
            state.y = (state.y as i32 + dy).clamp(0, (tier.height - d) as i32) as u32;
            String::new()
        }
        "p" => match state.item.clone() {
            None => "Pick something to place first.".to_owned(),
            Some(item_id) => {
                let (width, height) = (tier.width, tier.height);
                let (x, y) = (state.x, state.y);
                let outcome = cx
                    .app
                    .db
                    .place_item(user_id, state.room, &item_id, x as i16, y as i16, false, |current| {
                        let placed: Vec<Placed> = current
                            .iter()
                            .map(|p| Placed {
                                item_id: &p.item_id,
                                x: p.x as u32,
                                y: p.y as u32,
                            })
                            .collect();
                        house::can_place(content, width, height, &placed, &item_id, x, y)
                    })
                    .await?;
                let name = content.item(&item_id).map_or(item_id.clone(), |i| i.name());
                match outcome {
                    PlaceOutcome::Placed { .. } => {
                        cx.app.quest(user_id, pinchora_game::quests::QuestKind::Place).await;
                        if !cx.app.db.owns(user_id, &item_id).await? {
                            state.item = None;
                        }
                        format!("Placed **{name}**.")
                    }
                    PlaceOutcome::NotOwned => {
                        state.item = None;
                        format!("You have no **{name}** left.")
                    }
                    PlaceOutcome::Blocked(reason) => format!("Can't place it: {reason}."),
                }
            }
        },
        "k" => match under_cursor(content, &placements, state.x, state.y) {
            Some(p) => {
                let name = content.item(&p.item_id).map_or(p.item_id.clone(), |i| i.name());
                cx.app.db.pick_up(user_id, p.id).await?;
                format!("Picked up **{name}**; it is back in your inventory.")
            }
            None => "Nothing under the cursor.".to_owned(),
        },
        "f" => match under_cursor(content, &placements, state.x, state.y) {
            Some(p) => {
                cx.app.db.flip_placement(user_id, p.id).await?;
                "Flipped.".to_owned()
            }
            None => "Nothing under the cursor to flip.".to_owned(),
        },
        "n" => {
            state.room = (state.room + 1) % rooms.len() as i16;
            state.x = 1;
            state.y = house::WALL_ROWS;
            format!("Now editing room {}.", state.room + 1)
        }
        "q" => {
            state.page = state.page.saturating_sub(1);
            String::new()
        }
        "w" => {
            state.page += 1;
            String::new()
        }
        "x" => {
            let data = InteractionResponseData {
                content: Some("Saved! Show it off with `/house view`.".to_owned()),
                embeds: Some(Vec::new()),
                components: Some(Vec::new()),
                attachments: Some(Vec::new()),
                flags: Some(MessageFlags::EPHEMERAL),
                ..Default::default()
            };
            return cx.update(data).await;
        }
        _ => String::new(),
    };
    let data = editor(cx, user_id, state, &note).await?;
    cx.update(data).await
}

async fn editor(cx: &Cx, user_id: u64, mut state: EditState, note: &str) -> anyhow::Result<InteractionResponseData> {
    let content = Content::get();
    let (house_row, rooms) = ensure_house(&cx.app, user_id).await?;
    let tier = house::tier(house_row.tier);
    let room = rooms[state.room.clamp(0, rooms.len() as i16 - 1) as usize].clone();
    let placements = cx.app.db.placements(user_id, state.room).await?;
    let inventory = cx.app.db.inventory(user_id).await?;
    let pages = inventory.len().div_ceil(PAGE).max(1);
    state.page = state.page.min(pages - 1);

    let selected_item = state.item.as_deref().and_then(|i| content.item(i));
    let (cw, cd) = selected_item.map_or((1, 1), |i| (i.base.footprint[0], i.base.footprint[1]));
    let fits = selected_item.is_some_and(|_| {
        let placed: Vec<Placed> = placements
            .iter()
            .map(|p| Placed {
                item_id: &p.item_id,
                x: p.x as u32,
                y: p.y as u32,
            })
            .collect();
        house::can_place(
            content,
            tier.width,
            tier.height,
            &placed,
            state.item.as_deref().unwrap_or_default(),
            state.x,
            state.y,
        )
        .is_ok()
    });
    let hovered = under_cursor(content, &placements, state.x, state.y);
    let hovered_id = hovered.map(|p| p.id);
    let hovered_name = hovered.and_then(|p| content.item(&p.item_id)).map(|i| i.name());

    let title = room_title(content, &room);
    let png = render_room(
        &cx.app,
        RoomView {
            width: tier.width,
            height: tier.height,
            room,
            placements,
            cursor: Some(Cursor {
                x: state.x,
                y: state.y,
                w: cw,
                d: cd,
                ok: selected_item.is_none() || fits,
            }),
            selected: hovered_id,
            edit: true,
            pets: Vec::new(),
            seed: 0,
        },
    )
    .await?;

    let mut lines = Vec::new();
    if !note.is_empty() {
        lines.push(note.to_owned());
    }
    lines.push(format!(
        "Cursor **{},{}** · Holding: **{}**",
        state.x,
        state.y,
        selected_item.map_or("nothing".to_owned(), |i| i.name())
    ));
    if let Some(name) = &hovered_name {
        lines.push(format!("Under cursor: {name}"));
    }
    let embed = EmbedBuilder::new()
        .color(if fits {
            SUCCESS
        } else if selected_item.is_some() {
            WARN
        } else {
            BRAND
        })
        .title(format!("Editing room {} · {title}", state.room + 1))
        .description(lines.join("\n"));

    let options: Vec<SelectMenuOption> = inventory
        .iter()
        .skip(state.page * PAGE)
        .take(PAGE)
        .filter_map(|row| {
            let item = content.item(&row.item_id)?;
            let [iw, id] = item.base.footprint;
            Some(SelectMenuOption {
                default: state.item.as_deref() == Some(row.item_id.as_str()),
                description: Some(format!(
                    "x{} · {} · {iw}×{id}",
                    row.quantity,
                    item.base.category.label()
                )),
                emoji: None,
                label: item.name().chars().take(100).collect(),
                value: row.item_id.clone(),
            })
        })
        .collect();
    let empty = options.is_empty();
    let select = Component::SelectMenu(SelectMenu {
        channel_types: None,
        custom_id: state.id("s"),
        default_values: None,
        disabled: empty,
        id: None,
        kind: SelectMenuType::Text,
        max_values: Some(1),
        min_values: Some(1),
        options: Some(if empty {
            vec![SelectMenuOption {
                default: false,
                description: None,
                emoji: None,
                label: "Inventory empty".to_owned(),
                value: "none".to_owned(),
            }]
        } else {
            options
        }),
        placeholder: Some(if empty {
            "Your inventory is empty — buy furniture with /shop".to_owned()
        } else {
            format!("Choose furniture (page {}/{pages})", state.page + 1)
        }),
        required: None,
    });
    let button = |label: &str, action: &str, style: ButtonStyle, disabled: bool| {
        Component::Button(Button {
            custom_id: Some(state.id(action)),
            disabled,
            emoji: None,
            id: None,
            label: Some(label.to_owned()),
            sku_id: None,
            style,
            url: None,
        })
    };
    let row = |components: Vec<Component>| Component::ActionRow(ActionRow { id: None, components });
    let mut components = vec![
        row(vec![select]),
        row(vec![
            button("⬅", "l", ButtonStyle::Secondary, false),
            button("⬆", "u", ButtonStyle::Secondary, false),
            button("⬇", "d", ButtonStyle::Secondary, false),
            button("➡", "r", ButtonStyle::Secondary, false),
        ]),
        row(vec![
            button("Place", "p", ButtonStyle::Success, !fits),
            button("Pick up", "k", ButtonStyle::Danger, hovered_id.is_none()),
            button("Flip", "f", ButtonStyle::Secondary, hovered_id.is_none()),
            button("Next room", "n", ButtonStyle::Secondary, rooms.len() < 2),
            button("Done", "x", ButtonStyle::Primary, false),
        ]),
    ];
    if pages > 1 {
        components.push(row(vec![
            button("◀ Items", "q", ButtonStyle::Secondary, state.page == 0),
            button("Items ▶", "w", ButtonStyle::Secondary, state.page + 1 >= pages),
        ]));
    }
    Ok(Cx::card(embed, "room.png", png, components, true))
}

async fn upgrade_offer(cx: &Cx, user_id: u64) -> anyhow::Result<()> {
    let (house_row, _) = ensure_house(&cx.app, user_id).await?;
    let current = house::tier(house_row.tier);
    if house_row.tier as usize >= house::TIERS.len() {
        return cx
            .reply_ephemeral(format!(
                "You already live in the **{}**, the finest house there is.",
                current.name
            ))
            .await;
    }
    let next = house::tier(house_row.tier + 1);
    let balance = cx.app.db.user(user_id).await?.credits;
    let embed = EmbedBuilder::new()
        .color(BRAND)
        .title(format!("Upgrade to the {}", next.name))
        .description(format!(
            "**{} → {} rooms**, rooms grow to {}×{} tiles.\nAll your furniture stays where it is.\n\nPrice: **{}** shells · You have **{}**",
            current.rooms,
            next.rooms,
            next.width,
            next.height,
            thousands(next.price),
            thousands(balance)
        ))
        .build();
    let button = Component::Button(Button {
        custom_id: Some(format!("hu:{}", house_row.tier)),
        disabled: balance < next.price,
        emoji: None,
        id: None,
        label: Some(format!("Upgrade for {} shells", thousands(next.price))),
        sku_id: None,
        style: ButtonStyle::Success,
        url: None,
    });
    cx.reply(InteractionResponseData {
        embeds: Some(vec![embed]),
        components: Some(vec![Component::ActionRow(ActionRow {
            id: None,
            components: vec![button],
        })]),
        flags: Some(MessageFlags::EPHEMERAL),
        ..Default::default()
    })
    .await
}

pub(crate) async fn on_upgrade(cx: &Cx, mut args: Split<'_, char>) -> anyhow::Result<()> {
    let from: i16 = args.next().and_then(|v| v.parse().ok()).unwrap_or(0);
    let user_id = cx.author()?.id.get();
    let (_, rooms) = ensure_house(&cx.app, user_id).await?;
    let next = house::tier(from + 1);
    let content = Content::get();
    let new_rooms: Vec<NewRoom> = ROOM_ORDER
        .iter()
        .skip(rooms.len())
        .take(next.rooms.saturating_sub(rooms.len()))
        .map(|t| starter_room(content, t))
        .collect();
    let text = match cx.app.db.upgrade_house(user_id, from, next.price, &new_rooms).await? {
        UpgradeOutcome::Upgraded { balance, .. } => format!(
            "🏠 Welcome to your **{}**! {} new room{} to decorate. {} shells left.",
            next.name,
            new_rooms.len(),
            if new_rooms.len() == 1 { "" } else { "s" },
            thousands(balance)
        ),
        UpgradeOutcome::Insufficient { balance } => {
            format!(
                "You need {} shells but have {}.",
                thousands(next.price),
                thousands(balance)
            )
        }
        UpgradeOutcome::WrongTier { .. } => "Your house already changed; run `/house upgrade` again.".to_owned(),
    };
    cx.update(InteractionResponseData {
        content: Some(text),
        embeds: Some(Vec::new()),
        components: Some(Vec::new()),
        ..Default::default()
    })
    .await
}
