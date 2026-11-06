//! `/config modules`: switch parts of Pinchora on or off for the whole server, or limit them to some
//! channels. The same rules and the same write as the dashboard's modules page.

use pinchora_core::modules::{
    ChannelMode, Group, MAX_CHANNELS, Module, ModuleRule, Scope, check_channels, edit_channels, summary,
};
use pinchora_db::{AuditSource, NewAudit};
use serde_json::Value;
use twilight_interactions::command::{CommandModel, CommandOption, CreateCommand, CreateOption};
use twilight_model::{application::interaction::InteractionChannel, channel::message::MessageFlags};
use twilight_util::builder::embed::{EmbedBuilder, EmbedFieldBuilder, EmbedFooterBuilder};

use crate::{App, Cx, util::BRAND};

#[derive(CommandModel, CreateCommand)]
#[command(
    name = "modules",
    desc = "Switch parts of Pinchora on or off, for the whole server or per channel"
)]
#[allow(clippy::large_enum_variant)] // parsed once per interaction, never stored
pub enum ConfigModules {
    #[command(name = "show")]
    Show(ModulesShow),
    #[command(name = "toggle")]
    Toggle(ModulesToggle),
    #[command(name = "channels")]
    Channels(ModulesChannels),
}

#[derive(CommandModel, CreateCommand)]
#[command(name = "show", desc = "Which parts of Pinchora are on, and where")]
pub struct ModulesShow;

#[derive(CommandOption, CreateOption, Clone, Copy)]
pub enum ModuleChoice {
    #[option(name = "Leveling", value = "leveling")]
    Leveling,
    #[option(name = "Chat points", value = "points")]
    Points,
    #[option(name = "Welcome cards", value = "welcome")]
    Welcome,
    #[option(name = "Goodbye messages", value = "goodbye")]
    Goodbye,
    #[option(name = "Server log", value = "log")]
    Log,
    #[option(name = "Role panels", value = "role_panels")]
    RolePanels,
    #[option(name = "Points store", value = "store")]
    Store,
    #[option(name = "Suggestions", value = "suggestions")]
    Suggestions,
    #[option(name = "Polls", value = "polls")]
    Polls,
    #[option(name = "Giveaways", value = "giveaways")]
    Giveaways,
    #[option(name = "Profiles and ranks", value = "profile")]
    Profile,
    #[option(name = "Shell economy", value = "economy")]
    Economy,
    #[option(name = "Shops and gifts", value = "shop")]
    Shop,
    #[option(name = "Daily quests", value = "quests")]
    Quests,
    #[option(name = "Friends and hugs", value = "social")]
    Social,
    #[option(name = "Houses", value = "house")]
    House,
    #[option(name = "Plaza", value = "plaza")]
    Plaza,
    #[option(name = "Pets", value = "pets")]
    Pets,
    #[option(name = "Fishing", value = "fishing")]
    Fishing,
    #[option(name = "Duels", value = "duels")]
    Duels,
    #[option(name = "Word and trivia games", value = "puzzles")]
    Puzzles,
    #[option(name = "Party games", value = "party")]
    Party,
    #[option(name = "Critter drops and beachcombing", value = "collection")]
    Collection,
    #[option(name = "Gambling-style games", value = "gambling")]
    Gambling,
}

impl ModuleChoice {
    fn module(self) -> Module {
        match self {
            Self::Leveling => Module::Leveling,
            Self::Points => Module::Points,
            Self::Welcome => Module::Welcome,
            Self::Goodbye => Module::Goodbye,
            Self::Log => Module::Log,
            Self::RolePanels => Module::RolePanels,
            Self::Store => Module::Store,
            Self::Suggestions => Module::Suggestions,
            Self::Polls => Module::Polls,
            Self::Giveaways => Module::Giveaways,
            Self::Profile => Module::Profile,
            Self::Economy => Module::Economy,
            Self::Shop => Module::Shop,
            Self::Quests => Module::Quests,
            Self::Social => Module::Social,
            Self::House => Module::House,
            Self::Plaza => Module::Plaza,
            Self::Pets => Module::Pets,
            Self::Fishing => Module::Fishing,
            Self::Duels => Module::Duels,
            Self::Puzzles => Module::Puzzles,
            Self::Party => Module::Party,
            Self::Collection => Module::Collection,
            Self::Gambling => Module::Gambling,
        }
    }
}

#[derive(CommandModel, CreateCommand)]
#[command(name = "toggle", desc = "Switch a part of Pinchora on or off for the whole server")]
pub struct ModulesToggle {
    /// The part to switch
    module: ModuleChoice,
    /// On, or off for everyone
    enabled: bool,
}

#[derive(CommandOption, CreateOption, Clone, Copy)]
pub enum WhereChoice {
    #[option(name = "All channels", value = "all")]
    All,
    #[option(name = "Only in these channels", value = "only")]
    Only,
    #[option(name = "Everywhere except these channels", value = "except")]
    Except,
}

