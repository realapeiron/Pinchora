//! `PATCH /api/guilds/{g}/settings`: change a server's settings with the rules `/config` uses.
//! Checks, writes exactly the fields sent, tells the bot, records what changed.

use axum::{Json, extract::State};
use pinchora_cache::Invalidation;
use pinchora_core::admin::settings;
use pinchora_core::admin::settings::FieldError;
use pinchora_core::admin::settings::GuildSettingsPatch;
use pinchora_core::t;
use pinchora_db::{GuildSettings, PatchRefused};
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::{Map, Value};

use crate::audit;
use crate::audit::Change;
use crate::error::ApiError;
use crate::error::ApiJson;
use crate::error::ApiResult;
use crate::extract::ManagedGuild;
use crate::guild_info;
use crate::guild_info::GuildInfo;
use crate::routes::admin::guilds::SettingsView;
use crate::state::WebState;

/// `Some(value)` for a field that is present, even when it is `null`; missing fields stay `None`.
pub fn present<'de, D: Deserializer<'de>, T: Deserialize<'de>>(d: D) -> Result<Option<T>, D::Error> {
    T::deserialize(d).map(Some)
}

/// The JSON a settings form sends: any subset of the columns. Ids are strings, the accent is
/// `#rrggbb`, and `null` clears a field that may be empty.
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SettingsBody {
    expect: Option<Map<String, Value>>,
    leveling_enabled: Option<bool>,
    #[serde(default, deserialize_with = "present")]
    levelup_channel_id: Option<Option<String>>,
    #[serde(default, deserialize_with = "present")]
    levelup_message: Option<Option<String>>,
    levelup_mode: Option<String>,
    level_roles_stack: Option<bool>,
    voice_xp_enabled: Option<bool>,
    voice_xp_amount: Option<i64>,
    welcome_enabled: Option<bool>,
    #[serde(default, deserialize_with = "present")]
    welcome_channel_id: Option<Option<String>>,
    #[serde(default, deserialize_with = "present")]
    welcome_message: Option<Option<String>>,
    #[serde(default, deserialize_with = "present")]
    welcome_theme: Option<Option<String>>,
    #[serde(default, deserialize_with = "present")]
    welcome_title: Option<Option<String>>,
    #[serde(default, deserialize_with = "present")]
    welcome_accent: Option<Option<String>>,
    #[serde(default, deserialize_with = "present")]
    join_role_id: Option<Option<String>>,
    #[serde(default, deserialize_with = "present")]
    goodbye_channel_id: Option<Option<String>>,
    #[serde(default, deserialize_with = "present")]
    goodbye_message: Option<Option<String>>,
    points_enabled: Option<bool>,
    points_name: Option<String>,
    points_emoji: Option<String>,
    points_min: Option<i64>,
    points_max: Option<i64>,
    #[serde(default, deserialize_with = "present")]
    log_channel_id: Option<Option<String>>,
    log_members: Option<bool>,
    log_names: Option<bool>,
    log_bans: Option<bool>,
    log_voice: Option<bool>,
    /// `null` goes back to the default prefix.
    #[serde(default, deserialize_with = "present")]
    prefix: Option<Option<String>>,
}

#[derive(Serialize)]
pub struct SettingsResponse {
    #[serde(flatten)]
    settings: SettingsView,
    skipped: Vec<&'static str>,
}

/// A Discord id from a form: digits, not zero.
pub fn parse_id(raw: &str) -> Option<u64> {
    raw.parse::<u64>().ok().filter(|&id| id > 0)
}

fn id_field(errors: &mut Vec<FieldError>, field: &'static str, raw: Option<Option<String>>) -> Option<Option<u64>> {
    raw.map(|inner| {
        inner.and_then(|s| {
            let id = parse_id(&s);
            if id.is_none() {
                errors.push(FieldError::new(field, "That isn't a Discord id."));
            }
            id
        })
    })
}

