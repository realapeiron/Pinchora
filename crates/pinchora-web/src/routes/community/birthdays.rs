//! Birthday setup and upcoming public dates for the Community dashboard.

use axum::{Json, Router, extract::State, routing::get};

use crate::audit;
use crate::audit::Change;
use crate::error::ApiError;
use crate::error::ApiJson;
use crate::error::ApiResult;
use crate::extract::ManagedGuild;
use crate::guild_info;
use crate::state::WebState;
use pinchora_cache::Invalidation;
use pinchora_core::admin::settings::FieldError;
use pinchora_core::community::birthdays;
use pinchora_core::community::birthdays::BirthdaySettings;
use pinchora_core::t;
use serde::Deserialize;
use serde_json::{Value, json};

pub fn router() -> Router<WebState> {
    Router::new().route("/", get(show).put(save))
}

async fn view(state: &WebState, guild: u64) -> ApiResult<Value> {
    let s = state.app.db.birthday_settings(guild).await?;
    let now = state.app.now();
    let mut dates = state
        .app
        .db
        .public_birthdays(guild)
        .await?
        .into_iter()
        .filter_map(|b| {
            let tz = birthdays::timezone(&b.tz)?;
            let date = birthdays::next_date(b.month as u32, b.day as u32, now, tz)?;
            let days = (date - birthdays::local_date(now, tz)).num_days();
            (days <= 30).then_some((days, b, date))
        })
        .collect::<Vec<_>>();
    dates.sort_by_key(|(days, b, _)| (*days, b.user_id));
    let lang = state.app.guild_lang(guild).await;
    let upcoming = futures_util::future::join_all(dates.into_iter().take(30).map(|(_, b, date)| async move {
        let name = state
            .app
            .user(b.user_id as u64)
            .await
            .map(|u| u.global_name.clone().unwrap_or_else(|| u.name.clone()))
            .unwrap_or_else(|| t!(lang, "birthdays.member"));
        json!({"user_id":b.user_id.to_string(),"name":name,"date":date.to_string(),"month":b.month,"day":b.day})
    }))
    .await;
    Ok(json!({
        "language": lang.code(),
        "strings": pinchora_core::infra::i18n::keys(lang).into_iter().filter_map(|key| key.strip_prefix("birthdays.ui.").map(|name| (name.to_owned(), t!(lang, &key)))).collect::<std::collections::BTreeMap<_, _>>(),
        "settings": {
            "enabled": s.enabled,
            "channel_id": s.channel_id.map(|id| id.to_string()),
            "role_id": s.role_id.map(|id| id.to_string()),
            "message": if s.message == BirthdaySettings::default().message { t!(lang, "birthdays.default-message") } else { s.message },
            "announce_hour": s.announce_hour,
            "server_tz": s.server_tz,
            "shells_gift": s.shells_gift,
            "allow_age": s.allow_age,
            "badge": s.badge,
        },
        "upcoming": upcoming,
        "limit": 30,
        "today": now.date_naive().to_string()
    }))
}
async fn show(State(state): State<WebState>, managed: ManagedGuild) -> ApiResult<Json<Value>> {
    Ok(Json(view(&state, managed.id).await?))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Save {
    enabled: bool,
    channel_id: Option<String>,
    role_id: Option<String>,
    message: String,
    announce_hour: i16,
    server_tz: String,
    shells_gift: i16,
    allow_age: bool,
    badge: bool,
}
fn id(value: Option<String>, lang: pinchora_core::infra::i18n::Lang) -> ApiResult<Option<u64>> {
    value
        .map(|s| {
            s.parse::<u64>()
                .ok()
                .filter(|id| *id > 0 && *id <= i64::MAX as u64)
                .ok_or(())
                .map_err(|_| ApiError::malformed(t!(lang, "birthdays.invalid-id")))
        })
        .transpose()
}
async fn save(
    State(state): State<WebState>,
    managed: ManagedGuild,
    ApiJson(body): ApiJson<Save>,
) -> ApiResult<Json<Value>> {
    let guild = managed.id;
    let lang = state.app.guild_lang(guild).await;
    let s = BirthdaySettings {
        enabled: body.enabled,
        channel_id: id(body.channel_id, lang)?,
        role_id: id(body.role_id, lang)?,
        message: body.message,
        announce_hour: body.announce_hour,
        server_tz: body.server_tz,
        shells_gift: body.shells_gift,
        allow_age: body.allow_age,
        badge: body.badge,
    };
    if let Err(field) = birthdays::check_settings(&s) {
        return Err(ApiError::invalid(vec![FieldError::new(
            field,
            t!(
                lang,
                "birthdays.invalid-settings",
                field = t!(lang, &format!("birthdays.field.{field}"))
            ),
        )]));
    }
    if s.channel_id.is_some() || s.role_id.is_some() {
        let info = guild_info::get_with(&state, guild, |i| s.channel_id.is_none_or(|c| i.channel(c).is_some())).await?;
        if let Some(c) = s.channel_id {
            info.check_post_channel("channel_id", c)?;
        }
        if let Some(role) = s.role_id {
            let live = guild_info::live(&state, guild).await?;
            let grantor = guild_info::grantor(&state, &managed, &live.roles, true).await?;
            live.check_grant("role_id", role, grantor)?;
        }
    }
    let before = state.app.db.birthday_settings(guild).await?;
    state.app.db.save_birthday_settings(guild, &s).await?;
    audit::finish(
        &state,
        &managed.current.session,
        Change {
            guild,
            action: "birthdays.set",
            target: None,
            before: json!(before),
            after: json!(s),
        },
        Some(Invalidation::Guild(guild)),
    )
    .await;
    Ok(Json(view(&state, guild).await?))
}
