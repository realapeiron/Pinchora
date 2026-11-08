//! House views and the bot renderer; editing is routed through house_edit.
use std::{sync::LazyLock, time::Duration};

use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
    routing::get,
};
use pinchora_commands::{RoomData, character_png, house_catalog, house_room_name, house_seed_at, render_room_png};
use pinchora_db::{HouseRow, RoomRow};
use pinchora_game::house::tier;
use pinchora_render::sprites::{ATLAS_JSON, ATLAS_PNG};
use serde::Deserialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use crate::{
    error::{ApiError, ApiResult},
    extract::CurrentSession,
    ratelimit,
    state::WebState,
};

/// Bot pictures of rooms per session per minute; each also waits for a render slot like the bot's cards.
const RENDERS_PER_MINUTE: i64 = 30;
/// Character sprites per session per minute; a room asks for a couple, once each.
const SPRITES_PER_MINUTE: i64 = 120;
/// The longest character code that is read: real ones are a few dozen characters.
const MAX_CODE: usize = 400;
/// Whole-number scales the bot picture can be drawn at.
const SCALES: std::ops::RangeInclusive<u32> = 1..=4;

pub fn router() -> Router<WebState> {
    Router::new()
        .merge(super::house_edit::router())
        .route("/", get(own_house))
        .route("/content", get(content))
        .route("/character.png", get(character))
        .route("/rooms/{idx}", get(own_room))
        .route("/rooms/{idx}/render.png", get(own_render))
        .route("/visit/{user}", get(visit_house))
        .route("/visit/{user}/rooms/{idx}", get(visit_room))
        .route("/visit/{user}/rooms/{idx}/render.png", get(visit_render))
}

/// The sprite sheet, served as it is: `/assets/atlas.png` and `/assets/atlas.json`.
pub fn assets() -> Router<WebState> {
    Router::new()
        .route("/assets/atlas.png", get(atlas_png))
        .route("/assets/atlas.json", get(atlas_json))
}

fn etag(bytes: &[u8]) -> String {
    format!("\"{}\"", &hex::encode(Sha256::digest(bytes))[..16])
}

/// A body kept under a strong ETag, answered with 304 when the browser already has it.
pub(super) fn cached(
    headers: &HeaderMap,
    content_type: &'static str,
    cache: &'static str,
    tag: &str,
    body: Vec<u8>,
) -> Response {
    let fresh = headers
        .get(header::IF_NONE_MATCH)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v == tag);
    let mut response = if fresh {
        StatusCode::NOT_MODIFIED.into_response()
    } else {
        ([(header::CONTENT_TYPE, content_type)], body).into_response()
    };
    let h = response.headers_mut();
    h.insert(header::CACHE_CONTROL, HeaderValue::from_static(cache));
    if let Ok(tag) = HeaderValue::from_str(tag) {
        h.insert(header::ETAG, tag);
    }
    response
}

static ATLAS_TAG: LazyLock<String> = LazyLock::new(|| etag(ATLAS_PNG));
static ATLAS_JSON_TAG: LazyLock<String> = LazyLock::new(|| etag(ATLAS_JSON.as_bytes()));

async fn atlas_png(headers: HeaderMap) -> Response {
    cached(
        &headers,
        "image/png",
        "public, max-age=86400",
        &ATLAS_TAG,
        ATLAS_PNG.to_vec(),
    )
}

async fn atlas_json(headers: HeaderMap) -> Response {
    cached(
        &headers,
        "application/json",
        "public, max-age=86400",
        &ATLAS_JSON_TAG,
        ATLAS_JSON.as_bytes().to_vec(),
    )
}

/// The catalogue as JSON text with the versions of the sprite sheet, built once.
static CONTENT: LazyLock<(Vec<u8>, String)> = LazyLock::new(|| {
    let mut catalog = house_catalog();
    let content = pinchora_game::Content::get();
    let tr = pinchora_core::i18n::Lang::Tr;
    let names: serde_json::Map<String, Value> = content
        .items()
        .map(|item| {
            let id = item.id();
            (id.clone(), json!(content.name(&id, tr)))
        })
        .collect();
    catalog["names_tr"] = json!(names);
    if let Some(rows) = catalog["furniture"].as_array_mut() {
        for (row, furniture) in rows.iter_mut().zip(&content.furniture) {
            row["category_name"] = json!(furniture.category.label());
            row["category_name_tr"] = json!(furniture.category.label_in(tr));
            row["rarity_name_tr"] = json!(furniture.rarity.label_in(tr));
        }
    }

    for field in ["wallpapers", "floors"] {
        if let Some(rows) = catalog[field].as_array_mut() {
            for row in rows {
                if let Some(id) = row["id"].as_str() {
                    row["name_tr"] = json!(content.name(id, tr));
                }
            }
        }
    }

    catalog["assets"] = json!({
        "atlas_png": format!("/assets/atlas.png?v={}", ATLAS_TAG.trim_matches('"')),
        "atlas_json": format!("/assets/atlas.json?v={}", ATLAS_JSON_TAG.trim_matches('"')),
    });
    let body = serde_json::to_vec(&catalog).expect("the catalogue serialises");
    let tag = etag(&body);
    (body, tag)
});

