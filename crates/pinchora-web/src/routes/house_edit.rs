//! Authenticated adapters for the shared house editor. No independent placement rules.
use super::house::{Whose, room_data};
use crate::{
    audit, discord,
    error::{ApiError, ApiJson, ApiResult},
    extract::CurrentSession,
    state::WebState,
};
use axum::{
    Json, Router,
    extract::{DefaultBodyLimit, Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use pinchora_commands::{
    Who,
    house_edit::{self, Operation, Outcome},
};
use pinchora_db::{HouseUpdate, HouseUpdateOutcome, NewRoom};
use pinchora_game::{Content, Currency, house};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Deserialize)]
pub(super) struct Scope {
    #[serde(default = "own")]
    pub scope: String,
    pub guild: Option<u64>,
}
fn own() -> String {
    "own".into()
}

pub(super) async fn resolve(
    state: &WebState,
    current: &mut CurrentSession,
    scope: &Scope,
    write: bool,
) -> ApiResult<Who> {
    let me = current.session.user_id;
    match scope.scope.as_str() {
        "own" => Ok(Who::own(me)),
        "partner" => {
            let pair =
                state.app.db.partner(me).await?.ok_or_else(|| {
                    ApiError::new(StatusCode::FORBIDDEN, "partner_only", "You are no longer partners.")
                })?;
            if state.app.db.socially_blocked(me, pair.partner_id).await? {
                return Err(ApiError::new(StatusCode::FORBIDDEN, "access_denied", "access_denied"));
            }
            Ok(Who::partner_of(me, pair))
        }
        "plaza" => {
            let id = scope
                .guild
                .filter(|id| *id > 0 && *id <= i64::MAX as u64)
                .ok_or_else(|| ApiError::malformed("Choose a server."))?;
            // Identical Manage Server/Administrator/owner and bot-presence checks to ManagedGuild.
            let guilds = discord::user_guilds(state, current, true).await?;
            let guild = guilds
                .iter()
                .find(|g| g.id() == Some(id))
                .ok_or_else(ApiError::not_manager)?;
            super::super::reef::spaces::membership(state, id, me)
                .await
                .map_err(|_| ApiError::not_manager())?;
            if write
                && !guild.can_manage()
                && !crate::dashboard_access::permitted(
                    "plazas",
                    false,
                    &crate::dashboard_access::areas(state, id, me).await?,
                )
            {
                return Err(ApiError::not_manager());
            }
            if !discord::bot_present(state, id).await {
                return Err(ApiError::not_manager());
            }
            if !state
                .app
                .db
                .guild_modules(id)
                .await?
                .rule(pinchora_core::modules::Module::Plaza)
                .enabled
            {
                return Err(ApiError::new(
                    StatusCode::FORBIDDEN,
                    "module_disabled",
                    "The plaza module is disabled in this server.",
                ));
            }
            Ok(Who::own(id))
        }
        _ => Err(ApiError::malformed("Unknown house scope.")),
    }
}

pub fn router() -> Router<WebState> {
    Router::new()
        .route("/rooms/{idx}/ops", post(operate).layer(DefaultBodyLimit::max(4096)))
        .route("/inventory", get(inventory))
        .route("/surfaces", get(surfaces))
        .route("/thumbnail.png", get(thumbnail))
        .route("/guestbook", get(guestbook))
}

#[derive(Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
enum ExtraOperation {
    Surface {
        wallpaper: Option<String>,
        floor: Option<String>,
        confirm_price: Option<i64>,
    },
    Upgrade {
        from_tier: i16,
    },
    Lock {
        locked: bool,
    },
}
#[derive(Deserialize, Serialize)]
#[serde(untagged)]
enum Op {
    Edit(Operation),
    Extra(ExtraOperation),
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    #[serde(alias = "expected_fingerprint")]
    expected: String,
    request_id: Option<String>,
    expected_revision: Option<String>,
    op: Op,
}

