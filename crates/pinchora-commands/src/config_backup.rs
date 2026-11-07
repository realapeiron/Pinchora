//! Backup orchestration and the same pure validators used by the feature editors.
use crate::App;
use pinchora_core::{
    config_backup::{self, Backup, Import, Names, Policy, Preview, SECTIONS},
    i18n::Lang,
    settings, t,
};
use serde_json::{Value, json};
use twilight_model::id::Id;

pub async fn backup_names(app: &App, guild: u64) -> anyhow::Result<Names> {
    let roles = app.http.roles(Id::new(guild)).await?.models().await?;
    let channels = app.http.guild_channels(Id::new(guild)).await?.models().await?;
    Ok(Names {
        roles: roles.into_iter().map(|r| (r.id.to_string(), r.name)).collect(),
        channels: channels
            .into_iter()
            .filter_map(|c| Some((c.id.to_string(), c.name?)))
            .collect(),
    })
}
/// Expensive backup operations share a guild-wide budget across Discord and the dashboard.
pub async fn backup_rate_limit(app: &App, guild: u64, operation: &str, limit: i64, lang: Lang) -> anyhow::Result<()> {
    let hits = app
        .cache
        .incr(
            &format!("backup:{guild}:{operation}"),
            std::time::Duration::from_secs(60),
        )
        .await?;
    anyhow::ensure!(hits <= limit, "{}", t!(lang, "config_backup.rate_limited"));
    Ok(())
}

pub async fn backup_preview(
    app: &App,
    guild: u64,
    request: &Import,
    lang: Lang,
    actor: u64,
) -> anyhow::Result<(Backup, Preview)> {
    backup_rate_limit(app, guild, "preview", 10, lang).await?;
    config_backup::validate_shape(&request.backup, lang).map_err(anyhow::Error::msg)?;
    let names = backup_names(app, guild).await?;
    let before = app.db.snapshot_config(guild, names.clone()).await?;
    let mut backup = request.backup.clone();
    crate::routing::normalize_backup(&mut backup)?;
    let unmapped: Vec<String> = config_backup::remap(&mut backup, guild, &names)
        .into_iter()
        .map(|entry| {
            let (kind, detail) = entry.split_once(':').unwrap_or(("channel", &entry));
            format!("{}: {detail}", t!(lang, &format!("command_control.ui.{kind}")))
        })
        .collect();
    backup.names = names.clone();
    config_backup::align_serial_ids(&before, &mut backup, request.backup.source_guild == guild.to_string());
    // Upload ids are global. Allocate a deterministic, guild-scoped copy and rewrite design references.
    if request.backup.source_guild != guild.to_string() {
        use sha2::{Digest, Sha256};
        let uploads: Vec<(String, String)> = backup.sections["card_uploads"]
            .iter()
            .filter_map(|r| {
                let old = r["id"].as_str()?.to_owned();
                let digest = Sha256::digest(format!("{guild}:{old}"));
                let new: String = digest[..16].iter().map(|b| format!("{b:02x}")).collect();
                Some((old, new))
            })
            .collect();
        for (old, new) in uploads {
            for section in ["card_uploads", "card_designs"] {
                for row in backup.sections.get_mut(section).unwrap() {
                    replace_string(row, &old, &new);
                }
            }
        }
    }
    if request.policy == Policy::Merge
        && let (Some(old), Some(incoming)) = (
            before.sections["command_control"].first(),
            backup
                .sections
                .get_mut("command_control")
                .and_then(|rows| rows.first_mut()),
        )
    {
        let mut merged: pinchora_core::command_control::Config = serde_json::from_value(old["config"].clone())?;
        let incoming_config: pinchora_core::command_control::Config =
            serde_json::from_value(incoming["config"].clone())?;
        merged.rules.extend(incoming_config.rules);
        merged.aliases.extend(incoming_config.aliases);
        incoming["config"] = serde_json::to_value(merged)?;
    }
    let mut effective = backup.clone();
    if request.policy == Policy::Merge {
        for section in SECTIONS {
            let rows = effective.sections.get_mut(section.table).unwrap();
            for row in &before.sections[section.table] {
                if !rows
                    .iter()
                    .any(|r| config_backup::row_key(section, r) == config_backup::row_key(section, row))
                {
                    rows.push(row.clone());
                }
            }
        }
    }
    let mut errors = vec![];
    for (role, id) in config_backup::references(&backup) {
        if !(if role { &names.roles } else { &names.channels }).contains_key(&id) {
            errors.push(format!("{}: {id}", t!(lang, "config_backup.invalid")));
        }
    }
    if let Err(error) = validate_backup(&effective, lang) {
        errors.push(error);
    }
    if errors.is_empty()
        && let Err(error) = app.db.validate_backup_types(&backup).await
    {
        tracing::debug!(%error, "invalid backup field types");
        errors.push(t!(lang, "config_backup.invalid"));
    }
    if errors.is_empty()
        && unmapped.is_empty()
        && let Err(error) = check_grants(app, guild, actor, &effective, lang).await
    {
        errors.push(error.to_string());
    }
    let differences = config_backup::diff(&before, &backup, request.policy);
    let can_import = unmapped.is_empty() && errors.is_empty();
    Ok((
        backup,
        Preview {
            differences,
            unmapped,
            errors,
            can_import,
        },
    ))
}
fn replace_string(value: &mut Value, old: &str, new: &str) {
    match value {
        Value::String(s) if s == old => *s = new.into(),
        Value::Object(map) => {
            for v in map.values_mut() {
                replace_string(v, old, new);
            }
        }
        Value::Array(values) => {
            for v in values {
                replace_string(v, old, new);
            }
        }
        _ => {}
    }
}
pub async fn backup_apply(app: &App, guild: u64, request: &Import, lang: Lang, actor: u64) -> anyhow::Result<i64> {
    backup_rate_limit(app, guild, "apply", 5, lang).await?;
    let (backup, preview) = backup_preview(app, guild, request, lang, actor).await?;
    anyhow::ensure!(preview.can_import, "{}", t!(lang, "config_backup.blocked"));
    let id = app
        .db
        .import_config(guild, &backup, request.policy, backup.names.clone())
        .await?;
    app.announce_guild_change(guild).await;
    app.announce_xp_change(guild).await;
    Ok(id)
}

