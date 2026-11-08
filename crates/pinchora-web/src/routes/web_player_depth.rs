//! Player collections and social pages. Writes use the same Db transactions as Discord.
use super::*;
use chrono::{Duration as ChronoDuration, Utc};
use pinchora_db::{TripClaim, TripPayout, Wake};
use pinchora_game::{
    pets::{self, Care, FOOD_PRICE, WAKE_FEE, WAKE_NEEDS},
    quests,
    trips::{self, Rolls},
};

pub(super) fn router() -> Router<WebState> {
    Router::new()
        .route("/inventory", get(inventory))
        .route("/inventory/gift", post(gift))
        .route("/stall", get(stall))
        .route("/stall/listings", post(list_item))
        .route("/stall/listings/{id}", axum::routing::delete(unlist_item))
        .route("/pets", get(pet_list))
        .route("/pets/{id}/care", post(care))
        .route("/pets/{id}/active", post(active))
        .route("/pets/{id}/trip", post(start_trip))
        .route("/pets/claim", post(claim_trips))
        .route("/pets/{id}/wake", post(wake_pet))
        .route("/pets/{id}/card.png", get(pet_card))
        .route("/pets/{id}", axum::routing::patch(rename))
        .route("/fish-book", get(fish_book))
        .route("/quests", get(quest_list))
        .route("/guestbook", get(guestbook))
        .route("/guestbook/{id}", axum::routing::delete(delete_note))
        .route("/guestbook/blocks/{id}", post(block).delete(unblock))
        .route("/friends", get(friends))
        .route("/partner", get(partner))
}

#[derive(Default, Deserialize)]
struct Page {
    page: Option<u32>,
    q: Option<String>,
    category: Option<String>,
    rarity: Option<String>,
    set: Option<String>,
}
fn offset(page: Option<u32>) -> i64 {
    i64::from(page.unwrap_or(1).clamp(1, 1000) - 1) * 24
}

