//! What the custom command and auto-responder routes share: the JSON of a row, the checks of the roles and
//! channels it names against the server, and the filled-in messages of the editors' preview and test panels.

use pinchora_commands::{Rendered, Spec, mention_ids, pick_random, render_message, scrub_value, templates};
use pinchora_core::{
    settings::FieldError,
    tags::{ARGS_MAX, Action, CommandDef, Destination, ResponderDef, TriggerKind, Vars},
};
use pinchora_db::{CommandRow, ResponderRow};
use serde_json::{Value, json};

use crate::{
    error::{ApiError, ApiResult},
    guild_info::{self, GuildInfo},
    state::WebState,
};

// /Rows

/// A command as the dashboard shows it: what the server wrote, plus its id and use count.
pub fn command_json(row: &CommandRow) -> Value {
    let mut value = serde_json::to_value(&row.def).unwrap_or(Value::Null);
    if let Some(object) = value.as_object_mut() {
        object.insert("id".into(), row.id.to_string().into());
        object.insert("uses".into(), row.uses.into());
        object.insert("created_by".into(), row.created_by.to_string().into());
        object.insert("created_at".into(), json!(row.created_at));
        object.insert("updated_at".into(), json!(row.updated_at));
    }
    value
}

/// An auto-responder as the dashboard shows it.
pub fn responder_json(row: &ResponderRow) -> Value {
    let mut value = serde_json::to_value(&row.def).unwrap_or(Value::Null);
    if let Some(object) = value.as_object_mut() {
        object.insert("id".into(), row.id.to_string().into());
        object.insert("created_by".into(), row.created_by.to_string().into());
        object.insert("created_at".into(), json!(row.created_at));
        object.insert("updated_at".into(), json!(row.updated_at));
    }
    value
}

/// The `{id}` of a path, as a row id.
pub fn row_id(params: &std::collections::HashMap<String, String>) -> ApiResult<i64> {
    params
        .get("id")
        .and_then(|raw| raw.parse::<i64>().ok())
        .filter(|id| *id > 0)
        .ok_or_else(|| ApiError::malformed("Command ids are numbers."))
}

// /Roles and channels

/// The roles and channels a command or responder names, by what they are for.
#[derive(Debug, Default)]
pub struct Refs {
    pub action_roles: Vec<u64>,
    pub allow_roles: Vec<u64>,
    pub deny_roles: Vec<u64>,
    pub allow_channels: Vec<u64>,
    pub deny_channels: Vec<u64>,
    pub post_channel: Option<u64>,
    pub watched_role: Option<u64>,
}

fn action_roles(actions: &[Action]) -> Vec<u64> {
    actions.iter().filter_map(Action::role).collect()
}

impl Refs {
    pub fn of_command(def: &CommandDef) -> Self {
        Self {
            action_roles: action_roles(&def.actions),
            allow_roles: def.allow_roles.clone(),
            deny_roles: def.deny_roles.clone(),
            allow_channels: def.allow_channels.clone(),
            deny_channels: def.deny_channels.clone(),
            ..Default::default()
        }
    }

    pub fn of_responder(def: &ResponderDef) -> Self {
        Self {
            action_roles: action_roles(&def.actions),
            allow_channels: def.allow_channels.clone(),
            deny_channels: def.deny_channels.clone(),
            post_channel: def.channel_id.filter(|_| def.destination == Destination::Channel),
            watched_role: def.watched_role(),
            ..Default::default()
        }
    }

