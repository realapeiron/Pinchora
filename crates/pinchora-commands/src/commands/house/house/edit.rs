use super::*;

/// Where the cursor sits while no furniture is held: on the floor, or on one of the two walls
/// so hung items can be picked up and flipped. A held item decides this itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CursorMode {
    Floor,
    RightWall,
    LeftWall,
}

impl CursorMode {
    pub(super) const ALL: [Self; 3] = [Self::Floor, Self::RightWall, Self::LeftWall];

    /// The single character the mode has in a custom id.
    pub(super) fn code(self) -> char {
        match self {
            Self::Floor => 'f',
            Self::RightWall => 'r',
            Self::LeftWall => 'l',
        }
    }

    pub(super) fn from_code(code: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|m| code.len() == 1 && code.starts_with(m.code()))
    }

    pub(super) fn label(self, lang: Lang) -> String {
        match self {
            Self::Floor => t!(lang, "house.editor.floor"),
            Self::RightWall => t!(lang, "house.editor.right-wall"),
            Self::LeftWall => t!(lang, "house.editor.left-wall"),
        }
    }

    /// `Some(flipped)` on a wall, as in [`EditState::wall`]; `None` on the floor.
    pub(super) fn wall(self) -> Option<bool> {
        match self {
            Self::Floor => None,
            Self::RightWall => Some(false),
            Self::LeftWall => Some(true),
        }
    }

    pub(super) fn from_wall(wall: Option<bool>) -> Self {
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
    pub(super) fn is_guest(self) -> bool {
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
    pub(super) fn id(&self, action: &str) -> String {
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
    pub(super) fn wall(&self, content: &Content) -> Option<bool> {
        let Some(item_id) = self.item.as_deref() else {
            return self.mode.wall();
        };
        let item = content.item(item_id)?;
        (item.base.layer == Layer::Wall).then_some(self.flipped)
    }

    /// Puts the held item down; the cursor stays where it was, on its wall or the floor.
    pub(super) fn release(&mut self, content: &Content) {
        self.mode = CursorMode::from_wall(self.wall(content));
        self.item = None;
    }

    pub(super) fn parse(prefix: &'static str, args: &mut Split<'_, char>) -> Option<(String, Self)> {
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
pub(super) fn step_cursor(action: &str, (x, y): (u32, u32), left_wall: bool, (max_x, max_y): (u32, u32)) -> (u32, u32) {
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
pub(super) fn under_cursor<'a>(
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
pub(super) const PREV_PAGE: &str = "<prev>";
pub(super) const NEXT_PAGE: &str = "<next>";
/// The jump menu's first option with an empty hand: asks to store everything in the room.
pub(super) const CLEAR_ROOM: &str = "<clear>";

pub(super) fn placed_list(placements: &[PlacementRow]) -> Vec<Placed<'_>> {
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
pub(super) fn reach(content: &Content, tier: &house::Tier, state: &EditState) -> (u32, u32) {
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
pub(super) fn free_spots(
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
pub(super) enum Settle {
    Stayed,
    Moved,
    NoRoom,
}

/// Leaves the held item where it is when it fits there and shows, else moves it to the nearest spot
/// that fits and shows, so a picked item is always one **Place** away from being in the room. A spot
/// hidden behind a taller piece is the last choice, taken when nothing else fits. `keep_hidden` keeps an
/// item that fits where it is even then: the player put it there, and only a turn is changing the fit.
pub(super) fn settle(
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
pub(super) fn spot_label(lang: Lang, x: u32, y: u32, wall: Option<bool>) -> String {
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
pub(super) fn spot_hint(
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
pub(super) async fn take_back(
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
pub(super) async fn clear_prompt(
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
