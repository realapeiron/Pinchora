use crate::reef::room::*;
use std::{cell::Cell, future::ready};

fn layout(revision: &str) -> Layout {
    Layout {
        grid: Grid {
            width: 2,
            depth: 2,
            blocked: Default::default(),
        },
        targets: vec![],
        revision: revision.into(),
        json: json!({"revision": revision}),
    }
}

fn room_with_member() -> (Room, mpsc::Receiver<crate::reef::Event>) {
    let (outgoing, incoming) = mpsc::channel(64);
    let (close, _) = tokio::sync::watch::channel(None);
    let member = Member {
        peer: Peer {
            token: "member".into(),
            user: 1,
            name: "member".into(),
            character: "character".into(),
            outgoing,
            close,
        },
        tile: [0, 0],
        path: vec![],
        progress: 0.0,
        seat: None,
        direction: "SW".into(),
        used: None,
        advanced: Instant::now(),
        using: false,
        last_sequence: 0,
    };
    let room = Room {
        key: RoomKey {
            owner: 1,
            idx: 0,
            instance: None,
        },
        epoch: "1".into(),
        layout: layout("old"),
        fence: pinchora_db::ReefFence {
            namespace: "layout-test".into(),
            owner: 1,
            idx: 0,
            token: 1,
        },
        layout_events: [20].into_iter().collect(),
        members: [("member".into(), member)].into_iter().collect(),
    };
    (room, incoming)
}

#[tokio::test]
async fn busy_layout_defers_without_losing_or_repeating_notifications() {
    let (mut room, mut incoming) = room_with_member();
    let reads = Cell::new(0);
    let snapshot = load_layout_with_events(
        || {
            let n = reads.get();
            reads.set(n + 1);
            // Every scan observes another commit, including IDs below the known ID.
            ready(Ok((1..=n as i64).chain([20]).collect()))
        },
        || ready(Ok(layout("new"))),
    )
    .await
    .unwrap();
    assert!(snapshot.is_none(), "continuous commits must yield the actor");
    assert_eq!(reads.get(), LAYOUT_READ_ATTEMPTS + 1);
    assert!(!room.apply_layout(snapshot).unwrap());
    assert_eq!(room.layout.revision, "old");
    assert_eq!(room.layout_events, [20].into_iter().collect());
    assert!(incoming.try_recv().is_err());

    let events: Vec<_> = (1..=LAYOUT_READ_ATTEMPTS as i64).chain([20]).collect();
    for _ in 0..2 {
        let snapshot = load_layout_with_events(|| ready(Ok(events.clone())), || ready(Ok(layout("new"))))
            .await
            .unwrap();
        assert!(room.apply_layout(snapshot).unwrap());
    }
    let mut delivered = vec![];
    let mut snapshots = 0;
    while let Ok(event) = incoming.try_recv() {
        match event.body["type"].as_str().unwrap() {
            "layout_changed" => {
                assert_eq!(event.body["revision"], "new");
                delivered.push(event.body["event_id"].as_str().unwrap().to_owned());
            }
            "snapshot" => snapshots += 1,
            kind => panic!("unexpected event: {kind}"),
        }
    }
    assert_eq!(
        delivered,
        (1..=LAYOUT_READ_ATTEMPTS).map(|id| id.to_string()).collect::<Vec<_>>()
    );
    assert_eq!(snapshots, 1);
}

#[tokio::test]
async fn pruning_during_layout_reads_does_not_notify_unchanged_geometry() {
    let (mut room, mut incoming) = room_with_member();
    let reads = Cell::new(0);
    let snapshot = load_layout_with_events(
        || {
            let n = reads.get();
            reads.set(n + 1);
            ready(Ok(if n == 0 { vec![20] } else { vec![] }))
        },
        || ready(Ok(layout("old"))),
    )
    .await
    .unwrap();
    assert!(room.apply_layout(snapshot).unwrap());
    assert_eq!(reads.get(), 3);
    assert!(room.layout_events.is_empty());
    assert_eq!(room.layout.revision, "old");
    assert!(incoming.try_recv().is_err());
}

#[test]
fn invalid_layout_does_not_acknowledge_undelivered_events() {
    let (mut room, mut incoming) = room_with_member();
    let mut invalid = layout("new");
    invalid.grid.width = 0;
    assert!(room.apply_layout(Some((invalid, vec![10, 20]))).is_err());
    assert_eq!(room.layout_events, [20].into_iter().collect());
    assert_eq!(room.layout.revision, "old");
    assert!(incoming.try_recv().is_err());
}
