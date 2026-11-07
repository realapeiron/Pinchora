//! One command policy boundary for slash, text, components and forms.
use crate::Cx;
use pinchora_core::{
    command_control::{Command, Rule},
    t,
};
use std::time::Duration;
use twilight_model::application::{
    command::{CommandOption, CommandOptionType},
    interaction::application_command::{CommandData, CommandOptionValue},
};

pub fn control_catalog() -> Vec<Command> {
    fn walk(out: &mut Vec<Command>, path: String, description: String, permissions: &str, options: &[CommandOption]) {
        let top = path.split(' ').next().unwrap_or_default();
        out.push(Command {
            path: path.clone(),
            description,
            permissions: permissions.into(),
            protected: crate::routing::protected(&path),
            ephemeral_supported: matches!(
                top,
                "balance" | "rank" | "profile" | "info" | "inventory" | "leaderboard" | "stats" | "dex"
            ),
        });
        for option in options.iter().filter(|o| {
            matches!(
                o.kind,
                CommandOptionType::SubCommand | CommandOptionType::SubCommandGroup
            )
        }) {
            walk(
                out,
                format!("{path} {}", option.name),
                option.description.clone(),
                permissions,
                option.options.as_deref().unwrap_or_default(),
            );
        }
    }
    let mut out = vec![];
    for command in crate::command_definitions() {
        walk(
            &mut out,
            command.name,
            command.description,
            &command
                .default_member_permissions
                .unwrap_or_else(twilight_model::guild::Permissions::empty)
                .bits()
                .to_string(),
            &command.options,
        );
    }
    let original = crate::commands::legacy_definitions();
    for command in &mut out {
        let legacy = crate::routing::legacy(&command.path);
        let root = legacy.split(' ').next().unwrap();
        if legacy != command.path {
            if !crate::routing::is_admin_path(&command.path) {
                command.permissions = original
                    .iter()
                    .find(|c| c.name == root)
                    .unwrap()
                    .default_member_permissions
                    .unwrap_or_else(twilight_model::guild::Permissions::empty)
                    .bits()
                    .to_string();
            }
            command.ephemeral_supported = matches!(
                root,
                "balance" | "rank" | "profile" | "info" | "inventory" | "leaderboard" | "stats" | "dex"
            );
        }
    }
    out
}
/// Hidden legacy parents remain valid alias targets: their suffix is resolved after expansion.
pub fn control_validation_catalog() -> Vec<Command> {
    let mut commands = control_catalog();
    for route in crate::routing::MANIFEST.iter() {
        let parts: Vec<_> = route.legacy.split(' ').collect();
        for end in 1..=parts.len() {
            let path = parts[..end].join(" ");
            if !commands.iter().any(|c| c.path == path) {
                commands.push(Command {
                    protected: crate::routing::protected(&path),
                    path,
                    description: "Legacy text shortcut".into(),
                    permissions: "0".into(),
                    ephemeral_supported: false,
                });
            }
        }
    }
    commands
}

