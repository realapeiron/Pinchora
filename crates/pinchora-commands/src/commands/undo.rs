//! Editor undo: the last few changes the editor made to a room, kept in Redis per house owner,
//! room and editor (a partner decorating has a history of their own, which a new partnership
//! starts afresh).

use std::time::Duration;

use pinchora_db::{PlacementRow, RoomChange};
use pinchora_game::{Content, Layer};
use serde::{Deserialize, Serialize};

use super::house::Who;
use crate::App;

/// Steps kept per room.
const KEEP: usize = 10;
/// How long a room's history lives after its last edit.
const TTL: Duration = Duration::from_secs(24 * 60 * 60);

/// A piece as it stands, or stood, in the room.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Piece {
    pub(crate) id: i64,
    pub(crate) item: String,
    pub(crate) x: i16,
    pub(crate) y: i16,
    pub(crate) flipped: bool,
}

impl Piece {
    pub(crate) fn of(p: &PlacementRow) -> Self {
        Self {
            id: p.id,
            item: p.item_id.clone(),
            x: p.x,
            y: p.y,
            flipped: p.flipped,
        }
    }

    fn row(&self, room: i16) -> PlacementRow {
        PlacementRow {
            id: self.id,
            room_idx: room,
            item_id: self.item.clone(),
            x: self.x,
            y: self.y,
            flipped: self.flipped,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum Kind {
    Place,
    /// Taken up and put down somewhere else.
    Move,
    /// Taken up by Move and not put down (yet).
    Lift,
    Store,
    /// A floor piece turned a quarter.
    Turn,
    /// A wall piece moved to the other wall.
    Flip,
    Clear,
}

/// One editor action and what undoing it takes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct Step {
    pub(crate) kind: Kind,
    /// The piece's name; the room's for a clear.
    pub(crate) name: String,
    /// Pieces the action put in the room; undo takes them back to the inventory.
    #[serde(default)]
    pub(crate) added: Vec<Piece>,
    /// Pieces the action took out of the room; undo puts them back where they stood.
    #[serde(default)]
    pub(crate) removed: Vec<Piece>,
    /// A piece turned in place, as it stands now; undo turns it back.
    #[serde(default)]
    pub(crate) turned: Option<Piece>,
}

impl Step {
    pub(crate) fn new(kind: Kind, name: impl Into<String>) -> Self {
        Self {
            kind,
            name: name.into(),
            added: Vec::new(),
            removed: Vec::new(),
            turned: None,
        }
    }

    /// "moving **Oak Chair**", as in "Undid moving **Oak Chair**."
    pub(crate) fn describe(&self) -> String {
        let verb = match self.kind {
            Kind::Place => "placing",
            Kind::Move => "moving",
            Kind::Lift => "picking up",
            Kind::Store => "storing",
            Kind::Turn => "turning",
            Kind::Flip => "flipping",
            Kind::Clear => {
                let n = self.removed.len();
                return format!("clearing the room ({n} piece{})", if n == 1 { "" } else { "s" });
            }
        };
        format!("{verb} **{}**", self.name)
    }

    /// What undoing it does to the room, in order: take back what it added, turn back what it
    /// turned, then put back what it removed, rugs and floor pieces before what stands on them.
    pub(crate) fn reverse(&self, content: &Content, room: i16) -> Vec<RoomChange> {
        let rank = |p: &&Piece| match content.item(&p.item).map(|i| i.base.layer) {
            Some(Layer::Rug) => 0,
            Some(Layer::Floor) => 1,
            Some(Layer::Wall) => 2,
            Some(Layer::Surface) => 3,
            None => 4,
        };
        let mut back: Vec<&Piece> = self.removed.iter().collect();
        back.sort_by_key(rank);
        self.added
            .iter()
            .map(|p| RoomChange::Remove(p.row(room)))
            .chain(self.turned.iter().map(|p| RoomChange::Flip(p.row(room))))
            .chain(back.into_iter().map(|p| RoomChange::Add(p.row(room))))
            .collect()
    }

    /// Where the cursor goes once this is undone: the piece put back, the piece turned back, or
    /// the spot a taken-back piece stood on; as that piece stands after the undo.
    pub(crate) fn focus(&self) -> Option<Piece> {
        if let Some(p) = self.removed.first() {
            return Some(p.clone());
        }
        if let Some(p) = &self.turned {
            return Some(Piece {
                flipped: !p.flipped,
                ..p.clone()
            });
        }
        self.added.first().cloned()
    }
}

/// A room's undo steps, oldest first.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub(crate) struct History {
    /// [`fingerprint`] of the room as the newest step left it. When the room differs, it was
    /// changed outside the editor and the steps may no longer apply.
    room: u64,
    steps: Vec<Step>,
    /// Changed since it was loaded, so it needs saving.
    #[serde(skip)]
    dirty: bool,
}

impl History {
    pub(crate) fn is_empty(&self) -> bool {
        self.steps.is_empty()
    }

