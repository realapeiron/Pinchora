use super::*;

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

    let preview = editor_preview(cx, &state, tier, &placements);
    let jump_options = jump_options(cx, &state, &placements, &preview);

    let title = room_title(content, &room);
    let png = preview_image(cx, &state, tier, room, placements, &preview).await?;
    let embed = editor_embed(
        lang,
        &state,
        &preview,
        EditorCaption {
            note,
            owner_name,
            kept_back,
            title,
        },
    );
    let inventory_menu = inventory_menu(lang, &state, &inventory, per_page, pages);
    let jump_menu = jump_menu(lang, &state, &preview, jump_options);
    let components = editor_controls(
        lang,
        &state,
        &preview,
        EditorControls {
            who,
            rooms: rooms.len(),
            undo_empty: history.is_empty(),
            inventory_menu,
            jump_menu,
        },
    );
    Ok(Cx::card(embed, "room.png", png, components, true))
}
struct EditorPreview {
    selected_item: Option<pinchora_game::Item<'static>>,
    wall: Option<bool>,
    held: Option<(Layer, house::Rect)>,
    cw: u32,
    cd: u32,
    spots: Vec<(u32, u32)>,
    fits: bool,
    hovered_id: Option<i64>,
    hovered_name: Option<String>,
}

fn editor_preview(cx: &Cx, state: &EditState, tier: &house::Tier, placements: &[PlacementRow]) -> EditorPreview {
    let content = Content::get();
    let selected_item = state.item.as_deref().and_then(|i| content.item(i));
    let wall = state.wall(content);
    let held = state
        .item
        .as_deref()
        .and_then(|i| house::footprint(content, i, 0, 0, state.flipped));
    let (cw, cd) = held.map_or((1, 1), |(_, r)| (r.w, r.d));
    let heights = |id: &str| cx.app.sprites.item(content, id).map_or(0, |sprite| sprite.height());
    let spots = free_spots(content, tier, placements, state, &heights);
    let fits = selected_item.is_some() && spots.contains(&(state.x, state.y));
    let hovered = under_cursor(content, placements, state.x, state.y, wall);
    let hovered_id = hovered.map(|p| p.id);
    let hovered_name = hovered.and_then(|p| content.item(&p.item_id)).map(|i| i.name());

    EditorPreview {
        selected_item,
        wall,
        held,
        cw,
        cd,
        spots,
        fits,
        hovered_id,
        hovered_name,
    }
}

fn jump_options(
    cx: &Cx,
    state: &EditState,
    placements: &[PlacementRow],
    preview: &EditorPreview,
) -> Vec<SelectMenuOption> {
    let content = Content::get();
    let lang = cx.lang();
    let heights = |id: &str| cx.app.sprites.item(content, id).map_or(0, |sprite| sprite.height());
    // The jump menu: free spots for the held piece, or the pieces in the room for an empty hand.
    let on_floor = placed_list(placements);
    match (state.item.as_deref(), preview.held) {
        (Some(item_id), Some((layer, _))) => preview
            .spots
            .iter()
            .take(PAGE)
            .filter_map(|&(x, y)| {
                let (_, r) = house::footprint(content, item_id, x, y, state.flipped)?;
                let mut hint = spot_hint(lang, content, placements, layer, r, preview.wall);
                if house::is_hidden(content, &on_floor, &heights, item_id, x, y, state.flipped) {
                    hint.push_str(&t!(lang, "house.spot.hidden"));
                }
                Some(SelectMenuOption {
                    default: (x, y) == (state.x, state.y),
                    description: Some(hint),
                    emoji: None,
                    label: spot_label(lang, x, y, preview.wall),
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
                        default: preview.hovered_id == Some(p.id),
                        description: Some(at),
                        emoji: None,
                        label: item.name().chars().take(100).collect(),
                        value: format!("#{}", p.id),
                    })
                }))
                .collect()
        }
    }
}

async fn preview_image(
    cx: &Cx,
    state: &EditState,
    tier: &house::Tier,
    room: RoomRow,
    placements: Vec<PlacementRow>,
    preview: &EditorPreview,
) -> anyhow::Result<Vec<u8>> {
    // The held piece is drawn where it would go, so the picture shows the room as it will look.
    let mut shown = placements;
    if let Some(item_id) = state.item.as_deref().filter(|_| preview.held.is_some()) {
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
                w: preview.cw,
                d: preview.cd,
                wall: preview
                    .wall
                    .map(|flipped| if flipped { WallSide::Left } else { WallSide::Right }),
                ok: preview.selected_item.is_none() || preview.fits,
            }),
            selected: preview.hovered_id,
            edit: true,
            pets: Vec::new(),
            characters: Vec::new(),
            seed: 0,
        },
    )
    .await?;

    Ok(png)
}

struct EditorCaption<'a> {
    note: &'a str,
    owner_name: Option<String>,
    kept_back: bool,
    title: String,
}