    fn roles(&self) -> [(&'static str, Vec<u64>); 3] {
        [
            ("allow_roles", self.allow_roles.clone()),
            ("deny_roles", self.deny_roles.clone()),
            ("trigger_value", self.watched_role.into_iter().collect()),
        ]
    }

    fn channels(&self) -> [(&'static str, Vec<u64>); 2] {
        [
            ("allow_channels", self.allow_channels.clone()),
            ("deny_channels", self.deny_channels.clone()),
        ]
    }

    fn new_since(&self, old: &Self) -> Self {
        let fresh = |now: &[u64], was: &[u64]| now.iter().copied().filter(|id| !was.contains(id)).collect::<Vec<_>>();
        Self {
            action_roles: fresh(&self.action_roles, &old.action_roles),
            allow_roles: fresh(&self.allow_roles, &old.allow_roles),
            deny_roles: fresh(&self.deny_roles, &old.deny_roles),
            allow_channels: fresh(&self.allow_channels, &old.allow_channels),
            deny_channels: fresh(&self.deny_channels, &old.deny_channels),
            post_channel: self.post_channel.filter(|c| Some(*c) != old.post_channel),
            watched_role: self.watched_role.filter(|r| Some(*r) != old.watched_role),
        }
    }

    fn is_empty(&self) -> bool {
        self.action_roles.is_empty()
            && self.allow_roles.is_empty()
            && self.deny_roles.is_empty()
            && self.allow_channels.is_empty()
            && self.deny_channels.is_empty()
            && self.post_channel.is_none()
            && self.watched_role.is_none()
    }
}

/// Refuses (422) a role or channel that is not on the server, a role action the bot cannot do, and a
/// channel the bot cannot post in. Only what is new since `old` is checked: a role that was deleted after
/// it was put on a command must not stop the command from being edited.
pub async fn check_refs(state: &WebState, guild: u64, now: &Refs, old: Option<&Refs>) -> ApiResult<()> {
    let empty = Refs::default();
    let new = now.new_since(old.unwrap_or(&empty));
    if new.is_empty() {
        return Ok(());
    }
    let mut errors: Vec<FieldError> = Vec::new();
    let wants_info = new.roles().iter().any(|(_, ids)| !ids.is_empty())
        || new.channels().iter().any(|(_, ids)| !ids.is_empty())
        || new.post_channel.is_some();
    if wants_info {
        let info = guild_info::get_with(state, guild, |info| {
            new.roles()
                .iter()
                .all(|(_, ids)| ids.iter().all(|id| info.role(*id).is_some()))
                && new
                    .channels()
                    .iter()
                    .all(|(_, ids)| ids.iter().all(|id| info.channel(*id).is_some()))
                && new.post_channel.is_none_or(|id| info.channel(id).is_some())
        })
        .await?;
        for (field, ids) in new.roles() {
            errors.extend(ids.iter().filter_map(|id| info.check_role(field, *id).err()));
        }
        for (field, ids) in new.channels() {
            errors.extend(ids.iter().filter_map(|id| info.check_channel(field, *id).err()));
        }
        if let Some(id) = new.post_channel
            && let Err(error) = info.check_post_channel("channel_id", id)
        {
            errors.push(error);
        }
    }
    if !new.action_roles.is_empty() {
        // The bot's position is read live: the slash commands check it live too.
        let live = guild_info::live(state, guild).await?;
        errors.extend(
            new.action_roles
                .iter()
                .filter_map(|id| live.check_assignable("actions", *id).err()),
        );
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(ApiError::invalid(errors))
    }
}

/// What an import keeps of a command on another server: roles and channels this server lacks are
/// dropped (with a word about it) instead of failing the whole command. Returns what was dropped.
pub fn prune_command(def: &mut CommandDef, info: &GuildInfo) -> Vec<String> {
    let mut dropped = Vec::new();
    let mut keep_roles = |list: &mut Vec<u64>, what: &str| {
        let before = list.len();
        list.retain(|id| info.role(*id).is_some());
        if list.len() < before {
            dropped.push(format!(
                "{} of the {what} are not roles on this server and were left out",
                before - list.len()
            ));
        }
    };
    keep_roles(&mut def.allow_roles, "allowed roles");
    keep_roles(&mut def.deny_roles, "denied roles");
    let mut keep_channels = |list: &mut Vec<u64>, what: &str| {
        let before = list.len();
        list.retain(|id| info.channel(*id).is_some());
        if list.len() < before {
            dropped.push(format!(
                "{} of the {what} are not channels on this server and were left out",
                before - list.len()
            ));
        }
    };
    keep_channels(&mut def.allow_channels, "allowed channels");
    keep_channels(&mut def.deny_channels, "denied channels");
    prune_actions(&mut def.actions, info, &mut dropped);
    dropped
}

/// [`prune_command`] for a responder. A channel or role the responder cannot work without is not
/// dropped: the caller fails the item when it is missing.
pub fn prune_responder(def: &mut ResponderDef, info: &GuildInfo) -> Vec<String> {
    let mut dropped = Vec::new();
    for (list, what) in [
        (&mut def.allow_channels, "allowed channels"),
        (&mut def.deny_channels, "denied channels"),
    ] {
        let before = list.len();
        list.retain(|id| info.channel(*id).is_some());
        if list.len() < before {
            dropped.push(format!(
                "{} of the {what} are not channels on this server and were left out",
                before - list.len()
            ));
        }
    }
    prune_actions(&mut def.actions, info, &mut dropped);
    dropped
}

fn prune_actions(actions: &mut Vec<Action>, info: &GuildInfo, dropped: &mut Vec<String>) {
    let before = actions.len();
    actions.retain(|a| a.role().is_none_or(|id| info.check_assignable("actions", id).is_ok()));
    if actions.len() < before {
        dropped.push(format!(
            "{} role action(s) name a role this server lacks or I can't hand out and were left out",
            before - actions.len()
        ));
    }
}

// /Previews

/// The made-up numbers and names a preview is filled with, and the real ones it can use.
pub struct Sample<'a> {
    pub user: u64,
    pub name: &'a str,
    pub avatar: String,
    pub guild_name: &'a str,
    pub points_name: &'a str,
    pub info: Option<&'a GuildInfo>,
}

impl Sample<'_> {
    /// A text channel to stand in for "the channel it happens in".
    fn channel(&self, preferred: Option<u64>) -> u64 {
        preferred
            .or_else(|| {
                self.info
                    .and_then(|i| i.channels.iter().find(|c| c.takes_posts()))
                    .and_then(|c| c.id.parse().ok())
            })
            .unwrap_or(1)
    }

