//! Browser operations use the Discord editor's rules, atomic restore and Redis history.
use pinchora_db::{PlacementRow, Refusal, RestoreOutcome, RoomChange};
use pinchora_game::{Content, Layer};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::App;
use crate::Who;
use crate::admin::undo;
use crate::admin::undo::Kind;
use crate::admin::undo::Piece;
use crate::admin::undo::Step;
use crate::commands::house::house::standing_on;
use crate::commands::house::house::undo_fits;

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum Operation {
    Place {
        item_id: String,
        x: i16,
        y: i16,
        flipped: bool,
    },
    Move {
        id: String,
        x: i16,
        y: i16,
        flipped: bool,
    },
    Flip {
        id: String,
    },
    Pickup {
        id: String,
    },
    Clear,
    Undo,
}

pub fn fingerprint(rows: &[PlacementRow]) -> String {
    format!("{:016x}", undo::fingerprint(rows))
}

pub fn placements(rows: &[PlacementRow]) -> Value {
    json!(
        rows.iter()
            .map(|p| json!({"id": p.id.to_string(), "item_id": p.item_id,
        "x": p.x, "y": p.y, "flipped": p.flipped}))
            .collect::<Vec<_>>()
    )
}

pub async fn snapshot(app: &App, who: Who, room: i16, rows: &[PlacementRow]) -> Value {
    let history = undo::load(app, who, room, rows).await;
    json!({"placements": placements(rows), "fingerprint": fingerprint(rows),
        "undo": history.steps().iter().map(|s| json!({"kind": s.kind, "describe": s.describe()})).collect::<Vec<_>>()})
}

/// Called while the shared editor guard is held; surface state and revision travel together.
pub async fn versioned_snapshot(app: &App, who: Who, room: i16, rows: &[PlacementRow]) -> anyhow::Result<Value> {
    let mut value = snapshot(app, who, room, rows).await;
    value["revision"] = json!(app.db.reef_edit_revision(who.owner).await?);
    if let Some((_, rooms)) = app.db.house_if_any(who.owner).await?
        && let Some(r) = rooms.iter().find(|r| r.idx == room)
    {
        value["wallpaper"] = json!(r.wallpaper);
        value["floor"] = json!(r.floor);
    }
    Ok(value)
}

pub enum Outcome {
    Done(Value),
    Conflict(Value),
    Refused { code: &'static str, message: String },
}

/// Version comparison runs inside the very transaction that applies the operation.
pub async fn apply(
    app: &App,
    who: Who,
    room: i16,
    size: (u32, u32),
    expected: &str,
    op: Operation,
) -> anyhow::Result<Outcome> {
    apply_receipted(app, who, room, size, expected, op, None).await
}

#[allow(clippy::too_many_arguments)]
pub async fn apply_receipted(
    app: &App,
    who: Who,
    room: i16,
    size: (u32, u32),
    expected: &str,
    op: Operation,
    receipt: Option<&pinchora_db::ReefEditRequest>,
) -> anyhow::Result<Outcome> {
    let _guard = app.db.house_editor(who.owner).await?;
    if let Some(outcome) = replay_edit(app, who, room, receipt).await? {
        return Ok(outcome);
    }
    let content = Content::get();
    if matches!(&op, Operation::Place { x, y, .. } | Operation::Move { x, y, .. } if *x < 0 || *y < 0) {
        return Ok(Outcome::Refused {
            code: "blocked",
            message: "it does not fit inside the room there".into(),
        });
    }
    let before = app.db.placements(who.owner, room).await?;
    if fingerprint(&before) != expected {
        return Ok(Outcome::Conflict(snapshot(app, who, room, &before).await));
    }
    let mut history = undo::load(app, who, room, &before).await;
    let mut edit = RoomOperation {
        who,
        room,
        before: &before,
        history: &mut history,
    };
    let prepared = match match op {
        Operation::Place { item_id, x, y, flipped } => edit.place(item_id, x, y, flipped),
        Operation::Move { id, x, y, flipped } => edit.move_piece(id, x, y, flipped),
        Operation::Pickup { id } => edit.pickup(id),
        Operation::Flip { id } => edit.flip(id),
        Operation::Clear => edit.clear(),
        Operation::Undo => edit.undo(),
    } {
        Ok(prepared) => prepared,
        Err(outcome) => return Ok(outcome),
    };
    let PreparedEdit {
        step,
        changes,
        stock,
        refund,
        undoing,
    } = prepared;
    let result = app
        .db
        .edit_room_receipted(
            who.owner,
            stock,
            refund,
            room,
            &changes,
            (who.owner != who.stock).then_some((who.stock, who.bond)),
            receipt,
            |rows| {
                if fingerprint(rows) == expected {
                    Ok(())
                } else {
                    Err("room_changed".into())
                }
            },
            |rows, change| undo_fits(content, size, rows, change),
        )
        .await?;
    match result {
        RestoreOutcome::Refused(Refusal::Blocked(reason)) if reason == "room_changed" => {
            let rows = app.db.placements(who.owner, room).await?;
            Ok(Outcome::Conflict(snapshot(app, who, room, &rows).await))
        }
        RestoreOutcome::Refused(reason) => Ok(refusal_outcome(reason)),
        RestoreOutcome::Done { added } => {
            finish_edit(
                app,
                who,
                room,
                CommittedEdit {
                    step,
                    changes,
                    undoing,
                    before,
                    history,
                    added,
                },
            )
            .await
        }
    }
}

async fn replay_edit(
    app: &App,
    who: Who,
    room: i16,
    receipt: Option<&pinchora_db::ReefEditRequest>,
) -> anyhow::Result<Option<Outcome>> {
    if let Some(request) = receipt {
        match app.db.reef_edit_receipt(who.owner, room, request).await? {
            Some(true) => {
                let rows = app.db.placements(who.owner, room).await?;
                let mut value = versioned_snapshot(app, who, room, &rows).await?;
                value["replayed"] = json!(true);
                return Ok(Some(Outcome::Done(value)));
            }
            Some(false) => {
                return Ok(Some(Outcome::Refused {
                    code: "request_reused",
                    message: "This request id was already used.".into(),
                }));
            }
            None => {}
        }
    }
    Ok(None)
}

struct PreparedEdit {
    step: Step,
    changes: Vec<RoomChange>,
    stock: u64,
    refund: u64,
    undoing: bool,
}

struct RoomOperation<'a> {
    who: Who,
    room: i16,
    before: &'a Vec<PlacementRow>,
    history: &'a mut undo::History,
}

