use crate::progression::voice_xp::*;

#[test]
fn records_survive_the_trip_through_redis() {
    let record = Record {
        channel: 123,
        muted: true,
        deafened: false,
        seen: 1_700_000_000,
        roles: vec![5, 6, 7],
    };
    assert_eq!(Record::decode(&record.encode()), Some(record));
    let bare = Record {
        channel: 1,
        muted: false,
        deafened: true,
        seen: 0,
        roles: Vec::new(),
    };
    assert_eq!(bare.encode(), "1|2|0|");
    assert_eq!(Record::decode(&bare.encode()), Some(bare));
}

fn member(user: u64, bot: bool, roles: &[u64]) -> Member {
    serde_json::from_value(serde_json::json!({
        "user": { "id": user.to_string(), "username": "u", "discriminator": "0", "avatar": null, "bot": bot },
        "roles": roles.iter().map(u64::to_string).collect::<Vec<_>>(),
        "joined_at": "2024-01-01T00:00:00.000000+00:00",
        "deaf": false,
        "mute": false,
        "flags": 0,
    }))
    .expect("member JSON")
}

/// A voice state as GUILD_CREATE sends it: no member.
fn state(user: u64, channel: Option<u64>, self_mute: bool, self_deaf: bool) -> VoiceState {
    serde_json::from_value(serde_json::json!({
        "guild_id": "1",
        "channel_id": channel.map(|c| c.to_string()),
        "user_id": user.to_string(),
        "session_id": "s",
        "deaf": false,
        "mute": false,
        "self_deaf": self_deaf,
        "self_mute": self_mute,
        "self_video": false,
        "suppress": false,
        "request_to_speak_timestamp": null,
    }))
    .expect("voice state JSON")
}

#[test]
fn a_snapshot_takes_roles_and_bots_from_the_member_list() {
    let states = [
        state(1, Some(10), false, false),
        state(2, Some(10), true, true),
        state(3, Some(10), false, false),
        state(4, None, false, false),
        state(5, Some(20), false, false),
    ];
    let members = [
        member(1, false, &[7, 8]),
        member(2, false, &[]),
        member(3, true, &[]),
        member(4, false, &[]),
    ];
    let records = snapshot(&states, &members, 99);
    let users: Vec<u64> = records.iter().map(|(user, _)| *user).collect();
    assert_eq!(
        users,
        vec![1, 2],
        "3 is a bot, 4 is in no channel, 5 has no member to vouch for them"
    );
    assert_eq!(
        records[0].1,
        Record {
            channel: 10,
            muted: false,
            deafened: false,
            seen: 99,
            roles: vec![7, 8],
        }
    );
    assert!(records[1].1.muted && records[1].1.deafened, "self mute and deaf count");
    assert!(snapshot(&[], &members, 0).is_empty());
}

#[test]
fn an_event_without_a_member_is_still_a_human() {
    let record = record_of(&state(9, Some(3), false, false), None, 5).expect("a record");
    assert_eq!((record.channel, record.roles), (3, Vec::new()));
    assert!(
        record_of(&state(9, None, false, false), None, 5).is_none(),
        "left voice"
    );
    assert!(record_of(&state(9, Some(3), false, false), Some(&member(9, true, &[])), 5).is_none());
}

#[test]
fn a_late_ticker_catches_up_a_few_minutes_and_no_more() {
    assert_eq!(due_minutes(100, 100).collect::<Vec<_>>(), vec![100]);
    assert_eq!(due_minutes(101, 100).count(), 0, "nothing is due yet");
    assert_eq!(due_minutes(100, 102).collect::<Vec<_>>(), vec![100, 101, 102]);
    assert_eq!(
        due_minutes(100, 500).collect::<Vec<_>>(),
        vec![497, 498, 499, 500],
        "a long stall pays only the latest few"
    );
}