impl WhereChoice {
    fn mode(self) -> ChannelMode {
        match self {
            Self::All => ChannelMode::All,
            Self::Only => ChannelMode::Only,
            Self::Except => ChannelMode::Except,
        }
    }
}

#[derive(CommandModel, CreateCommand)]
#[command(
    name = "channels",
    desc = "Limit a part of Pinchora to some channels, or keep it out of some, one channel at a time"
)]
pub struct ModulesChannels {
    /// The part to limit
    module: ModuleChoice,
    /// Where it works (a channel or category also covers the threads and channels under it)
    #[command(rename = "where")]
    place: WhereChoice,
    /// The channel to add to the list
    channel: Option<InteractionChannel>,
    /// Take the channel off the list instead of adding it
    remove: Option<bool>,
}

/// One module as a line of a list: "🟢 **Pets** · on", and under it the channels it names.
fn line(module: Module, rule: &ModuleRule) -> String {
    let places = if rule.mode == ChannelMode::All {
        String::new()
    } else {
        let shown: Vec<String> = rule.channels.iter().take(4).map(|id| format!("<#{id}>")).collect();
        let more = rule.channels.len().saturating_sub(shown.len());
        let more = if more > 0 {
            format!(" and {more} more")
        } else {
            String::new()
        };
        format!("\n> {}{more}", shown.join(" "))
    };
    let mark = if rule.enabled { "🟢" } else { "⚫" };
    format!("{mark} **{}** · {}{places}", module.label(), summary(rule))
}

/// The fields of a rule that differ, as the `before` and `after` objects of an audit row.
pub fn module_diff(before: &ModuleRule, after: &ModuleRule) -> (Value, Value) {
    let (mut old, mut new) = (serde_json::Map::new(), serde_json::Map::new());
    let ids = |rule: &ModuleRule| -> Value { rule.channels.iter().map(u64::to_string).collect::<Vec<_>>().into() };
    if before.enabled != after.enabled {
        old.insert("enabled".into(), before.enabled.into());
        new.insert("enabled".into(), after.enabled.into());
    }
    if before.mode != after.mode {
        old.insert("channel_mode".into(), before.mode.key().into());
        new.insert("channel_mode".into(), after.mode.key().into());
    }
    if before.channels != after.channels {
        old.insert("channels".into(), ids(before));
        new.insert("channels".into(), ids(after));
    }
    (Value::Object(old), Value::Object(new))
}

/// Makes this pod and every other forget the cached rules of a server, so a change counts from the
/// next command or message. A failed announcement is only logged: the caches run out within a minute.
pub async fn announce_guild_change(app: &App, guild: u64) {
    app.forget_guild_settings(guild).await;
    if let Err(error) = app
        .cache
        .publish_invalidation(pinchora_cache::Invalidation::Guild(guild))
        .await
    {
        tracing::warn!(%error, guild, "invalidation not published; the bot catches up within a minute");
    }
}

/// The state of one module.
async fn state_of(app: &App, guild: u64, module: Module) -> anyhow::Result<ModuleRule> {
    Ok(app
        .db
        .module_states(guild)
        .await?
        .into_iter()
        .find(|(m, _)| *m == module)
        .map(|(_, rule)| rule)
        .unwrap_or_default())
}

/// Writes the change, announces it and records it, when it changed anything. `Ok(None)` when the
/// rule already was what was asked.
async fn change(
    cx: &Cx,
    guild: u64,
    module: Module,
    enabled: Option<bool>,
    channels: Option<(ChannelMode, Vec<u64>)>,
) -> anyhow::Result<ModuleRule> {
    let app = &cx.app;
    let before = state_of(app, guild, module).await?;
    let after = app
        .db
        .set_module(
            guild,
            module,
            enabled,
            channels.as_ref().map(|(mode, list)| (*mode, list.as_slice())),
        )
        .await?;
    if after != before {
        announce_guild_change(app, guild).await;
        let (old, new) = module_diff(&before, &after);
        let actor = cx.author()?;
        let (old, new) = (old.to_string(), new.to_string());
        if let Err(error) = app
            .db
            .audit(&NewAudit {
                guild,
                actor: actor.id.get(),
                actor_name: &cx.author_display_name(),
                source: AuditSource::Discord,
                action: "module.set",
                target: Some(module.key()),
                before: Some(&old),
                after: Some(&new),
            })
            .await
        {
            tracing::error!(error = ?error, guild, "a module change was made but not written to the audit log");
        }
    }
    Ok(after)
}

impl ConfigModules {
    pub async fn run(self, cx: &Cx, guild: u64) -> anyhow::Result<()> {
        match self {
            Self::Show(_) => show(cx, guild).await,
            Self::Toggle(c) => {
                let module = c.module.module();
                let rule = change(cx, guild, module, Some(c.enabled), None).await?;
                cx.reply_ephemeral(line(module, &rule)).await
            }
            Self::Channels(c) => channels(cx, guild, c).await,
        }
    }
}