    /// A role to stand in for "the role": the first the bot can hand out.
    fn role(&self, preferred: Option<u64>) -> u64 {
        preferred
            .or_else(|| {
                self.info
                    .and_then(|i| i.roles.iter().find(|r| r.assignable))
                    .and_then(|r| r.id.parse().ok())
            })
            .unwrap_or(1)
    }

    fn vars(&self, channel: Option<u64>, args: &str) -> Vars {
        let mut vars = Vars::new();
        vars.set("user", format!("<@{}>", self.user))
            .set("user.name", scrub_value(self.name))
            .set("user.id", self.user.to_string())
            .set("user.avatar", self.avatar.clone())
            .set("server", self.guild_name)
            .set("server.members", "1234")
            .set("channel", format!("<#{}>", self.channel(channel)))
            .set("points", "250")
            .set("points.name", self.points_name);
        vars.args(&scrub_value(&args.chars().take(ARGS_MAX).collect::<String>()));
        vars
    }
}

/// What a message would look like, for the editor.
pub struct Preview {
    pub rendered: Rendered,
    pub ephemeral: bool,
    pub notes: Vec<String>,
}

/// A command as `/tag` would answer the requester.
pub fn preview_command(def: &CommandDef, args: &str, sample: &Sample, points_on: bool) -> Preview {
    let spec = Spec {
        responses: &def.responses,
        embed: def.embed.as_ref(),
        allow_mentions: def.allow_mentions,
    };
    let vars = sample.vars(None, args);
    let rendered = render_message(&spec, &vars, &mut pick_random);
    let mut notes = notes_for(
        def.enabled,
        &def.actions,
        points_on,
        &templates(&spec),
        def.allow_mentions,
    );
    if def.reply_mode == pinchora_core::tags::ReplyMode::Ephemeral {
        notes.push("Only the person who runs it sees the answer.".to_owned());
    }
    Preview {
        rendered,
        ephemeral: def.reply_mode == pinchora_core::tags::ReplyMode::Ephemeral,
        notes,
    }
}

/// A responder as it would answer the requester, with made-up numbers where the event brings some.
pub fn preview_responder(def: &ResponderDef, sample: &Sample, points_on: bool) -> Preview {
    let spec = Spec {
        responses: &def.responses,
        embed: def.embed.as_ref(),
        allow_mentions: def.allow_mentions,
    };
    let mut vars = sample.vars(def.channel_id.filter(|_| def.destination == Destination::Channel), "");
    match def.trigger_kind {
        TriggerKind::MemberJoin => {
            vars.set("count", "1234");
        }
        TriggerKind::LevelUp => {
            vars.set("level", def.min_level().max(5).to_string());
            vars.set("role", format!("<@&{}>", sample.role(None)));
        }
        TriggerKind::RoleAdded | TriggerKind::RoleRemoved => {
            vars.set("role", format!("<@&{}>", sample.role(def.watched_role())));
        }
        TriggerKind::MemberLeave | TriggerKind::Keyword => {}
    }
    let rendered = render_message(&spec, &vars, &mut pick_random);
    let notes = notes_for(
        def.enabled,
        &def.actions,
        points_on,
        &templates(&spec),
        def.allow_mentions,
    );
    Preview {
        rendered,
        ephemeral: false,
        notes,
    }
}

fn notes_for(
    enabled: bool,
    actions: &[Action],
    points_on: bool,
    templates: &[&str],
    allow_mentions: bool,
) -> Vec<String> {
    let mut notes = Vec::new();
    if !enabled {
        notes.push("This one is switched off, so nobody gets it until you switch it on.".to_owned());
    }
    if !points_on && actions.iter().any(|a| matches!(a, Action::GivePoints { .. })) {
        notes.push("Server points are off, so give points actions do nothing until you switch points on.".to_owned());
    }
    if !allow_mentions
        && templates.iter().any(|t| {
            let (users, roles) = mention_ids(t);
            !users.is_empty()
                || !roles.is_empty()
                || t.to_lowercase().contains("@everyone")
                || t.to_lowercase().contains("@here")
        })
    {
        notes.push("Mentions in the text are left out unless you allow them.".to_owned());
    }
    notes.push("Numbers and the member are made up; {random} picks again each time.".to_owned());
    notes
}

