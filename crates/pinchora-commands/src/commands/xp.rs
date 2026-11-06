//! `/config xp`: multipliers and no-XP channels and roles, and voice XP.

use pinchora_core::{i18n::Lang, settings, t};
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
fn rule_line(lang: Lang, target: XpTarget, id: u64, multiplier: f64) -> String {
    if multiplier == 0.0 {
        t!(lang, "xp.no-xp", subject = subject(target, id))
    } else {
        t!(
            lang,
            "xp.multiplied",
            subject = subject(target, id),
            multiplier = multiplier
        )
    }
}

/// How voice XP is set, for `/config xp` and `/config show`.
pub(crate) fn voice_summary(lang: Lang, s: &GuildSettings) -> String {
    let mut text = t!(
        lang,
        "xp.voice-summary",
        state = t!(lang, if s.voice_xp_enabled { "cfg.on" } else { "cfg.off" }),
        amount = s.voice_xp_amount
    );
    if s.voice_xp_enabled && !s.leveling_enabled {
        text.push_str(&t!(lang, "xp.voice-leveling-off"));
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
            return cx.reply_ephemeral(t!(cx.lang(), "xp.pick-one")).await;
        }
    };
    let value = match settings::check_multiplier(c.value) {
        Ok(value) => value,
        Err(error) => return cx.reply_ephemeral(error.text(cx.lang())).await,
    };
    let who = subject(target, id);
    let text = if value == 1.0 {
        // 1 is what everything earns anyway, so the rule is simply dropped.
        cx.app.db.remove_xp_rule(guild, target, id).await?;
        t!(cx.lang(), "xp.normal-again", subject = who)
    } else if !cx.app.db.set_xp_rule(guild, target, id, value).await? {
        t!(cx.lang(), "xp.rules-full", max = MAX_XP_RULES)
    } else if value == 0.0 {
        t!(cx.lang(), "xp.none-earned", subject = who)
    } else {
        t!(cx.lang(), "xp.now-earns", subject = who, multiplier = value)
    };
    cx.app.announce_xp_change(guild).await;
    cx.reply_ephemeral(text).await
}

async fn clear(cx: &Cx, guild: u64, c: XpClear) -> anyhow::Result<()> {
    let text = match (&c.channel, c.role) {
        (None, None) => match cx.app.db.clear_xp_rules(guild).await? {
            0 => t!(cx.lang(), "xp.no-rules"),
            n => t!(cx.lang(), "xp.cleared", count = n),
        },
        (Some(_), Some(_)) => t!(cx.lang(), "xp.pick-not-both"),
        (channel, role) => {
            let (target, id) = match (channel, role) {
                (Some(channel), _) => (XpTarget::Channel, channel.id.get()),
                (_, Some(role)) => (XpTarget::Role, role.get()),
                (None, None) => unreachable!("handled above"),
            };
            if cx.app.db.remove_xp_rule(guild, target, id).await? {
                t!(cx.lang(), "xp.normal-again", subject = subject(target, id))
            } else {
                t!(cx.lang(), "xp.had-none", subject = subject(target, id))
            }
        }
    };
    cx.app.announce_xp_change(guild).await;
    cx.reply_ephemeral(text).await
}

async fn show(cx: &Cx, guild: u64) -> anyhow::Result<()> {
    let rules = cx.app.db.xp_rules(guild).await?;
    let settings = cx.app.db.guild_settings(guild).await?;
    let lang = cx.lang();
    let list = |target: XpTarget, rules: &std::collections::HashMap<u64, f64>| {
        let mut rows: Vec<(u64, f64)> = rules.iter().map(|(id, m)| (*id, *m)).collect();
        rows.sort_by_key(|row| row.0);
        rows.iter()
            .map(|(id, m)| rule_line(lang, target, *id, *m))
            .collect::<Vec<_>>()
            .join("\n")
    };
    // The lists go in the description: a field holds 1,024 characters, fifty rules need more.
    let mut sections = Vec::new();
    if !rules.channels.is_empty() {
        sections.push(t!(lang, "xp.channels", list = list(XpTarget::Channel, &rules.channels)));
    }
    if !rules.roles.is_empty() {
        sections.push(t!(lang, "xp.roles", list = list(XpTarget::Role, &rules.roles)));
    }
    if sections.is_empty() {
        sections.push(t!(lang, "xp.no-multipliers"));
    }
    let embed = EmbedBuilder::new()
        .color(BRAND)
        .title(t!(lang, "xp.rules-title"))
        .description(sections.join("\n\n"))
        .field(EmbedFieldBuilder::new(
            t!(lang, "xp.voice-field"),
            voice_summary(lang, &settings),
        ))
        .footer(EmbedFooterBuilder::new(t!(
            lang,
            "xp.rules-footer",
            count = rules.channels.len() + rules.roles.len(),
            max = MAX_XP_RULES
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
        Err(error) => return cx.reply_ephemeral(error.text(cx.lang())).await,
    };
    let settings = if c.enabled.is_none() && c.amount.is_none() {
        cx.app.db.guild_settings(guild).await?
    } else {
        cx.app.db.update_voice_xp(guild, c.enabled, amount).await?
    };
    cx.app.announce_guild_change(guild).await;
    // Other pods' caches can lag the switch: with nobody remembered, and writes refused until it is switched
    // on again, none of them pays.
    match c.enabled {
        Some(false) => {
            if let Err(error) = crate::voice_xp::switched_off(&cx.app, guild).await {
                tracing::warn!(guild, %error, "voice presence not forgotten");
            }
        }
        Some(true) => {
            if let Err(error) = crate::voice_xp::switched_on(&cx.app, guild).await {
                tracing::warn!(guild, %error, "voice switch not cleared");
            }
        }
        None => {}
    }
    let mut text = voice_summary(cx.lang(), &settings);
    if c.enabled == Some(true) {
        // Nobody is remembered while voice XP is off, and Discord cannot list who is in voice.
        text.push_str(&t!(cx.lang(), "xp.voice-hint"));
    }
    let embed = EmbedBuilder::new()
        .color(BRAND)
        .title(t!(cx.lang(), "xp.voice-title"))
        .description(text)
        .build();
    cx.reply(InteractionResponseData {
        embeds: Some(vec![embed]),
        flags: Some(MessageFlags::EPHEMERAL),
        ..Default::default()
    })
    .await
}
