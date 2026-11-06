//! `/config xp`: multipliers and no-XP channels and roles, and voice XP.

use pinchora_core::settings;
use pinchora_db::{GuildSettings, MAX_XP_RULES, XpTarget};
use twilight_interactions::command::{CommandModel, CreateCommand};
use twilight_model::{
    application::interaction::InteractionChannel,
    channel::message::MessageFlags,
    http::interaction::InteractionResponseData,
    id::{Id, marker::RoleMarker},
};
use twilight_util::builder::embed::{EmbedBuilder, EmbedFieldBuilder, EmbedFooterBuilder};

use crate::{Cx, util::BRAND};

#[derive(CommandModel, CreateCommand)]
#[command(name = "xp", desc = "XP multipliers, no-XP channels and roles, and voice XP")]
#[allow(clippy::large_enum_variant)] // parsed once per interaction, never stored
pub enum ConfigXp {
    #[command(name = "multiplier")]
    Multiplier(XpMultiplier),
    #[command(name = "clear")]
    Clear(XpClear),
    #[command(name = "show")]
    Show(XpShow),
    #[command(name = "voice")]
    Voice(XpVoice),
}

#[derive(CommandModel, CreateCommand)]
#[command(name = "multiplier", desc = "Scale the XP and points a channel or role earns")]
pub struct XpMultiplier {
    /// 1 is normal, 2 doubles it, 0 earns no XP at all
    #[command(min_value = 0.0, max_value = 5.0)]
    value: f64,
    /// The channel or category to change, text, voice or forum (pick this or a role)
    channel: Option<InteractionChannel>,
    /// The role to change; @everyone sets it for the whole server (pick this or a channel)
    role: Option<Id<RoleMarker>>,
}

#[derive(CommandModel, CreateCommand)]
#[command(name = "clear", desc = "Remove one multiplier, or all of them")]
pub struct XpClear {
    /// The channel or category to reset
    channel: Option<InteractionChannel>,
    /// The role to reset
    role: Option<Id<RoleMarker>>,
}

#[derive(CommandModel, CreateCommand)]
#[command(name = "show", desc = "Show the multipliers and voice XP settings")]
pub struct XpShow;

#[derive(CommandModel, CreateCommand)]
#[command(name = "voice", desc = "Give XP for time spent in voice channels")]
pub struct XpVoice {
    /// Members in a voice channel with someone else earn server XP every minute
    enabled: Option<bool>,
    /// Server XP a minute in voice earns
    #[command(min_value = 1, max_value = 100)]
    amount: Option<i64>,
}

/// What a channel or role is called in a reply.
fn subject(target: XpTarget, id: u64) -> String {
    match target {
        XpTarget::Channel => format!("<#{id}>"),
        // The server's own id is its @everyone role, which renders as such.
        XpTarget::Role => format!("<@&{id}>"),
    }
}

/// One line of a multiplier list.
fn rule_line(target: XpTarget, id: u64, multiplier: f64) -> String {
    if multiplier == 0.0 {
        format!("{} · **no XP**", subject(target, id))
    } else {
        format!("{} · **×{multiplier}**", subject(target, id))
    }
}

/// How voice XP is set, for `/config xp` and `/config show`.
pub(crate) fn voice_summary(s: &GuildSettings) -> String {
    let mut text = format!(
        "**{}** · {} XP a minute with company, not while muted and deafened or in the AFK channel",
        if s.voice_xp_enabled { "on" } else { "off" },
        s.voice_xp_amount
    );
    if s.voice_xp_enabled && !s.leveling_enabled {
        text.push_str("\n> Leveling is off, so nobody earns XP. Turn it on with `/config leveling`.");
    }
    text
}

impl ConfigXp {
    pub async fn run(self, cx: &Cx, guild: u64) -> anyhow::Result<()> {
        match self {
            Self::Multiplier(c) => multiplier(cx, guild, c).await,
            Self::Clear(c) => clear(cx, guild, c).await,
            Self::Show(_) => show(cx, guild).await,
            Self::Voice(c) => voice(cx, guild, c).await,
        }
    }
}

async fn multiplier(cx: &Cx, guild: u64, c: XpMultiplier) -> anyhow::Result<()> {
    let (target, id) = match (&c.channel, c.role) {
        (Some(channel), None) => (XpTarget::Channel, channel.id.get()),
        (None, Some(role)) => (XpTarget::Role, role.get()),
        _ => {
            return cx
                .reply_ephemeral("Pick a channel **or** a role to set the multiplier for.")
                .await;
        }
    };
    let value = match settings::check_multiplier(c.value) {
        Ok(value) => value,
        Err(error) => return cx.reply_ephemeral(error.message).await,
    };
    let who = subject(target, id);
    let text = if value == 1.0 {
        // 1 is what everything earns anyway, so the rule is simply dropped.
        cx.app.db.remove_xp_rule(guild, target, id).await?;
        format!("{who} earns normal XP and points again.")
    } else if !cx.app.db.set_xp_rule(guild, target, id, value).await? {
        format!("A server can have {MAX_XP_RULES} XP rules. Remove one with `/config xp clear` first.")
    } else if value == 0.0 {
        format!("{who} now earns **no XP and no points** here.")
    } else {
        format!("{who} now earns **×{value}** XP and points.")
    };
    cx.app.forget_xp_rules(guild).await;
    cx.reply_ephemeral(text).await
}