impl SettingsBody {
    /// The typed patch, or what could not be read.
    fn into_patch(self) -> Result<GuildSettingsPatch, Vec<FieldError>> {
        let mut errors = Vec::new();
        let levelup_mode = self.levelup_mode.and_then(|m| match settings::parse_levelup_mode(&m) {
            Ok(mode) => Some(mode),
            Err(error) => {
                errors.push(error);
                None
            }
        });
        let welcome_accent = self.welcome_accent.map(|accent| {
            accent.and_then(|a| match settings::check_accent(&a) {
                Ok(rgb) => Some(rgb),
                Err(error) => {
                    errors.push(error);
                    None
                }
            })
        });
        let patch = GuildSettingsPatch {
            leveling_enabled: self.leveling_enabled,
            levelup_channel_id: id_field(&mut errors, "levelup_channel_id", self.levelup_channel_id),
            levelup_message: self.levelup_message,
            levelup_mode,
            level_roles_stack: self.level_roles_stack,
            voice_xp_enabled: self.voice_xp_enabled,
            voice_xp_amount: self.voice_xp_amount,
            welcome_enabled: self.welcome_enabled,
            welcome_channel_id: id_field(&mut errors, "welcome_channel_id", self.welcome_channel_id),
            welcome_message: self.welcome_message,
            welcome_theme: self.welcome_theme,
            welcome_title: self.welcome_title,
            welcome_accent,
            join_role_id: id_field(&mut errors, "join_role_id", self.join_role_id),
            goodbye_channel_id: id_field(&mut errors, "goodbye_channel_id", self.goodbye_channel_id),
            goodbye_message: self.goodbye_message,
            points_enabled: self.points_enabled,
            points_name: self.points_name,
            points_emoji: self.points_emoji,
            points_min: self.points_min,
            points_max: self.points_max,
            log_channel_id: id_field(&mut errors, "log_channel_id", self.log_channel_id),
            log_members: self.log_members,
            log_names: self.log_names,
            log_bans: self.log_bans,
            log_voice: self.log_voice,
            prefix: self.prefix,
        };
        if errors.is_empty() { Ok(patch) } else { Err(errors) }
    }
}

/// The fields whose shown value differs, as `(before, after)` objects.
fn diff(before: &GuildSettings, after: &GuildSettings) -> (Map<String, Value>, Map<String, Value>) {
    let view = |s: &GuildSettings| match serde_json::to_value(SettingsView::from(s.clone())) {
        Ok(Value::Object(map)) => map,
        _ => Map::new(),
    };
    let (old, new) = (view(before), view(after));
    let mut changed = (Map::new(), Map::new());
    for (field, value) in new {
        let was = old.get(&field).cloned().unwrap_or(Value::Null);
        if was != value {
            changed.0.insert(field.clone(), was);
            changed.1.insert(field, value);
        }
    }
    changed
}

/// Validate every non-null Discord target in the patch, including a value equal to the first
/// snapshot: a concurrent writer can change that field before the locked write.
async fn check_discord(state: &WebState, managed: &ManagedGuild, patch: &GuildSettingsPatch) -> ApiResult<()> {
    let guild = managed.id;
    let target = |value: Option<Option<u64>>| value.flatten();
    let channels: Vec<(&'static str, u64)> = [
        ("levelup_channel_id", target(patch.levelup_channel_id)),
        ("welcome_channel_id", target(patch.welcome_channel_id)),
        ("goodbye_channel_id", target(patch.goodbye_channel_id)),
        ("log_channel_id", target(patch.log_channel_id)),
    ]
    .into_iter()
    .filter_map(|(field, id)| Some((field, id?)))
    .collect();
    let role = target(patch.join_role_id);
    if channels.is_empty() && role.is_none() {
        return Ok(());
    }
    // Channels may come from the minute-old lists; a role to hand out is checked live, like `/config`.
    let mut info: GuildInfo = guild_info::get_with(state, guild, |info| {
        channels.iter().all(|(_, id)| info.channel(*id).is_some())
    })
    .await?;
    let mut grantor = None;
    if role.is_some() {
        let live = guild_info::live(state, guild).await?;
        grantor = Some(guild_info::grantor(state, managed, &live.roles, true).await?);
        info = GuildInfo {
            roles: live.roles,
            bot: live.bot,
            ..info
        };
    }
    let mut errors: Vec<FieldError> = channels
        .iter()
        .filter_map(|(field, id)| info.check_post_channel(field, *id).err())
        .collect();
    if let (Some(role), Some(grantor)) = (role, grantor)
        && let Err(error) = info.check_grant("join_role_id", role, grantor)
    {
        errors.push(error);
    }
    if errors.is_empty() { Ok(()) } else { Err(errors.into()) }
}

