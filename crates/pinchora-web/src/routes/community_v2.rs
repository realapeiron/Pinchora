//! Community tools v2 dashboard. Every route uses ManagedGuild and strict write payloads.

use axum::{
    Json, Router,
    extract::{Path, State},
    routing::{get, put},
};
use pinchora_core::settings::FieldError;
use serde::Deserialize;
use serde_json::{Value, json};
use twilight_model::{channel::permission_overwrite::PermissionOverwriteType, guild::Permissions, id::Id};

use crate::{
    audit::{self, Change},
    error::{ApiError, ApiJson, ApiResult},
    extract::ManagedGuild,
    guild_info,
    state::WebState,
};

pub fn router() -> Router<WebState> {
    Router::new()
        .route("/", get(list))
        .route("/personal", get(personal))
        .route("/personal/reminders", put(add_reminder))
        .route("/personal/reminders/{id}", axum::routing::delete(remove_reminder))
        .route("/personal/checklist", put(add_checklist))
        .route(
            "/personal/checklist/{id}",
            put(complete_checklist).delete(remove_checklist),
        )
        .route("/personal/highlights", put(add_highlight))
        .route("/personal/highlights/{word}", axum::routing::delete(remove_highlight))
        .route("/autopost", put(save_autopost))
        .route("/autopost/{channel}", axum::routing::delete(remove_autopost))
        .route("/anniversary", put(save_anniversary))
        .route("/confessions/settings", put(save_confessions))
        .route("/confessions/{id}/review", put(review_confession))
        .route("/confessions/{id}/reveal", put(reveal_confession))
        .route("/confessions/{id}/ban", put(ban_confession))
        .route("/suggestions/{id}", put(save_suggestion))
}