/// Mentions written the way Discord shows them: `@name`, `@role`, `#channel`.
pub fn readable(text: &str, user: (u64, &str), info: Option<&GuildInfo>) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find("<") {
        out.push_str(&rest[..at]);
        let here = &rest[at..];
        let Some(end) = here.find('>') else {
            out.push_str(here);
            return out;
        };
        let inner = &here[1..end];
        let shown = if let Some(id) = inner.strip_prefix("@&").and_then(|d| d.parse::<u64>().ok()) {
            Some(format!(
                "@{}",
                info.and_then(|i| i.role(id)).map_or("role", |r| r.name.as_str())
            ))
        } else if let Some(id) = inner
            .strip_prefix("@!")
            .or_else(|| inner.strip_prefix('@'))
            .and_then(|d| d.parse::<u64>().ok())
        {
            Some(format!("@{}", if id == user.0 { user.1 } else { "someone" }))
        } else {
            inner.strip_prefix('#').and_then(|d| d.parse::<u64>().ok()).map(|id| {
                format!(
                    "#{}",
                    info.and_then(|i| i.channel(id)).map_or("channel", |c| c.name.as_str())
                )
            })
        };
        match shown {
            Some(shown) => {
                out.push_str(&shown);
                rest = &here[end + 1..];
            }
            None => {
                out.push('<');
                rest = &here[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

/// A preview as the dashboard's JSON: texts with mentions written out, the embed as the editor builds it.
pub fn preview_json(preview: &Preview, user: (u64, &str), info: Option<&GuildInfo>) -> Value {
    let show = |text: &str| readable(text, user, info);
    let mut embed = preview.rendered.embed.clone();
    if let Some(embed) = &mut embed {
        for text in embed.texts_mut() {
            *text = show(text);
        }
    }
    json!({
        "text": show(&preview.rendered.content),
        "embed": embed,
        "ephemeral": preview.ephemeral,
        "notes": preview.notes,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::guild_info::{BotInfo, ChannelInfo, RoleInfo};

    fn info() -> GuildInfo {
        GuildInfo {
            channels: vec![ChannelInfo {
                id: "20".into(),
                name: "general".into(),
                kind: 0,
                parent_id: None,
                position: 0,
            }],
            roles: vec![RoleInfo {
                id: "10".into(),
                name: "Regular".into(),
                color: 0,
                position: 1,
                managed: false,
                everyone: false,
                assignable: true,
                why_not: None,
            }],
            bot: BotInfo {
                top_role_position: Some(5),
                manage_roles: true,
            },
        }
    }

    #[test]
    fn mentions_read_as_names() {
        let info = info();
        assert_eq!(
            readable(
                "hi <@7>, <@!8> and <@&10> in <#20> or <@&99> <#98>",
                (7, "Sim"),
                Some(&info)
            ),
            "hi @Sim, @someone and @Regular in #general or @role #channel"
        );
        assert_eq!(readable("a < b <@x> <@7", (7, "Sim"), None), "a < b <@x> <@7");
        assert_eq!(readable("", (7, "Sim"), None), "");
    }

    #[test]
    fn what_is_new_is_all_that_is_checked() {
        let old = Refs {
            allow_roles: vec![1, 2],
            post_channel: Some(5),
            ..Default::default()
        };
        let now = Refs {
            allow_roles: vec![1, 2, 3],
            post_channel: Some(5),
            watched_role: Some(9),
            ..Default::default()
        };
        let new = now.new_since(&old);
        assert_eq!(new.allow_roles, [3]);
        assert_eq!((new.post_channel, new.watched_role), (None, Some(9)));
        assert!(now.new_since(&now).is_empty());
    }

    #[test]
    fn an_import_leaves_out_what_the_server_lacks() {
        let info = info();
        let mut def: CommandDef = serde_json::from_value(json!({
            "name": "x", "responses": ["y"], "allow_roles": ["10", "11"], "deny_channels": ["20", "21"],
            "actions": [{ "kind": "add_role", "role_id": "10" }, { "kind": "add_role", "role_id": "99" }, { "kind": "dm", "text": "hi" }]
        }))
        .unwrap();
        let dropped = prune_command(&mut def, &info);
        assert_eq!(def.allow_roles, [10]);
        assert_eq!(def.deny_channels, [20]);
        assert_eq!(
            def.actions.len(),
            2,
            "the unknown role action went, the direct message stayed"
        );
        assert_eq!(dropped.len(), 3, "{dropped:?}");
    }
}
