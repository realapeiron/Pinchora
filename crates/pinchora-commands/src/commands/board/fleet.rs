//! `/duel seafleet`: Sea Fleet, battleship for two. Each captain places a fleet in private (a random layout,
//! as often as they like, then lock it in), then they take turns firing at the other's waters from two menus
//! under the shared radar. The layout a captain is choosing is kept in the database, never in a button, and
//! locked-in fleets live in the game's state; nobody sees the other's except through shots.

use std::str::Split;

use chrono::{DateTime, Duration, Utc};
use pinchora_db::GameSession;
use pinchora_game::play::{
    Game as Play,
    fleet::{self as rules, CELLS, Fired, Fleet, SHIPS, SIZE, Sea},
};
use pinchora_render::{FleetCard, RadarCard, SeaView, ShipView, Theme};
use twilight_model::{
    channel::message::{
        Component,
        component::{ActionRow, ButtonStyle},
    },
    http::interaction::InteractionResponseData,
};
use twilight_util::builder::embed::EmbedBuilder;

use super::{
    FLEET_TURN_SECS, conclude, confirm_resign, edit_card, option, players, resign_button, resigned, select_row, tag,
};
use crate::{
    App, Cx,
    commands::{
        duel::{Kind as DuelKind, answer, expire_here},
        games::{button, location, pack, quiet, unpack},
    },
    util::{BRAND, SUCCESS, mention, relative, thousands},
};

/// The state's own fields: whether the battle has begun, whose turn it is to fire (0 host, 1 guest), each
/// captain's fleet once locked in, the shots fired into each captain's waters, the last shot as
/// `<sea>.<cell>` and the number of shots so far.
struct State {
    battle: bool,
    turn: usize,
    fleets: [Option<Fleet>; 2],
    seas: [Sea; 2],
    last: Option<(usize, usize)>,
    shots: usize,
}

impl State {
    fn new() -> Self {
        Self {
            battle: false,
            turn: 0,
            fleets: [None, None],
            seas: [Sea::new(), Sea::new()],
            last: None,
            shots: 0,
        }
    }

    fn encode(&self) -> String {
        let fleet = |f: &Option<Fleet>| f.as_ref().map_or_else(|| "-".to_owned(), Fleet::encode);
        let last = self
            .last
            .map_or_else(|| "-".to_owned(), |(sea, cell)| format!("{sea}.{cell}"));
        format!(
            "{}:{}:{}:{}:{}:{}:{last}:{}",
            if self.battle { 'b' } else { 'p' },
            self.turn,
            fleet(&self.fleets[0]),
            fleet(&self.fleets[1]),
            self.seas[0].encode(),
            self.seas[1].encode(),
            self.shots
        )
    }

    fn decode(fields: &str) -> Option<Self> {
        let mut parts = fields.split(':');
        let battle = match parts.next()? {
            "b" => true,
            "p" => false,
            _ => return None,
        };
        let turn = parts.next()?.parse().ok().filter(|t| *t < 2)?;
        let fleet = |text: &str| match text {
            "-" => Some(None),
            code => Fleet::decode(code).map(Some),
        };
        let fleets = [fleet(parts.next()?)?, fleet(parts.next()?)?];
        let seas = [Sea::decode(parts.next()?)?, Sea::decode(parts.next()?)?];
        let last = match parts.next()? {
            "-" => None,
            text => {
                let (sea, cell) = text.split_once('.')?;
                Some((
                    sea.parse().ok().filter(|s| *s < 2)?,
                    cell.parse().ok().filter(|c| *c < CELLS)?,
                ))
            }
        };
        let shots = parts.next()?.parse().ok()?;
        Some(Self {
            battle,
            turn,
            fleets,
            seas,
            last,
            shots,
        })
    }
}

pub(crate) fn initial_fields() -> String {
    State::new().encode()
}

/// Who still had to act when the game timed out: the captains with no fleet yet, or the one to fire.
pub(crate) fn slow_players(session: &GameSession) -> Vec<u64> {
    let (fields, _) = unpack(&session.state);
    let ids = players(session);
    match State::decode(fields) {
        Some(state) if state.battle => vec![ids[state.turn]],
        Some(state) => (0..2).filter(|&i| state.fleets[i].is_none()).map(|i| ids[i]).collect(),
        None => Vec::new(),
    }
}

