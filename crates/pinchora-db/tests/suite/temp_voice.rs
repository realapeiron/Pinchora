//! Runs against a real PostgreSQL when `PINCHORA_TEST_DATABASE_URL` is set; skipped otherwise.
//!
//! Temp voice: creators with their limits, rooms with their owners, the kick list, and what deleting a
//! player's data leaves of them.

use chrono::{Duration, Utc};
use pinchora_core::temp_voice::{MAX_CREATORS, MAX_ROOMS};
use pinchora_db::{CreatorSave, Db, NewTempRoom, RoomMake, VoiceCreator};

use crate::support::db;

/// Ids unique per run, so the suite can run repeatedly against one database.
use crate::support::ids;

fn creator(guild: u64, channel: u64) -> VoiceCreator {
    VoiceCreator {
        guild_id: guild as i64,
        channel_id: channel as i64,
        category_id: Some(channel as i64 + 1),
        name_template: "{user}'s room".to_owned(),
        default_limit: 0,
        default_locked: false,
        max_per_user: 1,
        panel: true,
        panel_channel_id: None,
        enabled: true,
    }
}

fn room(guild: u64, channel: u64, source: u64, owner: u64) -> NewTempRoom {
    NewTempRoom {
        channel_id: channel,
        guild_id: guild,
        source_id: source,
        owner_id: owner,
        locked: false,
        user_limit: 0,
    }
}

async fn made(db: &Db, new: NewTempRoom) -> pinchora_db::TempRoom {
    match db.create_temp_room(new).await.unwrap() {
        RoomMake::Made(room) => room,
        RoomMake::Full | RoomMake::PerUserFull => panic!("the server or owner is full"),
    }
}

#[tokio::test]
async fn room_creation_reserves_capacity_across_concurrent_discord_calls() {
    let _isolation = crate::support::shared().await;
    let Some(db) = db(5).await else { return };
    let [guild, owner] = ids();
    let reservation = db.reserve_temp_room(guild, owner).await.unwrap();
    let raced = tokio::time::timeout(
        std::time::Duration::from_millis(150),
        db.reserve_temp_room(guild, owner),
    )
    .await;
    assert!(raced.is_err(), "another creation passed the active reservation");
    drop(reservation);
    db.reserve_temp_room(guild, owner)
        .await
        .unwrap()
        .commit()
        .await
        .unwrap();
}

#[tokio::test]
async fn a_server_has_a_limited_number_of_creators() {
    let _isolation = crate::support::shared().await;
    let Some(db) = db(5).await else { return };
    let [guild, other, first] = ids();
    assert!(!db.voice_active(guild).await.unwrap());
    for n in 0..MAX_CREATORS as u64 {
        let saved = db.save_voice_creator(&creator(guild, first + 10 * n)).await.unwrap();
        assert_eq!(saved, CreatorSave::Saved(true));
    }
    assert!(db.voice_active(guild).await.unwrap());
    assert_eq!(
        db.save_voice_creator(&creator(guild, first + 1_000)).await.unwrap(),
        CreatorSave::Full
    );
    // Changing one that exists is not adding.
    let mut again = creator(guild, first);
    again.default_limit = 8;
    again.panel = false;
    again.panel_channel_id = Some(77);
    assert_eq!(db.save_voice_creator(&again).await.unwrap(), CreatorSave::Saved(false));
    let kept = db.voice_creator(guild, first).await.unwrap().unwrap();
    assert_eq!(
        (kept.default_limit, kept.panel, kept.panel_channel_id),
        (8, false, Some(77))
    );
    assert_eq!(db.voice_creators(guild).await.unwrap().len(), MAX_CREATORS);
    // Channel ids are unique, so a creator row for a channel in another server is left over: it is taken over.
    assert_eq!(
        db.save_voice_creator(&creator(other, first + 10)).await.unwrap(),
        CreatorSave::Saved(true)
    );
    assert!(db.voice_creator(guild, first + 10).await.unwrap().is_none());
    assert_eq!(db.voice_creators(other).await.unwrap().len(), 1);
    assert!(db.remove_voice_creator(guild, first).await.unwrap());
    assert!(!db.remove_voice_creator(guild, first).await.unwrap());
    assert!(db.voice_creator(guild, first).await.unwrap().is_none());
    // A room is not a creator.
    let [house, channel, owner] = ids();
    made(&db, room(house, channel, 1, owner)).await;
    assert_eq!(
        db.save_voice_creator(&creator(house, channel)).await.unwrap(),
        CreatorSave::Taken
    );
}