async fn inventory(State(st): State<WebState>, c: CurrentSession, Query(q): Query<Page>) -> ApiResult<Json<Value>> {
    let l = lang(&st, &c.session).await;
    let content = Content::get();
    let rows = st.app.db.inventory(c.session.user_id).await?;
    let placed = st.app.db.item_placements(c.session.user_id).await?;
    let mut owned = std::collections::BTreeMap::<String, (i32, Vec<Value>)>::new();
    for row in rows {
        owned.entry(row.item_id).or_default().0 = row.quantity;
    }
    for (item_id, room, count) in placed {
        owned
            .entry(item_id)
            .or_default()
            .1
            .push(json!({"room":room,"count":count}));
    }
    let needle = q.q.unwrap_or_default().to_lowercase();
    let items: Vec<Value> = owned
        .iter()
        .filter_map(|(id, (quantity, rooms))| {
            let item = content.item(id)?;
            let name = content.name(id, l).into_owned();
            let category = item.base.category.key();
            let rarity = item.base.rarity.label().to_lowercase();
            if !name.to_lowercase().contains(&needle)
                || q.category.as_deref().filter(|v| !v.is_empty()).is_some_and(|v| v != category)
                || q.rarity.as_deref().filter(|v| !v.is_empty()).is_some_and(|v| v != rarity.as_str())
                || q.set.as_deref().filter(|v| !v.is_empty()).is_some_and(|v| Some(v) != item.base.set.as_deref())
            {
                return None;
            }
            Some(
                json!({"id":id,"name":name,"quantity":quantity,"placed":rooms,"category":category,"rarity":rarity,"set":item.base.set,
            "category_name":item.base.category.label_in(l),"rarity_name":item.base.rarity.label_in(l),"bound":item.bound}),
            )
        })
        .collect();
    let total = items.len();
    Ok(Json(
        json!({"items":items.into_iter().skip(offset(q.page) as usize).take(24).collect::<Vec<_>>(),"total":total,
            "categories":pinchora_game::Category::ALL.iter().map(|v|json!({"id":v.key(),"name":v.label_in(l)})).collect::<Vec<_>>(),
            "rarities":pinchora_game::Rarity::ALL.iter().map(|v|json!({"id":v.label().to_lowercase(),"name":v.label_in(l)})).collect::<Vec<_>>(),
            "sets":content.sets.iter().map(|v|json!({"id":v.id,"name":content.name(&v.id,l)})).collect::<Vec<_>>()}),
    ))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Gift {
    to_user_id: String,
    item_id: String,
}
async fn gift(State(st): State<WebState>, c: CurrentSession, ApiJson(v): ApiJson<Gift>) -> ApiResult<Json<Value>> {
    let to = v.to_user_id.parse::<u64>().map_err(|_| ApiError::not_found())?;
    let item = Content::get().item(&v.item_id).ok_or_else(ApiError::not_found)?;
    if to == 0 || to > i64::MAX as u64 || to == c.session.user_id || item.bound {
        return Err(invalid(lang(&st, &c.session).await));
    }
    if !st.app.db.gift_item(c.session.user_id, to, &item.id()).await? {
        return Err(ApiError::not_found());
    }
    finish(
        &st,
        &c.session,
        "player.inventory.gift",
        json!({}),
        json!({"item":item.id()}),
    )
    .await;
    Ok(Json(json!({"ok":true})))
}
async fn stall(State(st): State<WebState>, c: CurrentSession) -> ApiResult<Json<Value>> {
    let l = lang(&st, &c.session).await;
    let items=st.app.db.listings(c.session.user_id).await?.iter().map(|r| json!({"id":r.id.to_string(),"item_id":r.item_id,"name":Content::get().name(&r.item_id,l),"price":r.price})).collect::<Vec<_>>();
    Ok(Json(json!({"items":items})))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Listing {
    item_id: String,
    price: i64,
}
async fn list_item(
    State(st): State<WebState>,
    c: CurrentSession,
    ApiJson(v): ApiJson<Listing>,
) -> ApiResult<Json<Value>> {
    let item = Content::get().item(&v.item_id).ok_or_else(ApiError::not_found)?;
    if !(1..=10_000_000).contains(&v.price) || item.bound {
        return Err(invalid(lang(&st, &c.session).await));
    }
    if !st.app.db.list_item(c.session.user_id, &item.id(), v.price).await? {
        return Err(ApiError::blocked(t!(
            lang(&st, &c.session).await,
            "stall.list-failed",
            max = pinchora_db::MAX_LISTINGS
        )));
    }
    finish(
        &st,
        &c.session,
        "player.stall.list",
        json!({}),
        json!({"item":item.id(),"price":v.price}),
    )
    .await;
    Ok(Json(json!({"ok":true})))
}
async fn unlist_item(State(st): State<WebState>, c: CurrentSession, Path(id): Path<i64>) -> ApiResult<Json<Value>> {
    let item = st
        .app
        .db
        .unlist_item(c.session.user_id, id)
        .await?
        .ok_or_else(ApiError::not_found)?;
    finish(&st, &c.session, "player.stall.unlist", json!({"item":item}), json!({})).await;
    Ok(Json(json!({"ok":true})))
}
async fn pet_list(State(st): State<WebState>, c: CurrentSession) -> ApiResult<Json<Value>> {
    let now = st.app.now();
    let l = lang(&st, &c.session).await;
    let items=st.app.db.pets(c.session.user_id).await?.iter().map(|p| {let n=pinchora_commands::player_pet_needs(p,now); json!({"id":p.id.to_string(),"name":p.name,"species":p.species,"species_name":Content::get().name(&p.species,l),"level":p.level,"xp":p.xp,"active":p.active,"needs":{"hunger":n.hunger,"fun":n.fun,"energy":n.energy,"hygiene":n.hygiene},"status":if p.is_away(now){"away"}else if pinchora_commands::player_pet_asleep(p,now){"asleep"}else if p.has_souvenir(now){"back"}else{"awake"},"trip_dest":p.trip_dest,"trip_ends_at":p.trip_ends_at}) }).collect::<Vec<_>>();
    Ok(Json(
        json!({"items":items,"trips":trips::all().iter().map(|v|json!({"id":v.id,"name":Content::get().name(&v.id,l),"hours":v.hours,"level":v.unlock_level})).collect::<Vec<_>>()}),
    ))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CareInput {
    action: String,
}
async fn care(
    State(st): State<WebState>,
    c: CurrentSession,
    Path(id): Path<i64>,
    ApiJson(v): ApiJson<CareInput>,
) -> ApiResult<Json<Value>> {
    let action = match v.action.as_str() {
        "feed" => Care::Feed,
        "play" => Care::Play,
        "bathe" => Care::Clean,
        "walk" => Care::Walk,
        _ => return Err(invalid(lang(&st, &c.session).await)),
    };
    let user = c.session.user_id;
    let now = st.app.now();
    let l = lang(&st, &c.session).await;
    let pet = st
        .app
        .db
        .pets(user)
        .await?
        .into_iter()
        .find(|p| p.id == id)
        .ok_or_else(ApiError::not_found)?;
    if pet.is_away(now) {
        return Err(ApiError::blocked(t!(l, "trip.cant-leave", name = pet.name)));
    }
    if pinchora_commands::player_pet_asleep(&pet, now) {
        return Err(ApiError::blocked(t!(l, "pet.care.asleep", name = pet.name)));
    }
    if action == Care::Walk
        && let Some(until) = st.app.db.walk_locked_until(user, now).await?
    {
        return Err(ApiError::resting(
            t!(
                l,
                "pet.care.break",
                name = pet.name,
                minutes = (until - now).num_minutes() + 1
            ),
            (until - now).num_seconds().max(1) as u64,
        ));
    }
    let key = format!("pet:{id}:{action:?}");
    if let Some(left) = st
        .app
        .cache
        .try_cooldown(&key, Duration::from_secs(action.cooldown_secs()))
        .await?
    {
        return Err(ApiError::resting(
            t!(l, "pet.care.break", name = pet.name, minutes = left.as_secs() / 60 + 1),
            left.as_secs(),
        ));
    }
    let Some(after) = action.apply(pinchora_commands::player_pet_needs(&pet, now)) else {
        st.app.cache.release_cooldown(&key).await?;
        return Err(ApiError::blocked(t!(l, "pet.care.too-tired", name = pet.name)));
    };
    if action == Care::Feed && st.app.db.spend_shells(user, FOOD_PRICE, "pet_food").await?.is_none() {
        st.app.cache.release_cooldown(&key).await?;
        return Err(ApiError::blocked(t!(l, "pet.care.food-price", price = FOOD_PRICE)));
    }
    let progress = pets::add_xp(pet.level, pet.xp, action.xp());
    let saved = st
        .app
        .db
        .save_pet(
            user,
            &pet,
            [after.hunger, after.fun, after.energy, after.hygiene],
            progress.level,
            progress.xp,
            now,
        )
        .await?;
    if !saved {
        if action == Care::Feed {
            st.app.db.earn_shells(user, FOOD_PRICE, "pet_food_refund").await?;
        }
        st.app.cache.release_cooldown(&key).await?;
        return Err(ApiError::blocked(t!(l, "pet.care.changed", name = pet.name)));
    }
    st.app.quest(user, quests::QuestKind::Care).await;
    st.app.count(user, pinchora_db::Counter::PetCare, 1).await;
    let found = if action == Care::Walk {
        let amount = pets::walk_shells(pet.level, rand::random::<f64>());
        st.app.db.earn_shells(user, amount, "pet_walk").await?;
        amount
    } else {
        0
    };
    finish(
        &st,
        &c.session,
        "player.pets.care",
        json!({}),
        json!({"action":v.action,"pet":id.to_string()}),
    )
    .await;
    Ok(Json(
        json!({"ok":true,"xp":progress.xp,"level":progress.level,"found_shells":found}),
    ))
}
async fn active(State(st): State<WebState>, c: CurrentSession, Path(id): Path<i64>) -> ApiResult<Json<Value>> {
    if !st.app.db.set_active_pet(c.session.user_id, id).await? {
        return Err(ApiError::not_found());
    }
    finish(
        &st,
        &c.session,
        "player.pets.active",
        json!({}),
        json!({"changed":true}),
    )
    .await;
    Ok(Json(json!({"ok":true})))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TripInput {
    dest: String,
    hours: u32,
}
async fn start_trip(
    State(st): State<WebState>,
    c: CurrentSession,
    Path(id): Path<i64>,
    ApiJson(v): ApiJson<TripInput>,
) -> ApiResult<Json<Value>> {
    let trip = trips::get(&v.dest).ok_or_else(ApiError::not_found)?;
    let pet = st
        .app
        .db
        .pets(c.session.user_id)
        .await?
        .into_iter()
        .find(|p| p.id == id)
        .ok_or_else(ApiError::not_found)?;
    let now = st.app.now();
    if trip.hours != v.hours
        || pet.level < trip.unlock_level
        || pet.trip_dest.is_some()
        || pinchora_commands::player_pet_asleep(&pet, now)
    {
        return Err(invalid(lang(&st, &c.session).await));
    }
    let n = pinchora_commands::player_pet_needs(&pet, now);
    let ends = now + ChronoDuration::hours(i64::from(trip.hours));
    if !st
        .app
        .db
        .start_trip(
            c.session.user_id,
            &pet,
            &trip.id,
            [n.hunger, n.fun, n.energy, n.hygiene],
            ends,
        )
        .await?
    {
        return Err(ApiError::blocked(t!(
            lang(&st, &c.session).await,
            "trip.cant-leave",
            name = pet.name
        )));
    }
    finish(
        &st,
        &c.session,
        "player.pets.trip",
        json!({}),
        json!({"dest":trip.id,"pet":id.to_string()}),
    )
    .await;
    Ok(Json(json!({"ok":true,"ends_at":ends})))
}
async fn claim_trips(State(st): State<WebState>, c: CurrentSession) -> ApiResult<Json<Value>> {
    let now = st.app.now();
    let user = c.session.user_id;
    let l = lang(&st, &c.session).await;
    let mut items = Vec::new();
    for pet in st
        .app
        .db
        .pets(user)
        .await?
        .into_iter()
        .filter(|p| p.has_souvenir(now))
        .take(8)
    {
        let rolls = Rolls {
            shells: rand::random(),
            furniture: rand::random(),
            pick: rand::random(),
            story: rand::random(),
        };
        let mut reward = None;
        let result=st.app.db.claim_trip(user,pet.id,now,|p,trip_id|{
            let trip=trips::get(trip_id).unwrap_or(&trips::all()[0]);
            let mut haul=trip.haul_in(Content::get(),&p.name,p.level,rolls,l);
            if pinchora_commands::player_pet_asleep(p,now){haul.xp=0;}
            let progress=pets::add_xp(p.level,p.xp,haul.xp);
            let paid=TripPayout{shells:haul.shells,item:haul.item.clone(),level:progress.level,xp:progress.xp};
            reward=Some(json!({"pet":p.id.to_string(),"shells":haul.shells,"item":haul.item,"xp":haul.xp,"story":haul.story}));
            paid
        }).await?;
        if matches!(result, TripClaim::Claimed { .. })
            && let Some(r) = reward
        {
            items.push(r);
        }
    }
    if !items.is_empty() {
        finish(
            &st,
            &c.session,
            "player.pets.claim",
            json!({}),
            json!({"count":items.len()}),
        )
        .await;
    }
    Ok(Json(json!({"items":items})))
}
async fn wake_pet(State(st): State<WebState>, c: CurrentSession, Path(id): Path<i64>) -> ApiResult<Json<Value>> {
    let now = st.app.now();
    let pet = st
        .app
        .db
        .pets(c.session.user_id)
        .await?
        .into_iter()
        .find(|p| p.id == id)
        .ok_or_else(ApiError::not_found)?;
    if !pinchora_commands::player_pet_asleep(&pet, now) {
        return Err(ApiError::blocked(t!(
            lang(&st, &c.session).await,
            "pet.wake.awake",
            name = pet.name
        )));
    }
    let n = WAKE_NEEDS;
    let result = st
        .app
        .db
        .wake_pet(
            c.session.user_id,
            id,
            pet.needs_at,
            [n.hunger, n.fun, n.energy, n.hygiene],
            now,
            now.timestamp().div_euclid(86_400) as i32,
            WAKE_FEE,
        )
        .await?;
    match result {
        Wake::Woke { free, balance } => {
            finish(&st, &c.session, "player.pets.wake", json!({}), json!({"free":free})).await;
            Ok(Json(json!({"ok":true,"free":free,"balance":balance})))
        }
        Wake::Stale => Err(ApiError::blocked(t!(
            lang(&st, &c.session).await,
            "pet.wake.awake",
            name = pet.name
        ))),
        Wake::Poor { balance } => Err(ApiError::blocked(t!(
            lang(&st, &c.session).await,
            "pet.wake.poor",
            name = pet.name,
            fee = WAKE_FEE,
            balance = balance
        ))),
    }
}
async fn pet_card(State(st): State<WebState>, c: CurrentSession, Path(id): Path<i64>) -> ApiResult<Response> {
    render_limit(&st, &c.session).await?;
    let pet = st
        .app
        .db
        .pets(c.session.user_id)
        .await?
        .into_iter()
        .find(|p| p.id == id)
        .ok_or_else(ApiError::not_found)?;
    Ok(png(pinchora_commands::player_pet_png(
        &st.app,
        lang(&st, &c.session).await,
        &pet,
    )
    .await
    .map_err(render_error)?))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PetName {
    name: String,
}
async fn rename(
    State(st): State<WebState>,
    c: CurrentSession,
    Path(id): Path<i64>,
    ApiJson(v): ApiJson<PetName>,
) -> ApiResult<Json<Value>> {
    let name = v.name.split_whitespace().collect::<Vec<_>>().join(" ");
    if name.is_empty() || name.chars().count() > 20 {
        return Err(invalid(lang(&st, &c.session).await));
    }
    if !st.app.db.rename_pet(c.session.user_id, id, &name).await? {
        return Err(ApiError::not_found());
    }
    finish(&st, &c.session, "player.pets.rename", json!({}), json!({"name":name})).await;
    Ok(Json(json!({"ok":true})))
}
async fn fish_book(State(st): State<WebState>, c: CurrentSession) -> ApiResult<Json<Value>> {
    let l = lang(&st, &c.session).await;
    let rows = st.app.db.fish_book(c.session.user_id).await?;
    Ok(Json(
        json!({"items":Content::get().fish.iter().map(|f|{let seen=rows.iter().find(|r|r.fish_id==f.id);json!({"id":f.id,"name":Content::get().name(&f.id,l),"rarity_name":f.rarity.label_in(l),"caught":seen.map_or(0,|r|r.caught),"best_cm":seen.map(|r|r.best_cm)})}).collect::<Vec<_>>()}),
    ))
}
async fn quest_list(State(st): State<WebState>, c: CurrentSession) -> ApiResult<Json<Value>> {
    let day = Utc::now().timestamp().div_euclid(86_400);
    let user = c.session.user_id;
    let l = lang(&st, &c.session).await;
    let mut items = Vec::new();
    for q in quests::for_day(user, day) {
        let key = q.kind.key();
        let progress = st.app.cache.counter(&format!("quest:{day}:{user}:{key}")).await?;
        let claimed = st
            .app
            .cache
            .counter(&format!("quest-claimed:{day}:{user}:{key}"))
            .await?
            > 0;
        items.push(json!({"kind":key,"text":q.text_in(l),"target":q.target,"progress":progress.min(q.target),"reward":q.reward,"claimed":claimed}));
    }
    Ok(Json(json!({"items":items,"day":day})))
}
async fn guestbook(State(st): State<WebState>, c: CurrentSession, Query(q): Query<Page>) -> ApiResult<Json<Value>> {
    let (rows, total) = st.app.db.guestbook(c.session.user_id, offset(q.page), 24).await?;
    let blocks = st.app.db.guestbook_blocks(c.session.user_id).await?;
    Ok(Json(
        json!({"items":rows.iter().map(|r|json!({"id":r.id.to_string(),"author_id":r.author_id.to_string(),"author_name":r.author_name,"body":r.body,"created_at":r.created_at})).collect::<Vec<_>>(),"total":total,"blocks":blocks.iter().map(|(id,name)|json!({"id":id.to_string(),"name":name})).collect::<Vec<_>>()}),
    ))
}
async fn delete_note(State(st): State<WebState>, c: CurrentSession, Path(id): Path<i64>) -> ApiResult<Json<Value>> {
    if st.app.db.delete_note(c.session.user_id, id).await?.is_none() {
        return Err(ApiError::not_found());
    }
    finish(
        &st,
        &c.session,
        "player.guestbook.delete",
        json!({"exists":true}),
        json!({"exists":false}),
    )
    .await;
    Ok(Json(json!({"ok":true})))
}
async fn block(State(st): State<WebState>, c: CurrentSession, Path(id): Path<u64>) -> ApiResult<Json<Value>> {
    if id == 0 || id > i64::MAX as u64 || id == c.session.user_id {
        return Err(ApiError::not_found());
    }
    let name = st.app.db.remembered_name(id).await?.unwrap_or_else(|| id.to_string());
    st.app.db.block_signer(c.session.user_id, id, &name).await?;
    finish(
        &st,
        &c.session,
        "player.guestbook.block",
        json!({"blocked":false}),
        json!({"blocked":true}),
    )
    .await;
    Ok(Json(json!({"ok":true})))
}
async fn unblock(State(st): State<WebState>, c: CurrentSession, Path(id): Path<u64>) -> ApiResult<Json<Value>> {
    if !st.app.db.unblock_signer(c.session.user_id, id).await? {
        return Err(ApiError::not_found());
    }
    finish(
        &st,
        &c.session,
        "player.guestbook.unblock",
        json!({"blocked":true}),
        json!({"blocked":false}),
    )
    .await;
    Ok(Json(json!({"ok":true})))
}
async fn friends(State(st): State<WebState>, c: CurrentSession, Query(q): Query<Page>) -> ApiResult<Json<Value>> {
    let (rows, total) = st.app.db.friends(c.session.user_id, offset(q.page), 24).await?;
    Ok(Json(
        json!({"items":rows.iter().map(|r|json!({"id":r.friend_id.to_string(),"name":r.name,"since":r.since,"xp":r.xp,"likes":r.likes,"locked":r.locked})).collect::<Vec<_>>(),"total":total}),
    ))
}
async fn partner(State(st): State<WebState>, c: CurrentSession) -> ApiResult<Json<Value>> {
    let p = st.app.db.partner(c.session.user_id).await?;
    let name = st.app.db.partner_name(c.session.user_id).await?;
    Ok(Json(
        json!({"partner":p.map(|v|json!({"id":v.partner_id.to_string(),"name":name,"since":v.since}))}),
    ))
}