/// How a game ended: the winner (0 host, 1 guest) and how.
struct End {
    winner: usize,
    how: String,
}

fn ship_views(fleet: &Fleet, sea: &Sea) -> Vec<ShipView> {
    (0..SHIPS.len())
        .map(|ship| ShipView {
            cell: fleet.cells_of(ship)[0],
            horizontal: fleet.ships[ship].horizontal,
            ship,
            sunk: sea.is_sunk(fleet, ship),
        })
        .collect()
}

/// How a board is shown: the end of the game if it is over, when the move is due, the column aimed at and a note to add.
#[derive(Default)]
struct Show<'a> {
    end: Option<&'a End>,
    deadline: DateTime<Utc>,
    aim: Option<usize>,
    note: &'a str,
}

/// The ships the public radars show: only the ones that have gone down, which every shot at them already
/// gave away. A fleet not yet sunk, and every fleet before the battle, shows nothing.
fn radar_ships(state: &State) -> [Vec<ShipView>; 2] {
    std::array::from_fn(|side| match (&state.fleets[side], state.battle) {
        (Some(fleet), true) => ship_views(fleet, &state.seas[side])
            .into_iter()
            .filter(|s| s.sunk)
            .collect(),
        _ => Vec::new(),
    })
}

/// The public message: both radars, whose turn it is and the menus to fire (or the button to place a fleet).
async fn public(
    app: &App,
    session: &GameSession,
    names: &[String; 2],
    state: &State,
    show: &Show<'_>,
) -> anyhow::Result<InteractionResponseData> {
    let Show {
        end,
        deadline,
        aim,
        note,
    } = *show;
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
    let status = match end {
        Some(end) => format!("{} wins by {}", tag(&names[end.winner]), end.how),
        None if state.battle => format!("{} to fire", tag(&names[state.turn])),
        None => {
            let ready = |i: usize| if state.fleets[i].is_some() { "ready" } else { "placing" };
            format!(
                "Fleets: {} {}, {} {}",
                tag(&names[0]),
                ready(0),
                tag(&names[1]),
                ready(1)
            )
        }
    };
    let content = match end {
        Some(end) => format!("🏆 {} wins by {}! {note}", mention(ids[end.winner]), end.how),
        None if state.battle => format!(
            "🚢 {} vs {}\n{} fires, {}{note}",
            mention(ids[0]),
            mention(ids[1]),
            mention(ids[state.turn]),
            relative(deadline)
        ),
        None => format!(
            "🚢 {} vs {}\nBoth captains: **place your fleet** in private, {}.\n{}",
            mention(ids[0]),
            mention(ids[1]),
            relative(deadline),
            note
        ),
    };
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
        format!("Shots fired: {}", state.shots)
    } else {
        "Each captain places five ships in private: a preset or a random layout, then **Lock in**.".to_owned()
    };
    if pot > 0 {
        description.push_str(&format!(
            "\nPot: **{}** shells ({} each)",
            thousands(pot),
            thousands(session.bet)
        ));
    }
    let embed = EmbedBuilder::new()
        .color(if end.is_some() { SUCCESS } else { BRAND })
        .title("Sea Fleet")
        .description(description);
    let components = match end {
        Some(_) => Vec::new(),
        None if state.battle => battle_menus(session.id, state, aim),
        None => vec![
            Component::ActionRow(ActionRow {
                id: None,
                components: vec![button(
                    format!("bs:o:{}", session.id),
                    "Place my fleet",
                    ButtonStyle::Primary,
                    false,
                )],
            }),
            resign_button("bs", session.id),
        ],
    };
    let mut data = Cx::card(embed, "seafleet.png", png, components, false);
    data.content = Some(content);
    data.allowed_mentions = quiet();
    Ok(data)
}