async fn clear(cx: &Cx, guild: u64, c: XpClear) -> anyhow::Result<()> {
    let text = match (&c.channel, c.role) {
        (None, None) => match cx.app.db.clear_xp_rules(guild).await? {
            0 => "There were no XP rules to clear.".to_owned(),
            n => format!(
                "Cleared **{n}** XP rule{}. Everything earns normal XP again.",
                if n == 1 { "" } else { "s" }
            ),
        },
        (Some(_), Some(_)) => "Pick a channel **or** a role, not both.".to_owned(),
        (channel, role) => {
            let (target, id) = match (channel, role) {
                (Some(channel), _) => (XpTarget::Channel, channel.id.get()),
                (_, Some(role)) => (XpTarget::Role, role.get()),
                (None, None) => unreachable!("handled above"),
            };
            if cx.app.db.remove_xp_rule(guild, target, id).await? {
                format!("{} earns normal XP and points again.", subject(target, id))
            } else {
                format!("{} had no multiplier.", subject(target, id))
            }
        }
    };
    cx.app.forget_xp_rules(guild).await;
    cx.reply_ephemeral(text).await
}

async fn show(cx: &Cx, guild: u64) -> anyhow::Result<()> {
    let rules = cx.app.db.xp_rules(guild).await?;
    let settings = cx.app.db.guild_settings(guild).await?;
    let list = |target: XpTarget, rules: &std::collections::HashMap<u64, f64>| {
        let mut rows: Vec<(u64, f64)> = rules.iter().map(|(id, m)| (*id, *m)).collect();
        rows.sort_by_key(|row| row.0);
        rows.iter()
            .map(|(id, m)| rule_line(target, *id, *m))
            .collect::<Vec<_>>()
            .join("\n")
    };
    // The lists go in the description: a field holds 1,024 characters, fifty rules need more.
    let mut sections = Vec::new();
    if !rules.channels.is_empty() {
        sections.push(format!("**Channels**\n{}", list(XpTarget::Channel, &rules.channels)));
    }
    if !rules.roles.is_empty() {
        sections.push(format!("**Roles**\n{}", list(XpTarget::Role, &rules.roles)));
    }
    if sections.is_empty() {
        sections.push(
            "No multipliers yet: every channel and role earns normal XP.\nSet one with `/config xp multiplier`."
                .to_owned(),
        );
    }
    let embed = EmbedBuilder::new()
        .color(BRAND)
        .title("XP rules")
        .description(sections.join("\n\n"))
        .field(EmbedFieldBuilder::new("Voice XP", voice_summary(&settings)))
        .footer(EmbedFooterBuilder::new(format!(
            "{}/{MAX_XP_RULES} rules · a channel's multiplier times the member's highest role multiplier; a no-XP role always wins · a thread or post follows its channel, a channel its category, unless it has its own · server XP and points only, global XP stays normal",
            rules.channels.len() + rules.roles.len()
        )))
        .build();
    cx.reply(InteractionResponseData {
        embeds: Some(vec![embed]),
        flags: Some(MessageFlags::EPHEMERAL),
        ..Default::default()
    })
    .await
}

async fn voice(cx: &Cx, guild: u64, c: XpVoice) -> anyhow::Result<()> {
    let amount = match c.amount.map(settings::check_voice_amount).transpose() {
        Ok(amount) => amount,
        Err(error) => return cx.reply_ephemeral(error.message).await,
    };
    let settings = if c.enabled.is_none() && c.amount.is_none() {
        cx.app.db.guild_settings(guild).await?
    } else {
        cx.app.db.update_voice_xp(guild, c.enabled, amount).await?
    };
    cx.app.forget_guild_settings(guild).await;
    // Other pods' caches lag the switch by minutes; with nobody remembered, none of them pays.
    if c.enabled == Some(false)
        && let Err(error) = crate::voice_xp::forget(&cx.app, guild).await
    {
        tracing::warn!(guild, %error, "voice presence not forgotten");
    }
    let mut text = voice_summary(&settings);
    if c.enabled == Some(true) {
        // Nobody is remembered while voice XP is off, and Discord cannot list who is in voice.
        text.push_str(
            "\n> Members already in a voice channel start earning when they next join, move, mute or unmute.",
        );
    }
    let embed = EmbedBuilder::new()
        .color(BRAND)
        .title("Voice XP")
        .description(text)
        .build();
    cx.reply(InteractionResponseData {
        embeds: Some(vec![embed]),
        flags: Some(MessageFlags::EPHEMERAL),
        ..Default::default()
    })
    .await
}