fn editor_embed(lang: Lang, state: &EditState, preview: &EditorPreview, caption: EditorCaption<'_>) -> EmbedBuilder {
    let EditorCaption {
        note,
        owner_name,
        kept_back,
        title,
    } = caption;
    let mut lines = Vec::new();
    if !note.is_empty() {
        lines.push(note.to_owned());
    }
    lines.push(t!(
        lang,
        "house.editor.cursor",
        spot = spot_label(lang, state.x, state.y, preview.wall),
        item = preview
            .selected_item
            .as_ref()
            .map_or_else(|| t!(lang, "house.editor.nothing"), |i| i.name())
    ));
    if let Some(name) = &preview.hovered_name {
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
    EmbedBuilder::new()
        .color(if preview.fits {
            SUCCESS
        } else if preview.selected_item.is_some() {
            WARN
        } else {
            BRAND
        })
        .title(heading)
        .description(lines.join("\n"))
        .footer(EmbedFooterBuilder::new(footer))
}

fn inventory_option(label: &str, value: &str, description: Option<String>, default: bool) -> SelectMenuOption {
    SelectMenuOption {
        default,
        description,
        emoji: None,
        label: label.chars().take(100).collect(),
        value: value.to_owned(),
    }
}

fn editor_select(
    state: &EditState,
    action: &str,
    options: Vec<SelectMenuOption>,
    placeholder: String,
    empty: &str,
) -> Component {
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
            vec![inventory_option(empty, "none", None, false)]
        } else {
            options
        }),
        placeholder: Some(placeholder),
        required: None,
    })
}

fn inventory_menu(
    lang: Lang,
    state: &EditState,
    inventory: &[pinchora_db::InventoryRow],
    per_page: usize,
    pages: usize,
) -> Component {
    let content = Content::get();
    let mut options: Vec<SelectMenuOption> = Vec::new();
    if state.page > 0 {
        options.push(inventory_option(
            &t!(lang, "house.editor.prev-items"),
            PREV_PAGE,
            None,
            false,
        ));
    }
    options.extend(
        inventory
            .iter()
            .skip(state.page * per_page)
            .take(per_page)
            .filter_map(|row| {
                let item = content.item(&row.item_id)?;
                let [iw, id] = item.base.footprint;
                Some(inventory_option(
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
        options.push(inventory_option(
            &t!(lang, "house.editor.more-items"),
            NEXT_PAGE,
            None,
            false,
        ));
    }
    editor_select(
        state,
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
    )
}

fn jump_menu(lang: Lang, state: &EditState, preview: &EditorPreview, jump_options: Vec<SelectMenuOption>) -> Component {
    let jump_placeholder = match (preview.selected_item.is_some(), jump_options.is_empty()) {
        (true, true) => t!(lang, "house.editor.no-spot"),
        (true, false) => t!(lang, "house.editor.go-to-spot", count = preview.spots.len()),
        (false, true) => t!(lang, "house.editor.room-empty"),
        (false, false) => t!(lang, "house.editor.go-to-thing"),
    };
    editor_select(
        state,
        "j",
        jump_options,
        jump_placeholder,
        &t!(lang, "house.editor.nothing-here"),
    )
}

struct EditorControls {
    who: Who,
    rooms: usize,
    undo_empty: bool,
    inventory_menu: Component,
    jump_menu: Component,
}

fn editor_controls(lang: Lang, state: &EditState, preview: &EditorPreview, controls: EditorControls) -> Vec<Component> {
    let EditorControls {
        who,
        rooms,
        undo_empty,
        inventory_menu,
        jump_menu,
    } = controls;
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
    let holding = preview.selected_item.is_some();
    let mut modes: Vec<Component> = CursorMode::ALL
        .into_iter()
        .map(|mode| {
            let style = if CursorMode::from_wall(preview.wall) == mode {
                ButtonStyle::Primary
            } else {
                ButtonStyle::Secondary
            };
            button(&mode.label(lang), &format!("m{}", mode.code()), style, holding)
        })
        .collect();
    let label = |key: &str| t!(lang, &format!("house.editor.{key}"));
    modes.push(button(&label("next-room"), "n", ButtonStyle::Secondary, rooms < 2));
    modes.push(button(&label("undo"), "u", ButtonStyle::Secondary, undo_empty));
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
            button(&label("place"), "p", ButtonStyle::Success, !preview.fits),
            button(
                &label("move"),
                "v",
                ButtonStyle::Primary,
                holding || preview.hovered_id.is_none() || who.is_guest(),
            ),
            if holding {
                button(&label("cancel"), "c", ButtonStyle::Secondary, false)
            } else {
                button(&label("store"), "k", ButtonStyle::Danger, preview.hovered_id.is_none())
            },
            button(
                &label("flip"),
                "f",
                ButtonStyle::Secondary,
                preview.hovered_id.is_none(),
            ),
            button(&label("done"), "x", ButtonStyle::Primary, false),
        ]),
        row(modes),
    ];
    components
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
