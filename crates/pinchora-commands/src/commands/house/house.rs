mod editor;
pub(crate) use editor::{editor, editor_as};
mod actions;
mod edit;
pub(crate) use actions::on_edit;
use edit::*;
pub(crate) use edit::{CursorMode, EditState};
pub use edit::{Who, standing_on, undo_fits};

use std::str::Split;

use chrono::{DateTime, Utc};
use pinchora_core::games::items::is_soulbound;
use pinchora_core::infra::i18n::Lang;
use pinchora_core::t;
use pinchora_db::{
    HouseRow, NewRoom, PetRow, PlaceOutcome, PlacementRow, Refusal, RestoreOutcome, RoomChange, RoomRow,
    UpgradeOutcome, VisitOutcome,
};
use pinchora_game::Content;
use pinchora_game::Layer;
use pinchora_game::character::Character;
use pinchora_game::house;
use pinchora_game::house::Placed;
use pinchora_game::house::ROOM_ORDER;
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

use crate::App;
use crate::Cx;
use crate::commands::social::pet::is_asleep;
use crate::commands::undo;
use crate::commands::undo::History;
use crate::commands::undo::Kind;
use crate::commands::undo::Piece;
use crate::commands::undo::Step;
use crate::games::game::scene::RoomPet;
use crate::games::game::scene::RoomView;
use crate::games::game::scene::render_view;
use crate::infra::util::BRAND;
use crate::infra::util::DANGER;
use crate::infra::util::SUCCESS;
use crate::infra::util::WARN;
use crate::infra::util::resolved_display_name;
use crate::infra::util::thousands_in;

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
    Guestbook(crate::commands::community::guestbook::HouseGuestbook),
    #[command(name = "contest")]
    Contest(crate::commands::community::contest::HouseContest),
    #[command(name = "aquarium")]
    Aquarium(crate::commands::games::reef_loop::Aquarium),
    #[command(name = "garden")]
    Garden(crate::commands::games::reef_loop::Garden),
    #[command(name = "craft")]
    Craft(crate::commands::games::reef_loop::Craft),
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
                None => crate::commands::social::friends::visit_shortlist(cx, room_index(c.room)).await,
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
                let data = crate::commands::house::decorate::panel(cx, room_index(c.room), 0, "").await?;
                cx.reply(data).await
            }
            Self::Guestbook(c) => crate::commands::community::guestbook::run(cx, c).await,
            Self::Contest(c) => c.run(cx).await,
            Self::Like(c) => {
                let host = &c.user.resolved;
                if host.bot || host.id == author.id {
                    return cx.reply_ephemeral(t!(cx.lang(), "house.like-self")).await;
                }
                if crate::commands::admin::privacy::private_house(cx, host.id.get()).await? {
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
                                liker = crate::infra::util::mention(author.id),
                                host = crate::infra::util::mention(host.id),
                                count = total
                            )),
                            allowed_mentions: crate::commands::social::social::ping_only(host.id.get()),
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
        crate::commands::games::reef_loop::attach_aquarium(cx, owner_id, &mut data).await?;
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
        crate::commands::community::plaza::guild_name(cx, owner).await
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

/// What a partner is called in the editor: the name they were last seen with.
async fn partner_label(app: &App, user: u64) -> String {
    match app.db.remembered_name(user).await {
        Ok(Some(name)) if !name.is_empty() => name,
        _ => "your partner".to_owned(),
    }
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
mod tests;
