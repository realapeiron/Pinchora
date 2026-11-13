//! Previews for the settings forms: messages with their placeholders filled in, as the bot fills them.
//! (The welcome and goodbye images are previewed by `cards`.)

use std::time::Duration;

use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use pinchora_commands::{deliver, fill, preview_png, server_card};
use pinchora_core::admin::settings;
use pinchora_core::admin::settings::FieldError;
use pinchora_core::admin::settings::MessageKind;
use pinchora_core::infra::i18n::Lang;
use pinchora_core::{
    admin::{
        card_design::{CardKind, Delivery},
        modules::{Access, Module},
    },
    t,
};
use serde::Deserialize;
use serde_json::{Value, json};
use twilight_http::{api_error::ApiError as DiscordApiError, error::ErrorType};
use twilight_model::{channel::message::AllowedMentions, id::Id};

use crate::{
    error::{ApiError, ApiJson, ApiResult},
    extract::ManagedGuild,
    guild_info, ratelimit,
    routes::admin::cards::{preview_error, sample_subject},
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
    let lang = managed.panel_lang;
    let kind = MessageKind::parse(&body.kind)
        .ok_or_else(|| ApiError::malformed(t!(lang, "welcome.web.errors.kind-is-welcome-goodbye-or-levelup")))?;
    Ok(Json(
        json!({ "text": render_message(kind, body.message.as_deref(), managed.current.session.display_name(), &managed.guild.name, Some(managed.panel_lang))? }),
    ))
}

