//! The private collection book and wishlist. All mutations use the session's player id.
use super::*;
use axum::routing::delete;
use chrono::{Datelike, Timelike};
use pinchora_db::{RollClaim, RollStart};
use pinchora_game::{
    Rarity,
    collection::{Collection, Kind, exchange_pick, habitat_sets, recycle_shards, shard_craft_cost},
};

pub(super) fn router() -> Router<WebState> {
    Router::new()
        .route("/book", get(book))
        .route("/wishlist", get(wishlist).put(set_wish))
        .route("/wishlist/{id}", delete(remove_wish))
        .route("/recycle", post(recycle))
        .route("/craft", post(craft))
        .route("/roll", post(start_roll))
        .route("/roll/claim", post(claim_roll))
}

async fn write_limit(st: &WebState, user: u64) -> ApiResult<()> {
    ratelimit::check_strict(
        &st.app.cache,
        "player-collection",
        &user.to_string(),
        20,
        Duration::from_secs(60),
    )
    .await
}

fn sets(dex: &[pinchora_db::DexRow]) -> Vec<Value> {
    let owned = dex.iter().map(|row| row.entry_id.clone()).collect();
    habitat_sets(&owned).into_iter().map(|set|
        json!({"id":format!("{:?}",set.habitat).to_lowercase(),"found":set.found,"total":set.total,"complete":set.complete()})).collect()
}

async fn book(State(st): State<WebState>, c: CurrentSession) -> ApiResult<Json<Value>> {
    let l = lang(&st, &c.session).await;
    let dex = st.app.db.dex(c.session.user_id).await?;
    let catalogue = Collection::get();
    let items: Vec<_> = catalogue
        .critters
        .iter()
        .chain(&catalogue.finds)
        .map(|e| {
            let owned = dex.iter().find(|d| d.entry_id == e.id);
            json!({"id":e.id,"name":e.name_in(l),"kind":if e.kind == Kind::Critter {"critter"} else {"find"},
            "rarity":e.rarity.label_in(l),"caught":owned.map_or(0, |d| d.caught),
            "kept":owned.map_or(0, |d| d.kept),"glimmer":owned.map_or(0, |d| d.glimmer),
            "habitat":e.habitat.map(|h| format!("{h:?}").to_lowercase())})
        })
        .collect();
    let progress = sets(&dex);
    let owned = dex.iter().map(|row| row.entry_id.clone()).collect();
    let roll = st.app.db.collection_roll(c.session.user_id).await?;
    Ok(Json(
        json!({"items":items,"found":dex.len(),"total":catalogue.len(),"shards":st.app.db.shards(c.session.user_id).await?,
        "roll":roll.map(|r| json!({"entry_id":r.entry_id,"name":catalogue.entry(&r.entry_id).map(|e|e.name_in(l)),"glimmer":r.glimmer,"rolled_at":r.rolled_at,"claimed":r.claimed_at.is_some()})),
        "sets":progress,"craft_cost":shard_craft_cost(&owned),"fragments":st.app.db.fragments(c.session.user_id).await?}),
    ))
}

async fn start_roll(State(st): State<WebState>, c: CurrentSession) -> ApiResult<Json<Value>> {
    write_limit(&st, c.session.user_id).await?;
    let now = st.app.now();
    let entry = pinchora_game::collection::drop_cards(now.hour(), now.month(), &mut || rand::random())
        .into_iter()
        .next()
        .ok_or_else(|| ApiError::internal(anyhow::anyhow!("empty critter pool")))?;
    let glimmer = rand::random::<f64>() < pinchora_game::collection::GLIMMER_SHARE;
    let result = st
        .app
        .db
        .start_collection_roll(c.session.user_id, &entry.id, glimmer, now)
        .await?;
    let l = lang(&st, &c.session).await;
    match result {
        RollStart::Started(roll) => {
            finish(
                &st,
                &c.session,
                "player.collection.roll",
                json!({}),
                json!({"entry":roll.entry_id}),
            )
            .await;
            Ok(Json(
                json!({"entry_id":roll.entry_id,"name":entry.name_in(l),"glimmer":roll.glimmer,"rolled_at":roll.rolled_at,"claimed":false}),
            ))
        }
        RollStart::Wait(_) => Err(ApiError::blocked(t!(l, "collection-depth.roll-wait"))),
        RollStart::Limit => Err(ApiError::blocked(t!(l, "collection-depth.roll-limit"))),
    }
}