/// The menus to fire: a column, then (once picked) a row, and the buttons under them.
fn battle_menus(id: i64, state: &State, aim: Option<usize>) -> Vec<Component> {
    let sea = &state.seas[1 - state.turn];
    let open = |col: usize, row: usize| sea.shot(row * SIZE + col).is_none();
    let columns = (0..SIZE)
        .filter(|&col| (0..SIZE).any(|row| open(col, row)))
        .map(|col| {
            option(
                format!("Column {}", char::from(b'A' + col as u8)),
                col.to_string(),
                None,
                aim == Some(col),
            )
        })
        .collect();
    let mut rows = vec![select_row(
        format!("bs:a:{id}:{}", state.shots),
        "Fire: pick a column (A-J)",
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
            &format!("Column {}: pick the row to hit", char::from(b'A' + col as u8)),
            options,
            (1, 1),
        ));
    }
    rows.push(Component::ActionRow(ActionRow {
        id: None,
        components: vec![
            button(format!("bs:m:{id}"), "My fleet", ButtonStyle::Secondary, false),
            button(format!("bs:q:{id}"), "Resign", ButtonStyle::Danger, false),
        ],
    }));
    rows
}

/// The first message, shown when the challenge is accepted: fleets are still to be placed.
pub(crate) async fn begin(cx: &Cx, session: &GameSession) -> anyhow::Result<()> {
    let (fields, names) = unpack(&session.state);
    let state = State::decode(fields).ok_or_else(|| anyhow::anyhow!("bad sea fleet state {fields:?}"))?;
    let data = public(
        &cx.app,
        session,
        &names,
        &state,
        &Show {
            deadline: session.expires_at,
            ..Show::default()
        },
    )
    .await?;
    cx.update(data).await
}

/// `bs:y:<id>` and `bs:n:<id>` answer a challenge; `bs:o:<id>` opens a captain's private placement;
/// `bs:v:<id>:<rev>` draws another layout and `bs:l:<id>:<rev>` locks in the one on the message (the
/// layout itself stays in the database, the buttons name only its revision); `bs:a:<id>:<shots>` picks a
/// column and `bs:f:<id>:<shots>:<column>` a row (the value) to fire at; `bs:m:<id>` shows a captain's own
/// fleet and `bs:r:<id>` refreshes it; `bs:q:<id>` asks to resign and `bs:R:<id>` confirms.
pub(crate) async fn on_component(cx: &Cx, mut args: Split<'_, char>, values: &[String]) -> anyhow::Result<()> {
    let action = args.next();
    let Some(id) = args.next().and_then(|v| v.parse::<i64>().ok()) else {
        return cx.reply_ephemeral("This game has expired.").await;
    };
    let mut number = || args.next().and_then(|v| v.parse::<usize>().ok());
    let value = values.first().and_then(|v| v.parse::<usize>().ok());
    match action {
        Some("y") => answer(cx, DuelKind::SeaFleet, id, true).await,
        Some("n") => answer(cx, DuelKind::SeaFleet, id, false).await,
        Some("q") => confirm_resign(cx, "bs", id).await,
        Some("R") => on_resign(cx, id).await,
        Some("o") => on_open(cx, id).await,
        Some("m") => on_fleet(cx, id, false).await,
        Some("r") => on_fleet(cx, id, true).await,
        Some("v") => on_shuffle(cx, id).await,
        Some("l") => match number() {
            Some(rev) => on_lock(cx, id, rev as i64).await,
            None => cx.reply_ephemeral("This button has expired.").await,
        },
        Some("a") => match (number(), value) {
            (Some(shots), Some(col)) => on_aim(cx, id, shots, col).await,
            _ => cx.reply_ephemeral("This menu has expired.").await,
        },
        Some("f") => match (number(), number(), value) {
            (Some(shots), Some(col), Some(row)) => on_fire(cx, id, shots, col, row).await,
            _ => cx.reply_ephemeral("This menu has expired.").await,
        },
        _ => cx.reply_ephemeral("This button has expired.").await,
    }
}

/// A running game the clicker plays in. `None` after answering the click when there is nothing to do.
async fn running(cx: &Cx, id: i64) -> anyhow::Result<Option<(GameSession, [String; 2], State, usize)>> {
    let me = cx.author()?.id.get();
    // Another kind's id is not this game: its buttons pass another module's switch.
    let Some(session) = cx
        .app
        .db
        .game(id)
        .await?
        .filter(|s| s.is_active() && s.kind == DuelKind::SeaFleet.key())
    else {
        cx.reply_ephemeral("This game is over.").await?;
        return Ok(None);
    };
    if !session.has_player(me) {
        cx.reply_ephemeral("You're not in this game. Challenge someone with `/duel seafleet`!")
            .await?;
        return Ok(None);
    }
    if session.expired() {
        expire_here(cx, &session).await?;
        return Ok(None);
    }
    let (fields, names) = unpack(&session.state);
    let state = State::decode(fields).ok_or_else(|| anyhow::anyhow!("bad sea fleet state {fields:?}"))?;
    let index = usize::from(me != players(&session)[0]);
    Ok(Some((session, names, state, index)))
}