async fn operate(
    State(state): State<WebState>,
    mut current: CurrentSession,
    Path(idx): Path<String>,
    Query(scope): Query<Scope>,
    ApiJson(request): ApiJson<Request>,
) -> ApiResult<Response> {
    let who = resolve(&state, &mut current, &scope, true).await?;
    state.app.cache.ping().await?;
    let receipt = match (&request.request_id, &request.expected_revision) {
        (Some(id), Some(revision)) if !id.is_empty() && id.len() <= 100 => Some(pinchora_db::ReefEditRequest {
            actor: current.session.user_id,
            request: id.clone(),
            payload: serde_json::to_string(&json!({"op":request.op,"expected":request.expected,"revision":revision}))
                .map_err(anyhow::Error::from)?,
            revision: revision
                .parse::<i64>()
                .ok()
                .filter(|n| *n >= 0)
                .ok_or_else(|| ApiError::malformed("Invalid revision."))?,
        }),
        (None, None) => None,
        _ => return Err(ApiError::malformed("Request id and revision are required together.")),
    };
    let _settings_guard = if matches!(&request.op, Op::Extra(_)) {
        Some(state.app.db.house_editor(who.owner).await?)
    } else {
        None
    };
    let data = room_data(&state, Whose::Own(who.owner), &idx).await?;
    let before = house_edit::snapshot(&state.app, who, data.room.idx, &data.placements).await;
    if let Some(receipt) = receipt.as_ref().filter(|_| matches!(&request.op, Op::Extra(_))) {
        match state
            .app
            .db
            .reef_edit_receipt(who.owner, data.room.idx, receipt)
            .await?
        {
            Some(true) => {
                let mut value =
                    house_edit::versioned_snapshot(&state.app, who, data.room.idx, &data.placements).await?;
                value["replayed"] = json!(true);
                return Ok(Json(value).into_response());
            }
            Some(false) => return Err(ApiError::blocked("This request id was already used.")),
            None => {}
        }
    }
    if receipt.is_none() && request.expected != house_edit::fingerprint(&data.placements) {
        return Ok(conflict(before));
    }
    let operation = serde_json::to_value(&request.op).map_err(anyhow::Error::from)?;
    let mut result = match request.op {
        Op::Edit(op) => match house_edit::apply_receipted(
            &state.app,
            who,
            data.room.idx,
            (data.width, data.depth),
            &request.expected,
            op,
            receipt.as_ref(),
        )
        .await?
        {
            Outcome::Done(value) => value,
            Outcome::Conflict(value) => return Ok(conflict(value)),
            Outcome::Refused { code, message } => {
                return Ok((
                    StatusCode::UNPROCESSABLE_ENTITY,
                    Json(json!({"code":code,"message":message,"error":{"code":code,"message":message}})),
                )
                    .into_response());
            }
        },
        Op::Extra(op) => {
            // The bot exposes decorating, upgrading and locking only for your own house.
            if scope.scope != "own" && !(scope.scope == "plaza" && matches!(&op, ExtraOperation::Surface { .. })) {
                return Err(ApiError::new(
                    StatusCode::FORBIDDEN,
                    "owner_only",
                    "Only the owner can change this.",
                ));
            }
            match op {
                ExtraOperation::Surface {
                    wallpaper,
                    floor,
                    confirm_price,
                } => {
                    if wallpaper.is_some() == floor.is_some() {
                        return Err(ApiError::malformed("Choose one surface."));
                    }
                    let wall = wallpaper.is_some();
                    let id = wallpaper.as_ref().or(floor.as_ref()).expect("one surface");
                    let content = Content::get();
                    let list = if wall {
                        &content.surfaces.wallpapers
                    } else {
                        &content.surfaces.floors
                    };
                    let surface = list.iter().find(|s| &s.id == id).ok_or_else(ApiError::not_found)?;
                    let key = format!("{}:{id}", if wall { "wp" } else { "fl" });
                    if scope.scope == "plaza" && !state.app.db.owns(who.stock, &key).await? {
                        return Err(ApiError::blocked("You do not own this piece."));
                    }
                    let (currency, label) = match surface.currency {
                        Currency::Shells => (pinchora_db::Currency::Shells, "shells"),
                        Currency::Pearls => (pinchora_db::Currency::Pearls, "pearls"),
                    };
                    let outcome = state
                        .app
                        .db
                        .update_house_receipted(
                            who.owner,
                            data.room.idx,
                            HouseUpdate::Surface {
                                wallpaper: wallpaper.as_deref(),
                                floor: floor.as_deref(),
                                key: &key,
                                price: surface.price,
                                currency,
                                confirmed: scope.scope == "own" && confirm_price == Some(surface.price),
                            },
                            receipt.as_ref(),
                            |rows| house_edit::fingerprint(rows) == request.expected,
                        )
                        .await?;
                    match outcome {
                        HouseUpdateOutcome::Done => {},
                        HouseUpdateOutcome::Conflict => return fresh_conflict(&state, who, data.room.idx).await,
                        HouseUpdateOutcome::Confirm { balance } => return Ok((StatusCode::CONFLICT, Json(json!({"code":"confirm", "price":surface.price,"currency":label,"balance":balance,"error":{"code":"confirm","message":"Confirm this purchase."}}))).into_response()),
                        HouseUpdateOutcome::Insufficient { balance } => return Ok((StatusCode::UNPROCESSABLE_ENTITY, Json(json!({"code":"insufficient","balance":balance,"error":{"code":"insufficient","message":"Not enough currency."}}))).into_response()),
                    }
                }
                ExtraOperation::Upgrade { from_tier } => {
                    if !(1..6).contains(&from_tier) {
                        return Err(ApiError::blocked("Your house is already at the largest tier."));
                    }
                    let next = house::tier(from_tier + 1);
                    let content = Content::get();
                    let new_rooms: Vec<NewRoom> = house::ROOM_ORDER
                        .iter()
                        .skip(house::tier(from_tier).rooms)
                        .take(next.rooms - house::tier(from_tier).rooms)
                        .map(|name| {
                            let r = content.room_type(name).expect("built-in room type");
                            NewRoom {
                                room_type: (*name).into(),
                                wallpaper: r.default_wallpaper.clone(),
                                floor: r.default_floor.clone(),
                            }
                        })
                        .collect();
                    match state.app.db.update_house_receipted(who.owner, data.room.idx,
                        HouseUpdate::Upgrade { from_tier, price: next.price, rooms: &new_rooms }, receipt.as_ref(),
                        |rows| house_edit::fingerprint(rows) == request.expected).await? {
                        HouseUpdateOutcome::Done => {},
                        HouseUpdateOutcome::Insufficient { balance } => return Ok((StatusCode::PAYMENT_REQUIRED, Json(json!({"code":"insufficient","balance":balance,"error":{"code":"insufficient","message":"Not enough shells."}}))).into_response()),
                        _ => return fresh_conflict(&state, who, data.room.idx).await,
                    }
                }
                ExtraOperation::Lock { locked } => {
                    if state
                        .app
                        .db
                        .update_house_receipted(
                            who.owner,
                            data.room.idx,
                            HouseUpdate::Lock(locked),
                            receipt.as_ref(),
                            |rows| house_edit::fingerprint(rows) == request.expected,
                        )
                        .await?
                        != HouseUpdateOutcome::Done
                    {
                        return fresh_conflict(&state, who, data.room.idx).await;
                    }
                }
            }
            house_edit::versioned_snapshot(&state.app, who, data.room.idx, &data.placements).await?
        }
    };
    result["scope"] = json!(scope.scope);
    result["whose"] = json!(who.owner.to_string());
    {
        audit::finish(
            &state,
            &current.session,
            audit::Change {
                // Personal house records must not put player ids into a guild-only column.
                // actor_id is covered by the existing privacy anonymization registry.
                guild: if scope.scope == "plaza" { who.owner } else { 0 },
                action: "house.edit",
                target: Some(&idx),
                before: json!({"fingerprint":before["fingerprint"]}),
                after: json!({"operation":operation,"fingerprint":result["fingerprint"],"scope":scope.scope}),
            },
            None,
        )
        .await;
    }
    let _ = state
        .app
        .cache
        .publish_invalidation(pinchora_cache::Invalidation::House(who.owner))
        .await;
    Ok(Json(result).into_response())
}
async fn fresh_conflict(state: &WebState, who: Who, room: i16) -> ApiResult<Response> {
    let rows = state.app.db.placements(who.owner, room).await?;
    Ok(conflict(house_edit::snapshot(&state.app, who, room, &rows).await))
}
fn conflict(mut value: Value) -> Response {
    value["code"] = json!("room_changed");
    value["error"] = json!({"code":"room_changed","message":"This room changed somewhere else (Discord?), reloaded."});
    (StatusCode::CONFLICT, Json(value)).into_response()
}

