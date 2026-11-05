use std::str::Split;

use chrono::Utc;
use pinchora_db::{HouseRow, NewRoom, PlaceOutcome, PlacementRow, RoomRow, UpgradeOutcome, VisitOutcome};
use pinchora_game::{
    Content, Layer,
    character::Character,
    house::{self, Placed, ROOM_ORDER},
};
use pinchora_render::room::{Cursor, WallSide};
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
    #[command(name = "decorate")]
    Decorate(HouseDecorate),
}

#[derive(CommandModel, CreateCommand)]
#[command(name = "decorate", desc = "Change a room's wallpaper and floor")]
pub struct HouseDecorate {
    /// Room number (default: 1)
    #[command(min_value = 1, max_value = 6)]
    room: Option<i64>,
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
                let data = room_card(
                    cx,
                    author.id.get(),
                    &cx.author_display_name(),
                    room_index(c.room),
                    None,
                    None,
                )
                .await?;
                cx.reply(data).await
            }
            Self::Visit(c) => visit(cx, &author, &c.user, room_index(c.room)).await,
            Self::Edit(c) => {
                let (_, rooms) = ensure_house(&cx.app, author.id.get()).await?;
                let room = room_index(c.room).min(rooms.len().saturating_sub(1) as i16);
                let state = EditState {
                    prefix: "he",
                    room,
                    x: 1,
                    y: 0,
                    page: 0,
                    flipped: false,
                    mode: CursorMode::Floor,
                    item: None,
                };
                let note = "Pick furniture from the menu, move with the arrows, then press **Place**.";
                let data = editor(cx, author.id.get(), state, note).await?;
                cx.reply(data).await
            }
            Self::Upgrade(_) => upgrade_offer(cx, author.id.get()).await,
            Self::Decorate(c) => {
                let data = super::decorate::panel(cx, room_index(c.room), 0, "").await?;
                cx.reply(data).await
            }
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
pub(crate) async fn room_card(
    cx: &Cx,
    owner_id: u64,
    owner_name: &str,
    room: i16,
    footer: Option<String>,
    guest: Option<u64>,
) -> anyhow::Result<InteractionResponseData> {
    let plaza = cx.guild_id() == Some(owner_id);
    let (house, rooms) = ensure_house(&cx.app, owner_id).await?;
    let room = room.clamp(0, rooms.len().saturating_sub(1) as i16);
    let row = rooms[room as usize].clone();
    let placements = cx.app.db.placements(owner_id, room).await?;
    let tier = house::tier(house.tier);
    let count = placements.len();
    let content = Content::get();
    let title = room_title(content, &row);
    let mut pets: Vec<String> = Vec::new();
    let mut characters: Vec<Character> = Vec::new();
    // A guest who is the owner (clicking through their own rooms) is only there once.
    for who in std::iter::once(owner_id).chain(guest.filter(|g| *g != owner_id)) {
        if let Some(pet) = cx.app.db.active_pet(who).await? {
            pets.push(pet.species);
        }
        if let Some(code) = cx.app.db.character(who).await?
            && let Some(character) = Character::decode(content, &code)
        {
            characters.push(character);
        }
    }
    let png = render_room(
        &cx.app,
        RoomView {
            width: tier.width,
            depth: tier.depth,
            room: row,
            placements,
            cursor: None,
            selected: None,
            edit: false,
            pets,
            characters,
            seed: (Utc::now().timestamp() / 600) as u64,
        },
    )
    .await?;
    let mut embed = EmbedBuilder::new()
        .color(BRAND)
        .title(if plaza {
            format!("{owner_name} plaza")
        } else {
            format!("{owner_name}'s {}", tier.name)
        })
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
        let data = room_card(cx, host.id.get(), &name, room, None, None).await?;
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
    let data = room_card(cx, host.id.get(), &name, room, footer, Some(author.id.get())).await?;
    cx.reply(data).await
}