pub fn command_reserved_aliases() -> Vec<&'static str> {
    crate::text::parse::ALIASES
        .iter()
        .map(|(alias, _)| *alias)
        .chain(
            crate::routing::MANIFEST
                .iter()
                .map(|r| r.legacy.split(' ').next().unwrap()),
        )
        .collect()
}
pub(crate) fn path(data: &CommandData) -> String {
    let mut path = data.name.clone();
    let mut options = &data.options;
    while let Some(option) = options.first() {
        match &option.value {
            CommandOptionValue::SubCommand(children) | CommandOptionValue::SubCommandGroup(children) => {
                path.push(' ');
                path.push_str(&option.name);
                options = children;
            }
            _ => break,
        }
    }
    path
}
/// Every prefix has one owning command. More specific paths keep subcommand rules in force.
pub(crate) fn owner(prefix: &str) -> Option<&'static str> {
    Some(match prefix {
        "he" => "house edit",
        "hv" => "house view",
        "hu" => "house upgrade",
        "dw" | "df" | "dn" => "house decorate",
        "gb" => "house guestbook",
        "rc" => "house contest",
        "pe" => "plaza edit",
        "pd" => "plaza donate",
        "mk" => "shop market",
        "cb" | "cp" => "shop catalog",
        "iv" => "inventory",
        "sl" => "stall sell",
        "su" => "stall manage",
        "sb" => "stall view",
        "gf" => "gift",
        "pc" => "pet view",
        "pa" => "pet adopt",
        "ps" => "pet list",
        "pn" => "pet list",
        "py" => "pet trip",
        "fr" => "fish cast",
        "fx" => "fish gear",
        "fz" => "fish sell",
        "qc" => "quests",
        "tt" => "duel tictactoe",
        "c4" => "duel connect4",
        "rp" => "duel rps",
        "bs" => "duel seafleet",
        "ck" => "duel checkers",
        "cc" => "duel chess",
        "of" => "duel othello",
        "sh" => "duel share",
        "pz" => "play picross",
        "tz" => "play 2048",
        "hm" => "play hangman",
        "wo" => "play wordle",
        "tv" => "play trivia",
        "mm" => "play memory",
        "mi" => "play minesweeper",
        "yl" => "play higherlower",
        "ag" => "play anagram",
        "yt" => "play typing",
        "gn" => "play guessnumber",
        "dy" => "play derby",
        "sz" => "play shells",
        "hs" => "play heist",
        "ss" => "surge",
        "hl" => "party hotpearl",
        "ac" => "party acronym",
        "cn" => "party postcard",
        "wx" => "party wordrace",
        "wy" => "party wouldyourrather",
        "td" => "party truthordare",
        "ni" => "party nhie",
        "cd" | "mb" => "critterdrop",
        "bc" => "dex",
        "kp" => "crabpot",
        "st" => "store",
        "rr" | "rb" | "rs" => "roles post",
        "cr" => "color pick",
        "pt" => "partner",
        "fd" => "friends list",
        "hg" => "hug",
        "gw" => "giveaway",
        "tg" => "customcommand list",
        "cm" => "customcommand",
        "sg" => "suggest",
        "pl" => "poll",
        "sc" => "stats",
        "tk" | "tp" | "tf" | "tr" => "ticket",
        "am" | "ax" => "automod",
        "vf" | "cq" => "verify",
        "rd" => "raid",
        "sn" => "feeds list",
        "vc" | "vo" => "voice",
        "nf" => "info user",
        "xi" => "xp",
        "sr" => "starboard top",
        "in" => "invites top",
        "tl" => "invites temp",
        "lb" => "leaderboard",
        "ch" | "ca" => "character",
        "cz" | "ti" | "cs" | "sw" => "customize",
        "hp" => "help",
        "pr" => "privacy",
        "rm" => "remind",
        "kc" => "commandcontrol",
        "cf" => "backup",
        "an" | "vp" => "antinuke",
        "bd" => "birthday set",
        "ah" => "achievements",
        "af" => "afk set",
        "sk" => "sticky set",
        "sm" => "schedule add",
        "ev" => "event",
        _ => return None,
    })
}
pub(crate) async fn component_owner(cx: &Cx, custom_id: &str) -> anyhow::Result<Option<&'static str>> {
    let mut parts = custom_id.split(':');
    let prefix = parts.next().unwrap_or_default();
    let action = parts.next().unwrap_or_default();
    let path = match (prefix, action) {
        ("ev", "c") => Some("event create"),
        ("ev", "p") => Some("event attendees"),
        ("fx", "p") => Some("fish shop"),
        ("fz", "b") => Some("fish book"),
        ("sc", "s") => Some("stats server"),
        ("sc", "m") => Some("stats me"),
        ("sc", "c") => Some("stats channel"),
        ("ax", "r") => Some("automod exempt role"),
        ("ax", "c") => Some("automod exempt channel"),
        ("vc", "lk") => Some("voice lock"),
        ("vc", "ul") => Some("voice unlock"),
        ("vc", "lm") => Some("voice limit"),
        ("vc", "rn") => Some("voice rename"),
        ("vc", "kk") | ("vo", "k") => Some("voice kick"),
        ("vc", "tr") | ("vo", "t") => Some("voice transfer"),
        ("vc", "hd") => Some("voice hide"),
        ("vc", "cl") => Some("voice claim"),
        ("cm", "c") => Some("customcommand create"),
        ("cm", "e") => Some("customcommand edit"),
        ("hg", "pat") => Some("pat"),
        ("hg", "wave") => Some("wave"),
        ("hg", "highfive") => Some("highfive"),
        ("pt", "leave" | "stay") => Some("partner leave"),
        ("tp", "cl" | "cy") => Some("ticket close"),
        ("tp", "cm") => Some("ticket claim"),
        ("tp", "tr") => Some("ticket transcript"),
        ("tp", "ro") => Some("ticket reopen"),
        ("xi", "confirm") => {
            let nonce = parts.next().unwrap_or_default();
            let payload = cx
                .app
                .cache
                .get(&format!(
                    "xp:confirm:{}:{}:{nonce}",
                    cx.guild_id().unwrap_or_default(),
                    cx.author()?.id
                ))
                .await?;
            match payload.as_deref().and_then(|p| p.split(':').nth(1)) {
                Some("reset") => Some("xp reset"),
                Some("all") => Some("xp reset-all"),
                Some("go") => Some("xp import mee6"),
                _ => owner(prefix),
            }
        }
        _ => owner(prefix),
    };
    Ok(path)
}