/// `GET /api/house/content`: everything the browser needs to know about furniture, surfaces, pets and
/// tiers, and where the sprite sheet is. The same for everyone, so any browser may keep it for an hour.
async fn content(headers: HeaderMap) -> Response {
    let (body, tag) = &*CONTENT;
    cached(&headers, "application/json", "public, max-age=3600", tag, body.clone())
}

/// Whose house is being looked at.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Whose {
    Own(u64),
    Visit { owner: u64, guest: u64 },
}

impl Whose {
    fn owner(self) -> u64 {
        match self {
            Self::Own(owner) | Self::Visit { owner, .. } => owner,
        }
    }

    fn scope(self) -> &'static str {
        match self {
            Self::Own(_) => "own",
            Self::Visit { owner, guest } if owner == guest => "own",
            Self::Visit { .. } => "visit",
        }
    }
}

fn no_house(whose: Whose) -> ApiError {
    let message = match whose {
        Whose::Own(_) => "You have not moved in yet. Run /house in Discord to get your first room.",
        Whose::Visit { .. } => "That player has no house yet.",
    };
    ApiError::new(StatusCode::NOT_FOUND, "no_house", message)
}

/// The house and its rooms, or why they cannot be shown: no house, a hidden one, or a locked door for a visitor.
async fn open(state: &WebState, whose: Whose) -> ApiResult<(HouseRow, Vec<RoomRow>)> {
    if let Whose::Visit { owner, guest } = whose
        && state.app.db.socially_blocked(owner, guest).await?
    {
        return Err(ApiError::new(
            StatusCode::FORBIDDEN,
            "private",
            "This house is private.",
        ));
    }
    if whose.scope() == "visit" {
        // The bot's order: a hidden house is not there at all, so it comes before missing or locked. And a
        // server's plaza (a house owned by the server's id) is only ever reached through the server.
        if state.app.db.privacy(whose.owner()).await?.hide_house {
            return Err(ApiError::new(
                StatusCode::FORBIDDEN,
                "private",
                "This house is private.",
            ));
        }
        if state.app.db.is_guild(whose.owner()).await? {
            return Err(no_house(whose));
        }
    }
    let Some((house, rooms)) = state.app.db.house_if_any(whose.owner()).await? else {
        return Err(no_house(whose));
    };
    if house.locked && whose.scope() == "visit" {
        let name = owner_name(state, whose.owner()).await;
        return Err(ApiError::new(
            StatusCode::FORBIDDEN,
            "locked",
            format!("{name} has locked their house."),
        ));
    }
    Ok((house, rooms))
}

/// The name last remembered for the owner, or a stand-in.
async fn owner_name(state: &WebState, owner: u64) -> String {
    state
        .app
        .db
        .remembered_name(owner)
        .await
        .ok()
        .flatten()
        .unwrap_or_else(|| "This player".to_owned())
}

fn room_index(raw: &str) -> ApiResult<i16> {
    raw.parse::<i16>()
        .ok()
        .filter(|idx| *idx >= 0)
        .ok_or_else(|| ApiError::malformed("Room numbers are small whole numbers, counted from 0."))
}

fn user_id(raw: &str) -> ApiResult<u64> {
    raw.parse::<u64>()
        .ok()
        .filter(|id| *id > 0)
        .ok_or_else(|| ApiError::malformed("That is not a player id."))
}

