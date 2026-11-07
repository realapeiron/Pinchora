//! Starboard and vote-channel settings for the server dashboard.

use axum::{
    Json, Router,
    extract::{Path, State},
    routing::{get, post},
};
use pinchora_cache::Invalidation;
use pinchora_core::{
    settings::{FieldError, parse_hex_color},
    starboard::{MAX_BOARDS, MAX_ROLE_WEIGHT, MAX_SOURCE_CHANNELS, MAX_THRESHOLD, MAX_WEIGHTED_ROLES},
};
use pinchora_db::{Starboard, VoteChannel};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::{
    audit::{self, Change},
    error::{ApiError, ApiJson, ApiResult},
    extract::ManagedGuild,
    guild_info,
    state::WebState,
};

#[derive(Deserialize)]
struct BoardPath {
    id: i64,
}

#[derive(Deserialize)]
struct VotePath {
    channel: String,
}

pub fn router() -> Router<WebState> {
    Router::new()
        .route("/", get(list))
        .route("/boards", post(save_board))
        .route("/boards/{id}", axum::routing::delete(delete_board))
        .route("/votes", post(save_vote))
        .route("/votes/{channel}", axum::routing::delete(delete_vote))
}

fn board_json(b: &Starboard) -> Value {
    json!({"id": b.id.to_string(), "name": b.name, "channel_id": b.channel_id.to_string(), "emoji": b.emoji,
        "threshold": b.threshold, "self_star": b.self_star, "allow_bots": b.allow_bots,
        "only_channels": b.only_channels.iter().map(ToString::to_string).collect::<Vec<_>>(),
        "except_channels": b.except_channels.iter().map(ToString::to_string).collect::<Vec<_>>(),
        "require_image": b.require_image, "color": format!("#{:06x}", b.color),
        "role_weights": b.role_weights, "remove_below": b.remove_below, "enabled": b.enabled})
}
fn vote_json(v: &VoteChannel) -> Value {
    json!({"channel_id": v.channel_id.to_string(), "up_emoji": v.up_emoji, "down_emoji": v.down_emoji,
        "only_media": v.only_media, "create_thread": v.create_thread, "enabled": v.enabled})
}