fn render_message(
    kind: MessageKind,
    message: Option<&str>,
    name: &str,
    server: &str,
    lang: Option<Lang>,
) -> ApiResult<String> {
    let template = match message {
        Some(text) => {
            settings::check_message(kind, text).map_err(|error| {
                if let Some(lang) = lang {
                    ApiError::invalid(vec![FieldError::new(error.field, error.text(lang))])
                } else {
                    ApiError::from(error)
                }
            })?;
            text
        }
        None => kind.default_message(),
    };
    let values = sample(kind, name, server);
    let pairs: Vec<(&str, &str)> = values.iter().map(|(k, v)| (*k, v.as_str())).collect();
    Ok(fill(template, &pairs))
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TestMessageBody {
    message: Option<String>,
    channel_id: Option<String>,
}

/// Sends the current welcome, goodbye or welcome DM draft with the manager as the sample member.
pub async fn test_message(
    State(state): State<WebState>,
    managed: ManagedGuild,
    Path((_, kind)): Path<(String, String)>,
    ApiJson(body): ApiJson<TestMessageBody>,
) -> ApiResult<Json<Value>> {
    let guild = managed.id;
    let session = &managed.current.session;
    let lang = state
        .app
        .user_lang_choice(session.user_id)
        .await
        .unwrap_or(state.app.guild_lang(guild).await);
    let dm = kind == "welcome_dm";
    let message_kind = match kind.as_str() {
        "welcome" | "welcome_dm" => MessageKind::Welcome,
        "goodbye" => MessageKind::Goodbye,
        _ => return Err(ApiError::malformed(t!(lang, "studio.test-kind"))),
    };
    let module = if message_kind == MessageKind::Welcome {
        Module::Welcome
    } else {
        Module::Goodbye
    };
    let settings = state.app.db.guild_settings(guild).await?;
    module_test_gate(state.app.module_access(guild, None, module).await, lang)?;
    if message_kind == MessageKind::Welcome && !settings.welcome_enabled {
        return Err(ApiError::blocked(t!(lang, "studio.test-module-off")));
    }
    let card_kind = if message_kind == MessageKind::Welcome {
        CardKind::Welcome
    } else {
        CardKind::Goodbye
    };
    let card = server_card(&state.app, &settings, guild, card_kind).await?;
    if dm && card.delivery != Delivery::Dm {
        return Err(ApiError::blocked(t!(lang, "studio.test-dm-delivery")));
    }
    if !dm && card.delivery == Delivery::Dm {
        return Err(ApiError::blocked(t!(lang, "studio.test-channel-delivery")));
    }
    let default = pinchora_commands::default_message(lang, message_kind);
    let text = render_message(
        message_kind,
        Some(body.message.as_deref().unwrap_or(&default)),
        session.display_name(),
        &managed.guild.name,
        Some(lang),
    )?;
    check_test_rate_limit(&state, guild, session.user_id).await?;
    let channel = if dm {
        state
            .app
            .http
            .create_private_channel(Id::new(session.user_id.max(1)))
            .await
            .map_err(|error| dm_open_error(&error, lang))?
            .model()
            .await
            .map_err(|_| {
                ApiError::new(
                    StatusCode::BAD_GATEWAY,
                    "discord",
                    t!(lang, "studio.test-dm-unavailable"),
                )
            })?
            .id
    } else {
        Id::new(check_test_channel(&state, &managed, body.channel_id.as_deref(), card.delivery, lang).await?)
    };
    let content = test_content(&text, session.display_name(), lang);
    let image = if card.delivery.has_image() {
        let subject = sample_subject(session, &managed.guild.name);
        let png = preview_png(&state.app, guild, &card.design, &subject)
            .await
            .map_err(preview_error)?;
        Some((format!("{}.png", card_kind.key()), png))
    } else {
        None
    };
    let delivery = if dm { Delivery::WithMessage } else { card.delivery };
    let image = image
        .as_ref()
        .map(|(name, png)| (name.as_str(), png.clone(), card.design.alt_text.as_str()));
    deliver(
        &state.app,
        channel,
        delivery,
        &content,
        image,
        &AllowedMentions::default(),
    )
    .await
    .map_err(|error| {
        tracing::info!(guild, channel = channel.get(), %error, "message test refused by Discord");
        ApiError::new(
            StatusCode::BAD_GATEWAY,
            "discord",
            t!(lang, "studio.test-discord-refused"),
        )
    })?;
    Ok(Json(json!({"posted": true})))
}

async fn check_test_rate_limit(state: &WebState, guild: u64, user: u64) -> ApiResult<()> {
    for (scope, id) in [("message-test", guild), ("message-test-user", user)] {
        ratelimit::check_strict(&state.app.cache, scope, &id.to_string(), 3, Duration::from_secs(60)).await?;
    }
    Ok(())
}

fn test_content(text: &str, name: &str, lang: Lang) -> String {
    let note = t!(lang, "studio.test-note", name = name);
    format!(
        "{}\n-# {}",
        text.chars()
            .take(2000usize.saturating_sub(note.chars().count() + 5))
            .collect::<String>(),
        note
    )
}

fn module_test_gate(access: anyhow::Result<Access>, lang: Lang) -> ApiResult<()> {
    match access {
        Ok(Access::Allowed) => Ok(()),
        Ok(_) => Err(ApiError::blocked(t!(lang, "studio.test-module-off"))),
        Err(error) => {
            tracing::warn!(%error, "module lookup for message test failed");
            Err(ApiError::new(
                StatusCode::BAD_GATEWAY,
                "unavailable",
                t!(lang, "studio.test-module-unavailable"),
            ))
        }
    }
}

fn dm_open_error(error: &twilight_http::Error, lang: pinchora_core::infra::i18n::Lang) -> ApiError {
    if matches!(error.kind(), ErrorType::Response { error: DiscordApiError::General(response), .. } if response.code == 50007)
    {
        ApiError::blocked(t!(lang, "studio.test-dm-blocked"))
    } else {
        ApiError::new(
            StatusCode::BAD_GATEWAY,
            "discord",
            t!(lang, "studio.test-dm-unavailable"),
        )
    }
}

async fn check_test_channel(
    state: &WebState,
    managed: &ManagedGuild,
    raw: Option<&str>,
    delivery: Delivery,
    lang: pinchora_core::infra::i18n::Lang,
) -> ApiResult<u64> {
    let id = raw
        .and_then(super::settings::parse_id)
        .ok_or_else(|| ApiError::blocked(t!(lang, "studio.test-pick-channel")))?;
    let info = guild_info::get_with(state, managed.id, |info| info.channel(id).is_some()).await?;
    if !info.channel(id).is_some_and(|channel| channel.takes_posts()) {
        return Err(ApiError::invalid(vec![FieldError::new(
            "channel_id",
            t!(lang, "studio.test-invalid-channel"),
        )]));
    }
    if !guild_info::manager_can_view(state, managed, id, &info.roles).await? {
        return Err(ApiError::blocked(t!(lang, "studio.test-channel-hidden")));
    }
    let missing = &info.channel(id).expect("checked channel").missing_permissions;
    let required: &[&str] = if delivery.has_image() {
        &["view_channel", "send_messages", "attach_files"]
    } else {
        &["view_channel", "send_messages"]
    };
    let missing: Vec<&str> = missing
        .iter()
        .map(String::as_str)
        .filter(|key| required.contains(key))
        .collect();
    if !missing.is_empty() {
        let names = missing
            .iter()
            .map(|key| t!(lang, &format!("studio.permission-{key}")))
            .collect::<Vec<_>>()
            .join(", ");
        return Err(ApiError::blocked(t!(
            lang,
            "studio.test-missing-permissions",
            permissions = names
        )));
    }
    Ok(id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn message_test_module_lookup_fails_closed() {
        assert!(module_test_gate(Ok(Access::Allowed), Lang::En).is_ok());
        assert_eq!(
            module_test_gate(Ok(Access::ServerOff), Lang::En).unwrap_err().status,
            StatusCode::UNPROCESSABLE_ENTITY
        );
        let failure = module_test_gate(Err(anyhow::anyhow!("database unavailable")), Lang::Tr).unwrap_err();
        assert_eq!((failure.status, failure.code), (StatusCode::BAD_GATEWAY, "unavailable"));
        assert!(failure.message.contains("denetlenemedi"));
    }
}