#[tokio::test]
async fn rooms_are_recorded_found_changed_and_forgotten() {
    let _isolation = crate::support::shared().await;
    let Some(db) = db(5).await else { return };
    let [guild, source, a, b, owner, other] = ids();
    let mut setup = creator(guild, source);
    setup.max_per_user = 2;
    db.save_voice_creator(&setup).await.unwrap();
    let first = made(&db, room(guild, a, source, owner)).await;
    assert_eq!(first.owner_id, Some(owner as i64));
    let second = made(&db, room(guild, b, source, owner)).await;
    assert!(second.created_at >= first.created_at);
    assert_eq!(db.rooms_owned_by(guild, owner).await.unwrap().len(), 2);
    assert_eq!(db.rooms_of_creator(source).await.unwrap(), 2);
    assert_eq!(db.temp_rooms(guild).await.unwrap().len(), 2);
    assert!(db.temp_room(a).await.unwrap().is_some());
    assert!(db.rooms_owned_by(guild, other).await.unwrap().is_empty());

    // Lock, hide, limit: a field left out stays as it is.
    db.set_room_state(a, Some(true), None, Some(5)).await.unwrap();
    db.set_room_state(a, None, Some(true), None).await.unwrap();
    let now = db.temp_room(a).await.unwrap().unwrap();
    assert_eq!((now.locked, now.hidden, now.user_limit), (true, true, 5));
    db.mark_room_renamed(a, Utc::now()).await.unwrap();
    db.set_room_panel(a, Some(900), Some(900), Some(901)).await.unwrap();
    let now = db.temp_room(a).await.unwrap().unwrap();
    assert!(now.last_rename_at.is_some());
    assert_eq!(
        (now.text_channel_id, now.panel_channel_id, now.panel_message_id),
        (Some(900), Some(900), Some(901))
    );
    // The paired thread leads back to its room.
    assert_eq!(db.temp_room_of(guild, 900).await.unwrap().unwrap().channel_id, a as i64);
    assert_eq!(db.temp_room_of(guild, a).await.unwrap().unwrap().channel_id, a as i64);
    assert!(db.temp_room_of(guild + 1, a).await.unwrap().is_none());

    assert!(db.delete_temp_room(a).await.unwrap());
    assert!(!db.delete_temp_room(a).await.unwrap());
    assert!(db.temp_room(a).await.unwrap().is_none());
}

#[tokio::test]
async fn the_owner_changes_only_hands_that_still_hold_the_room() {
    let _isolation = crate::support::shared().await;
    let Some(db) = db(5).await else { return };
    let [guild, source, channel, owner, heir, rival] = ids();
    made(&db, room(guild, channel, source, owner)).await;
    // Two claims race: the one that saw the old owner wins, the other finds the room taken.
    assert!(db.move_room_owner(channel, Some(owner), heir).await.unwrap());
    assert!(!db.move_room_owner(channel, Some(owner), rival).await.unwrap());
    assert_eq!(
        db.temp_room(channel).await.unwrap().unwrap().owner_id,
        Some(heir as i64)
    );
    // A room with no owner is claimed from `None`.
    sqlx::query("UPDATE temp_voice_channels SET owner_id = NULL WHERE channel_id = $1")
        .bind(channel as i64)
        .execute(db.pool())
        .await
        .unwrap();
    assert!(!db.move_room_owner(channel, Some(heir), rival).await.unwrap());
    assert!(db.move_room_owner(channel, None, rival).await.unwrap());
}

