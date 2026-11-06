//! The welcome and goodbye image editor's API: the saved designs, the templates, uploaded backgrounds,
//! previews drawn by the bot's own renderer, and a test message to a channel. Designs are checked by
//! `pinchora_core::card_design`, the rules the bot reads them with.

use std::{io::Cursor, time::Duration};

use axum::{
    Json,
    body::Bytes,
    extract::{Path, State},
    http::{HeaderMap, HeaderValue, header},
    response::{IntoResponse, Response},
};
use image::{ImageFormat, ImageReader, Limits, imageops::FilterType};
use pinchora_cache::Invalidation;
use pinchora_commands::{CardSubject, PreviewError, ServerCard, atlas_has, deliver, preview_png, server_card};
use pinchora_core::{
    card_design::{
        self, CardDesign, CardKind, Delivery, HEIGHT_RANGE, UPLOAD_MAX_BYTES, UPLOAD_MAX_SIDE, UPLOAD_STORED_MAX_BYTES,
        WIDTH_RANGE,
    },
    settings::{self, FieldError, MessageKind},
};
use pinchora_render::{encode_png, templates};
use serde::Deserialize;
use serde_json::{Value, json};
use twilight_model::{channel::message::AllowedMentions, id::Id, util::ImageHash};

use super::settings::parse_id;
use crate::{
    audit::{self, Change},
    auth::session::Session,
    error::{ApiError, ApiJson, ApiResult},
    extract::ManagedGuild,
    guild_info, ratelimit,
    state::WebState,
};

/// Previews per session per minute (the editor asks once an edit has settled).
const PREVIEWS_PER_MINUTE: i64 = 60;
/// Previews per server per minute, whoever asks: one busy server cannot fill the preview lane for the rest.
const PREVIEWS_PER_GUILD_PER_MINUTE: i64 = 20;
/// Test messages per server per minute.
const TESTS_PER_MINUTE: i64 = 3;
/// Uploads per server per hour.
const UPLOADS_PER_HOUR: i64 = 20;
/// The member number previews and tests show.
const SAMPLE_MEMBERS: u64 = 1234;

/// What a preview that was not drawn tells the dashboard: when to ask again.
fn preview_error(error: PreviewError) -> ApiError {
    match error {
        PreviewError::Busy => ApiError {
            retry_after: Some(2),
            ..ApiError::new(
                axum::http::StatusCode::SERVICE_UNAVAILABLE,
                "busy",
                "Pinchora is drawing a lot of pictures right now. The preview comes back in a moment.",
            )
        },
        PreviewError::TooSlow => ApiError {
            retry_after: Some(5),
            ..ApiError::new(
                axum::http::StatusCode::SERVICE_UNAVAILABLE,
                "slow",
                "The preview took too long to draw. Try again, or make the design lighter.",
            )
        },
        PreviewError::Failed(error) => ApiError::internal(error),
    }
}

fn kind_of(raw: &str) -> ApiResult<CardKind> {
    CardKind::parse(raw).ok_or_else(|| ApiError::malformed("The kind is welcome or goodbye."))
}

/// The design JSON a request sent, checked.
fn design_of(value: &Value) -> ApiResult<CardDesign> {
    card_design::parse_design(value, &atlas_has).map_err(ApiError::invalid)
}

/// The uploaded picture a design names must be one of the server's own.
async fn check_upload(state: &WebState, guild: u64, design: &CardDesign) -> ApiResult<()> {
    if let Some(id) = design.upload()
        && state.app.db.card_upload(guild, id).await?.is_none()
    {
        return Err(FieldError::new("background", "That uploaded picture is gone. Upload it again.").into());
    }
    Ok(())
}

fn card_json(kind: CardKind, card: &ServerCard) -> Value {
    json!({
        "kind": kind.key(),
        "design": card.design,
        "custom": card.custom,
        "delivery": card.delivery.key(),
        "deliveries": Delivery::offered(kind).iter().map(|d| d.key()).collect::<Vec<_>>(),
    })
}

fn upload_url(guild: u64, id: &str) -> String {
    format!("/api/guilds/{guild}/cards/uploads/{id}")
}