/// A layout drawn from every legal one by the bot's secure random source (`rand`'s thread generator is a
/// CSPRNG seeded by the system), so no layout can be worked out from the ones before it or from the shots.
fn random_fleet() -> Fleet {
    Fleet::random(&mut |bound| rand::random_range(0..bound))
}

/// The private placement message for a captain's layout, with the buttons to draw another or lock it in.
/// The buttons carry the revision of the layout and nothing else of it.
async fn placement(app: &App, id: i64, name: &str, fleet: &Fleet, rev: i64) -> anyhow::Result<InteractionResponseData> {
    let sea = Sea::new();
    let shots = sea.encode();
    let ships = ship_views(fleet, &sea);
    let (title, subtitle, name) = ("Place your fleet".to_owned(), format!("Layout {rev}"), name.to_owned());
    let png = app
        .render(move |r| {
            r.fleet_card(&FleetCard {
                title: &title,
                subtitle: &subtitle,
                sea: SeaView {
                    name: &name,
                    shots: &shots,
                    ships,
                    afloat: SHIPS.len(),
                },
                sunk: [false; 5],
                footer: "Shuffle for another layout, then lock it in",
                last: None,
                theme: Theme::reef(),
            })
        })
        .await?;
    let rows = vec![Component::ActionRow(ActionRow {
        id: None,
        components: vec![
            button(format!("bs:v:{id}:{rev}"), "Shuffle", ButtonStyle::Secondary, false),
            button(format!("bs:l:{id}:{rev}"), "Lock in fleet", ButtonStyle::Success, false),
        ],
    })];
    let embed = EmbedBuilder::new()
        .color(BRAND)
        .title("Place your fleet")
        .description("Only you can see this. **Shuffle** for another random layout, then **Lock in fleet**.");
    Ok(Cx::card(embed, "fleet.png", png, rows, true))
}

/// Draws a new layout for a captain and shows it.
async fn draw(
    app: &App,
    session: &GameSession,
    names: &[String; 2],
    me: usize,
) -> anyhow::Result<InteractionResponseData> {
    let fleet = random_fleet();
    let rev = app
        .db
        .set_fleet_draft(session.id, players(session)[me], &fleet.encode())
        .await?;
    placement(app, session.id, &names[me], &fleet, rev).await
}

/// `bs:o:<id>`: a captain opens their private placement.
async fn on_open(cx: &Cx, id: i64) -> anyhow::Result<()> {
    let Some((session, names, state, me)) = running(cx, id).await? else {
        return Ok(());
    };
    if state.battle {
        return cx
            .reply_ephemeral("The battle has begun. Use **My fleet** to see your waters.")
            .await;
    }
    if state.fleets[me].is_some() {
        return cx
            .reply_ephemeral(format!("Your fleet is locked in. Waiting for **{}**.", names[1 - me]))
            .await;
    }
    let data = draw(&cx.app, &session, &names, me).await?;
    cx.reply(data).await
}

/// `bs:v:<id>:<rev>`: another layout on the private message.
async fn on_shuffle(cx: &Cx, id: i64) -> anyhow::Result<()> {
    let Some((session, names, state, me)) = running(cx, id).await? else {
        return Ok(());
    };
    if state.battle || state.fleets[me].is_some() {
        return cx.update(locked(&cx.app, &state, &names, me).await?).await;
    }
    let data = draw(&cx.app, &session, &names, me).await?;
    cx.update(data).await
}