async fn claim_roll(State(st): State<WebState>, c: CurrentSession) -> ApiResult<Json<Value>> {
    write_limit(&st, c.session.user_id).await?;
    let l = lang(&st, &c.session).await;
    let roll = st
        .app
        .db
        .collection_roll(c.session.user_id)
        .await?
        .ok_or_else(|| ApiError::blocked(t!(l, "collection-depth.roll-gone")))?;
    let entry = Collection::get().entry(&roll.entry_id).ok_or_else(|| invalid(l))?;
    let pay = entry.pay();
    let award = pinchora_db::Award {
        entry: &entry.id,
        new_shells: pay.new_shells,
        dup_shells: pay.dup_shells,
        dup_fragments: pay.dup_fragments,
        bonus_fragments: i64::from(entry.salvage),
    };
    match st
        .app
        .db
        .claim_collection_roll(c.session.user_id, st.app.now(), award)
        .await?
    {
        RollClaim::Claimed(granted) => {
            finish(
                &st,
                &c.session,
                "player.collection.roll.claim",
                json!({}),
                json!({"entry":entry.id}),
            )
            .await;
            Ok(Json(
                json!({"name":entry.name_in(l),"glimmer":roll.glimmer,"shells":granted.shells,"fragments":granted.fragments}),
            ))
        }
        RollClaim::Gone => Err(ApiError::blocked(t!(l, "collection-depth.roll-gone"))),
    }
}

async fn wishlist(State(st): State<WebState>, c: CurrentSession) -> ApiResult<Json<Value>> {
    let l = lang(&st, &c.session).await;
    let rows = st.app.db.wishlist(c.session.user_id).await?;
    Ok(Json(
        json!({"items":rows.iter().filter_map(|r| Collection::get().entry(&r.entry_id).map(|e|
        json!({"id":r.entry_id,"name":e.name_in(l),"ping_on_spawn":r.ping_on_spawn}))).collect::<Vec<_>>(),"max":10}),
    ))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Wish {
    entry_id: String,
    ping_on_spawn: bool,
}
async fn set_wish(State(st): State<WebState>, c: CurrentSession, ApiJson(v): ApiJson<Wish>) -> ApiResult<Json<Value>> {
    write_limit(&st, c.session.user_id).await?;
    let l = lang(&st, &c.session).await;
    let entry = Collection::get().entry(&v.entry_id).ok_or_else(|| invalid(l))?;
    if v.ping_on_spawn && entry.kind != Kind::Critter {
        return Err(invalid(l));
    }
    if !st
        .app
        .db
        .set_wish(c.session.user_id, &v.entry_id, v.ping_on_spawn)
        .await?
    {
        return Err(ApiError::blocked(t!(l, "collection-depth.wishlist-full")));
    }
    finish(
        &st,
        &c.session,
        "player.collection.wish",
        json!({}),
        json!({"entry":v.entry_id,"ping":v.ping_on_spawn}),
    )
    .await;
    Ok(Json(json!({"ok":true})))
}
async fn remove_wish(State(st): State<WebState>, c: CurrentSession, Path(id): Path<String>) -> ApiResult<Json<Value>> {
    write_limit(&st, c.session.user_id).await?;
    if Collection::get().entry(&id).is_none() {
        return Err(ApiError::not_found());
    }
    st.app.db.remove_wish(c.session.user_id, &id).await?;
    finish(
        &st,
        &c.session,
        "player.collection.unwish",
        json!({"entry":id}),
        json!({}),
    )
    .await;
    Ok(Json(json!({"ok":true})))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Recycle {
    entry_id: String,
}
async fn recycle(
    State(st): State<WebState>,
    c: CurrentSession,
    ApiJson(v): ApiJson<Recycle>,
) -> ApiResult<Json<Value>> {
    write_limit(&st, c.session.user_id).await?;
    let l = lang(&st, &c.session).await;
    let e = Collection::get().entry(&v.entry_id).ok_or_else(|| invalid(l))?;
    let worth = recycle_shards(e.rarity);
    let left = st
        .app
        .db
        .recycle_critter(c.session.user_id, &v.entry_id, worth)
        .await?
        .ok_or_else(|| ApiError::blocked(t!(l, "collection-depth.no-spare")))?;
    finish(
        &st,
        &c.session,
        "player.collection.recycle",
        json!({"entry":v.entry_id}),
        json!({"shards":worth}),
    )
    .await;
    Ok(Json(json!({"shards":left,"earned":worth})))
}

async fn craft(State(st): State<WebState>, c: CurrentSession) -> ApiResult<Json<Value>> {
    write_limit(&st, c.session.user_id).await?;
    let l = lang(&st, &c.session).await;
    let dex = st.app.db.dex(c.session.user_id).await?;
    let owned = dex.into_iter().map(|row| row.entry_id).collect();
    let cost = shard_craft_cost(&owned);
    let item = exchange_pick(Content::get(), Rarity::Common, rand::random())
        .ok_or_else(|| ApiError::internal(anyhow::anyhow!("empty collection loot pool")))?;
    let left = st
        .app
        .db
        .craft_shard_loot(c.session.user_id, cost, &item)
        .await?
        .ok_or_else(|| ApiError::blocked(t!(l, "collection-depth.not-enough-shards", cost = cost)))?;
    finish(
        &st,
        &c.session,
        "player.collection.craft",
        json!({"cost":cost}),
        json!({"item":item}),
    )
    .await;
    Ok(Json(
        json!({"item":item,"name":Content::get().name(&item,l),"shards":left,"cost":cost}),
    ))
}
