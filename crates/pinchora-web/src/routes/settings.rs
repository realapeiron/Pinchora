//! `PATCH /api/guilds/{g}/settings`: change a server's settings with the rules `/config` uses.
//! Checks, writes exactly the fields sent, tells the bot, records what changed.

use axum::{Json, extract::State};
use pinchora_cache::Invalidation;
use pinchora_core::settings::{self, FieldError, GuildSettingsPatch};
use pinchora_db::{GuildSettings, PatchRefused};
use serde::{Deserialize, Deserializer};
use serde_json::{Map, Value};

use super::guilds::SettingsView;
use crate::{
    audit::{self, Change},
    error::{ApiError, ApiJson, ApiResult},
    extract::ManagedGuild,
    guild_info::{self, GuildInfo},
    state::WebState,
};

/// `Some(value)` for a field that is present, even when it is `null`; missing fields stay `None`.
pub fn present<'de, D: Deserializer<'de>, T: Deserialize<'de>>(d: D) -> Result<Option<T>, D::Error> {
    T::deserialize(d).map(Some)
}

/// The JSON a settings form sends: any subset of the columns. Ids are strings, the accent is
/// `#rrggbb`, and `null` clears a field that may be empty.
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SettingsBody {
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

/// The channels and the join role the patch moves to must be ones the bot can use. Unchanged
/// fields are not checked, so a channel deleted in Discord never blocks saving something else.
async fn check_discord(state: &WebState, guild: u64, before: &GuildSettings, after: &GuildSettings) -> ApiResult<()> {
    let moved = |old: Option<i64>, new: Option<i64>| new.filter(|_| new != old).map(|id| id as u64);
    let channels: Vec<(&'static str, u64)> = [
        (
            "levelup_channel_id",
            moved(before.levelup_channel_id, after.levelup_channel_id),
        ),
        (
            "welcome_channel_id",
            moved(before.welcome_channel_id, after.welcome_channel_id),
        ),
        (
            "goodbye_channel_id",
            moved(before.goodbye_channel_id, after.goodbye_channel_id),
        ),
        ("log_channel_id", moved(before.log_channel_id, after.log_channel_id)),
    ]
    .into_iter()
    .filter_map(|(field, id)| Some((field, id?)))
    .collect();
    let role = moved(before.join_role_id, after.join_role_id);
    if channels.is_empty() && role.is_none() {
        return Ok(());
    }
    // Channels may come from the minute-old lists; a role to hand out is checked live, like `/config`.
    let mut info: GuildInfo = guild_info::get_with(state, guild, |info| {
        channels.iter().all(|(_, id)| info.channel(*id).is_some())
    })
    .await?;
    if role.is_some() {
        let live = guild_info::live(state, guild).await?;
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
    if let Some(role) = role
        && let Err(error) = info.check_assignable("join_role_id", role)
    {
        errors.push(error);
    }
    if errors.is_empty() { Ok(()) } else { Err(errors.into()) }
}

pub async fn patch(
    State(state): State<WebState>,
    managed: ManagedGuild,
    ApiJson(body): ApiJson<SettingsBody>,
) -> ApiResult<Json<SettingsView>> {
    let db = &state.app.db;
    let guild = managed.id;
    let current = db.guild_settings(guild).await?;
    let patch = body.into_patch().map_err(ApiError::invalid)?;
    let patch = settings::check_patch(patch, (current.points_min.into(), current.points_max.into()))?;
    let next = current.apply(&patch);
    let (before, after) = diff(&current, &next);
    if after.is_empty() {
        return Ok(Json(current.into()));
    }
    check_discord(&state, guild, &current, &next).await?;
    let updated = match db.patch_guild_settings(guild, &patch).await {
        Ok(updated) => updated,
        // Another save moved the other end of the points range between the check and the write.
        Err(error) => match error.downcast_ref::<PatchRefused>() {
            Some(PatchRefused(refused)) => return Err(ApiError::invalid(vec![refused.clone()])),
            None => return Err(error.into()),
        },
    };
    // Like `/config xp voice enabled:false`: with nobody remembered in voice, no pod pays anyone,
    // even one whose cached settings still say voice XP is on.
    if current.voice_xp_enabled
        && !updated.voice_xp_enabled
        && let Err(error) = pinchora_commands::forget_voice_presence(&state.app, guild).await
    {
        tracing::warn!(guild, %error, "voice presence not forgotten");
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
    Ok(Json(updated.into()))
}

#[cfg(test)]
mod tests {
    use super::*;

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
