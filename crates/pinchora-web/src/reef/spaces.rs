//! In-world adapters. Inventory, friendships, notes, likes and events keep their existing owners.
use super::RoomKey;
use crate::{
    audit, discord,
    error::{ApiError, ApiJson, ApiResult},
    extract::CurrentSession,
    state::WebState,
};
use axum::{
    Json, Router,
    extract::{DefaultBodyLimit, Path, Query, State},
    http::StatusCode,
    routing::{get, post},
};
use serde::Deserialize;
use serde_json::{Value, json};
use twilight_model::id::Id;

pub(super) async fn limit(s: &WebState, user: u64, action: &str, count: i64) -> ApiResult<()> {
    crate::ratelimit::check_strict(
        &s.app.cache,
        &format!("reef:{action}"),
        &user.to_string(),
        count,
        std::time::Duration::from_secs(60),
    )
    .await
}

fn denied() -> ApiError {
    ApiError::new(StatusCode::FORBIDDEN, "access_denied", "access_denied")
}
fn invalid() -> ApiError {
    ApiError::new(
        StatusCode::UNPROCESSABLE_ENTITY,
        "invalid_safety_request",
        "invalid_safety_request",
    )
}
fn id(raw: &str) -> ApiResult<u64> {
    raw.parse::<u64>()
        .ok()
        .filter(|n| *n > 0 && *n <= i64::MAX as u64 && n.to_string() == raw)
        .ok_or_else(invalid)
}
async fn record(s: &WebState, c: &CurrentSession, action: &str, target: &str, after: Value) {
    audit::finish(
        s,
        &c.session,
        audit::Change {
            guild: 0,
            action,
            target: Some(target),
            before: Value::Null,
            after,
        },
        None,
    )
    .await;
}
pub fn router() -> Router<WebState> {
    Router::new()
        .route("/preferences", get(preferences).put(save_preferences))
        .route("/social", get(social))
        .route("/doorbells", get(doorbells))
        .route("/houses/{owner}/doorbells", post(ring))
        .route("/doorbells/{id}/decision", post(decide))
        .route("/houses/{owner}/guestbook", get(notes).post(sign))
        .route("/houses/{owner}/guestbook/{note}/report", post(report_note))
        .route("/houses/{owner}/appreciate", post(appreciate))
        .route("/guilds/{guild}/plazas", get(plazas))
        .route("/guilds/{guild}/gatherings", get(gatherings).post(schedule))
        .route("/guilds/{guild}/gatherings/{event}/rsvp", post(rsvp))
        .layer(DefaultBodyLimit::max(4096))
}
pub async fn membership(state: &WebState, guild: u64, user: u64) -> anyhow::Result<Vec<u64>> {
    anyhow::ensure!(state.app.db.is_guild(guild).await?, "access denied");
    anyhow::ensure!(
        state
            .app
            .db
            .guild_modules(guild)
            .await?
            .rule(pinchora_core::modules::Module::Plaza)
            .enabled,
        "access denied"
    );
    let member = state
        .app
        .http
        .guild_member(Id::new(guild), Id::new(user))
        .await?
        .model()
        .await?;
    anyhow::ensure!(discord::bot_present(state, guild).await, "access denied");
    Ok(member.roles.iter().map(|r| r.get()).collect())
}
pub async fn visit_consent(state: &WebState, owner: u64, user: u64) -> anyhow::Result<bool> {
    state.app.db.reef_house_access(owner, user).await
}
pub(super) async fn manager(
    state: &WebState,
    current: &mut CurrentSession,
    guild: u64,
    area_path: &str,
) -> ApiResult<()> {
    membership(state, guild, current.session.user_id)
        .await
        .map_err(|_| denied())?;
    let guilds = discord::user_guilds(state, current, true).await?;
    let full = guilds.iter().any(|g| g.id() == Some(guild) && g.can_manage());
    if !full
        && !crate::dashboard_access::permitted(
            area_path,
            false,
            &crate::dashboard_access::areas(state, guild, current.session.user_id).await?,
        )
    {
        return Err(denied());
    }
    Ok(())
}
async fn house(state: &WebState, user: u64, owner: u64) -> ApiResult<()> {
    if state.app.db.is_guild(owner).await? || !state.app.db.reef_house_access(owner, user).await? {
        return Err(denied());
    }
    Ok(())
}
async fn preferences(
    State(s): State<WebState>,
    c: CurrentSession,
) -> ApiResult<Json<pinchora_db::ReefSpacePreferences>> {
    Ok(Json(s.app.db.reef_space_preferences(c.session.user_id).await?))
}
async fn save_preferences(
    State(s): State<WebState>,
    c: CurrentSession,
    ApiJson(p): ApiJson<pinchora_db::ReefSpacePreferences>,
) -> ApiResult<Json<Value>> {
    if !["open", "friends", "doorbell", "closed"].contains(&p.visits.as_str()) {
        return Err(invalid());
    }
    s.app.db.reef_save_space_preferences(c.session.user_id, &p).await?;
    record(&s, &c, "reef.preferences", "self", json!(p)).await;
    s.reef.policy_changed(&s, c.session.user_id).await;
    s.reef.recheck(&s).await;
    Ok(Json(json!(p)))
}
async fn social(State(s): State<WebState>, c: CurrentSession) -> ApiResult<Json<Value>> {
    let user = c.session.user_id;
    let (friends, _) = s.app.db.friends(user, 0, 100).await?;
    let mut people: Vec<_> = friends.into_iter().map(|f| (f.friend_id, f.name, false)).collect();
    if let Some(p) = s.app.db.partner(user).await? {
        people.retain(|(id, _, _)| *id != p.partner_id);
        people.insert(
            0,
            (
                p.partner_id,
                s.app.db.remembered_name(p.partner_id).await?.unwrap_or_default(),
                true,
            ),
        );
    }
    let mut result = vec![];
    for (other, name, partner) in people {
        if s.app.db.socially_blocked(user, other).await? || s.app.db.privacy(other).await?.hide_house {
            continue;
        }
        let p = s.app.db.reef_space_preferences(other).await?;
        // Presence reveals only online status, never another host's private location.
        let online = if p.presence {
            s.app
                .cache
                .reef_get(&super::routing::controller_key(other))
                .await?
                .is_some()
        } else {
            false
        };
        result.push(json!({"id":other.to_string(),"name":pinchora_game::chat::display_name(&name),"partner":partner,"online":online,"visitable":s.app.db.reef_house_access(other,user).await?,"doorbell":p.doorbell&&p.visits!="closed"}));
    }
    Ok(Json(json!({"people":result})))
}
async fn doorbells(State(s): State<WebState>, c: CurrentSession) -> ApiResult<Json<Value>> {
    Ok(Json(json!({"items":s.app.db.reef_doorbells(c.session.user_id).await?})))
}
async fn ring(State(s): State<WebState>, c: CurrentSession, Path(owner): Path<String>) -> ApiResult<Json<Value>> {
    limit(&s, c.session.user_id, "doorbell", 12).await?;
    let owner = id(&owner)?;
    if s.app.db.is_guild(owner).await? || s.app.db.privacy(owner).await?.hide_house {
        return Err(denied());
    }
    let request = s.app.db.reef_ring(owner, c.session.user_id).await?.ok_or_else(denied)?;
    record(&s, &c, "reef.doorbell.request", &request, json!({"requested":true})).await;
    Ok(Json(json!({"id":request,"expires_in":120})))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Decision {
    accept: bool,
}
async fn decide(
    State(s): State<WebState>,
    c: CurrentSession,
    Path(raw): Path<String>,
    ApiJson(body): ApiJson<Decision>,
) -> ApiResult<Json<Value>> {
    if !s
        .app
        .db
        .reef_decide_doorbell(c.session.user_id, id(&raw)? as i64, body.accept)
        .await?
    {
        return Err(denied());
    }
    record(&s, &c, "reef.doorbell.decision", &raw, json!({"accepted":body.accept})).await;
    Ok(Json(json!({"accepted":body.accept})))
}
async fn notes(State(s): State<WebState>, c: CurrentSession, Path(owner): Path<String>) -> ApiResult<Json<Value>> {
    let owner = id(&owner)?;
    house(&s, c.session.user_id, owner).await?;
    let (rows, _) = s.app.db.guestbook(owner, 0, 50).await?;
    let mut notes = vec![];
    for n in rows {
        if !s.app.db.socially_blocked(c.session.user_id, n.author_id as u64).await? {
            notes.push(json!({"id":n.id.to_string(),"author_id":n.author_id.to_string(),"name":pinchora_game::chat::display_name(&n.author_name),"body":n.body,"created_at":n.created_at}));
        }
    }
    Ok(Json(json!({"notes":notes})))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Note {
    text: String,
}
async fn sign(
    State(s): State<WebState>,
    c: CurrentSession,
    Path(owner): Path<String>,
    ApiJson(n): ApiJson<Note>,
) -> ApiResult<Json<Value>> {
    limit(&s, c.session.user_id, "guestbook", 10).await?;
    let owner = id(&owner)?;
    let user = c.session.user_id;
    house(&s, user, owner).await?;
    if owner == user || !s.app.db.reef_eligible(user).await? || s.app.db.reef_sanctioned(user, owner, 0, true).await? {
        return Err(denied());
    }
    pinchora_game::chat::check(&n.text).map_err(|_| invalid())?;
    let result = s
        .app
        .db
        .sign_guestbook(
            owner,
            user,
            &pinchora_game::chat::display_name(c.session.display_name()),
            &n.text,
            chrono::Utc::now().date_naive(),
        )
        .await?;
    match result {
        pinchora_db::SignOutcome::Signed => {
            record(&s, &c, "reef.guestbook.sign", "guestbook", json!({"signed":true})).await;
            Ok(Json(json!({"signed":true})))
        }
        pinchora_db::SignOutcome::AlreadyToday => Ok(Json(json!({"signed":false}))),
        _ => Err(denied()),
    }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Report {
    reason: String,
}
async fn report_note(
    State(s): State<WebState>,
    c: CurrentSession,
    Path((owner, note)): Path<(String, String)>,
    ApiJson(body): ApiJson<Report>,
) -> ApiResult<Json<Value>> {
    limit(&s, c.session.user_id, "report", 10).await?;
    let owner = id(&owner)?;
    house(&s, c.session.user_id, owner).await?;
    if !["harassment", "personal_info", "spam", "other"].contains(&body.reason.as_str()) {
        return Err(invalid());
    }
    let evidence = s
        .app
        .db
        .reef_note_evidence(owner, c.session.user_id, id(&note)? as i64)
        .await?
        .ok_or_else(denied)?;
    let receipt = s
        .app
        .db
        .reef_report(c.session.user_id, evidence, &body.reason)
        .await?
        .ok_or_else(denied)?;
    Ok(Json(json!({"receipt":receipt})))
}
async fn appreciate(State(s): State<WebState>, c: CurrentSession, Path(owner): Path<String>) -> ApiResult<Json<Value>> {
    limit(&s, c.session.user_id, "appreciate", 20).await?;
    let owner = id(&owner)?;
    house(&s, c.session.user_id, owner).await?;
    if owner == c.session.user_id {
        return Err(denied());
    }
    let likes = s
        .app
        .db
        .like_house(owner, c.session.user_id, chrono::Utc::now().date_naive())
        .await?;
    if likes.is_some() {
        record(&s, &c, "reef.appreciate", "house", json!({"appreciated":true})).await;
    }
    Ok(Json(json!({"appreciated":likes.is_some(),"likes":likes})))
}
#[derive(Deserialize)]
struct PlazaQuery {
    instance: Option<u16>,
    #[serde(default)]
    room: i16,
}
async fn plazas(
    State(s): State<WebState>,
    c: CurrentSession,
    Path(guild): Path<String>,
    Query(q): Query<PlazaQuery>,
) -> ApiResult<Json<Value>> {
    let guild = id(&guild)?;
    membership(&s, guild, c.session.user_id).await.map_err(|_| denied())?;
    if !(0..6).contains(&q.room) || q.instance.is_some_and(|n| n >= 1024) {
        return Err(invalid());
    }
    let (_, rooms) = s.app.db.house_if_any(guild).await?.ok_or_else(denied)?;
    if !rooms.iter().any(|r| r.idx == q.room) {
        return Err(denied());
    }
    let instances: Vec<u16> = q.instance.map_or_else(|| (0..16).collect(), |i| vec![i]);
    for instance in instances {
        let key = RoomKey {
            owner: guild,
            idx: q.room,
            instance: Some(instance),
        };
        let count = s
            .app
            .cache
            .reef_get(&format!("occupancy:{}", key.name()))
            .await?
            .and_then(|n| n.parse::<u32>().ok())
            .unwrap_or(0);
        if count < 32 {
            return Ok(Json(
                json!({"room":key.name(),"instance":instance.to_string(),"players":count,"capacity":32}),
            ));
        }
    }
    Err(ApiError::new(StatusCode::CONFLICT, "room_full", "room_full"))
}
async fn event_rows(s: &WebState, guild: u64, roles: &[u64]) -> ApiResult<Vec<Value>> {
    if !s
        .app
        .db
        .guild_modules(guild)
        .await?
        .rule(pinchora_core::modules::Module::Events)
        .enabled
    {
        return Err(denied());
    }
    Ok(s.app.db.events(guild,true).await?.into_iter().filter(|e| RoomKey::parse(&e.location).is_some_and(|k|k.owner==guild&&k.instance.is_some()) && e.required_role_id.is_none_or(|r|r as u64==guild||roles.contains(&(r as u64)))).map(|e|json!({"id":e.id.to_string(),"title":e.title,"starts_at":e.starts_at,"ends_at":e.ends_at,"room":e.location,"options":e.options})).collect())
}
async fn gatherings(State(s): State<WebState>, c: CurrentSession, Path(guild): Path<String>) -> ApiResult<Json<Value>> {
    let guild = id(&guild)?;
    let roles = membership(&s, guild, c.session.user_id).await.map_err(|_| denied())?;
    Ok(Json(json!({"events":event_rows(&s,guild,&roles).await?})))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Gathering {
    title: String,
    starts_at: chrono::DateTime<chrono::Utc>,
    ends_at: chrono::DateTime<chrono::Utc>,
}
async fn schedule(
    State(s): State<WebState>,
    mut c: CurrentSession,
    Path(guild): Path<String>,
    ApiJson(body): ApiJson<Gathering>,
) -> ApiResult<Json<Value>> {
    limit(&s, c.session.user_id, "schedule", 6).await?;
    let guild = id(&guild)?;
    manager(&s, &mut c, guild, "gatherings").await?;
    if !s
        .app
        .db
        .guild_modules(guild)
        .await?
        .rule(pinchora_core::modules::Module::Events)
        .enabled
    {
        return Err(denied());
    }
    pinchora_game::chat::check(&body.title).map_err(|_| invalid())?;
    if s.app.db.house_if_any(guild).await?.is_none() {
        return Err(denied());
    }
    let draft = pinchora_db::EventDraft {
        title: body.title,
        description: String::new(),
        starts_at: body.starts_at,
        ends_at: body.ends_at,
        tz: "UTC".into(),
        location: format!("plaza:{guild}:0:0"),
        channel_id: None,
        host_id: Some(c.session.user_id as i64),
        options: {
            let mut options = pinchora_core::events::default_options();
            options[0].capacity = 32;
            options
        },
        required_role_id: None,
        reminders: vec![],
        recurrence: None,
    };
    draft.validate().map_err(|_| invalid())?;
    let row = s.app.db.create_event(guild, &draft).await?;
    audit::finish(
        &s,
        &c.session,
        audit::Change {
            guild,
            action: "reef.gathering.create",
            target: Some(&row.id.to_string()),
            before: Value::Null,
            after: json!({"id":row.id.to_string(),"room":row.location}),
        },
        None,
    )
    .await;
    Ok(Json(json!({"id":row.id.to_string(),"room":row.location})))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Rsvp {
    option: String,
}
async fn rsvp(
    State(s): State<WebState>,
    c: CurrentSession,
    Path((guild, event)): Path<(String, String)>,
    ApiJson(body): ApiJson<Rsvp>,
) -> ApiResult<Json<Value>> {
    limit(&s, c.session.user_id, "rsvp", 20).await?;
    let guild = id(&guild)?;
    let event = id(&event)? as i64;
    let roles = membership(&s, guild, c.session.user_id).await.map_err(|_| denied())?;
    if !event_rows(&s, guild, &roles)
        .await?
        .iter()
        .any(|e| e["id"].as_str().and_then(|id| id.parse::<i64>().ok()) == Some(event))
    {
        return Err(denied());
    }
    let result = s
        .app
        .db
        .rsvp_event_with_roles(guild, event, c.session.user_id, &body.option, &roles)
        .await?;
    use pinchora_db::RsvpResult::*;
    let status = match result {
        Confirmed => "confirmed",
        Waitlisted => "waitlisted",
        Removed => "removed",
        _ => return Err(invalid()),
    };
    audit::finish(
        &s,
        &c.session,
        audit::Change {
            guild,
            action: "reef.gathering.rsvp",
            target: Some(&event.to_string()),
            before: Value::Null,
            after: json!({"status":status}),
        },
        None,
    )
    .await;
    Ok(Json(json!({"status":status})))
}