/// The private view of a captain's own fleet and the shots at it, without buttons.
async fn locked(app: &App, state: &State, names: &[String; 2], me: usize) -> anyhow::Result<InteractionResponseData> {
    let Some(fleet) = &state.fleets[me] else {
        anyhow::bail!("no fleet to show");
    };
    let sea = &state.seas[me];
    let shots = sea.encode();
    let ships = ship_views(fleet, sea);
    let sunk: [bool; 5] = std::array::from_fn(|i| sea.is_sunk(fleet, i));
    let afloat = sea.afloat(fleet).len();
    let (name, last) = (names[me].clone(), state.last.filter(|(s, _)| *s == me).map(|(_, c)| c));
    let footer = if state.battle {
        "Only you can see this".to_owned()
    } else {
        format!("Locked in. Waiting for {}", names[1 - me])
    };
    let png = app
        .render(move |r| {
            r.fleet_card(&FleetCard {
                title: "Your fleet",
                subtitle: &format!("{afloat} of 5 ships afloat"),
                sea: SeaView {
                    name: &name,
                    shots: &shots,
                    ships,
                    afloat,
                },
                sunk,
                footer: &footer,
                last,
                theme: Theme::reef(),
            })
        })
        .await?;
    let embed = EmbedBuilder::new()
        .color(BRAND)
        .title("Your fleet")
        .description("Only you can see this. **Refresh** after the other captain fires.");
    Ok(Cx::card(embed, "fleet.png", png, Vec::new(), true))
}

/// `bs:l:<id>:<rev>`: a captain locks the layout they are looking at in. When both have, the battle begins.
async fn on_lock(cx: &Cx, id: i64, rev: i64) -> anyhow::Result<()> {
    let Some((session, names, state, me)) = running(cx, id).await? else {
        return Ok(());
    };
    if state.battle || state.fleets[me].is_some() {
        return cx.update(locked(&cx.app, &state, &names, me).await?).await;
    }
    let user = players(&session)[me];
    // The layout is taken out of the drafts first, so a double click locks in once.
    let Some(layout) = cx.app.db.take_fleet_draft(id, user, rev).await? else {
        return match cx.app.db.fleet_draft(id, user).await? {
            // The click came from an older placement message: show the layout the captain has now.
            Some((layout, current)) => {
                let fleet = Fleet::decode(&layout).ok_or_else(|| anyhow::anyhow!("bad fleet draft {layout:?}"))?;
                cx.update(placement(&cx.app, id, &names[me], &fleet, current).await?)
                    .await
            }
            None => {
                cx.reply_ephemeral("That layout is gone. Press **Place my fleet** again.")
                    .await
            }
        };
    };
    let fleet = Fleet::decode(&layout).ok_or_else(|| anyhow::anyhow!("bad fleet draft {layout:?}"))?;
    // Both captains can lock in at once; the one that loses the race for the state tries again.
    for _ in 0..4 {
        let Some((session, names, mut state, me)) = running(cx, id).await? else {
            return Ok(());
        };
        if state.battle || state.fleets[me].is_some() {
            return cx.update(locked(&cx.app, &state, &names, me).await?).await;
        }
        state.fleets[me] = Some(fleet.clone());
        let both = state.fleets.iter().all(Option::is_some);
        let (ttl, note) = if both {
            state.battle = true;
            state.turn = rand::random_range(0..2);
            (
                FLEET_TURN_SECS,
                format!(
                    "\n⚓ Fleets are out! {} fires first.",
                    mention(players(&session)[state.turn])
                ),
            )
        } else {
            let left = (session.expires_at - Utc::now()).num_seconds().max(1);
            (left, String::new())
        };
        let packed = pack(&state.encode(), [&names[0], &names[1]]);
        if !cx.app.db.advance_game(id, &session.state, &packed, ttl).await? {
            continue;
        }
        let deadline = Utc::now() + Duration::seconds(ttl);
        let data = public(
            &cx.app,
            &session,
            &names,
            &state,
            &Show {
                deadline,
                note: &note,
                ..Show::default()
            },
        )
        .await?;
        if let Some((channel, message)) = location(&session) {
            edit_card(&cx.app, channel, message, &data).await;
        }
        let mine = locked(&cx.app, &state, &names, me).await?;
        return cx.update(mine).await;
    }
    // Give the layout back, so the captain can try again.
    cx.app.db.set_fleet_draft(id, user, &layout).await?;
    cx.reply_ephemeral("The game moved on while you locked in. Press **Place my fleet** again.")
        .await
}

/// `bs:m:<id>` (a new private message) and `bs:r:<id>` (refresh it): a captain looks at their own waters.
async fn on_fleet(cx: &Cx, id: i64, refresh: bool) -> anyhow::Result<()> {
    let Some((_, names, state, me)) = running(cx, id).await? else {
        return Ok(());
    };
    if state.fleets[me].is_none() {
        return cx
            .reply_ephemeral("You haven't placed your fleet yet. Press **Place my fleet**.")
            .await;
    }
    let mut data = locked(&cx.app, &state, &names, me).await?;
    data.components = Some(vec![Component::ActionRow(ActionRow {
        id: None,
        components: vec![button(format!("bs:r:{id}"), "Refresh", ButtonStyle::Secondary, false)],
    })]);
    if refresh {
        cx.update(data).await
    } else {
        cx.reply(data).await
    }
}

