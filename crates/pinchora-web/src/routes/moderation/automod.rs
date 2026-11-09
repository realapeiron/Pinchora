//! AutoMod settings and safe previews; every route uses ManagedGuild.
#[path = "automod_words.rs"]
mod words;
use crate::audit;
use crate::audit::Change;
use crate::error::ApiError;
use crate::error::ApiJson;
use crate::error::ApiResult;
use crate::extract::ManagedGuild;
use crate::guild_info;
use crate::state::WebState;
use axum::{
    Json, Router,
    extract::{Path, State},
    routing::{get, post},
};
use pinchora_core::moderation::automod;
use pinchora_core::moderation::automod::Config;
use pinchora_core::moderation::automod::Input;
use pinchora_core::t;
use serde::Deserialize;
use serde_json::{Value, json};

pub fn router() -> Router<WebState> {
    Router::new()
        .route("/", get(show).put(save))
        .route("/profile", post(apply_profile))
        .route("/test", post(test))
        .route("/strikes/{user}", axum::routing::delete(reset))
}
async fn view(state: &WebState, managed: &ManagedGuild) -> ApiResult<Json<Value>> {
    let guild = managed.id;
    let lang = state.app.guild_lang(guild).await;
    let config = state.app.db.automod_config(guild).await?;
    let hits=state.app.db.automod_hits(guild).await?.into_iter().map(|h|json!({"id":h.id.to_string(),"rule_id":h.rule_id.map(|n|n.to_string()),"user_id":h.user_id.to_string(),"channel_id":h.channel_id.to_string(),"message_id":h.message_id.map(|n|n.to_string()),"action":h.action,"snippet":if state.app.message_content{h.snippet}else{None},"created_at":h.created_at,"reviewed":h.reviewed})).collect::<Vec<_>>();
    let profiles = automod::PROFILES
        .into_iter()
        .map(|name| (name, automod::profile(name, state.app.now()).unwrap()))
        .collect::<std::collections::BTreeMap<_, _>>();
    let sync = state.app.db.automod_sync_status(guild).await?;
    Ok(Json(
        json!({"config":config,"hits":hits,"message_content":state.app.message_content,"max_rules":automod::MAX_RULES,"profiles":profiles,"kinds":automod::KINDS,"actions":automod::ACTIONS,"strings":words::strings(lang),"sync_error":sync.as_ref().and_then(|s|s.sync_error.clone()),"synced_at":sync.and_then(|s|s.synced_at)}),
    ))
}
async fn show(State(state): State<WebState>, managed: ManagedGuild) -> ApiResult<Json<Value>> {
    view(&state, &managed).await
}

async fn persist(state: &WebState, managed: &ManagedGuild, config: Config) -> ApiResult<Json<Value>> {
    let guild = managed.id;
    let lang = state.app.guild_lang(guild).await;
    automod::validate(&config, lang).map_err(ApiError::blocked)?;
    if config
        .rules
        .iter()
        .any(|r| !r.exempt_roles.is_empty() || !r.exempt_channels.is_empty() || !r.only_channels.is_empty())
        || config.alert_channel_id.is_some()
    {
        let info = guild_info::get_with(state, guild, |_| true).await?;
        if let Some(id) = &config.alert_channel_id {
            info.check_post_channel(
                "alert_channel_id",
                id.parse().map_err(|_| ApiError::blocked(t!(lang, "automod.invalid")))?,
            )?;
        }
        for rule in &config.rules {
            for id in rule.exempt_channels.iter().chain(&rule.only_channels) {
                info.check_channel(
                    "rules",
                    id.parse().map_err(|_| ApiError::blocked(t!(lang, "automod.invalid")))?,
                )?;
            }
            for id in &rule.exempt_roles {
                info.check_role(
                    "rules",
                    id.parse().map_err(|_| ApiError::blocked(t!(lang, "automod.invalid")))?,
                )?;
            }
        }
    }
    pinchora_commands::check_automod_roles(&state.app, guild, managed.current.session.user_id, &config)
        .await
        .map_err(|e| ApiError::blocked(e.to_string()))?;
    let before = state.app.db.automod_config(guild).await?;
    state
        .app
        .db
        .save_automod(guild, managed.current.session.user_id, &config)
        .await?;
    audit::finish(
        state,
        &managed.current.session,
        Change {
            guild,
            action: "automod.set",
            target: None,
            before: json!(before),
            after: json!(&config),
        },
        Some(pinchora_cache::Invalidation::Guild(guild)),
    )
    .await;
    view(state, managed).await
}
async fn save(
    State(state): State<WebState>,
    managed: ManagedGuild,
    ApiJson(mut config): ApiJson<Config>,
) -> ApiResult<Json<Value>> {
    // Observation is controlled by profile application, never by an untrusted timestamp.
    config.observe_until = state.app.db.automod_config(managed.id).await?.observe_until;
    persist(&state, &managed, config).await
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ProfileBody {
    profile: String,
}
async fn apply_profile(
    State(state): State<WebState>,
    managed: ManagedGuild,
    ApiJson(body): ApiJson<ProfileBody>,
) -> ApiResult<Json<Value>> {
    let lang = state.app.guild_lang(managed.id).await;
    let old = state.app.db.automod_config(managed.id).await?;
    let config = automod::apply_profile(&old, &body.profile, state.app.now())
        .ok_or_else(|| ApiError::blocked(t!(lang, "automod.invalid_profile")))?;
    persist(&state, &managed, config).await
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TestBody {
    text: String,
    channel_id: Option<String>,
    config: Option<Config>,
}
async fn test(
    State(state): State<WebState>,
    managed: ManagedGuild,
    ApiJson(body): ApiJson<TestBody>,
) -> ApiResult<Json<Value>> {
    let lang = state.app.guild_lang(managed.id).await;
    if body.text.chars().count() > 2000 {
        return Err(ApiError::blocked(t!(lang, "automod.invalid")));
    }
    let config = match body.config {
        Some(c) => c,
        None => state.app.db.automod_config(managed.id).await?,
    };
    automod::validate(&config, lang).map_err(ApiError::blocked)?;
    let channels: Vec<String> = body.channel_id.into_iter().collect();
    let hits = config
        .rules
        .iter()
        .filter(|r| r.applies(&channels, &[]) && (!r.needs_content() || state.app.message_content))
        .filter(|r| {
            automod::matches(
                r,
                &Input {
                    content: &body.text,
                    ..Input::default()
                },
            )
        })
        .map(|r| json!({"name":r.name,"kind":r.kind,"log_only":config.dry(r,state.app.now()),"actions":r.actions}))
        .collect::<Vec<_>>();
    Ok(Json(json!({"matches":hits,"note":t!(lang,"automod.test_note")})))
}
async fn reset(
    State(state): State<WebState>,
    managed: ManagedGuild,
    Path(params): Path<std::collections::HashMap<String, String>>,
) -> ApiResult<Json<Value>> {
    let lang = state.app.guild_lang(managed.id).await;
    let user = params
        .get("user")
        .filter(|s| automod::valid_id(s))
        .and_then(|s| s.parse::<u64>().ok())
        .ok_or_else(|| ApiError::blocked(t!(lang, "automod.invalid")))?;
    state.app.db.reset_automod_strikes(managed.id, user).await?;
    audit::finish(
        &state,
        &managed.current.session,
        Change {
            guild: managed.id,
            action: "automod.strikes.reset",
            target: Some(&user.to_string()),
            before: json!({}),
            after: json!({"user_id":user.to_string()}),
        },
        None,
    )
    .await;
    Ok(Json(json!({"ok":true})))
}
