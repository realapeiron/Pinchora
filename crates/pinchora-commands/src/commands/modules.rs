//! `/config modules`: switch parts of Pinchora on or off for the whole server, or limit them to some
//! channels. The same rules and the same write as the dashboard's modules page.

use pinchora_core::{
    i18n::Lang,
    modules::{ChannelMode, Group, MAX_CHANNELS, Module, ModuleRule, Scope, check_channels, edit_channels, summary_in},
    t,
};
use pinchora_db::{AuditSource, NewAudit};
use serde_json::Value;
use twilight_interactions::command::{CommandModel, CommandOption, CreateCommand, CreateOption};
use twilight_model::{
    application::{
        command::{CommandOptionChoice, CommandOptionChoiceValue},
        interaction::{
            InteractionChannel,
            application_command::{CommandData, CommandDataOption, CommandOptionValue},
        },
    },
    channel::message::MessageFlags,
};
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

/// How many suggestions Discord takes for one autocomplete.
const MAX_SUGGESTIONS: usize = 25;

/// The module a typed or picked `text` names: its key, or its name in any case. A member of the
/// list is picked by key; typing a name and sending without picking works too.
fn find(lang: Lang, text: &str) -> Option<Module> {
    let text = text.trim();
    Module::parse(text).or_else(|| {
        Module::ALL
            .into_iter()
            .find(|m| m.label().eq_ignore_ascii_case(text) || m.name(lang).to_lowercase() == text.to_lowercase())
    })
}

/// What the `module` option suggests for what has been typed so far: the modules whose name, key or
/// blurb holds it, in list order, at most [`MAX_SUGGESTIONS`] (Discord's limit; the list is longer than that).
fn suggestions(lang: Lang, typed: &str) -> Vec<CommandOptionChoice> {
    let typed = typed.trim().to_lowercase().replace('_', " ");
    Module::ALL
        .into_iter()
        .filter(|m| {
            typed.is_empty()
                || m.name(lang).to_lowercase().contains(&typed)
                || m.label().to_lowercase().contains(&typed)
                || m.key().replace('_', " ").contains(&typed)
                || m.blurb_in(lang).to_lowercase().contains(&typed)
                || m.blurb().to_lowercase().contains(&typed)
        })
        .take(MAX_SUGGESTIONS)
        .map(|m| CommandOptionChoice {
            name: m.name(lang),
            name_localizations: None,
            value: CommandOptionChoiceValue::String(m.key().to_owned()),
        })
        .collect()
}

/// The suggestions for the `module` option of `/config modules toggle` and `channels` while it is
/// being typed; empty for any other option, so a stray autocomplete still gets an answer.
pub(crate) fn autocomplete(lang: Lang, data: &CommandData) -> Vec<CommandOptionChoice> {
    let inner = |options: &[CommandDataOption], name: &str| -> Option<Vec<CommandDataOption>> {
        options.iter().find(|o| o.name == name).and_then(|o| match &o.value {
            CommandOptionValue::SubCommand(inner) | CommandOptionValue::SubCommandGroup(inner) => Some(inner.clone()),
            _ => None,
        })
    };
    if data.name != "config" {
        return Vec::new();
    }
    let Some(group) = inner(&data.options, "modules") else {
        return Vec::new();
    };
    let options = inner(&group, "toggle")
        .or_else(|| inner(&group, "channels"))
        .unwrap_or_default();
    options
        .iter()
        .find_map(|o| match &o.value {
            CommandOptionValue::Focused(typed, _) if o.name == "module" => Some(suggestions(lang, typed)),
            _ => None,
        })
        .unwrap_or_default()
}

/// The module the option named, or the private answer that says there is none.
async fn named(cx: &Cx, text: &str) -> anyhow::Result<Option<Module>> {
    let found = find(cx.lang(), text);
    if found == Some(Module::AntiNuke)
        && let Some(guild) = cx.guild_id()
        && let Err(why) = crate::anti_nuke::authorize_antinuke_module(&cx.app, guild, cx.author()?.id.get()).await
    {
        cx.reply_ephemeral(why.to_string()).await?;
        return Ok(None);
    }
    if found.is_none() {
        cx.reply_ephemeral(t!(cx.lang(), "modules.not-a-module")).await?;
    }
    Ok(found)
}

