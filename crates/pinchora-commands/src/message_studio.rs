//! Buttons and menus on saved message kits resolve against the version they were posted with.

use pinchora_core::{
    card_design::CardKind,
    message_studio::{self, KitComponent, MemberData, MessageKit, validate},
    t,
};
use twilight_model::{
    channel::message::{AllowedMentions, MessageFlags},
    http::interaction::InteractionResponseData,
};

use crate::Cx;

pub struct EventMember<'a> {
    pub user_id: u64,
    pub user: &'a str,
    pub server: &'a str,
    pub level: Option<u32>,
    pub booster: bool,
    pub returning: bool,
    pub ordinal: Option<u64>,
}

/// A bound event kit supplies random and conditional text for this member.
pub async fn event_text(
    app: &crate::App,
    guild: u64,
    kind: CardKind,
    member: EventMember<'_>,
) -> anyhow::Result<Option<(String, bool)>> {
    let payload: Option<(serde_json::Value,)> = sqlx::query_as(
        "SELECT k.payload FROM message_event_kits e
        JOIN message_kits k ON k.guild_id=e.guild_id AND k.key=e.kit_key
        WHERE e.guild_id=$1 AND e.kind=$2",
    )
    .bind(guild as i64)
    .bind(kind.key())
    .fetch_optional(app.db.pool())
    .await?;
    let Some((payload,)) = payload else {
        return Ok(None);
    };
    let kit: MessageKit = serde_json::from_value(payload)?;
    let kit = match validate(kit) {
        Ok(kit) if kit.embeds.is_empty() && kit.components.is_empty() => kit,
        _ => return Ok(None),
    };
    let variant = if kit.variants.is_empty() {
        0
    } else {
        rand::random_range(0..kit.variants.len())
    };
    let data = MemberData {
        user: member.user,
        server: member.server,
        level: member.level,
        booster: member.booster,
        returning: member.returning,
        account_age_days: message_studio::account_age_days(member.user_id, app.now().timestamp_millis()),
        ordinal: member.ordinal,
    };
    Ok(Some((
        message_studio::render_content(&kit, variant, &data),
        kit.allow_mass_mentions,
    )))
}

pub(crate) async fn on_component<'a>(
    cx: &Cx,
    mut parts: impl Iterator<Item = &'a str>,
    values: &[String],
) -> anyhow::Result<()> {
    let Some(guild) = cx.guild_id() else {
        return cx.reply_ephemeral(t!(cx.lang(), "component.button-expired")).await;
    };
    let Some(key) = parts.next().filter(|s| !s.is_empty() && s.len() <= 40) else {
        return cx.reply_ephemeral(t!(cx.lang(), "component.button-expired")).await;
    };
    let Some(version) = parts.next().and_then(|s| s.parse::<i64>().ok()) else {
        return cx.reply_ephemeral(t!(cx.lang(), "component.button-expired")).await;
    };
    let Some(index) = parts.next().and_then(|s| s.parse::<usize>().ok()) else {
        return cx.reply_ephemeral(t!(cx.lang(), "component.button-expired")).await;
    };
    if parts.next().is_some() {
        return cx.reply_ephemeral(t!(cx.lang(), "component.button-expired")).await;
    }
    let row: Option<(serde_json::Value,)> = sqlx::query_as(
        "SELECT payload FROM message_kit_history
        WHERE guild_id=$1 AND key=$2 AND version=$3",
    )
    .bind(guild as i64)
    .bind(key)
    .bind(version)
    .fetch_optional(cx.app.db.pool())
    .await?;
    let Some((payload,)) = row else {
        return cx.reply_ephemeral(t!(cx.lang(), "component.button-expired")).await;
    };
    let kit: MessageKit = serde_json::from_value(payload)?;
    let Ok(kit) = validate(kit) else {
        return cx.reply_ephemeral(t!(cx.lang(), "component.button-expired")).await;
    };
    let reply = match kit.components.get(index) {
        Some(KitComponent::Button {
            response: Some(reply), ..
        }) if values.is_empty() => Some(reply.as_str()),
        Some(KitComponent::Select { options, .. }) => values
            .first()
            .and_then(|value| options.iter().find(|o| &o.value == value))
            .map(|o| o.response.as_str()),
        _ => None,
    };
    let Some(reply) = reply else {
        return cx.reply_ephemeral(t!(cx.lang(), "component.button-expired")).await;
    };
    cx.reply(InteractionResponseData {
        content: Some(reply.to_owned()),
        flags: Some(MessageFlags::EPHEMERAL),
        allowed_mentions: Some(AllowedMentions::default()),
        ..Default::default()
    })
    .await
}