async fn channels(cx: &Cx, guild: u64, c: ModulesChannels) -> anyhow::Result<()> {
    let module = c.module.module();
    let now = state_of(&cx.app, guild, module).await?;
    let edited = edit_channels(
        &now,
        c.place.mode(),
        c.channel.map(|ch| ch.id.get()),
        c.remove == Some(true),
    )
    .and_then(|(mode, list)| check_channels(module, mode, &list).map(|list| (mode, list)));
    let (mode, list) = match edited {
        Ok(edit) => edit,
        Err(error) => return cx.reply_ephemeral(error.message).await,
    };
    let rule = change(cx, guild, module, None, Some((mode, list))).await?;
    let mut text = line(module, &rule);
    if module.scope() == Scope::Channels && rule.mode != ChannelMode::All {
        text.push_str(&format!(
            "\nUp to {MAX_CHANNELS} channels; a category covers the channels and threads under it."
        ));
    }
    cx.reply_ephemeral(text).await
}

async fn show(cx: &Cx, guild: u64) -> anyhow::Result<()> {
    let states = cx.app.db.module_states(guild).await?;
    let mut embed = EmbedBuilder::new()
        .color(BRAND)
        .title("Modules")
        .description("Which parts of Pinchora this server uses. `/config`, `/help` and `/ping` always work.");
    for group in Group::ALL {
        let lines: Vec<String> = states
            .iter()
            .filter(|(m, _)| m.group() == group)
            .map(|(m, rule)| line(*m, rule))
            .collect();
        embed = embed.field(EmbedFieldBuilder::new(group.label(), lines.join("\n")));
    }
    let embed = embed
        .footer(EmbedFooterBuilder::new(
            "Change them with /config modules toggle and channels, or on the dashboard.",
        ))
        .build();
    cx.reply(twilight_model::http::interaction::InteractionResponseData {
        embeds: Some(vec![embed]),
        flags: Some(MessageFlags::EPHEMERAL),
        ..Default::default()
    })
    .await
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use twilight_model::application::command::{CommandOptionChoiceValue, CommandOptionType};

    use super::*;

    /// The choices of `config modules toggle module`, as registered with Discord.
    fn choices() -> Vec<(String, String)> {
        let config = crate::command_definitions()
            .into_iter()
            .find(|c| c.name == "config")
            .expect("/config is registered");
        let modules = config
            .options
            .iter()
            .find(|o| o.name == "modules")
            .expect("/config modules");
        let toggle = modules
            .options
            .as_deref()
            .unwrap_or_default()
            .iter()
            .find(|o| o.name == "toggle")
            .expect("toggle");
        let option = toggle
            .options
            .as_deref()
            .unwrap_or_default()
            .iter()
            .find(|o| o.name == "module")
            .expect("module option");
        assert_eq!(option.kind, CommandOptionType::String);
        option
            .choices
            .as_deref()
            .unwrap_or_default()
            .iter()
            .map(|choice| match &choice.value {
                CommandOptionChoiceValue::String(v) => (choice.name.clone(), v.clone()),
                other => panic!("a choice that is not text: {other:?}"),
            })
            .collect()
    }

    /// Discord's choice list is the module list: one choice per module, same order, same words.
    #[test]
    fn the_slash_choices_are_the_modules() {
        let found = choices();
        let wanted: Vec<(String, String)> = Module::ALL
            .iter()
            .map(|m| (m.label().to_owned(), m.key().to_owned()))
            .collect();
        assert_eq!(found, wanted);
        assert!(
            found.len() <= 25,
            "Discord shows 25 choices at most: switch to autocomplete"
        );
    }

    #[test]
    fn the_audit_row_holds_only_what_changed() {
        let before = ModuleRule::default();
        let after = ModuleRule {
            enabled: true,
            mode: ChannelMode::Only,
            channels: vec![7, 9],
        };
        let (old, new) = module_diff(&before, &after);
        assert_eq!(old, json!({ "channel_mode": "all", "channels": [] }));
        assert_eq!(new, json!({ "channel_mode": "only", "channels": ["7", "9"] }));
        let off = ModuleRule {
            enabled: false,
            ..after.clone()
        };
        assert_eq!(
            module_diff(&after, &off),
            (json!({ "enabled": true }), json!({ "enabled": false }))
        );
    }

    #[test]
    fn lists_name_the_channels() {
        let rule = ModuleRule {
            enabled: true,
            mode: ChannelMode::Except,
            channels: (1..=6).collect(),
        };
        let text = line(Module::Pets, &rule);
        assert!(
            text.contains("**Pets**") && text.contains("<#1>") && text.contains("and 2 more"),
            "{text}"
        );
        assert!(line(Module::Pets, &ModuleRule::default()).ends_with("· on"));
        assert!(summary(&rule).contains("off in 6 channels"));
    }
}