/// Autocomplete must not disclose suggestions for a denied command or consume its cooldown.
pub(crate) async fn autocomplete_allowed(cx: &Cx, data: &CommandData) -> anyhow::Result<bool> {
    use pinchora_core::modules::{Access, Module};
    if crate::routing::is_admin_path(crate::routing::canonical(&path(data)))
        && (cx.guild_id().is_none() || !cx.can_manage_guild())
    {
        return Ok(false);
    }
    let data = crate::routing::adapt(data.clone())?;
    let Some(guild) = cx.guild_id() else { return Ok(true) };
    if data.name == "customcommand" && !cx.can_manage_guild() {
        return Ok(false);
    }
    let path = path(&data);
    if crate::routing::saved_permissions_denied(cx, &path).await? {
        return Ok(false);
    }
    let sub = path.split(' ').nth(1);
    if let Some(module) = Module::of_command(&data.name, sub)
        && cx
            .app
            .module_access(guild, cx.interaction.channel.as_ref().map(|c| c.id.get()), module)
            .await?
            != Access::Allowed
    {
        return Ok(false);
    }
    let config = cx.app.command_control(guild).await?;
    Ok(policy_check(cx, &path, &config).await.is_ok())
}

async fn policy_check(
    cx: &Cx,
    path: &str,
    config: &pinchora_core::command_control::Config,
) -> Result<(), pinchora_core::command_control::Denial> {
    let guild = cx.guild_id().unwrap_or_default();
    let mut roles: Vec<String> = cx
        .interaction
        .member
        .as_ref()
        .map(|m| m.roles.iter().map(ToString::to_string).collect())
        .unwrap_or_default();
    roles.push(guild.to_string());
    let needs_channels = crate::routing::matching(config, path)
        .into_iter()
        .any(|(_, r)| !r.allowed_channels.is_empty() || !r.denied_channels.is_empty());
    let lineage = match cx.interaction.channel.as_ref() {
        Some(channel) if needs_channels => cx
            .app
            .place_lineage(channel.id.get())
            .await
            .into_iter()
            .map(|id| id.to_string())
            .collect(),
        _ => vec![],
    };
    if !crate::routing::protected(path) {
        for (_, rule) in crate::routing::matching(config, path) {
            rule.check(&roles, &lineage)?;
        }
    }
    Ok(())
}

