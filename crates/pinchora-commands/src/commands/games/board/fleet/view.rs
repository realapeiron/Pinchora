use super::*;

pub(super) fn ship_views(fleet: &Fleet, sea: &Sea) -> Vec<ShipView> {
    (0..SHIPS.len())
        .map(|ship| ShipView {
            cell: fleet.cells_of(ship)[0],
            horizontal: fleet.ships[ship].horizontal,
            ship,
            sunk: sea.is_sunk(fleet, ship),
        })
        .collect()
}

/// The ships the public radars show: only the ones that have gone down, which every shot at them already
/// gave away. A fleet not yet sunk, and every fleet before the battle, shows nothing.
pub(super) fn radar_ships(state: &State) -> [Vec<ShipView>; 2] {
    std::array::from_fn(|side| match (&state.fleets[side], state.battle) {
        (Some(fleet), true) => ship_views(fleet, &state.seas[side])
            .into_iter()
            .filter(|s| s.sunk)
            .collect(),
        _ => Vec::new(),
    })
}

/// The public message in `lang`: both radars, whose turn it is and the menus to fire (or the button to place a fleet).
pub(super) async fn public(
    app: &App,
    lang: Lang,
    session: &GameSession,
    names: &[String; 2],
    state: &State,
    show: &Show<'_>,
) -> anyhow::Result<InteractionResponseData> {
    let Show { end, aim, .. } = *show;
    let ids = players(session);
    // What each radar shows: a captain's waters with the shots into them and the ships that have gone down.
    let shots = [state.seas[0].encode(), state.seas[1].encode()];
    let ships = radar_ships(state);
    let afloat: [usize; 2] = std::array::from_fn(|side| {
        state.fleets[side]
            .as_ref()
            .map_or(SHIPS.len(), |fleet| state.seas[side].afloat(fleet).len())
    });
    let turn = (state.battle && end.is_none()).then_some(state.turn as u8);
    let (status, content) = radar_messages(lang, names, state, show, ids);
    let pot = if session.bet > 0 { session.bet * 2 } else { 0 };
    let png = {
        let (status, last) = (status.clone(), state.last.map(|(sea, cell)| (sea as u8, cell)));
        let winner = end.map(|e| e.winner as u8);
        let names = names.clone();
        app.render(move |r| {
            let [ships0, ships1] = ships;
            let view = |side: usize, ships: Vec<ShipView>| SeaView {
                name: &names[side],
                shots: &shots[side],
                ships,
                afloat: afloat[side],
            };
            r.radar_card(&RadarCard {
                lang,
                seas: [view(0, ships0), view(1, ships1)],
                turn,
                winner,
                last,
                status: &status,
                pot,
                theme: Theme::reef(),
            })
        })
        .await?
    };
    let mut description = if state.battle {
        t!(lang, "seafleet.shots", n = state.shots)
    } else {
        t!(lang, "seafleet.intro")
    };
    if pot > 0 {
        description.push('\n');
        description.push_str(&t!(
            lang,
            "duel.pot-line",
            pot = thousands_in(lang, pot),
            bet = thousands_in(lang, session.bet)
        ));
    }
    let embed = EmbedBuilder::new()
        .color(if end.is_some() { SUCCESS } else { BRAND })
        .title(Play::SeaFleet.name(lang))
        .description(description);
    let components = match end {
        Some(_) => Vec::new(),
        None if state.battle => battle_menus(lang, session.id, state, aim),
        None => vec![
            Component::ActionRow(ActionRow {
                id: None,
                components: vec![button(
                    format!("bs:o:{}", session.id),
                    &t!(lang, "seafleet.place"),
                    ButtonStyle::Primary,
                    false,
                )],
            }),
            resign_button(lang, "bs", session.id),
        ],
    };
    let mut data = Cx::card(embed, "seafleet.png", png, components, false);
    data.content = Some(content);
    data.allowed_mentions = quiet();
    Ok(data)
}

/// The menus to fire: a column, then (once picked) a row, and the buttons under them.
pub(super) fn battle_menus(lang: Lang, id: i64, state: &State, aim: Option<usize>) -> Vec<Component> {
    let sea = &state.seas[1 - state.turn];
    let open = |col: usize, row: usize| sea.shot(row * SIZE + col).is_none();
    let columns = (0..SIZE)
        .filter(|&col| (0..SIZE).any(|row| open(col, row)))
        .map(|col| {
            option(
                t!(lang, "seafleet.column", letter = char::from(b'A' + col as u8)),
                col.to_string(),
                None,
                aim == Some(col),
            )
        })
        .collect();
    let mut rows = vec![select_row(
        format!("bs:a:{id}:{}", state.shots),
        &t!(lang, "seafleet.pick-column"),
        columns,
        (1, 1),
    )];
    if let Some(col) = aim {
        let options = (0..SIZE)
            .filter(|&row| open(col, row))
            .map(|row| option(rules::cell_name(row * SIZE + col), row.to_string(), None, false))
            .collect();
        rows.push(select_row(
            format!("bs:f:{id}:{}:{col}", state.shots),
            &t!(lang, "seafleet.pick-row", letter = char::from(b'A' + col as u8)),
            options,
            (1, 1),
        ));
    }
    rows.push(Component::ActionRow(ActionRow {
        id: None,
        components: vec![
            button(
                format!("bs:m:{id}"),
                &t!(lang, "seafleet.my-fleet"),
                ButtonStyle::Secondary,
                false,
            ),
            button(
                format!("bs:q:{id}"),
                &t!(lang, "board.resign"),
                ButtonStyle::Danger,
                false,
            ),
        ],
    }));
    rows
}

