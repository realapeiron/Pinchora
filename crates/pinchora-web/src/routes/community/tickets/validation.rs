use pinchora_core::admin::settings::{FieldError, parse_hex_color};
use pinchora_core::community::tickets::{self, PanelDraft, PanelStyle};
use pinchora_core::infra::i18n::Lang;
use pinchora_core::t;

use crate::error::{ApiError, ApiResult};
use crate::extract::ManagedGuild;
use crate::guild_info::GuildInfo;
use crate::routes::admin::settings::parse_id;
use crate::state::WebState;

use super::{CATEGORY, CheckedPanel, PanelBody, info_for, invalid};

/// An id field of a body: absent or `null` is none, anything else must be a number.
pub(super) fn optional_id(field: &'static str, raw: &Option<String>, lang: Lang) -> ApiResult<Option<u64>> {
    match raw.as_deref() {
        None | Some("") => Ok(None),
        Some(raw) => parse_id(raw)
            .map(Some)
            .ok_or_else(|| ApiError::malformed(t!(lang, "tickets.web.errors.field-id", field = field))),
    }
}

pub(super) fn check_category(
    info: &GuildInfo,
    field: &'static str,
    id: Option<u64>,
    lang: Lang,
) -> Result<(), FieldError> {
    let Some(id) = id else { return Ok(()) };
    match info.channel(id) {
        Some(c) if c.kind == CATEGORY => Ok(()),
        Some(c) => Err(FieldError::new(
            field,
            t!(lang, "tickets.web.errors.panel-category-kind", name = c.name),
        )),
        None => Err(FieldError::new(
            field,
            t!(lang, "tickets.web.errors.panel-category-gone"),
        )),
    }
}

pub(super) fn check_post_channel(info: &GuildInfo, field: &'static str, id: u64, lang: Lang) -> Result<(), FieldError> {
    match info.channel(id) {
        Some(channel) if channel.takes_posts() => Ok(()),
        Some(channel) => Err(FieldError::new(
            field,
            t!(lang, "tickets.web.errors.panel-post-kind", name = channel.name),
        )),
        None => Err(FieldError::new(
            field,
            t!(lang, "tickets.web.errors.panel-channel-gone"),
        )),
    }
}

pub(super) async fn check_panel_body(
    state: &WebState,
    managed: &ManagedGuild,
    body: &PanelBody,
) -> ApiResult<CheckedPanel> {
    let guild = managed.id;
    let lang = managed.panel_lang;
    let channel = parse_id(&body.channel_id)
        .ok_or_else(|| invalid("channel_id", t!(lang, "tickets.web.errors.panel-channel")))?;
    let roles: Vec<u64> = body
        .support_roles
        .iter()
        .map(|raw| {
            parse_id(raw).ok_or_else(|| ApiError::malformed(t!(lang, "economy.web.errors.role-ids-are-numbers")))
        })
        .collect::<ApiResult<_>>()?;
    let open_category = optional_id("open_category_id", &body.open_category_id, lang)?;
    let log = optional_id("log_channel_id", &body.log_channel_id, lang)?;
    let mut named = vec![channel];
    named.extend(open_category);
    named.extend(log);
    let info = info_for(state, guild, &named, &roles).await?;
    let style = match body.style.as_deref() {
        None => PanelStyle::Buttons,
        Some(key) => {
            PanelStyle::parse(key).ok_or_else(|| invalid("style", t!(lang, "tickets.web.errors.panel-style")))?
        }
    };
    let mut errors = tickets::check_panel(
        &PanelDraft {
            name: &body.name,
            title: &body.title,
            description: &body.description,
            style,
            topics: &body.topics,
            questions: &body.questions,
            welcome: &body.welcome_text,
            support_roles: roles.len(),
        },
        lang,
    );
    errors.extend(check_post_channel(&info, "channel_id", channel, lang).err());
    errors.extend(check_category(&info, "open_category_id", open_category, lang).err());
    if let Some(log) = log {
        errors.extend(check_post_channel(&info, "log_channel_id", log, lang).err());
    }
    for role in &roles {
        match info.check_role("support_roles", *role) {
            Ok(r) if r.everyone => errors.push(FieldError::new(
                "support_roles",
                t!(lang, "tickets.web.errors.panel-everyone"),
            )),
            Ok(_) => {}
            Err(_) => errors.push(FieldError::new(
                "support_roles",
                t!(lang, "tickets.web.errors.panel-role-gone"),
            )),
        }
    }
    let color = match body.color.as_deref().map(str::trim).filter(|c| !c.is_empty()) {
        None => tickets::DEFAULT_COLOR,
        Some(hex) => match parse_hex_color(hex) {
            Some(color) => color,
            None => {
                errors.push(FieldError::new("color", t!(lang, "tickets.web.errors.panel-color")));
                0
            }
        },
    };
    if !errors.is_empty() {
        return Err(ApiError::invalid(errors));
    }
    Ok(CheckedPanel {
        channel,
        color,
        style,
        roles,
        open_category,
        log,
    })
}
