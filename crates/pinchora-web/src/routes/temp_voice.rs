//! Temp voice: the creator channels a server set up (what a room is called, how many it takes, whether it
//! starts locked, whether it gets a panel) and the rooms alive right now, which are read-only. Same rules as
//! `/voice creators`.

use std::collections::HashMap;

use axum::{
    Json, Router,
    extract::{Path, State},
    routing::{get, put},
};
use pinchora_cache::Invalidation;
use pinchora_core::{
    i18n::Lang,
    modules::Module,
    settings::FieldError,
    t,
    temp_voice::{
        DEFAULT_TEMPLATE, MAX_CREATORS, MAX_LIMIT, MAX_PER_USER, MAX_ROOMS, MAX_TEMPLATE, check_limit, check_per_user,
        check_template,
    },
};
use pinchora_db::{CreatorSave, TempRoom, VoiceCreator};
use serde::Deserialize;
use serde_json::{Value, json};

use super::settings::{parse_id, present};
use crate::{
    audit::{self, Change},
    error::{ApiError, ApiJson, ApiResult},
    extract::ManagedGuild,
    guild_info::{self, GuildInfo},
    state::WebState,
};

/// Voice channel and category, as Discord numbers channel types.
const VOICE: u8 = 2;
const CATEGORY: u8 = 4;
/// Owners looked up for the live list, at most.
const OWNER_LOOKUPS: usize = 25;

pub fn router() -> Router<WebState> {
    Router::new()
        .route("/", get(overview))
        .route("/creators/{channel}", put(put_creator).delete(delete_creator))
}

/// A creator as the audit log keeps it: every setting, no live counts.
fn audit_json(creator: &VoiceCreator) -> Value {
    let mut all = creator_json(creator, 0);
    if let Some(object) = all.as_object_mut() {
        object.remove("rooms");
    }
    all
}

fn creator_json(creator: &VoiceCreator, rooms: usize) -> Value {
    json!({
        "channel_id": creator.channel_id.to_string(),
        "category_id": creator.category_id.map(|id| id.to_string()),
        "name_template": creator.name_template,
        "default_limit": creator.default_limit,
        "default_locked": creator.default_locked,
        "max_per_user": creator.max_per_user,
        "panel": creator.panel,
        "panel_channel_id": creator.panel_channel_id.map(|id| id.to_string()),
        "enabled": creator.enabled,
        "rooms": rooms,
    })
}

fn room_json(room: &TempRoom, owner_name: Option<&str>, members: usize) -> Value {
    json!({
        "channel_id": room.channel_id.to_string(),
        "source_id": room.source_id.to_string(),
        "owner_id": room.owner_id.map(|id| id.to_string()),
        "owner_name": owner_name,
        "created_at": room.created_at,
        "locked": room.locked,
        "hidden": room.hidden,
        "user_limit": room.user_limit,
        "members": members,
    })
}

/// What the page shows: the creators, the rooms alive, the limits, and the permissions the bot lacks.
async fn view(state: &WebState, guild: u64) -> ApiResult<Value> {
    let db = &state.app.db;
    let creators = db.voice_creators(guild).await?;
    let rooms = db.temp_rooms(guild).await?;
    let enabled = db
        .module_states(guild)
        .await?
        .into_iter()
        .find(|(module, _)| *module == Module::TempVoice)
        .is_none_or(|(_, rule)| rule.enabled);
    // Who owns what, and how many sit in each: names come from Discord (kept for ten minutes), counts from
    // the bot's own tracking of the voice events.
    let mut owners: HashMap<i64, Option<String>> = HashMap::new();
    for room in rooms.iter().take(OWNER_LOOKUPS) {
        if let Some(owner) = room.owner_id
            && !owners.contains_key(&owner)
        {
            let name = state
                .app
                .user(owner as u64)
                .await
                .map(|user| user.global_name.clone().unwrap_or_else(|| user.name.clone()));
            owners.insert(owner, name);
        }
    }
    let mut shown = Vec::with_capacity(rooms.len());
    for room in &rooms {
        let members = pinchora_commands::temp_voice_members(&state.app, room.channel_id as u64).await;
        let name = room.owner_id.and_then(|o| owners.get(&o)).and_then(|n| n.as_deref());
        shown.push(room_json(room, name, members));
    }
    let created: Vec<Value> = creators
        .iter()
        .map(|c| creator_json(c, rooms.iter().filter(|r| r.source_id == c.channel_id).count()))
        .collect();
    let missing = pinchora_commands::temp_voice_gaps(&state.app, guild).await.ok();
    let lang = state.app.guild_lang(guild).await;
    Ok(json!({
        "enabled": enabled,
        "creators": created,
        "rooms": shown,
        "missing": missing,
        "limits": {
            "max_creators": MAX_CREATORS,
            "max_rooms": MAX_ROOMS,
            "max_per_user": MAX_PER_USER,
            "max_template": MAX_TEMPLATE,
            "max_limit": MAX_LIMIT,
            "default_template": t!(lang, "voice.default-template", user = "{user}"),
        },
    }))
}