pub async fn patch(
    State(state): State<WebState>,
    managed: ManagedGuild,
    ApiJson(body): ApiJson<SettingsBody>,
) -> ApiResult<Json<SettingsResponse>> {
    let db = &state.app.db;
    let guild = managed.id;
    let current = db.guild_settings(guild).await?;
    let lang = state.app.guild_lang(guild).await;
    if body.expect.as_ref().is_some_and(|map| map.contains_key("expect")) {
        return Err(ApiError::malformed(t!(lang, "settings-error.expect")));
    }
    let expect = body
        .expect
        .clone()
        .map(|map| serde_json::from_value::<SettingsBody>(Value::Object(map)))
        .transpose()
        .map_err(|_| ApiError::malformed(t!(lang, "settings-error.expect")))?
        .map(SettingsBody::into_patch)
        .transpose()
        .map_err(ApiError::invalid)?;
    let patch = body.into_patch().map_err(ApiError::invalid)?;
    let patch = if expect.is_some() {
        settings::check_patch_fields(patch)?
    } else {
        settings::check_patch(patch, (current.points_min.into(), current.points_max.into()))?
    };
    // This snapshot only selects the targets to validate. The original patch is compared again
    // under the row lock; a newly eligible Discord target is skipped by the checked-row guard.
    let (candidate, _) = pinchora_db::patch_if_current(&current, &patch, expect.as_ref());
    let next = current.apply(&patch);
    let (_, after) = diff(&current, &next);
    if after.is_empty() && expect.is_none() {
        return Ok(Json(SettingsResponse {
            settings: current.into(),
            skipped: Vec::new(),
        }));
    }
    check_discord(&state, &managed, &candidate).await?;
    if next.leveling_enabled && !current.leveling_enabled
        || next.voice_xp_enabled && !current.voice_xp_enabled
        || next.level_roles_stack != current.level_roles_stack
        || expect.is_some()
            && (patch.leveling_enabled == Some(true)
                || patch.voice_xp_enabled == Some(true)
                || patch.level_roles_stack.is_some())
    {
        crate::dashboard_access::check_module_roles(&state, &managed, pinchora_core::admin::modules::Module::Leveling)
            .await?;
    }
    if next.welcome_enabled && !current.welcome_enabled || expect.is_some() && patch.welcome_enabled == Some(true) {
        crate::dashboard_access::check_module_roles(&state, &managed, pinchora_core::admin::modules::Module::Welcome)
            .await?;
    }
    let (locked_before, updated, skipped) = match db
        .patch_guild_settings_checked(guild, &patch, expect.as_ref(), Some(&current))
        .await
    {
        Ok(updated) => updated,
        // Another save moved the other end of the points range between the check and the write.
        Err(error) => match error.downcast_ref::<PatchRefused>() {
            Some(PatchRefused(refused)) => return Err(ApiError::invalid(vec![refused.clone()])),
            None => return Err(error.into()),
        },
    };
    let (before, after) = diff(&locked_before, &updated);
    if after.is_empty() {
        return Ok(Json(SettingsResponse {
            settings: updated.into(),
            skipped,
        }));
    }
    // Like `/config xp voice enabled:false`: with nobody remembered in voice, and writes refused until it is
    // switched on again, no pod pays anyone, even one whose cached settings still say voice XP is on.
    if locked_before.voice_xp_enabled
        && !updated.voice_xp_enabled
        && let Err(error) = pinchora_commands::voice_switched_off(&state.app, guild).await
    {
        tracing::warn!(guild, %error, "voice presence not forgotten");
    }
    if !locked_before.voice_xp_enabled
        && updated.voice_xp_enabled
        && let Err(error) = pinchora_commands::voice_switched_on(&state.app, guild).await
    {
        tracing::warn!(guild, %error, "voice switch not cleared");
    }
    audit::finish(
        &state,
        &managed.current.session,
        Change {
            guild,
            action: "settings.patch",
            target: None,
            before: Value::Object(before),
            after: Value::Object(after),
        },
        Some(Invalidation::Guild(guild)),
    )
    .await;
    Ok(Json(SettingsResponse {
        settings: updated.into(),
        skipped,
    }))
}

#[cfg(test)]
mod tests {
    use crate::routes::admin::settings::*;

    fn body(json: Value) -> SettingsBody {
        serde_json::from_value(json).unwrap()
    }

    #[test]
    fn null_clears_and_missing_keeps() {
        let patch = body(serde_json::json!({ "welcome_channel_id": null, "goodbye_channel_id": "42" }))
            .into_patch()
            .unwrap();
        assert_eq!(patch.welcome_channel_id, Some(None));
        assert_eq!(patch.goodbye_channel_id, Some(Some(42)));
        assert_eq!(patch.log_channel_id, None);
    }

    #[test]
    fn unreadable_fields_are_named() {
        let errors = body(serde_json::json!({
            "log_channel_id": "general",
            "welcome_accent": "red",
            "levelup_mode": "loud",
        }))
        .into_patch()
        .unwrap_err();
        let fields: Vec<_> = errors.iter().map(|e| e.field).collect();
        assert_eq!(fields, ["levelup_mode", "welcome_accent", "log_channel_id"]);
        assert!(serde_json::from_value::<SettingsBody>(serde_json::json!({ "owner": 1 })).is_err());
    }

    #[test]
    fn the_diff_holds_only_what_changed() {
        let before = GuildSettings {
            points_name: "points".into(),
            welcome_accent: Some(0xff6b4a),
            ..Default::default()
        };
        let after = GuildSettings {
            points_name: "coins".into(),
            ..before.clone()
        };
        let (old, new) = diff(&before, &after);
        assert_eq!(Value::Object(old), serde_json::json!({ "points_name": "points" }));
        assert_eq!(Value::Object(new), serde_json::json!({ "points_name": "coins" }));
    }
}
