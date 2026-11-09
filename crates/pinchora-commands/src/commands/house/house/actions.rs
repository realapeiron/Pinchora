use super::*;

struct HouseEditAction<'a> {
    cx: &'a Cx,
    who: Who,
    state: EditState,
    tier: &'a house::Tier,
    rooms: Vec<RoomRow>,
    placements: Vec<PlacementRow>,
    history: History,
    guest_of: Option<String>,
    placed_now: bool,
}

impl HouseEditAction<'_> {
    async fn select_item(&mut self, value: Option<&str>) -> anyhow::Result<Option<String>> {
        let cx = self.cx;
        let state = &mut self.state;
        let placements = &self.placements;
        let content = Content::get();
        let tier = self.tier;
        let lang = self.cx.lang();
        let heights = |id: &str| cx.app.sprites.item(content, id).map_or(0, |sprite| sprite.height());
        let note = match value {
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
                    Some(item_id) => match settle(content, tier, placements, state, &heights, false) {
                        Settle::NoRoom => t!(lang, "house.edit.fits-nowhere", item = item_name(content, &item_id)),
                        _ => t!(
                            lang,
                            "house.edit.holding",
                            item = item_name(content, &item_id),
                            spot = spot_label(lang, state.x, state.y, state.wall(content))
                        ),
                    },
                    None => t!(lang, "house.edit.pick"),
                }
            }
        };
        Ok(Some(note))
    }

    async fn jump_cursor(&mut self, value: Option<&str>) -> anyhow::Result<Option<String>> {
        let cx = self.cx;
        let state = &mut self.state;
        let placements = &self.placements;
        let rooms = &self.rooms;
        let guest_of = &self.guest_of;
        let content = Content::get();
        let tier = self.tier;
        let note = {
            match value {
                Some(CLEAR_ROOM) if !placements.is_empty() => {
                    let title = room_title(content, &rooms[state.room as usize]);
                    clear_prompt(cx, state, &title, placements.len(), guest_of.as_deref()).await?;
                    return Ok(None);
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
                            .find(|&(x, y)| under_cursor(content, placements, x, y, wall).is_some_and(|u| u.id == p.id))
                            .unwrap_or((r.x, r.y));
                    }
                }
                Some(spot) => {
                    if let Some((x, y)) = spot.split_once('.')
                        && let (Ok(x), Ok(y)) = (x.parse::<u32>(), y.parse::<u32>())
                    {
                        let (max_x, max_y) = reach(content, tier, state);
                        (state.x, state.y) = (x.min(max_x), y.min(max_y));
                    }
                }
                None => {}
            }
            String::new()
        };
        Ok(Some(note))
    }

    async fn move_cursor(&mut self, action: &str) -> anyhow::Result<Option<String>> {
        let state = &mut self.state;
        let content = Content::get();
        let tier = self.tier;
        let note = {
            let max = reach(content, tier, state);
            (state.x, state.y) = step_cursor(action, (state.x, state.y), state.wall(content) == Some(true), max);
            String::new()
        };
        Ok(Some(note))
    }

    async fn change_surface(&mut self, action: &str) -> anyhow::Result<Option<String>> {
        let state = &mut self.state;
        let content = Content::get();
        let tier = self.tier;
        let note = {
            state.mode = CursorMode::from_code(&action[1..]).unwrap_or(CursorMode::Floor);
            let (max_x, max_y) = reach(content, tier, state);
            (state.x, state.y) = (state.x.min(max_x), state.y.min(max_y));
            String::new()
        };
        Ok(Some(note))
    }

    async fn place_item(&mut self) -> anyhow::Result<Option<String>> {
        let cx = self.cx;
        let prefix = self.state.prefix;
        let who = self.who;
        let state = &mut self.state;
        let placements = &self.placements;
        let history = &mut self.history;
        let content = Content::get();
        let tier = self.tier;
        let user_id = self.who.owner;
        let lang = self.cx.lang();
        let heights = |id: &str| cx.app.sprites.item(content, id).map_or(0, |sprite| sprite.height());
        let note = match state.item.clone() {
            None => t!(lang, "house.edit.pick-first"),
            Some(item_id) => {
                let (x, y, flipped) = (state.x, state.y, state.flipped);
                let outcome = place_held(cx, who, state, tier, placements, &item_id).await?;
                let name = item_name(content, &item_id);
                match outcome {
                    PlaceOutcome::Placed { id } => {
                        self.placed_now = prefix == "he";
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
                            match settle(content, tier, &placements, state, &heights, false) {
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
        };
        Ok(Some(note))
    }

    async fn guest_move_note(&mut self) -> anyhow::Result<Option<String>> {
        let guest_of = &self.guest_of;
        let lang = self.cx.lang();
        let note = t!(
            lang,
            "house.edit.move-theirs",
            name = guest_of.as_deref().unwrap_or_default()
        );
        Ok(Some(note))
    }

    async fn lift_piece(&mut self) -> anyhow::Result<Option<String>> {
        let cx = self.cx;
        let who = self.who;
        let state = &mut self.state;
        let placements = &self.placements;
        let history = &mut self.history;
        let content = Content::get();
        let tier = self.tier;
        let lang = self.cx.lang();
        let note = match under_cursor(content, placements, state.x, state.y, state.wall(content)) {
            Some(p) => {
                let mut step = Step::new(Kind::Lift, item_name(content, &p.item_id));
                let Some(removed) =
                    take_back(&cx.app.db, content, who, (tier.width, tier.depth), placements, p).await?
                else {
                    cx.reply_ephemeral(t!(lang, "house.edit.blocked")).await?;
                    return Ok(None);
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
                t!(
                    lang,
                    "house.edit.moving",
                    item = item_name(content, &p.item_id),
                    extra = extra
                )
            }
            None => t!(lang, "house.edit.nothing-to-move"),
        };
        Ok(Some(note))
    }

    async fn store_piece(&mut self) -> anyhow::Result<Option<String>> {
        let cx = self.cx;
        let who = self.who;
        let state = &mut self.state;
        let placements = &self.placements;
        let history = &mut self.history;
        let guest_of = &self.guest_of;
        let content = Content::get();
        let tier = self.tier;
        let lang = self.cx.lang();
        let note = match under_cursor(content, placements, state.x, state.y, state.wall(content)) {
            Some(p) => {
                let mut step = Step::new(Kind::Store, item_name(content, &p.item_id));
                let Some(removed) =
                    take_back(&cx.app.db, content, who, (tier.width, tier.depth), placements, p).await?
                else {
                    cx.reply_ephemeral(t!(lang, "house.edit.blocked")).await?;
                    return Ok(None);
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
                        item = item_name(content, &p.item_id),
                        extra = extra,
                        name = owner
                    ),
                    None => t!(
                        lang,
                        "house.edit.stored",
                        item = item_name(content, &p.item_id),
                        extra = extra
                    ),
                }
            }
            None => t!(lang, "house.edit.nothing-under"),
        };
        Ok(Some(note))
    }

    async fn release_item(&mut self) -> anyhow::Result<Option<String>> {
        let state = &mut self.state;
        let content = Content::get();
        let note = {
            state.release(content);
            String::new()
        };
        Ok(Some(note))
    }

    async fn turn_item(&mut self) -> anyhow::Result<Option<String>> {
        let cx = self.cx;
        let state = &mut self.state;
        let placements = &self.placements;
        let content = Content::get();
        let tier = self.tier;
        let lang = self.cx.lang();
        let heights = |id: &str| cx.app.sprites.item(content, id).map_or(0, |sprite| sprite.height());
        let note = {
            state.flipped = !state.flipped;
            match settle(content, tier, placements, state, &heights, true) {
                Settle::Moved => t!(
                    lang,
                    "house.edit.turned-moved",
                    spot = spot_label(lang, state.x, state.y, state.wall(content))
                ),
                Settle::NoRoom => t!(lang, "house.edit.turned-no-room"),
                Settle::Stayed => String::new(),
            }
        };
        Ok(Some(note))
    }

    async fn flip_piece(&mut self) -> anyhow::Result<Option<String>> {
        let cx = self.cx;
        let who = self.who;
        let state = &mut self.state;
        let placements = &self.placements;
        let history = &mut self.history;
        let content = Content::get();
        let tier = self.tier;
        let lang = self.cx.lang();
        let note = match under_cursor(content, placements, state.x, state.y, state.wall(content)) {
            Some(p) => {
                let others: Vec<Placed> = placed_list(placements)
                    .into_iter()
                    .zip(placements)
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
                                    if undo::fingerprint(rows) == undo::fingerprint(placements) {
                                        Ok(())
                                    } else {
                                        Err("room_changed".into())
                                    }
                                },
                                |rows, change| undo_fits(content, (tier.width, tier.depth), rows, change),
                            )
                            .await?;
                        if !matches!(outcome, RestoreOutcome::Done { .. }) {
                            cx.reply_ephemeral(t!(lang, "house.edit.nothing-to-flip")).await?;
                            return Ok(None);
                        }
                        let on_wall = content.item(&p.item_id).is_some_and(|i| i.base.layer == Layer::Wall);
                        let mut step = Step::new(
                            if on_wall { Kind::Flip } else { Kind::Turn },
                            item_name(content, &p.item_id),
                        );
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
        };
        Ok(Some(note))
    }

    async fn clear_room(&mut self) -> anyhow::Result<Option<String>> {
        let cx = self.cx;
        let who = self.who;
        let state = &mut self.state;
        let placements = &self.placements;
        let rooms = &self.rooms;
        let history = &mut self.history;
        let guest_of = &self.guest_of;
        let content = Content::get();
        let tier = self.tier;
        let lang = self.cx.lang();
        let note = {
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
                        if undo::fingerprint(rows) == undo::fingerprint(placements) {
                            Ok(())
                        } else {
                            Err("room_changed".into())
                        }
                    },
                    |rows, change| undo_fits(content, (tier.width, tier.depth), rows, change),
                )
                .await?;
            if !matches!(outcome, RestoreOutcome::Done { .. }) {
                cx.reply_ephemeral(t!(lang, "house.edit.already-empty")).await?;
                return Ok(None);
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
        };
        Ok(Some(note))
    }

    async fn undo_edit(&mut self) -> anyhow::Result<Option<String>> {
        let cx = self.cx;
        let who = self.who;
        let state = &mut self.state;
        let placements = &self.placements;
        let history = &mut self.history;
        let guest_of = &self.guest_of;
        let content = Content::get();
        let tier = self.tier;
        let lang = self.cx.lang();
        let note = match history.pop() {
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
                            if undo::fingerprint(rows) == undo::fingerprint(placements) {
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
                            Refusal::Moved(item) => t!(lang, "house.edit.undo-moved", item = item_name(content, &item)),
                            Refusal::NotOwned(item) => match &guest_of {
                                Some(owner) => {
                                    t!(
                                        lang,
                                        "house.edit.undo-none-theirs",
                                        item = item_name(content, &item),
                                        name = owner
                                    )
                                }
                                None => t!(lang, "house.edit.undo-none", item = item_name(content, &item)),
                            },
                            Refusal::Blocked(reason) => reason,
                        };
                        t!(lang, "house.edit.cant-undo", step = step.describe(), why = why)
                    }
                }
            }
        };
        Ok(Some(note))
    }

    async fn next_room(&mut self) -> anyhow::Result<Option<String>> {
        let state = &mut self.state;
        let rooms = &self.rooms;
        let lang = self.cx.lang();
        let note = {
            state.room = (state.room + 1) % rooms.len() as i16;
            state.x = 1;
            state.y = 0;
            t!(lang, "house.edit.now-room", room = state.room + 1)
        };
        Ok(Some(note))
    }

    async fn close_editor(&mut self) -> anyhow::Result<Option<String>> {
        let cx = self.cx;
        let prefix = self.state.prefix;
        let lang = self.cx.lang();

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
        cx.update(data).await?;
        Ok(None)
    }
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
        "pe" => match crate::commands::community::plaza::editable_plaza(cx) {
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
    // The partner whose house this is, when it is not the player's own: some lines name them.
    let guest_of = if who.is_guest() {
        Some(partner_label(&cx.app, who.owner).await)
    } else {
        None
    };
    let (house_row, rooms) = ensure_house(&cx.app, user_id).await?;
    let tier = house::tier(house_row.tier);
    state.room = state.room.clamp(0, rooms.len().saturating_sub(1) as i16);
    let placements = cx.app.db.placements(user_id, state.room).await?;
    let start_room = state.room;
    let history = undo::load(&cx.app, who, state.room, &placements).await;
    let mut edit = HouseEditAction {
        cx,
        who,
        state,
        tier,
        rooms,
        placements,
        history,
        guest_of,
        placed_now: false,
    };
    let note = match action.as_str() {
        "s" => edit.select_item(value).await?,
        "j" => edit.jump_cursor(value).await?,
        "ul" | "dr" | "ur" | "dl" => edit.move_cursor(&action).await?,
        "mf" | "mr" | "ml" => edit.change_surface(&action).await?,
        "p" => edit.place_item().await?,
        "v" if who.is_guest() => edit.guest_move_note().await?,
        "v" => edit.lift_piece().await?,
        "k" => edit.store_piece().await?,
        "c" => edit.release_item().await?,
        "t" => edit.turn_item().await?,
        "f" => edit.flip_piece().await?,
        "z" => edit.clear_room().await?,
        "u" => edit.undo_edit().await?,
        "n" => edit.next_room().await?,
        "x" => edit.close_editor().await?,
        _ => Some(String::new()),
    };
    let Some(note) = note else {
        return Ok(());
    };
    let HouseEditAction {
        state,
        history,
        placed_now,
        ..
    } = edit;
    // A new room has its own history; the editor loads it.
    let history = (state.room == start_room).then_some(history);
    let data = editor_as(cx, who, state, &note, history).await?;
    cx.update(data).await?;
    if placed_now {
        crate::commands::social::customize::announce(cx).await;
    }
    Ok(())
}

async fn place_held(
    cx: &Cx,
    who: Who,
    state: &EditState,
    tier: &house::Tier,
    placements: &[PlacementRow],
    item_id: &str,
) -> anyhow::Result<PlaceOutcome> {
    let content = Content::get();
    let lang = cx.lang();
    let (width, depth) = (tier.width, tier.depth);
    let (x, y, flipped) = (state.x, state.y, state.flipped);
    let changes = [RoomChange::Add(PlacementRow {
        id: 0,
        room_idx: state.room,
        item_id: item_id.to_owned(),
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
                if undo::fingerprint(rows) == undo::fingerprint(placements) {
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
            if reason != "access_denied" && reason != "room_changed" && reason != "You are no longer partners." =>
        {
            PlaceOutcome::Blocked(reason)
        }
        RestoreOutcome::Refused(_) => PlaceOutcome::Blocked(t!(lang, "house.edit.blocked")),
    };
    Ok(outcome)
}

fn item_name(content: &Content, item_id: &str) -> String {
    content.item(item_id).map_or(item_id.to_owned(), |i| i.name())
}
