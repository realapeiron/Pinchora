//! Previews for the settings forms: messages with their placeholders filled in, as the bot fills them.
//! (The welcome and goodbye images are previewed by `cards`.)

use axum::{Json, extract::State};
use pinchora_commands::fill;
use pinchora_core::settings::{self, MessageKind};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::{
    error::{ApiError, ApiJson, ApiResult},
    extract::ManagedGuild,
    state::WebState,
};

/// The member number a preview shows, as `/config welcome-card` does.
const SAMPLE_COUNT: &str = "1234";
const SAMPLE_LEVEL: &str = "5";
const SAMPLE_ROLE: &str = "@Regular";

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
