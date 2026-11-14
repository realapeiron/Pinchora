use super::*;

impl ColorSetup {
    pub(super) async fn run(self, cx: &Cx, guild: u64) -> anyhow::Result<()> {
        let lang = cx.lang();
        let db = &cx.app.db;
        let mut settings = db.color_settings(guild).await?;
        let mut notes: Vec<String> = Vec::new();
        if let Some(on) = self.enabled {
            settings.enabled = on;
        }
        if let Some(mode) = self.mode {
            settings.mode = mode.key().to_owned();
        }
        if let Some(role) = self.allow {
            let id = role.get() as i64;
            if !settings.allowed_roles.contains(&id) {
                if settings.allowed_roles.len() >= MAX_COLOR_ALLOWED {
                    return cx
                        .reply_ephemeral(t!(lang, "roles.color.allowed-full", max = MAX_COLOR_ALLOWED))
                        .await;
                }
                settings.allowed_roles.push(id);
            }
        }
        if let Some(role) = self.disallow {
            settings.allowed_roles.retain(|r| *r != role.get() as i64);
        }
        if let Some(anchor) = &self.anchor {
            if let Some(why) = roles::unassignable(cx, guild, anchor).await {
                return cx
                    .reply_ephemeral(give::configure_refusal(lang, anchor.id.get(), &why))
                    .await;
            }
            settings.anchor_role_id = Some(anchor.id.get() as i64);
        }
        if let Some(text) = self.roles.as_deref() {
            adopt(cx, guild, text, &mut notes).await?;
        }
        db.save_color_settings(guild, &settings).await?;
        let count = db.color_roles(guild).await?.len();
        let state = if settings.enabled {
            t!(lang, "roles.color.on")
        } else {
            t!(lang, "roles.color.off-word")
        };
        let mode = match settings.mode.as_str() {
            "hex" => t!(lang, "roles.color.mode-hex"),
            _ => t!(lang, "roles.color.mode-palette"),
        };
        let anchor = settings
            .anchor_role_id
            .map_or_else(|| t!(lang, "roles.color.anchor-none"), |r| format!("<@&{r}>"));
        let who = if settings.allowed_roles.is_empty() {
            t!(lang, "roles.color.allowed-everyone")
        } else {
            settings
                .allowed_roles
                .iter()
                .map(|r| format!("<@&{r}>"))
                .collect::<Vec<_>>()
                .join(", ")
        };
        let mut text = t!(
            lang,
            "roles.color.summary",
            state = state,
            mode = mode,
            anchor = anchor,
            who = who,
            count = count,
            max = MAX_COLOR_ROLES
        );
        for note in notes {
            text.push('\n');
            text.push_str(&note);
        }
        cx.reply_ephemeral(text).await
    }
}

/// Takes the roles mentioned in `text` as colour roles: each keeps its own colour and name.
async fn adopt(cx: &Cx, guild: u64, text: &str, notes: &mut Vec<String>) -> anyhow::Result<()> {
    let lang = cx.lang();
    let ids = role_ids(text, SETUP_ROLES);
    if ids.is_empty() {
        notes.push(t!(lang, "roles.color.no-roles-read"));
        return Ok(());
    }
    let Some((all, _)) = bot_view(&cx.app, guild).await else {
        notes.push(t!(lang, "roles.refusal.unreachable"));
        return Ok(());
    };
    for id in ids {
        let label = format!("<@&{id}>");
        let Some(role) = all.iter().find(|r| r.id.get() == id) else {
            notes.push(t!(lang, "roles.color.unknown-role", role = id));
            continue;
        };
        if let Some(why) = roles::unassignable(cx, guild, role).await {
            notes.push(give::configure_refusal(lang, id, &why));
            continue;
        }
        let rgb = role.colors.primary_color;
        if rgb == 0 {
            notes.push(t!(lang, "roles.color.role-has-no-colour", role = label));
            continue;
        }
        let added = cx
            .app
            .db
            .add_color_role(guild, id, &hex_of(rgb), &role.name, MAX_COLOR_ROLES as i64)
            .await?;
        notes.push(match added {
            ColorAdded::Added | ColorAdded::Updated => t!(lang, "roles.color.adopted", role = label, hex = hex_of(rgb)),
            ColorAdded::Full => t!(lang, "roles.color.full", max = MAX_COLOR_ROLES),
            ColorAdded::HexTaken => t!(lang, "roles.color.hex-taken", role = label, hex = hex_of(rgb)),
        });
    }
    Ok(())
}

/// The role ids mentioned (`<@&123>`) or written as bare numbers in `text`, without repeats, at most `max`.
pub(super) fn role_ids(text: &str, max: usize) -> Vec<u64> {
    let mut found: Vec<u64> = Vec::new();
    let mut digits = String::new();
    for c in text.chars().chain(std::iter::once(' ')) {
        if c.is_ascii_digit() {
            digits.push(c);
            continue;
        }
        if digits.len() >= 15
            && let Ok(id) = digits.parse::<u64>()
            && id > 0
            && !found.contains(&id)
        {
            found.push(id);
        }
        digits.clear();
    }
    found.truncate(max);
    found
}

