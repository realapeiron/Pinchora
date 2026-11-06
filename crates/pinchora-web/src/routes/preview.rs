//! Previews for the settings forms, drawn by the bot's own code: the welcome card PNG exactly as
//! `/config welcome-card` renders it, and messages with their placeholders filled in.

use std::time::Duration;

use axum::{
    Json,
    extract::State,
    http::{HeaderValue, header},
    response::{IntoResponse, Response},
};
use pinchora_commands::{WelcomeDesign, fill, welcome_card_png};
use pinchora_core::settings::{self, FieldError, MessageKind};
use serde::Deserialize;
use serde_json::{Value, json};
use twilight_model::{id::Id, util::ImageHash};

use crate::{
    auth::session::Session,
    error::{ApiError, ApiJson, ApiResult},
    extract::ManagedGuild,
    ratelimit,
    state::WebState,
};

/// Card renders per session per minute; each also waits for a render slot like the bot's cards.
const RENDERS_PER_MINUTE: i64 = 30;
/// The member number a preview shows, as `/config welcome-card` does.
const SAMPLE_COUNT: &str = "1234";
const SAMPLE_LEVEL: &str = "5";
const SAMPLE_ROLE: &str = "@Regular";

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CardBody {
    theme: Option<String>,
    title: Option<String>,
    /// `#rrggbb`; missing or `null` is the theme's own accent.
    accent: Option<String>,
}

impl CardBody {
    fn design(self) -> Result<WelcomeDesign, Vec<FieldError>> {
        let mut errors = Vec::new();
        if let Some(theme) = &self.theme
            && let Err(error) = settings::check_theme(theme)
        {
            errors.push(error);
        }
        let title = self
            .title
            .as_deref()
            .map(settings::check_welcome_title)
            .transpose()
            .unwrap_or_else(|error| {
                errors.push(error);
                None
            });
        let accent = self
            .accent
            .as_deref()
            .map(settings::check_accent)
            .transpose()
            .unwrap_or_else(|error| {
                errors.push(error);
                None
            });
        if !errors.is_empty() {
            return Err(errors);
        }
        Ok(WelcomeDesign {
            theme: self.theme,
            title,
            accent,
        })
    }
}

/// The logged-in user's avatar hash, as Discord gave it at login.
fn avatar(session: &Session) -> Option<ImageHash> {
    session
        .avatar
        .as_deref()
        .and_then(|hash| ImageHash::parse(hash.as_bytes()).ok())
}

/// `POST /preview/welcome-card`: the card a new member would get, with the requester as the
/// member and the server's real name. Nothing is saved.
pub async fn welcome_card(
    State(state): State<WebState>,
    managed: ManagedGuild,
    ApiJson(body): ApiJson<CardBody>,
) -> ApiResult<Response> {
    let session = &managed.current.session;
    ratelimit::check(
        &state.app.cache,
        "preview",
        &session.user_id.to_string(),
        RENDERS_PER_MINUTE,
        Duration::from_secs(60),
    )
    .await?;
    let design = body.design().map_err(ApiError::invalid)?;
    let png = welcome_card_png(
        &state.app,
        Id::new(session.user_id.max(1)),
        avatar(session),
        session.display_name(),
        &managed.guild.name,
        &design,
    )
    .await?;
    let mut response = png.into_response();
    let headers = response.headers_mut();
    headers.insert(header::CONTENT_TYPE, HeaderValue::from_static("image/png"));
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("private, no-store"));
    Ok(response)
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MessageBody {
    kind: String,
    /// `null` or missing previews the default message.
    message: Option<String>,
}

/// What the placeholders turn into in a preview: the requester and the real server, sample numbers.
fn sample(kind: MessageKind, name: &str, server: &str) -> Vec<(&'static str, String)> {
    let mention = format!("@{name}");
    match kind {
        MessageKind::Welcome => vec![
            ("user", mention),
            ("server", server.to_owned()),
            ("count", SAMPLE_COUNT.to_owned()),
        ],
        // A member who left is named, not mentioned.
        MessageKind::Goodbye => vec![
            ("user", name.to_owned()),
            ("server", server.to_owned()),
            ("members", SAMPLE_COUNT.to_owned()),
        ],
        MessageKind::LevelUp => vec![
            ("user", mention),
            ("level", SAMPLE_LEVEL.to_owned()),
            ("server", server.to_owned()),
            ("role", SAMPLE_ROLE.to_owned()),
        ],
    }
}

/// `POST /preview/message`: the text a welcome, goodbye or level-up message turns into.
pub async fn message(
    State(_state): State<WebState>,
    managed: ManagedGuild,
    ApiJson(body): ApiJson<MessageBody>,
) -> ApiResult<Json<Value>> {
    let kind =
        MessageKind::parse(&body.kind).ok_or_else(|| ApiError::malformed("kind is welcome, goodbye or levelup."))?;
    let template = match body.message.as_deref() {
        Some(text) => {
            settings::check_message(kind, text)?;
            text
        }
        None => kind.default_message(),
    };
    let values = sample(kind, managed.current.session.display_name(), &managed.guild.name);
    let pairs: Vec<(&str, &str)> = values.iter().map(|(k, v)| (*k, v.as_str())).collect();
    Ok(Json(json!({ "text": fill(template, &pairs) })))
}