#[test]
fn a_member_who_left_is_a_record_nobody_is_paid_by() {
    let left = Record::left(1_700_000_000);
    assert!(left.has_left() && !left.muted && !left.deafened && left.roles.is_empty());
    assert_eq!(Record::decode(&left.encode()), Some(left));
    assert_eq!(Record::left(5).encode(), "0|0|5|");
    let here = Record {
        channel: 7,
        muted: false,
        deafened: false,
        seen: 1,
        roles: Vec::new(),
    };
    assert!(!here.has_left());
}

#[test]
fn rounds_do_not_stack() {
    let rounds = Arc::new(Semaphore::new(ROUNDS_AT_ONCE));
    let mut running = Vec::new();
    for _ in 0..ROUNDS_AT_ONCE {
        running.push(start_round(&rounds).expect("a round may start"));
    }
    assert!(
        start_round(&rounds).is_none(),
        "the next minute is skipped while they pay"
    );
    running.pop();
    assert!(start_round(&rounds).is_some(), "once one is done another may start");
}

#[test]
fn a_snapshot_keeps_only_what_the_rebuild_needs() {
    let guild: Guild = serde_json::from_str(
        r#"{
                "id": "5", "name": "g", "afk_channel_id": "99", "afk_timeout": 300, "application_id": null,
                "banner": null, "default_message_notifications": 0, "description": null,
                "discovery_splash": null, "emojis": [], "explicit_content_filter": 0, "features": [],
                "icon": null, "large": false, "mfa_level": 0, "nsfw_level": 0, "owner_id": "1",
                "preferred_locale": "en-US", "premium_progress_bar_enabled": false,
                "public_updates_channel_id": null, "roles": [], "rules_channel_id": null, "splash": null,
                "system_channel_flags": 0, "system_channel_id": null, "vanity_url_code": null,
                "verification_level": 0,
                "channels": [
                    { "id": "10", "type": 2, "name": "lounge", "parent_id": "11" },
                    { "id": "11", "type": 4, "name": "Voice" }
                ],
                "threads": [
                    { "id": "12", "type": 11, "name": "chat", "parent_id": "13", "guild_id": "5" }
                ],
                "members": [
                    { "user": { "id": "1", "username": "a", "discriminator": "0", "avatar": null },
                      "roles": ["7"], "joined_at": "2024-01-01T00:00:00.000000+00:00", "deaf": false,
                      "mute": false, "flags": 0 },
                    { "user": { "id": "2", "username": "b", "discriminator": "0", "avatar": null },
                      "roles": [], "joined_at": "2024-01-01T00:00:00.000000+00:00", "deaf": false,
                      "mute": false, "flags": 0 }
                ],
                "voice_states": [
                    { "guild_id": "5", "channel_id": "10", "user_id": "1", "session_id": "s", "deaf": false,
                      "mute": false, "self_deaf": false, "self_mute": true, "self_video": false,
                      "suppress": false, "request_to_speak_timestamp": null }
                ]
            }"#,
    )
    .expect("guild JSON");
    let snapshot = VoiceSnapshot::of(&guild, 77);
    assert_eq!((snapshot.guild(), snapshot.seen, snapshot.afk), (5, 77, Some(99)));
    let users: Vec<u64> = snapshot.records.iter().map(|(user, _)| *user).collect();
    assert_eq!(users, vec![1], "only the member in voice is kept, with their roles");
    assert_eq!(snapshot.records[0].1.roles, vec![7]);
    assert!(snapshot.records[0].1.muted);
    let mut places = snapshot.places.clone();
    places.sort_by_key(|(id, _)| *id);
    let seen: Vec<(u64, Option<u64>, bool)> = places.iter().map(|(id, p)| (*id, p.parent, p.thread)).collect();
    assert_eq!(
        seen,
        vec![(10, Some(11), false), (11, None, false), (12, Some(13), true)],
        "channels and threads, where they sit"
    );
}

#[test]
fn garbage_is_not_a_record() {
    for raw in ["", "x", "1|2|3", "a|0|0|", "1|b|0|", "1|0|c|", "1|0|0|9,x"] {
        assert_eq!(Record::decode(raw), None, "{raw:?}");
    }
}