/// The turn's captain, checked: `None` after answering when it is not the clicker's turn or the click came
/// from a message that has moved on (which gets the current one).
async fn firing(cx: &Cx, id: i64, shots: usize) -> anyhow::Result<Option<(GameSession, [String; 2], State, usize)>> {
    let Some((session, names, state, me)) = running(cx, id).await? else {
        return Ok(None);
    };
    if !state.battle {
        cx.reply_ephemeral("The battle hasn't begun yet. Both captains still have to lock in their fleets.")
            .await?;
        return Ok(None);
    }
    if state.shots != shots {
        let data = public(
            &cx.app,
            &session,
            &names,
            &state,
            &Show {
                deadline: session.expires_at,
                ..Show::default()
            },
        )
        .await?;
        cx.update(data).await?;
        return Ok(None);
    }
    if state.turn != me {
        cx.reply_ephemeral("Not your turn yet.").await?;
        return Ok(None);
    }
    Ok(Some((session, names, state, me)))
}

/// `bs:a:<id>:<shots>`: a column was picked; the row menu appears.
async fn on_aim(cx: &Cx, id: i64, shots: usize, col: usize) -> anyhow::Result<()> {
    let Some((session, _, state, _)) = firing(cx, id, shots).await? else {
        return Ok(());
    };
    if col >= SIZE {
        return cx.reply_ephemeral("This menu has expired.").await;
    }
    // Only the menus change, so the picture stays as it is.
    cx.update(InteractionResponseData {
        components: Some(battle_menus(session.id, &state, Some(col))),
        content: Some(format!(
            "🚢 {} vs {}\n{} fires, {}\nColumn **{}** picked: now pick the row.",
            mention(players(&session)[0]),
            mention(players(&session)[1]),
            mention(players(&session)[state.turn]),
            relative(session.expires_at),
            char::from(b'A' + col as u8)
        )),
        allowed_mentions: quiet(),
        ..Default::default()
    })
    .await
}

/// `bs:f:<id>:<shots>:<column>`: a row was picked, so the shot is fired.
async fn on_fire(cx: &Cx, id: i64, shots: usize, col: usize, row: usize) -> anyhow::Result<()> {
    let Some((session, names, mut state, me)) = firing(cx, id, shots).await? else {
        return Ok(());
    };
    if col >= SIZE || row >= SIZE {
        return cx.reply_ephemeral("This menu has expired.").await;
    }
    let target = 1 - me;
    let cell = row * SIZE + col;
    let Some(fleet) = state.fleets[target].clone() else {
        anyhow::bail!("firing at a sea without a fleet");
    };
    let fired = state.seas[target].fire(&fleet, cell);
    if fired == Fired::Again {
        return cx.reply_ephemeral("You already fired there. Pick another cell.").await;
    }
    state.shots += 1;
    state.last = Some((target, cell));
    let name = rules::cell_name(cell);
    let note = match fired {
        Fired::Miss => format!("\n💦 {name}: a miss."),
        Fired::Hit => format!("\n💥 {name}: a hit!"),
        Fired::Sunk(ship) => format!("\n💥 {name}: **sunk the {}!**", SHIPS[ship].0),
        Fired::Again => String::new(),
    };
    let ids = players(&session);
    if state.seas[target].all_sunk(&fleet) {
        let packed = pack(&state.encode(), [&names[0], &names[1]]);
        let end = End {
            winner: me,
            how: "sinking the whole fleet".to_owned(),
        };
        let Some(reward) = conclude(&cx.app, &session, &packed, Play::SeaFleet, Some(ids[me]), state.shots).await?
        else {
            return cx.reply_ephemeral("The battle moved on under you. Look again.").await;
        };
        let data = public(
            &cx.app,
            &session,
            &names,
            &state,
            &Show {
                deadline: Utc::now(),
                end: Some(&end),
                note: &format!("{note}\n{reward}"),
                ..Show::default()
            },
        )
        .await?;
        return cx.update(data).await;
    }
    state.turn = target;
    let packed = pack(&state.encode(), [&names[0], &names[1]]);
    if !cx
        .app
        .db
        .advance_game(id, &session.state, &packed, FLEET_TURN_SECS)
        .await?
    {
        return cx
            .reply_ephemeral("The battle moved on under you. Look again and pick a column.")
            .await;
    }
    let deadline = Utc::now() + Duration::seconds(FLEET_TURN_SECS);
    let data = public(
        &cx.app,
        &session,
        &names,
        &state,
        &Show {
            deadline,
            note: &note,
            ..Show::default()
        },
    )
    .await?;
    cx.update(data).await
}

