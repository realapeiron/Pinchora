//! `/config xp`: multipliers and no-XP channels and roles, and voice XP.

use pinchora_core::leveling_parity::{Curve, LevelingParity};
use pinchora_core::{i18n::Lang, settings, t};
use pinchora_db::{GuildSettings, MAX_LEVEL_ROLES, MAX_XP_RULES, XpTarget};
use twilight_interactions::command::{CommandModel, CreateCommand};
use twilight_model::{
    application::interaction::InteractionChannel,
    channel::message::MessageFlags,
    guild::Role,
    http::interaction::InteractionResponseData,
    id::{Id, marker::RoleMarker},
};
use twilight_util::builder::embed::{EmbedBuilder, EmbedFieldBuilder, EmbedFooterBuilder};

use crate::{Cx, roles, util::BRAND};

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
    #[command(name = "curve")]
    Curve(XpCurve),
    #[command(name = "sources")]
    Sources(XpSources),
    #[command(name = "tuning")]
    Tuning(XpTuning),
    #[command(name = "public")]
    Public(XpPublic),
    #[command(name = "voice-role-add")]
    VoiceRoleAdd(XpVoiceRoleAdd),
    #[command(name = "voice-role-remove")]
    VoiceRoleRemove(XpVoiceRoleRemove),
    #[command(name = "voice-role-list")]
    VoiceRoleList(XpVoiceRoleList),
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

#[derive(CommandModel, CreateCommand)]
#[command(name = "curve", desc = "Choose the level XP curve")]
pub struct XpCurve {
    /// classic, linear, mee6 or custom
    kind: String,
    /// Base XP per level
    #[command(min_value = 10, max_value = 100000)]
    base: Option<i64>,
    /// Linear XP coefficient
    #[command(min_value = 0, max_value = 10000)]
    linear: Option<i64>,
    /// Quadratic XP coefficient
    #[command(min_value = 0, max_value = 1000)]
    quadratic: Option<i64>,
}

#[derive(CommandModel, CreateCommand)]
#[command(name = "sources", desc = "Switch XP sources and leave reset")]
pub struct XpSources {
    /// Award XP for messages
    messages: Option<bool>,
    /// Award XP for reactions
    reactions: Option<bool>,
    /// Award XP for eligible voice time
    voice: Option<bool>,
    /// Bonus for long or attached messages
    effort: Option<bool>,
    /// Award XP for votes where supported
    votes: Option<bool>,
    /// Track a separate voice level
    separate_voice_level: Option<bool>,
    /// Reset server XP when a member leaves
    reset_on_leave: Option<bool>,
}

#[derive(CommandModel, CreateCommand)]
#[command(name = "tuning", desc = "Set XP amounts and message cooldown")]
pub struct XpTuning {
    /// Minimum XP per message
    min: Option<i64>,
    /// Maximum XP per message
    max: Option<i64>,
    /// Seconds between XP messages
    cooldown: Option<i64>,
    /// XP per reaction
    reaction: Option<i64>,
    /// Bonus XP for effort
    effort: Option<i64>,
    /// XP per supported vote
    vote: Option<i64>,
}

#[derive(CommandModel, CreateCommand)]
#[command(name = "public", desc = "Set highlights channel and leaderboard URL")]
pub struct XpPublic {
    /// Channel for weekly highlights
    channel: Option<InteractionChannel>,
    /// Lowercase path for the public leaderboard; off to remove
    slug: Option<String>,
}

#[derive(CommandModel, CreateCommand)]
#[command(name = "voice-role-add", desc = "Give a role at a voice level")]
pub struct XpVoiceRoleAdd {
    /// Voice level that earns the role
    #[command(min_value = 1, max_value = 1000)]
    level: i64,
    /// Role to give
    role: Role,
}

#[derive(CommandModel, CreateCommand)]
#[command(name = "voice-role-remove", desc = "Remove a voice level role")]
pub struct XpVoiceRoleRemove {
    /// Role to remove
    role: Id<RoleMarker>,
}

