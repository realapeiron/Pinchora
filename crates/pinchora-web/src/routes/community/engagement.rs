//! Managed guild API for QOTD, counting, applications and modmail.
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
    routing::{get, put},
};
use pinchora_cache::Invalidation;
use pinchora_core::admin::settings::FieldError;
use pinchora_core::t;
use serde::Deserialize;
use serde_json::{Value, json};
use twilight_model::channel::message::{
    Component,
    component::{ActionRow, Button, ButtonStyle},
};

pub fn router() -> Router<WebState> {
    Router::new()
        .route("/", get(list))
        .route("/qotd", put(save_qotd))
        .route("/qotd/{id}/decision", put(decide_qotd))
        .route("/counting", put(save_counting))
        .route("/counting/reset", axum::routing::post(reset_counting))
        .route("/forms", axum::routing::post(save_form))
        .route("/forms/{id}", put(save_form_id))
        .route("/forms/{id}/post", axum::routing::post(post_form))
        .route(
            "/forms/{id}/submissions/{submission}",
            get(submissions).put(decide_submission),
        )
        .route("/modmail", put(save_modmail))
        .route("/modmail/{id}/close", axum::routing::post(close_mail))
}
fn bad(field: &'static str, message: &str) -> ApiError {
    ApiError::invalid(vec![FieldError::new(field, message)])
}
fn id(raw: &str, field: &'static str) -> ApiResult<i64> {
    raw.parse::<i64>()
        .ok()
        .filter(|n| *n > 0)
        .ok_or_else(|| bad(field, "Choose a valid Discord ID."))
}
async fn audited(state: &WebState, guild: &ManagedGuild, action: &str, target: Option<&str>, after: Value) {
    audit::finish(
        state,
        &guild.current.session,
        Change {
            guild: guild.id,
            action,
            target,
            before: Value::Null,
            after,
        },
        Some(Invalidation::Guild(guild.id)),
    )
    .await;
}
async fn list(State(state): State<WebState>, guild: ManagedGuild) -> ApiResult<Json<Value>> {
    let g = guild.id as i64;
    type QotdSettingsRow = (Option<i64>, Option<i64>, i16, String, bool);
    let qotd: Option<QotdSettingsRow> = sqlx::query_as(
        "SELECT channel_id,ping_role_id,hour,timezone,thread_per_question FROM qotd_settings WHERE guild_id=$1",
    )
    .bind(g)
    .fetch_optional(state.app.db.pool())
    .await?;
    let counting = state.app.db.counting_for_guild(guild.id).await?;
    let forms = state.app.db.forms(guild.id).await?;
    let mail = state.app.db.modmail_settings(guild.id).await?;
    let questions = state.app.db.qotd_queue(guild.id).await?;
    let submissions:Vec<(i64,i64,i64,Value,String,chrono::DateTime<chrono::Utc>)>=sqlx::query_as(
        "SELECT s.id,s.form_id,s.user_id,s.answers,s.status,s.created_at FROM form_submissions s JOIN forms f ON f.id=s.form_id WHERE f.guild_id=$1 ORDER BY s.created_at DESC LIMIT 100")
        .bind(g).fetch_all(state.app.db.pool()).await?;
    let threads:Vec<(i64,i64,i64,String,chrono::DateTime<chrono::Utc>)>=sqlx::query_as(
        "SELECT id,user_id,thread_id,status,opened_at FROM modmail_threads WHERE guild_id=$1 AND status='open' ORDER BY opened_at DESC LIMIT 100")
        .bind(g).fetch_all(state.app.db.pool()).await?;
    Ok(Json(json!({
        "qotd":qotd.map(|(channel,role,hour,timezone,thread)|json!({
            "channel_id":channel.map(|n|n.to_string()),"ping_role_id":role.map(|n|n.to_string()),
            "hour":hour,"timezone":timezone,"thread":thread})),
        "questions":questions.iter().map(|q|json!({"id":q.id.to_string(),"text":q.text,
            "text_tr":q.text_tr,"approved":q.approved,"asked_at":q.asked_at})).collect::<Vec<_>>(),
        "counting":counting.map(|c|json!({"channel_id":c.channel_id.to_string(),"current":c.current,
            "record":c.record,"reset_on_fail":c.reset_on_fail,"allow_math":c.allow_math,
            "fail_role_id":c.fail_role_id.map(|n|n.to_string())})),
        "forms":forms.iter().map(|f|json!({"id":f.id.to_string(),"name":f.name,"questions":f.questions,
            "submit_channel_id":f.submit_channel_id.map(|n|n.to_string()),"review_roles":f.review_roles.iter().map(ToString::to_string).collect::<Vec<_>>(),
            "accept_role_id":f.accept_role_id.map(|n|n.to_string()),"dm_result":f.dm_result,
            "open":f.open,"message_id":f.message_id.map(|n|n.to_string())})).collect::<Vec<_>>(),
        "submissions":submissions.iter().map(|(id,form,user,answers,status,created)|json!({
            "id":id.to_string(),"form_id":form.to_string(),"user_id":user.to_string(),
            "answers":answers,"status":status,"created_at":created})).collect::<Vec<_>>(),
        "modmail":mail.map(|s|json!({"enabled":s.enabled,"forum_channel_id":s.forum_channel_id.map(|n|n.to_string()),
            "staff_roles":s.staff_roles.iter().map(ToString::to_string).collect::<Vec<_>>(),
            "greeting":s.greeting,"close_after_hours":s.close_after_hours,
            "anonymous_staff":s.anonymous_staff,"log_channel_id":s.log_channel_id.map(|n|n.to_string())})),
        "threads":threads.iter().map(|(id,user,thread,status,opened)|json!({"id":id.to_string(),
            "user_id":user.to_string(),"thread_id":thread.to_string(),"status":status,"opened_at":opened})).collect::<Vec<_>>(),
        "message_content":state.app.message_content
    })))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct QotdBody {
    channel_id: Option<String>,
    ping_role_id: Option<String>,
    hour: i16,
    timezone: String,
    thread: bool,
}
async fn save_qotd(
    State(state): State<WebState>,
    guild: ManagedGuild,
    ApiJson(body): ApiJson<QotdBody>,
) -> ApiResult<Json<Value>> {
    if !(0..=23).contains(&body.hour) || body.timezone.parse::<chrono_tz::Tz>().is_err() {
        return Err(bad("timezone", "Choose an IANA time zone and an hour from 0 to 23."));
    }
    let channel = body.channel_id.as_deref().map(|x| id(x, "channel_id")).transpose()?;
    let role = body
        .ping_role_id
        .as_deref()
        .map(|x| id(x, "ping_role_id"))
        .transpose()?;
    if let Some(channel) = channel {
        guild_info::get(&state, guild.id, false)
            .await?
            .check_post_channel("channel_id", channel as u64)?;
    }
    if let Some(role) = role
        && guild_info::get(&state, guild.id, false)
            .await?
            .role(role as u64)
            .is_none()
    {
        return Err(bad("ping_role_id", "Choose a role in this server."));
    }
    sqlx::query("INSERT INTO qotd_settings(guild_id,channel_id,ping_role_id,hour,timezone,thread_per_question) VALUES($1,$2,$3,$4,$5,$6) ON CONFLICT(guild_id) DO UPDATE SET channel_id=$2,ping_role_id=$3,hour=$4,timezone=$5,thread_per_question=$6")
        .bind(guild.id as i64).bind(channel).bind(role).bind(body.hour).bind(&body.timezone).bind(body.thread)
        .execute(state.app.db.pool()).await?;
    state.app.db.seed_qotd(guild.id).await?;
    audited(
        &state,
        &guild,
        "community.qotd.settings",
        None,
        json!({"channel_id":body.channel_id,"hour":body.hour,"timezone":body.timezone}),
    )
    .await;
    Ok(Json(json!({"ok":true})))
}
#[derive(Deserialize)]
struct Decision {
    approve: bool,
}
async fn decide_qotd(
    State(state): State<WebState>,
    guild: ManagedGuild,
    Path(raw): Path<String>,
    ApiJson(body): ApiJson<Decision>,
) -> ApiResult<Json<Value>> {
    let question = id(&raw, "id")?;
    if !state.app.db.decide_qotd(guild.id, question, body.approve).await? {
        return Err(ApiError::not_found());
    }
    audited(
        &state,
        &guild,
        "community.qotd.decision",
        Some(&raw),
        json!({"approved":body.approve}),
    )
    .await;
    Ok(Json(json!({"ok":true})))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CountingBody {
    channel_id: String,
    reset_on_fail: bool,
    allow_math: bool,
    fail_role_id: Option<String>,
}
async fn save_counting(
    State(state): State<WebState>,
    guild: ManagedGuild,
    ApiJson(body): ApiJson<CountingBody>,
) -> ApiResult<Json<Value>> {
    let channel = id(&body.channel_id, "channel_id")?;
    let info = guild_info::get(&state, guild.id, false).await?;
    info.check_post_channel("channel_id", channel as u64)?;
    let role = body
        .fail_role_id
        .as_deref()
        .map(|x| id(x, "fail_role_id"))
        .transpose()?;
    if let Some(role) = role {
        let Some(found) = info.role(role as u64) else {
            return Err(bad("fail_role_id", "Choose a role in this server."));
        };
        if !found.action_ok || !found.assignable {
            return Err(bad("fail_role_id", "That role cannot be granted safely."));
        }
    }
    state
        .app
        .db
        .save_counting(
            guild.id,
            channel as u64,
            body.reset_on_fail,
            body.allow_math,
            role.map(|n| n as u64),
        )
        .await?;
    audited(
        &state,
        &guild,
        "community.counting.settings",
        None,
        json!({"channel_id":body.channel_id,
        "reset_on_fail":body.reset_on_fail,"allow_math":body.allow_math}),
    )
    .await;
    Ok(Json(json!({"ok":true})))
}
async fn reset_counting(State(state): State<WebState>, guild: ManagedGuild) -> ApiResult<Json<Value>> {
    if !state.app.db.reset_counting(guild.id).await? {
        return Err(ApiError::not_found());
    }
    audited(&state, &guild, "community.counting.reset", None, json!({"current":0})).await;
    Ok(Json(json!({"ok":true})))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FormBody {
    name: String,
    questions: Vec<String>,
    submit_channel_id: String,
    review_roles: Vec<String>,
    accept_role_id: Option<String>,
    dm_result: bool,
    open: bool,
}
async fn save_form(
    State(state): State<WebState>,
    guild: ManagedGuild,
    ApiJson(body): ApiJson<FormBody>,
) -> ApiResult<Json<Value>> {
    form_write(state, guild, None, body).await
}
async fn save_form_id(
    State(state): State<WebState>,
    guild: ManagedGuild,
    Path(raw): Path<String>,
    ApiJson(body): ApiJson<FormBody>,
) -> ApiResult<Json<Value>> {
    form_write(state, guild, Some(id(&raw, "id")?), body).await
}
async fn form_write(
    state: WebState,
    guild: ManagedGuild,
    existing: Option<i64>,
    body: FormBody,
) -> ApiResult<Json<Value>> {
    let lang = guild.panel_lang;
    if !pinchora_core::community::community::valid_form(&body.name, &json!(body.questions)) {
        return Err(bad(
            "questions",
            "Use a name of at most 45 characters and 1 to 5 questions of at most 45 characters.",
        ));
    }
    let channel = id(&body.submit_channel_id, "submit_channel_id")?;
    let info = guild_info::get(&state, guild.id, false).await?;
    info.check_post_channel("submit_channel_id", channel as u64)?;
    let role = body
        .accept_role_id
        .as_deref()
        .map(|r| id(r, "accept_role_id"))
        .transpose()?;
    if let Some(role) = role {
        let Some(found) = info.role(role as u64) else {
            return Err(bad("accept_role_id", "Choose a role in this server."));
        };
        if !found.action_ok || !found.assignable {
            return Err(bad("accept_role_id", "That role cannot be granted safely."));
        }
    }
    if body.review_roles.len() > 10 {
        return Err(bad("review_roles", "Choose at most 10 review roles."));
    }
    let mut review = Vec::new();
    for raw in &body.review_roles {
        let role = id(raw, "review_roles")?;
        if info.role(role as u64).is_none() {
            return Err(bad("review_roles", "Choose roles in this server."));
        }
        review.push(role);
    }
    let form_id = state
        .app
        .db
        .save_form(&pinchora_db::FormRow {
            id: existing.unwrap_or(0),
            guild_id: guild.id as i64,
            name: body.name.clone(),
            questions: json!(body.questions),
            submit_channel_id: Some(channel),
            review_roles: review,
            accept_role_id: role,
            dm_result: body.dm_result,
            open: body.open,
            message_id: None,
        })
        .await?
        .ok_or_else(|| {
            if existing.is_some() {
                ApiError::not_found()
            } else {
                ApiError::limit(t!(
                    lang,
                    "community_engagement.web.errors.this-server-already-has-10-forms"
                ))
            }
        })?;
    let target = form_id.to_string();
    audited(
        &state,
        &guild,
        "community.forms.save",
        Some(&target),
        json!({"name":body.name,"open":body.open}),
    )
    .await;
    Ok(Json(json!({"id":target})))
}
#[derive(Deserialize)]
struct SubmissionPath {
    id: String,
    submission: String,
}
#[derive(Deserialize)]
struct FormPostBody {
    channel_id: String,
}
async fn post_form(
    State(state): State<WebState>,
    guild: ManagedGuild,
    Path(raw): Path<String>,
    ApiJson(body): ApiJson<FormPostBody>,
) -> ApiResult<Json<Value>> {
    let form_id = id(&raw, "id")?;
    let channel = id(&body.channel_id, "channel_id")?;
    guild_info::get(&state, guild.id, false)
        .await?
        .check_post_channel("channel_id", channel as u64)?;
    let Some(form) = state.app.db.form(guild.id, form_id).await?.filter(|f| f.open) else {
        return Err(ApiError::not_found());
    };
    let row = Component::ActionRow(ActionRow {
        id: None,
        components: vec![Component::Button(Button {
            custom_id: Some(format!("fm:o:{}", form.id)),
            disabled: false,
            emoji: None,
            id: None,
            label: Some(form.name.clone()),
            sku_id: None,
            style: ButtonStyle::Primary,
            url: None,
        })],
    });
    let response = state
        .app
        .http
        .create_message(twilight_model::id::Id::new(channel as u64))
        .content(&form.name)
        .components(&[row])
        .await
        .map_err(|e| ApiError::internal(e.into()))?;
    let message = response.model().await.map_err(|e| ApiError::internal(e.into()))?;
    sqlx::query("UPDATE forms SET message_id=$2 WHERE id=$1 AND guild_id=$3")
        .bind(form_id)
        .bind(message.id.get() as i64)
        .bind(guild.id as i64)
        .execute(state.app.db.pool())
        .await?;
    audited(
        &state,
        &guild,
        "community.forms.post",
        Some(&raw),
        json!({"channel_id":body.channel_id}),
    )
    .await;
    Ok(Json(json!({"message_id":message.id.get().to_string()})))
}

async fn submissions(
    State(state): State<WebState>,
    guild: ManagedGuild,
    Path(path): Path<SubmissionPath>,
) -> ApiResult<Json<Value>> {
    let form = id(&path.id, "id")?;
    let sub = id(&path.submission, "submission")?;
    let row:Option<(Value,String)>=sqlx::query_as("SELECT s.answers,s.status FROM form_submissions s JOIN forms f ON f.id=s.form_id WHERE f.guild_id=$1 AND f.id=$2 AND s.id=$3")
        .bind(guild.id as i64).bind(form).bind(sub).fetch_optional(state.app.db.pool()).await?;
    let Some((answers, status)) = row else {
        return Err(ApiError::not_found());
    };
    Ok(Json(json!({"answers":answers,"status":status})))
}
#[derive(Deserialize)]
struct ReviewBody {
    accept: bool,
    reason: Option<String>,
}
async fn decide_submission(
    State(state): State<WebState>,
    guild: ManagedGuild,
    Path(path): Path<SubmissionPath>,
    ApiJson(body): ApiJson<ReviewBody>,
) -> ApiResult<Json<Value>> {
    let form = id(&path.id, "id")?;
    let submission = id(&path.submission, "submission")?;
    let belongs: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM form_submissions WHERE id=$1 AND form_id=$2)")
        .bind(submission)
        .bind(form)
        .fetch_one(state.app.db.pool())
        .await?;
    if !belongs || state.app.db.form(guild.id, form).await?.is_none() {
        return Err(ApiError::not_found());
    }
    let Some((user, _, role, dm)) = state
        .app
        .db
        .decide_form(guild.id, submission, guild.current.session.user_id, body.accept)
        .await?
    else {
        return Err(ApiError::not_found());
    };
    if body.accept
        && let Some(role) = role
        && let Err(error) =
            pinchora_commands::grant_community_role(&state.app, guild.id, user as u64, role as u64).await
    {
        tracing::warn!(%error,guild=guild.id,"form acceptance role could not be granted");
    }
    if dm && !state.app.db.dm_opt_out(user as u64).await? {
        let lang = state
            .app
            .user_lang_choice(user as u64)
            .await
            .unwrap_or(state.app.guild_lang(guild.id).await);
        let message = if body.accept {
            t!(lang, "community-pack.form-accepted")
        } else {
            t!(lang, "community-pack.form-denied")
        };
        if let Ok(response) = state
            .app
            .http
            .create_private_channel(twilight_model::id::Id::new(user as u64))
            .await
            && let Ok(channel) = response.model().await
        {
            let _ = state.app.http.create_message(channel.id).content(&message).await;
        }
    }
    sqlx::query("UPDATE form_submissions SET reason=$2 WHERE id=$1")
        .bind(submission)
        .bind(body.reason)
        .execute(state.app.db.pool())
        .await?;
    audited(
        &state,
        &guild,
        "community.forms.review",
        Some(&path.submission),
        json!({"accepted":body.accept}),
    )
    .await;
    Ok(Json(json!({"ok":true})))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct MailBody {
    enabled: bool,
    forum_channel_id: Option<String>,
    staff_roles: Vec<String>,
    greeting: String,
    close_after_hours: i32,
    anonymous_staff: bool,
    log_channel_id: Option<String>,
}
async fn save_modmail(
    State(state): State<WebState>,
    guild: ManagedGuild,
    ApiJson(body): ApiJson<MailBody>,
) -> ApiResult<Json<Value>> {
    if body.greeting.chars().count() > 500
        || !(1..=8760).contains(&body.close_after_hours)
        || body.staff_roles.len() > 10
    {
        return Err(bad("greeting", "Check the greeting, close interval and staff roles."));
    }
    let forum = body
        .forum_channel_id
        .as_deref()
        .map(|x| id(x, "forum_channel_id"))
        .transpose()?;
    let log = body
        .log_channel_id
        .as_deref()
        .map(|x| id(x, "log_channel_id"))
        .transpose()?;
    let info = guild_info::get(&state, guild.id, false).await?;
    if body.enabled && forum.is_none() {
        return Err(bad("forum_channel_id", "Choose a staff forum."));
    }
    if let Some(forum) = forum
        && !info.channel(forum as u64).is_some_and(|c| c.kind == 15)
    {
        return Err(bad("forum_channel_id", "Choose a forum in this server."));
    }
    if let Some(log) = log {
        info.check_post_channel("log_channel_id", log as u64)?
    }
    let mut roles = Vec::new();
    for raw in &body.staff_roles {
        let role = id(raw, "staff_roles")?;
        if info.role(role as u64).is_none() {
            return Err(bad("staff_roles", "Choose roles in this server."));
        }
        roles.push(role);
    }
    sqlx::query("INSERT INTO modmail_settings(guild_id,enabled,forum_channel_id,staff_roles,greeting,close_after_hours,anonymous_staff,log_channel_id) VALUES($1,$2,$3,$4,$5,$6,$7,$8) ON CONFLICT(guild_id) DO UPDATE SET enabled=$2,forum_channel_id=$3,staff_roles=$4,greeting=$5,close_after_hours=$6,anonymous_staff=$7,log_channel_id=$8")
        .bind(guild.id as i64).bind(body.enabled).bind(forum).bind(roles).bind(&body.greeting)
        .bind(body.close_after_hours).bind(body.anonymous_staff).bind(log).execute(state.app.db.pool()).await?;
    audited(
        &state,
        &guild,
        "community.modmail.settings",
        None,
        json!({"enabled":body.enabled,
        "forum_channel_id":body.forum_channel_id,"anonymous_staff":body.anonymous_staff}),
    )
    .await;
    Ok(Json(json!({"ok":true})))
}
async fn close_mail(
    State(state): State<WebState>,
    guild: ManagedGuild,
    Path(raw): Path<String>,
) -> ApiResult<Json<Value>> {
    let thread = id(&raw, "id")?;
    let row: Option<i64> =
        sqlx::query_scalar("SELECT thread_id FROM modmail_threads WHERE guild_id=$1 AND id=$2 AND status='open'")
            .bind(guild.id as i64)
            .bind(thread)
            .fetch_optional(state.app.db.pool())
            .await?;
    let Some(channel) = row else {
        return Err(ApiError::not_found());
    };
    pinchora_commands::close_community_mail(
        &state.app,
        guild.id,
        channel as u64,
        guild.current.session.user_id,
        None,
    )
    .await?;
    audited(
        &state,
        &guild,
        "community.modmail.close",
        Some(&raw),
        json!({"status":"closed"}),
    )
    .await;
    Ok(Json(json!({"ok":true})))
}
