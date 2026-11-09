//! Safety management uses ManagedGuild; member captcha routes require a bound, CSRF-checked session.
use crate::audit;
use crate::audit::Change;
use crate::error::ApiError;
use crate::error::ApiJson;
use crate::error::ApiResult;
use crate::extract::CurrentSession;
use crate::extract::ManagedGuild;
use crate::state::WebState;
use axum::{
    Json, Router,
    extract::{Path, State},
    routing::{get, post},
};
use base64::{Engine, engine::general_purpose::STANDARD};
use pinchora_core::infra::i18n::Lang;
use pinchora_core::moderation::verification::Config;
use pinchora_core::t;
use serde::Deserialize;
use serde_json::{Value, json};
#[path = "verification_words.rs"]
mod words;
pub fn router() -> Router<WebState> {
    Router::new()
        .route("/", get(show).put(save))
        .route("/preview", post(preview))
        .route("/post", post(panel))
        .route("/raid", post(raid))
}
pub fn member_router() -> Router<WebState> {
    Router::new()
        .route("/{token}", get(member))
        .route("/{token}/captcha", post(captcha))
        .route("/{token}/answer", post(answer))
        .route("/{token}/help", post(help))
}
async fn data(state: &WebState, guild: u64) -> ApiResult<Json<Value>> {
    let lang = state.app.guild_lang(guild).await;
    let events=state.app.db.raid_events(guild).await?.into_iter().map(|e|json!({"id":e.id.to_string(),"started_at":e.started_at,"ended_at":e.ended_at,"expires_at":e.expires_at,"joins":e.joins,"action":e.action,"error":e.error})).collect::<Vec<_>>();
    let joins = state
        .app
        .db
        .raid_joins_since(guild, state.app.now() - chrono::Duration::days(7))
        .await?
        .into_iter()
        .take(50)
        .map(|j| json!({"user_id":j.user_id.to_string(),"joined_at":j.joined_at,"status":j.status}))
        .collect::<Vec<_>>();
    Ok(Json(
        json!({"config":state.app.db.safety_config(guild).await?,"events":events,"joins":joins,"language":lang.code(),"strings":words::strings(lang)}),
    ))
}
async fn show(State(s): State<WebState>, m: ManagedGuild) -> ApiResult<Json<Value>> {
    data(&s, m.id).await
}
async fn save(State(s): State<WebState>, m: ManagedGuild, ApiJson(mut c): ApiJson<Config>) -> ApiResult<Json<Value>> {
    let before = s.app.db.safety_config(m.id).await?;
    c.verification.message_id = before.verification.message_id.clone();
    pinchora_commands::check_verification_roles(&s.app, m.id, m.current.session.user_id, &c)
        .await
        .map_err(|e| ApiError::blocked(e.to_string()))?;
    s.app.db.save_safety(m.id, &c).await?;
    audit::finish(
        &s,
        &m.current.session,
        Change {
            guild: m.id,
            action: "safety.set",
            target: None,
            before: json!(before),
            after: json!(c),
        },
        Some(pinchora_cache::Invalidation::Guild(m.id)),
    )
    .await;
    data(&s, m.id).await
}
async fn preview(State(s): State<WebState>, m: ManagedGuild) -> ApiResult<Json<Value>> {
    let lang = s.app.guild_lang(m.id).await;
    let png = s
        .app
        .render_preview(move |r| {
            r.captcha_card(&pinchora_render::CaptchaCard {
                answer: "A7K3M",
                seed: 345678,
                lang,
            })
        })
        .await
        .map_err(|e| ApiError::blocked(e.to_string()))?;
    Ok(Json(
        json!({"image":format!("data:image/png;base64,{}",STANDARD.encode(png))}),
    ))
}
async fn panel(State(s): State<WebState>, m: ManagedGuild) -> ApiResult<Json<Value>> {
    pinchora_commands::post_verification(&s.app, m.id)
        .await
        .map_err(|e| ApiError::blocked(e.to_string()))?;
    audit::finish(
        &s,
        &m.current.session,
        Change {
            guild: m.id,
            action: "safety.post",
            target: None,
            before: json!(null),
            after: json!({"message_id":s.app.db.safety_config(m.id).await?.verification.message_id}),
        },
        None,
    )
    .await;
    data(&s, m.id).await
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RaidBody {
    action: String,
}
async fn raid(State(s): State<WebState>, m: ManagedGuild, ApiJson(body): ApiJson<RaidBody>) -> ApiResult<Json<Value>> {
    let lang = s.app.guild_lang(m.id).await;
    let before = s.app.db.active_raid(m.id).await?.map(|e| e.id);
    match body.action.as_str() {
        "lockdown" => {
            pinchora_commands::start_raid(&s.app, m.id, Some(m.current.session.user_id), 10, 0, "lockdown").await
        }
        "unlock" => pinchora_commands::unlock_raid(&s.app, m.id, m.current.session.user_id, None).await,
        _ => return Err(ApiError::blocked(t!(lang, "verification.invalid"))),
    }
    .map_err(|e| ApiError::blocked(e.to_string()))?;
    audit::finish(
        &s,
        &m.current.session,
        Change {
            guild: m.id,
            action: "safety.raid",
            target: None,
            before: json!({"event":before}),
            after: json!({"action":body.action,"event":s.app.db.active_raid(m.id).await?.map(|e| e.id)}),
        },
        None,
    )
    .await;
    data(&s, m.id).await
}
async fn identity(s: &WebState, token: &str, current: &CurrentSession) -> ApiResult<(u64, Lang)> {
    let fallback_lang = s
        .app
        .user_lang_choice(current.session.user_id)
        .await
        .unwrap_or_default();
    let guild = pinchora_commands::verification_web_identity(&s.app, token, current.session.user_id)
        .await
        .map_err(|_| ApiError::blocked(t!(fallback_lang, "verification.expired")))?;
    let lang = s.app.guild_lang(guild).await;
    Ok((guild, lang))
}
async fn member(
    State(s): State<WebState>,
    Path(token): Path<String>,
    current: CurrentSession,
) -> ApiResult<Json<Value>> {
    let (_, lang) = identity(&s, &token, &current).await?;
    Ok(Json(json!({"language":lang.code(),"strings":words::strings(lang)})))
}
async fn captcha(
    State(s): State<WebState>,
    Path(token): Path<String>,
    current: CurrentSession,
) -> ApiResult<Json<Value>> {
    let (guild, lang) = identity(&s, &token, &current).await?;
    let (nonce, png) = pinchora_commands::verification_challenge(&s.app, guild, current.session.user_id, lang)
        .await
        .map_err(|e| ApiError::blocked(e.to_string()))?;
    Ok(Json(
        json!({"nonce":nonce,"image":format!("data:image/png;base64,{}",STANDARD.encode(png))}),
    ))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Answer {
    nonce: String,
    answer: String,
}
async fn answer(
    State(s): State<WebState>,
    Path(token): Path<String>,
    current: CurrentSession,
    ApiJson(body): ApiJson<Answer>,
) -> ApiResult<Json<Value>> {
    let (guild, lang) = identity(&s, &token, &current).await?;
    let result =
        pinchora_commands::verification_answer(&s.app, guild, current.session.user_id, &body.nonce, &body.answer, lang)
            .await
            .map_err(|e| ApiError::blocked(e.to_string()))?;
    let done = s
        .app
        .db
        .pending_verification(guild, current.session.user_id)
        .await?
        .is_some_and(|p| p.verified_at.is_some());
    if done {
        s.app.cache.forget(&format!("vf:web:{token}")).await?;
    }
    Ok(Json(json!({"message":result,"done":done})))
}

async fn help(State(s): State<WebState>, Path(token): Path<String>, current: CurrentSession) -> ApiResult<Json<Value>> {
    let (guild, lang) = identity(&s, &token, &current).await?;
    let message = pinchora_commands::verification_help(&s.app, guild, current.session.user_id, lang)
        .await
        .map_err(|e| ApiError::blocked(e.to_string()))?;
    Ok(Json(json!({"message":message})))
}
