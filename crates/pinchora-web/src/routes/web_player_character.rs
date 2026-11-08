//! Character editor adapters over W15's decoder, shelf rules, randomiser and compositor.
use super::*;
use pinchora_game::character::{self as game, Access, Character, Shelf};
use pinchora_render::character::{ACCESSORY_TINTS, Channel};

pub(super) fn router() -> Router<WebState> {
    Router::new()
        .route("/", get(read).put(save))
        .route("/preview.png", get(preview))
        .route("/randomise", post(randomise))
        .route("/accessories/{id}/buy", post(buy))
        .route("/outfits/{slot}", put(outfit_save).delete(outfit_delete))
        .route("/outfits/{slot}/wear", post(outfit_wear))
}
async fn shelf(state: &WebState, s: &Session) -> ApiResult<Shelf> {
    Ok(Shelf::new(
        Content::get(),
        &state.app.db.wardrobe_facts(s.user_id).await?,
    ))
}
async fn checked(state: &WebState, s: &Session, code: &str) -> ApiResult<Character> {
    let l = lang(state, s).await;
    if code.len() > 400 {
        return Err(invalid(l));
    }
    let c = Character::decode(Content::get(), code).ok_or_else(|| invalid(l))?;
    if shelf(state, s).await?.blocker(&c).is_some() {
        return Err(ApiError::blocked(t!(l, "web-player.accessory-locked")));
    }
    Ok(c)
}
async fn read(
    State(state): State<WebState>,
    current: CurrentSession,
    Query(locale): Query<Locale>,
) -> ApiResult<Json<Value>> {
    let s = &current.session;
    let l = locale.lang.unwrap_or(lang(&state, s).await);
    let c = Content::get();
    let shelf = shelf(&state, s).await?;
    let spec = state
        .app
        .db
        .character(s.user_id)
        .await?
        .and_then(|v| Character::decode(c, &v))
        .unwrap_or_else(|| Character::starter(c))
        .encode();
    let outfits = state.app.db.outfits(s.user_id).await?;
    Ok(Json(
        json!({"spec":spec,"shells":state.app.db.user(s.user_id).await?.credits,
        "catalog":{"looks":c.looks.iter().map(|v|json!({"id":v.id,"name":c.name(&v.id,l)})).collect::<Vec<_>>(),
        "channels":Channel::ALL.map(|ch|json!({"id":ch.key(),"name":t!(l,&format!("character.channel.{}",ch.key())),
            "swatches":ch.swatches().iter().enumerate().map(|(i,sw)|json!({"id":i,"value":format!("#{:06x}",sw.rgb)})).collect::<Vec<_>>()})),
        "tints":ACCESSORY_TINTS.iter().enumerate().map(|(i,sw)|json!({"id":i,"value":format!("#{:06x}",sw.rgb)})).collect::<Vec<_>>(),
        "accessories":game::accessories().iter().map(|a|{
            let access=shelf.access(a);let owned=access==Access::Owned;
            let unlock=match &access {Access::Level(at)=>t!(l,"web-player.unlock-level",level=at),Access::Badge(b)=>t!(l,"web-player.unlock-badge",badge=c.name(b,l)),_=>String::new()};
            json!({"id":a.id,"name":c.name(&a.id,l),"slot":a.slot,"tintable":a.tintable,"price":a.price,"owned":owned,"buyable":matches!(access,Access::Buy(_)),"unlock":unlock})
        }).collect::<Vec<_>>(),
        "presets":game::presets().iter().map(|p|json!({"id":p.id,"name":c.name(&p.id,l),"spec":p.code})).collect::<Vec<_>>()},
        "outfits":outfits.iter().map(|o|json!({"slot":o.slot,"name":o.name,"spec":o.code})).collect::<Vec<_>>()}),
    ))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Spec {
    spec: String,
}
async fn save(
    State(state): State<WebState>,
    current: CurrentSession,
    ApiJson(v): ApiJson<Spec>,
) -> ApiResult<Json<Value>> {
    let s = &current.session;
    let c = checked(&state, s, &v.spec).await?;
    let before = state.app.db.character(s.user_id).await?;
    let code = c.encode();
    if before.as_ref() != Some(&code) {
        state.app.db.set_character(s.user_id, &code).await?;
        finish(&state, s, "player.character", json!(before), json!(code)).await;
    }
    Ok(Json(json!({"spec":code})))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Preview {
    lang: Option<Lang>,
    spec: String,
    scale: Option<u32>,
}
async fn preview(
    State(state): State<WebState>,
    current: CurrentSession,
    Query(q): Query<Preview>,
) -> ApiResult<Response> {
    let s = &current.session;
    let _language = q.lang;
    render_limit(&state, s).await?;
    let c = checked(&state, s, &q.spec).await?;
    let scale = q.scale.unwrap_or(4);
    if !(1..=4).contains(&scale) {
        return Err(invalid(lang(&state, s).await));
    }
    let sprites = state.app.sprites.clone();
    let bytes = state
        .app
        .render(move |_| {
            let img = sprites.character(&c);
            pinchora_render::encode_png(&image::imageops::resize(
                img.as_ref(),
                img.width() * scale,
                img.height() * scale,
                image::imageops::FilterType::Nearest,
            ))
        })
        .await?;
    Ok(png(bytes))
}
async fn randomise(State(state): State<WebState>, current: CurrentSession) -> ApiResult<Json<Value>> {
    let shelf = shelf(&state, &current.session).await?;
    let c = pinchora_commands::randomise_character(Content::get(), &shelf, &mut rand::rng());
    Ok(Json(json!({"spec":c.encode()})))
}
async fn buy(State(state): State<WebState>, current: CurrentSession, Path(id): Path<String>) -> ApiResult<Json<Value>> {
    let s = &current.session;
    let l = lang(&state, s).await;
    let item = game::accessory(&id).ok_or_else(|| invalid(l))?;
    let result = match shelf(&state, s).await?.access(item) {
        Access::Owned => CustomizeBuy::AlreadyOwned,
        Access::Buy(price) => state.app.db.buy_accessory(s.user_id, &id, price).await?,
        _ => return Err(ApiError::blocked(t!(l, "web-player.accessory-locked"))),
    };
    purchase(&state, s, "accessories", &id, result).await
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Outfit {
    name: Option<String>,
    spec: Option<String>,
}
async fn outfit_save(
    State(state): State<WebState>,
    current: CurrentSession,
    Path(slot): Path<i16>,
    ApiJson(v): ApiJson<Outfit>,
) -> ApiResult<StatusCode> {
    let s = &current.session;
    let l = lang(&state, s).await;
    if !(1..=3).contains(&slot)
        || v.name
            .as_ref()
            .is_some_and(|v| !(1..=32).contains(&v.trim().chars().count()))
    {
        return Err(invalid(l));
    }
    let code = match v.spec {
        Some(code) => code,
        None => state
            .app
            .db
            .character(s.user_id)
            .await?
            .unwrap_or_else(|| Character::starter(Content::get()).encode()),
    };
    let c = checked(&state, s, &code).await?;
    let before = state
        .app
        .db
        .outfits(s.user_id)
        .await?
        .into_iter()
        .find(|o| o.slot == slot)
        .map(|o| json!({"spec":o.code,"name":o.name}));
    state
        .app
        .db
        .save_player_outfit(s.user_id, slot, &c.encode(), v.name.as_deref())
        .await?;
    finish(
        &state,
        s,
        "player.outfit.save",
        json!(before),
        json!({"slot":slot,"spec":c.encode(),"name":v.name}),
    )
    .await;
    Ok(StatusCode::NO_CONTENT)
}
async fn outfit_delete(
    State(state): State<WebState>,
    current: CurrentSession,
    Path(slot): Path<i16>,
) -> ApiResult<StatusCode> {
    let s = &current.session;
    if !(1..=3).contains(&slot) {
        return Err(invalid(lang(&state, s).await));
    }
    let before = state
        .app
        .db
        .outfits(s.user_id)
        .await?
        .into_iter()
        .find(|o| o.slot == slot)
        .map(|o| json!({"slot":slot,"spec":o.code,"name":o.name}));
    state.app.db.delete_player_outfit(s.user_id, slot).await?;
    if before.is_some() {
        finish(&state, s, "player.outfit.delete", json!(before), Value::Null).await;
    }
    Ok(StatusCode::NO_CONTENT)
}
async fn outfit_wear(
    State(state): State<WebState>,
    current: CurrentSession,
    Path(slot): Path<i16>,
) -> ApiResult<Json<Value>> {
    let outfit = state
        .app
        .db
        .outfits(current.session.user_id)
        .await?
        .into_iter()
        .find(|o| o.slot == slot)
        .ok_or_else(ApiError::not_found)?;
    save(State(state), current, ApiJson(Spec { spec: outfit.code })).await
}
