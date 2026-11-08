//! Guild-independent player pages. Every route uses the session's player, never a client user id.
use crate::{
    audit::{self, Change},
    auth::session::Session,
    error::{ApiError, ApiJson, ApiResult},
    extract::CurrentSession,
    ratelimit,
    state::WebState,
};
use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::{StatusCode, header},
    response::{IntoResponse, Response},
    routing::{get, post, put},
};
use pinchora_commands::{ProfileContext, profile_card_for};
use pinchora_core::{i18n::Lang, leveling, t};
use pinchora_db::{CustomizeBuy, PlayerProfile, UserRow};
use pinchora_game::{BadgeKind, Content, TitleKind, stat_value};
use pinchora_render::{AccentError, Frame, Opacity, parse_accent};
use serde::Deserialize;
use serde_json::{Value, json};
use std::time::Duration;
use twilight_model::user::User;

#[path = "web_player_character.rs"]
mod character;

#[derive(Default, Deserialize)]
pub(super) struct Locale {
    pub lang: Option<Lang>,
}

pub fn router() -> Router<WebState> {
    Router::new()
        .route("/home", get(home))
        .route("/profile", get(profile).put(save_profile).patch(patch_bio))
        .route("/preview/profile-card.png", get(preview))
        .route("/wallet", get(wallet))
        .route("/preview/wallet-card.png", get(wallet_preview))
        .route("/art/{kind}/{id}", get(art))
        .route("/badges", get(badges).put(save_badges).delete(reset_badges))
        .route("/badges/seen", post(seen_badges))
        .route("/backgrounds", get(backgrounds))
        .route("/titles", get(titles))
        .route("/style", put(save_style).delete(reset_style))
        .route("/showcase", put(save_showcase))
        .route("/background", put(save_background))
        .route("/title", put(save_title))
        .route("/{kind}/{id}/buy", post(buy))
        .nest("/character", character::router())
}

pub(super) async fn lang(state: &WebState, s: &Session) -> Lang {
    state.app.user_lang_choice(s.user_id).await.unwrap_or_default()
}
pub(super) fn invalid(lang: Lang) -> ApiError {
    ApiError::blocked(t!(lang, "web-player.invalid"))
}
pub(super) async fn finish(state: &WebState, s: &Session, action: &str, before: Value, after: Value) {
    // Reserve guild 0 for player-only events. Retained audit rows contain no free text or player target ids.
    audit::finish(
        state,
        s,
        Change {
            guild: 0,
            action,
            target: None,
            before: audit_metadata(before),
            after: audit_metadata(after),
        },
        None,
    )
    .await;
}
fn audit_metadata(mut value: Value) -> Value {
    if let Value::Object(fields) = &mut value {
        for (key, entry) in fields.iter_mut() {
            if matches!(key.as_str(), "bio" | "name") {
                *entry = json!({"characters": entry.as_str().map_or(0, |v| v.chars().count())});
            } else if key == "showcase_pet" {
                *entry = json!(!entry.is_null());
            } else {
                *entry = audit_metadata(entry.take());
            }
        }
    }
    value
}

pub(super) async fn render_limit(state: &WebState, s: &Session) -> ApiResult<()> {
    ratelimit::check(
        &state.app.cache,
        "player-render",
        &s.user_id.to_string(),
        60,
        Duration::from_secs(60),
    )
    .await
}
pub(super) fn png(bytes: Vec<u8>) -> Response {
    (
        [
            (header::CONTENT_TYPE, "image/png"),
            (header::CACHE_CONTROL, "private, no-store"),
        ],
        bytes,
    )
        .into_response()
}
fn discord_user(s: &Session) -> ApiResult<User> {
    serde_json::from_value(json!({"id":s.user_id.to_string(), "username":s.username,
        "global_name":s.global_name, "avatar":s.avatar, "discriminator":"0"}))
    .map_err(|e| ApiError::internal(e.into()))
}

