use super::*;
use pinchora_core::admin::settings;
use serde_json::json;

pub fn validate_backup(backup: &Backup, lang: Lang) -> Result<(), String> {
    config_backup::validate_shape(backup, lang)?;
    let mut normalized = backup.clone();
    crate::infra::routing::normalize_backup(&mut normalized).map_err(|_| t!(lang, "config_backup.invalid"))?;
    let validation = Validation {
        backup: &normalized,
        lang,
    };
    validation.section_limits()?;
    validation.timezones()?;
    validation.scheduled_content()?;
    validation.command_control()?;
    validation.guild_modules()?;
    validation.guild_settings()?;
    validation.card_uploads()?;
    validation.guild_branding()?;
    validation.card_designs()?;
    validation.progression_rewards()?;
    validation.warning_thresholds()?;
    validation.auto_roles()?;
    validation.invites()?;
    validation.role_selection()?;
    validation.ticket_settings()?;
    validation.ticket_fields()?;
    validation.forms()?;
    validation.qotd_timezone()?;
    validation.ticket_forms()?;
    validation.ticket_panels()?;
    validation.moderation_policy()?;
    validation.automod()?;
    validation.verification()?;
    validation.feeds()?;
    validation.starboards()?;
    validation.vote_channels()?;
    validation.logging()?;
    validation.role_panels()?;
    validation.role_panel_entries()?;
    validation.panel_reactions()?;
    validation.counter_channels()?;
    validation.voice_creators()?;
    validation.custom_responses()?;
    Ok(())
}