async fn list(State(state): State<WebState>, guild: ManagedGuild) -> ApiResult<Json<Value>> {
    let boards = state.app.db.starboards(guild.id).await?;
    let votes = state.app.db.votes(guild.id).await?;
    Ok(Json(
        json!({"boards": boards.iter().map(board_json).collect::<Vec<_>>(),
        "votes": votes.iter().map(vote_json).collect::<Vec<_>>(), "message_content": state.app.message_content,
        "limits": {"boards": MAX_BOARDS, "threshold": MAX_THRESHOLD}}),
    ))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct BoardBody {
    id: Option<String>,
    name: String,
    channel_id: String,
    emoji: String,
    threshold: i32,
    self_star: bool,
    allow_bots: bool,
    only_channels: Vec<String>,
    except_channels: Vec<String>,
    require_image: bool,
    color: String,
    role_weights: serde_json::Value,
    remove_below: bool,
    enabled: bool,
}

fn bad(field: &'static str, message: &str) -> ApiError {
    ApiError::invalid(vec![FieldError::new(field, message)])
}
fn id(value: &str, field: &'static str) -> ApiResult<u64> {
    value
        .parse::<u64>()
        .ok()
        .filter(|n| *n > 0 && *n <= i64::MAX as u64)
        .ok_or_else(|| bad(field, "Choose a valid Discord ID."))
}
fn ids(values: &[String], field: &'static str) -> ApiResult<Vec<i64>> {
    if values.len() > MAX_SOURCE_CHANNELS {
        return Err(bad(field, "Choose at most 50 channels."));
    }
    values.iter().map(|v| Ok(id(v, field)? as i64)).collect()
}

async fn save_board(
    State(state): State<WebState>,
    guild: ManagedGuild,
    ApiJson(body): ApiJson<BoardBody>,
) -> ApiResult<Json<Value>> {
    let db = &state.app.db;
    let channel = id(&body.channel_id, "channel_id")?;
    let info = guild_info::get(&state, guild.id, false).await?;
    if !info.channel(channel).is_some_and(guild_info::ChannelInfo::takes_posts) {
        return Err(bad(
            "channel_id",
            "Choose a text or announcement channel in this server.",
        ));
    }
    if body.name.trim().is_empty() || body.name.chars().count() > 40 {
        return Err(bad("name", "Use a name of 1–40 characters."));
    }
    if body.emoji.trim().is_empty() || body.emoji.chars().count() > 64 {
        return Err(bad("emoji", "Choose one emoji."));
    }
    if !(1..=MAX_THRESHOLD).contains(&body.threshold) {
        return Err(bad("threshold", "Choose 1–50 stars."));
    }
    let Some(color) = parse_hex_color(&body.color) else {
        return Err(bad("color", "Use a colour like #ffc857."));
    };
    let only = ids(&body.only_channels, "only_channels")?;
    let except = ids(&body.except_channels, "except_channels")?;
    for source in only.iter().chain(&except) {
        if info.channel(*source as u64).is_none() {
            return Err(bad("only_channels", "Choose channels in this server."));
        }
    }
    let Some(weights) = body.role_weights.as_object() else {
        return Err(bad("role_weights", "Use role IDs with weights from 1 to 50."));
    };
    if weights.len() > MAX_WEIGHTED_ROLES
        || weights.iter().any(|(r, w)| {
            id(r, "role_weights").is_err() || !w.as_i64().is_some_and(|n| (1..=MAX_ROLE_WEIGHT).contains(&n))
        })
    {
        return Err(bad("role_weights", "Use up to 25 role IDs with weights from 1 to 50."));
    }
    let before = body.id.as_deref().map(|v| id(v, "id")).transpose()?.map(|v| v as i64);
    let previous = if let Some(before) = before {
        let previous = db.starboard(before).await?;
        if db
            .starboard(before)
            .await?
            .is_none_or(|b| b.guild_id != guild.id as i64 || b.channel_id != channel as i64)
        {
            return Err(ApiError::not_found());
        }
        previous
    } else if db.starboards(guild.id).await?.len() >= MAX_BOARDS {
        return Err(ApiError::limit("This server already has 3/3 starboards."));
    } else {
        None
    };
    let saved = db
        .save_starboard(&Starboard {
            id: before.unwrap_or(0),
            guild_id: guild.id as i64,
            name: body.name,
            channel_id: channel as i64,
            emoji: body.emoji,
            threshold: body.threshold,
            self_star: body.self_star,
            allow_bots: body.allow_bots,
            only_channels: only,
            except_channels: except,
            require_image: body.require_image,
            color: color as i32,
            role_weights: body.role_weights,
            remove_below: body.remove_below,
            enabled: body.enabled,
        })
        .await?;
    let target = saved.id.to_string();
    audit::finish(
        &state,
        &guild.current.session,
        Change {
            guild: guild.id,
            action: "starboard.save",
            target: Some(&target),
            before: previous.as_ref().map_or(Value::Null, board_json),
            after: board_json(&saved),
        },
        Some(Invalidation::Guild(guild.id)),
    )
    .await;
    Ok(Json(board_json(&saved)))
}

async fn delete_board(
    State(state): State<WebState>,
    guild: ManagedGuild,
    Path(BoardPath { id }): Path<BoardPath>,
) -> ApiResult<Json<Value>> {
    let previous = state
        .app
        .db
        .starboard(id)
        .await?
        .filter(|b| b.guild_id == guild.id as i64)
        .ok_or_else(ApiError::not_found)?;
    if !state.app.db.delete_starboard(guild.id, id).await? {
        return Err(ApiError::not_found());
    }
    let target = id.to_string();
    audit::finish(
        &state,
        &guild.current.session,
        Change {
            guild: guild.id,
            action: "starboard.delete",
            target: Some(&target),
            before: board_json(&previous),
            after: Value::Null,
        },
        Some(Invalidation::Guild(guild.id)),
    )
    .await;
    Ok(Json(json!({"removed": true})))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct VoteBody {
    channel_id: String,
    up_emoji: String,
    down_emoji: String,
    only_media: bool,
    create_thread: bool,
    enabled: bool,
}

async fn save_vote(
    State(state): State<WebState>,
    guild: ManagedGuild,
    ApiJson(body): ApiJson<VoteBody>,
) -> ApiResult<Json<Value>> {
    let channel = id(&body.channel_id, "channel_id")?;
    let info = guild_info::get(&state, guild.id, false).await?;
    if !info.channel(channel).is_some_and(guild_info::ChannelInfo::takes_posts) {
        return Err(bad("channel_id", "Choose a text channel in this server."));
    }
    if body.up_emoji.trim().is_empty()
        || body.down_emoji.trim().is_empty()
        || body.up_emoji == body.down_emoji
        || body.up_emoji.len() > 64
        || body.down_emoji.len() > 64
    {
        return Err(bad("up_emoji", "Choose two different emojis."));
    }
    let row = VoteChannel {
        channel_id: channel as i64,
        guild_id: guild.id as i64,
        up_emoji: body.up_emoji,
        down_emoji: body.down_emoji,
        only_media: body.only_media,
        create_thread: body.create_thread,
        enabled: body.enabled,
    };
    let previous = state
        .app
        .db
        .votes(guild.id)
        .await?
        .into_iter()
        .find(|v| v.channel_id == row.channel_id);
    state.app.db.save_vote(&row).await?;
    audit::finish(
        &state,
        &guild.current.session,
        Change {
            guild: guild.id,
            action: "starboard.vote.save",
            target: Some(&body.channel_id),
            before: previous.as_ref().map_or(Value::Null, vote_json),
            after: vote_json(&row),
        },
        Some(Invalidation::Guild(guild.id)),
    )
    .await;
    Ok(Json(vote_json(&row)))
}

async fn delete_vote(
    State(state): State<WebState>,
    guild: ManagedGuild,
    Path(VotePath { channel }): Path<VotePath>,
) -> ApiResult<Json<Value>> {
    let channel_id = id(&channel, "channel")?;
    let previous = state
        .app
        .db
        .votes(guild.id)
        .await?
        .into_iter()
        .find(|v| v.channel_id == channel_id as i64)
        .ok_or_else(ApiError::not_found)?;
    if !state.app.db.delete_vote(guild.id, channel_id).await? {
        return Err(ApiError::not_found());
    }
    audit::finish(
        &state,
        &guild.current.session,
        Change {
            guild: guild.id,
            action: "starboard.vote.delete",
            target: Some(&channel),
            before: vote_json(&previous),
            after: Value::Null,
        },
        Some(Invalidation::Guild(guild.id)),
    )
    .await;
    Ok(Json(json!({"removed": true})))
}