pub(crate) async fn on_view_nav(cx: &Cx, mut args: Split<'_, char>) -> anyhow::Result<()> {
    let owner: u64 = args.next().and_then(|v| v.parse().ok()).unwrap_or(0);
    let room: i16 = args.next().and_then(|v| v.parse().ok()).unwrap_or(0);
    let name = if cx.guild_id() == Some(owner) {
        super::plaza::guild_name(cx, owner).await
    } else {
        let user = cx
            .app
            .http
            .user(twilight_model::id::Id::new(owner.max(1)))
            .await?
            .model()
            .await?;
        user.global_name.clone().unwrap_or_else(|| user.name.clone())
    };
    let data = room_card(cx, owner, &name, room, None, Some(cx.author()?.id.get())).await?;
    cx.update(data).await
}

/// Where the cursor sits while no furniture is held: on the floor, or on one of the two walls
/// so hung items can be picked up and flipped. A held item decides this itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CursorMode {
    Floor,
    RightWall,
    LeftWall,
}

impl CursorMode {
    const ALL: [Self; 3] = [Self::Floor, Self::RightWall, Self::LeftWall];

    /// The single character the mode has in a custom id.
    fn code(self) -> char {
        match self {
            Self::Floor => 'f',
            Self::RightWall => 'r',
            Self::LeftWall => 'l',
        }
    }

    fn from_code(code: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|m| code.len() == 1 && code.starts_with(m.code()))
    }

    fn label(self) -> &'static str {
        match self {
            Self::Floor => "Floor",
            Self::RightWall => "Right wall",
            Self::LeftWall => "Left wall",
        }
    }

    /// `Some(flipped)` on a wall, as in [`EditState::wall`]; `None` on the floor.
    fn wall(self) -> Option<bool> {
        match self {
            Self::Floor => None,
            Self::RightWall => Some(false),
            Self::LeftWall => Some(true),
        }
    }

    fn from_wall(wall: Option<bool>) -> Self {
        match wall {
            None => Self::Floor,
            Some(false) => Self::RightWall,
            Some(true) => Self::LeftWall,
        }
    }
}

/// Editor state; it all rides in each component's custom id.
#[derive(Debug, Clone)]
pub(crate) struct EditState {
    pub(crate) prefix: &'static str,
    pub(crate) room: i16,
    pub(crate) x: u32,
    pub(crate) y: u32,
    pub(crate) page: usize,
    /// The held item turned a quarter (floor items) or hung on the left wall (wall items).
    pub(crate) flipped: bool,
    /// Where the cursor sits with an empty hand.
    pub(crate) mode: CursorMode,
    pub(crate) item: Option<String>,
}

impl EditState {
    /// `<prefix>:<action>:<room>:<x>:<y>:<page>:<flipped 0|1>:<mode f|r|l>:<item>`
    fn id(&self, action: &str) -> String {
        format!(
            "{}:{action}:{}:{}:{}:{}:{}:{}:{}",
            self.prefix,
            self.room,
            self.x,
            self.y,
            self.page,
            u8::from(self.flipped),
            self.mode.code(),
            self.item.as_deref().unwrap_or("")
        )
    }

    /// `Some(flipped)` when the cursor is on a wall, so it moves on that wall: the held item
    /// hangs there, or (empty hand) the mode says so.
    fn wall(&self, content: &Content) -> Option<bool> {
        let Some(item_id) = self.item.as_deref() else {
            return self.mode.wall();
        };
        let item = content.item(item_id)?;
        (item.base.layer == Layer::Wall).then_some(self.flipped)
    }

    /// Puts the held item down; the cursor stays where it was, on its wall or the floor.
    fn release(&mut self, content: &Content) {
        self.mode = CursorMode::from_wall(self.wall(content));
        self.item = None;
    }

    fn parse(prefix: &'static str, args: &mut Split<'_, char>) -> Option<(String, Self)> {
        let action = args.next()?.to_owned();
        let room = args.next()?.parse().ok()?;
        let x = args.next()?.parse().ok()?;
        let y = args.next()?.parse().ok()?;
        let page = args.next()?.parse().ok()?;
        let flipped = match args.next()? {
            "0" => false,
            "1" => true,
            _ => return None,
        };
        let mode = CursorMode::from_code(args.next()?)?;
        let item = args.next().filter(|s| !s.is_empty()).map(str::to_owned);
        Some((
            action,
            Self {
                prefix,
                room,
                x,
                y,
                page,
                flipped,
                mode,
                item,
            },
        ))
    }
}