#[derive(CommandModel, CreateCommand)]
#[command(name = "toggle", desc = "Switch a part of Pinchora on or off for the whole server")]
pub struct ModulesToggle {
    /// The part to switch (start typing its name)
    #[command(autocomplete = true)]
    module: String,
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
    /// The part to limit (start typing its name)
    #[command(autocomplete = true)]
    module: String,
    /// Where it works (a channel or category also covers the threads and channels under it)
    #[command(rename = "where")]
    place: WhereChoice,
    /// The channel to add to the list
    channel: Option<InteractionChannel>,
    /// Take the channel off the list instead of adding it
    remove: Option<bool>,
}

/// One module as a line of a list: "🟢 **Pets** · on", and under it the channels it names.
fn line(lang: Lang, module: Module, rule: &ModuleRule) -> String {
    let places = if rule.mode == ChannelMode::All {
        String::new()
    } else {
        let shown: Vec<String> = rule.channels.iter().take(4).map(|id| format!("<#{id}>")).collect();
        let more = rule.channels.len().saturating_sub(shown.len());
        let more = if more > 0 {
            t!(lang, "modules.more", count = more)
        } else {
            String::new()
        };
        format!("\n> {}{more}", shown.join(" "))
    };
    let mark = if rule.enabled { "🟢" } else { "⚫" };
    format!("{mark} **{}** · {}{places}", module.name(lang), summary_in(lang, rule))
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
    app.announce_guild_change(guild).await;
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
    if module == Module::AntiNuke && enabled == Some(true) && !before.enabled {
        app.cache
            .antinuke_activation(guild, app.now().timestamp_millis(), true)
            .await?;
    }
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
                let Some(module) = named(cx, &c.module).await? else {
                    return Ok(());
                };
                let rule = change(cx, guild, module, Some(c.enabled), None).await?;
                cx.reply_ephemeral(line(cx.lang(), module, &rule)).await
            }
            Self::Channels(c) => channels(cx, guild, c).await,
        }
    }
}

async fn channels(cx: &Cx, guild: u64, c: ModulesChannels) -> anyhow::Result<()> {
    let Some(module) = named(cx, &c.module).await? else {
        return Ok(());
    };
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
        Err(error) => return cx.reply_ephemeral(error.text(cx.lang())).await,
    };
    let rule = change(cx, guild, module, None, Some((mode, list))).await?;
    let mut text = line(cx.lang(), module, &rule);
    if module.scope() == Scope::Channels && rule.mode != ChannelMode::All {
        text.push('\n');
        text.push_str(&t!(cx.lang(), "modules.limit", max = MAX_CHANNELS));
    }
    cx.reply_ephemeral(text).await
}

