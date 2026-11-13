//! The server store: roles members buy with server points. Same rules as `/config store`.

use pinchora_core::infra::i18n::Lang;
use std::collections::HashMap;

use axum::{
    Json,
    extract::{Path, State},
};
use pinchora_cache::Invalidation;
use pinchora_core::admin::settings;
use pinchora_core::t;
use pinchora_db::{MAX_STORE_ITEMS, StoreItem};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::audit;
use crate::audit::Change;
use crate::error::ApiError;
use crate::error::ApiJson;
use crate::error::ApiResult;
use crate::extract::ManagedGuild;
use crate::guild_info;
use crate::routes::admin::settings::parse_id;
use crate::routes::admin::settings::present;
use crate::state::WebState;

fn item_json(item: &StoreItem) -> Value {
    json!({
        "id": item.id.to_string(),
        "role_id": item.role_id.to_string(),
        "price": item.price,
        "stock": item.stock,
    })
}

async fn view(state: &WebState, guild: u64) -> ApiResult<Value> {
    let items: Vec<Value> = state.app.db.store_items(guild).await?.iter().map(item_json).collect();
    Ok(json!({ "items": items, "max": MAX_STORE_ITEMS }))
}

/// `GET /store`.
pub async fn store(State(state): State<WebState>, managed: ManagedGuild) -> ApiResult<Json<Value>> {
    Ok(Json(view(&state, managed.id).await?))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ItemBody {
    price: i64,
    /// `null` (or missing) is unlimited.
    #[serde(default, deserialize_with = "present")]
    stock: Option<Option<i64>>,
}

fn role_of(params: &HashMap<String, String>, lang: Lang) -> ApiResult<u64> {
    params
        .get("role")
        .and_then(|raw| parse_id(raw))
        .ok_or_else(|| ApiError::malformed(t!(lang, "economy.web.errors.role-ids-are-numbers")))
}

/// `PUT /store/{role}`: puts a role on sale, or changes its price and stock.
pub async fn put_item(
    State(state): State<WebState>,
    managed: ManagedGuild,
    Path(params): Path<HashMap<String, String>>,
    ApiJson(body): ApiJson<ItemBody>,
) -> ApiResult<Json<Value>> {
    let lang = managed.panel_lang;
    let (guild, role) = (managed.id, role_of(&params, lang)?);
    let price = settings::check_price(body.price);
    let stock = body.stock.flatten().map(settings::check_stock).transpose();
    let (price, stock) = match (price, stock) {
        (Ok(price), Ok(stock)) => (price, stock),
        (price, stock) => return Err(ApiError::invalid(price.err().into_iter().chain(stock.err()).collect())),
    };
    let info = guild_info::live(&state, guild).await?;
    let grantor = guild_info::grantor(&state, &managed, &info.roles, true).await?;
    info.check_grant("role_id", role, grantor)?;
    let db = &state.app.db;
    let was = db
        .store_items(guild)
        .await?
        .into_iter()
        .find(|i| i.role_id as u64 == role);
    if was.is_some_and(|w| w.price == price && w.stock == stock) {
        return Ok(Json(view(&state, guild).await?));
    }
    let Some(item) = db.add_store_item(guild, role, price, stock).await? else {
        return Err(ApiError::limit(t!(
            lang,
            "economy.web.errors.store-roles-limit",
            max = MAX_STORE_ITEMS
        )));
    };
    let target = role.to_string();
    audit::finish(
        &state,
        &managed.current.session,
        Change {
            guild,
            action: "store.put",
            target: Some(&target),
            before: was.map_or(Value::Null, |w| json!({ "price": w.price, "stock": w.stock })),
            after: json!({ "price": item.price, "stock": item.stock }),
        },
        Some(Invalidation::Guild(guild)),
    )
    .await;
    Ok(Json(view(&state, guild).await?))
}

/// `DELETE /store/{role}`: off sale; members who bought it keep it.
pub async fn delete_item(
    State(state): State<WebState>,
    managed: ManagedGuild,
    Path(params): Path<HashMap<String, String>>,
) -> ApiResult<Json<Value>> {
    let lang = managed.panel_lang;
    let (guild, role) = (managed.id, role_of(&params, lang)?);
    let db = &state.app.db;
    let was = db
        .store_items(guild)
        .await?
        .into_iter()
        .find(|i| i.role_id as u64 == role);
    if let Some(was) = was
        && db.remove_store_item(guild, role).await?
    {
        let target = role.to_string();
        audit::finish(
            &state,
            &managed.current.session,
            Change {
                guild,
                action: "store.delete",
                target: Some(&target),
                before: json!({ "price": was.price, "stock": was.stock }),
                after: Value::Null,
            },
            Some(Invalidation::Guild(guild)),
        )
        .await;
    }
    Ok(Json(view(&state, guild).await?))
}