/// Moves the cursor one step for an arrow button and keeps it inside `0..=max`. The arrows
/// follow the isometric screen: ↖ x−1, ↘ x+1, ↗ y−1, ↙ y+1. On the left wall the two axes swap
/// (↗/↙ slide along the wall, ↖/↘ change level), so every arrow still points where it looks.
fn step_cursor(action: &str, (x, y): (u32, u32), left_wall: bool, (max_x, max_y): (u32, u32)) -> (u32, u32) {
    let (dx, dy): (i32, i32) = match action {
        "ul" => (-1, 0),
        "dr" => (1, 0),
        "ur" => (0, -1),
        "dl" => (0, 1),
        _ => (0, 0),
    };
    let (dx, dy) = if left_wall { (dy, dx) } else { (dx, dy) };
    (
        (x as i32 + dx).clamp(0, max_x as i32) as u32,
        (y as i32 + dy).clamp(0, max_y as i32) as u32,
    )
}

/// The placement under the cursor. On the floor (`wall` is `None`): the item whose footprint
/// covers the tile, surface before floor before rug. On a wall (`Some(flipped)`): the wall
/// item on that wall covering the cell.
fn under_cursor<'a>(
    content: &Content,
    placements: &'a [PlacementRow],
    x: u32,
    y: u32,
    wall: Option<bool>,
) -> Option<&'a PlacementRow> {
    let rank = |layer: Layer| match layer {
        Layer::Surface => 0,
        Layer::Floor => 1,
        Layer::Rug => 2,
        Layer::Wall => 3,
    };
    placements
        .iter()
        .filter_map(|p| {
            let (layer, r) = house::footprint(content, &p.item_id, p.x as u32, p.y as u32, p.flipped)?;
            let here = match wall {
                Some(flipped) => layer == Layer::Wall && p.flipped == flipped,
                None => layer != Layer::Wall,
            };
            (here && x >= r.x && x < r.x + r.w && y >= r.y && y < r.y + r.d).then_some((rank(layer), p))
        })
        .min_by_key(|(rank, p)| (*rank, -p.id))
        .map(|(_, p)| p)
}