pub(crate) async fn refuses(cx: &Cx, path: &str, component: bool) -> anyhow::Result<bool> {
    let Some(guild) = cx.guild_id() else {
        return Ok(false);
    };
    if !component && crate::routing::refuses_saved_permissions(cx, path).await? {
        return Ok(true);
    }
    if crate::routing::protected(path) {
        return Ok(false);
    }
    let config = cx.app.command_control(guild).await?;
    if let Err(reason) = policy_check(cx, path, &config).await {
        cx.reply_ephemeral(reason.message(cx.lang())).await?;
        return Ok(true);
    }
    // A policy cooldown is additional to every handler’s built-in cooldown.
    {
        let seconds = crate::routing::matching(&config, path)
            .into_iter()
            .map(|(_, r)| r.cooldown_secs)
            .max()
            .unwrap_or(0);
        if seconds > 0 {
            let root = crate::routing::matching(&config, path)
                .into_iter()
                .filter(|(_, r)| r.cooldown_secs == seconds)
                .map(|(p, _)| p.as_str())
                .next()
                .unwrap_or(path);
            if let Some(left) = cx
                .app
                .cache
                .try_cooldown(
                    &format!("command-control:{guild}:{}:{root}", cx.author()?.id),
                    Duration::from_secs(seconds.into()),
                )
                .await?
            {
                cx.reply_ephemeral(t!(
                    cx.lang(),
                    "command_control.cooldown",
                    seconds = left.as_secs().max(1)
                ))
                .await?;
                return Ok(true);
            }
        }
    }
    let mut policy = Rule::default();
    for (_, rule) in crate::routing::matching(&config, path) {
        policy.ephemeral |= rule.ephemeral;
        for (out, value) in [
            (&mut policy.delete_reply_secs, rule.delete_reply_secs),
            (&mut policy.delete_invocation_secs, rule.delete_invocation_secs),
        ] {
            if value > 0 {
                *out = if *out == 0 { value } else { (*out).min(value) };
            }
        }
    }
    cx.responder.set_command_policy(policy);
    Ok(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn every_component_has_an_owner() {
        let source = include_str!("components.rs");
        let mut missing = vec![];
        for s in source.split("Some(\"").skip(1) {
            let prefix = s.split('"').next().unwrap();
            if prefix.len() == 2 && owner(prefix).is_none() {
                missing.push(prefix);
            }
        }
        assert!(missing.is_empty(), "prefixes without owners: {missing:?}");
    }
    #[test]
    fn component_owners_are_real_commands() {
        let catalog = control_validation_catalog();
        let source = include_str!("components.rs");
        let mut invalid = vec![];
        for s in source.split("Some(\"").skip(1) {
            let prefix = s.split('"').next().unwrap();
            if let Some(path) = owner(prefix)
                && !catalog.iter().any(|c| c.path == path)
            {
                invalid.push((prefix, path));
            }
        }
        assert!(invalid.is_empty(), "invalid component owners: {invalid:?}");
    }
    #[test]
    fn quick_game_buttons_follow_subcommand_rules() {
        let catalog = control_catalog();
        for (prefix, path) in [
            ("mm", "play memory"),
            ("mi", "play minesweeper"),
            ("yl", "play higherlower"),
            ("ag", "play anagram"),
            ("yt", "play typing"),
            ("gn", "play guessnumber"),
            ("of", "duel othello"),
            ("ni", "party nhie"),
        ] {
            assert_eq!(owner(prefix), Some(path));
            assert!(catalog.iter().any(|command| command.path == path));
        }
    }

    #[test]
    fn aliases_cannot_shadow_names_or_chain() {
        use pinchora_core::{command_control::Config, i18n::Lang};
        let catalog = control_catalog();
        for (alias, path) in [("play", "play wordle"), ("bal", "balance"), ("w", "unknown")] {
            let mut config = Config::default();
            config.aliases.insert(alias.into(), path.into());
            assert!(
                config
                    .validate(&catalog, &command_reserved_aliases(), Lang::En)
                    .is_err()
            );
        }
        let mut config = Config::default();
        config.aliases.insert("w".into(), "play wordle".into());
        config
            .validate(&catalog, &command_reserved_aliases(), Lang::En)
            .unwrap();
    }
}
