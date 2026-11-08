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
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use pinchora_commands::{
    Who,
    house_edit::{self, Operation, Outcome},
};
use pinchora_db::{BuyOutcome, NewRoom, UpgradeOutcome};
use pinchora_game::{Content, Currency, house};
use serde::Deserialize;
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
        "partner" => state
            .app
            .db
            .partner(me)
            .await?
            .map(|pair| Who::partner_of(me, pair))
            .ok_or_else(|| ApiError::new(StatusCode::FORBIDDEN, "partner_only", "You are no longer partners.")),
        "plaza" => {
            let id = scope
                .guild
                .filter(|id| *id > 0)
                .ok_or_else(|| ApiError::malformed("Choose a server."))?;
            // Identical Manage Server/Administrator/owner and bot-presence checks to ManagedGuild.
            let guilds = discord::user_guilds(state, current, false).await?;
            if !guilds.iter().any(|g| g.id() == Some(id) && (!write || g.can_manage())) {
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
        .route("/rooms/{idx}/ops", post(operate))
        .route("/inventory", get(inventory))
        .route("/surfaces", get(surfaces))
        .route("/thumbnail.png", get(thumbnail))
        .route("/guestbook", get(guestbook))
}

#[derive(Deserialize)]
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
#[derive(Deserialize)]
#[serde(untagged)]
enum Op {
    Edit(Operation),
    Extra(ExtraOperation),
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    expected: String,
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
    let data = room_data(&state, Whose::Own(who.owner), &idx).await?;
    let before = house_edit::snapshot(&state.app, who, data.room.idx, &data.placements).await;
    if request.expected != house_edit::fingerprint(&data.placements) {
        return Ok(conflict(before));
    }
    let mut result = match request.op {
        Op::Edit(op) => match house_edit::apply(
            &state.app,
            who,
            data.room.idx,
            (data.width, data.depth),
            &request.expected,
            op,
        )
        .await?
        {
            Outcome::Done(value) => value,
            Outcome::Conflict(value) => return Ok(conflict(value)),
            Outcome::Refused { code, message } => {
                return Err(ApiError::new(StatusCode::UNPROCESSABLE_ENTITY, code, message));
            }
        },
        Op::Extra(op) => {
            // The bot exposes decorating, upgrading and locking only for your own house.
            if scope.scope != "own" {
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
                    if !state.app.db.owns(who.owner, &key).await? {
                        let user = state.app.db.user(who.owner).await?;
                        let (currency, balance, label) = match surface.currency {
                            Currency::Shells => (pinchora_db::Currency::Shells, user.credits, "shells"),
                            Currency::Pearls => (pinchora_db::Currency::Pearls, user.pearls, "pearls"),
                        };
                        if confirm_price != Some(surface.price) {
                            return Ok((StatusCode::CONFLICT, Json(json!({"code":"confirm", "price":surface.price,"currency":label,"balance":balance,"error":{"code":"confirm","message":"Confirm this purchase."}}))).into_response());
                        }
                        match state
                            .app
                            .db
                            .buy_item(who.owner, &key, surface.price, currency, 1)
                            .await?
                        {
                            BuyOutcome::Bought { .. } => {}
                            BuyOutcome::Insufficient { balance } => {
                                return Ok((StatusCode::UNPROCESSABLE_ENTITY, Json(json!({"code":"insufficient","balance":balance,"error":{"code":"insufficient","message":"Not enough currency."}}))).into_response());
                            }
                            BuyOutcome::SoldOut => return Err(ApiError::blocked("This item is sold out.")),
                        }
                    }
                    state
                        .app
                        .db
                        .set_room_surfaces(who.owner, data.room.idx, wallpaper.as_deref(), floor.as_deref())
                        .await?;
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
                    match state
                        .app
                        .db
                        .upgrade_house(who.owner, from_tier, next.price, &new_rooms)
                        .await?
                    {
                        UpgradeOutcome::Upgraded { .. } => {}
                        UpgradeOutcome::Insufficient { balance } => {
                            return Ok((StatusCode::PAYMENT_REQUIRED, Json(json!({"code":"insufficient","balance":balance,"error":{"code":"insufficient","message":"Not enough shells."}}))).into_response());
                        }
                        UpgradeOutcome::WrongTier { .. } => return Ok(conflict(before)),
                    }
                }
                ExtraOperation::Lock { locked } => state.app.db.set_house_locked(who.owner, locked).await?,
            }
            house_edit::snapshot(&state.app, who, data.room.idx, &data.placements).await
        }
    };
    result["scope"] = json!(scope.scope);
    result["whose"] = json!(who.owner.to_string());
    if scope.scope == "plaza" {
        audit::finish(
            &state,
            &current.session,
            audit::Change {
                guild: who.owner,
                action: "house.edit",
                target: Some(&idx),
                before,
                after: result.clone(),
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
