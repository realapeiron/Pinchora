use super::*;
use pinchora_core::admin::settings;

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

pub(super) async fn check_grants(app: &App, guild: u64, actor: u64, backup: &Backup, lang: Lang) -> anyhow::Result<()> {
    if let Some(row) = backup.sections.get("mod_parity_settings").and_then(|rows| rows.first()) {
        let config = serde_json::from_value(row["config"].clone())?;
        crate::check_moderation_policy_config(app, guild, actor, &config).await?;
    }
    let grants = collect_grants(backup, lang)?;
    let channels = app.http.guild_channels(Id::new(guild)).await?.models().await?;
    check_message_channels(backup, lang, &channels)?;
    check_feature_channels(backup, lang, &channels)?;
    if grants.is_empty() {
        return Ok(());
    }
    check_role_grants(app, guild, actor, lang, grants).await
}
fn collect_grants(backup: &Backup, lang: Lang) -> anyhow::Result<Vec<(u64, bool)>> {
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
    Ok(grants)
}
fn check_message_channels(
    backup: &Backup,
    lang: Lang,
    channels: &[twilight_model::channel::Channel],
) -> anyhow::Result<()> {
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
    Ok(())
}
fn check_feature_channels(
    backup: &Backup,
    lang: Lang,
    channels: &[twilight_model::channel::Channel],
) -> anyhow::Result<()> {
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
    Ok(())
}
async fn check_role_grants(
    app: &App,
    guild: u64,
    actor: u64,
    lang: Lang,
    grants: Vec<(u64, bool)>,
) -> anyhow::Result<()> {
    use settings::{Grantor, RoleFacts};
    use twilight_model::guild::Permissions;
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
