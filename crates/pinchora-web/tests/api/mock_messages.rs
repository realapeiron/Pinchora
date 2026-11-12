use super::*;

pub(super) fn guild_channels(mock: &Mock, guild: u64) -> Value {
    let mut channels = channels_json(guild);
    if mock.stats_public_channels.load(Ordering::SeqCst) {
        channels[1]["permission_overwrites"] = json!([{
            "id": guild.to_string(), "type": 0, "allow": "0", "deny": (1u64 << 10).to_string()
        }]);
    }
    if mock.channel_overwrites_unknown.load(Ordering::SeqCst) {
        channels[1].as_object_mut().unwrap().remove("permission_overwrites");
    }
    if mock.hidden_from_fan.load(Ordering::SeqCst) {
        channels[1]["permission_overwrites"] = json!([{
            "id": FAN.to_string(), "type": 0, "allow": "0", "deny": (1u64 << 10).to_string()
        }]);
    }
    if let Some((home, channel)) = *mock.foreign_channel.lock().unwrap()
        && home == guild
    {
        channels
            .as_array_mut()
            .unwrap()
            .push(channel_json(guild, channel, "foreign", 0, 4, None));
    }
    channels
}

pub(super) fn guild_roles(mock: &Mock, guild: u64) -> Value {
    let mut roles = roles_json(guild, mock.bot_position.load(Ordering::SeqCst));
    if mock.stats_public_channels.load(Ordering::SeqCst) || mock.no_image_permissions.load(Ordering::SeqCst) {
        roles[0]["permissions"] = json!((SEND_MESSAGES | (1u64 << 10)).to_string());
    }
    apply_role_permissions(mock, &mut roles);
    roles
}

pub(super) fn dm_channel(mock: &Mock, body: &[u8]) -> (StatusCode, String) {
    mock.dm_opens.fetch_add(1, Ordering::SeqCst);
    match mock.dm_open_failure.load(Ordering::SeqCst) {
        1 => {
            return (
                StatusCode::FORBIDDEN,
                json!({"message":"Cannot send messages to this user","code":50007}).to_string(),
            );
        }
        2 => {
            return (
                StatusCode::BAD_GATEWAY,
                json!({"message":"Bad Gateway","code":0}).to_string(),
            );
        }
        _ => {}
    }
    let request: Value = serde_json::from_slice(body).unwrap_or(Value::Null);
    let recipient = request["recipient_id"].as_str().unwrap_or("0");
    (
        StatusCode::OK,
        json!({"id":recipient,"type":1,"recipients":[]}).to_string(),
    )
}

pub(super) fn message_json(channel: u64, id: u64) -> Value {
    json!({
        "id": id.to_string(),
        "channel_id": channel.to_string(),
        "type": 0,
        "author": user_json(APPLICATION_ID, "pinchora"),
        "content": "",
        "timestamp": "2024-01-01T00:00:00.000000+00:00",
        "edited_timestamp": null,
        "tts": false,
        "mention_everyone": false,
        "mentions": [],
        "mention_roles": [],
        "pinned": false,
        "attachments": [],
        "embeds": [],
        "flags": 0,
    })
}

pub(super) fn post_message(mock: &Mock, channel: &str, body: &[u8]) -> (StatusCode, String) {
    let channel = channel.parse::<u64>().unwrap_or(1);
    let text = String::from_utf8_lossy(body).into_owned();
    mock.posts.lock().unwrap().push((channel, text));
    if mock.refuse_post.load(Ordering::SeqCst) {
        (
            StatusCode::FORBIDDEN,
            json!({"message":"Missing Permissions","code":50013}).to_string(),
        )
    } else {
        (StatusCode::OK, message_json(channel, unique()).to_string())
    }
}