async fn home(State(state): State<WebState>, current: CurrentSession) -> ApiResult<Json<Value>> {
    let id = current.session.user_id;
    let row = state.app.db.user(id).await?;
    let pets = state.app.db.pets(id).await?;
    let house = state.app.db.house_preview(id).await?;
    let progress = leveling::progress(row.xp);
    Ok(Json(
        json!({"shells":row.credits, "pearls":row.pearls, "level":progress.level,
        "xp_current":progress.current, "xp_needed":progress.needed, "streak":state.app.db.player_stats(id).await?.streak,
        "pets":pets.iter().map(|p| json!({"id":p.id.to_string(),"name":p.name,"species":p.species})).collect::<Vec<_>>(),
        "house":house.map(|h| json!({"name":h.house.name,"room":h.room.idx})),
        "discord_url":format!("discord://-/users/{}",state.app.application_id)}),
    ))
}
async fn profile(
    State(state): State<WebState>,
    current: CurrentSession,
    Query(locale): Query<Locale>,
) -> ApiResult<Json<Value>> {
    let s = &current.session;
    let row = state.app.db.user(s.user_id).await?;
    let language = locale.lang.unwrap_or(lang(&state, s).await);
    let content = Content::get();
    let progress = leveling::progress(row.xp);
    let stats = state.app.db.player_stats(s.user_id).await?;
    let fish = state.app.db.fish_book(s.user_id).await?;
    let pets = state.app.db.pets(s.user_id).await?;
    let items = state.app.db.owned_item_ids(s.user_id).await?;
    Ok(Json(
        json!({"profile":PlayerProfile::from_row(&row), "shells":row.credits,"pearls":row.pearls,"xp":row.xp,"level":progress.level, "xp_current":progress.current,"xp_needed":progress.needed,"reputation":row.reputation,"streak":stats.streak,
        "badges":badge_catalog(&state,s,&row,language).await?,
        "backgrounds":background_catalog(&state,s,language).await?,
        "titles":title_catalog(&state,s,language).await?,
        "frames":Frame::ALL.map(|f| json!({"id":f.id(),"name":t!(language,&format!("customize.style.frame.{}",f.id())),"level":f.unlock_level()})),
        "opacities":Opacity::ALL.map(|o| json!({"id":o.index(),"name":t!(language,&format!("customize.style.opacity.{}",o.label().to_lowercase()))})),
        "accents":pinchora_render::ACCENT_PRESETS.iter().map(|(name,rgb)| json!({"name":t!(language,&format!("customize.style.accent.{}",name.to_lowercase())),"value":format!("#{rgb:06x}")})).collect::<Vec<_>>(),
        "showcase": {"fish":fish.iter().map(|f|json!({"id":f.fish_id,"name":content.name(&f.fish_id,language)})).collect::<Vec<_>>(),
            "pets":pets.iter().map(|p|json!({"id":p.id.to_string(),"name":p.name})).collect::<Vec<_>>(),
            "items":items.iter().map(|id|json!({"id":id,"name":content.name(id,language)})).collect::<Vec<_>>(),
            "house":state.app.db.has_house_room(s.user_id).await?}}),
    ))
}
async fn badge_catalog(state: &WebState, s: &Session, row: &UserRow, language: Lang) -> ApiResult<Vec<Value>> {
    let c = Content::get();
    let stats = state.app.db.player_stats(s.user_id).await?;
    let owned = state.app.db.owned_badges(s.user_id).await?;
    Ok(c.badges.iter().map(|b| {
        let earned = b.is_earned(&stats,&owned);
        let (progress,target,unit,price) = match b.kind {
            BadgeKind::Achievement{stat,at} => (Some(stat_value(&stats,stat).max(0)),Some(at),Some(stat.noun_in(language)),None),
            BadgeKind::Shop{price} => (None,None,None,Some(price)),
            BadgeKind::Granted => (None,None,None,None),
        };
        json!({"id":b.id,"name":c.name(&b.id,language),"description":c.description(&b.id,language).unwrap_or_else(|| b.description.as_str().into()),
            "earned":earned,"new":earned && row.badges_seen.as_ref().is_some_and(|seen|!seen.contains(&b.id)),
            "progress":progress,"target":target,"unit":unit,"price":price})
    }).collect())
}
async fn background_catalog(state: &WebState, s: &Session, l: Lang) -> ApiResult<Vec<Value>> {
    let owned = state.app.db.owned_backgrounds(s.user_id).await?;
    Ok(Content::get()
        .backgrounds
        .iter()
        .map(|b| json!({"id":b.id,"name":Content::get().name(&b.id,l),"price":b.price,"owned":owned.contains(&b.id)}))
        .collect())
}
async fn title_catalog(state: &WebState, s: &Session, l: Lang) -> ApiResult<Vec<Value>> {
    let c = Content::get();
    let stats = state.app.db.player_stats(s.user_id).await?;
    let badges = state.app.db.owned_badges(s.user_id).await?;
    let granted = state.app.db.granted_titles(s.user_id).await?;
    Ok(c.titles.iter().map(|title| {
        let owned = granted.contains(&title.id) || match &title.kind {
            TitleKind::Stat{stat,at} => stat_value(&stats,*stat) >= *at,
            TitleKind::Badge{badge} => c.badge(badge).is_some_and(|b|b.is_earned(&stats,&badges)),
            TitleKind::Special => false,
        };
        json!({"id":title.id,"name":c.name(&title.id,l),"description":c.description(&title.id,l).unwrap_or_else(|| title.description.as_str().into()),"owned":owned})
    }).collect())
}
async fn badges(State(state): State<WebState>, current: CurrentSession) -> ApiResult<Json<Value>> {
    let s = &current.session;
    let row = state.app.db.user(s.user_id).await?;
    Ok(Json(
        json!({"items":badge_catalog(&state,s,&row,lang(&state,s).await).await?,"ids":row.badges,"auto":!row.badges_set}),
    ))
}
async fn backgrounds(State(state): State<WebState>, current: CurrentSession) -> ApiResult<Json<Value>> {
    Ok(Json(
        json!({"items":background_catalog(&state,&current.session,lang(&state,&current.session).await).await?}),
    ))
}
async fn titles(State(state): State<WebState>, current: CurrentSession) -> ApiResult<Json<Value>> {
    Ok(Json(
        json!({"items":title_catalog(&state,&current.session,lang(&state,&current.session).await).await?}),
    ))
}
async fn seen_badges(State(state): State<WebState>, current: CurrentSession) -> ApiResult<StatusCode> {
    let db = &state.app.db;
    let id = current.session.user_id;
    db.user(id).await?;
    let earned: Vec<String> = Content::get()
        .earned_badges(&db.player_stats(id).await?, &db.owned_badges(id).await?)
        .iter()
        .map(|b| b.id.clone())
        .collect();
    db.mark_badges_seen(id, &earned).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn validate(state: &WebState, s: &Session, row: &UserRow, p: &mut PlayerProfile, preview: bool) -> ApiResult<()> {
    let l = lang(state, s).await;
    let c = Content::get();
    let db = &state.app.db;
    let id = s.user_id;
    if p.bio.chars().count() > 300
        || !(0..=2).contains(&p.panel_opacity)
        || p.badges.len() > 6
        || p.badges.iter().enumerate().any(|(i, b)| p.badges[..i].contains(b))
    {
        return Err(invalid(l));
    }
    if let Some(accent) = &p.accent {
        p.accent = Some(format!(
            "#{:06x}",
            parse_accent(accent).map_err(|e| ApiError::blocked(t!(
                l,
                match e {
                    AccentError::Malformed => "customize.style.refusal-malformed",
                    AccentError::TooDark => "customize.style.refusal-dark",
                }
            )))?
        ));
    }
    let frame = Frame::from_id(&p.frame).ok_or_else(|| invalid(l))?;
    let level = leveling::level_of(row.xp);
    if !frame.is_unlocked(level) {
        return Err(ApiError::blocked(t!(
            l,
            "customize.style.frame-refused",
            frame = t!(l, &format!("customize.style.frame.{}", frame.id())),
            at = frame.unlock_level(),
            level = level
        )));
    }
    if let Some(bg) = &p.background
        && (c.background(bg).is_none() || (!preview && !db.owned_backgrounds(id).await?.contains(bg)))
    {
        return Err(invalid(l));
    }
    let stats = db.player_stats(id).await?;
    let owned = db.owned_badges(id).await?;
    if p.badges
        .iter()
        .any(|b| !c.badge(b).is_some_and(|b| b.is_earned(&stats, &owned)))
    {
        return Err(invalid(l));
    }
    if let Some(title) = &p.title
        && !title_catalog(state, s, l)
            .await?
            .iter()
            .any(|t| t["id"] == *title && t["owned"] == true)
    {
        return Err(invalid(l));
    }
    if let Some(fish) = &p.showcase_fish
        && (c.fish(fish).is_none() || !db.has_fish(id, fish).await?)
    {
        return Err(invalid(l));
    }
    if let Some(pet) = &p.showcase_pet {
        let pet = pet.parse::<i64>().map_err(|_| invalid(l))?;
        if db.pet_name_species(id, pet).await?.is_none() {
            return Err(invalid(l));
        }
    }
    if let Some(item) = &p.showcase_item
        && (c.item(item).is_none() || !db.owns_item(id, item).await?)
    {
        return Err(invalid(l));
    }
    if p.showcase_house && !db.has_house_room(id).await? {
        return Err(invalid(l));
    }
    Ok(())
}
async fn persist(state: &WebState, s: &Session, mut p: PlayerProfile) -> ApiResult<Json<Value>> {
    let row = state.app.db.user(s.user_id).await?;
    validate(state, s, &row, &mut p, false).await?;
    let before = serde_json::to_value(PlayerProfile::from_row(&row)).map_err(|e| ApiError::internal(e.into()))?;
    let after = json!(p);
    if before != after {
        state.app.db.save_player_profile(s.user_id, &p).await?;
        finish(state, s, "player.profile", before, after.clone()).await;
    }
    Ok(Json(after))
}
async fn save_profile(
    State(state): State<WebState>,
    current: CurrentSession,
    ApiJson(p): ApiJson<PlayerProfile>,
) -> ApiResult<Json<Value>> {
    persist(&state, &current.session, p).await
}

#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct Preview {
    lang: Option<Lang>,
    draft: Option<String>,
    accent: Option<String>,
    opacity: Option<i16>,
    frame: Option<String>,
    background: Option<String>,
    badges: Option<String>,
    title: Option<String>,
    showcase_fish: Option<String>,
    showcase_pet: Option<String>,
    showcase_item: Option<String>,
    showcase_house: Option<bool>,
}
async fn preview(
    State(state): State<WebState>,
    current: CurrentSession,
    Query(q): Query<Preview>,
) -> ApiResult<Response> {
    let s = &current.session;
    render_limit(&state, s).await?;
    let mut row = state.app.db.user(s.user_id).await?;
    let l = q.lang.unwrap_or(lang(&state, s).await);
    let mut p = if let Some(draft) = q.draft {
        if draft.len() > 12000 {
            return Err(invalid(l));
        }
        serde_json::from_str(&draft).map_err(|_| invalid(l))?
    } else {
        PlayerProfile::from_row(&row)
    };
    if let Some(v) = q.accent {
        p.accent = (!v.is_empty()).then_some(v);
    }
    if let Some(v) = q.opacity {
        p.panel_opacity = v;
    }
    if let Some(v) = q.frame {
        p.frame = v;
    }
    if let Some(v) = q.background {
        p.background = (!v.is_empty()).then_some(v);
    }
    if let Some(v) = q.title {
        p.title = (!v.is_empty()).then_some(v);
    }
    if let Some(v) = q.badges {
        p.badges = v.split(',').filter(|v| !v.is_empty()).map(str::to_owned).collect();
        p.badges_set = true;
    }
    if let Some(v) = q.showcase_fish {
        p.showcase_fish = (!v.is_empty()).then_some(v);
    }
    if let Some(v) = q.showcase_pet {
        p.showcase_pet = (!v.is_empty()).then_some(v);
    }
    if let Some(v) = q.showcase_item {
        p.showcase_item = (!v.is_empty()).then_some(v);
    }
    if let Some(v) = q.showcase_house {
        p.showcase_house = v;
    }
    // Existing selections may have become unavailable (a spent-shell badge, a gifted item).
    // The shared assembler already hides these; only changed drafts need validation.
    if p != PlayerProfile::from_row(&row) {
        validate(&state, s, &row, &mut p, true).await?;
    }
    p.apply(&mut row);
    Ok(png(profile_card_for(
        &state.app,
        &discord_user(s)?,
        s.display_name().into(),
        Some(row),
        ProfileContext {
            guild: None,
            viewer: s.user_id,
            lang: l,
        },
    )
    .await?))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Bio {
    bio: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct IdChoice {
    id: Option<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct BadgeChoice {
    ids: Vec<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Style {
    accent: Option<String>,
    panel_opacity: i16,
    frame: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Showcase {
    fish: Option<String>,
    pet: Option<String>,
    item: Option<String>,
    house: bool,
}
async fn draft(state: &WebState, s: &Session) -> ApiResult<PlayerProfile> {
    Ok(PlayerProfile::from_row(&state.app.db.user(s.user_id).await?))
}
async fn patch_bio(State(st): State<WebState>, c: CurrentSession, ApiJson(v): ApiJson<Bio>) -> ApiResult<Json<Value>> {
    let mut p = draft(&st, &c.session).await?;
    p.bio = v.bio;
    persist(&st, &c.session, p).await
}
async fn save_background(
    State(st): State<WebState>,
    c: CurrentSession,
    ApiJson(v): ApiJson<IdChoice>,
) -> ApiResult<Json<Value>> {
    let mut p = draft(&st, &c.session).await?;
    p.background = v.id;
    persist(&st, &c.session, p).await
}
async fn save_title(
    State(st): State<WebState>,
    c: CurrentSession,
    ApiJson(v): ApiJson<IdChoice>,
) -> ApiResult<Json<Value>> {
    let mut p = draft(&st, &c.session).await?;
    p.title = v.id;
    persist(&st, &c.session, p).await
}
async fn save_badges(
    State(st): State<WebState>,
    c: CurrentSession,
    ApiJson(v): ApiJson<BadgeChoice>,
) -> ApiResult<Json<Value>> {
    let mut p = draft(&st, &c.session).await?;
    p.badges = v.ids;
    p.badges_set = true;
    persist(&st, &c.session, p).await
}
async fn reset_badges(State(st): State<WebState>, c: CurrentSession) -> ApiResult<Json<Value>> {
    let mut p = draft(&st, &c.session).await?;
    p.badges.clear();
    p.badges_set = false;
    persist(&st, &c.session, p).await
}
async fn save_style(
    State(st): State<WebState>,
    c: CurrentSession,
    ApiJson(v): ApiJson<Style>,
) -> ApiResult<Json<Value>> {
    let mut p = draft(&st, &c.session).await?;
    p.accent = v.accent;
    p.panel_opacity = v.panel_opacity;
    p.frame = v.frame;
    persist(&st, &c.session, p).await
}
async fn reset_style(State(st): State<WebState>, c: CurrentSession) -> ApiResult<Json<Value>> {
    let mut p = draft(&st, &c.session).await?;
    p.accent = None;
    p.panel_opacity = 1;
    p.frame = "plain".into();
    persist(&st, &c.session, p).await
}
async fn save_showcase(
    State(st): State<WebState>,
    c: CurrentSession,
    ApiJson(v): ApiJson<Showcase>,
) -> ApiResult<Json<Value>> {
    let mut p = draft(&st, &c.session).await?;
    p.showcase_fish = v.fish;
    p.showcase_pet = v.pet;
    p.showcase_item = v.item;
    p.showcase_house = v.house;
    persist(&st, &c.session, p).await
}

async fn buy(
    State(state): State<WebState>,
    current: CurrentSession,
    Path((kind, id)): Path<(String, String)>,
) -> ApiResult<Json<Value>> {
    let c = Content::get();
    let s = &current.session;
    let l = lang(&state, s).await;
    let result = match kind.as_str() {
        "backgrounds" => {
            let b = c.background(&id).ok_or_else(|| invalid(l))?;
            state.app.db.buy_background(s.user_id, &id, b.price).await?
        }
        "badges" => {
            let b = c.badge(&id).ok_or_else(|| invalid(l))?;
            let BadgeKind::Shop { price } = b.kind else {
                return Err(invalid(l));
            };
            state.app.db.buy_badge(s.user_id, &id, price).await?
        }
        _ => return Err(ApiError::not_found()),
    };
    purchase(&state, s, &kind, &id, result).await
}
pub(super) async fn purchase(
    state: &WebState,
    s: &Session,
    kind: &str,
    id: &str,
    result: CustomizeBuy,
) -> ApiResult<Json<Value>> {
    let result = match result {
        CustomizeBuy::Bought { balance } => {
            let after = json!({"kind":kind,"id":id,"balance":balance});
            finish(state, s, "player.buy", Value::Null, after.clone()).await;
            after
        }
        CustomizeBuy::AlreadyOwned => json!({"owned":true}),
        CustomizeBuy::Insufficient { .. } => {
            return Err(ApiError::blocked(t!(lang(state, s).await, "web-player.insufficient")));
        }
    };
    Ok(Json(result))
}
async fn wallet(State(state): State<WebState>, current: CurrentSession) -> ApiResult<Json<Value>> {
    let row = state.app.db.user(current.session.user_id).await?;
    Ok(Json(
        json!({"shells":row.credits,"pearls":row.pearls,"ledger":state.app.db.player_ledger(current.session.user_id).await?}),
    ))
}

async fn art(
    State(state): State<WebState>,
    _current: CurrentSession,
    Path((kind, id)): Path<(String, String)>,
) -> ApiResult<Response> {
    let art = match kind.as_str() {
        "badge" => state.app.sprites.badge(&id),
        "background" => state.app.sprites.background(&id),
        _ => None,
    }
    .ok_or_else(ApiError::not_found)?;
    Ok(png(art.png.clone()))
}
async fn wallet_preview(
    State(state): State<WebState>,
    current: CurrentSession,
    Query(locale): Query<Locale>,
) -> ApiResult<Response> {
    let s = &current.session;
    render_limit(&state, s).await?;
    Ok(png(pinchora_commands::wallet_card_for(
        &state.app,
        &discord_user(s)?,
        s.display_name().into(),
        locale.lang.unwrap_or(lang(&state, s).await),
    )
    .await?))
}