#[derive(CommandModel, CreateCommand)]
#[command(name = "voice-role-list", desc = "List voice level roles")]
pub struct XpVoiceRoleList;

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
            Self::Curve(c) => {
                parity_change(cx, guild, move |p| {
                    p.curve = match c.kind.as_str() {
                        "classic" => Curve::Classic,
                        "mee6" => Curve::Mee6,
                        "linear" => Curve::Linear {
                            base: c.base.unwrap_or(100) as u64,
                            per_level: c.linear.unwrap_or(50) as u64,
                        },
                        "custom" => Curve::Polynomial {
                            base: c.base.unwrap_or(100) as u64,
                            linear: c.linear.unwrap_or(50) as u64,
                            quadratic: c.quadratic.unwrap_or(5) as u64,
                        },
                        _ => return Err("Choose classic, linear, mee6 or custom."),
                    };
                    Ok(())
                })
                .await
            }
            Self::Sources(c) => {
                parity_change(cx, guild, move |p| {
                    if let Some(v) = c.messages {
                        p.messages = v;
                    }
                    if let Some(v) = c.reactions {
                        p.reactions = v;
                    }
                    if let Some(v) = c.voice {
                        p.voice = v;
                    }
                    if let Some(v) = c.effort {
                        p.effort = v;
                    }
                    if let Some(v) = c.votes {
                        p.votes = v;
                    }
                    if let Some(v) = c.separate_voice_level {
                        p.separate_voice_level = v;
                    }
                    if let Some(v) = c.reset_on_leave {
                        p.reset_on_leave = v;
                    }
                    Ok(())
                })
                .await
            }
            Self::Tuning(c) => {
                parity_change(cx, guild, move |p| {
                    if let Some(v) = c.min {
                        p.message_min = v;
                    }
                    if let Some(v) = c.max {
                        p.message_max = v;
                    }
                    if let Some(v) = c.cooldown {
                        p.message_cooldown_secs = v as u64;
                    }
                    if let Some(v) = c.reaction {
                        p.reaction_xp = v;
                    }
                    if let Some(v) = c.effort {
                        p.effort_bonus = v;
                    }
                    if let Some(v) = c.vote {
                        p.vote_xp = v;
                    }
                    Ok(())
                })
                .await
            }
            Self::Public(c) => {
                if c.channel.as_ref().is_some_and(|channel| {
                    !matches!(
                        channel.kind,
                        twilight_model::channel::ChannelType::GuildText
                            | twilight_model::channel::ChannelType::GuildAnnouncement
                    )
                }) {
                    return cx.reply_ephemeral(t!(cx.lang(), "xp.parity-invalid")).await;
                }
                parity_change(cx, guild, move |p| {
                    if let Some(channel) = c.channel {
                        p.highlights_channel_id = Some(channel.id.get().to_string());
                    }
                    if let Some(slug) = c.slug {
                        p.vanity_slug = (slug != "off").then_some(slug);
                    }
                    Ok(())
                })
                .await
            }
            Self::VoiceRoleAdd(c) => {
                let level = match settings::check_level(c.level) {
                    Ok(level) => level,
                    Err(error) => return cx.reply_ephemeral(error.text(cx.lang())).await,
                };
                if let Some(why) = roles::unassignable(cx, guild, &c.role).await {
                    return cx.reply_ephemeral(why).await;
                }
                if !cx.app.db.add_voice_level_role(guild, c.role.id.get(), level).await? {
                    return cx
                        .reply_ephemeral(t!(cx.lang(), "level-roles.full", max = MAX_LEVEL_ROLES))
                        .await;
                }
                cx.reply_ephemeral(t!(
                    cx.lang(),
                    "xp.voice-role-added",
                    level = level,
                    role = c.role.id.get()
                ))
                .await
            }
            Self::VoiceRoleRemove(c) => {
                let removed = cx.app.db.remove_voice_level_role(guild, c.role.get()).await?;
                cx.reply_ephemeral(t!(
                    cx.lang(),
                    if removed {
                        "xp.voice-role-removed"
                    } else {
                        "xp.voice-role-missing"
                    },
                    role = c.role.get()
                ))
                .await
            }
            Self::VoiceRoleList(_) => {
                let rows = cx.app.db.voice_level_roles(guild).await?;
                let text = if rows.is_empty() {
                    t!(cx.lang(), "xp.voice-roles-empty")
                } else {
                    rows.iter()
                        .map(|r| t!(cx.lang(), "xp.voice-role-row", level = r.level, role = r.role_id))
                        .collect::<Vec<_>>()
                        .join("\n")
                };
                cx.reply_ephemeral(text).await
            }
        }
    }
}

async fn parity_change(
    cx: &Cx,
    guild: u64,
    change: impl FnOnce(&mut LevelingParity) -> Result<(), &'static str>,
) -> anyhow::Result<()> {
    let mut policy = cx.app.db.leveling_parity(guild).await?;
    if change(&mut policy).and_then(|()| policy.validate()).is_err() {
        return cx.reply_ephemeral(t!(cx.lang(), "xp.parity-invalid")).await;
    }
    cx.app.db.save_leveling_parity(guild, &policy).await?;
    cx.app.forget_leveling_parity(guild).await;
    cx.reply_ephemeral(t!(cx.lang(), "xp.parity-saved")).await
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