/// `GET /cards`: both images as the bot sends them now, the template gallery and the server's uploads.
pub async fn cards(State(state): State<WebState>, managed: ManagedGuild) -> ApiResult<Json<Value>> {
    let guild = managed.id;
    let settings = state.app.db.guild_settings(guild).await?;
    let welcome = server_card(&state.app, &settings, guild, CardKind::Welcome).await?;
    let goodbye = server_card(&state.app, &settings, guild, CardKind::Goodbye).await?;
    let templates: Vec<Value> = templates::TEMPLATES
        .iter()
        .map(|(key, name)| {
            json!({
                "key": key,
                "name": name,
                "welcome": templates::template(key, CardKind::Welcome),
                "goodbye": templates::template(key, CardKind::Goodbye),
            })
        })
        .collect();
    let uploads: Vec<Value> = state
        .app
        .db
        .card_uploads(guild)
        .await?
        .into_iter()
        .map(|u| json!({ "id": u.id, "width": u.width, "height": u.height, "url": upload_url(guild, &u.id) }))
        .collect();
    Ok(Json(json!({
        "welcome": card_json(CardKind::Welcome, &welcome),
        "goodbye": card_json(CardKind::Goodbye, &goodbye),
        "templates": templates,
        "uploads": uploads,
    })))
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PutBody {
    /// `null`: the classic card from the welcome settings.
    design: Option<Value>,
    delivery: String,
}

/// What the audit log keeps of a card: how it is sent, and the design (or that it is the classic one).
fn audit_json(card: &ServerCard) -> Value {
    json!({
        "delivery": card.delivery.key(),
        "design": if card.custom { json!(card.design) } else { json!("classic") },
    })
}

/// `PUT /cards/{kind}`: saves the design and how it is sent.
pub async fn put_card(
    State(state): State<WebState>,
    managed: ManagedGuild,
    Path((_, kind)): Path<(String, String)>,
    ApiJson(body): ApiJson<PutBody>,
) -> ApiResult<Json<Value>> {
    let guild = managed.id;
    let kind = kind_of(&kind)?;
    let delivery = card_design::check_delivery(kind, &body.delivery)?;
    let design = match &body.design {
        Some(value) => {
            let design = design_of(value)?;
            check_upload(&state, guild, &design).await?;
            Some(design)
        }
        None => None,
    };
    let db = &state.app.db;
    let settings = db.guild_settings(guild).await?;
    let before = server_card(&state.app, &settings, guild, kind).await?;
    let json = design
        .as_ref()
        .map(serde_json::to_string)
        .transpose()
        .map_err(anyhow::Error::from)?;
    db.put_card_design(guild, kind, json.as_deref(), delivery).await?;
    let after = server_card(&state.app, &settings, guild, kind).await?;
    let (old, new) = (audit_json(&before), audit_json(&after));
    if old != new {
        audit::finish(
            &state,
            &managed.current.session,
            Change {
                guild,
                action: "card.put",
                target: Some(kind.key()),
                before: old,
                after: new,
            },
            Some(Invalidation::Guild(guild)),
        )
        .await;
    }
    Ok(Json(card_json(kind, &after)))
}

/// The logged-in user's avatar hash, as Discord gave it at login.
fn avatar(session: &Session) -> Option<ImageHash> {
    session
        .avatar
        .as_deref()
        .and_then(|hash| ImageHash::parse(hash.as_bytes()).ok())
}

/// The requester as the member a preview or test shows, in the real server.
fn sample_subject(session: &Session, server: &str) -> CardSubject {
    CardSubject {
        user: Id::new(session.user_id.max(1)),
        avatar: avatar(session),
        name: session.display_name().to_owned(),
        username: session.username.clone(),
        server: server.to_owned(),
        members: Some(SAMPLE_MEMBERS),
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PreviewBody {
    design: Value,
}

/// `POST /preview/card`: the PNG the bot would draw for `design`, with the requester as the member and
/// the server's real name. Nothing is saved.
pub async fn preview(
    State(state): State<WebState>,
    managed: ManagedGuild,
    ApiJson(body): ApiJson<PreviewBody>,
) -> ApiResult<Response> {
    let session = &managed.current.session;
    ratelimit::check(
        &state.app.cache,
        "preview",
        &session.user_id.to_string(),
        PREVIEWS_PER_MINUTE,
        Duration::from_secs(60),
    )
    .await?;
    ratelimit::check(
        &state.app.cache,
        "preview-guild",
        &managed.id.to_string(),
        PREVIEWS_PER_GUILD_PER_MINUTE,
        Duration::from_secs(60),
    )
    .await?;
    let design = design_of(&body.design)?;
    check_upload(&state, managed.id, &design).await?;
    let subject = sample_subject(session, &managed.guild.name);
    let png = preview_png(&state.app, managed.id, &design, &subject)
        .await
        .map_err(preview_error)?;
    Ok(png_response(png, "private, no-store"))
}

fn png_response(png: Vec<u8>, cache: &'static str) -> Response {
    let mut response = png.into_response();
    let headers = response.headers_mut();
    headers.insert(header::CONTENT_TYPE, HeaderValue::from_static("image/png"));
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static(cache));
    response
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TestBody {
    design: Value,
    delivery: String,
    channel_id: String,
}

/// `POST /cards/{kind}/test`: posts the image and text, as `delivery` would, to a channel the requester
/// picks, with the requester as the member. A direct message delivery is tried out in the channel too.
pub async fn test(
    State(state): State<WebState>,
    managed: ManagedGuild,
    Path((_, kind)): Path<(String, String)>,
    ApiJson(body): ApiJson<TestBody>,
) -> ApiResult<Json<Value>> {
    let guild = managed.id;
    let kind = kind_of(&kind)?;
    let delivery = card_design::check_delivery(kind, &body.delivery)?;
    let design = design_of(&body.design)?;
    check_upload(&state, guild, &design).await?;
    let channel = parse_id(&body.channel_id).ok_or_else(|| ApiError::malformed("Channel ids are numbers."))?;
    let info = guild_info::get_with(&state, guild, |info| info.channel(channel).is_some()).await?;
    info.check_post_channel("channel_id", channel)?;
    ratelimit::check(
        &state.app.cache,
        "card-test",
        &guild.to_string(),
        TESTS_PER_MINUTE,
        Duration::from_secs(60),
    )
    .await?;
    let session = &managed.current.session;
    let subject = sample_subject(session, &managed.guild.name);
    let settings = state.app.db.guild_settings(guild).await?;
    let text = test_text(kind, &settings, &subject);
    let image = if delivery.has_image() {
        let png = preview_png(&state.app, guild, &design, &subject)
            .await
            .map_err(preview_error)?;
        Some((format!("{}.png", kind.key()), png))
    } else {
        None
    };
    let delivery = if delivery == Delivery::Dm {
        Delivery::WithMessage
    } else {
        delivery
    };
    // A test pings nobody, the requester included.
    let mentions = AllowedMentions::default();
    let image = image.as_ref().map(|(name, png)| (name.as_str(), png.clone()));
    deliver(&state.app, Id::new(channel), delivery, &text, image, &mentions)
        .await
        .map_err(|error| {
            tracing::info!(guild, channel, %error, "card test not posted");
            ApiError::new(
                axum::http::StatusCode::BAD_GATEWAY,
                "discord",
                "Discord did not take the test message. Check that the bot can send messages and attach files there.",
            )
        })?;
    tracing::info!(
        guild,
        channel,
        user_id = session.user_id,
        kind = kind.key(),
        "card test posted"
    );
    Ok(Json(json!({ "posted": true })))
}

/// The server's welcome or goodbye text with the requester filled in, marked as a test.
fn test_text(kind: CardKind, settings: &pinchora_db::GuildSettings, subject: &CardSubject) -> String {
    let members = SAMPLE_MEMBERS.to_string();
    let message = match kind {
        CardKind::Welcome => {
            let template = settings
                .welcome_message
                .as_deref()
                .unwrap_or(MessageKind::Welcome.default_message());
            let user = format!("<@{}>", subject.user);
            pinchora_commands::fill(
                template,
                &[("user", &user), ("server", &subject.server), ("count", &members)],
            )
        }
        CardKind::Goodbye => {
            let template = settings.goodbye_message.as_deref().unwrap_or(settings::DEFAULT_GOODBYE);
            pinchora_commands::fill(
                template,
                &[
                    ("user", &subject.name),
                    ("server", &subject.server),
                    ("members", &members),
                ],
            )
        }
    };
    format!(
        "{message}\n-# Test of the {} image, sent from the dashboard by {}.",
        kind.key(),
        subject.name
    )
}

/// `POST /cards/uploads`: a background picture (PNG, JPEG, WebP or GIF, its first frame), up to
/// [`UPLOAD_MAX_BYTES`] and [`UPLOAD_MAX_SIDE`] px a side. It is decoded, shrunk to fit the largest
/// canvas (and [`UPLOAD_STORED_MAX_BYTES`]) and stored as PNG; what was sent is never kept or served.
pub async fn upload(
    State(state): State<WebState>,
    managed: ManagedGuild,
    headers: HeaderMap,
    body: Bytes,
) -> ApiResult<Json<Value>> {
    let guild = managed.id;
    let declared = headers
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default();
    if !matches!(declared, "image/png" | "image/jpeg" | "image/webp" | "image/gif") {
        return Err(FieldError::new("upload", "Upload a PNG, JPEG, WebP or GIF picture.").into());
    }
    if body.len() > UPLOAD_MAX_BYTES {
        return Err(FieldError::new(
            "upload",
            format!("A picture is at most {} MB.", UPLOAD_MAX_BYTES / (1024 * 1024)),
        )
        .into());
    }
    ratelimit::check(
        &state.app.cache,
        "card-upload",
        &guild.to_string(),
        UPLOADS_PER_HOUR,
        Duration::from_secs(3600),
    )
    .await?;
    let (png, width, height) = tokio::task::spawn_blocking(move || reencode(&body))
        .await
        .map_err(anyhow::Error::from)?
        .map_err(|message| ApiError::from(FieldError::new("upload", message)))?;
    let id = hex::encode(rand::random::<[u8; 16]>());
    state.app.db.add_card_upload(guild, &id, &png, width, height).await?;
    audit::finish(
        &state,
        &managed.current.session,
        Change {
            guild,
            action: "card.upload",
            target: Some(&id),
            before: Value::Null,
            after: json!({ "width": width, "height": height, "bytes": png.len() }),
        },
        None,
    )
    .await;
    Ok(Json(
        json!({ "id": id, "width": width, "height": height, "url": upload_url(guild, &id) }),
    ))
}

/// The picture decoded within limits, shrunk to fit the largest canvas, as PNG with its size; or why not.
fn reencode(bytes: &[u8]) -> Result<(Vec<u8>, u32, u32), String> {
    let unreadable = || "That file isn't a picture that can be read.".to_owned();
    let mut reader = ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|_| unreadable())?;
    if !matches!(
        reader.format(),
        Some(ImageFormat::Png | ImageFormat::Jpeg | ImageFormat::WebP | ImageFormat::Gif)
    ) {
        return Err("Upload a PNG, JPEG, WebP or GIF picture.".to_owned());
    }
    let mut limits = Limits::default();
    limits.max_image_width = Some(UPLOAD_MAX_SIDE);
    limits.max_image_height = Some(UPLOAD_MAX_SIDE);
    limits.max_alloc = Some(256 * 1024 * 1024);
    reader.limits(limits);
    let picture = reader
        .decode()
        .map_err(|_| format!("Pictures are at most {UPLOAD_MAX_SIDE} px a side, and must be a readable image."))?;
    let (max_w, max_h) = (*WIDTH_RANGE.end(), *HEIGHT_RANGE.end());
    let mut picture = if picture.width() > max_w || picture.height() > max_h {
        picture.resize(max_w, max_h, FilterType::Triangle)
    } else {
        picture
    };
    loop {
        let rgba = picture.to_rgba8();
        let (w, h) = rgba.dimensions();
        let png = encode_png(&rgba).map_err(|_| unreadable())?;
        if png.len() <= UPLOAD_STORED_MAX_BYTES || w.min(h) <= 64 {
            return Ok((png, w, h));
        }
        // Too many bytes to keep (a photo or noise): shrink by what is over, a little more, and look again.
        let scale = ((UPLOAD_STORED_MAX_BYTES as f64 / png.len() as f64).sqrt() * 0.95).min(0.9);
        picture = picture.resize(
            ((f64::from(w) * scale) as u32).max(1),
            ((f64::from(h) * scale) as u32).max(1),
            FilterType::Triangle,
        );
    }
}

/// `GET /cards/uploads/{id}`: an uploaded picture as stored.
pub async fn upload_png(
    State(state): State<WebState>,
    managed: ManagedGuild,
    Path((_, id)): Path<(String, String)>,
) -> ApiResult<Response> {
    if !card_design::is_upload_id(&id) {
        return Err(ApiError::not_found());
    }
    let png = state
        .app
        .db
        .card_upload(managed.id, &id)
        .await?
        .ok_or_else(ApiError::not_found)?;
    Ok(png_response(png, "private, max-age=86400"))
}
