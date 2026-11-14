use super::*;

pub(super) async fn announce_voice_level(
    app: &Arc<App>,
    settings: &GuildSettings,
    guild: u64,
    user: u64,
    level: u32,
    record: &Record,
) {
    let result = async {
        let rewards: Vec<LevelReward> = app
            .db
            .voice_level_roles(guild)
            .await?
            .into_iter()
            .map(|row| LevelReward {
                level: row.level as u32,
                role_id: row.role_id as u64,
            })
            .collect();
        let member = app
            .http
            .guild_member(Id::new(guild), Id::new(user))
            .await?
            .model()
            .await?;
        let held: Vec<u64> = member.roles.iter().map(|r| r.get()).collect();
        let (given, _) =
            roles::apply_level_roles(app, guild, user, level, &held, &rewards, settings.level_roles_stack).await;
        let role = rewards
            .iter()
            .filter(|row| given.contains(&row.role_id))
            .max_by_key(|row| row.level)
            .map(|row| row.role_id);
        let mode = LevelUpMode::parse(&settings.levelup_mode);
        let here = Id::new(record.channel.max(1));
        let channel = match mode {
            LevelUpMode::Off => return anyhow::Ok(()),
            LevelUpMode::Current => here,
            LevelUpMode::Channel => match settings.levelup_channel_id {
                Some(ch) if app.module_allowed(guild, Some(ch as u64), Module::Leveling).await => Id::new(ch as u64),
                _ => here,
            },
            LevelUpMode::Dm => match leveling::private_channel(app, user).await {
                Some(ch) => ch,
                None => return Ok(()),
            },
        };
        let lang = if mode == LevelUpMode::Dm {
            crate::community::server_events::dm_lang(app, user, guild).await
        } else {
            app.guild_lang(guild).await
        };
        let content = if let Some(role) = role {
            t!(lang, "xp.voice-levelup-role", user = user, level = level, role = role)
        } else {
            t!(lang, "xp.voice-levelup", user = user, level = level)
        };
        let payload = serde_json::json!({"content": content, "allowed_mentions": {"parse": []}});
        app.http
            .create_message(channel)
            .payload_json(&serde_json::to_vec(&payload)?)
            .await?;
        anyhow::Ok(())
    }
    .await;
    if let Err(error) = result {
        tracing::warn!(%error, guild, user, "voice level-up failed");
    }
}
