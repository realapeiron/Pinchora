use std::str::Split;

use chrono::{DateTime, Utc};
use pinchora_db::{
    HouseRow, NewRoom, PetRow, PlaceOutcome, PlacementRow, Refusal, RestoreOutcome, RoomChange, RoomRow,
    UpgradeOutcome, VisitOutcome,
};
use pinchora_game::{
    Content, Layer,
    character::Character,
    house::{self, Placed, ROOM_ORDER},
};
use pinchora_render::room::{Cursor, WallSide, column_letter};
use twilight_interactions::command::{CommandModel, CreateCommand, ResolvedUser};
use twilight_model::{
    channel::message::{
        Component, MessageFlags,
        component::{ActionRow, Button, ButtonStyle, SelectMenu, SelectMenuOption, SelectMenuType},
    },
    http::interaction::InteractionResponseData,
    user::User,
};
use twilight_util::builder::embed::{EmbedBuilder, EmbedFooterBuilder, ImageSource};

use super::{
    pet::is_asleep,
    undo::{self, History, Kind, Piece, Step},
};
use crate::{
    App, Cx,
    game::scene::{RoomPet, RoomView, render_view},
    util::{BRAND, DANGER, SUCCESS, WARN, resolved_display_name, thousands},
};

/// Shells a guest earns for the first visit to a house each day.
const VISIT_REWARD: i64 = 25;
/// Paid visits per guest per day.
const VISIT_CAP: i64 = 10;
/// Inventory entries per select-menu page (Discord's limit).
const PAGE: usize = 25;
/// Pets that wander around their owner's house at once.
const DAYCARE: usize = 8;

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
    #[command(name = "guestbook")]
    Guestbook(super::guestbook::HouseGuestbook),
    #[command(name = "contest")]
    Contest(super::contest::HouseContest),
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
    /// Whose house (leave out to pick from your friends)
    user: Option<ResolvedUser>,
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
    /// Whose house: your partner's, to decorate it together (default: yours)
    owner: Option<ResolvedUser>,
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
                let unread = cx.app.db.guestbook_unread(author.id.get()).await?;
                let footer = (unread > 0).then(|| {
                    format!(
                        "📖 {unread} new guestbook note{} · read with /house guestbook",
                        if unread == 1 { "" } else { "s" }
                    )
                });
                let data = room_card(
                    cx,
                    author.id.get(),
                    &cx.author_display_name(),
                    room_index(c.room),
                    footer,
                    None,
                )
                .await?;
                cx.reply(data).await
            }
            Self::Visit(c) => match &c.user {
                Some(user) if user.resolved.bot => cx.reply_ephemeral("Bots don't have houses.").await,
                Some(user) => {
                    let name = resolved_display_name(user);
                    visit(cx, &author, user.resolved.id.get(), name, room_index(c.room)).await
                }
                None => super::friends::visit_shortlist(cx, room_index(c.room)).await,
            },
            Self::Edit(c) => {
                let me = author.id.get();
                // A partner's house is edited through its own prefix: the click finds the owner
                // in the partners table, so the custom id never carries one.
                let (who, prefix, note) = match &c.owner {
                    Some(owner) if owner.resolved.id != author.id => {
                        if owner.resolved.bot {
                            return cx.reply_ephemeral("Bots don't have houses.").await;
                        }
                        match cx.app.db.partner(me).await? {
                            Some(p) if p.partner_id == owner.resolved.id.get() => {
                                ensure_house(&cx.app, me).await?;
                                (
                                    Who::partner_of(me, p),
                                    "pt",
                                    "Pick furniture from your inventory: it jumps to a free spot, then press **Place**.",
                                )
                            }
                            _ => {
                                return cx
                                    .reply_ephemeral(
                                        "You can only decorate the house of your partner. Pair up with `/partner propose`.",
                                    )
                                    .await;
                            }
                        }
                    }
                    _ => (
                        Who::own(me),
                        "he",
                        "Pick furniture: it jumps to a free spot, then press **Place**.",
                    ),
                };
                let (_, rooms) = ensure_house(&cx.app, who.owner).await?;
                let room = room_index(c.room).min(rooms.len().saturating_sub(1) as i16);
                let state = EditState {
                    prefix,
                    room,
                    x: 1,
                    y: 0,
                    page: 0,
                    flipped: false,
                    mode: CursorMode::Floor,
                    item: None,
                };
                let data = editor_as(cx, who, state, note, None).await?;
                cx.reply(data).await
            }
            Self::Upgrade(_) => upgrade_offer(cx, author.id.get()).await,
            Self::Decorate(c) => {
                let data = super::decorate::panel(cx, room_index(c.room), 0, "").await?;
                cx.reply(data).await
            }
            Self::Guestbook(c) => super::guestbook::run(cx, c).await,
            Self::Contest(c) => c.run(cx).await,
            Self::Like(c) => {
                let host = &c.user.resolved;
                if host.bot || host.id == author.id {
                    return cx.reply_ephemeral("Like someone else's house!").await;
                }
                if super::privacy::private_house(cx, host.id.get()).await? {
                    return Ok(());
                }
                ensure_house(&cx.app, host.id.get()).await?;
                match cx
                    .app
                    .db
                    .like_house(host.id.get(), author.id.get(), cx.app.now().date_naive())
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

pub(crate) fn room_index(room: Option<i64>) -> i16 {
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

pub(crate) async fn render_room(app: &App, view: RoomView) -> anyhow::Result<Vec<u8>> {
    let sprites = app.sprites.clone();
    app.render(move |_| render_view(&sprites, &view)).await
}

pub(crate) fn room_title(content: &Content, room: &RoomRow) -> String {
    content
        .room_type(&room.room_type)
        .map_or_else(|| room.room_type.clone(), |r| r.name.clone())
}

/// The owner's pets that are in the house: the active one first, up to [`DAYCARE`], and none that
/// is away on a field trip. A pet that fell asleep lies in the room instead of wandering.
fn home_pets(mut owned: Vec<PetRow>, now: DateTime<Utc>) -> Vec<RoomPet> {
    owned.retain(|p| !p.is_away(now));
    owned.sort_by_key(|p| !p.active);
    owned
        .into_iter()
        .take(DAYCARE)
        .map(|p| RoomPet {
            asleep: is_asleep(&p, now),
            species: p.species,
        })
        .collect()
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
    let mut pets: Vec<RoomPet> = Vec::new();
    let mut characters: Vec<Character> = Vec::new();
    // The owner's pets live here (up to DAYCARE, the active one first); a guest brings only
    // their active pet. A guest who is the owner is only there once.
    let now = cx.app.now();
    pets.extend(home_pets(cx.app.db.pets(owner_id).await?, now));
    for who in std::iter::once(owner_id).chain(guest.filter(|g| *g != owner_id)) {
        if who != owner_id
            && let Some(pet) = cx.app.db.active_pet(who).await?.filter(|p| !p.is_away(now))
        {
            pets.push(RoomPet {
                asleep: is_asleep(&pet, now),
                species: pet.species,
            });
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
    let mut buttons = Vec::new();
    if rooms.len() > 1 {
        buttons.push(nav("◀ Room", room - 1, room == 0));
        buttons.push(nav("Room ▶", room + 1, room as usize + 1 >= rooms.len()));
    }
    // A guest can leave a note in the owner's guestbook.
    if !plaza && guest.is_some_and(|g| g != owner_id) {
        buttons.push(Component::Button(Button {
            custom_id: Some(format!("gb:s:{owner_id}")),
            disabled: false,
            emoji: None,
            id: None,
            label: Some("Sign guestbook".to_owned()),
            sku_id: None,
            style: ButtonStyle::Primary,
            url: None,
        }));
    }
    let components = if buttons.is_empty() {
        Vec::new()
    } else {
        vec![Component::ActionRow(ActionRow {
            id: None,
            components: buttons,
        })]
    };
    Ok(Cx::card(embed, "room.png", png, components, false))
}

/// Visits a house: whoever the command named, or a friend the shortlist offered.
pub(crate) async fn visit(cx: &Cx, author: &User, host_id: u64, name: String, room: i16) -> anyhow::Result<()> {
    if host_id == author.id.get() {
        let data = room_card(cx, host_id, &name, room, None, None).await?;
        return cx.reply(data).await;
    }
    // A hidden house is not there at all: nobody can walk in, not even by name.
    if cx.app.db.privacy(host_id).await?.hide_house {
        return cx.reply_ephemeral(format!("{name}'s house is private.")).await;
    }
    let (house, _) = ensure_house(&cx.app, host_id).await?;
    if house.locked {
        return cx.reply_ephemeral(format!("{name} has locked their house.")).await;
    }
    let outcome = cx
        .app
        .db
        .record_visit(
            host_id,
            author.id.get(),
            cx.app.now().date_naive(),
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
    let data = room_card(cx, host_id, &name, room, footer, Some(author.id.get())).await?;
    cx.reply(data).await
}

pub(crate) async fn on_view_nav(cx: &Cx, mut args: Split<'_, char>) -> anyhow::Result<()> {
    let owner: u64 = args.next().and_then(|v| v.parse().ok()).unwrap_or(0);
    let room: i16 = args.next().and_then(|v| v.parse().ok()).unwrap_or(0);
    // A house hidden after its picture was posted stops showing its other rooms.
    if owner != cx.author()?.id.get() && cx.app.db.privacy(owner).await?.hide_house {
        return cx.reply_ephemeral("This house is private.").await;
    }
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

/// Whose house an editor click changes and whose inventory its pieces come from. Alone they are
/// the same player (or the server, for the plaza); a partner decorating has the pieces come out of
/// their own inventory and go into the owner's house, where they belong to the owner.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Who {
    pub(crate) owner: u64,
    pub(crate) stock: u64,
    /// For a partner, when their partnership began (Unix microseconds; 0 otherwise): an undo
    /// history belongs to one partnership.
    pub(crate) bond: i64,
}

impl Who {
    pub(crate) fn own(id: u64) -> Self {
        Self {
            owner: id,
            stock: id,
            bond: 0,
        }
    }

    /// `editor` decorating the house of their partner (`pair`).
    fn partner_of(editor: u64, pair: pinchora_db::PartnerRow) -> Self {
        Self {
            owner: pair.partner_id,
            stock: editor,
            bond: pair.since.timestamp_micros(),
        }
    }

    /// Decorating somebody else's house.
    fn is_guest(self) -> bool {
        self.owner != self.stock
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

/// Inventory menu values that turn its page instead of picking furniture.
const PREV_PAGE: &str = "<prev>";
const NEXT_PAGE: &str = "<next>";
/// The jump menu's first option with an empty hand: asks to store everything in the room.
const CLEAR_ROOM: &str = "<clear>";

fn placed_list(placements: &[PlacementRow]) -> Vec<Placed<'_>> {
    placements
        .iter()
        .map(|p| Placed {
            item_id: &p.item_id,
            x: p.x as u32,
            y: p.y as u32,
            flipped: p.flipped,
        })
        .collect()
}

/// Furthest cursor position that keeps the held item (or the one-tile cursor) on the floor or its wall.
fn reach(content: &Content, tier: &house::Tier, state: &EditState) -> (u32, u32) {
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
}

/// Every spot where the held item fits as it is turned now, nearest to the cursor first.
fn free_spots(
    content: &Content,
    tier: &house::Tier,
    placements: &[PlacementRow],
    state: &EditState,
) -> Vec<(u32, u32)> {
    let Some(item_id) = state.item.as_deref() else {
        return Vec::new();
    };
    let placed = placed_list(placements);
    let (max_x, max_y) = reach(content, tier, state);
    let mut spots: Vec<(u32, u32)> = (0..=max_y)
        .flat_map(|y| (0..=max_x).map(move |x| (x, y)))
        .filter(|&(x, y)| {
            house::can_place(content, tier.width, tier.depth, &placed, item_id, x, y, state.flipped).is_ok()
        })
        .collect();
    spots.sort_by_key(|&(x, y)| (x.abs_diff(state.x) + y.abs_diff(state.y), y, x));
    spots
}

#[derive(Debug, PartialEq, Eq)]
enum Settle {
    Stayed,
    Moved,
    NoRoom,
}

/// Leaves the held item where it is when it fits there, else moves it to the nearest spot
/// that fits, so a picked item is always one **Place** away from being in the room.
fn settle(content: &Content, tier: &house::Tier, placements: &[PlacementRow], state: &mut EditState) -> Settle {
    match free_spots(content, tier, placements, state).first() {
        Some(&spot) if spot == (state.x, state.y) => Settle::Stayed,
        Some(&(x, y)) => {
            (state.x, state.y) = (x, y);
            Settle::Moved
        }
        None => {
            let (max_x, max_y) = reach(content, tier, state);
            (state.x, state.y) = (state.x.min(max_x), state.y.min(max_y));
            Settle::NoRoom
        }
    }
}

/// A cursor position as players read it: "C4" on the floor (the letters and numbers drawn
/// along the front edges), "Right wall 3 · high" on a wall.
fn spot_label(x: u32, y: u32, wall: Option<bool>) -> String {
    const HEIGHTS: [&str; 3] = ["high", "middle", "low"];
    match wall {
        None => format!("{}{}", column_letter(x), y + 1),
        Some(left) => format!(
            "{} wall {} · {}",
            if left { "Left" } else { "Right" },
            x + 1,
            HEIGHTS.get(y as usize).copied().unwrap_or("low")
        ),
    }
}

/// What a spot is next to, so the jump menu reads "next to Oak Bed" and not bare coordinates.
fn spot_hint(
    content: &Content,
    placements: &[PlacementRow],
    layer: Layer,
    spot: house::Rect,
    wall: Option<bool>,
) -> String {
    let name = |p: &PlacementRow| content.item(&p.item_id).map_or(p.item_id.clone(), |i| i.name());
    let around = house::Rect {
        x: spot.x.saturating_sub(1),
        y: spot.y.saturating_sub(1),
        w: spot.w + 1 + u32::from(spot.x > 0),
        d: spot.d + 1 + u32::from(spot.y > 0),
    };
    let mut holder = None;
    let mut neighbour = None;
    for p in placements {
        let Some((l, r)) = house::footprint(content, &p.item_id, p.x as u32, p.y as u32, p.flipped) else {
            continue;
        };
        let same_plane = match wall {
            Some(left) => l == Layer::Wall && p.flipped == left,
            None => l != Layer::Wall,
        };
        if !same_plane {
            continue;
        }
        if layer == Layer::Surface && l == Layer::Floor && r.contains(&spot) {
            holder = Some(p);
        } else if neighbour.is_none() && l != Layer::Rug && r.overlaps(&around) && !r.overlaps(&spot) {
            neighbour = Some(p);
        }
    }
    match (holder, neighbour, wall) {
        (Some(p), _, _) => format!("on top of {}", name(p)),
        (None, Some(p), Some(_)) => format!("beside {}", name(p)),
        (None, Some(p), None) => format!("next to {}", name(p)),
        (None, None, Some(_)) => "free wall".to_owned(),
        (None, None, None) if spot.x == 0 || spot.y == 0 => "against a wall".to_owned(),
        (None, None, None) => "open floor".to_owned(),
    }
}

/// Surface pieces standing on a placed holder; they go back to the inventory with it.
fn standing_on<'a>(content: &Content, placements: &'a [PlacementRow], holder: &PlacementRow) -> Vec<&'a PlacementRow> {
    let Some((Layer::Floor, base)) = house::footprint(
        content,
        &holder.item_id,
        holder.x as u32,
        holder.y as u32,
        holder.flipped,
    ) else {
        return Vec::new();
    };
    placements
        .iter()
        .filter(|p| {
            p.id != holder.id
                && matches!(
                    house::footprint(content, &p.item_id, p.x as u32, p.y as u32, p.flipped),
                    Some((Layer::Surface, r)) if base.contains(&r)
                )
        })
        .collect()
}

/// Takes a placement (and whatever stands on it) back into the inventory; returns every piece
/// that went, the placement first.
async fn take_back(
    cx: &Cx,
    content: &Content,
    user_id: u64,
    placements: &[PlacementRow],
    p: &PlacementRow,
) -> anyhow::Result<Vec<Piece>> {
    let on_top = standing_on(content, placements, p);
    for top in &on_top {
        cx.app.db.pick_up(user_id, top.id).await?;
    }
    cx.app.db.pick_up(user_id, p.id).await?;
    Ok(std::iter::once(p).chain(on_top).map(Piece::of).collect())
}

/// Whether an undo change still fits the room as it stands (`current`).
fn undo_fits(
    content: &Content,
    (width, depth): (u32, u32),
    current: &[PlacementRow],
    change: &RoomChange,
) -> Result<(), String> {
    match change {
        RoomChange::Remove(p) if !standing_on(content, current, p).is_empty() => {
            Err("something else stands on it now".to_owned())
        }
        RoomChange::Remove(_) => Ok(()),
        RoomChange::Flip(p) => {
            let others: Vec<Placed> = placed_list(current)
                .into_iter()
                .zip(current)
                .filter(|(_, o)| o.id != p.id)
                .map(|(placed, _)| placed)
                .collect();
            let (x, y) = (p.x as u32, p.y as u32);
            house::can_place(content, width, depth, &others, &p.item_id, x, y, !p.flipped)?;
            if house::flip_drops_surface_items(content, &others, &p.item_id, x, y, p.flipped) {
                return Err("what stands on it would no longer be on top".to_owned());
            }
            Ok(())
        }
        RoomChange::Add(p) => house::can_place(
            content,
            width,
            depth,
            &placed_list(current),
            &p.item_id,
            p.x as u32,
            p.y as u32,
            p.flipped,
        ),
    }
}

/// Asks before storing every piece in the room. The room picture already on the message stays.
async fn clear_prompt(cx: &Cx, state: &EditState, title: &str, pieces: usize, whose: &str) -> anyhow::Result<()> {
    let mut embed = EmbedBuilder::new()
        .color(DANGER)
        .title(format!("Clear room {} · {title}?", state.room + 1))
        .description(format!(
            "Store all **{pieces}** piece{} in this room back in {whose} inventory?\n**Undo** puts them all back where they were.",
            if pieces == 1 { "" } else { "s" }
        ));
    if let Ok(image) = ImageSource::attachment("room.png") {
        embed = embed.image(image);
    }
    let button = |label: &str, action: &str, style: ButtonStyle| {
        Component::Button(Button {
            custom_id: Some(state.id(action)),
            disabled: false,
            emoji: None,
            id: None,
            label: Some(label.to_owned()),
            sku_id: None,
            style,
            url: None,
        })
    };
    cx.update(InteractionResponseData {
        embeds: Some(vec![embed.build()]),
        components: Some(vec![Component::ActionRow(ActionRow {
            id: None,
            components: vec![
                button(
                    &if pieces == 1 {
                        "Store it".to_owned()
                    } else {
                        format!("Store all {pieces}")
                    },
                    "z",
                    ButtonStyle::Danger,
                ),
                button("Cancel", "c", ButtonStyle::Secondary),
            ],
        })]),
        flags: Some(MessageFlags::EPHEMERAL),
        ..Default::default()
    })
    .await
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
    // The house being edited: the player's own, the server plaza, or their partner's.
    let me = cx.author()?.id.get();
    let who = match prefix {
        "pe" => match super::plaza::editable_plaza(cx) {
            Some(guild) => Who::own(guild),
            None => return cx.reply_ephemeral("Only server managers can arrange the plaza.").await,
        },
        "pt" => match cx.app.db.partner(me).await? {
            Some(p) => Who::partner_of(me, p),
            None => {
                return cx
                    .reply_ephemeral("You are no longer partners, so you can't decorate their house.")
                    .await;
            }
        },
        _ => Who::own(me),
    };
    let user_id = who.owner;
    let whose = if who.is_guest() {
        format!("{}'s", partner_label(&cx.app, who.owner).await)
    } else {
        "your".to_owned()
    };
    let content = Content::get();
    let (house_row, rooms) = ensure_house(&cx.app, user_id).await?;
    let tier = house::tier(house_row.tier);
    state.room = state.room.clamp(0, rooms.len().saturating_sub(1) as i16);
    let placements = cx.app.db.placements(user_id, state.room).await?;
    let name_of = |item_id: &str| content.item(item_id).map_or(item_id.to_owned(), |i| i.name());
    // Set when this click put a piece in the player's own house, which counts toward badges.
    let mut placed_now = false;
    let start_room = state.room;
    let mut history = undo::load(&cx.app, who, state.room, &placements).await;

    let note = match action.as_str() {
        "s" => match value {
            Some(PREV_PAGE) => {
                state.page = state.page.saturating_sub(1);
                String::new()
            }
            Some(NEXT_PAGE) => {
                state.page += 1;
                String::new()
            }
            _ => {
                state.item = value.filter(|v| content.item(v).is_some()).map(str::to_owned);
                state.flipped = false;
                match state.item.clone() {
                    Some(item_id) => match settle(content, tier, &placements, &mut state) {
                        Settle::NoRoom => format!(
                            "**{}** fits nowhere in this room right now. **Turn** it, or store something.",
                            name_of(&item_id)
                        ),
                        _ => format!(
                            "Holding **{}**. Press **Place** to put it at **{}**, or pick another spot.",
                            name_of(&item_id),
                            spot_label(state.x, state.y, state.wall(content))
                        ),
                    },
                    None => "Pick something from the menu.".to_owned(),
                }
            }
        },
        // A spot from the jump menu: `x.y` for the held item, `#id` for a placed piece.
        "j" => {
            match value {
                Some(CLEAR_ROOM) if !placements.is_empty() => {
                    let title = room_title(content, &rooms[state.room as usize]);
                    return clear_prompt(cx, &state, &title, placements.len(), &whose).await;
                }
                Some(id) if id.starts_with('#') => {
                    let target = id[1..]
                        .parse::<i64>()
                        .ok()
                        .and_then(|id| placements.iter().find(|p| p.id == id));
                    if let Some(p) = target
                        && let Some((layer, r)) =
                            house::footprint(content, &p.item_id, p.x as u32, p.y as u32, p.flipped)
                    {
                        let wall = (layer == Layer::Wall).then_some(p.flipped);
                        state.mode = CursorMode::from_wall(wall);
                        // A tile of its footprint where it is the piece under the cursor
                        // (not a lamp standing on it).
                        (state.x, state.y) = (r.y..r.y + r.d)
                            .flat_map(|y| (r.x..r.x + r.w).map(move |x| (x, y)))
                            .find(|&(x, y)| {
                                under_cursor(content, &placements, x, y, wall).is_some_and(|u| u.id == p.id)
                            })
                            .unwrap_or((r.x, r.y));
                    }
                }
                Some(spot) => {
                    if let Some((x, y)) = spot.split_once('.')
                        && let (Ok(x), Ok(y)) = (x.parse::<u32>(), y.parse::<u32>())
                    {
                        let (max_x, max_y) = reach(content, tier, &state);
                        (state.x, state.y) = (x.min(max_x), y.min(max_y));
                    }
                }
                None => {}
            }
            String::new()
        }
        "ul" | "dr" | "ur" | "dl" => {
            let max = reach(content, tier, &state);
            (state.x, state.y) = step_cursor(&action, (state.x, state.y), state.wall(content) == Some(true), max);
            String::new()
        }
        // Moves the empty-hand cursor to the floor or a wall; the code is the mode's letter.
        "mf" | "mr" | "ml" => {
            state.mode = CursorMode::from_code(&action[1..]).unwrap_or(CursorMode::Floor);
            let (max_x, max_y) = reach(content, tier, &state);
            (state.x, state.y) = (state.x.min(max_x), state.y.min(max_y));
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
                    .place_item_for(
                        who.owner,
                        who.stock,
                        state.room,
                        &item_id,
                        x as i16,
                        y as i16,
                        flipped,
                        |current| {
                            let placed = placed_list(current);
                            house::can_place(content, width, depth, &placed, &item_id, x, y, flipped)
                        },
                    )
                    .await?;
                let name = name_of(&item_id);
                match outcome {
                    PlaceOutcome::Placed { id } => {
                        placed_now = prefix == "he";
                        let piece = Piece {
                            id,
                            item: item_id.clone(),
                            x: x as i16,
                            y: y as i16,
                            flipped,
                        };
                        history.placed(piece, &name);
                        cx.app.quest(who.stock, pinchora_game::quests::QuestKind::Place).await;
                        if cx.app.db.owns(who.stock, &item_id).await? {
                            // Keep holding the next one, already on the nearest free spot.
                            let placements = cx.app.db.placements(user_id, state.room).await?;
                            match settle(content, tier, &placements, &mut state) {
                                Settle::NoRoom => {
                                    state.release(content);
                                    format!("Placed **{name}**. There is no free spot for another one.")
                                }
                                _ => format!(
                                    "Placed **{name}**. Holding another: **Place** again at **{}**, or pick something else.",
                                    spot_label(state.x, state.y, state.wall(content))
                                ),
                            }
                        } else {
                            state.release(content);
                            format!("Placed **{name}**.")
                        }
                    }
                    PlaceOutcome::NotOwned => {
                        state.release(content);
                        format!("You have no **{name}** left.")
                    }
                    PlaceOutcome::Blocked(reason) => format!("Can't place it: {reason}."),
                }
            }
        },
        // Move: takes the piece under the cursor into the hand, turned as it was.
        "v" if who.is_guest() => format!(
            "A piece in {whose} house is theirs, so it can't go into your hand. **Store** it to give it back to them, or place one of yours."
        ),
        "v" => match under_cursor(content, &placements, state.x, state.y, state.wall(content)) {
            Some(p) => {
                let mut step = Step::new(Kind::Lift, name_of(&p.item_id));
                step.removed = take_back(cx, content, user_id, &placements, p).await?;
                let on_top = step.removed.len() - 1;
                history.push(step);
                state.item = Some(p.item_id.clone());
                state.flipped = p.flipped;
                (state.x, state.y) = (p.x as u32, p.y as u32);
                let extra = if on_top > 0 {
                    format!(" What stood on it ({on_top}) went back to your inventory.")
                } else {
                    String::new()
                };
                format!(
                    "Moving **{}**: pick a spot with **Go to** or the arrows, then **Place**.{extra}",
                    name_of(&p.item_id)
                )
            }
            None => "Nothing under the cursor to move.".to_owned(),
        },
        "k" => match under_cursor(content, &placements, state.x, state.y, state.wall(content)) {
            Some(p) => {
                let mut step = Step::new(Kind::Store, name_of(&p.item_id));
                step.removed = take_back(cx, content, user_id, &placements, p).await?;
                let on_top = step.removed.len() - 1;
                history.push(step);
                let extra = if on_top > 0 {
                    format!(", with the {on_top} on it")
                } else {
                    String::new()
                };
                format!(
                    "Stored **{}**{extra}; it is back in {whose} inventory.",
                    name_of(&p.item_id)
                )
            }
            None => "Nothing under the cursor.".to_owned(),
        },
        // Cancel: empties the hand; the held piece never left the inventory.
        "c" => {
            state.release(content);
            String::new()
        }
        // Turns the held item; when the turned footprint no longer fits, it moves to the nearest spot.
        "t" => {
            state.flipped = !state.flipped;
            match settle(content, tier, &placements, &mut state) {
                Settle::Moved => format!(
                    "Turned it and moved to **{}** so it fits.",
                    spot_label(state.x, state.y, state.wall(content))
                ),
                Settle::NoRoom => "Turned it, but it fits nowhere this way.".to_owned(),
                Settle::Stayed => String::new(),
            }
        }
        "f" => match under_cursor(content, &placements, state.x, state.y, state.wall(content)) {
            Some(p) => {
                let others: Vec<Placed> = placed_list(&placements)
                    .into_iter()
                    .zip(&placements)
                    .filter(|(_, o)| o.id != p.id)
                    .map(|(placed, _)| placed)
                    .collect();
                let (x, y) = (p.x as u32, p.y as u32);
                match house::can_place(content, tier.width, tier.depth, &others, &p.item_id, x, y, !p.flipped) {
                    Ok(()) if house::flip_drops_surface_items(content, &others, &p.item_id, x, y, p.flipped) => {
                        "Can't flip it: what stands on it would no longer be on top.".to_owned()
                    }
                    Ok(()) => {
                        cx.app.db.flip_placement(user_id, p.id).await?;
                        let on_wall = content.item(&p.item_id).is_some_and(|i| i.base.layer == Layer::Wall);
                        let mut step = Step::new(if on_wall { Kind::Flip } else { Kind::Turn }, name_of(&p.item_id));
                        step.turned = Some(Piece {
                            flipped: !p.flipped,
                            ..Piece::of(p)
                        });
                        history.push(step);
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
        // Clear room, confirmed: every piece goes back to the inventory, as one undo step.
        "z" => {
            let removed = cx.app.db.clear_room(user_id, state.room).await?;
            state.release(content);
            if removed.is_empty() {
                "The room is already empty.".to_owned()
            } else {
                let n = removed.len();
                let mut step = Step::new(Kind::Clear, room_title(content, &rooms[state.room as usize]));
                step.removed = removed.iter().map(Piece::of).collect();
                history.push(step);
                format!(
                    "Stored all **{n}** piece{} in {whose} inventory. **Undo** puts them back.",
                    if n == 1 { "" } else { "s" }
                )
            }
        }
        "u" => match history.pop() {
            None => "Nothing to undo.".to_owned(),
            Some(step) => {
                let changes = step.reverse(content, state.room);
                let size = (tier.width, tier.depth);
                let outcome = cx
                    .app
                    .db
                    .restore_for(who.owner, who.stock, state.room, &changes, |current, change| {
                        undo_fits(content, size, current, change)
                    })
                    .await?;
                match outcome {
                    RestoreOutcome::Done { added } => {
                        let put_back = changes.iter().filter_map(|c| match c {
                            RoomChange::Add(p) => Some(p.id),
                            _ => None,
                        });
                        for (old, new) in put_back.zip(added) {
                            history.renumber(old, new);
                        }
                        state.item = None;
                        state.flipped = false;
                        if let Some(p) = step.focus() {
                            let on_wall = content.item(&p.item).is_some_and(|i| i.base.layer == Layer::Wall);
                            state.mode = CursorMode::from_wall(on_wall.then_some(p.flipped));
                            (state.x, state.y) = (p.x.max(0) as u32, p.y.max(0) as u32);
                        }
                        format!("Undid {}.", step.describe())
                    }
                    RestoreOutcome::Refused(why) => {
                        let why = match why {
                            Refusal::Moved(item) => format!("**{}** is no longer where it was", name_of(&item)),
                            Refusal::NotOwned(item) if who.is_guest() => {
                                format!("{whose} inventory has no **{}** left", name_of(&item))
                            }
                            Refusal::NotOwned(item) => format!("you have no **{}** left", name_of(&item)),
                            Refusal::Blocked(reason) => reason,
                        };
                        format!("Can't undo {}: {why}. That step is off the undo list.", step.describe())
                    }
                }
            }
        },
        "n" => {
            state.room = (state.room + 1) % rooms.len() as i16;
            state.x = 1;
            state.y = 0;
            format!("Now editing room {}.", state.room + 1)
        }
        "x" => {
            let data = InteractionResponseData {
                content: Some(
                    match prefix {
                        "pe" => "Saved! See it with `/plaza view`.",
                        "pt" => "Saved! They can see it with `/house view`; you can too, with `/house visit`.",
                        _ => "Saved! Show it off with `/house view`.",
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
    // A new room has its own history; the editor loads it.
    let history = (state.room == start_room).then_some(history);
    let data = editor_as(cx, who, state, &note, history).await?;
    cx.update(data).await?;
    if placed_now {
        super::customize::announce(cx).await;
    }
    Ok(())
}

/// The editor message for somebody's own house (or the plaza). `history` is the room's undo
/// history when the caller already has it (saved here if it changed); `None` loads it.
pub(crate) async fn editor(
    cx: &Cx,
    user_id: u64,
    state: EditState,
    note: &str,
    history: Option<History>,
) -> anyhow::Result<InteractionResponseData> {
    editor_as(cx, Who::own(user_id), state, note, history).await
}

/// What a partner is called in the editor: the name they were last seen with.
async fn partner_label(app: &App, user: u64) -> String {
    match app.db.remembered_name(user).await {
        Ok(Some(name)) if !name.is_empty() => name,
        _ => "your partner".to_owned(),
    }
}

/// [`editor`] for `who`: the house of `who.owner`, with the pieces of `who.stock`'s inventory.
pub(crate) async fn editor_as(
    cx: &Cx,
    who: Who,
    mut state: EditState,
    note: &str,
    history: Option<History>,
) -> anyhow::Result<InteractionResponseData> {
    let content = Content::get();
    let user_id = who.owner;
    let (house_row, rooms) = ensure_house(&cx.app, user_id).await?;
    let tier = house::tier(house_row.tier);
    let room = rooms[state.room.clamp(0, rooms.len() as i16 - 1) as usize].clone();
    let (placements, inventory) = tokio::try_join!(
        cx.app.db.placements(user_id, state.room),
        cx.app.db.inventory(who.stock)
    )?;
    let mut history = match history {
        Some(history) => history,
        None => undo::load(&cx.app, who, state.room, &placements).await,
    };
    undo::save(&cx.app, who, state.room, &mut history, &placements).await;
    let owner_name = if who.is_guest() {
        Some(partner_label(&cx.app, who.owner).await)
    } else {
        None
    };
    // One page shows 25 pieces; with more, two of its slots turn the page.
    let per_page = if inventory.len() > PAGE { PAGE - 2 } else { PAGE };
    let pages = inventory.len().div_ceil(per_page).max(1);
    state.page = state.page.min(pages - 1);

    let selected_item = state.item.as_deref().and_then(|i| content.item(i));
    let wall = state.wall(content);
    let held = state
        .item
        .as_deref()
        .and_then(|i| house::footprint(content, i, 0, 0, state.flipped));
    let (cw, cd) = held.map_or((1, 1), |(_, r)| (r.w, r.d));
    let spots = free_spots(content, tier, &placements, &state);
    let fits = selected_item.is_some() && spots.contains(&(state.x, state.y));
    let hovered = under_cursor(content, &placements, state.x, state.y, wall);
    let hovered_id = hovered.map(|p| p.id);
    let hovered_name = hovered.and_then(|p| content.item(&p.item_id)).map(|i| i.name());

    // The jump menu: free spots for the held piece, or the pieces in the room for an empty hand.
    let jump_options: Vec<SelectMenuOption> = match (state.item.as_deref(), held) {
        (Some(item_id), Some((layer, _))) => spots
            .iter()
            .take(PAGE)
            .filter_map(|&(x, y)| {
                let (_, r) = house::footprint(content, item_id, x, y, state.flipped)?;
                Some(SelectMenuOption {
                    default: (x, y) == (state.x, state.y),
                    description: Some(spot_hint(content, &placements, layer, r, wall)),
                    emoji: None,
                    label: spot_label(x, y, wall),
                    value: format!("{x}.{y}"),
                })
            })
            .collect(),
        _ => {
            let mut near: Vec<&PlacementRow> = placements.iter().collect();
            near.sort_by_key(|p| ((p.x as u32).abs_diff(state.x) + (p.y as u32).abs_diff(state.y), p.id));
            let clear = (!placements.is_empty()).then(|| SelectMenuOption {
                default: false,
                description: Some(format!(
                    "Store all {} piece{} in your inventory",
                    placements.len(),
                    if placements.len() == 1 { "" } else { "s" }
                )),
                emoji: None,
                label: "Clear the room…".to_owned(),
                value: CLEAR_ROOM.to_owned(),
            });
            clear
                .into_iter()
                .chain(near.into_iter().take(PAGE - 1).filter_map(|p| {
                    let item = content.item(&p.item_id)?;
                    let (layer, _) = house::footprint(content, &p.item_id, p.x as u32, p.y as u32, p.flipped)?;
                    let at = spot_label(p.x as u32, p.y as u32, (layer == Layer::Wall).then_some(p.flipped));
                    Some(SelectMenuOption {
                        default: hovered_id == Some(p.id),
                        description: Some(at),
                        emoji: None,
                        label: item.name().chars().take(100).collect(),
                        value: format!("#{}", p.id),
                    })
                }))
                .collect()
        }
    };

    let title = room_title(content, &room);
    // The held piece is drawn where it would go, so the picture shows the room as it will look.
    let mut shown = placements;
    if let Some(item_id) = state.item.as_deref().filter(|_| held.is_some()) {
        shown.push(PlacementRow {
            id: -1,
            room_idx: room.idx,
            item_id: item_id.to_owned(),
            x: state.x as i16,
            y: state.y as i16,
            flipped: state.flipped,
        });
    }
    let png = render_room(
        &cx.app,
        RoomView {
            width: tier.width,
            depth: tier.depth,
            room,
            placements: shown,
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
        "Cursor **{}** · Holding: **{}**",
        spot_label(state.x, state.y, wall),
        selected_item.map_or("nothing".to_owned(), |i| i.name())
    ));
    if let Some(name) = &hovered_name {
        lines.push(format!("Under cursor: {name}"));
    }
    let (heading, footer) = match &owner_name {
        Some(name) => (
            format!("Decorating {name}'s room {} · {title}", state.room + 1),
            format!(
                "A piece you place comes from your inventory and belongs to {name} from then on. Stored pieces go to their inventory."
            ),
        ),
        None => (
            format!("Editing room {} · {title}", state.room + 1),
            "Pick furniture and it jumps to a free spot. Go to moves the cursor anywhere in one click.".to_owned(),
        ),
    };
    let embed = EmbedBuilder::new()
        .color(if fits {
            SUCCESS
        } else if selected_item.is_some() {
            WARN
        } else {
            BRAND
        })
        .title(heading)
        .description(lines.join("\n"))
        .footer(EmbedFooterBuilder::new(footer));

    let mut options: Vec<SelectMenuOption> = Vec::new();
    let option = |label: &str, value: &str, description: Option<String>, default: bool| SelectMenuOption {
        default,
        description,
        emoji: None,
        label: label.chars().take(100).collect(),
        value: value.to_owned(),
    };
    if state.page > 0 {
        options.push(option("◀ Previous items", PREV_PAGE, None, false));
    }
    options.extend(
        inventory
            .iter()
            .skip(state.page * per_page)
            .take(per_page)
            .filter_map(|row| {
                let item = content.item(&row.item_id)?;
                let [iw, id] = item.base.footprint;
                Some(option(
                    &item.name(),
                    &row.item_id,
                    Some(format!(
                        "x{} · {} · {iw}×{id}",
                        row.quantity,
                        item.base.category.label()
                    )),
                    state.item.as_deref() == Some(row.item_id.as_str()),
                ))
            }),
    );
    if state.page + 1 < pages {
        options.push(option("More items ▶", NEXT_PAGE, None, false));
    }
    let select = |action: &str, options: Vec<SelectMenuOption>, placeholder: String, empty: &str| {
        let disabled = options.is_empty();
        Component::SelectMenu(SelectMenu {
            channel_types: None,
            custom_id: state.id(action),
            default_values: None,
            disabled,
            id: None,
            kind: SelectMenuType::Text,
            max_values: Some(1),
            min_values: Some(1),
            options: Some(if disabled {
                vec![option(empty, "none", None, false)]
            } else {
                options
            }),
            placeholder: Some(placeholder),
            required: None,
        })
    };
    let inventory_menu = select(
        "s",
        options,
        if inventory.is_empty() {
            "Your inventory is empty — buy furniture with /shop".to_owned()
        } else if pages > 1 {
            format!("Choose furniture (page {}/{pages})", state.page + 1)
        } else {
            "Choose furniture".to_owned()
        },
        "Inventory empty",
    );
    let jump_placeholder = match (selected_item.is_some(), jump_options.is_empty()) {
        (true, true) => "No free spot fits this".to_owned(),
        (true, false) => format!("Go to a free spot ({} fit)", spots.len()),
        (false, true) => "The room is empty".to_owned(),
        (false, false) => "Go to something in the room".to_owned(),
    };
    let jump_menu = select("j", jump_options, jump_placeholder, "Nothing here");
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
    let holding = selected_item.is_some();
    let mut modes: Vec<Component> = CursorMode::ALL
        .into_iter()
        .map(|mode| {
            let style = if CursorMode::from_wall(wall) == mode {
                ButtonStyle::Primary
            } else {
                ButtonStyle::Secondary
            };
            button(mode.label(), &format!("m{}", mode.code()), style, holding)
        })
        .collect();
    modes.push(button("Next room", "n", ButtonStyle::Secondary, rooms.len() < 2));
    modes.push(button("↶ Undo", "u", ButtonStyle::Secondary, history.is_empty()));
    let components = vec![
        row(vec![inventory_menu]),
        row(vec![jump_menu]),
        row(vec![
            button("↖", "ul", ButtonStyle::Secondary, false),
            button("↗", "ur", ButtonStyle::Secondary, false),
            button("↙", "dl", ButtonStyle::Secondary, false),
            button("↘", "dr", ButtonStyle::Secondary, false),
            button("↻ Turn", "t", ButtonStyle::Secondary, !holding),
        ]),
        row(vec![
            button("Place", "p", ButtonStyle::Success, !fits),
            button(
                "Move",
                "v",
                ButtonStyle::Primary,
                holding || hovered_id.is_none() || who.is_guest(),
            ),
            if holding {
                button("Cancel", "c", ButtonStyle::Secondary, false)
            } else {
                button("Store", "k", ButtonStyle::Danger, hovered_id.is_none())
            },
            button("Flip", "f", ButtonStyle::Secondary, hovered_id.is_none()),
            button("Done", "x", ButtonStyle::Primary, false),
        ]),
        row(modes),
    ];
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

    fn pet(id: i64, species: &str, active: bool, away_until: Option<DateTime<Utc>>) -> PetRow {
        PetRow {
            id,
            owner_id: 1,
            species: species.to_owned(),
            name: species.to_owned(),
            level: 1,
            xp: 0,
            hunger: 80,
            fun: 80,
            energy: 80,
            hygiene: 80,
            needs_at: Utc::now(),
            active,
            trip_dest: away_until.map(|_| "tidepool_meadow".to_owned()),
            trip_ends_at: away_until,
        }
    }

    #[test]
    fn a_pet_on_a_field_trip_is_not_at_home() {
        let now = Utc::now();
        let hour = chrono::Duration::hours(1);
        let pets = vec![
            pet(1, "crab", false, None),
            pet(2, "guppy", true, None),
            pet(3, "otter", false, Some(now + hour)),
            // Back from its trip (the souvenir is still unclaimed): home again.
            pet(4, "jelly", false, Some(now - hour)),
        ];
        let home = |pets| home_pets(pets, now);
        assert_eq!(
            home(pets),
            [RoomPet::awake("guppy"), RoomPet::awake("crab"), RoomPet::awake("jelly")]
        );
        assert!(home(Vec::new()).is_empty());
    }

    #[test]
    fn a_pet_that_fell_asleep_is_drawn_asleep() {
        let now = Utc::now();
        // 80 in everything runs dry at 32 h and the pet drops off a day later, at 56 h.
        let dozing = |hours: i64| PetRow {
            needs_at: now - chrono::Duration::hours(hours),
            ..pet(2, "crab", false, None)
        };
        assert_eq!(
            home_pets(vec![pet(1, "guppy", true, None), dozing(60)], now),
            [RoomPet::awake("guppy"), RoomPet::asleep("crab")]
        );
        assert_eq!(home_pets(vec![dozing(50)], now), [RoomPet::awake("crab")]);
        // An away pet is not home at all, asleep or not.
        let gone = PetRow {
            trip_dest: Some("tidepool_meadow".to_owned()),
            trip_ends_at: Some(now + chrono::Duration::hours(1)),
            ..dozing(60)
        };
        assert!(home_pets(vec![gone], now).is_empty());
    }

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
    fn a_picked_item_lands_on_the_nearest_free_spot() {
        let content = Content::get();
        let tier = house::tier(1);
        let sofa = content
            .items()
            .find(|i| i.base.layer == Layer::Floor && i.base.footprint == [2, 1])
            .map(|i| i.id())
            .expect("content has a 2×1 floor item");
        let blocker = PlacementRow {
            id: 1,
            room_idx: 0,
            item_id: sofa.clone(),
            x: 3,
            y: 3,
            flipped: false,
        };
        let mut state = EditState {
            prefix: "he",
            room: 0,
            x: 3,
            y: 3,
            page: 0,
            flipped: false,
            mode: CursorMode::Floor,
            item: Some(sofa.clone()),
        };
        let placements = [blocker];
        // The cursor sits on the first sofa, so the second moves one tile away.
        assert_eq!(settle(content, tier, &placements, &mut state), Settle::Moved);
        assert_eq!((state.x + state.y).abs_diff(6), 1);
        assert_eq!(settle(content, tier, &placements, &mut state), Settle::Stayed);
        let spots = free_spots(content, tier, &placements, &state);
        assert_eq!(spots.first(), Some(&(state.x, state.y)));
        assert!(!spots.contains(&(3, 3)) && !spots.contains(&(2, 3)));
        assert_eq!(spot_label(2, 3, None), "C4");
        assert_eq!(spot_label(0, 1, Some(true)), "Left wall 1 · middle");
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