pub(crate) async fn on_edit(
    cx: &Cx,
    prefix: &'static str,
    mut args: Split<'_, char>,
    value: Option<&str>,
) -> anyhow::Result<()> {
    let Some((action, mut state)) = EditState::parse(prefix, &mut args) else {
        return cx
            .reply_ephemeral("This editor has expired. Run `/house edit` again.")
            .await;
    };
    // The owner being edited: the player's own house, or the server plaza.
    let user_id = match prefix {
        "pe" => match super::plaza::editable_plaza(cx) {
            Some(guild) => guild,
            None => return cx.reply_ephemeral("Only server managers can arrange the plaza.").await,
        },
        _ => cx.author()?.id.get(),
    };
    let content = Content::get();
    let (house_row, rooms) = ensure_house(&cx.app, user_id).await?;
    let tier = house::tier(house_row.tier);
    state.room = state.room.clamp(0, rooms.len().saturating_sub(1) as i16);
    let placements = cx.app.db.placements(user_id, state.room).await?;
    // Furthest cursor position that keeps the held item (or the one-tile cursor) on the floor or its wall.
    let reach = |state: &EditState| {
        let held = state
            .item
            .as_deref()
            .and_then(|i| house::footprint(content, i, 0, 0, state.flipped));
        match (held, state.wall(content)) {
            (Some((Layer::Wall, r)), _) => (
                house::wall_length(tier.width, tier.depth, state.flipped).saturating_sub(r.w),
                house::WALL_LEVELS.saturating_sub(r.d),
            ),
            (Some((_, r)), _) => (tier.width.saturating_sub(r.w), tier.depth.saturating_sub(r.d)),
            (None, Some(left)) => (
                house::wall_length(tier.width, tier.depth, left) - 1,
                house::WALL_LEVELS - 1,
            ),
            (None, None) => (tier.width - 1, tier.depth - 1),
        }
    };

    let note = match action.as_str() {
        "s" => {
            state.item = value.map(str::to_owned);
            state.flipped = false;
            match state.item.as_deref().and_then(|i| content.item(i)) {
                Some(item) => {
                    let [iw, id] = item.base.footprint;
                    let (max_x, max_y) = reach(&state);
                    state.x = state.x.min(max_x);
                    state.y = state.y.min(max_y);
                    format!("Selected **{}** ({iw}×{id}). Move it, then **Place**.", item.name())
                }
                None => "Pick something from the menu.".to_owned(),
            }
        }
        "ul" | "dr" | "ur" | "dl" => {
            let max = reach(&state);
            (state.x, state.y) = step_cursor(&action, (state.x, state.y), state.wall(content) == Some(true), max);
            String::new()
        }
        // Moves the empty-hand cursor to the floor or a wall; the code is the mode's letter.
        "mf" | "mr" | "ml" => {
            state.mode = CursorMode::from_code(&action[1..]).unwrap_or(CursorMode::Floor);
            let (max_x, max_y) = reach(&state);
            state.x = state.x.min(max_x);
            state.y = state.y.min(max_y);
            String::new()
        }
        "p" => match state.item.clone() {
            None => "Pick something to place first.".to_owned(),
            Some(item_id) => {
                let (width, depth) = (tier.width, tier.depth);
                let (x, y, flipped) = (state.x, state.y, state.flipped);
                let outcome = cx
                    .app
                    .db
                    .place_item(user_id, state.room, &item_id, x as i16, y as i16, flipped, |current| {
                        let placed: Vec<Placed> = current
                            .iter()
                            .map(|p| Placed {
                                item_id: &p.item_id,
                                x: p.x as u32,
                                y: p.y as u32,
                                flipped: p.flipped,
                            })
                            .collect();
                        house::can_place(content, width, depth, &placed, &item_id, x, y, flipped)
                    })
                    .await?;
                let name = content.item(&item_id).map_or(item_id.clone(), |i| i.name());
                match outcome {
                    PlaceOutcome::Placed { .. } => {
                        cx.app.quest(user_id, pinchora_game::quests::QuestKind::Place).await;
                        if !cx.app.db.owns(user_id, &item_id).await? {
                            state.release(content);
                        }
                        format!("Placed **{name}**.")
                    }
                    PlaceOutcome::NotOwned => {
                        state.release(content);
                        format!("You have no **{name}** left.")
                    }
                    PlaceOutcome::Blocked(reason) => format!("Can't place it: {reason}."),
                }
            }
        },
        "k" => match under_cursor(content, &placements, state.x, state.y, state.wall(content)) {
            Some(p) => {
                let name = content.item(&p.item_id).map_or(p.item_id.clone(), |i| i.name());
                cx.app.db.pick_up(user_id, p.id).await?;
                format!("Picked up **{name}**; it is back in your inventory.")
            }
            None => "Nothing under the cursor.".to_owned(),
        },
        // Turns the held item; the new footprint (or wall) may need the cursor pulled back inside.
        "t" => {
            state.flipped = !state.flipped;
            let (max_x, max_y) = reach(&state);
            state.x = state.x.min(max_x);
            state.y = state.y.min(max_y);
            String::new()
        }
        "f" => match under_cursor(content, &placements, state.x, state.y, state.wall(content)) {
            Some(p) => {
                let others: Vec<Placed> = placements
                    .iter()
                    .filter(|o| o.id != p.id)
                    .map(|o| Placed {
                        item_id: &o.item_id,
                        x: o.x as u32,
                        y: o.y as u32,
                        flipped: o.flipped,
                    })
                    .collect();
                let (x, y) = (p.x as u32, p.y as u32);
                match house::can_place(content, tier.width, tier.depth, &others, &p.item_id, x, y, !p.flipped) {
                    Ok(()) if house::flip_drops_surface_items(content, &others, &p.item_id, x, y, p.flipped) => {
                        "Can't flip it: what stands on it would no longer be on top.".to_owned()
                    }
                    Ok(()) => {
                        cx.app.db.flip_placement(user_id, p.id).await?;
                        // With an empty hand the cursor follows a wall item to its new wall.
                        if let Some(left) = state.wall(content).filter(|_| state.item.is_none()) {
                            state.mode = CursorMode::from_wall(Some(!left));
                        }
                        "Flipped.".to_owned()
                    }
                    Err(reason) => format!("Can't flip it: {reason}."),
                }
            }
            None => "Nothing under the cursor to flip.".to_owned(),
        },
        "n" => {
            state.room = (state.room + 1) % rooms.len() as i16;
            state.x = 1;
            state.y = 0;
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
                content: Some(
                    if prefix == "pe" {
                        "Saved! See it with `/plaza view`."
                    } else {
                        "Saved! Show it off with `/house view`."
                    }
                    .to_owned(),
                ),
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

pub(crate) async fn editor(
    cx: &Cx,
    user_id: u64,
    mut state: EditState,
    note: &str,
) -> anyhow::Result<InteractionResponseData> {
    let content = Content::get();
    let (house_row, rooms) = ensure_house(&cx.app, user_id).await?;
    let tier = house::tier(house_row.tier);
    let room = rooms[state.room.clamp(0, rooms.len() as i16 - 1) as usize].clone();
    let placements = cx.app.db.placements(user_id, state.room).await?;
    let inventory = cx.app.db.inventory(user_id).await?;
    let pages = inventory.len().div_ceil(PAGE).max(1);
    state.page = state.page.min(pages - 1);

    let selected_item = state.item.as_deref().and_then(|i| content.item(i));
    let wall = state.wall(content);
    let (cw, cd) = state
        .item
        .as_deref()
        .and_then(|i| house::footprint(content, i, 0, 0, state.flipped))
        .map_or((1, 1), |(_, r)| (r.w, r.d));
    let fits = selected_item.is_some_and(|_| {
        let placed: Vec<Placed> = placements
            .iter()
            .map(|p| Placed {
                item_id: &p.item_id,
                x: p.x as u32,
                y: p.y as u32,
                flipped: p.flipped,
            })
            .collect();
        house::can_place(
            content,
            tier.width,
            tier.depth,
            &placed,
            state.item.as_deref().unwrap_or_default(),
            state.x,
            state.y,
            state.flipped,
        )
        .is_ok()
    });
    let hovered = under_cursor(content, &placements, state.x, state.y, wall);
    let hovered_id = hovered.map(|p| p.id);
    let hovered_name = hovered.and_then(|p| content.item(&p.item_id)).map(|i| i.name());

    let title = room_title(content, &room);
    let png = render_room(
        &cx.app,
        RoomView {
            width: tier.width,
            depth: tier.depth,
            room,
            placements,
            cursor: Some(Cursor {
                x: state.x,
                y: state.y,
                w: cw,
                d: cd,
                wall: wall.map(|flipped| if flipped { WallSide::Left } else { WallSide::Right }),
                ok: selected_item.is_none() || fits,
            }),
            selected: hovered_id,
            edit: true,
            pets: Vec::new(),
            characters: Vec::new(),
            seed: 0,
        },
    )
    .await?;

    let mut lines = Vec::new();
    if !note.is_empty() {
        lines.push(note.to_owned());
    }
    lines.push(format!(
        "Cursor **{},{}**{} · Holding: **{}**",
        state.x,
        state.y,
        match wall {
            Some(false) => " on the right wall",
            Some(true) => " on the left wall",
            None => "",
        },
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
            button("↖", "ul", ButtonStyle::Secondary, false),
            button("↗", "ur", ButtonStyle::Secondary, false),
            button("↙", "dl", ButtonStyle::Secondary, false),
            button("↘", "dr", ButtonStyle::Secondary, false),
            button("↻ Turn", "t", ButtonStyle::Secondary, selected_item.is_none()),
        ]),
        row(vec![
            button("Place", "p", ButtonStyle::Success, !fits),
            button("Pick up", "k", ButtonStyle::Danger, hovered_id.is_none()),
            button("Flip", "f", ButtonStyle::Secondary, hovered_id.is_none()),
            button("Next room", "n", ButtonStyle::Secondary, rooms.len() < 2),
            button("Done", "x", ButtonStyle::Primary, false),
        ]),
    ];
    // Item paging (when there is more than one page) and, for an empty hand, where the cursor sits.
    let mut tools = Vec::new();
    if pages > 1 {
        tools.push(button("◀ Items", "q", ButtonStyle::Secondary, state.page == 0));
        tools.push(button("Items ▶", "w", ButtonStyle::Secondary, state.page + 1 >= pages));
    }
    for mode in CursorMode::ALL {
        let style = if CursorMode::from_wall(wall) == mode {
            ButtonStyle::Primary
        } else {
            ButtonStyle::Secondary
        };
        tools.push(button(
            mode.label(),
            &format!("m{}", mode.code()),
            style,
            selected_item.is_some(),
        ));
    }
    components.push(row(tools));
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
            next.depth,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arrows_follow_the_isometric_screen() {
        let max = (9, 9);
        // Floor and right wall: ↖ x−1, ↘ x+1, ↗ y−1, ↙ y+1.
        assert_eq!(step_cursor("ul", (4, 4), false, max), (3, 4));
        assert_eq!(step_cursor("dr", (4, 4), false, max), (5, 4));
        assert_eq!(step_cursor("ur", (4, 4), false, max), (4, 3));
        assert_eq!(step_cursor("dl", (4, 4), false, max), (4, 5));
        // Left wall: ↗/↙ slide along the wall (x−1/x+1), ↖ is level up, ↘ level down.
        assert_eq!(step_cursor("ur", (4, 1), true, max), (3, 1));
        assert_eq!(step_cursor("dl", (4, 1), true, max), (5, 1));
        assert_eq!(step_cursor("ul", (4, 1), true, max), (4, 0));
        assert_eq!(step_cursor("dr", (4, 1), true, max), (4, 2));
        assert_eq!(step_cursor("?", (4, 4), false, max), (4, 4));
    }

    #[test]
    fn arrows_stay_inside_the_room_or_wall() {
        assert_eq!(step_cursor("ul", (0, 2), false, (5, 4)), (0, 2));
        assert_eq!(step_cursor("ur", (2, 0), false, (5, 4)), (2, 0));
        assert_eq!(step_cursor("dr", (5, 2), false, (5, 4)), (5, 2));
        assert_eq!(step_cursor("dl", (2, 4), false, (5, 4)), (2, 4));
        // A wall: x along it, y is the level (0..WALL_LEVELS).
        let levels = house::WALL_LEVELS - 1;
        assert_eq!(
            step_cursor("ur", (3, 0), false, (6, levels)),
            (3, 0),
            "already on the top level"
        );
        assert_eq!(
            step_cursor("dl", (3, levels), false, (6, levels)),
            (3, levels),
            "already on the bottom level"
        );
        assert_eq!(step_cursor("ul", (3, 0), true, (6, levels)), (3, 0));
        assert_eq!(step_cursor("dr", (3, levels), true, (6, levels)), (3, levels));
        assert_eq!(step_cursor("ur", (0, 1), true, (6, levels)), (0, 1), "at the corner");
        assert_eq!(step_cursor("dl", (6, 1), true, (6, levels)), (6, 1), "at the far end");
    }

    #[test]
    fn picks_the_right_placement_under_the_cursor() {
        let content = Content::get();
        let find = |layer: Layer, footprint: [u32; 2]| {
            content
                .items()
                .find(|i| i.base.layer == layer && i.base.footprint == footprint)
                .map(|i| i.id())
                .expect("content has this kind of item")
        };
        let row = |id: i64, item_id: String, x: i16, y: i16, flipped: bool| PlacementRow {
            id,
            room_idx: 0,
            item_id,
            x,
            y,
            flipped,
        };
        let rug = row(1, find(Layer::Rug, [3, 2]), 0, 0, false);
        let sofa = row(2, find(Layer::Floor, [3, 2]), 1, 1, true);
        let lamp = row(3, find(Layer::Surface, [1, 1]), 2, 2, false);
        let frame = row(4, find(Layer::Wall, [2, 2]), 1, 0, false);
        let all = [rug, sofa, lamp, frame];
        let at = |x, y, wall| under_cursor(content, &all, x, y, wall).map(|p| p.id);
        // The turned sofa covers x 1..3, y 1..4 (a 3×2 footprint would reach x = 3).
        assert_eq!(at(2, 3, None), Some(2));
        assert_eq!(at(3, 1, None), None);
        // Surface over floor over rug.
        assert_eq!(at(2, 2, None), Some(3));
        assert_eq!(at(1, 1, None), Some(2));
        assert_eq!(at(0, 0, None), Some(1));
        // Wall items only answer on their own wall.
        assert_eq!(at(2, 1, Some(false)), Some(4));
        assert_eq!(at(2, 1, Some(true)), None);
        assert_eq!(at(3, 0, Some(false)), None);
    }

    #[test]
    fn edit_state_survives_the_custom_id() {
        let state = EditState {
            prefix: "pe",
            room: 2,
            x: 7,
            y: 3,
            page: 4,
            flipped: true,
            mode: CursorMode::LeftWall,
            item: Some("sofa__mint".to_owned()),
        };
        let id = state.id("ul");
        let mut parts = id.split(':');
        assert_eq!(parts.next(), Some("pe"));
        let (action, back) = EditState::parse("pe", &mut parts).expect("parses");
        assert_eq!(action, "ul");
        assert_eq!((back.room, back.x, back.y, back.page), (2, 7, 3, 4));
        assert!(back.flipped);
        assert_eq!(back.mode, CursorMode::LeftWall);
        assert_eq!(back.item.as_deref(), Some("sofa__mint"));
        // The cursor mode survives with an empty hand too.
        for mode in CursorMode::ALL {
            let empty = EditState {
                mode,
                item: None,
                ..state.clone()
            };
            let id = empty.id("dr");
            let mut parts = id.split(':');
            parts.next();
            let (_, back) = EditState::parse("pe", &mut parts).expect("parses");
            assert_eq!((back.mode, back.item), (mode, None));
        }
        // Editors from before the flip flag and before the cursor mode are told to start over.
        let mut old = "he:l:0:1:2:0:sofa".split(':');
        old.next();
        assert!(EditState::parse("he", &mut old).is_none());
        let mut old = "he:ul:0:1:2:0:0:sofa".split(':');
        old.next();
        assert!(EditState::parse("he", &mut old).is_none());
        let mut bad = "he:ul:0:1:2:0:0:x:sofa".split(':');
        bad.next();
        assert!(EditState::parse("he", &mut bad).is_none());
    }

    #[test]
    fn empty_hand_cursor_follows_its_mode() {
        let content = Content::get();
        let at = |mode, item: Option<&str>, flipped| EditState {
            prefix: "he",
            room: 0,
            x: 0,
            y: 0,
            page: 0,
            flipped,
            mode,
            item: item.map(str::to_owned),
        };
        assert_eq!(at(CursorMode::Floor, None, false).wall(content), None);
        assert_eq!(at(CursorMode::RightWall, None, false).wall(content), Some(false));
        assert_eq!(at(CursorMode::LeftWall, None, false).wall(content), Some(true));
        // A held item decides for itself, whatever the mode says.
        let frame = content
            .items()
            .find(|i| i.base.layer == Layer::Wall)
            .map(|i| i.id())
            .expect("content has wall items");
        let sofa = content
            .items()
            .find(|i| i.base.layer == Layer::Floor)
            .map(|i| i.id())
            .expect("content has floor items");
        assert_eq!(at(CursorMode::Floor, Some(&frame), true).wall(content), Some(true));
        assert_eq!(at(CursorMode::LeftWall, Some(&sofa), false).wall(content), None);
        // Putting a held wall item down leaves the cursor on its wall.
        let mut state = at(CursorMode::Floor, Some(&frame), true);
        state.release(content);
        assert_eq!((state.mode, state.item), (CursorMode::LeftWall, None));
        let mut state = at(CursorMode::RightWall, Some(&sofa), false);
        state.release(content);
        assert_eq!(state.mode, CursorMode::Floor);
    }

    #[test]
    fn custom_ids_fit_discords_limit() {
        for item in Content::get().items() {
            let state = EditState {
                prefix: "he",
                room: 5,
                x: 99,
                y: 99,
                page: 999,
                flipped: true,
                mode: CursorMode::RightWall,
                item: Some(item.id()),
            };
            assert!(state.id("dl").len() <= 100, "{} is too long", item.id());
        }
    }
}