/// `GET /temp-voice`.
pub async fn overview(State(state): State<WebState>, managed: ManagedGuild) -> ApiResult<Json<Value>> {
    Ok(Json(view(&state, managed.id).await?))
}

/// What `PUT /temp-voice/creators/{channel}` may carry: any subset. A new creator starts from the defaults and
/// puts its rooms where the channel itself sits.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreatorBody {
    #[serde(default, deserialize_with = "present")]
    category_id: Option<Option<String>>,
    name_template: Option<String>,
    default_limit: Option<i64>,
    default_locked: Option<bool>,
    max_per_user: Option<i64>,
    panel: Option<bool>,
    #[serde(default, deserialize_with = "present")]
    panel_channel_id: Option<Option<String>>,
    enabled: Option<bool>,
}

fn channel_of(params: &HashMap<String, String>, lang: Lang) -> ApiResult<u64> {
    params
        .get("channel")
        .and_then(|raw| parse_id(raw))
        .ok_or_else(|| ApiError::malformed(t!(lang, "voice.web.channel-id")))
}

/// A channel id from the body, checked against the server's channels with `fits`.
fn channel_field(
    info: &GuildInfo,
    lang: Lang,
    field: &'static str,
    raw: &str,
    fits: impl Fn(&guild_info::ChannelInfo) -> Option<String>,
) -> Result<u64, FieldError> {
    let id = parse_id(raw).ok_or_else(|| FieldError::new(field, t!(lang, "voice.web.invalid-id")))?;
    let channel = info.check_channel(field, id).map_err(|error| {
        FieldError::new(
            field,
            t!(
                lang,
                "voice.web.unknown-channel",
                detail = localized_detail(lang, error.message, "voice.web.unknown-channel-detail")
            ),
        )
    })?;
    match fits(channel) {
        Some(why) => Err(FieldError::new(field, why)),
        None => Ok(id),
    }
}

fn localized_detail(lang: Lang, english: String, tr_key: &str) -> String {
    if lang == Lang::Tr { t!(lang, tr_key) } else { english }
}

/// The value of a check, or the problem noted in `errors` so every problem of a form is reported at once.
fn collect<T>(errors: &mut Vec<FieldError>, result: Result<T, FieldError>) -> Option<T> {
    match result {
        Ok(value) => Some(value),
        Err(error) => {
            errors.push(error);
            None
        }
    }
}