pub fn validate_backup(backup: &Backup, lang: Lang) -> Result<(), String> {
    config_backup::validate_shape(backup, lang)?;
    let mut normalized = backup.clone();
    crate::routing::normalize_backup(&mut normalized).map_err(|_| t!(lang, "config_backup.invalid"))?;
    let backup = &normalized;
    let invalid = || t!(lang, "config_backup.invalid");
    let fields =
        |errors: Vec<settings::FieldError>| errors.into_iter().map(|e| e.text(lang)).collect::<Vec<_>>().join(" · ");
    let s = |row: &Value, key: &str| row[key].as_str().map(str::to_owned).ok_or_else(invalid);
    let n = |row: &Value, key: &str| row[key].as_i64().ok_or_else(invalid);
    let string_list = |row: &Value, key: &str| -> Result<Vec<String>, String> {
        serde_json::from_value(row[key].clone()).map_err(|_| invalid())
    };
    let get = |table: &str| backup.sections.get(table).map(Vec::as_slice).unwrap_or_default();
    for (table, max) in [
        ("guild_modules", pinchora_core::modules::Module::ALL.len()),
        ("card_uploads", pinchora_core::card_design::MAX_UPLOADS),
        ("card_designs", 2),
        ("auto_roles", pinchora_core::roles_pack::MAX_AUTO_ROLES),
        ("color_roles", pinchora_core::roles_pack::MAX_COLOR_ROLES),
        ("voice_creators", pinchora_core::temp_voice::MAX_CREATORS),
        ("ticket_panels", pinchora_core::tickets::MAX_PANELS),
        ("ticket_forms", 10),
        ("forms", 10),
        ("qotd_questions", 500),
        ("automod_rules", pinchora_core::automod::MAX_RULES),
        ("feeds", pinchora_core::feeds::MAX_FEEDS as usize),
        ("starboards", 3),
        ("counter_channels", pinchora_core::server_stats::MAX_COUNTERS),
        ("scheduled_messages", pinchora_core::utility::MAX_SCHEDULES),
        ("custom_commands", 50),
        ("custom_responders", 50),
        ("level_roles", pinchora_db::MAX_LEVEL_ROLES as usize),
        ("rep_roles", pinchora_db::MAX_REP_ROLES as usize),
        ("store_items", pinchora_db::MAX_STORE_ITEMS as usize),
        ("xp_rules", pinchora_db::MAX_XP_RULES as usize),
        ("invite_rewards", 20),
        ("mod_warn_thresholds", pinchora_core::moderation::MAX_THRESHOLDS),
    ] {
        if get(table).len() > max {
            return Err(format!(
                "{}: {}",
                t!(lang, &format!("config_backup.section.{table}")),
                invalid()
            ));
        }
    }
    for table in ["utility_settings", "event_settings"] {
        for row in get(table) {
            if pinchora_core::utility::timezone(&s(row, "timezone")?).is_none() {
                return Err(invalid());
            }
        }
    }
    for (table, max) in [("sticky_messages", 1000), ("scheduled_messages", 2000)] {
        for row in get(table) {
            let content = s(row, "content")?;
            let embed: Option<pinchora_core::tags::EmbedSpec> =
                serde_json::from_value(row["embed"].clone()).map_err(|_| invalid())?;
            let cleaned = pinchora_core::tags::clean_embed(embed.clone()).map_err(fields)?;
            if content.chars().count() > max
                || pinchora_core::utility::clean(&content, max) != content
                || (content.is_empty() && cleaned.is_none())
                || cleaned != embed
                || !row["enabled"].is_boolean()
            {
                return Err(invalid());
            }
            if table == "sticky_messages" {
                if !(5..=86400).contains(&n(row, "min_gap_secs")?) || !(1..=1000).contains(&n(row, "min_messages")?) {
                    return Err(invalid());
                }
            } else {
                let schedule: pinchora_core::utility::Schedule =
                    serde_json::from_value(row["schedule"].clone()).map_err(|_| invalid())?;
                if !pinchora_core::utility::validate(&schedule)
                    || pinchora_core::utility::timezone(&s(row, "tz")?).is_none()
                    || chrono::DateTime::parse_from_rfc3339(&s(row, "next_run_at")?).is_err()
                {
                    return Err(invalid());
                }
            }
        }
    }
    for row in get("command_control") {
        let config: pinchora_core::command_control::Config =
            serde_json::from_value(row["config"].clone()).map_err(|_| invalid())?;
        let mut previous = config.clone();
        previous
            .aliases
            .retain(|name, _| matches!(name.as_str(), "setup" | "mod" | "rewards"));
        config.validate_update(
            &previous,
            &crate::command_control::control_validation_catalog(),
            &crate::command_reserved_aliases(),
            lang,
        )?;
    }
    for row in get("guild_modules") {
        use pinchora_core::modules::{self, ChannelMode, Module};
        let module = Module::parse(&s(row, "module")?).ok_or_else(invalid)?;
        let mode = ChannelMode::parse(&s(row, "channel_mode")?).ok_or_else(invalid)?;
        if module == Module::CommandControl && (row["enabled"] != true || mode != ChannelMode::All) {
            return Err(t!(lang, "command_control.protected"));
        }
        let ids = string_list(row, "channels")?
            .iter()
            .map(|id| id.parse::<u64>().map_err(|_| invalid()))
            .collect::<Result<Vec<_>, _>>()?;
        modules::check_channels(module, mode, &ids).map_err(|e| e.text(lang))?;
    }
    for row in get("guilds") {
        if !row["welcome_accent"].is_null() && !(0..=0xffffff).contains(&n(row, "welcome_accent")?) {
            return Err(invalid());
        }
        for (key, kind) in [
            ("welcome_message", settings::MessageKind::Welcome),
            ("goodbye_message", settings::MessageKind::Goodbye),
            ("levelup_message", settings::MessageKind::LevelUp),
        ] {
            if let Some(text) = row[key].as_str() {
                settings::check_message(kind, text).map_err(|e| e.text(lang))?;
            }
        }
        for (key, check) in [
            (
                "prefix",
                settings::check_prefix as fn(&str) -> Result<String, settings::FieldError>,
            ),
            ("welcome_title", settings::check_welcome_title),
            ("points_name", settings::check_points_name),
            ("points_emoji", settings::check_points_emoji),
        ] {
            if let Some(text) = row[key].as_str() {
                check(text).map_err(|e| e.text(lang))?;
            }
        }
        if let Some(theme) = row["welcome_theme"].as_str() {
            settings::check_theme(theme).map_err(|e| e.text(lang))?;
        }
        settings::parse_levelup_mode(&s(row, "levelup_mode")?).map_err(|e| e.text(lang))?;
        settings::check_points(n(row, "points_min")?, n(row, "points_max")?).map_err(|e| e.text(lang))?;
        settings::check_voice_amount(n(row, "voice_xp_amount")?).map_err(|e| e.text(lang))?;
        if let Some(language) = row["language"].as_str()
            && Lang::from_code(language).is_none()
        {
            return Err(invalid());
        }
    }
    for row in get("card_uploads") {
        use pinchora_core::card_design::*;
        let id = s(row, "id")?;
        let png = s(row, "png")?;
        if !is_upload_id(&id)
            || !png.starts_with("\\x89504e470d0a1a0a")
            || png.len() > UPLOAD_STORED_MAX_BYTES * 2 + 2
            || !(1..=i64::from(UPLOAD_MAX_SIDE)).contains(&n(row, "width")?)
            || !(1..=i64::from(UPLOAD_MAX_SIDE)).contains(&n(row, "height")?)
        {
            return Err(invalid());
        }
        let bytes = png.as_bytes()[2..]
            .chunks(2)
            .map(|pair| {
                std::str::from_utf8(pair)
                    .ok()
                    .and_then(|hex| u8::from_str_radix(hex, 16).ok())
                    .filter(|_| pair.len() == 2)
                    .ok_or_else(invalid)
            })
            .collect::<Result<Vec<_>, _>>()?;
        let mut reader = image::ImageReader::with_format(std::io::Cursor::new(bytes), image::ImageFormat::Png);
        let mut limits = image::Limits::default();
        limits.max_image_width = Some(UPLOAD_MAX_SIDE);
        limits.max_image_height = Some(UPLOAD_MAX_SIDE);
        limits.max_alloc = Some(256 * 1024 * 1024);
        reader.limits(limits);
        let decoded = reader.decode().map_err(|_| invalid())?;
        if i64::from(decoded.width()) != n(row, "width")? || i64::from(decoded.height()) != n(row, "height")? {
            return Err(invalid());
        }
    }
    for row in get("card_designs") {
        use pinchora_core::card_design::*;
        let kind = CardKind::parse(&s(row, "kind")?).ok_or_else(invalid)?;
        check_delivery(kind, &s(row, "delivery")?).map_err(|e| e.text(lang))?;
        if !row["design"].is_null() {
            let design = parse_design(&row["design"], &crate::atlas_has).map_err(fields)?;
            if let Some(upload) = design.upload()
                && !get("card_uploads").iter().any(|r| r["id"] == upload)
            {
                return Err(invalid());
            }
        }
    }
    for row in get("level_roles") {
        settings::check_level(n(row, "level")?).map_err(|e| e.text(lang))?;
    }
    for row in get("rep_roles") {
        settings::check_reputation(n(row, "reputation")?).map_err(|e| e.text(lang))?;
    }
    for row in get("store_items") {
        settings::check_price(n(row, "price")?).map_err(|e| e.text(lang))?;
        if !row["stock"].is_null() && n(row, "stock")? != 0 {
            settings::check_stock(n(row, "stock")?).map_err(|e| e.text(lang))?;
        }
    }
    for row in get("xp_rules") {
        settings::check_multiplier(row["multiplier"].as_f64().ok_or_else(invalid)?).map_err(|e| e.text(lang))?;
    }
    for row in get("mod_warn_thresholds") {
        let duration = if row["duration_secs"].is_null() {
            None
        } else {
            Some(chrono::Duration::try_seconds(n(row, "duration_secs")?).ok_or_else(invalid)?)
        };
        pinchora_core::moderation::check_threshold(n(row, "warns")?, &s(row, "action")?, duration)
            .map_err(|e| e.text(lang))?;
    }
    for row in get("auto_roles") {
        use pinchora_core::roles_pack::*;
        AutoKind::parse(&s(row, "kind")?).ok_or_else(invalid)?;
        let delay = n(row, "delay_secs")?;
        let expiry = n(row, "remove_after_secs")?;
        if (delay != 0 && !(30..=MAX_DELAY_SECS).contains(&delay))
            || (expiry != 0 && !(MIN_EXPIRY_SECS..=MAX_EXPIRY_SECS).contains(&expiry))
        {
            return Err(invalid());
        }
    }
    for row in get("invite_settings") {
        for (key, min, max) in [
            ("fake_days", 0, 365),
            ("temp_per_user_daily", 1, 20),
            ("temp_max_age_secs", 60, 604800),
            ("temp_max_uses", 1, 100),
        ] {
            if !(min..=max).contains(&n(row, key)?) {
                return Err(invalid());
            }
        }
    }
    for row in get("invite_rewards") {
        if !(1..=1_000_000).contains(&n(row, "invites")?) {
            return Err(invalid());
        }
    }
    for row in get("role_panels") {
        pinchora_core::roles_pack::check_rules(
            row["exclusive"].as_bool().ok_or_else(invalid)?,
            n(row, "min_picks")? as usize,
            n(row, "max_picks")? as usize,
        )
        .map_err(|_| invalid())?;
    }
    for row in get("ticket_settings") {
        use pinchora_core::tickets::*;
        check_naming(&s(row, "naming")?, lang).map_err(|e| e.text(lang))?;
        check_auto_close(n(row, "auto_close_hours")?, lang).map_err(|e| e.text(lang))?;
        check_open_limit(n(row, "max_open_per_user")?, lang).map_err(|e| e.text(lang))?;
        Mode::parse(&s(row, "mode")?).ok_or_else(invalid)?;
    }
    // Ticket models accept omitted defaults, but their deserializers ignore unknown fields.
    // The raw JSON is persisted, so reject extra keys instead of silently carrying member data.
    for (table, field, keys) in [
        (
            "ticket_forms",
            "questions",
            &["label", "placeholder", "long", "required"][..],
        ),
        (
            "ticket_panels",
            "buttons",
            &["label", "emoji", "description", "tone"][..],
        ),
    ] {
        for row in get(table) {
            let entries = row[field].as_array().ok_or_else(invalid)?;
            for entry in entries {
                if entry
                    .as_object()
                    .ok_or_else(invalid)?
                    .keys()
                    .any(|key| !keys.contains(&key.as_str()))
                {
                    return Err(invalid());
                }
            }
        }
    }
    for row in get("forms") {
        if !pinchora_core::community::valid_form(&s(row, "name")?, &row["questions"]) {
            return Err(invalid());
        }
    }
    for row in get("qotd_settings") {
        if s(row, "timezone")?.parse::<chrono_tz::Tz>().is_err() {
            return Err(invalid());
        }
    }
    for row in get("ticket_forms") {
        let questions = serde_json::from_value::<Vec<pinchora_core::tickets::Question>>(row["questions"].clone())
            .map_err(|_| invalid())?;
        let errors = pinchora_core::tickets::check_questions(&questions, lang);
        if !errors.is_empty() {
            return Err(fields(errors));
        }
    }
    for row in get("ticket_panels") {
        use pinchora_core::tickets::*;
        let topics = serde_json::from_value::<Vec<Topic>>(row["buttons"].clone()).map_err(|_| invalid())?;
        let questions = if row["form_id"].is_null() {
            vec![]
        } else {
            let form = get("ticket_forms")
                .iter()
                .find(|f| f["id"] == row["form_id"])
                .ok_or_else(invalid)?;
            serde_json::from_value(form["questions"].clone()).map_err(|_| invalid())?
        };
        let name = s(row, "name")?;
        if !(0..=0xffffff).contains(&n(row, "color")?) {
            return Err(invalid());
        }
        let title = s(row, "title")?;
        let description = s(row, "description")?;
        let welcome = s(row, "welcome_text")?;
        let errors = check_panel(
            &PanelDraft {
                name: &name,
                title: &title,
                description: &description,
                style: PanelStyle::parse(&s(row, "style")?).ok_or_else(invalid)?,
                topics: &topics,
                questions: &questions,
                welcome: &welcome,
                support_roles: string_list(row, "support_roles")?.len(),
            },
            lang,
        );
        if !errors.is_empty() {
            return Err(fields(errors));
        }
    }
    if let Some(row) = get("mod_parity_settings").first() {
        let config: pinchora_core::mod_parity::Config =
            serde_json::from_value(row["config"].clone()).map_err(|_| invalid())?;
        if !config.valid() {
            return Err(invalid());
        }
    }
    if let Some(row) = get("automod_settings").first() {
        let mut row = row.clone();
        row["observe_until"] = Value::Null;
        row["rules"] = json!(get("automod_rules"));
        let config = serde_json::from_value(row).map_err(|_| invalid())?;
        pinchora_core::automod::validate(&config, lang)?;
    } else if !get("automod_rules").is_empty() {
        return Err(invalid());
    }
    let config = pinchora_core::verification::Config {
        verification: get("verification")
            .first()
            .map(|v| serde_json::from_value(v.clone()))
            .transpose()
            .map_err(|_| invalid())?
            .unwrap_or_default(),
        raid: get("raid_settings")
            .first()
            .map(|v| serde_json::from_value(v.clone()))
            .transpose()
            .map_err(|_| invalid())?
            .unwrap_or_default(),
    };
    pinchora_core::verification::validate(&config, lang)?;
    for row in get("feeds") {
        use pinchora_core::feeds::*;
        let kind = Kind::parse(&s(row, "kind")?).ok_or_else(invalid)?;
        parse_source(kind, &s(row, "source")?).map_err(|_| invalid())?;
        check_message(row["message"].as_str()).map_err(|_| invalid())?;
        for field in ["filter_include", "filter_exclude"] {
            let words = string_list(row, field)?;
            filter_words(words.iter().map(String::as_str)).map_err(|_| invalid())?;
        }
    }
    for row in get("starboards") {
        use pinchora_core::starboard::*;
        let name = s(row, "name")?;
        let emoji = s(row, "emoji")?;
        if name.trim().is_empty()
            || name.chars().count() > 40
            || emoji.trim().is_empty()
            || emoji.chars().count() > 64
            || !(1..=i64::from(MAX_THRESHOLD)).contains(&n(row, "threshold")?)
            || !(0..=0xffffff).contains(&n(row, "color")?)
        {
            return Err(invalid());
        }
        for key in ["only_channels", "except_channels"] {
            if string_list(row, key)?.len() > MAX_SOURCE_CHANNELS {
                return Err(invalid());
            }
        }
        let weights = row["role_weights"].as_object().ok_or_else(invalid)?;
        if weights.len() > MAX_WEIGHTED_ROLES
            || weights
                .values()
                .any(|v| !v.as_i64().is_some_and(|n| (1..=MAX_ROLE_WEIGHT).contains(&n)))
        {
            return Err(invalid());
        }
    }
    for row in get("vote_channels") {
        let up = s(row, "up_emoji")?;
        let down = s(row, "down_emoji")?;
        if up.trim().is_empty() || down.trim().is_empty() || up == down || up.len() > 64 || down.len() > 64 {
            return Err(invalid());
        }
    }
    for row in get("log_channels") {
        pinchora_core::logging::LogCategory::parse(&s(row, "category")?).ok_or_else(invalid)?;
    }
    for row in get("log_settings") {
        for key in ["ignored_roles", "ignored_channels"] {
            let ids = string_list(row, key)?
                .iter()
                .map(|id| id.parse::<u64>().map_err(|_| invalid()))
                .collect::<Result<Vec<_>, _>>()?;
            pinchora_core::logging::check_ignored(key, &ids).map_err(|e| e.text(lang))?;
        }
        for event in string_list(row, "disabled_events")? {
            pinchora_core::logging::LogEvent::parse(&event).ok_or_else(invalid)?;
        }
    }
    for row in get("role_panels") {
        let title = s(row, "title")?;
        if title.trim().is_empty()
            || title.chars().count() > 80
            || row["description"].as_str().is_some_and(|d| d.chars().count() > 500)
        {
            return Err(invalid());
        }
        pinchora_core::roles_pack::PanelStyle::parse(&s(row, "style")?).ok_or_else(invalid)?;
        if get("role_panel_roles")
            .iter()
            .filter(|r| r["panel_id"] == row["id"])
            .count()
            > pinchora_core::roles_pack::MAX_PANEL_ENTRIES
        {
            return Err(invalid());
        }
    }
    for row in get("role_panel_roles") {
        if !get("role_panels").iter().any(|p| p["id"] == row["panel_id"]) {
            return Err(invalid());
        }
        let label = s(row, "label")?;
        if label.trim().is_empty()
            || label.chars().count() > 80
            || row["description"].as_str().is_some_and(|d| d.chars().count() > 100)
            || row["emoji"].as_str().is_some_and(|e| crate::check_emoji(e).is_none())
        {
            return Err(invalid());
        }
    }
    for row in get("panel_reactions") {
        if !get("role_panel_roles")
            .iter()
            .any(|p| p["panel_id"] == row["panel_id"] && p["role_id"] == row["role_id"])
        {
            return Err(invalid());
        }
    }
    for row in get("counter_channels") {
        let kind = pinchora_core::server_stats::CounterKind::parse(&s(row, "kind")?).ok_or_else(invalid)?;
        if kind.needs_role() && row["role_id"].is_null() {
            return Err(invalid());
        }
        pinchora_core::server_stats::check_template(&s(row, "template")?).map_err(|e| e.text(lang))?;
    }
    for row in get("voice_creators") {
        pinchora_core::temp_voice::check_template(&s(row, "name_template")?).map_err(|e| e.text(lang))?;
        pinchora_core::temp_voice::check_limit(n(row, "default_limit")?).map_err(|e| e.text(lang))?;
        pinchora_core::temp_voice::check_per_user(n(row, "max_per_user")?).map_err(|e| e.text(lang))?;
    }
    for table in ["custom_commands", "custom_responders"] {
        for row in get(table) {
            let mut row = row.clone();
            row.as_object_mut().unwrap().remove("id");
            if table == "custom_commands" {
                let def: pinchora_core::tags::CommandDef = serde_json::from_value(row).map_err(|_| invalid())?;
                let clean = def
                    .clone()
                    .clean(
                        &crate::command_control::control_validation_catalog()
                            .into_iter()
                            .map(|c| c.path)
                            .collect::<Vec<_>>(),
                    )
                    .map_err(fields)?;
                if clean != def {
                    return Err(invalid());
                }
            } else {
                let def: pinchora_core::tags::ResponderDef = serde_json::from_value(row).map_err(|_| invalid())?;
                if def.clone().clean().map_err(fields)? != def {
                    return Err(invalid());
                }
            }
        }
    }
    Ok(())
}