pub(super) async fn overview(state: &WebState, current: &CurrentSession, whose: Whose) -> ApiResult<Json<Value>> {
    let (house, rooms) = open(state, whose).await?;
    let owner = whose.owner();
    let counts = state.app.db.placement_counts(owner).await?;
    let t = tier(house.tier);
    let name = match whose.scope() {
        "own" => current.session.display_name().to_owned(),
        _ => owner_name(state, owner).await,
    };
    Ok(Json(json!({
        "scope": whose.scope(),
        "whose": owner.to_string(),
        "owner": { "id": owner.to_string(), "name": name, "you": whose.scope() == "own" },
        "house": {
            "name": house.name,
            "tier": house.tier,
            "tier_name": t.name,
            "likes": house.likes,
            "locked": house.locked,
        },
        "width": t.width,
        "depth": t.depth,
        "rooms": rooms.iter().map(|room| json!({
            "idx": room.idx,
            "room_type": room.room_type,
            "name": house_room_name(room),
            "wallpaper": room.wallpaper,
            "floor": room.floor,
            "items": counts.iter().find(|(idx, _)| *idx == room.idx).map_or(0, |(_, n)| *n),
        })).collect::<Vec<_>>(),
    })))
}

pub(super) async fn room_data(state: &WebState, whose: Whose, idx: &str) -> ApiResult<RoomData> {
    let idx = room_index(idx)?;
    let (house, rooms) = open(state, whose).await?;
    RoomData::load(&state.app, whose.owner(), &house, &rooms, idx)
        .await?
        .ok_or_else(|| ApiError::new(StatusCode::NOT_FOUND, "no_room", "That house has no such room."))
}

/// `GET /api/house`: your house: its tier, its rooms and how full each is.
async fn own_house(
    State(state): State<WebState>,
    mut current: CurrentSession,
    Query(query): Query<super::house_edit::Scope>,
) -> ApiResult<Json<Value>> {
    let who = super::house_edit::resolve(&state, &mut current, &query, false).await?;
    let Json(mut value) = overview(&state, &current, Whose::Own(who.owner)).await?;
    value["scope"] = json!(query.scope);
    value["whose"] = json!(who.owner.to_string());
    value["owner"]["you"] = json!(who.owner == current.session.user_id);
    value["editable"] = json!(
        query.scope != "plaza"
            || crate::discord::user_guilds(&state, &mut current, false)
                .await?
                .iter()
                .any(|g| g.id() == Some(who.owner) && g.can_manage())
    );
    if who.owner != current.session.user_id {
        value["owner"]["name"] = json!(owner_name(&state, who.owner).await);
    }
    let n = value["house"]["tier"].as_i64().unwrap_or(1) as i16;
    value["next"] = if n < 6 {
        let t = tier(n + 1);
        json!({"name":t.name,"price":t.price,"rooms":t.rooms,"width":t.width,"depth":t.depth})
    } else {
        Value::Null
    };
    Ok(Json(value))
}

/// `GET /api/house/rooms/{idx}`: one of your rooms with everything in it.
async fn own_room(
    State(state): State<WebState>,
    current: CurrentSession,
    Path(idx): Path<String>,
    Query(query): Query<super::house_edit::Scope>,
) -> ApiResult<Json<Value>> {
    let mut current = current;
    let who = super::house_edit::resolve(&state, &mut current, &query, false).await?;
    let data = room_data(&state, Whose::Own(who.owner), &idx).await?;
    let mut value = data.json();
    let extra = pinchora_commands::house_edit::snapshot(&state.app, who, data.room.idx, &data.placements).await;
    value
        .as_object_mut()
        .expect("room object")
        .extend(extra.as_object().expect("snapshot object").clone());
    value["scope"] = json!(query.scope);
    value["whose"] = json!(who.owner.to_string());
    Ok(Json(value))
}

/// `GET /api/house/visit/{user}`: somebody's house, unless they locked it.
async fn visit_house(
    State(state): State<WebState>,
    current: CurrentSession,
    Path(user): Path<String>,
) -> ApiResult<Json<Value>> {
    let whose = Whose::Visit {
        owner: user_id(&user)?,
        guest: current.session.user_id,
    };
    overview(&state, &current, whose).await
}

/// `GET /api/house/visit/{user}/rooms/{idx}`: a room of somebody's house. Nobody is paid for looking
/// from the web: visit rewards stay with `/house visit`.
async fn visit_room(
    State(state): State<WebState>,
    current: CurrentSession,
    Path((user, idx)): Path<(String, String)>,
) -> ApiResult<Json<Value>> {
    let whose = Whose::Visit {
        owner: user_id(&user)?,
        guest: current.session.user_id,
    };
    Ok(Json(room_data(&state, whose, &idx).await?.json()))
}

#[derive(Deserialize)]
struct RenderQuery {
    #[serde(flatten)]
    scope: super::house_edit::Scope,
    /// Where the pets stand: the number the room's JSON carries. The ten-minute window of now when missing.
    seed: Option<u64>,
    /// Whole pixels per art pixel; 2, as in Discord, when missing.
    scale: Option<u32>,
    cursor: Option<String>,
    selected: Option<i64>,
    grid: Option<u8>,
}