impl RoomOperation<'_> {
    fn find_piece(&self, id: &str) -> Option<PlacementRow> {
        self.before.iter().find(|p| p.id.to_string() == id).cloned()
    }

    fn place(&mut self, item_id: String, x: i16, y: i16, flipped: bool) -> Result<PreparedEdit, Outcome> {
        let content = Content::get();
        let who = self.who;
        let room = self.room;

        let Some(item) = content.item(&item_id) else {
            return Err(refused("unknown item"));
        };
        let p = PlacementRow {
            id: 0,
            room_idx: room,
            item_id,
            x,
            y,
            flipped,
        };
        Ok(PreparedEdit {
            step: Step::new(Kind::Place, item.name()),
            changes: vec![RoomChange::Add(p)],
            stock: who.stock,
            refund: who.owner,
            undoing: false,
        })
    }

    fn move_piece(&mut self, id: String, x: i16, y: i16, flipped: bool) -> Result<PreparedEdit, Outcome> {
        let content = Content::get();
        let who = self.who;
        let before = self.before;

        // Discord also forbids moving the owner's pieces while decorating as a guest.
        if who.owner != who.stock {
            return Err(Outcome::Refused {
                code: "owner_only",
                message: "Only the owner can move this piece.".into(),
            });
        }
        let Some(p) = self.find_piece(&id) else {
            return Err(refused("that piece is no longer here"));
        };
        let mut step = Step::new(
            Kind::Move,
            content.item(&p.item_id).map_or(p.item_id.clone(), |i| i.name()),
        );
        let mut changes = Vec::new();
        for top in standing_on(content, before, &p) {
            step.removed.push(Piece::of(top));
            changes.push(RoomChange::Remove(top.clone()));
        }
        step.removed.push(Piece::of(&p));
        changes.push(RoomChange::Remove(p.clone()));
        changes.push(RoomChange::Add(PlacementRow { x, y, flipped, ..p }));
        Ok(PreparedEdit {
            step,
            changes,
            stock: who.stock,
            refund: who.owner,
            undoing: false,
        })
    }

    fn pickup(&mut self, id: String) -> Result<PreparedEdit, Outcome> {
        let content = Content::get();
        let who = self.who;
        let before = self.before;

        let Some(p) = self.find_piece(&id) else {
            return Err(refused("that piece is no longer here"));
        };
        let mut step = Step::new(
            Kind::Store,
            content.item(&p.item_id).map_or(p.item_id.clone(), |i| i.name()),
        );
        let mut changes = Vec::new();
        for top in standing_on(content, before, &p) {
            step.removed.push(Piece::of(top));
            changes.push(RoomChange::Remove(top.clone()));
        }
        step.removed.push(Piece::of(&p));
        changes.push(RoomChange::Remove(p));
        Ok(PreparedEdit {
            step,
            changes,
            stock: who.owner,
            refund: who.owner,
            undoing: false,
        })
    }

    fn flip(&mut self, id: String) -> Result<PreparedEdit, Outcome> {
        let content = Content::get();
        let who = self.who;

        let Some(p) = self.find_piece(&id) else {
            return Err(refused("that piece is no longer here"));
        };
        let item = content.item(&p.item_id);
        let kind = if item.as_ref().is_some_and(|i| i.base.layer == Layer::Wall) {
            Kind::Flip
        } else {
            Kind::Turn
        };
        let mut step = Step::new(kind, item.map_or(p.item_id.clone(), |i| i.name()));
        step.turned = Some(Piece {
            flipped: !p.flipped,
            ..Piece::of(&p)
        });
        Ok(PreparedEdit {
            step,
            changes: vec![RoomChange::Flip(p)],
            stock: who.owner,
            refund: who.owner,
            undoing: false,
        })
    }

    fn clear(&mut self) -> Result<PreparedEdit, Outcome> {
        let content = Content::get();
        let who = self.who;
        let before = self.before;

        let mut step = Step::new(Kind::Clear, "room");
        step.removed = before.iter().map(Piece::of).collect();
        let mut rows = before.clone();
        rows.sort_by_key(|p| {
            if content.item(&p.item_id).is_some_and(|i| i.base.layer == Layer::Surface) {
                0
            } else {
                1
            }
        });
        Ok(PreparedEdit {
            step,
            changes: rows.into_iter().map(RoomChange::Remove).collect(),
            stock: who.owner,
            refund: who.owner,
            undoing: false,
        })
    }

    fn undo(&mut self) -> Result<PreparedEdit, Outcome> {
        let content = Content::get();
        let who = self.who;
        let room = self.room;
        let history = &mut self.history;

        let Some(step) = history.pop() else {
            return Err(refused("nothing to undo"));
        };
        let changes = step.reverse(content, room);
        Ok(PreparedEdit {
            step,
            changes,
            stock: who.owner,
            refund: who.stock,
            undoing: true,
        })
    }
}