struct Validation<'a> {
    backup: &'a Backup,
    lang: Lang,
}
impl Validation<'_> {
    fn invalid(&self) -> String {
        t!(self.lang, "config_backup.invalid")
    }
    fn fields(&self, errors: Vec<settings::FieldError>) -> String {
        errors
            .into_iter()
            .map(|e| e.text(self.lang))
            .collect::<Vec<_>>()
            .join(" · ")
    }
    fn string(&self, row: &Value, key: &str) -> Result<String, String> {
        row[key].as_str().map(str::to_owned).ok_or_else(|| self.invalid())
    }
    fn number(&self, row: &Value, key: &str) -> Result<i64, String> {
        row[key].as_i64().ok_or_else(|| self.invalid())
    }
    fn string_list(&self, row: &Value, key: &str) -> Result<Vec<String>, String> {
        serde_json::from_value(row[key].clone()).map_err(|_| self.invalid())
    }
    fn rows(&self, table: &str) -> &[Value] {
        self.backup.sections.get(table).map(Vec::as_slice).unwrap_or_default()
    }
    fn section_limits(&self) -> Result<(), String> {
        let lang = self.lang;
        for (table, max) in [
            ("guild_modules", pinchora_core::admin::modules::Module::ALL.len()),
            // Up to two card backgrounds plus four desired/accepted branding images are retained.
            ("card_uploads", pinchora_core::admin::card_design::MAX_UPLOADS + 6),
            ("card_designs", 2),
            ("auto_roles", pinchora_core::roles::automation::MAX_AUTO_ROLES),
            ("color_roles", pinchora_core::roles::automation::MAX_COLOR_ROLES),
            ("voice_creators", pinchora_core::admin::temp_voice::MAX_CREATORS),
            ("ticket_panels", pinchora_core::community::tickets::MAX_PANELS),
            ("ticket_forms", 10),
            ("forms", 10),
            ("qotd_questions", 500),
            ("automod_rules", pinchora_core::moderation::automod::MAX_RULES),
            ("feeds", pinchora_core::community::feeds::MAX_FEEDS as usize),
            ("starboards", 3),
            ("counter_channels", pinchora_core::admin::server_stats::MAX_COUNTERS),
            (
                "scheduled_messages",
                pinchora_core::admin::afk_sticky_schedule::MAX_SCHEDULES,
            ),
            ("custom_commands", 50),
            ("custom_responders", 50),
            ("level_roles", pinchora_db::MAX_LEVEL_ROLES as usize),
            ("rep_roles", pinchora_db::MAX_REP_ROLES as usize),
            ("store_items", pinchora_db::MAX_STORE_ITEMS as usize),
            ("xp_rules", pinchora_db::MAX_XP_RULES as usize),
            ("invite_rewards", 20),
            (
                "mod_warn_thresholds",
                pinchora_core::moderation::moderation::MAX_THRESHOLDS,
            ),
        ] {
            if self.rows(table).len() > max {
                return Err(format!(
                    "{}: {}",
                    t!(lang, &format!("config_backup.section.{table}")),
                    self.invalid()
                ));
            }
        }
        Ok(())
    }

    fn timezones(&self) -> Result<(), String> {
        for table in ["utility_settings", "event_settings"] {
            for row in self.rows(table) {
                if pinchora_core::admin::afk_sticky_schedule::timezone(&self.string(row, "timezone")?).is_none() {
                    return Err(self.invalid());
                }
            }
        }
        Ok(())
    }

    fn scheduled_content(&self) -> Result<(), String> {
        for (table, max) in [("sticky_messages", 1000), ("scheduled_messages", 2000)] {
            for row in self.rows(table) {
                let content = self.string(row, "content")?;
                let embed: Option<pinchora_core::community::tags::EmbedSpec> =
                    serde_json::from_value(row["embed"].clone()).map_err(|_| self.invalid())?;
                let cleaned =
                    pinchora_core::community::tags::clean_embed(embed.clone()).map_err(|errors| self.fields(errors))?;
                if content.chars().count() > max
                    || pinchora_core::admin::afk_sticky_schedule::clean(&content, max) != content
                    || (content.is_empty() && cleaned.is_none())
                    || cleaned != embed
                    || !row["enabled"].is_boolean()
                {
                    return Err(self.invalid());
                }
                if table == "sticky_messages" {
                    if !(5..=86400).contains(&self.number(row, "min_gap_secs")?)
                        || !(1..=1000).contains(&self.number(row, "min_messages")?)
                    {
                        return Err(self.invalid());
                    }
                } else {
                    let schedule: pinchora_core::admin::afk_sticky_schedule::Schedule =
                        serde_json::from_value(row["schedule"].clone()).map_err(|_| self.invalid())?;
                    if !pinchora_core::admin::afk_sticky_schedule::validate(&schedule)
                        || pinchora_core::admin::afk_sticky_schedule::timezone(&self.string(row, "tz")?).is_none()
                        || chrono::DateTime::parse_from_rfc3339(&self.string(row, "next_run_at")?).is_err()
                    {
                        return Err(self.invalid());
                    }
                }
            }
        }
        Ok(())
    }

    fn command_control(&self) -> Result<(), String> {
        let lang = self.lang;
        for row in self.rows("command_control") {
            let config: pinchora_core::admin::command_control::Config =
                serde_json::from_value(row["config"].clone()).map_err(|_| self.invalid())?;
            let mut previous = config.clone();
            previous
                .aliases
                .retain(|name, _| matches!(name.as_str(), "setup" | "mod" | "rewards"));
            config.validate_update(
                &previous,
                &crate::admin::command_control::control_validation_catalog(),
                &crate::command_reserved_aliases(),
                lang,
            )?;
        }
        Ok(())
    }

    fn guild_modules(&self) -> Result<(), String> {
        let lang = self.lang;
        for row in self.rows("guild_modules") {
            use pinchora_core::admin::modules;
            use pinchora_core::admin::modules::ChannelMode;
            use pinchora_core::admin::modules::Module;
            let module = Module::parse(&self.string(row, "module")?).ok_or_else(|| self.invalid())?;
            let mode = ChannelMode::parse(&self.string(row, "channel_mode")?).ok_or_else(|| self.invalid())?;
            if module == Module::CommandControl && mode != ChannelMode::All {
                return Err(t!(lang, "command_control.protected"));
            }
            let ids = self
                .string_list(row, "channels")?
                .iter()
                .map(|id| id.parse::<u64>().map_err(|_| self.invalid()))
                .collect::<Result<Vec<_>, _>>()?;
            modules::check_channels(module, mode, &ids).map_err(|e| e.text(lang))?;
        }
        Ok(())
    }

    fn guild_settings(&self) -> Result<(), String> {
        let lang = self.lang;
        for row in self.rows("guilds") {
            if !row["welcome_accent"].is_null() && !(0..=0xffffff).contains(&self.number(row, "welcome_accent")?) {
                return Err(self.invalid());
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
            settings::parse_levelup_mode(&self.string(row, "levelup_mode")?).map_err(|e| e.text(lang))?;
            settings::check_points(self.number(row, "points_min")?, self.number(row, "points_max")?)
                .map_err(|e| e.text(lang))?;
            settings::check_voice_amount(self.number(row, "voice_xp_amount")?).map_err(|e| e.text(lang))?;
            if let Some(language) = row["language"].as_str()
                && Lang::from_code(language).is_none()
            {
                return Err(self.invalid());
            }
        }
        Ok(())
    }

    fn card_uploads(&self) -> Result<(), String> {
        for row in self.rows("card_uploads") {
            use pinchora_core::admin::card_design::*;
            let id = self.string(row, "id")?;
            let png = self.string(row, "png")?;
            if !is_upload_id(&id)
                || !png.starts_with("\\x89504e470d0a1a0a")
                || png.len() > UPLOAD_STORED_MAX_BYTES * 2 + 2
                || !(1..=i64::from(UPLOAD_MAX_SIDE)).contains(&self.number(row, "width")?)
                || !(1..=i64::from(UPLOAD_MAX_SIDE)).contains(&self.number(row, "height")?)
            {
                return Err(self.invalid());
            }
            let bytes = png.as_bytes()[2..]
                .chunks(2)
                .map(|pair| {
                    std::str::from_utf8(pair)
                        .ok()
                        .and_then(|hex| u8::from_str_radix(hex, 16).ok())
                        .filter(|_| pair.len() == 2)
                        .ok_or_else(|| self.invalid())
                })
                .collect::<Result<Vec<_>, _>>()?;
            let mut reader = image::ImageReader::with_format(std::io::Cursor::new(bytes), image::ImageFormat::Png);
            let mut limits = image::Limits::default();
            limits.max_image_width = Some(UPLOAD_MAX_SIDE);
            limits.max_image_height = Some(UPLOAD_MAX_SIDE);
            limits.max_alloc = Some(256 * 1024 * 1024);
            reader.limits(limits);
            let decoded = reader.decode().map_err(|_| self.invalid())?;
            if i64::from(decoded.width()) != self.number(row, "width")?
                || i64::from(decoded.height()) != self.number(row, "height")?
            {
                return Err(self.invalid());
            }
        }
        Ok(())
    }

    fn guild_branding(&self) -> Result<(), String> {
        for row in self.rows("guild_branding") {
            let optional = |key: &str| -> Result<Option<String>, String> {
                if row[key].is_null() {
                    Ok(None)
                } else {
                    self.string(row, key).map(Some)
                }
            };
            let branding = pinchora_db::GuildBranding {
                nickname: optional("nickname")?,
                bio: optional("bio")?,
                avatar_upload_id: optional("avatar_upload_id")?,
                banner_upload_id: optional("banner_upload_id")?,
                embed_color: if row["embed_color"].is_null() {
                    None
                } else {
                    Some(i32::try_from(self.number(row, "embed_color")?).map_err(|_| self.invalid())?)
                },
                ..Default::default()
            };
            crate::admin::branding::validate(&branding).map_err(|_| self.invalid())?;
            for upload in [&branding.avatar_upload_id, &branding.banner_upload_id]
                .into_iter()
                .flatten()
            {
                if !self.rows("card_uploads").iter().any(|r| r["id"] == *upload) {
                    return Err(self.invalid());
                }
            }
        }
        Ok(())
    }

    fn card_designs(&self) -> Result<(), String> {
        let lang = self.lang;
        for row in self.rows("card_designs") {
            use pinchora_core::admin::card_design::*;
            let kind = CardKind::parse(&self.string(row, "kind")?).ok_or_else(|| self.invalid())?;
            check_delivery(kind, &self.string(row, "delivery")?).map_err(|e| e.text(lang))?;
            if !row["design"].is_null() {
                let design = parse_design(&row["design"], &crate::atlas_has).map_err(|errors| self.fields(errors))?;
                if let Some(upload) = design.upload()
                    && !self.rows("card_uploads").iter().any(|r| r["id"] == upload)
                {
                    return Err(self.invalid());
                }
            }
        }
        Ok(())
    }

    fn progression_rewards(&self) -> Result<(), String> {
        let lang = self.lang;
        for row in self.rows("level_roles") {
            settings::check_level(self.number(row, "level")?).map_err(|e| e.text(lang))?;
        }
        for row in self.rows("rep_roles") {
            settings::check_reputation(self.number(row, "reputation")?).map_err(|e| e.text(lang))?;
        }
        for row in self.rows("store_items") {
            settings::check_price(self.number(row, "price")?).map_err(|e| e.text(lang))?;
            if !row["stock"].is_null() && self.number(row, "stock")? != 0 {
                settings::check_stock(self.number(row, "stock")?).map_err(|e| e.text(lang))?;
            }
        }
        for row in self.rows("xp_rules") {
            settings::check_multiplier(row["multiplier"].as_f64().ok_or_else(|| self.invalid())?)
                .map_err(|e| e.text(lang))?;
        }
        Ok(())
    }

    fn warning_thresholds(&self) -> Result<(), String> {
        let lang = self.lang;
        for row in self.rows("mod_warn_thresholds") {
            let duration = if row["duration_secs"].is_null() {
                None
            } else {
                Some(chrono::Duration::try_seconds(self.number(row, "duration_secs")?).ok_or_else(|| self.invalid())?)
            };
            pinchora_core::moderation::moderation::check_threshold(
                self.number(row, "warns")?,
                &self.string(row, "action")?,
                duration,
            )
            .map_err(|e| e.text(lang))?;
        }
        Ok(())
    }

    fn auto_roles(&self) -> Result<(), String> {
        for row in self.rows("auto_roles") {
            use pinchora_core::roles::automation::*;
            AutoKind::parse(&self.string(row, "kind")?).ok_or_else(|| self.invalid())?;
            let delay = self.number(row, "delay_secs")?;
            let expiry = self.number(row, "remove_after_secs")?;
            if (delay != 0 && !(30..=MAX_DELAY_SECS).contains(&delay))
                || (expiry != 0 && !(MIN_EXPIRY_SECS..=MAX_EXPIRY_SECS).contains(&expiry))
            {
                return Err(self.invalid());
            }
        }
        Ok(())
    }

    fn invites(&self) -> Result<(), String> {
        for row in self.rows("invite_settings") {
            for (key, min, max) in [
                ("fake_days", 0, 365),
                ("temp_per_user_daily", 1, 20),
                ("temp_max_age_secs", 60, 604800),
                ("temp_max_uses", 1, 100),
            ] {
                if !(min..=max).contains(&self.number(row, key)?) {
                    return Err(self.invalid());
                }
            }
        }
        for row in self.rows("invite_rewards") {
            if !(1..=1_000_000).contains(&self.number(row, "invites")?) {
                return Err(self.invalid());
            }
        }
        Ok(())
    }

    fn role_selection(&self) -> Result<(), String> {
        for row in self.rows("role_panels") {
            pinchora_core::roles::automation::check_rules(
                row["exclusive"].as_bool().ok_or_else(|| self.invalid())?,
                self.number(row, "min_picks")? as usize,
                self.number(row, "max_picks")? as usize,
            )
            .map_err(|_| self.invalid())?;
        }
        Ok(())
    }

    fn ticket_settings(&self) -> Result<(), String> {
        let lang = self.lang;
        for row in self.rows("ticket_settings") {
            use pinchora_core::community::tickets::*;
            check_naming(&self.string(row, "naming")?, lang).map_err(|e| e.text(lang))?;
            check_auto_close(self.number(row, "auto_close_hours")?, lang).map_err(|e| e.text(lang))?;
            check_open_limit(self.number(row, "max_open_per_user")?, lang).map_err(|e| e.text(lang))?;
            Mode::parse(&self.string(row, "mode")?).ok_or_else(|| self.invalid())?;
        }
        Ok(())
    }

    fn ticket_fields(&self) -> Result<(), String> {
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
            for row in self.rows(table) {
                let entries = row[field].as_array().ok_or_else(|| self.invalid())?;
                for entry in entries {
                    if entry
                        .as_object()
                        .ok_or_else(|| self.invalid())?
                        .keys()
                        .any(|key| !keys.contains(&key.as_str()))
                    {
                        return Err(self.invalid());
                    }
                }
            }
        }
        Ok(())
    }

    fn forms(&self) -> Result<(), String> {
        for row in self.rows("forms") {
            if !pinchora_core::community::community::valid_form(&self.string(row, "name")?, &row["questions"]) {
                return Err(self.invalid());
            }
        }
        Ok(())
    }

    fn qotd_timezone(&self) -> Result<(), String> {
        for row in self.rows("qotd_settings") {
            if self.string(row, "timezone")?.parse::<chrono_tz::Tz>().is_err() {
                return Err(self.invalid());
            }
        }
        Ok(())
    }

    fn ticket_forms(&self) -> Result<(), String> {
        let lang = self.lang;
        for row in self.rows("ticket_forms") {
            let questions =
                serde_json::from_value::<Vec<pinchora_core::community::tickets::Question>>(row["questions"].clone())
                    .map_err(|_| self.invalid())?;
            let errors = pinchora_core::community::tickets::check_questions(&questions, lang);
            if !errors.is_empty() {
                return Err(self.fields(errors));
            }
        }
        Ok(())
    }

    fn ticket_panels(&self) -> Result<(), String> {
        let lang = self.lang;
        for row in self.rows("ticket_panels") {
            use pinchora_core::community::tickets::*;
            let topics = serde_json::from_value::<Vec<Topic>>(row["buttons"].clone()).map_err(|_| self.invalid())?;
            let questions = if row["form_id"].is_null() {
                vec![]
            } else {
                let form = self
                    .rows("ticket_forms")
                    .iter()
                    .find(|f| f["id"] == row["form_id"])
                    .ok_or_else(|| self.invalid())?;
                serde_json::from_value(form["questions"].clone()).map_err(|_| self.invalid())?
            };
            let name = self.string(row, "name")?;
            if !(0..=0xffffff).contains(&self.number(row, "color")?) {
                return Err(self.invalid());
            }
            let title = self.string(row, "title")?;
            let description = self.string(row, "description")?;
            let welcome = self.string(row, "welcome_text")?;
            let errors = check_panel(
                &PanelDraft {
                    name: &name,
                    title: &title,
                    description: &description,
                    style: PanelStyle::parse(&self.string(row, "style")?).ok_or_else(|| self.invalid())?,
                    topics: &topics,
                    questions: &questions,
                    welcome: &welcome,
                    support_roles: self.string_list(row, "support_roles")?.len(),
                },
                lang,
            );
            if !errors.is_empty() {
                return Err(self.fields(errors));
            }
        }
        Ok(())
    }

    fn moderation_policy(&self) -> Result<(), String> {
        if let Some(row) = self.rows("mod_parity_settings").first() {
            let config: pinchora_core::moderation::policy::Config =
                serde_json::from_value(row["config"].clone()).map_err(|_| self.invalid())?;
            if !config.valid() {
                return Err(self.invalid());
            }
        }
        Ok(())
    }

    fn automod(&self) -> Result<(), String> {
        let lang = self.lang;
        if let Some(row) = self.rows("automod_settings").first() {
            let mut row = row.clone();
            row["observe_until"] = Value::Null;
            row["rules"] = json!(self.rows("automod_rules"));
            let config = serde_json::from_value(row).map_err(|_| self.invalid())?;
            pinchora_core::moderation::automod::validate(&config, lang)?;
        } else if !self.rows("automod_rules").is_empty() {
            return Err(self.invalid());
        }
        Ok(())
    }

    fn verification(&self) -> Result<(), String> {
        let lang = self.lang;
        let config = pinchora_core::moderation::verification::Config {
            verification: self
                .rows("verification")
                .first()
                .map(|v| serde_json::from_value(v.clone()))
                .transpose()
                .map_err(|_| self.invalid())?
                .unwrap_or_default(),
            raid: self
                .rows("raid_settings")
                .first()
                .map(|v| serde_json::from_value(v.clone()))
                .transpose()
                .map_err(|_| self.invalid())?
                .unwrap_or_default(),
        };
        pinchora_core::moderation::verification::validate(&config, lang)?;
        Ok(())
    }

    fn feeds(&self) -> Result<(), String> {
        for row in self.rows("feeds") {
            use pinchora_core::community::feeds::*;
            let kind = Kind::parse(&self.string(row, "kind")?).ok_or_else(|| self.invalid())?;
            parse_source(kind, &self.string(row, "source")?).map_err(|_| self.invalid())?;
            check_message(row["message"].as_str()).map_err(|_| self.invalid())?;
            for field in ["filter_include", "filter_exclude"] {
                let words = self.string_list(row, field)?;
                filter_words(words.iter().map(String::as_str)).map_err(|_| self.invalid())?;
            }
        }
        Ok(())
    }

    fn starboards(&self) -> Result<(), String> {
        for row in self.rows("starboards") {
            use pinchora_core::community::starboard::*;
            let name = self.string(row, "name")?;
            let emoji = self.string(row, "emoji")?;
            if name.trim().is_empty()
                || name.chars().count() > 40
                || emoji.trim().is_empty()
                || emoji.chars().count() > 64
                || !(1..=i64::from(MAX_THRESHOLD)).contains(&self.number(row, "threshold")?)
                || !(0..=0xffffff).contains(&self.number(row, "color")?)
            {
                return Err(self.invalid());
            }
            for key in ["only_channels", "except_channels"] {
                if self.string_list(row, key)?.len() > MAX_SOURCE_CHANNELS {
                    return Err(self.invalid());
                }
            }
            let weights = row["role_weights"].as_object().ok_or_else(|| self.invalid())?;
            if weights.len() > MAX_WEIGHTED_ROLES
                || weights
                    .values()
                    .any(|v| !v.as_i64().is_some_and(|n| (1..=MAX_ROLE_WEIGHT).contains(&n)))
            {
                return Err(self.invalid());
            }
        }
        Ok(())
    }

    fn vote_channels(&self) -> Result<(), String> {
        for row in self.rows("vote_channels") {
            let up = self.string(row, "up_emoji")?;
            let down = self.string(row, "down_emoji")?;
            if up.trim().is_empty() || down.trim().is_empty() || up == down || up.len() > 64 || down.len() > 64 {
                return Err(self.invalid());
            }
        }
        Ok(())
    }

    fn logging(&self) -> Result<(), String> {
        let lang = self.lang;
        for row in self.rows("log_channels") {
            pinchora_core::admin::logging::LogCategory::parse(&self.string(row, "category")?)
                .ok_or_else(|| self.invalid())?;
        }
        for row in self.rows("log_settings") {
            for key in ["ignored_roles", "ignored_channels"] {
                let ids = self
                    .string_list(row, key)?
                    .iter()
                    .map(|id| id.parse::<u64>().map_err(|_| self.invalid()))
                    .collect::<Result<Vec<_>, _>>()?;
                pinchora_core::admin::logging::check_ignored(key, &ids).map_err(|e| e.text(lang))?;
            }
            for event in self.string_list(row, "disabled_events")? {
                pinchora_core::admin::logging::LogEvent::parse(&event).ok_or_else(|| self.invalid())?;
            }
        }
        Ok(())
    }

    fn role_panels(&self) -> Result<(), String> {
        for row in self.rows("role_panels") {
            let title = self.string(row, "title")?;
            if title.trim().is_empty()
                || title.chars().count() > 80
                || row["description"].as_str().is_some_and(|d| d.chars().count() > 500)
            {
                return Err(self.invalid());
            }
            pinchora_core::roles::automation::PanelStyle::parse(&self.string(row, "style")?)
                .ok_or_else(|| self.invalid())?;
            if self
                .rows("role_panel_roles")
                .iter()
                .filter(|r| r["panel_id"] == row["id"])
                .count()
                > pinchora_core::roles::automation::MAX_PANEL_ENTRIES
            {
                return Err(self.invalid());
            }
        }
        Ok(())
    }

    fn role_panel_entries(&self) -> Result<(), String> {
        for row in self.rows("role_panel_roles") {
            if !self.rows("role_panels").iter().any(|p| p["id"] == row["panel_id"]) {
                return Err(self.invalid());
            }
            let label = self.string(row, "label")?;
            if label.trim().is_empty()
                || label.chars().count() > 80
                || row["description"].as_str().is_some_and(|d| d.chars().count() > 100)
                || row["emoji"].as_str().is_some_and(|e| crate::check_emoji(e).is_none())
            {
                return Err(self.invalid());
            }
        }
        Ok(())
    }

    fn panel_reactions(&self) -> Result<(), String> {
        for row in self.rows("panel_reactions") {
            if !self
                .rows("role_panel_roles")
                .iter()
                .any(|p| p["panel_id"] == row["panel_id"] && p["role_id"] == row["role_id"])
            {
                return Err(self.invalid());
            }
        }
        Ok(())
    }

    fn counter_channels(&self) -> Result<(), String> {
        let lang = self.lang;
        for row in self.rows("counter_channels") {
            let kind = pinchora_core::admin::server_stats::CounterKind::parse(&self.string(row, "kind")?)
                .ok_or_else(|| self.invalid())?;
            if kind.needs_role() && row["role_id"].is_null() {
                return Err(self.invalid());
            }
            pinchora_core::admin::server_stats::check_template(&self.string(row, "template")?)
                .map_err(|e| e.text(lang))?;
        }
        Ok(())
    }

    fn voice_creators(&self) -> Result<(), String> {
        let lang = self.lang;
        for row in self.rows("voice_creators") {
            pinchora_core::admin::temp_voice::check_template(&self.string(row, "name_template")?)
                .map_err(|e| e.text(lang))?;
            pinchora_core::admin::temp_voice::check_limit(self.number(row, "default_limit")?)
                .map_err(|e| e.text(lang))?;
            pinchora_core::admin::temp_voice::check_per_user(self.number(row, "max_per_user")?)
                .map_err(|e| e.text(lang))?;
        }
        Ok(())
    }

    fn custom_responses(&self) -> Result<(), String> {
        for table in ["custom_commands", "custom_responders"] {
            for row in self.rows(table) {
                let mut row = row.clone();
                row.as_object_mut().unwrap().remove("id");
                if table == "custom_commands" {
                    let def: pinchora_core::community::tags::CommandDef =
                        serde_json::from_value(row).map_err(|_| self.invalid())?;
                    let clean = def
                        .clone()
                        .clean(
                            &crate::admin::command_control::control_validation_catalog()
                                .into_iter()
                                .map(|c| c.path)
                                .collect::<Vec<_>>(),
                        )
                        .map_err(|errors| self.fields(errors))?;
                    if clean != def {
                        return Err(self.invalid());
                    }
                } else {
                    let def: pinchora_core::community::tags::ResponderDef =
                        serde_json::from_value(row).map_err(|_| self.invalid())?;
                    if def.clone().clean().map_err(|errors| self.fields(errors))? != def {
                        return Err(self.invalid());
                    }
                }
            }
        }
        Ok(())
    }
}