async fn render(
    state: &WebState,
    current: &CurrentSession,
    mut data: RoomData,
    query: RenderQuery,
) -> ApiResult<Response> {
    ratelimit::check(
        &state.app.cache,
        "house-render",
        &current.session.user_id.to_string(),
        RENDERS_PER_MINUTE,
        Duration::from_secs(60),
    )
    .await?;
    let scale = query.scale.unwrap_or(2);
    if !SCALES.contains(&scale) {
        return Err(ApiError::malformed("The scale is 1, 2, 3 or 4."));
    }
    data.seed = query.seed.unwrap_or_else(|| house_seed_at(state.app.now()));
    let mut view = data.view();
    view.selected = query.selected;
    view.edit = query.grid == Some(1);
    if let Some(cursor) = query.cursor {
        let parts: Vec<_> = cursor.split(',').collect();
        let x = parts.first().and_then(|s| s.parse::<u32>().ok());
        let y = parts.get(1).and_then(|s| s.parse::<u32>().ok());
        let (Some(x), Some(y)) = (x, y) else {
            return Err(ApiError::malformed("Invalid cursor."));
        };
        let wall = match parts.get(2).copied().unwrap_or("floor") {
            "floor" | "f" => None,
            "right" | "r" => Some(pinchora_render::room::WallSide::Right),
            "left" | "l" => Some(pinchora_render::room::WallSide::Left),
            _ => return Err(ApiError::malformed("Invalid cursor.")),
        };
        if x >= if matches!(wall, Some(pinchora_render::room::WallSide::Left)) {
            data.depth
        } else {
            data.width
        } || y >= if wall.is_some() { 3 } else { data.depth }
        {
            return Err(ApiError::malformed("Invalid cursor."));
        }
        view.cursor = Some(pinchora_render::room::Cursor {
            x,
            y,
            w: 1,
            d: 1,
            wall,
            ok: true,
        });
    }
    let png = render_room_png(&state.app, view, scale).await?;
    let mut response = png.into_response();
    let h = response.headers_mut();
    h.insert(header::CONTENT_TYPE, HeaderValue::from_static("image/png"));
    h.insert(header::CACHE_CONTROL, HeaderValue::from_static("private, no-store"));
    Ok(response)
}

/// `GET /api/house/rooms/{idx}/render.png`: the picture the bot sends to Discord for one of your rooms.
async fn own_render(
    State(state): State<WebState>,
    current: CurrentSession,
    Path(idx): Path<String>,
    Query(query): Query<RenderQuery>,
) -> ApiResult<Response> {
    let mut current = current;
    let who = super::house_edit::resolve(&state, &mut current, &query.scope, false).await?;
    let data = room_data(&state, Whose::Own(who.owner), &idx).await?;
    render(&state, &current, data, query).await
}

/// `GET /api/house/visit/{user}/rooms/{idx}/render.png`: the bot's picture of a room of somebody's house.
async fn visit_render(
    State(state): State<WebState>,
    current: CurrentSession,
    Path((user, idx)): Path<(String, String)>,
    Query(query): Query<RenderQuery>,
) -> ApiResult<Response> {
    let whose = Whose::Visit {
        owner: user_id(&user)?,
        guest: current.session.user_id,
    };
    let data = room_data(&state, whose, &idx).await?;
    render(&state, &current, data, query).await
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CharacterQuery {
    code: String,
}

/// `GET /api/house/character.png?code=`: a character's sprite at native size, as the room draws it,
/// for a code a room listed. The browser draws rooms itself but never draws characters.
async fn character(
    State(state): State<WebState>,
    current: CurrentSession,
    Query(query): Query<CharacterQuery>,
) -> ApiResult<Response> {
    if query.code.len() > MAX_CODE {
        return Err(ApiError::malformed("That is not a character code."));
    }
    ratelimit::check(
        &state.app.cache,
        "house-sprite",
        &current.session.user_id.to_string(),
        SPRITES_PER_MINUTE,
        Duration::from_secs(60),
    )
    .await?;
    let png = character_png(&state.app, &query.code)
        .await?
        .ok_or_else(|| ApiError::malformed("That is not a character code."))?;
    let mut response = png.into_response();
    let h = response.headers_mut();
    h.insert(header::CONTENT_TYPE, HeaderValue::from_static("image/png"));
    h.insert(header::CACHE_CONTROL, HeaderValue::from_static("private, max-age=600"));
    Ok(response)
}
