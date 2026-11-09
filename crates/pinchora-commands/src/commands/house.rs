use std::str::Split;

use chrono::{DateTime, Utc};
use pinchora_core::{i18n::Lang, items::is_soulbound, t};
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
    util::{BRAND, DANGER, SUCCESS, WARN, resolved_display_name, thousands_in},
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
    #[command(name = "aquarium")]
    Aquarium(super::reef_loop::Aquarium),
    #[command(name = "garden")]
    Garden(super::reef_loop::Garden),
    #[command(name = "craft")]
    Craft(super::reef_loop::Craft),
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
            Self::Aquarium(c) => c.run(cx).await,
            Self::Garden(c) => c.run(cx).await,
            Self::Craft(c) => c.run(cx).await,

            Self::View(c) => {
                let unread = cx.app.db.guestbook_unread(author.id.get()).await?;
                let footer = (unread > 0).then(|| t!(cx.lang(), "house.view.unread", count = unread));
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
                Some(user) if user.resolved.bot => cx.reply_ephemeral(t!(cx.lang(), "house.bot")).await,
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
                            return cx.reply_ephemeral(t!(cx.lang(), "house.bot")).await;
                        }
                        match cx.app.db.partner(me).await? {
                            Some(p) if p.partner_id == owner.resolved.id.get() => {
                                ensure_house(&cx.app, me).await?;
                                (Who::partner_of(me, p), "pt", t!(cx.lang(), "house.edit.start-partner"))
                            }
                            _ => {
                                return cx.reply_ephemeral(t!(cx.lang(), "house.partner-only")).await;
                            }
                        }
                    }
                    _ => (Who::own(me), "he", t!(cx.lang(), "house.edit.start")),
                };
                if who.is_guest() && cx.app.db.socially_blocked(who.owner, me).await? {
                    return cx.reply_ephemeral(t!(cx.lang(), "house.edit.blocked")).await;
                }
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
                let data = editor_as(cx, who, state, &note, None).await?;
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
                    return cx.reply_ephemeral(t!(cx.lang(), "house.like-self")).await;
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
                        // The owner hears of the like; the one who liked is not pinged for their own.
                        cx.reply(InteractionResponseData {
                            content: Some(t!(
                                cx.lang(),
                                "house.liked",
                                liker = crate::util::mention(author.id),
                                host = crate::util::mention(host.id),
                                count = total
                            )),
                            allowed_mentions: super::social::ping_only(host.id.get()),
                            ..Default::default()
                        })
                        .await
                    }
                    None => cx.reply_ephemeral(t!(cx.lang(), "house.already-liked")).await,
                }
            }
            Self::Lock(c) => {
                ensure_house(&cx.app, author.id.get()).await?;
                cx.app.db.set_house_locked(author.id.get(), c.locked).await?;
                let text = if c.locked {
                    t!(cx.lang(), "house.locked")
                } else {
                    t!(cx.lang(), "house.unlocked")
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
pub(crate) fn home_pets(mut owned: Vec<PetRow>, now: DateTime<Utc>) -> Vec<RoomPet> {
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
            t!(cx.lang(), "house.view.plaza-title", name = owner_name)
        } else {
            t!(cx.lang(), "house.view.title", name = owner_name, house = tier.name)
        })
        .description(t!(
            cx.lang(),
            "house.view.summary",
            room = room + 1,
            rooms = rooms.len(),
            title = title,
            count = count,
            likes = house.likes
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
        buttons.push(nav(&t!(cx.lang(), "house.view.prev"), room - 1, room == 0));
        buttons.push(nav(
            &t!(cx.lang(), "house.view.next"),
            room + 1,
            room as usize + 1 >= rooms.len(),
        ));
    }
    // A guest can leave a note in the owner's guestbook.
    if !plaza && guest.is_some_and(|g| g != owner_id) {
        buttons.push(Component::Button(Button {
            custom_id: Some(format!("gb:s:{owner_id}")),
            disabled: false,
            emoji: None,
            id: None,
            label: Some(t!(cx.lang(), "house.view.sign")),
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
    let mut data = Cx::card(embed, "room.png", png, components, false);
    if !plaza {
        super::reef_loop::attach_aquarium(cx, owner_id, &mut data).await?;
    }
    Ok(data)
}

/// Visits a house: whoever the command named, or a friend the shortlist offered.
pub(crate) async fn visit(cx: &Cx, author: &User, host_id: u64, name: String, room: i16) -> anyhow::Result<()> {
    if host_id == author.id.get() {
        let data = room_card(cx, host_id, &name, room, None, None).await?;
        return cx.reply(data).await;
    }
    // A hidden house is not there at all: nobody can walk in, not even by name.
    if cx.app.db.privacy(host_id).await?.hide_house || cx.app.db.socially_blocked(host_id, author.id.get()).await? {
        return cx
            .reply_ephemeral(t!(cx.lang(), "house.visit.private", name = name))
            .await;
    }
    let (house, _) = ensure_house(&cx.app, host_id).await?;
    if house.locked {
        return cx
            .reply_ephemeral(t!(cx.lang(), "house.visit.locked", name = name))
            .await;
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
        VisitOutcome::Rewarded { reward } if reward > 0 => Some(t!(cx.lang(), "house.visit.paid", reward = reward)),
        VisitOutcome::Rewarded { .. } => Some(t!(cx.lang(), "house.visit.cap", cap = VISIT_CAP)),
        VisitOutcome::AlreadyToday => None,
        VisitOutcome::Unavailable => {
            return cx
                .reply_ephemeral(t!(cx.lang(), "house.visit.private", name = name))
                .await;
        }
    };
    let data = room_card(cx, host_id, &name, room, footer, Some(author.id.get())).await?;
    cx.reply(data).await
}

pub(crate) async fn on_view_nav(cx: &Cx, mut args: Split<'_, char>) -> anyhow::Result<()> {
    let owner: u64 = args.next().and_then(|v| v.parse().ok()).unwrap_or(0);
    let room: i16 = args.next().and_then(|v| v.parse().ok()).unwrap_or(0);
    // Old navigation controls must honor the same current access policy as a new visit,
    // including the aquarium attached to every room. The owner can still enter.
    let viewer = cx.author()?.id.get();
    if owner != viewer {
        let private = cx.app.db.privacy(owner).await?.hide_house;
        let restricted = cx.guild_id() != Some(owner)
            && (cx.app.db.socially_blocked(owner, viewer).await?
                || cx
                    .app
                    .db
                    .house_if_any(owner)
                    .await?
                    .is_some_and(|(house, _)| house.locked));
        if private || restricted {
            return cx.reply_ephemeral(t!(cx.lang(), "privacy.house-private")).await;
        }
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

    fn label(self, lang: Lang) -> String {
        match self {
            Self::Floor => t!(lang, "house.editor.floor"),
            Self::RightWall => t!(lang, "house.editor.right-wall"),
            Self::LeftWall => t!(lang, "house.editor.left-wall"),
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
pub struct Who {
    pub owner: u64,
    pub stock: u64,
    /// For a partner, when their partnership began (Unix microseconds; 0 otherwise): an undo
    /// history belongs to one partnership.
    pub bond: i64,
}

impl Who {
    pub fn own(id: u64) -> Self {
        Self {
            owner: id,
            stock: id,
            bond: 0,
        }
    }

    /// `editor` decorating the house of their partner (`pair`).
    pub fn partner_of(editor: u64, pair: pinchora_db::PartnerRow) -> Self {
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

/// How tall each item's art is, in pixels: what decides whether a piece in front hides another.
type Heights<'a> = &'a dyn Fn(&str) -> u32;

/// Every spot where the held item fits as it is turned now: the ones where it stays in sight first,
/// then the ones where a taller piece in front would hide most of it, each group nearest to the
/// cursor first.
fn free_spots(
    content: &Content,
    tier: &house::Tier,
    placements: &[PlacementRow],
    state: &EditState,
    heights: Heights<'_>,
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
    let hidden = |&(x, y): &(u32, u32)| house::is_hidden(content, &placed, heights, item_id, x, y, state.flipped);
    spots.sort_by_cached_key(|spot| {
        (
            hidden(spot),
            spot.0.abs_diff(state.x) + spot.1.abs_diff(state.y),
            spot.1,
            spot.0,
        )
    });
    spots
}

#[derive(Debug, PartialEq, Eq)]
enum Settle {
    Stayed,
    Moved,
    NoRoom,
}

/// Leaves the held item where it is when it fits there and shows, else moves it to the nearest spot
/// that fits and shows, so a picked item is always one **Place** away from being in the room. A spot
/// hidden behind a taller piece is the last choice, taken when nothing else fits. `keep_hidden` keeps an
/// item that fits where it is even then: the player put it there, and only a turn is changing the fit.
fn settle(
    content: &Content,
    tier: &house::Tier,
    placements: &[PlacementRow],
    state: &mut EditState,
    heights: Heights<'_>,
    keep_hidden: bool,
) -> Settle {
    let spots = free_spots(content, tier, placements, state, heights);
    let here = (state.x, state.y);
    match spots.first() {
        Some(&spot) if spot == here => Settle::Stayed,
        Some(_) if keep_hidden && spots.contains(&here) => Settle::Stayed,
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
fn spot_label(lang: Lang, x: u32, y: u32, wall: Option<bool>) -> String {
    const HEIGHTS: [&str; 3] = ["high", "middle", "low"];
    match wall {
        None => format!("{}{}", column_letter(x), y + 1),
        Some(left) => t!(
            lang,
            if left {
                "house.spot.left-wall"
            } else {
                "house.spot.right-wall"
            },
            n = x + 1,
            height = t!(
                lang,
                &format!("house.spot.{}", HEIGHTS.get(y as usize).copied().unwrap_or("low"))
            )
        ),
    }
}

/// What a spot is next to, so the jump menu reads "next to Oak Bed" and not bare coordinates.
fn spot_hint(
    lang: Lang,
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
        (Some(p), _, _) => t!(lang, "house.spot.on-top", item = name(p)),
        (None, Some(p), Some(_)) => t!(lang, "house.spot.beside", item = name(p)),
        (None, Some(p), None) => t!(lang, "house.spot.next-to", item = name(p)),
        (None, None, Some(_)) => t!(lang, "house.spot.free-wall"),
        (None, None, None) if spot.x == 0 || spot.y == 0 => t!(lang, "house.spot.against-wall"),
        (None, None, None) => t!(lang, "house.spot.open-floor"),
    }
}

/// Surface pieces standing on a placed holder; they go back to the inventory with it.
pub fn standing_on<'a>(
    content: &Content,
    placements: &'a [PlacementRow],
    holder: &PlacementRow,
) -> Vec<&'a PlacementRow> {
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
    db: &pinchora_db::Db,
    content: &Content,
    who: Who,
    size: (u32, u32),
    placements: &[PlacementRow],
    p: &PlacementRow,
) -> anyhow::Result<Option<Vec<Piece>>> {
    let on_top = standing_on(content, placements, p);
    // Remove supported items first, but commit the whole group and inventory refund together.
    let changes: Vec<_> = on_top
        .iter()
        .copied()
        .chain(std::iter::once(p))
        .cloned()
        .map(RoomChange::Remove)
        .collect();
    let outcome = db
        .edit_room(
            who.owner,
            who.owner,
            who.owner,
            p.room_idx,
            &changes,
            who.is_guest().then_some((who.stock, who.bond)),
            |rows| {
                if undo::fingerprint(rows) == undo::fingerprint(placements) {
                    Ok(())
                } else {
                    Err("room_changed".into())
                }
            },
            |rows, change| undo_fits(content, size, rows, change),
        )
        .await?;
    Ok(matches!(outcome, RestoreOutcome::Done { .. })
        .then(|| std::iter::once(p).chain(on_top).map(Piece::of).collect()))
}

/// Whether an undo change still fits the room as it stands (`current`).
pub fn undo_fits(
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
async fn clear_prompt(
    cx: &Cx,
    state: &EditState,
    title: &str,
    pieces: usize,
    guest_of: Option<&str>,
) -> anyhow::Result<()> {
    let lang = cx.lang();
    let question = match guest_of {
        Some(name) => t!(lang, "house.clear.question-theirs", count = pieces, name = name),
        None => t!(lang, "house.clear.question", count = pieces),
    };
    let mut embed = EmbedBuilder::new()
        .color(DANGER)
        .title(t!(lang, "house.clear.title", room = state.room + 1, title = title))
        .description(question);
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
                button(&t!(lang, "house.clear.store", count = pieces), "z", ButtonStyle::Danger),
                button(&t!(lang, "house.editor.cancel"), "c", ButtonStyle::Secondary),
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
        return cx.reply_ephemeral(t!(cx.lang(), "house.edit.expired")).await;
    };
    // The house being edited: the player's own, the server plaza, or their partner's.
    let me = cx.author()?.id.get();
    let who = match prefix {
        "pe" => match super::plaza::editable_plaza(cx) {
            Some(guild) => Who::own(guild),
            None => return cx.reply_ephemeral(t!(cx.lang(), "house.edit.plaza-managers")).await,
        },
        "pt" => match cx.app.db.partner(me).await? {
            Some(p) => Who::partner_of(me, p),
            None => {
                return cx.reply_ephemeral(t!(cx.lang(), "house.edit.no-longer-partners")).await;
            }
        },
        _ => Who::own(me),
    };
    let _editor_guard = cx.app.db.house_editor(who.owner).await?;
    if who.is_guest() && cx.app.db.socially_blocked(who.owner, me).await? {
        return cx.reply_ephemeral(t!(cx.lang(), "house.edit.blocked")).await;
    }
    let user_id = who.owner;
    let lang = cx.lang();
    // The partner whose house this is, when it is not the player's own: some lines name them.
    let guest_of = if who.is_guest() {
        Some(partner_label(&cx.app, who.owner).await)
    } else {
        None
    };
    let content = Content::get();
    let (house_row, rooms) = ensure_house(&cx.app, user_id).await?;
    let tier = house::tier(house_row.tier);
    state.room = state.room.clamp(0, rooms.len().saturating_sub(1) as i16);
    let placements = cx.app.db.placements(user_id, state.room).await?;
    let name_of = |item_id: &str| content.item(item_id).map_or(item_id.to_owned(), |i| i.name());
    let heights = |id: &str| cx.app.sprites.item(content, id).map_or(0, |sprite| sprite.height());
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
                    Some(item_id) => match settle(content, tier, &placements, &mut state, &heights, false) {
                        Settle::NoRoom => t!(lang, "house.edit.fits-nowhere", item = name_of(&item_id)),
                        _ => t!(
                            lang,
                            "house.edit.holding",
                            item = name_of(&item_id),
                            spot = spot_label(lang, state.x, state.y, state.wall(content))
                        ),
                    },
                    None => t!(lang, "house.edit.pick"),
                }
            }
        },
        // A spot from the jump menu: `x.y` for the held item, `#id` for a placed piece.
        "j" => {
            match value {
                Some(CLEAR_ROOM) if !placements.is_empty() => {
                    let title = room_title(content, &rooms[state.room as usize]);
                    return clear_prompt(cx, &state, &title, placements.len(), guest_of.as_deref()).await;
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
            None => t!(lang, "house.edit.pick-first"),
            Some(item_id) => {
                let (width, depth) = (tier.width, tier.depth);
                let (x, y, flipped) = (state.x, state.y, state.flipped);
                let changes = [RoomChange::Add(PlacementRow {
                    id: 0,
                    room_idx: state.room,
                    item_id: item_id.clone(),
                    x: x as i16,
                    y: y as i16,
                    flipped,
                })];
                let outcome = cx
                    .app
                    .db
                    .edit_room(
                        who.owner,
                        who.stock,
                        who.owner,
                        state.room,
                        &changes,
                        who.is_guest().then_some((who.stock, who.bond)),
                        |rows| {
                            if undo::fingerprint(rows) == undo::fingerprint(&placements) {
                                Ok(())
                            } else {
                                Err("room_changed".into())
                            }
                        },
                        |rows, change| undo_fits(content, (width, depth), rows, change),
                    )
                    .await?;
                let outcome = match outcome {
                    RestoreOutcome::Done { added } => PlaceOutcome::Placed { id: added[0] },
                    RestoreOutcome::Refused(Refusal::NotOwned(_)) => PlaceOutcome::NotOwned,
                    RestoreOutcome::Refused(Refusal::Blocked(reason))
                        if reason != "access_denied"
                            && reason != "room_changed"
                            && reason != "You are no longer partners." =>
                    {
                        PlaceOutcome::Blocked(reason)
                    }
                    RestoreOutcome::Refused(_) => PlaceOutcome::Blocked(t!(lang, "house.edit.blocked")),
                };
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
                            match settle(content, tier, &placements, &mut state, &heights, false) {
                                Settle::NoRoom => {
                                    state.release(content);
                                    t!(lang, "house.edit.placed-full", item = name)
                                }
                                _ => t!(
                                    lang,
                                    "house.edit.placed-next",
                                    item = name,
                                    spot = spot_label(lang, state.x, state.y, state.wall(content))
                                ),
                            }
                        } else {
                            state.release(content);
                            t!(lang, "house.edit.placed", item = name)
                        }
                    }
                    PlaceOutcome::NotOwned => {
                        state.release(content);
                        t!(lang, "house.edit.none-left", item = name)
                    }
                    PlaceOutcome::Blocked(reason) => t!(lang, "house.edit.cant-place", reason = reason),
                }
            }
        },
        // Move: takes the piece under the cursor into the hand, turned as it was.
        "v" if who.is_guest() => t!(
            lang,
            "house.edit.move-theirs",
            name = guest_of.as_deref().unwrap_or_default()
        ),
        "v" => match under_cursor(content, &placements, state.x, state.y, state.wall(content)) {
            Some(p) => {
                let mut step = Step::new(Kind::Lift, name_of(&p.item_id));
                let Some(removed) =
                    take_back(&cx.app.db, content, who, (tier.width, tier.depth), &placements, p).await?
                else {
                    return cx.reply_ephemeral(t!(lang, "house.edit.blocked")).await;
                };
                step.removed = removed;
                let on_top = step.removed.len() - 1;
                history.push(step);
                state.item = Some(p.item_id.clone());
                state.flipped = p.flipped;
                (state.x, state.y) = (p.x as u32, p.y as u32);
                let extra = if on_top > 0 {
                    t!(lang, "house.edit.move-on-top", count = on_top)
                } else {
                    String::new()
                };
                t!(lang, "house.edit.moving", item = name_of(&p.item_id), extra = extra)
            }
            None => t!(lang, "house.edit.nothing-to-move"),
        },
        "k" => match under_cursor(content, &placements, state.x, state.y, state.wall(content)) {
            Some(p) => {
                let mut step = Step::new(Kind::Store, name_of(&p.item_id));
                let Some(removed) =
                    take_back(&cx.app.db, content, who, (tier.width, tier.depth), &placements, p).await?
                else {
                    return cx.reply_ephemeral(t!(lang, "house.edit.blocked")).await;
                };
                step.removed = removed;
                let on_top = step.removed.len() - 1;
                history.push(step);
                let extra = if on_top > 0 {
                    t!(lang, "house.edit.store-on-top", count = on_top)
                } else {
                    String::new()
                };
                match &guest_of {
                    Some(owner) => t!(
                        lang,
                        "house.edit.stored-theirs",
                        item = name_of(&p.item_id),
                        extra = extra,
                        name = owner
                    ),
                    None => t!(lang, "house.edit.stored", item = name_of(&p.item_id), extra = extra),
                }
            }
            None => t!(lang, "house.edit.nothing-under"),
        },
        // Cancel: empties the hand; the held piece never left the inventory.
        "c" => {
            state.release(content);
            String::new()
        }
        // Turns the held item; when the turned footprint no longer fits, it moves to the nearest spot.
        "t" => {
            state.flipped = !state.flipped;
            match settle(content, tier, &placements, &mut state, &heights, true) {
                Settle::Moved => t!(
                    lang,
                    "house.edit.turned-moved",
                    spot = spot_label(lang, state.x, state.y, state.wall(content))
                ),
                Settle::NoRoom => t!(lang, "house.edit.turned-no-room"),
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
                        t!(lang, "house.edit.cant-flip-top")
                    }
                    Ok(()) => {
                        let outcome = cx
                            .app
                            .db
                            .edit_room(
                                who.owner,
                                who.owner,
                                who.owner,
                                state.room,
                                &[RoomChange::Flip(p.clone())],
                                who.is_guest().then_some((who.stock, who.bond)),
                                |rows| {
                                    if undo::fingerprint(rows) == undo::fingerprint(&placements) {
                                        Ok(())
                                    } else {
                                        Err("room_changed".into())
                                    }
                                },
                                |rows, change| undo_fits(content, (tier.width, tier.depth), rows, change),
                            )
                            .await?;
                        if !matches!(outcome, RestoreOutcome::Done { .. }) {
                            return cx.reply_ephemeral(t!(lang, "house.edit.nothing-to-flip")).await;
                        }
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
                        t!(lang, "house.edit.flipped")
                    }
                    Err(reason) => t!(lang, "house.edit.cant-flip", reason = reason),
                }
            }
            None => t!(lang, "house.edit.nothing-to-flip"),
        },
        // Clear room, confirmed: every piece goes back to the inventory, as one undo step.
        "z" => {
            let mut removed = placements.clone();
            removed.sort_by_key(|p| {
                if content.item(&p.item_id).is_some_and(|i| i.base.layer == Layer::Surface) {
                    0
                } else {
                    1
                }
            });
            let changes: Vec<_> = removed.iter().cloned().map(RoomChange::Remove).collect();
            let outcome = cx
                .app
                .db
                .edit_room(
                    who.owner,
                    who.owner,
                    who.owner,
                    state.room,
                    &changes,
                    who.is_guest().then_some((who.stock, who.bond)),
                    |rows| {
                        if undo::fingerprint(rows) == undo::fingerprint(&placements) {
                            Ok(())
                        } else {
                            Err("room_changed".into())
                        }
                    },
                    |rows, change| undo_fits(content, (tier.width, tier.depth), rows, change),
                )
                .await?;
            if !matches!(outcome, RestoreOutcome::Done { .. }) {
                return cx.reply_ephemeral(t!(lang, "house.edit.already-empty")).await;
            }
            state.release(content);
            if removed.is_empty() {
                t!(lang, "house.edit.already-empty")
            } else {
                let n = removed.len();
                let mut step = Step::new(Kind::Clear, room_title(content, &rooms[state.room as usize]));
                step.removed = removed.iter().map(Piece::of).collect();
                history.push(step);
                match &guest_of {
                    Some(owner) => t!(lang, "house.edit.cleared-theirs", count = n, name = owner),
                    None => t!(lang, "house.edit.cleared", count = n),
                }
            }
        }
        "u" => match history.pop() {
            None => t!(lang, "house.edit.nothing-to-undo"),
            Some(step) => {
                let changes = step.reverse(content, state.room);
                let size = (tier.width, tier.depth);
                let outcome = cx
                    .app
                    .db
                    .edit_room(
                        who.owner,
                        who.owner,
                        who.stock,
                        state.room,
                        &changes,
                        who.is_guest().then_some((who.stock, who.bond)),
                        |rows| {
                            if undo::fingerprint(rows) == undo::fingerprint(&placements) {
                                Ok(())
                            } else {
                                Err("room_changed".into())
                            }
                        },
                        |current, change| undo_fits(content, size, current, change),
                    )
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
                        t!(lang, "house.edit.undid", step = step.describe())
                    }
                    RestoreOutcome::Refused(why) => {
                        let why = match why {
                            Refusal::Moved(item) => t!(lang, "house.edit.undo-moved", item = name_of(&item)),
                            Refusal::NotOwned(item) => match &guest_of {
                                Some(owner) => {
                                    t!(lang, "house.edit.undo-none-theirs", item = name_of(&item), name = owner)
                                }
                                None => t!(lang, "house.edit.undo-none", item = name_of(&item)),
                            },
                            Refusal::Blocked(reason) => reason,
                        };
                        t!(lang, "house.edit.cant-undo", step = step.describe(), why = why)
                    }
                }
            }
        },
        "n" => {
            state.room = (state.room + 1) % rooms.len() as i16;
            state.x = 1;
            state.y = 0;
            t!(lang, "house.edit.now-room", room = state.room + 1)
        }
        "x" => {
            let data = InteractionResponseData {
                content: Some(t!(
                    lang,
                    match prefix {
                        "pe" => "house.edit.saved-plaza",
                        "pt" => "house.edit.saved-partner",
                        _ => "house.edit.saved",
                    }
                )),
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
    let lang = cx.lang();
    let content = Content::get();
    let user_id = who.owner;
    // Editor refresh follows bot mutations; Reef verifies the layout before broadcasting.
    let _ = cx
        .app
        .cache
        .publish_invalidation(pinchora_cache::Invalidation::House(user_id))
        .await;
    let (house_row, rooms) = ensure_house(&cx.app, user_id).await?;
    let tier = house::tier(house_row.tier);
    let room = rooms[state.room.clamp(0, rooms.len() as i16 - 1) as usize].clone();
    let (placements, mut inventory) = tokio::try_join!(
        cx.app.db.placements(user_id, state.room),
        cx.app.db.inventory(who.stock)
    )?;
    // A soulbound piece never goes into somebody else's house, so a guest's menu leaves them out.
    let kept_back = who.is_guest() && inventory.iter().any(|r| is_soulbound(&r.item_id));
    if who.is_guest() {
        inventory.retain(|r| !is_soulbound(&r.item_id));
    }
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
    let heights = |id: &str| cx.app.sprites.item(content, id).map_or(0, |sprite| sprite.height());
    let spots = free_spots(content, tier, &placements, &state, &heights);
    let fits = selected_item.is_some() && spots.contains(&(state.x, state.y));
    let hovered = under_cursor(content, &placements, state.x, state.y, wall);
    let hovered_id = hovered.map(|p| p.id);
    let hovered_name = hovered.and_then(|p| content.item(&p.item_id)).map(|i| i.name());

    // The jump menu: free spots for the held piece, or the pieces in the room for an empty hand.
    let on_floor = placed_list(&placements);
    let jump_options: Vec<SelectMenuOption> = match (state.item.as_deref(), held) {
        (Some(item_id), Some((layer, _))) => spots
            .iter()
            .take(PAGE)
            .filter_map(|&(x, y)| {
                let (_, r) = house::footprint(content, item_id, x, y, state.flipped)?;
                let mut hint = spot_hint(lang, content, &placements, layer, r, wall);
                if house::is_hidden(content, &on_floor, &heights, item_id, x, y, state.flipped) {
                    hint.push_str(&t!(lang, "house.spot.hidden"));
                }
                Some(SelectMenuOption {
                    default: (x, y) == (state.x, state.y),
                    description: Some(hint),
                    emoji: None,
                    label: spot_label(lang, x, y, wall),
                    value: format!("{x}.{y}"),
                })
            })
            .collect(),
        _ => {
            let mut near: Vec<&PlacementRow> = placements.iter().collect();
            near.sort_by_key(|p| ((p.x as u32).abs_diff(state.x) + (p.y as u32).abs_diff(state.y), p.id));
            let clear = (!placements.is_empty()).then(|| SelectMenuOption {
                default: false,
                description: Some(t!(lang, "house.clear.option-hint", count = placements.len())),
                emoji: None,
                label: t!(lang, "house.clear.option"),
                value: CLEAR_ROOM.to_owned(),
            });
            clear
                .into_iter()
                .chain(near.into_iter().take(PAGE - 1).filter_map(|p| {
                    let item = content.item(&p.item_id)?;
                    let (layer, _) = house::footprint(content, &p.item_id, p.x as u32, p.y as u32, p.flipped)?;
                    let at = spot_label(
                        lang,
                        p.x as u32,
                        p.y as u32,
                        (layer == Layer::Wall).then_some(p.flipped),
                    );
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
    lines.push(t!(
        lang,
        "house.editor.cursor",
        spot = spot_label(lang, state.x, state.y, wall),
        item = selected_item.map_or_else(|| t!(lang, "house.editor.nothing"), |i| i.name())
    ));
    if let Some(name) = &hovered_name {
        lines.push(t!(lang, "house.editor.under", name = name));
    }
    let (heading, footer) = match &owner_name {
        Some(name) => (
            t!(
                lang,
                "house.editor.partner-title",
                name = name,
                room = state.room + 1,
                title = title
            ),
            t!(lang, "house.editor.partner-footer", name = name)
                + &if kept_back {
                    t!(lang, "house.editor.soulbound-out")
                } else {
                    String::new()
                },
        ),
        None => (
            t!(lang, "house.editor.title", room = state.room + 1, title = title),
            t!(lang, "house.editor.footer"),
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
        options.push(option(&t!(lang, "house.editor.prev-items"), PREV_PAGE, None, false));
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
                        "x{} · {}{} · {iw}×{id}",
                        row.quantity,
                        if item.bound {
                            t!(lang, "house.editor.soulbound")
                        } else {
                            String::new()
                        },
                        item.base.category.label()
                    )),
                    state.item.as_deref() == Some(row.item_id.as_str()),
                ))
            }),
    );
    if state.page + 1 < pages {
        options.push(option(&t!(lang, "house.editor.more-items"), NEXT_PAGE, None, false));
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
            t!(lang, "house.editor.inventory-empty-hint")
        } else if pages > 1 {
            t!(lang, "house.editor.choose-page", page = state.page + 1, pages = pages)
        } else {
            t!(lang, "house.editor.choose")
        },
        &t!(lang, "house.editor.inventory-empty"),
    );
    let jump_placeholder = match (selected_item.is_some(), jump_options.is_empty()) {
        (true, true) => t!(lang, "house.editor.no-spot"),
        (true, false) => t!(lang, "house.editor.go-to-spot", count = spots.len()),
        (false, true) => t!(lang, "house.editor.room-empty"),
        (false, false) => t!(lang, "house.editor.go-to-thing"),
    };
    let jump_menu = select(
        "j",
        jump_options,
        jump_placeholder,
        &t!(lang, "house.editor.nothing-here"),
    );
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
            button(&mode.label(lang), &format!("m{}", mode.code()), style, holding)
        })
        .collect();
    let label = |key: &str| t!(lang, &format!("house.editor.{key}"));
    modes.push(button(
        &label("next-room"),
        "n",
        ButtonStyle::Secondary,
        rooms.len() < 2,
    ));
    modes.push(button(&label("undo"), "u", ButtonStyle::Secondary, history.is_empty()));
    let components = vec![
        row(vec![inventory_menu]),
        row(vec![jump_menu]),
        row(vec![
            button("↖", "ul", ButtonStyle::Secondary, false),
            button("↗", "ur", ButtonStyle::Secondary, false),
            button("↙", "dl", ButtonStyle::Secondary, false),
            button("↘", "dr", ButtonStyle::Secondary, false),
            button(&label("turn"), "t", ButtonStyle::Secondary, !holding),
        ]),
        row(vec![
            button(&label("place"), "p", ButtonStyle::Success, !fits),
            button(
                &label("move"),
                "v",
                ButtonStyle::Primary,
                holding || hovered_id.is_none() || who.is_guest(),
            ),
            if holding {
                button(&label("cancel"), "c", ButtonStyle::Secondary, false)
            } else {
                button(&label("store"), "k", ButtonStyle::Danger, hovered_id.is_none())
            },
            button(&label("flip"), "f", ButtonStyle::Secondary, hovered_id.is_none()),
            button(&label("done"), "x", ButtonStyle::Primary, false),
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
            .reply_ephemeral(t!(cx.lang(), "house.upgrade.finest", house = current.name))
            .await;
    }
    let next = house::tier(house_row.tier + 1);
    let balance = cx.app.db.user(user_id).await?.credits;
    let embed = EmbedBuilder::new()
        .color(BRAND)
        .title(t!(cx.lang(), "house.upgrade.title", house = next.name))
        .description(t!(
            cx.lang(),
            "house.upgrade.offer",
            from = current.rooms,
            to = next.rooms,
            width = next.width,
            depth = next.depth,
            price = thousands_in(cx.lang(), next.price),
            balance = thousands_in(cx.lang(), balance)
        ))
        .build();
    let button = Component::Button(Button {
        custom_id: Some(format!("hu:{}", house_row.tier)),
        disabled: balance < next.price,
        emoji: None,
        id: None,
        label: Some(t!(
            cx.lang(),
            "house.upgrade.button",
            price = thousands_in(cx.lang(), next.price)
        )),
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
        UpgradeOutcome::Upgraded { balance, .. } => t!(
            cx.lang(),
            "house.upgrade.done",
            house = next.name,
            count = new_rooms.len(),
            balance = thousands_in(cx.lang(), balance)
        ),
        UpgradeOutcome::Insufficient { balance } => t!(
            cx.lang(),
            "house.upgrade.short",
            price = thousands_in(cx.lang(), next.price),
            balance = thousands_in(cx.lang(), balance)
        ),
        UpgradeOutcome::WrongTier { .. } => t!(cx.lang(), "house.upgrade.changed"),
    };
    let _ = cx
        .app
        .cache
        .publish_invalidation(pinchora_cache::Invalidation::House(user_id))
        .await;
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

    #[tokio::test]
    async fn storing_a_support_and_its_items_refuses_a_stale_group_atomically() {
        let Ok(url) = std::env::var("PINCHORA_TEST_DATABASE_URL") else {
            return;
        };
        crate::fixture_isolation::shared().await;
        let db = pinchora_db::Db::connect(&url, 5).await.unwrap();
        db.migrate().await.unwrap();
        let owner = (std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
            >> 4) as u64;
        let content = Content::get();
        db.house(
            owner,
            &NewRoom {
                room_type: "living".into(),
                wallpaper: content.surfaces.wallpapers[0].id.clone(),
                floor: content.surfaces.floors[0].id.clone(),
            },
        )
        .await
        .unwrap();
        for item in ["crumbcorner_table__moss", "pagepair_books__moss"] {
            db.grant_item(owner, item, 1).await.unwrap();
            assert!(matches!(
                db.place_item(owner, 0, item, 1, 1, false, |_| Ok(())).await.unwrap(),
                PlaceOutcome::Placed { .. }
            ));
        }
        let before = db.placements(owner, 0).await.unwrap();
        let support = before.iter().find(|p| p.item_id == "crumbcorner_table__moss").unwrap();
        assert_eq!(standing_on(content, &before, support).len(), 1);
        sqlx::query("UPDATE placements SET flipped=NOT flipped WHERE id=$1")
            .bind(support.id)
            .execute(db.pool())
            .await
            .unwrap();
        assert!(
            take_back(&db, content, Who::own(owner), (8, 8), &before, support)
                .await
                .unwrap()
                .is_none()
        );
        assert_eq!(db.placements(owner, 0).await.unwrap().len(), 2);
        assert!(db.inventory(owner).await.unwrap().iter().all(|r| r.quantity == 0));
        let current = db.placements(owner, 0).await.unwrap();
        let support = current.iter().find(|p| p.item_id == "crumbcorner_table__moss").unwrap();
        assert_eq!(
            take_back(&db, content, Who::own(owner), (8, 8), &current, support)
                .await
                .unwrap()
                .unwrap()
                .len(),
            2
        );
        assert!(db.placements(owner, 0).await.unwrap().is_empty());
        assert!(db.inventory(owner).await.unwrap().iter().all(|r| r.quantity == 1));
    }

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
        let flat = |_: &str| 0;
        assert_eq!(
            settle(content, tier, &placements, &mut state, &flat, false),
            Settle::Moved
        );
        assert_eq!((state.x + state.y).abs_diff(6), 1);
        assert_eq!(
            settle(content, tier, &placements, &mut state, &flat, false),
            Settle::Stayed
        );
        let spots = free_spots(content, tier, &placements, &state, &flat);
        assert_eq!(spots.first(), Some(&(state.x, state.y)));
        assert!(!spots.contains(&(3, 3)) && !spots.contains(&(2, 3)));
        assert_eq!(spot_label(Lang::En, 2, 3, None), "C4");
        assert_eq!(spot_label(Lang::En, 0, 1, Some(true)), "Left wall 1 · middle");
        assert_eq!(spot_label(Lang::Tr, 0, 1, Some(true)), "Sol duvar 1 · orta");
    }

    /// A small lamp, a tall bed and the tier-1 room, for the tests of what the jump avoids.
    fn lamp_and_bed() -> (&'static Content, &'static house::Tier, String, String, PlacementRow) {
        let content = Content::get();
        let find = |layer: Layer, footprint: [u32; 2]| {
            content
                .items()
                .find(|i| i.base.layer == layer && i.base.footprint == footprint)
                .map(|i| i.id())
                .expect("content has this kind of item")
        };
        let (lamp, bed) = (find(Layer::Surface, [1, 1]), find(Layer::Floor, [2, 2]));
        // The bed stands right in front of the room's back corner.
        let bed_row = PlacementRow {
            id: 1,
            room_idx: 0,
            item_id: bed.clone(),
            x: 0,
            y: 1,
            flipped: false,
        };
        (content, house::tier(1), lamp, bed, bed_row)
    }

    fn holding(item: &str, x: u32, y: u32) -> EditState {
        EditState {
            prefix: "he",
            room: 0,
            x,
            y,
            page: 0,
            flipped: false,
            mode: CursorMode::Floor,
            item: Some(item.to_owned()),
        }
    }

    #[test]
    fn a_picked_lamp_is_not_put_where_a_bed_hides_it() {
        let (content, tier, lamp, bed, bed_row) = lamp_and_bed();
        // The lamp's art is 20 px tall, the bed's 120: the bed in front of A1 covers the lamp there.
        let tall = {
            let bed = bed.clone();
            move |id: &str| if id == bed { 120 } else { 20 }
        };
        let placements = [bed_row];
        let placed = placed_list(&placements);
        assert!(
            house::is_hidden(content, &placed, &tall, &lamp, 0, 0, false),
            "A1 is behind the bed"
        );
        assert!(!house::is_hidden(content, &placed, &tall, &lamp, 5, 0, false));

        // The cursor starts on A1, which is free: the lamp still moves to the nearest spot in sight.
        let mut state = holding(&lamp, 0, 0);
        assert!(
            free_spots(content, tier, &placements, &state, &tall).contains(&(0, 0)),
            "A1 is free"
        );
        assert_eq!(
            settle(content, tier, &placements, &mut state, &tall, false),
            Settle::Moved
        );
        let (x, y) = (state.x, state.y);
        assert!(
            !house::is_hidden(content, &placed, &tall, &lamp, x, y, false),
            "{} is in sight",
            spot_label(Lang::En, x, y, None)
        );
        let spots = free_spots(content, tier, &placements, &state, &tall);
        let distance = |&(sx, sy): &(u32, u32)| sx.abs_diff(0) + sy.abs_diff(0);
        let nearest_seen = spots
            .iter()
            .filter(|&&(sx, sy)| !house::is_hidden(content, &placed, &tall, &lamp, sx, sy, false))
            .map(distance)
            .min();
        assert_eq!(Some(x + y), nearest_seen, "and it is the nearest one that is");
        // The menu lists what shows before what is hidden.
        let first_hidden = spots
            .iter()
            .position(|&(sx, sy)| house::is_hidden(content, &placed, &tall, &lamp, sx, sy, false));
        if let Some(at) = first_hidden {
            assert!(
                spots[at..]
                    .iter()
                    .all(|&(sx, sy)| house::is_hidden(content, &placed, &tall, &lamp, sx, sy, false)),
                "no spot in sight after a hidden one"
            );
        }

        // Turning an item keeps it where the player put it as long as it fits.
        let mut turned = holding(&lamp, 0, 0);
        turned.flipped = true;
        assert_eq!(
            settle(content, tier, &placements, &mut turned, &tall, true),
            Settle::Stayed
        );
        assert_eq!((turned.x, turned.y), (0, 0));
    }

    #[test]
    fn when_every_spot_is_hidden_any_free_one_will_do() {
        let (content, tier, lamp, _, _) = lamp_and_bed();
        let tall = |id: &str| if id == lamp { 20 } else { 400 };
        let block = content
            .items()
            .find(|i| i.base.layer == Layer::Floor && i.base.footprint == [1, 1])
            .map(|i| i.id())
            .expect("a 1×1 floor item");
        // Tall pieces fill every row but the back one, so what is left is all behind them.
        let placements: Vec<PlacementRow> = (1..tier.depth)
            .flat_map(|y| (0..tier.width).map(move |x| (x, y)))
            .enumerate()
            .map(|(n, (x, y))| PlacementRow {
                id: n as i64 + 1,
                room_idx: 0,
                item_id: block.clone(),
                x: x as i16,
                y: y as i16,
                flipped: false,
            })
            .collect();
        let placed = placed_list(&placements);
        let mut state = holding(&lamp, 0, 3);
        let spots = free_spots(content, tier, &placements, &state, &tall);
        assert!(!spots.is_empty() && spots.iter().all(|&(_, y)| y == 0), "{spots:?}");
        assert!(
            spots
                .iter()
                .any(|&(x, y)| house::is_hidden(content, &placed, &tall, &lamp, x, y, false)),
            "the back row is behind the tall pieces"
        );
        // Nothing shows well, yet the lamp still goes to a free spot instead of being refused.
        assert_eq!(
            settle(content, tier, &placements, &mut state, &tall, false),
            Settle::Moved
        );
        assert!(spots.contains(&(state.x, state.y)));
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