async fn show(cx: &Cx, guild: u64) -> anyhow::Result<()> {
    let states = cx.app.db.module_states(guild).await?;
    let lang = cx.lang();
    let mut embed = EmbedBuilder::new()
        .color(BRAND)
        .title(t!(lang, "modules.title"))
        .description(t!(lang, "modules.about"));
    for group in Group::ALL {
        let lines: Vec<String> = states
            .iter()
            .filter(|(m, _)| m.group() == group)
            .map(|(m, rule)| line(lang, *m, rule))
            .collect();
        // A group no module belongs to yet has nothing to show (Discord refuses an empty field).
        if lines.is_empty() {
            continue;
        }
        embed = embed.field(EmbedFieldBuilder::new(group.name(lang), lines.join("\n")));
    }
    let embed = embed
        .footer(EmbedFooterBuilder::new(t!(lang, "modules.footer")))
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
    use twilight_model::application::command::CommandOptionType;

    use super::*;

    /// The `module` option of `config modules <sub>`, as registered with Discord.
    fn module_option(sub: &str) -> twilight_model::application::command::CommandOption {
        let config = crate::command_definitions()
            .into_iter()
            .find(|c| c.name == "setup")
            .expect("/setup is registered");
        let modules = config
            .options
            .iter()
            .find(|o| o.name == "server")
            .expect("/setup server");
        let sub = modules
            .options
            .as_deref()
            .unwrap_or_default()
            .iter()
            .find(|o| o.name == format!("modules-{sub}"))
            .expect("subcommand");
        sub.options
            .as_deref()
            .unwrap_or_default()
            .iter()
            .find(|o| o.name == "module")
            .expect("module option")
            .clone()
    }

    fn texts(choices: &[CommandOptionChoice]) -> Vec<(String, String)> {
        choices
            .iter()
            .map(|choice| match &choice.value {
                CommandOptionChoiceValue::String(v) => (choice.name.clone(), v.clone()),
                other => panic!("a choice that is not text: {other:?}"),
            })
            .collect()
    }

    /// The modules outgrew Discord's 25 fixed choices, so the option is typed text with suggestions:
    /// registering both choices and autocomplete is refused by Discord.
    #[test]
    fn the_module_option_is_autocomplete_not_a_fixed_list() {
        for sub in ["toggle", "channels"] {
            let option = module_option(sub);
            assert_eq!(option.kind, CommandOptionType::String, "{sub}");
            assert_eq!(option.autocomplete, Some(true), "{sub}");
            assert!(option.choices.as_deref().unwrap_or_default().is_empty(), "{sub}");
        }
    }

    /// Discord shows 25 suggestions at most, so every module must be found by typing, not by scrolling.
    #[test]
    fn suggestions_stay_within_discords_limit_and_reach_every_module() {
        let first = texts(&suggestions(Lang::En, ""));
        assert_eq!(first.len(), Module::ALL.len().min(MAX_SUGGESTIONS));
        let wanted: Vec<(String, String)> = Module::ALL
            .iter()
            .take(MAX_SUGGESTIONS)
            .map(|m| (m.label().to_owned(), m.key().to_owned()))
            .collect();
        assert_eq!(first, wanted, "list order, same words as the dashboard");
        for module in Module::ALL {
            for typed in [
                module.label().to_owned(),
                module.key().to_owned(),
                module.label().to_uppercase(),
            ] {
                let found = texts(&suggestions(Lang::En, &typed));
                assert!(found.len() <= MAX_SUGGESTIONS, "{typed}");
                assert!(
                    found.iter().any(|(_, key)| key == module.key()),
                    "typing `{typed}` does not suggest {}",
                    module.key()
                );
            }
        }
        for (name, key) in first {
            assert!(name.chars().count() <= 100 && !name.is_empty(), "{name}");
            assert!(find(Lang::En, &key).is_some(), "{key}");
        }
        assert_eq!(
            texts(&suggestions(Lang::En, "chess")),
            [("Board and puzzle games".to_owned(), "board_games".to_owned())],
            "a game finds the module that holds it"
        );
        assert!(suggestions(Lang::En, "zzzz").is_empty());
    }

    #[test]
    fn a_typed_name_is_as_good_as_a_picked_key() {
        for module in Module::ALL {
            assert_eq!(find(Lang::En, module.key()), Some(module));
            assert_eq!(find(Lang::En, module.label()), Some(module));
            assert_eq!(
                find(Lang::En, &format!("  {}  ", module.label().to_uppercase())),
                Some(module)
            );
            assert_eq!(
                find(Lang::Tr, &module.name(Lang::Tr)),
                Some(module),
                "its Turkish name too"
            );
        }
        assert_eq!(find(Lang::En, "pets"), Some(Module::Pets));
        assert_eq!(find(Lang::En, "pet"), None, "half a name is not a module");
        assert_eq!(find(Lang::En, ""), None);
    }

    fn command(options: serde_json::Value) -> CommandData {
        serde_json::from_value(json!({ "id": "1", "name": "config", "type": 1, "options": options }))
            .expect("command data")
    }

    #[test]
    fn only_the_module_option_is_completed() {
        let focused = |sub: &str, name: &str, typed: &str| {
            command(json!([{ "name": "modules", "type": 2, "options": [
                { "name": sub, "type": 1, "options": [
                    { "name": name, "type": 3, "value": typed, "focused": true }
                ] }
            ] }]))
        };
        for sub in ["toggle", "channels"] {
            let found = texts(&autocomplete(Lang::En, &focused(sub, "module", "chess")));
            assert_eq!(found.len(), 1, "{sub}");
            assert_eq!(found[0].1, "board_games");
        }
        assert!(autocomplete(Lang::En, &focused("toggle", "enabled", "chess")).is_empty());
        assert!(autocomplete(Lang::En, &focused("show", "module", "chess")).is_empty());
        let other = command(json!([{ "name": "goodbye", "type": 1, "options": [
            { "name": "module", "type": 3, "value": "chess", "focused": true }
        ] }]));
        assert!(autocomplete(Lang::En, &other).is_empty());
        assert!(autocomplete(Lang::En, &command(json!([]))).is_empty());
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
        let text = line(Lang::En, Module::Pets, &rule);
        assert!(
            text.contains("**Pets**") && text.contains("<#1>") && text.contains("and 2 more"),
            "{text}"
        );
        assert!(line(Lang::En, Module::Pets, &ModuleRule::default()).ends_with("· on"));
        assert!(summary_in(Lang::En, &rule).contains("off in 6 channels"));
    }
}