    pub(crate) fn push(&mut self, step: Step) {
        self.steps.push(step);
        if self.steps.len() > KEEP {
            self.steps.remove(0);
        }
        self.dirty = true;
    }

    pub(crate) fn pop(&mut self) -> Option<Step> {
        let step = self.steps.pop();
        self.dirty |= step.is_some();
        step
    }

    /// Records a placement. Right after Move lifted the same item, it completes that move, so
    /// one undo puts the piece back where it came from.
    pub(crate) fn placed(&mut self, piece: Piece, name: &str) {
        if let Some(top) = self.steps.last_mut()
            && top.kind == Kind::Lift
            && top.removed.first().is_some_and(|p| p.item == piece.item)
        {
            top.kind = Kind::Move;
            top.added.push(piece);
            self.dirty = true;
            return;
        }
        let mut step = Step::new(Kind::Place, name);
        step.added.push(piece);
        self.push(step);
    }

    /// An undo put the piece that had id `old` back as `new`; older steps follow it there.
    pub(crate) fn renumber(&mut self, old: i64, new: i64) {
        let pieces = self.steps.iter_mut().flat_map(|s| {
            s.added
                .iter_mut()
                .chain(s.removed.iter_mut())
                .chain(s.turned.iter_mut())
        });
        for p in pieces.filter(|p| p.id == old) {
            p.id = new;
        }
        self.dirty = true;
    }
}

/// A hash of the room's placements (FNV-1a, in id order); any change to the room changes it.
pub(crate) fn fingerprint(placements: &[PlacementRow]) -> u64 {
    let mut rows: Vec<&PlacementRow> = placements.iter().collect();
    rows.sort_by_key(|p| p.id);
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    let mut eat = |bytes: &[u8]| {
        for b in bytes {
            hash = (hash ^ u64::from(*b)).wrapping_mul(0x0100_0000_01b3);
        }
    };
    for p in rows {
        eat(&p.id.to_le_bytes());
        eat(p.item_id.as_bytes());
        eat(&[0]);
        eat(&p.x.to_le_bytes());
        eat(&p.y.to_le_bytes());
        eat(&[u8::from(p.flipped)]);
    }
    hash
}

/// `bond` is 0 for an editor in their own house, and for a partner the moment their partnership
/// began, so a piece placed under one partnership cannot be undone under the next.
fn key(owner: u64, editor: u64, bond: i64, room: i16) -> String {
    if owner == editor {
        format!("undo:{owner}:{room}")
    } else {
        format!("undo:{owner}:{room}:{editor}:{bond}")
    }
}

/// The room's undo history; empty when the room changed since the editor last saved it.
pub(crate) async fn load(app: &App, who: Who, room: i16, placements: &[PlacementRow]) -> History {
    let raw = app
        .cache
        .get(&key(who.owner, who.stock, who.bond, room))
        .await
        .unwrap_or_else(|error| {
            tracing::debug!(%error, "undo history not read");
            None
        });
    let Some(history) = raw.and_then(|r| serde_json::from_str::<History>(&r).ok()) else {
        return History::default();
    };
    if history.room == fingerprint(placements) {
        history
    } else {
        // Saving this empty history deletes the stale one.
        History {
            dirty: true,
            ..History::default()
        }
    }
}

/// Saves the history, if it changed, against the room as it is now.
pub(crate) async fn save(app: &App, who: Who, room: i16, history: &mut History, placements: &[PlacementRow]) {
    if !history.dirty {
        return;
    }
    history.dirty = false;
    history.room = fingerprint(placements);
    let key = key(who.owner, who.stock, who.bond, room);
    let saved = if history.steps.is_empty() {
        app.cache.forget(&key).await
    } else {
        match serde_json::to_string(history) {
            Ok(json) => app.cache.put(&key, &json, TTL).await,
            Err(error) => Err(error.into()),
        }
    };
    if let Err(error) = saved {
        tracing::warn!(%error, "undo history not saved");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn piece(id: i64, item: &str, x: i16) -> Piece {
        Piece {
            id,
            item: item.to_owned(),
            x,
            y: 0,
            flipped: false,
        }
    }

    #[test]
    fn keeps_the_last_ten_steps() {
        let mut history = History::default();
        for n in 0..12 {
            history.push(Step::new(Kind::Store, format!("piece {n}")));
        }
        assert_eq!(history.steps.len(), KEEP);
        assert_eq!(history.pop().map(|s| s.name), Some("piece 11".to_owned()));
        assert_eq!(history.steps.first().map(|s| s.name.as_str()), Some("piece 2"));
    }

    #[test]
    fn placing_a_lifted_piece_makes_one_move() {
        let mut history = History::default();
        let mut lift = Step::new(Kind::Lift, "Sofa");
        lift.removed.push(piece(7, "sofa", 1));
        history.push(lift);
        history.placed(piece(9, "sofa", 4), "Sofa");
        assert_eq!(history.steps.len(), 1);
        let step = &history.steps[0];
        assert_eq!(step.kind, Kind::Move);
        assert_eq!(step.describe(), "moving **Sofa**");
        assert_eq!(step.focus(), Some(piece(7, "sofa", 1)), "undo returns to the old spot");
        // Another item after a lift is its own step; the lift stays a pick-up.
        let mut lift = Step::new(Kind::Lift, "Lamp");
        lift.removed.push(piece(10, "lamp", 2));
        history.push(lift);
        history.placed(piece(11, "rug", 0), "Rug");
        assert_eq!(history.steps.len(), 3);
        assert_eq!(history.steps[1].describe(), "picking up **Lamp**");
        assert_eq!(history.steps[2].kind, Kind::Place);
    }

    #[test]
    fn renumbering_follows_a_piece_put_back() {
        let mut history = History::default();
        history.placed(piece(5, "chair", 0), "Chair");
        let mut store = Step::new(Kind::Store, "Chair");
        store.removed.push(piece(5, "chair", 0));
        history.push(store);
        // Undoing the store put the chair back with a new id; undoing the place must find it.
        let undone = history.pop().expect("the store step");
        assert_eq!(undone.focus().map(|p| p.id), Some(5));
        history.renumber(5, 12);
        assert_eq!(history.steps[0].added[0].id, 12);
    }

    #[test]
    fn fingerprint_sees_every_change() {
        let row = |id: i64, x: i16, flipped: bool| PlacementRow {
            id,
            room_idx: 0,
            item_id: "sofa".to_owned(),
            x,
            y: 1,
            flipped,
        };
        let room = [row(1, 0, false), row(2, 3, false)];
        let reordered = [row(2, 3, false), row(1, 0, false)];
        assert_eq!(fingerprint(&room), fingerprint(&reordered));
        assert_ne!(fingerprint(&room), fingerprint(&[row(1, 0, false)]));
        assert_ne!(fingerprint(&room), fingerprint(&[row(1, 0, true), row(2, 3, false)]));
        assert_ne!(fingerprint(&room), fingerprint(&[row(1, 1, false), row(2, 3, false)]));
        assert_ne!(fingerprint(&[]), fingerprint(&room));
    }

    #[test]
    fn history_survives_json() {
        let mut history = History::default();
        history.placed(piece(5, "chair", 3), "Chair");
        let json = serde_json::to_string(&history).expect("serializes");
        let back: History = serde_json::from_str(&json).expect("parses");
        assert_eq!(back.steps[0].added, history.steps[0].added);
        assert!(!back.dirty);
    }
}