#[tokio::test]
async fn kicked_members_stay_out_until_invited_and_go_with_the_room() {
    let _isolation = crate::support::shared().await;
    let Some(db) = db(5).await else { return };
    let [guild, source, channel, owner, pest, friend] = ids();
    made(&db, room(guild, channel, source, owner)).await;
    assert!(!db.is_banned_from_room(channel, pest).await.unwrap());
    db.ban_from_room(channel, pest).await.unwrap();
    db.ban_from_room(channel, pest).await.unwrap();
    db.ban_from_room(channel, friend).await.unwrap();
    assert!(db.is_banned_from_room(channel, pest).await.unwrap());
    assert_eq!(db.room_bans(channel).await.unwrap().len(), 2);
    assert!(db.unban_from_room(channel, friend).await.unwrap());
    assert!(!db.unban_from_room(channel, friend).await.unwrap());
    assert!(!db.is_banned_from_room(channel, friend).await.unwrap());
    // The kick list is the room's: it goes when the room does.
    db.delete_temp_room(channel).await.unwrap();
    assert!(!db.is_banned_from_room(channel, pest).await.unwrap());
}

#[tokio::test]
async fn a_server_holds_a_limited_number_of_rooms() {
    let _isolation = crate::support::shared().await;
    let Some(db) = db(5).await else { return };
    let [guild, source, owner, first] = ids();
    for n in 0..MAX_ROOMS as u64 {
        made(&db, room(guild, first + n, source, owner + n)).await;
    }
    assert!(matches!(
        db.create_temp_room(room(guild, first + 10_000, source, owner))
            .await
            .unwrap(),
        RoomMake::Full
    ));
    assert!(db.delete_temp_room(first).await.unwrap());
    made(&db, room(guild, first + 10_000, source, owner)).await;
}

#[tokio::test]
async fn the_sweep_walks_old_rooms_in_batches() {
    let _isolation = crate::support::exclusive().await;
    let Some(db) = db(5).await else { return };
    let [guild, source, owner, first] = ids();
    for n in 0..5 {
        made(&db, room(guild, first + n, source, owner + n)).await;
    }
    // Rooms younger than the cut-off are left alone.
    assert!(
        db.rooms_to_sweep(Utc::now() - Duration::minutes(5), 0, 10)
            .await
            .unwrap()
            .iter()
            .all(|r| r.guild_id != guild as i64)
    );
    let all: Vec<i64> = db
        .rooms_to_sweep(Utc::now() + Duration::minutes(1), first - 1, 3)
        .await
        .unwrap()
        .into_iter()
        .map(|r| r.channel_id)
        .collect();
    assert_eq!(all.len(), 3);
    assert!(all.windows(2).all(|w| w[0] < w[1]), "oldest id first: {all:?}");
    let next = db
        .rooms_to_sweep(Utc::now() + Duration::minutes(1), *all.last().unwrap() as u64, 10)
        .await
        .unwrap();
    assert!(next.iter().all(|r| r.channel_id > *all.last().unwrap()));
}

#[tokio::test]
async fn deleting_a_players_data_leaves_their_room_without_an_owner_and_takes_their_kicks() {
    let _isolation = crate::support::shared().await;
    let Some(db) = db(5).await else { return };
    let [guild, source, channel, owner, guest, pest] = ids();
    made(&db, room(guild, channel, source, owner)).await;
    db.ban_from_room(channel, pest).await.unwrap();
    db.ban_from_room(channel, guest).await.unwrap();
    db.delete_user_data(owner, Utc::now()).await.unwrap();
    let kept = db.temp_room(channel).await.unwrap().unwrap();
    assert_eq!(kept.owner_id, None, "the room goes on, with nobody to own it");
    // Anyone in it may claim it.
    assert!(db.move_room_owner(channel, None, guest).await.unwrap());
    db.delete_user_data(pest, Utc::now()).await.unwrap();
    assert!(!db.is_banned_from_room(channel, pest).await.unwrap());
    assert!(
        db.is_banned_from_room(channel, guest).await.unwrap(),
        "other members' kicks stay"
    );
}