fn refused(message: &str) -> Outcome {
    Outcome::Refused {
        code: "blocked",
        message: message.to_owned(),
    }
}

struct CommittedEdit {
    step: Step,
    changes: Vec<RoomChange>,
    undoing: bool,
    before: Vec<PlacementRow>,
    history: undo::History,
    added: Vec<i64>,
}

async fn finish_edit(app: &App, who: Who, room: i16, committed: CommittedEdit) -> anyhow::Result<Outcome> {
    let CommittedEdit {
        mut step,
        changes,
        undoing,
        before,
        mut history,
        added,
    } = committed;
    let mut after = before.clone();
    let mut ids = added.into_iter();
    for change in &changes {
        match change {
            RoomChange::Remove(p) => after.retain(|r| r.id != p.id),
            RoomChange::Flip(p) => {
                if let Some(r) = after.iter_mut().find(|r| r.id == p.id) {
                    r.flipped = !r.flipped;
                }
            }
            RoomChange::Add(p) => {
                let id = ids.next().expect("restore returns every added id");
                if undoing {
                    history.renumber(p.id, id);
                }
                let row = PlacementRow { id, ..p.clone() };
                if !undoing {
                    step.added.push(Piece::of(&row));
                }
                after.push(row);
            }
        }
    }
    let description = step.describe();
    if !undoing && !changes.is_empty() {
        history.push(step);
    }
    // Save against our exact committed result, never a subsequent writer's placements.
    undo::save(app, who, room, &mut history, &after).await;
    let _ = app
        .cache
        .publish_invalidation(pinchora_cache::Invalidation::House(who.owner))
        .await;
    let mut value = versioned_snapshot(app, who, room, &after).await?;
    value["step"] = description.into();
    Ok(Outcome::Done(value))
}

fn refusal_outcome(reason: Refusal) -> Outcome {
    match reason {
        Refusal::NotOwned(_) => Outcome::Refused {
            code: "not_owned",
            message: "You do not own this piece.".into(),
        },
        Refusal::Moved(_) => refused("that piece is no longer here"),
        Refusal::Blocked(message) => Outcome::Refused {
            code: "blocked",
            message,
        },
    }
}