/// The captain who confirmed on the private message resigns; the public radar shows it.
async fn on_resign(cx: &Cx, id: i64) -> anyhow::Result<()> {
    let Some((session, names, state, me)) = running(cx, id).await? else {
        return Ok(());
    };
    let ids = players(&session);
    let Some(note) = conclude(
        &cx.app,
        &session,
        &session.state,
        Play::SeaFleet,
        Some(ids[1 - me]),
        state.shots,
    )
    .await?
    else {
        return resigned(cx, "The game was over before you could resign.").await;
    };
    let end = End {
        winner: 1 - me,
        how: format!("resignation ({} gave up)", tag(&names[me])),
    };
    let data = public(
        &cx.app,
        &session,
        &names,
        &state,
        &Show {
            deadline: Utc::now(),
            end: Some(&end),
            note: &note,
            ..Show::default()
        },
    )
    .await?;
    if let Some((channel, message)) = location(&session) {
        edit_card(&cx.app, channel, message, &data).await;
    }
    resigned(cx, "You resigned.").await
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Two fleets written out, so a test knows where the ships lie.
    const HARBOUR: &str = "00h09h92v96v45h";
    const SCATTER: &str = "11v60h38h84v53v";

    fn fleet(code: &str) -> Fleet {
        Fleet::decode(code).expect("a valid fleet")
    }

    #[test]
    fn state_round_trips() {
        let mut state = State::new();
        state.fleets[0] = Some(fleet(HARBOUR));
        state.fleets[1] = Some(fleet(SCATTER));
        state.battle = true;
        state.turn = 1;
        let fleet = state.fleets[0].clone().unwrap();
        state.seas[0].fire(&fleet, 44);
        state.last = Some((0, 44));
        state.shots = 1;
        let again = State::decode(&state.encode()).expect("decodes");
        assert_eq!(again.encode(), state.encode());
        assert_eq!(
            (again.battle, again.turn, again.last, again.shots),
            (true, 1, Some((0, 44)), 1)
        );
        assert!(State::decode(&initial_fields()).is_some());
        for bad in ["", "x:0:-:-", "b:2:-:-:a:b:-:0"] {
            assert!(State::decode(bad).is_none(), "{bad:?}");
        }
    }

    #[test]
    fn placement_ends_when_both_fleets_are_set() {
        let mut state = State::new();
        assert!(!state.battle && state.fleets.iter().all(Option::is_none));
        state.fleets[0] = Some(fleet(HARBOUR));
        assert_eq!(state.fleets.iter().filter(|f| f.is_some()).count(), 1);
    }

    #[test]
    fn the_public_radar_shows_nothing_of_a_fleet_until_a_ship_is_down() {
        let mut state = State::new();
        state.fleets = [Some(fleet(HARBOUR)), Some(fleet(SCATTER))];
        // Before the battle, nothing; at the start of it, nothing either.
        assert!(radar_ships(&state).iter().all(Vec::is_empty));
        state.battle = true;
        assert!(radar_ships(&state).iter().all(Vec::is_empty));
        // A hit that sinks nothing shows no ship.
        let harbour = state.fleets[0].clone().unwrap();
        state.seas[0].fire(&harbour, 0);
        assert!(radar_ships(&state).iter().all(Vec::is_empty));
        // The dinghy (E6 and F6) goes down: only it shows, and only on that radar.
        for cell in harbour.cells_of(4) {
            state.seas[0].fire(&harbour, cell);
        }
        let ships = radar_ships(&state);
        assert_eq!(ships[0].len(), 1);
        assert_eq!(ships[0][0].ship, 4);
        assert!(ships[1].is_empty());
    }
}