/// `PUT /temp-voice/creators/{channel}`: makes a voice channel a creator, or changes it.
pub async fn put_creator(
    State(state): State<WebState>,
    managed: ManagedGuild,
    Path(params): Path<HashMap<String, String>>,
    ApiJson(body): ApiJson<CreatorBody>,
) -> ApiResult<Json<Value>> {
    let guild = managed.id;
    let lang = state.app.guild_lang(guild).await;
    let channel = channel_of(&params, lang)?;
    let db = &state.app.db;
    let info = guild_info::get_with(&state, guild, |info| {
        info.channel(channel).is_some()
            && [&body.category_id, &body.panel_channel_id]
                .into_iter()
                .flatten()
                .flatten()
                .filter_map(|raw| parse_id(raw))
                .all(|id| info.channel(id).is_some())
    })
    .await?;
    let named = info.check_channel("channel_id", channel).map_err(|error| {
        FieldError::new(
            "channel_id",
            t!(
                lang,
                "voice.web.unknown-channel",
                detail = localized_detail(lang, error.message, "voice.web.unknown-channel-detail")
            ),
        )
    })?;
    if named.kind != VOICE {
        return Err(FieldError::new("channel_id", t!(lang, "voice.web.not-voice", name = &named.name)).into());
    }
    let before = db.voice_creator(guild, channel).await?;

    let mut errors = Vec::new();
    let template = match &body.name_template {
        Some(raw) => collect(
            &mut errors,
            check_template(raw).map_err(|error| {
                FieldError::new(
                    "name_template",
                    t!(
                        lang,
                        "voice.web.bad-template",
                        detail = localized_detail(lang, error.message, "voice.web.bad-template-detail")
                    ),
                )
            }),
        ),
        None => Some(
            before
                .as_ref()
                .map_or_else(|| DEFAULT_TEMPLATE.to_owned(), |b| b.name_template.clone()),
        ),
    };
    let limit = match body.default_limit {
        Some(limit) => collect(
            &mut errors,
            check_limit(limit).map_err(|error| {
                FieldError::new(
                    "default_limit",
                    t!(
                        lang,
                        "voice.web.bad-limit",
                        detail = localized_detail(lang, error.message, "voice.web.bad-limit-detail")
                    ),
                )
            }),
        ),
        None => Some(before.as_ref().map_or(0, |b| b.default_limit)),
    };
    let per_user = match body.max_per_user {
        Some(count) => collect(
            &mut errors,
            check_per_user(count).map_err(|error| {
                FieldError::new(
                    "max_per_user",
                    t!(
                        lang,
                        "voice.web.bad-per-user",
                        detail = localized_detail(lang, error.message, "voice.web.bad-per-user-detail")
                    ),
                )
            }),
        ),
        None => Some(before.as_ref().map_or(1, |b| b.max_per_user)),
    };
    // The category: named, cleared (`null`: rooms at the top), or as it was (else the creator's own).
    let category = match &body.category_id {
        Some(Some(raw)) => collect(
            &mut errors,
            channel_field(&info, lang, "category_id", raw, |c| {
                (c.kind != CATEGORY).then(|| t!(lang, "voice.web.not-category", name = &c.name))
            }),
        )
        .map(|id| Some(id as i64)),
        Some(None) => Some(None),
        None => Some(match &before {
            Some(b) => b.category_id,
            None => named.parent_id.as_deref().and_then(parse_id).map(|id| id as i64),
        }),
    };
    let thread_in = match &body.panel_channel_id {
        Some(Some(raw)) => collect(
            &mut errors,
            channel_field(&info, lang, "panel_channel_id", raw, |c| {
                (!c.takes_posts()).then(|| t!(lang, "voice.web.not-text", name = &c.name))
            }),
        )
        .map(|id| Some(id as i64)),
        Some(None) => Some(None),
        None => Some(before.as_ref().and_then(|b| b.panel_channel_id)),
    };
    let (Some(template), Some(limit), Some(per_user), Some(category), Some(thread_in)) =
        (template, limit, per_user, category, thread_in)
    else {
        return Err(ApiError::invalid(errors));
    };
    let creator = VoiceCreator {
        guild_id: guild as i64,
        channel_id: channel as i64,
        category_id: category,
        name_template: template,
        default_limit: limit,
        default_locked: body
            .default_locked
            .unwrap_or_else(|| before.as_ref().is_some_and(|b| b.default_locked)),
        max_per_user: per_user,
        panel: body.panel.unwrap_or_else(|| before.as_ref().is_none_or(|b| b.panel)),
        panel_channel_id: thread_in,
        enabled: body
            .enabled
            .unwrap_or_else(|| before.as_ref().is_none_or(|b| b.enabled)),
    };
    if before.as_ref() == Some(&creator) {
        return Ok(Json(view(&state, guild).await?));
    }
    match db.save_voice_creator(&creator).await? {
        CreatorSave::Saved(_) => {}
        CreatorSave::Full => {
            return Err(ApiError::limit(t!(lang, "voice.web.creator-cap", count = MAX_CREATORS)));
        }
        CreatorSave::Taken => {
            return Err(FieldError::new("channel_id", t!(lang, "voice.web.taken")).into());
        }
    }
    pinchora_commands::forget_temp_voice(guild).await;
    let target = format!("channel:{channel}");
    let (old, new) = changes(before.as_ref(), &creator);
    audit::finish(
        &state,
        &managed.current.session,
        Change {
            guild,
            action: "voice_creator.put",
            target: Some(&target),
            before: old,
            after: new,
        },
        Some(Invalidation::Guild(guild)),
    )
    .await;
    Ok(Json(view(&state, guild).await?))
}