async fn inventory(
    State(state): State<WebState>,
    mut current: CurrentSession,
    Query(scope): Query<Scope>,
) -> ApiResult<Json<Value>> {
    let who = resolve(&state, &mut current, &scope, true).await?;
    let rows = state.app.db.inventory(who.stock).await?;
    let (_, rooms) = state
        .app
        .db
        .house_if_any(who.owner)
        .await?
        .ok_or_else(ApiError::not_found)?;
    let mut placed = Vec::new();
    for room in rooms {
        for p in state.app.db.placements(who.owner, room.idx).await? {
            placed.push(json!({"item_id":p.item_id,"room":room.idx}));
        }
    }
    Ok(Json(
        json!({"items":rows.iter().filter(|r| Content::get().item(&r.item_id).is_some()).map(|r| json!({"item_id":r.item_id,"quantity":r.quantity})).collect::<Vec<_>>(),"placed":placed}),
    ))
}
async fn surfaces(
    State(state): State<WebState>,
    mut current: CurrentSession,
    Query(scope): Query<Scope>,
) -> ApiResult<Json<Value>> {
    let who = resolve(&state, &mut current, &scope, true).await?;
    let rows = state.app.db.inventory(who.stock).await?;
    let content = Content::get();
    let list = |wall: bool| {
        let surfaces = if wall {
            &content.surfaces.wallpapers
        } else {
            &content.surfaces.floors
        };
        surfaces.iter().map(|s| json!({"id":s.id,"name":s.name,"name_tr":content.name(&s.id,pinchora_core::i18n::Lang::Tr),"price":s.price,"currency":match s.currency { Currency::Shells => "shells", Currency::Pearls => "pearls" },"owned":rows.iter().any(|r|r.quantity>0 && r.item_id == format!("{}:{}",if wall {"wp"} else {"fl"},s.id))})).collect::<Vec<_>>()
    };
    Ok(Json(json!({"wallpapers":list(true),"floors":list(false)})))
}
async fn thumbnail(
    State(state): State<WebState>,
    mut current: CurrentSession,
    Query(scope): Query<Scope>,
    headers: HeaderMap,
) -> ApiResult<Response> {
    let who = resolve(&state, &mut current, &scope, false).await?;
    let data = room_data(&state, Whose::Own(who.owner), "0").await?;
    let view = data.view();
    let sprites = state.app.sprites.clone();
    let png = state
        .app
        .render(move |_| {
            let art = sprites
                .house_thumbnail(&view)
                .ok_or_else(|| anyhow::anyhow!("thumbnail unavailable"))?;
            Ok(art.png.clone())
        })
        .await?;
    let tag = format!(
        "\"{}-{}-{}-{}-{}\"",
        data.width,
        data.depth,
        house_edit::fingerprint(&data.placements),
        data.room.wallpaper,
        data.room.floor
    );
    Ok(super::house::cached(
        &headers,
        "image/png",
        "private, no-cache",
        &tag,
        png,
    ))
}
async fn guestbook(State(state): State<WebState>, current: CurrentSession) -> ApiResult<Json<Value>> {
    let (notes, total) = state.app.db.guestbook(current.session.user_id, 0, 50).await?;
    Ok(Json(
        json!({"notes":notes.iter().map(|n| json!({"id":n.id.to_string(),"author_name":n.author_name,"body":n.body,"created_at":n.created_at})).collect::<Vec<_>>(),"total":total}),
    ))
}
