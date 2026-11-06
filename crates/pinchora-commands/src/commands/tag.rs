//! `/tag`: runs one of the server's custom commands (written with `/customcommand` or on the dashboard).

use std::time::Duration;

use pinchora_core::{
    modules::Module,
    tags::{ARGS_MAX, NAME_MAX, ReplyMode, tidy_name, truncate},
};
use pinchora_db::CommandRow;
use twilight_interactions::command::{CommandModel, CreateCommand};
use twilight_model::{
    application::{
        command::{CommandOptionChoice, CommandOptionChoiceValue},
        interaction::application_command::{CommandData, CommandDataOption, CommandOptionValue},
    },
    channel::message::MessageFlags,
    http::interaction::InteractionResponseData,
};

use crate::{
    Cx,
    custom::{
        Actor, Scene, Spec, all_texts, pick, render_message, run_discord_actions, run_points, scene_vars, to_discord,
    },
    util::strip_mentions,
};

/// How many suggestions Discord takes for one autocomplete.
const MAX_SUGGESTIONS: usize = 25;

#[derive(CommandModel, CreateCommand)]
#[command(name = "tag", desc = "Run one of this server's custom commands", contexts = "guild")]
pub struct Tag {
    /// The command to run (start typing its name)
    #[command(autocomplete = true, max_length = 32)]
    name: String,
    /// Words for the command to use, as {args}, {1}, {2}...
    #[command(max_length = 200)]
    args: Option<String>,
}

impl Tag {
    pub async fn run(self, cx: &Cx) -> anyhow::Result<()> {
        let Some(guild) = cx.guild_id() else {
            return cx.reply_ephemeral("Custom commands live in servers.").await;
        };
        let set = cx.app.tag_set(guild).await?;
        let name = tidy_name(&self.name);
        let Some(row) = set.commands.iter().find(|c| c.def.name == name) else {
            let shown = truncate(&strip_mentions(&name), NAME_MAX).replace('`', "'");
            return cx
                .reply_ephemeral(format!(
                    "There is no custom command called `{shown}`. See them with `/customcommand list`."
                ))
                .await;
        };
        run_command(cx, guild, row, self.args.as_deref().unwrap_or("")).await
    }
}

/// How long a cooldown has left, in the biggest unit that reads well.
fn wait(left: Duration) -> String {
    let seconds = left.as_secs().max(1);
    match seconds {
        0..=89 => format!("{seconds} s"),
        90..=5_399 => format!("{} min", seconds.div_ceil(60)),
        _ => format!("{} h", seconds.div_ceil(3_600)),
    }
}

/// Checks who may run `row` and where, answers with its text, then does what it does besides.
async fn run_command(cx: &Cx, guild: u64, row: &CommandRow, args: &str) -> anyhow::Result<()> {
    let def = &row.def;
    if !def.enabled {
        return cx.reply_ephemeral("That command is switched off.").await;
    }
    let user = cx.author()?;
    let member = cx.interaction.member.as_ref();
    let held: Vec<u64> = member
        .map(|m| m.roles.iter().map(|r| r.get()).collect())
        .unwrap_or_default();
    let channel = cx.interaction.channel.as_ref().map(|c| c.id.get());
    let lineage = match channel {
        Some(channel) => {
            cx.app
                .named_lineage(channel, &[&def.allow_channels, &def.deny_channels])
                .await
        }
        None => Vec::new(),
    };
    if let Err(why) = def.allows(&held, &lineage) {
        return cx.reply_ephemeral(why).await;
    }
    if def.cooldown_secs > 0 {
        let key = format!("tag:{guild}:{}:{}", row.id, user.id);
        let ttl = Duration::from_secs(def.cooldown_secs as u64);
        if let Some(left) = cx.app.cache.try_cooldown(&key, ttl).await? {
            return cx
                .reply_ephemeral(format!(
                    "`/tag {}` is cooling down. Try again in {}.",
                    def.name,
                    wait(left)
                ))
                .await;
        }
    }

    // A private answer must never become a public one: if a lookup below runs long, the gateway would
    // defer publicly, and the answer would then fill in a message everyone sees. Deferring privately first
    // keeps it private whatever it waits for.
    if def.reply_mode == ReplyMode::Ephemeral {
        cx.responder.defer(true).await?;
    }
    // Points are the database's business and quick, so the answer can show the new balance; the rest
    // talks to Discord and follows the answer.
    let balance = run_points(&cx.app, guild, user.id.get(), &def.actions).await;
    let mut scene = Scene::of(guild, channel, user, member.and_then(|m| m.nick.as_deref()));
    scene.args = args.chars().take(ARGS_MAX).collect();
    let spec = Spec {
        responses: &def.responses,
        embed: def.embed.as_ref(),
        allow_mentions: def.allow_mentions,
    };
    let vars = scene_vars(&cx.app, &scene, &all_texts(&spec, &def.actions), balance).await;
    let rendered = render_message(&spec, &vars, &mut pick);
    let (content, embed, mentions) = to_discord(&rendered, guild, user.id.get());
    // Discord refuses a message with nothing in it (`{1}` with no words typed, say).
    let content = if content.is_empty() && embed.is_none() {
        "…".to_owned()
    } else {
        content
    };
    cx.reply(InteractionResponseData {
        content: Some(content).filter(|c| !c.is_empty()),
        embeds: embed.map(|e| vec![e]),
        allowed_mentions: Some(mentions),
        flags: (def.reply_mode == ReplyMode::Ephemeral).then_some(MessageFlags::EPHEMERAL),
        ..Default::default()
    })
    .await?;
    metrics::counter!("pinchora_custom_commands_total").increment(1);

    let actor = Actor {
        guild,
        user: user.id.get(),
        held: &held,
        message: None,
    };
    let notes = run_discord_actions(&cx.app, &actor, &def.actions, &vars).await;
    if !notes.is_empty() {
        cx.reply_ephemeral(notes.join("\n")).await?;
    }
    cx.app.db.count_custom_command_use(row.id).await?;
    Ok(())
}