fn discord_id(value: &Value, lang: Lang) -> anyhow::Result<Option<u64>> {
    if value.is_null() {
        return Ok(None);
    }
    let id = value
        .as_str()
        .and_then(|s| s.parse::<u64>().ok())
        .or_else(|| value.as_u64());
    Ok(Some(
        id.filter(|id| *id > 0 && *id <= i64::MAX as u64)
            .ok_or_else(|| anyhow::anyhow!(t!(lang, "config_backup.invalid")))?,
    ))
}

async fn check_grants(app: &App, guild: u64, actor: u64, backup: &Backup, lang: Lang) -> anyhow::Result<()> {
    use settings::{Grantor, RoleFacts};
    use twilight_model::guild::Permissions;
    if let Some(row) = backup.sections.get("mod_parity_settings").and_then(|rows| rows.first()) {
        let config = serde_json::from_value(row["config"].clone())?;
        crate::check_mod_parity_config(app, guild, actor, &config).await?;
    }
    let mut grants = Vec::new();
    for (table, field) in [
        ("forms", "accept_role_id"),
        ("counting_channels", "fail_role_id"),
        ("level_roles", "role_id"),
        ("rep_roles", "role_id"),
        ("store_items", "role_id"),
        ("auto_roles", "role_id"),
        ("color_roles", "role_id"),
        ("role_panel_roles", "role_id"),
        ("panel_reactions", "role_id"),
        ("invite_rewards", "role_id"),
        ("guilds", "join_role_id"),
        ("verification", "verified_role_id"),
        ("verification", "unverified_role_id"),
    ] {
        for row in &backup.sections[table] {
            if let Some(id) = discord_id(&row[field], lang)? {
                grants.push((id, false));
            }
        }
    }
    for table in ["custom_commands", "custom_responders", "automod_rules"] {
        for row in &backup.sections[table] {
            if let Some(actions) = row["actions"].as_array() {
                for action in actions {
                    let field = if table == "automod_rules" { "role" } else { "role_id" };
                    if let Some(id) = discord_id(&action[field], lang)? {
                        grants.push((id, true));
                    }
                }
            }
        }
    }
    for row in &backup.sections["automod_settings"] {
        if let Some(escalation) = row["escalation"].as_array() {
            for step in escalation {
                if let Some(id) = discord_id(&step["action"]["role"], lang)? {
                    grants.push((id, true));
                }
            }
        }
    }
    let channels = app.http.guild_channels(Id::new(guild)).await?.models().await?;
    for (table, fields) in [
        (
            "guilds",
            &[
                "welcome_channel_id",
                "goodbye_channel_id",
                "log_channel_id",
                "levelup_channel_id",
            ][..],
        ),
        ("sticky_messages", &["channel_id"][..]),
        ("scheduled_messages", &["channel_id"][..]),
        ("feeds", &["channel_id"][..]),
        ("vote_channels", &["channel_id"][..]),
        ("suggestion_settings", &["channel_id"][..]),
        ("collection_settings", &["channel_id"][..]),
        ("party_settings", &["surge_channel_id"][..]),
        ("ticket_settings", &["log_channel_id"][..]),
        ("invite_settings", &["log_channel_id", "temp_channel_id"][..]),
        ("starboards", &["channel_id"][..]),
        ("ticket_panels", &["channel_id", "log_channel_id"][..]),
        ("log_channels", &["channel_id"][..]),
        ("mod_settings", &["log_channel_id"][..]),
        ("automod_settings", &["alert_channel_id"][..]),
    ] {
        for row in &backup.sections[table] {
            for field in fields {
                if let Some(id) = discord_id(&row[field], lang)? {
                    anyhow::ensure!(
                        channels.iter().any(|c| c.id.get() == id
                            && matches!(
                                c.kind,
                                twilight_model::channel::ChannelType::GuildText
                                    | twilight_model::channel::ChannelType::GuildAnnouncement
                            )),
                        "{}",
                        t!(lang, "config_backup.invalid")
                    );
                }
            }
        }
    }
    use twilight_model::channel::ChannelType::{GuildAnnouncement, GuildCategory, GuildForum, GuildText, GuildVoice};
    for (table, field, kinds) in [
        ("voice_creators", "channel_id", &[GuildVoice][..]),
        ("voice_creators", "category_id", &[GuildCategory][..]),
        (
            "voice_creators",
            "panel_channel_id",
            &[GuildText, GuildAnnouncement][..],
        ),
        ("ticket_settings", "open_category_id", &[GuildCategory][..]),
        ("ticket_settings", "closed_category_id", &[GuildCategory][..]),
        ("ticket_panels", "open_category_id", &[GuildCategory][..]),
        ("qotd_settings", "channel_id", &[GuildText, GuildAnnouncement][..]),
        ("counting_channels", "channel_id", &[GuildText, GuildAnnouncement][..]),
        ("forms", "submit_channel_id", &[GuildText, GuildAnnouncement][..]),
        ("modmail_settings", "forum_channel_id", &[GuildForum][..]),
        (
            "modmail_settings",
            "log_channel_id",
            &[GuildText, GuildAnnouncement][..],
        ),
        ("role_panels", "channel_id", &[GuildText, GuildAnnouncement][..]),
        (
            "counter_channels",
            "channel_id",
            &[GuildVoice, GuildText, GuildAnnouncement, GuildCategory][..],
        ),
    ] {
        for row in &backup.sections[table] {
            if let Some(id) = discord_id(&row[field], lang)? {
                anyhow::ensure!(
                    channels.iter().any(|c| c.id.get() == id && kinds.contains(&c.kind)),
                    "{}",
                    t!(lang, "config_backup.invalid")
                );
            }
        }
    }
    if grants.is_empty() {
        return Ok(());
    }
    // Ownership and permissions may have changed since the guild permission cache was populated.
    let owner = app.http.guild(Id::new(guild)).await?.model().await?.owner_id.get();
    let roles = app.http.roles(Id::new(guild)).await?.models().await?;
    let member = app
        .http
        .guild_member(Id::new(guild), Id::new(actor))
        .await?
        .model()
        .await?;
    let bot = app
        .http
        .guild_member(Id::new(guild), Id::new(app.application_id.get()))
        .await?
        .model()
        .await?;
    let top = |held: &[twilight_model::id::Id<twilight_model::id::marker::RoleMarker>]| {
        roles
            .iter()
            .filter(|r| held.contains(&r.id))
            .map(|r| r.position)
            .max()
            .unwrap_or(0)
    };
    let administrator = roles
        .iter()
        .any(|r| member.roles.contains(&r.id) && r.permissions.contains(Permissions::ADMINISTRATOR));
    let grantor = Grantor {
        owner: owner == actor,
        administrator,
        top_role: Some(top(&member.roles)),
    };
    for (id, action) in grants {
        let role = roles
            .iter()
            .find(|r| r.id.get() == id)
            .ok_or_else(|| anyhow::anyhow!("{}", t!(lang, "config_backup.invalid")))?;
        let role_facts = RoleFacts {
            everyone: id == guild,
            managed: role.managed,
            position: role.position,
            permissions: role.permissions.bits(),
        };
        let reason = settings::role_refusal_in(lang, role_facts, Some(top(&bot.roles)), &role.name).or_else(|| {
            if action {
                settings::action_role_refusal_in(lang, role_facts, grantor, &role.name)
            } else {
                settings::grant_refusal_in(lang, role_facts, grantor, &role.name)
            }
        });
        if let Some(reason) = reason {
            anyhow::bail!("{reason}");
        }
    }
    Ok(())
}