/// The fields of a creator that changed, before and after (all of them for a new creator).
fn changes(before: Option<&VoiceCreator>, after: &VoiceCreator) -> (Value, Value) {
    let now = audit_json(after);
    let Some(before) = before else {
        return (Value::Null, now);
    };
    let was = audit_json(before);
    let (mut old, mut new) = (serde_json::Map::new(), serde_json::Map::new());
    for key in [
        "category_id",
        "name_template",
        "default_limit",
        "default_locked",
        "max_per_user",
        "panel",
        "panel_channel_id",
        "enabled",
    ] {
        if was[key] != now[key] {
            old.insert(key.to_owned(), was[key].clone());
            new.insert(key.to_owned(), now[key].clone());
        }
    }
    (Value::Object(old), Value::Object(new))
}

/// `DELETE /temp-voice/creators/{channel}`: the channel stops making rooms; the rooms it made stay until they
/// empty.
pub async fn delete_creator(
    State(state): State<WebState>,
    managed: ManagedGuild,
    Path(params): Path<HashMap<String, String>>,
) -> ApiResult<Json<Value>> {
    let guild = managed.id;
    let channel = channel_of(&params, state.app.guild_lang(guild).await)?;
    let db = &state.app.db;
    if let Some(was) = db.voice_creator(guild, channel).await?
        && db.remove_voice_creator(guild, channel).await?
    {
        pinchora_commands::forget_temp_voice(guild).await;
        let target = format!("channel:{channel}");
        audit::finish(
            &state,
            &managed.current.session,
            Change {
                guild,
                action: "voice_creator.delete",
                target: Some(&target),
                before: audit_json(&was),
                after: Value::Null,
            },
            Some(Invalidation::Guild(guild)),
        )
        .await;
    }
    Ok(Json(view(&state, guild).await?))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn creator() -> VoiceCreator {
        VoiceCreator {
            guild_id: 1,
            channel_id: 2,
            category_id: Some(3),
            name_template: "{user}'s room".to_owned(),
            default_limit: 0,
            default_locked: false,
            max_per_user: 1,
            panel: true,
            panel_channel_id: None,
            enabled: true,
        }
    }

    #[test]
    fn the_audit_log_keeps_only_what_changed() {
        let before = creator();
        let mut after = creator();
        after.default_limit = 4;
        after.panel_channel_id = Some(9);
        let (old, new) = changes(Some(&before), &after);
        assert_eq!(old, json!({ "default_limit": 0, "panel_channel_id": null }));
        assert_eq!(new, json!({ "default_limit": 4, "panel_channel_id": "9" }));
        let (old, new) = changes(None, &after);
        assert!(old.is_null());
        assert_eq!(new["name_template"], "{user}'s room");
        assert!(new.get("rooms").is_none());
    }

    #[test]
    fn unknown_fields_are_refused() {
        assert!(serde_json::from_value::<CreatorBody>(json!({ "colour": 1 })).is_err());
        let body: CreatorBody = serde_json::from_value(json!({ "category_id": null })).unwrap();
        assert_eq!(body.category_id, Some(None));
        let body: CreatorBody = serde_json::from_value(json!({})).unwrap();
        assert_eq!(body.category_id, None);
    }
}