/// The text of the option being typed into, whichever subcommand it sits under.
fn focused(options: &[CommandDataOption]) -> Option<String> {
    options.iter().find_map(|o| match &o.value {
        CommandOptionValue::Focused(typed, _) if o.name == "name" => Some(typed.clone()),
        CommandOptionValue::SubCommand(inner) | CommandOptionValue::SubCommandGroup(inner) => focused(inner),
        _ => None,
    })
}

/// What the `name` option of `/tag` and of `/customcommand edit|delete|info` suggests while it is typed
/// into: the commands whose name starts with, or holds, what has been typed (their description too),
/// at most 25. Nothing when the module is off or the server has none.
pub(crate) async fn autocomplete(cx: &Cx, data: &CommandData) -> Vec<CommandOptionChoice> {
    let (Some(typed), Some(guild)) = (focused(&data.options), cx.guild_id()) else {
        return Vec::new();
    };
    let channel = cx.interaction.channel.as_ref().map(|c| c.id.get());
    if !cx.app.module_allowed(guild, channel, Module::CustomCommands).await {
        return Vec::new();
    }
    let Ok(set) = cx.app.tag_set(guild).await else {
        return Vec::new();
    };
    // `/tag` only offers what can be run; managing offers everything.
    suggestions(&set.commands, &typed, data.name == "customcommand")
}

fn suggestions(commands: &[CommandRow], typed: &str, with_disabled: bool) -> Vec<CommandOptionChoice> {
    let typed = tidy_name(typed);
    let mut found: Vec<(bool, &CommandRow)> = commands
        .iter()
        .filter(|c| with_disabled || c.def.enabled)
        .filter_map(|c| {
            let starts = c.def.name.starts_with(&typed);
            (typed.is_empty()
                || starts
                || c.def.name.contains(&typed)
                || c.def.description.to_lowercase().contains(&typed))
            .then_some((!starts, c))
        })
        .collect();
    found.sort_by(|a, b| (a.0, &a.1.def.name).cmp(&(b.0, &b.1.def.name)));
    found
        .into_iter()
        .take(MAX_SUGGESTIONS)
        .map(|(_, c)| {
            let mut label = c.def.name.clone();
            if !c.def.description.is_empty() {
                label = format!("{label} — {}", c.def.description);
            }
            if !c.def.enabled {
                label.push_str(" (off)");
            }
            CommandOptionChoice {
                name: truncate(&label, 100),
                name_localizations: None,
                value: CommandOptionChoiceValue::String(c.def.name.clone()),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use chrono::Utc;
    use pinchora_core::tags::CommandDef;

    use super::*;

    fn row(name: &str, description: &str, enabled: bool) -> CommandRow {
        CommandRow {
            id: 1,
            guild_id: 1,
            def: CommandDef {
                name: name.into(),
                description: description.into(),
                responses: vec!["x".into()],
                embed: None,
                reply_mode: ReplyMode::Public,
                actions: Vec::new(),
                allow_roles: Vec::new(),
                deny_roles: Vec::new(),
                allow_channels: Vec::new(),
                deny_channels: Vec::new(),
                cooldown_secs: 0,
                enabled,
                allow_mentions: false,
            },
            uses: 0,
            created_by: 1,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    fn names(found: &[CommandOptionChoice]) -> Vec<String> {
        found
            .iter()
            .map(|c| match &c.value {
                CommandOptionChoiceValue::String(name) => name.clone(),
                other => panic!("not text: {other:?}"),
            })
            .collect()
    }

    #[test]
    fn suggestions_put_prefixes_first_and_cap_at_discords_limit() {
        let commands = vec![
            row("rules", "Server rules", true),
            row("crabrules", "", true),
            row("hello", "Says hi", true),
            row("old", "", false),
        ];
        assert_eq!(
            names(&suggestions(&commands, "", false)),
            ["crabrules", "hello", "rules"]
        );
        assert_eq!(names(&suggestions(&commands, "rul", false)), ["rules", "crabrules"]);
        assert_eq!(
            names(&suggestions(&commands, "says", false)),
            ["hello"],
            "the description counts"
        );
        assert_eq!(names(&suggestions(&commands, "/RUL", false)), ["rules", "crabrules"]);
        assert!(
            names(&suggestions(&commands, "old", false)).is_empty(),
            "a switched-off command is not offered"
        );
        assert_eq!(names(&suggestions(&commands, "old", true)), ["old"]);
        let many: Vec<CommandRow> = (0..40).map(|n| row(&format!("c{n:02}"), "", true)).collect();
        assert_eq!(suggestions(&many, "", false).len(), MAX_SUGGESTIONS);
        let long = row("name", &"d".repeat(100), true);
        assert_eq!(suggestions(&[long], "", false)[0].name.chars().count(), 100);
    }

    #[test]
    fn a_cooldown_is_worded_in_a_sensible_unit() {
        assert_eq!(wait(Duration::from_millis(200)), "1 s");
        assert_eq!(wait(Duration::from_secs(45)), "45 s");
        assert_eq!(wait(Duration::from_secs(120)), "2 min");
        assert_eq!(wait(Duration::from_secs(3_600)), "60 min");
        assert_eq!(wait(Duration::from_secs(7_200)), "2 h");
    }
}