async fn personal(State(state): State<WebState>, guild: ManagedGuild) -> ApiResult<Json<Value>> {
    let user = guild.current.session.user_id as i64;
    let reminders:Vec<(i64,String,i32,chrono::DateTime<chrono::Utc>)> = sqlx::query_as(
        "SELECT id,body,interval_minutes,next_at FROM personal_recurring_reminders WHERE user_id=$1 AND enabled ORDER BY next_at LIMIT 25")
        .bind(user).fetch_all(state.app.db.pool()).await?;
    let checklist: Vec<(i64, String, bool)> =
        sqlx::query_as("SELECT id,body,done FROM personal_checklist_items WHERE user_id=$1 ORDER BY done,id LIMIT 100")
            .bind(user)
            .fetch_all(state.app.db.pool())
            .await?;
    let highlights: Vec<(String,)> =
        sqlx::query_as("SELECT keyword FROM keyword_highlights WHERE guild_id=$1 AND user_id=$2 ORDER BY keyword")
            .bind(guild.id as i64)
            .bind(user)
            .fetch_all(state.app.db.pool())
            .await?;
    Ok(Json(json!({
        "reminders":reminders.iter().map(|(id,body,interval,next)|json!({"id":id.to_string(),"body":body,"interval_minutes":interval,"next_at":next})).collect::<Vec<_>>(),
        "checklist":checklist.iter().map(|(id,body,done)|json!({"id":id.to_string(),"body":body,"done":done})).collect::<Vec<_>>(),
        "highlights":highlights.into_iter().map(|r|r.0).collect::<Vec<_>>()
    })))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ReminderBody {
    text: String,
    interval_minutes: i32,
}
async fn add_reminder(
    State(state): State<WebState>,
    guild: ManagedGuild,
    ApiJson(body): ApiJson<ReminderBody>,
) -> ApiResult<Json<Value>> {
    let text = pinchora_commands::scrub_value(body.text.trim());
    if text.is_empty() || text.chars().count() > 200 {
        return Err(bad("text", "Use 1 to 200 characters."));
    }
    if !(60..=525600).contains(&body.interval_minutes) {
        return Err(bad("interval_minutes", "Choose 1 hour to 365 days."));
    }
    let user = guild.current.session.user_id as i64;
    let lang = state.app.user_lang_choice(user as u64).await.unwrap_or_default();
    let Some(row) = state
        .app
        .db
        .add_recurring_reminder(
            user as u64,
            &text,
            body.interval_minutes,
            state.app.now() + chrono::Duration::minutes(i64::from(body.interval_minutes)),
            lang.code(),
        )
        .await?
    else {
        return Err(bad("text", "You can have 25 recurring reminders."));
    };
    Ok(Json(json!({"id":row.to_string()})))
}
async fn remove_reminder(
    State(state): State<WebState>,
    guild: ManagedGuild,
    Path((_, raw)): Path<(String, String)>,
) -> ApiResult<Json<Value>> {
    let n = sqlx::query("DELETE FROM personal_recurring_reminders WHERE user_id=$1 AND id=$2")
        .bind(guild.current.session.user_id as i64)
        .bind(id(&raw, "id")?)
        .execute(state.app.db.pool())
        .await?
        .rows_affected();
    if n == 0 {
        return Err(ApiError::not_found());
    }
    Ok(Json(json!({"ok":true})))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TextBody {
    text: String,
}
async fn add_checklist(
    State(state): State<WebState>,
    guild: ManagedGuild,
    ApiJson(body): ApiJson<TextBody>,
) -> ApiResult<Json<Value>> {
    let text = pinchora_commands::scrub_value(body.text.trim());
    if text.is_empty() || text.chars().count() > 200 {
        return Err(bad("text", "Use 1 to 200 characters."));
    }
    let user = guild.current.session.user_id as i64;
    let Some(row) = state.app.db.add_checklist_item(user as u64, &text).await? else {
        return Err(bad("text", "Your checklist has 50 open items."));
    };
    Ok(Json(json!({"id":row.to_string()})))
}
async fn complete_checklist(
    State(state): State<WebState>,
    guild: ManagedGuild,
    Path((_, raw)): Path<(String, String)>,
) -> ApiResult<Json<Value>> {
    let n = sqlx::query("UPDATE personal_checklist_items SET done=TRUE WHERE user_id=$1 AND id=$2")
        .bind(guild.current.session.user_id as i64)
        .bind(id(&raw, "id")?)
        .execute(state.app.db.pool())
        .await?
        .rows_affected();
    if n == 0 {
        return Err(ApiError::not_found());
    }
    Ok(Json(json!({"ok":true})))
}
async fn remove_checklist(
    State(state): State<WebState>,
    guild: ManagedGuild,
    Path((_, raw)): Path<(String, String)>,
) -> ApiResult<Json<Value>> {
    let n = sqlx::query("DELETE FROM personal_checklist_items WHERE user_id=$1 AND id=$2")
        .bind(guild.current.session.user_id as i64)
        .bind(id(&raw, "id")?)
        .execute(state.app.db.pool())
        .await?
        .rows_affected();
    if n == 0 {
        return Err(ApiError::not_found());
    }
    Ok(Json(json!({"ok":true})))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WordBody {
    word: String,
}
async fn add_highlight(
    State(state): State<WebState>,
    guild: ManagedGuild,
    ApiJson(body): ApiJson<WordBody>,
) -> ApiResult<Json<Value>> {
    if !state.app.message_content {
        return Err(bad("word", "Message Content intent is required."));
    }
    let word = body.word.trim().to_lowercase();
    if word.chars().count() < 2 || word.chars().count() > 40 || !word.chars().all(char::is_alphanumeric) {
        return Err(bad("word", "Use one word of 2 to 40 letters or digits."));
    }
    let user = guild.current.session.user_id as i64;
    if !state.app.db.add_keyword_highlight(guild.id, user as u64, &word).await? {
        return Err(bad("word", "You can follow 10 words in this server."));
    }
    Ok(Json(json!({"word":word})))
}
async fn remove_highlight(
    State(state): State<WebState>,
    guild: ManagedGuild,
    Path((_, word)): Path<(String, String)>,
) -> ApiResult<Json<Value>> {
    let n = sqlx::query("DELETE FROM keyword_highlights WHERE guild_id=$1 AND user_id=$2 AND keyword=$3")
        .bind(guild.id as i64)
        .bind(guild.current.session.user_id as i64)
        .bind(word.to_lowercase())
        .execute(state.app.db.pool())
        .await?
        .rows_affected();
    if n == 0 {
        return Err(ApiError::not_found());
    }
    Ok(Json(json!({"ok":true})))
}

fn bad(field: &'static str, message: &str) -> ApiError {
    ApiError::invalid(vec![FieldError::new(field, message)])
}
fn id(raw: &str, field: &'static str) -> ApiResult<i64> {
    raw.parse::<i64>()
        .ok()
        .filter(|n| *n > 0)
        .ok_or_else(|| bad(field, "Choose a valid ID."))
}
async fn audited(
    state: &WebState,
    guild: &ManagedGuild,
    action: &str,
    target: Option<&str>,
    before: Value,
    after: Value,
) {
    audit::finish(
        state,
        &guild.current.session,
        Change {
            guild: guild.id,
            action,
            target,
            before,
            after,
        },
        Some(pinchora_cache::Invalidation::Guild(guild.id)),
    )
    .await;
}

async fn list(State(state): State<WebState>, guild: ManagedGuild) -> ApiResult<Json<Value>> {
    type GiveawaySummary = (
        i64,
        String,
        String,
        chrono::DateTime<chrono::Utc>,
        Option<chrono::DateTime<chrono::Utc>>,
        i32,
        i64,
    );
    type PollSummary = (
        i64,
        String,
        String,
        Vec<String>,
        bool,
        bool,
        chrono::DateTime<chrono::Utc>,
    );
    type SuggestionSummary = (i64, i32, String, String, Option<String>, Option<i64>, i64, i64);
    let g = guild.id as i64;
    let giveaways:Vec<GiveawaySummary> =
        sqlx::query_as("SELECT g.id,g.prize,g.status,g.ends_at,g.starts_at,g.winners,(SELECT count(*) FROM giveaway_entries e WHERE e.giveaway_id=g.id) FROM giveaways g WHERE g.guild_id=$1 ORDER BY g.id DESC LIMIT 50")
            .bind(g).fetch_all(state.app.db.pool()).await?;
    let polls:Vec<PollSummary> =
        sqlx::query_as("SELECT id,question,status,options,multiple,anonymous,ends_at FROM polls WHERE guild_id=$1 ORDER BY id DESC LIMIT 50")
            .bind(g).fetch_all(state.app.db.pool()).await?;
    let suggestions:Vec<SuggestionSummary> =
        sqlx::query_as("SELECT s.id,s.number,s.body,s.status,s.staff_note,s.thread_id,(SELECT count(*) FROM suggestion_votes v WHERE v.suggestion_id=s.id AND v.up),(SELECT count(*) FROM suggestion_votes v WHERE v.suggestion_id=s.id AND NOT v.up) FROM suggestions s WHERE s.guild_id=$1 ORDER BY s.id DESC LIMIT 100")
            .bind(g).fetch_all(state.app.db.pool()).await?;
    let auto:Vec<(i64,bool,bool)> = sqlx::query_as("SELECT channel_id,auto_thread,auto_crosspost FROM community_auto_channels WHERE guild_id=$1 ORDER BY channel_id")
        .bind(g).fetch_all(state.app.db.pool()).await?;
    let anniversary: Option<(Option<i64>, bool)> =
        sqlx::query_as("SELECT channel_id,enabled FROM anniversary_settings WHERE guild_id=$1")
            .bind(g)
            .fetch_optional(state.app.db.pool())
            .await?;
    let confession_settings: Option<(Option<i64>, Option<i64>, bool)> = sqlx::query_as(
        "SELECT review_channel_id,publish_channel_id,enabled FROM confession_settings WHERE guild_id=$1",
    )
    .bind(g)
    .fetch_optional(state.app.db.pool())
    .await?;
    let confessions: Vec<(i64, String, String, chrono::DateTime<chrono::Utc>, i64)> = sqlx::query_as(
        "SELECT c.id,c.body,c.status,c.created_at,(SELECT count(*) FROM confession_reports r WHERE r.confession_id=c.id) FROM confessions c WHERE c.guild_id=$1 ORDER BY c.created_at DESC LIMIT 100",
    )
    .bind(g)
    .fetch_all(state.app.db.pool())
    .await?;
    Ok(Json(json!({
        "giveaways":giveaways.iter().map(|(id,prize,status,ends,starts,winners,entries)|json!({"id":id.to_string(),"prize":prize,"status":status,"ends_at":ends,"starts_at":starts,"winners":winners,"entries":entries})).collect::<Vec<_>>(),
        "polls":polls.iter().map(|(id,question,status,options,multiple,anonymous,ends)|json!({"id":id.to_string(),"question":question,"status":status,"options":options,"multiple":multiple,"anonymous":anonymous,"ends_at":ends})).collect::<Vec<_>>(),
        "suggestions":suggestions.iter().map(|(id,number,body,status,note,thread,up,down)|json!({"id":id.to_string(),"number":number,"body":body,"status":status,"staff_note":note,"thread_id":thread.map(|n|n.to_string()),"up":up,"down":down})).collect::<Vec<_>>(),
        "autopost":auto.iter().map(|(channel,thread,cross)|json!({"channel_id":channel.to_string(),"thread":thread,"crosspost":cross})).collect::<Vec<_>>(),
        "anniversary":{"channel_id":anniversary.as_ref().and_then(|a|a.0).map(|n|n.to_string()),"enabled":anniversary.is_some_and(|a|a.1)},
        "confession_settings":{"review_channel_id":confession_settings.as_ref().and_then(|c|c.0).map(|n|n.to_string()),"publish_channel_id":confession_settings.as_ref().and_then(|c|c.1).map(|n|n.to_string()),"enabled":confession_settings.is_some_and(|c|c.2)},
        "confessions":confessions.iter().map(|(id,body,status,created,reports)|json!({"id":id.to_string(),"body":body,"status":status,"created_at":created,"reports":reports})).collect::<Vec<_>>(),
        "message_content":state.app.message_content,
    })))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AutopostBody {
    channel_id: String,
    thread: bool,
    crosspost: bool,
}
async fn save_autopost(
    State(state): State<WebState>,
    guild: ManagedGuild,
    ApiJson(body): ApiJson<AutopostBody>,
) -> ApiResult<Json<Value>> {
    let channel = id(&body.channel_id, "channel_id")?;
    if !body.thread && !body.crosspost {
        return Err(bad("thread", "Choose at least one action."));
    }
    let info = guild_info::get(&state, guild.id, false).await?;
    info.check_post_channel("channel_id", channel as u64)?;
    if body.crosspost && info.channel(channel as u64).is_none_or(|c| c.kind != 5) {
        return Err(bad("crosspost", "Crossposting needs an announcement channel."));
    }
    let before: Option<(bool, bool)> = sqlx::query_as(
        "SELECT auto_thread,auto_crosspost FROM community_auto_channels WHERE guild_id=$1 AND channel_id=$2",
    )
    .bind(guild.id as i64)
    .bind(channel)
    .fetch_optional(state.app.db.pool())
    .await?;
    sqlx::query("INSERT INTO community_auto_channels(guild_id,channel_id,auto_thread,auto_crosspost) VALUES($1,$2,$3,$4) ON CONFLICT(guild_id,channel_id) DO UPDATE SET auto_thread=$3,auto_crosspost=$4")
        .bind(guild.id as i64).bind(channel).bind(body.thread).bind(body.crosspost).execute(state.app.db.pool()).await?;
    audited(
        &state,
        &guild,
        "community.autopost.save",
        Some(&body.channel_id),
        json!(before),
        json!({"thread":body.thread,"crosspost":body.crosspost}),
    )
    .await;
    Ok(Json(json!({"ok":true})))
}
async fn remove_autopost(
    State(state): State<WebState>,
    guild: ManagedGuild,
    Path((_, raw)): Path<(String, String)>,
) -> ApiResult<Json<Value>> {
    let channel = id(&raw, "channel_id")?;
    let n = sqlx::query("DELETE FROM community_auto_channels WHERE guild_id=$1 AND channel_id=$2")
        .bind(guild.id as i64)
        .bind(channel)
        .execute(state.app.db.pool())
        .await?
        .rows_affected();
    if n == 0 {
        return Err(ApiError::not_found());
    }
    audited(
        &state,
        &guild,
        "community.autopost.remove",
        Some(&raw),
        Value::Null,
        Value::Null,
    )
    .await;
    Ok(Json(json!({"ok":true})))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AnniversaryBody {
    channel_id: Option<String>,
    enabled: bool,
}
async fn save_anniversary(
    State(state): State<WebState>,
    guild: ManagedGuild,
    ApiJson(body): ApiJson<AnniversaryBody>,
) -> ApiResult<Json<Value>> {
    let channel = body.channel_id.as_deref().map(|s| id(s, "channel_id")).transpose()?;
    if body.enabled && channel.is_none() {
        return Err(bad("channel_id", "Choose a channel."));
    }
    if let Some(channel) = channel {
        guild_info::get(&state, guild.id, false)
            .await?
            .check_post_channel("channel_id", channel as u64)?;
    }
    let before: Option<(Option<i64>, bool)> =
        sqlx::query_as("SELECT channel_id,enabled FROM anniversary_settings WHERE guild_id=$1")
            .bind(guild.id as i64)
            .fetch_optional(state.app.db.pool())
            .await?;
    sqlx::query("INSERT INTO anniversary_settings(guild_id,channel_id,enabled) VALUES($1,$2,$3) ON CONFLICT(guild_id) DO UPDATE SET channel_id=$2,enabled=$3")
        .bind(guild.id as i64).bind(channel).bind(body.enabled).execute(state.app.db.pool()).await?;
    audited(
        &state,
        &guild,
        "community.anniversary.save",
        None,
        json!(before),
        json!({"channel_id":body.channel_id,"enabled":body.enabled}),
    )
    .await;
    Ok(Json(json!({"ok":true})))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ConfessionSettingsBody {
    review_channel_id: String,
    publish_channel_id: String,
    enabled: bool,
}
async fn save_confessions(
    State(state): State<WebState>,
    guild: ManagedGuild,
    ApiJson(body): ApiJson<ConfessionSettingsBody>,
) -> ApiResult<Json<Value>> {
    let review = id(&body.review_channel_id, "review_channel_id")?;
    let publish = id(&body.publish_channel_id, "publish_channel_id")?;
    if review == publish {
        return Err(bad("publish_channel_id", "Choose a different public channel."));
    }
    let info = guild_info::get(&state, guild.id, false).await?;
    info.check_post_channel("review_channel_id", review as u64)?;
    info.check_post_channel("publish_channel_id", publish as u64)?;
    let channel = state
        .app
        .http
        .channel(Id::new(review as u64))
        .await
        .map_err(anyhow::Error::from)?
        .model()
        .await
        .map_err(anyhow::Error::from)?;
    let base = info
        .roles
        .iter()
        .find(|role| role.everyone)
        .and_then(|role| role.permissions.parse::<u64>().ok())
        .map(Permissions::from_bits_truncate)
        .unwrap_or(Permissions::empty());
    let public = channel
        .permission_overwrites
        .as_deref()
        .unwrap_or(&[])
        .iter()
        .find(|o| o.kind == PermissionOverwriteType::Role && o.id.get() == guild.id)
        .map_or(base, |o| (base - o.deny) | o.allow)
        .contains(Permissions::VIEW_CHANNEL);
    if public {
        return Err(bad(
            "review_channel_id",
            "The review channel must be private from @everyone.",
        ));
    }
    let before: Option<(Option<i64>, Option<i64>, bool)> = sqlx::query_as(
        "SELECT review_channel_id,publish_channel_id,enabled FROM confession_settings WHERE guild_id=$1",
    )
    .bind(guild.id as i64)
    .fetch_optional(state.app.db.pool())
    .await?;
    sqlx::query("INSERT INTO confession_settings(guild_id,review_channel_id,publish_channel_id,enabled) VALUES($1,$2,$3,$4) ON CONFLICT(guild_id) DO UPDATE SET review_channel_id=$2,publish_channel_id=$3,enabled=$4")
        .bind(guild.id as i64).bind(review).bind(publish).bind(body.enabled).execute(state.app.db.pool()).await?;
    audited(&state,&guild,"community.confessions.settings",None,json!(before),json!({"review_channel_id":body.review_channel_id,"publish_channel_id":body.publish_channel_id,"enabled":body.enabled})).await;
    Ok(Json(json!({"ok":true})))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ReviewBody {
    approve: bool,
}
async fn review_confession(
    State(state): State<WebState>,
    guild: ManagedGuild,
    Path((_, raw)): Path<(String, String)>,
    ApiJson(body): ApiJson<ReviewBody>,
) -> ApiResult<Json<Value>> {
    let confession = id(&raw, "id")?;
    let row:Option<(String,)> = sqlx::query_as("UPDATE confessions SET status=$3,reviewer_id=$4,reviewed_at=now() WHERE guild_id=$1 AND id=$2 AND status='pending' RETURNING body")
        .bind(guild.id as i64).bind(confession).bind(if body.approve {"approved"} else {"denied"})
        .bind(guild.current.session.user_id as i64).fetch_optional(state.app.db.pool()).await?;
    let Some((text,)) = row else {
        return Err(ApiError::not_found());
    };
    if body.approve {
        let channel: Option<(Option<i64>,)> =
            sqlx::query_as("SELECT publish_channel_id FROM confession_settings WHERE guild_id=$1")
                .bind(guild.id as i64)
                .fetch_optional(state.app.db.pool())
                .await?;
        if let Some((Some(channel),)) = channel {
            let lang = state.app.guild_lang(guild.id).await;
            let content = format!(
                "{}\n\n{text}",
                pinchora_core::t!(lang, "community-v2.confession-public-title", id = confession)
            );
            let mentions = quiet();
            let post = state
                .app
                .http
                .create_message(Id::new(channel as u64))
                .content(&content)
                .allowed_mentions(mentions.as_ref())
                .await;
            let message = match post {
                Ok(response) => response.model().await.map_err(anyhow::Error::from),
                Err(error) => Err(anyhow::Error::from(error)),
            };
            let message = match message {
                Ok(message) => message,
                Err(error) => {
                    sqlx::query(
                        "UPDATE confessions SET status='pending',reviewer_id=NULL,reviewed_at=NULL WHERE id=$1",
                    )
                    .bind(confession)
                    .execute(state.app.db.pool())
                    .await?;
                    return Err(error.into());
                }
            };
            sqlx::query("UPDATE confessions SET publish_message_id=$2 WHERE id=$1")
                .bind(confession)
                .bind(message.id.get() as i64)
                .execute(state.app.db.pool())
                .await?;
        } else {
            sqlx::query("UPDATE confessions SET status='pending',reviewer_id=NULL,reviewed_at=NULL WHERE id=$1")
                .bind(confession)
                .execute(state.app.db.pool())
                .await?;
            return Err(bad("approve", "Choose a public confession channel."));
        }
    }
    audited(
        &state,
        &guild,
        "community.confessions.review",
        Some(&raw),
        Value::Null,
        json!({"approved":body.approve}),
    )
    .await;
    Ok(Json(json!({"ok":true})))
}
fn quiet() -> Option<twilight_model::channel::message::AllowedMentions> {
    Some(twilight_model::channel::message::AllowedMentions {
        parse: Vec::new(),
        replied_user: false,
        roles: Vec::new(),
        users: Vec::new(),
    })
}
async fn reveal_confession(
    State(state): State<WebState>,
    guild: ManagedGuild,
    Path((_, raw)): Path<(String, String)>,
) -> ApiResult<Json<Value>> {
    let confession = id(&raw, "id")?;
    let owner = state
        .app
        .http
        .guild(Id::new(guild.id))
        .await
        .map_err(anyhow::Error::from)?
        .model()
        .await
        .map_err(anyhow::Error::from)?
        .owner_id
        .get();
    if owner != guild.current.session.user_id {
        return Err(ApiError::not_found());
    }
    let author: Option<(i64,)> = sqlx::query_as("SELECT author_id FROM confessions WHERE guild_id=$1 AND id=$2")
        .bind(guild.id as i64)
        .bind(confession)
        .fetch_optional(state.app.db.pool())
        .await?;
    let Some((author,)) = author else {
        return Err(ApiError::not_found());
    };
    audit::record(
        &state,
        &guild.current.session,
        &Change {
            guild: guild.id,
            action: "community.confessions.reveal",
            target: Some(&raw),
            before: Value::Null,
            after: json!({"revealed":true}),
        },
    )
    .await?;
    sqlx::query("INSERT INTO confession_reveals(guild_id,confession_id,owner_id) VALUES($1,$2,$3)")
        .bind(guild.id as i64)
        .bind(confession)
        .bind(owner as i64)
        .execute(state.app.db.pool())
        .await?;
    Ok(Json(json!({"author_id":author.to_string()})))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SuggestionBody {
    status: String,
    staff_note: Option<String>,
}
async fn save_suggestion(
    State(state): State<WebState>,
    guild: ManagedGuild,
    Path((_, raw)): Path<(String, String)>,
    ApiJson(body): ApiJson<SuggestionBody>,
) -> ApiResult<Json<Value>> {
    let suggestion = id(&raw, "id")?;
    if !matches!(
        body.status.as_str(),
        "open" | "considering" | "approved" | "implemented" | "denied"
    ) {
        return Err(bad("status", "Choose a valid status."));
    }
    if body.staff_note.as_deref().is_some_and(|n| n.chars().count() > 1000) {
        return Err(bad("staff_note", "Use at most 1000 characters."));
    }
    let before: Option<(String, Option<String>)> =
        sqlx::query_as("SELECT status,staff_note FROM suggestions WHERE guild_id=$1 AND id=$2")
            .bind(guild.id as i64)
            .bind(suggestion)
            .fetch_optional(state.app.db.pool())
            .await?;
    if before.is_none() {
        return Err(ApiError::not_found());
    }
    sqlx::query(
        "UPDATE suggestions SET status=$3,staff_note=$4,decided_by=$5,decided_at=now() WHERE guild_id=$1 AND id=$2",
    )
    .bind(guild.id as i64)
    .bind(suggestion)
    .bind(&body.status)
    .bind(&body.staff_note)
    .bind(guild.current.session.user_id as i64)
    .execute(state.app.db.pool())
    .await?;
    pinchora_commands::refresh_suggestion_message(&state.app, guild.id, suggestion).await?;
    audited(
        &state,
        &guild,
        "community.suggestions.update",
        Some(&raw),
        json!(before),
        json!({"status":body.status,"staff_note":body.staff_note}),
    )
    .await;
    Ok(Json(json!({"ok":true})))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct BanBody {
    banned: bool,
}
async fn ban_confession(
    State(state): State<WebState>,
    guild: ManagedGuild,
    Path((_, raw)): Path<(String, String)>,
    ApiJson(body): ApiJson<BanBody>,
) -> ApiResult<Json<Value>> {
    if !state
        .app
        .db
        .ban_confession_author(guild.id, id(&raw, "id")?, guild.current.session.user_id, body.banned)
        .await?
    {
        return Err(ApiError::not_found());
    }
    audited(
        &state,
        &guild,
        "community.confessions.ban",
        Some(&raw),
        Value::Null,
        json!({"banned":body.banned}),
    )
    .await;
    Ok(Json(json!({"ok":true})))
}