/// The private placement message for a captain's layout, with the buttons to draw another or lock it in.
/// The buttons carry the revision of the layout and nothing else of it.
pub(super) async fn placement(
    app: &App,
    lang: Lang,
    id: i64,
    name: &str,
    fleet: &Fleet,
    rev: i64,
) -> anyhow::Result<InteractionResponseData> {
    let sea = Sea::new();
    let shots = sea.encode();
    let ships = ship_views(fleet, &sea);
    // The card draws its title, subtitle and footer in capitals.
    let (title, subtitle, name) = (
        lang.upper(&t!(lang, "seafleet.place-title")),
        lang.upper(&t!(lang, "seafleet.layout", rev = rev)),
        name.to_owned(),
    );
    let footer = lang.upper(&t!(lang, "seafleet.place-footer"));
    let png = app
        .render(move |r| {
            r.fleet_card(&FleetCard {
                lang,
                title: &title,
                subtitle: &subtitle,
                sea: SeaView {
                    name: &name,
                    shots: &shots,
                    ships,
                    afloat: SHIPS.len(),
                },
                sunk: [false; 5],
                footer: &footer,
                last: None,
                theme: Theme::reef(),
            })
        })
        .await?;
    let rows = vec![Component::ActionRow(ActionRow {
        id: None,
        components: vec![
            button(
                format!("bs:v:{id}:{rev}"),
                &t!(lang, "seafleet.shuffle"),
                ButtonStyle::Secondary,
                false,
            ),
            button(
                format!("bs:l:{id}:{rev}"),
                &t!(lang, "seafleet.lock"),
                ButtonStyle::Success,
                false,
            ),
        ],
    })];
    let embed = EmbedBuilder::new()
        .color(BRAND)
        .title(t!(lang, "seafleet.place-title"))
        .description(t!(lang, "seafleet.place-desc"));
    Ok(Cx::card(embed, "fleet.png", png, rows, true))
}

/// Draws a new layout for a captain and shows it.
pub(super) async fn draw(
    app: &App,
    lang: Lang,
    session: &GameSession,
    names: &[String; 2],
    me: usize,
) -> anyhow::Result<InteractionResponseData> {
    let fleet = random_fleet();
    let rev = app
        .db
        .set_fleet_draft(session.id, players(session)[me], &fleet.encode())
        .await?;
    placement(app, lang, session.id, &names[me], &fleet, rev).await
}

fn radar_messages(lang: Lang, names: &[String; 2], state: &State, show: &Show<'_>, ids: [u64; 2]) -> (String, String) {
    let Show {
        end, deadline, note, ..
    } = *show;
    let status = match end {
        Some(end) => t!(
            lang,
            "board.status-won",
            name = tag(&names[end.winner]),
            how = end.how.text(lang, names)
        ),
        None if state.battle => t!(lang, "seafleet.status-fire", name = tag(&names[state.turn])),
        None => {
            let ready = |i: usize| {
                if state.fleets[i].is_some() {
                    t!(lang, "seafleet.ready")
                } else {
                    t!(lang, "seafleet.placing")
                }
            };
            t!(
                lang,
                "seafleet.status-placing",
                first = tag(&names[0]),
                first_state = ready(0),
                second = tag(&names[1]),
                second_state = ready(1)
            )
        }
    };
    // The card draws it in capitals.
    let status = lang.upper(&status);
    let content = match end {
        Some(end) => t!(
            lang,
            "board.won",
            winner = mention(ids[end.winner]),
            how = end.how.text(lang, names),
            note = note
        ),
        None if state.battle => t!(
            lang,
            "seafleet.turn",
            host = mention(ids[0]),
            guest = mention(ids[1]),
            fires = mention(ids[state.turn]),
            when = relative(deadline),
            note = note
        ),
        None => t!(
            lang,
            "seafleet.placing-turn",
            host = mention(ids[0]),
            guest = mention(ids[1]),
            when = relative(deadline),
            note = note
        ),
    };
    (status, content)
}
