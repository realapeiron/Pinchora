//! Text commands: `p!fish cast reef`, or `@Pinchora fish cast reef`, runs `/fish cast spot:reef`.
//!
//! The message is read against the registered slash commands into the same command data Discord
//! sends, the member is given the permissions Discord would compute for them in the server, and the
//! command then goes through [`handle_interaction`] like a slash command: the module gate, the
//! Manage Server checks, cooldowns and handlers are the very same. Only the answers travel
//! differently, as replies to the message (see [`Responder::text`]).

pub(crate) mod access;
pub(crate) mod parse;

use std::{collections::HashMap, sync::Arc, sync::LazyLock, time::Duration};

use pinchora_core::{
    i18n::Lang,
    modules::{Access, Module, refusal},
    settings::DEFAULT_PREFIX,
    t,
};
use serde_json::json;
use twilight_model::{
    application::{
        command::Command,
        interaction::{
            Interaction, InteractionChannel, InteractionDataResolved, InteractionMember,
            application_command::CommandData,
        },
    },
    channel::message::{Message, MessageFlags, MessageType},
    guild::{PartialMember, Permissions},
    http::interaction::InteractionResponseData,
    id::{Id, marker::UserMarker},
    user::User,
};

use crate::{App, Responder, app::PermissionFacts, handle_interaction};

use access::{Verdict, channel_permissions};
use parse::{Parsed, Refusal};

/// The registered commands, read once.
static TREE: LazyLock<Vec<Command>> = LazyLock::new(crate::command_definitions);

/// Commands that stay slash-only, as `name` or `name sub`; anything below an entry is covered. They
/// answer with the slash command to use before they run. Either their first answer is a private
/// panel that a channel reply cannot stand in for (any other private panel is caught as it is
/// answered), or it shows the server's settings, which a reply in the channel would show to everyone.
const SLASH_ONLY: &[&str] = &[
    "character",
    "setup",
    "safety",
    "community",
    "integrations",
    "progression",
    "customize",
    "house decorate",
    "house edit",
    "play 2048",
    "play picross",
    "play wordle",
    "stall manage",
];

/// A member's text commands count at most this often; faster ones are ignored.
const RATE: Duration = Duration::from_millis(1_500);

/// What the message says after the bot's mention or the server's prefix, and whether it was the mention.
fn strip<'m>(content: &'m str, prefix: &str, bot: u64) -> Option<(&'m str, bool)> {
    let content = content.trim_start();
    for mention in [format!("<@{bot}>"), format!("<@!{bot}>")] {
        if let Some(rest) = content.strip_prefix(mention.as_str()) {
            let rest = rest.trim_start();
            // `@Pinchora p!profile` works too.
            return Some((strip_prefix(rest, prefix).unwrap_or(rest), true));
        }
    }
    strip_prefix(content, prefix).map(|rest| (rest, false))
}

/// `content` after `prefix`, which matches in any case.
fn strip_prefix<'m>(content: &'m str, prefix: &str) -> Option<&'m str> {
    let head = content.get(..prefix.len())?;
    head.eq_ignore_ascii_case(prefix).then(|| &content[prefix.len()..])
}

/// Whether a message can be a text command at all: a member's message in a server.
fn eligible(message: &Message) -> bool {
    !message.author.bot
        && message.webhook_id.is_none()
        && message.guild_id.is_some()
        && matches!(message.kind, MessageType::Regular | MessageType::Reply)
}

/// The prefix a server's text commands start with; the default when its settings can't be read.
async fn prefix_of(app: &App, guild: u64) -> String {
    match app.guild_settings(guild).await {
        Ok(settings) => settings.prefix().to_owned(),
        Err(error) => {
            tracing::debug!(%error, guild, "settings unavailable; using the default prefix");
            DEFAULT_PREFIX.to_owned()
        }
    }
}

/// How a server calls commands by text, for usage lines and `/help`: its prefix when the bot reads
/// messages, otherwise the bot's mention.
pub(crate) async fn shown_prefix(app: &App, guild: u64) -> String {
    if app.message_content {
        prefix_of(app, guild).await
    } else {
        "@Pinchora ".to_owned()
    }
}

/// Cheaply, whether `message` starts with the bot's mention or the server's prefix.
pub(crate) async fn calls_bot(app: &App, message: &Message) -> bool {
    let Some(guild) = message.guild_id.filter(|_| eligible(message)) else {
        return false;
    };
    let prefix = prefix_of(app, guild.get()).await;
    strip(&message.content, &prefix, app.application_id.get()).is_some()
}

/// The member's permissions in the server, as Discord works them out for a slash command: the
/// owner and administrators have all of them, everyone else what `@everyone` and their roles give.
/// The channel's overwrites come on top of this ([`channel_permissions`]).
fn guild_permissions(facts: &PermissionFacts, guild: u64, user: u64, held: &[u64]) -> Permissions {
    if user == facts.owner {
        return Permissions::all();
    }
    let granted = facts
        .roles
        .iter()
        .filter(|r| r.id.get() == guild || held.contains(&r.id.get()))
        .fold(Permissions::empty(), |p, r| p | r.permissions);
    if granted.contains(Permissions::ADMINISTRATOR) {
        Permissions::all()
    } else {
        granted
    }
}

/// The permission names a command asks for, worded as Discord's settings show them; `None` when it
/// asks for none of the four a server sets commands up with.
fn permission_names(needed: Permissions, lang: Lang) -> Option<String> {
    let names: Vec<String> = [
        (Permissions::MANAGE_GUILD, "text.permission.manage-server"),
        (Permissions::MANAGE_ROLES, "text.permission.manage-roles"),
        (Permissions::MANAGE_MESSAGES, "text.permission.manage-messages"),
        (Permissions::ADMINISTRATOR, "text.permission.administrator"),
    ]
    .into_iter()
    .filter(|(p, _)| needed.contains(*p))
    .map(|(_, key)| t!(lang, key))
    .collect();
    (!names.is_empty()).then(|| names.join(&t!(lang, "text.and")))
}

/// A member as resolved data carries them.
fn resolved_member(member: &PartialMember) -> InteractionMember {
    InteractionMember {
        avatar: member.avatar,
        avatar_decoration_data: member.avatar_decoration_data.clone(),
        banner: member.banner,
        communication_disabled_until: member.communication_disabled_until,
        flags: member.flags,
        joined_at: member.joined_at,
        nick: member.nick.clone(),
        pending: false,
        permissions: Permissions::empty(),
        premium_since: member.premium_since,
        roles: member.roles.clone(),
    }
}

/// A user and, when they are in the server, their membership: from the message's mentions, else
/// from Discord.
async fn look_up_user(
    app: &App,
    guild: u64,
    message: &Message,
    id: Id<UserMarker>,
) -> Option<(User, Option<InteractionMember>)> {
    let mention = message.mentions.iter().find(|m| m.id == id);
    let (member, member_user) = match mention.and_then(|m| m.member.as_ref()) {
        Some(member) => (Some(resolved_member(member)), None),
        None => match async { anyhow::Ok(app.http.guild_member(Id::new(guild), id).await?.model().await?) }.await {
            Ok(m) => (
                Some(InteractionMember {
                    avatar: m.avatar,
                    avatar_decoration_data: m.avatar_decoration_data,
                    banner: m.banner,
                    communication_disabled_until: m.communication_disabled_until,
                    flags: m.flags,
                    joined_at: m.joined_at,
                    nick: m.nick,
                    pending: m.pending,
                    permissions: Permissions::empty(),
                    premium_since: m.premium_since,
                    roles: m.roles,
                }),
                Some(m.user),
            ),
            Err(_) => (None, None),
        },
    };
    let user = match (app.user(id.get()).await, member_user, mention) {
        (Some(user), _, _) => (*user).clone(),
        (None, Some(user), _) => user,
        (None, None, Some(mention)) => serde_json::from_value(json!({
            "id": mention.id.to_string(),
            "username": mention.name,
            "discriminator": format!("{:04}", mention.discriminator),
            "avatar": mention.avatar.map(|a| a.to_string()),
            "bot": mention.bot,
            "global_name": null,
        }))
        .ok()?,
        (None, None, None) => return None,
    };
    Some((user, member))
}

/// Looks up what the options name, as Discord would have sent it, or says what could not be found.
async fn resolve(
    app: &App,
    guild: u64,
    message: &Message,
    parsed: &Parsed<'_>,
    facts: &PermissionFacts,
    lang: Lang,
) -> Result<InteractionDataResolved, String> {
    let mut resolved = InteractionDataResolved {
        attachments: HashMap::new(),
        channels: HashMap::new(),
        members: HashMap::new(),
        messages: HashMap::new(),
        roles: HashMap::new(),
        users: HashMap::new(),
    };
    for &id in &parsed.users {
        let (user, member) = look_up_user(app, guild, message, id)
            .await
            .ok_or_else(|| t!(lang, "text.member-not-found", id = id))?;
        resolved.users.insert(id, user);
        if let Some(member) = member {
            resolved.members.insert(id, member);
        }
    }
    for &id in &parsed.roles {
        let role = facts
            .roles
            .iter()
            .find(|r| r.id == id)
            .ok_or_else(|| t!(lang, "text.role-not-found"))?;
        resolved.roles.insert(id, role.clone());
    }
    for &(id, option) in &parsed.channels {
        let channel = match app.http.channel(id).await {
            Ok(response) => response.model().await.ok(),
            Err(_) => None,
        }
        .filter(|c| c.guild_id.map(|g| g.get()) == Some(guild))
        .ok_or_else(|| t!(lang, "text.channel-not-found"))?;
        if let Some(kinds) = option.channel_types.as_ref().filter(|k| !k.is_empty())
            && !kinds.contains(&channel.kind)
        {
            return Err(t!(lang, "text.channel-kind", option = option.name));
        }
        resolved.channels.insert(
            id,
            InteractionChannel {
                id,
                kind: channel.kind,
                name: channel.name.unwrap_or_default(),
                parent_id: channel.parent_id,
                permissions: Permissions::empty(),
                app_permissions: None,
                thread_metadata: channel.thread_metadata,
            },
        );
    }
    Ok(resolved)
}

/// The slash command interaction the message stands for, as Discord would send it.
fn interaction(
    app: &App,
    message: &Message,
    permissions: Permissions,
    data: CommandData,
) -> anyhow::Result<Interaction> {
    let guild = message.guild_id.map(|g| g.to_string());
    let mut member = match &message.member {
        Some(member) => serde_json::to_value(member)?,
        None => json!({ "roles": [], "joined_at": null, "deaf": false, "mute": false, "flags": 0 }),
    };
    member["user"] = serde_json::to_value(&message.author)?;
    member["permissions"] = json!(permissions.bits().to_string());
    let interaction: Interaction = serde_json::from_value(json!({
        "id": message.id.to_string(),
        "application_id": app.application_id.to_string(),
        "type": 2,
        "token": format!("text-{}", message.id),
        "version": 1,
        "guild_id": guild,
        "channel": { "id": message.channel_id.to_string(), "type": 0, "guild_id": guild },
        "context": 0,
        "attachment_size_limit": 10_485_760,
        "authorizing_integration_owners": {},
        "entitlements": [],
        "member": member,
        "data": serde_json::to_value(&data)?,
    }))?;
    Ok(interaction)
}

/// A private answer to a text command: a reply that goes away by itself.
fn private(text: impl Into<String>) -> InteractionResponseData {
    InteractionResponseData {
        content: Some(text.into()),
        flags: Some(MessageFlags::EPHEMERAL),
        ..Default::default()
    }
}

/// Whether the rate limiter lets a text command through: only when it counted this one as the member's
/// first in a while. A limiter that can't be read lets nothing through, because nothing could limit
/// the reply either.
fn within_rate(counted: &anyhow::Result<Option<Duration>>) -> bool {
    matches!(counted, Ok(None))
}

/// Whether a command's default permissions let a member with `server` permissions use it: none are
/// asked for, or the member has them all. `"0"` (nothing set, but not nothing asked either) is how a
/// command is kept for administrators.
fn allowed_by_default(needed: Option<Permissions>, server: Permissions) -> bool {
    match needed {
        None => true,
        Some(needed) if needed.is_empty() => server.contains(Permissions::ADMINISTRATOR),
        Some(needed) => server.contains(needed),
    }
}

/// Runs `message` as a text command if it is one, to the end, and returns whether it was one. A
/// member's message that starts with the server's prefix or the bot's mention and names one of the
/// bot's commands; anything else is left alone.
///
/// The server's `text_commands` module decides first: where it is off, a command typed with the
/// prefix is ignored without a word (another bot may be using that prefix there), and one typed
/// after the bot's mention is refused. The rate limit fails closed: when it cannot be read the
/// command is ignored, because a reply could not be limited either.
pub async fn on_text_command(app: Arc<App>, message: &Message) -> bool {
    let Some(guild) = message.guild_id.filter(|_| eligible(message)).map(|g| g.get()) else {
        return false;
    };
    let prefix = prefix_of(&app, guild).await;
    let Some((rest, mentioned)) = strip(&message.content, &prefix, app.application_id.get()) else {
        return false;
    };
    let shown = if mentioned || !app.message_content {
        "@Pinchora ".to_owned()
    } else {
        prefix.clone()
    };
    let expanded = match app.command_control(guild).await {
        Ok(config) => config.expand(rest),
        Err(error) => {
            tracing::warn!(%error, "command policy unavailable");
            return true;
        }
    };
    let (expanded, moved) = crate::routing::resolve_text(&expanded);
    let rest = expanded.as_str();
    if !mentioned && !parse::is_command(&TREE, rest) {
        return false;
    }
    let user = message.author.id.get();
    let lang = crate::lang::resolve_text(&app, user, guild).await;
    let parsed = parse::parse(&TREE, &shown, rest, lang);
    let gate = app
        .module_access(guild, Some(message.channel_id.get()), Module::TextCommands)
        .await;
    if !mentioned && !matches!(gate, Ok(Access::Allowed)) {
        if let Err(error) = &gate {
            tracing::warn!(%error, guild, "text commands ignored: module rules unavailable");
        }
        metrics::counter!("pinchora_text_commands_refused_total", "reason" => "module").increment(1);
        return true;
    }
    let counted = app.cache.try_cooldown(&format!("text:{user}"), RATE).await;
    if !within_rate(&counted) {
        if let Err(error) = &counted {
            tracing::warn!(%error, "text commands ignored: rate limit unavailable");
            metrics::counter!("pinchora_text_commands_refused_total", "reason" => "rate_limit").increment(1);
        }
        return true;
    }
    let path = match &parsed {
        Ok(parsed) => parsed.path.clone(),
        Err(_) => "help".to_owned(),
    };
    let started = std::time::Instant::now();
    let responder = Arc::new(Responder::text(
        app.clone(),
        guild,
        message.channel_id,
        message.id,
        path.clone(),
        lang,
    ));
    if let Some(route) = moved
        && matches!(
            app.cache
                .try_cooldown(
                    &format!("command-moved:{guild}:{user}:{}", route.legacy),
                    Duration::from_secs(86400),
                )
                .await,
            Ok(None)
        )
    {
        responder.set_migration_notice(t!(lang, "text.moved", old = route.legacy, new = route.canonical));
    }
    let notice = match prepare(&app, guild, message, parsed, gate, &prefix, lang).await {
        Ok(Prepared::Run(interaction)) => {
            handle_interaction(app.clone(), *interaction, responder.clone()).await;
            None
        }
        Ok(Prepared::Notice(notice)) => Some(notice),
        Err(error) => {
            tracing::error!(error = ?error, name = %path, "text command failed before it ran");
            Some(t!(lang, "error.generic"))
        }
    };
    if let Some(notice) = notice
        && let Err(error) = responder.respond(private(notice), false).await
    {
        tracing::debug!(%error, "text command notice not posted");
    }
    let label = path.split(' ').next().unwrap_or_default().to_owned();
    metrics::counter!("pinchora_text_commands_total", "name" => label).increment(1);
    metrics::histogram!("pinchora_text_command_seconds").record(started.elapsed().as_secs_f64());
    true
}

/// What a text command comes to before any handler runs.
enum Prepared {
    /// The slash command interaction it stands for.
    Run(Box<Interaction>),
    /// What to tell the member instead.
    Notice(String),
}

/// Everything between the parse and the handler: the interaction to run, or the notice to post.
/// `gate` is what the `text_commands` module said for this channel.
async fn prepare(
    app: &App,
    guild: u64,
    message: &Message,
    parsed: Result<Parsed<'_>, Refusal>,
    gate: anyhow::Result<Access>,
    prefix: &str,
    lang: Lang,
) -> anyhow::Result<Prepared> {
    let user = message.author.id.get();
    let held: Vec<u64> = message
        .member
        .as_ref()
        .map(|m| m.roles.iter().map(|r| r.get()).collect())
        .unwrap_or_default();
    let held = held.as_slice();
    // Switched off here, and the member spoke to the bot directly: say so, as a slash command would.
    match gate? {
        Access::Allowed => {}
        access => {
            let manager = match app.permission_facts(guild).await {
                Ok(facts) => guild_permissions(&facts, guild, user, held).contains(Permissions::MANAGE_GUILD),
                Err(_) => false,
            };
            metrics::counter!("pinchora_text_commands_refused_total", "reason" => "module").increment(1);
            return Ok(Prepared::Notice(refusal(Module::TextCommands, access, manager, lang)));
        }
    }
    let parsed = match parsed {
        Ok(parsed) => parsed,
        Err(Refusal::Usage(usage)) => return Ok(Prepared::Notice(usage)),
        Err(Refusal::Unknown) => {
            let hint = if app.message_content {
                t!(lang, "text.unknown-prefix", prefix = prefix)
            } else {
                t!(lang, "text.unknown-mention")
            };
            return Ok(Prepared::Notice(hint));
        }
    };
    if crate::routing::is_admin_path(&parsed.command.name) {
        let facts = app.permission_facts(guild).await?;
        if !guild_permissions(&facts, guild, user, held).contains(Permissions::MANAGE_GUILD) {
            return Ok(Prepared::Notice(t!(lang, "config.needs-manage-server")));
        }
    }
    if parsed.path != "customize bio"
        && SLASH_ONLY
            .iter()
            .any(|e| parsed.path == *e || parsed.path.starts_with(&format!("{e} ")))
    {
        let notice = t!(lang, "text.slash-only", path = parsed.path);
        return Ok(Prepared::Notice(notice));
    }
    // What Discord does before a slash command is sent: where the member may use commands at all,
    // and what the server's Integrations settings say about this command.
    let facts = app.permission_facts(guild).await?;
    let server = guild_permissions(&facts, guild, user, held);
    let channel = app.channel_rules(message.channel_id.get()).await?;
    let permissions = channel_permissions(server, guild, user, held, &channel.overwrites);
    if !permissions.contains(Permissions::USE_SLASH_COMMANDS) {
        metrics::counter!("pinchora_text_commands_refused_total", "reason" => "channel_permission").increment(1);
        return Ok(Prepared::Notice(t!(lang, "text.no-commands-here")));
    }
    let verdict = if server.contains(Permissions::ADMINISTRATOR) {
        Verdict::Silent
    } else {
        let overrides = app.command_overrides(guild).await?;
        overrides.verdict(&parsed.command.name, guild, user, held, &channel.lineage)
    };
    let may = match verdict {
        Verdict::Denied => false,
        Verdict::Allowed => true,
        Verdict::Silent => allowed_by_default(parsed.command.default_member_permissions, server),
    };
    if !may {
        metrics::counter!("pinchora_text_commands_refused_total", "reason" => "command_permission").increment(1);
        let command = &parsed.command.name;
        let notice = match (verdict, parsed.command.default_member_permissions) {
            (Verdict::Silent, Some(needed)) if !needed.is_empty() => match permission_names(needed, lang) {
                Some(permissions) => t!(
                    lang,
                    "text.need-permission",
                    permissions = permissions,
                    command = command
                ),
                None => t!(lang, "text.need-other-permission", command = command),
            },
            (Verdict::Silent, _) => t!(lang, "text.admins-only", command = command),
            _ => t!(lang, "text.server-denied", command = command),
        };
        return Ok(Prepared::Notice(notice));
    }
    let resolved = match resolve(app, guild, message, &parsed, &facts, lang).await {
        Ok(resolved) => resolved,
        Err(notice) => return Ok(Prepared::Notice(notice)),
    };
    let mut data = parsed.data;
    data.guild_id = message.guild_id;
    let any = !(resolved.users.is_empty() && resolved.roles.is_empty() && resolved.channels.is_empty());
    data.resolved = any.then_some(resolved);
    Ok(Prepared::Run(Box::new(interaction(app, message, permissions, data)?)))
}

#[cfg(test)]
mod tests {
    use twilight_model::application::command::CommandOptionType;

    use super::*;

    #[test]
    fn the_prefix_or_the_mention_comes_first() {
        assert_eq!(strip("p!profile", "p!", 1), Some(("profile", false)));
        assert_eq!(strip("P!Profile", "p!", 1), Some(("Profile", false)));
        assert_eq!(strip("  p! fish cast", "p!", 1), Some((" fish cast", false)));
        assert_eq!(strip("<@1> fish cast", "p!", 1), Some(("fish cast", true)));
        assert_eq!(strip("<@!1>help", "p!", 1), Some(("help", true)));
        assert_eq!(strip("<@1> p!help", "p!", 1), Some(("help", true)));
        assert_eq!(strip("<@2> help", "p!", 1), None, "someone else's mention");
        assert_eq!(strip("hello p!profile", "p!", 1), None);
        assert_eq!(
            strip("ğ", "p!", 1),
            None,
            "a short message that is not on a char boundary"
        );
        assert_eq!(strip("ğprofile", "ğ", 1), Some(("profile", false)));
    }

    #[test]
    fn every_slash_only_entry_names_a_command() {
        let paths: Vec<String> = TREE
            .iter()
            .flat_map(|c| {
                let subs = c.options.iter().filter(|o| {
                    matches!(
                        o.kind,
                        CommandOptionType::SubCommand | CommandOptionType::SubCommandGroup
                    )
                });
                std::iter::once(c.name.clone())
                    .chain(subs.map(|o| format!("{} {}", c.name, o.name)).collect::<Vec<_>>())
            })
            .collect();
        for entry in SLASH_ONLY {
            assert!(paths.iter().any(|p| p == entry), "`{entry}` is not a command");
        }
    }

    fn role(id: u64, permissions: Permissions) -> twilight_model::guild::Role {
        serde_json::from_value(json!({
            "id": id.to_string(),
            "name": "r",
            "color": 0,
            "colors": { "primary_color": 0, "secondary_color": null, "tertiary_color": null },
            "hoist": false,
            "managed": false,
            "mentionable": false,
            "permissions": permissions.bits().to_string(),
            "position": 1,
            "flags": 0,
        }))
        .expect("role JSON")
    }

    #[test]
    fn the_rate_limit_fails_closed() {
        assert!(within_rate(&Ok(None)), "the first command in a while");
        assert!(!within_rate(&Ok(Some(Duration::from_millis(900)))), "too soon");
        assert!(
            !within_rate(&Err(anyhow::anyhow!("redis is down"))),
            "unknown is denied"
        );
    }

    #[test]
    fn a_command_kept_for_administrators_is_only_theirs() {
        let none = Permissions::empty();
        let staff = Permissions::MANAGE_GUILD;
        // Nothing asked: everyone.
        assert!(allowed_by_default(None, none));
        // Something asked: those who have it all.
        assert!(!allowed_by_default(Some(staff), none));
        assert!(allowed_by_default(Some(staff), staff | Permissions::SEND_MESSAGES));
        assert!(allowed_by_default(Some(staff), Permissions::all()));
        // "0": nobody but administrators (an empty set is contained in every set, which must not count).
        assert!(!allowed_by_default(Some(none), none));
        assert!(!allowed_by_default(Some(none), staff));
        assert!(allowed_by_default(Some(none), Permissions::ADMINISTRATOR));
        assert!(allowed_by_default(Some(none), Permissions::all()));
    }

    #[test]
    fn permissions_follow_roles_owners_and_administrators() {
        let facts = PermissionFacts {
            owner: 9,
            roles: vec![
                role(5, Permissions::SEND_MESSAGES),
                role(6, Permissions::MANAGE_GUILD),
                role(7, Permissions::ADMINISTRATOR),
            ],
        };
        assert_eq!(guild_permissions(&facts, 5, 1, &[]), Permissions::SEND_MESSAGES);
        assert!(guild_permissions(&facts, 5, 1, &[6]).contains(Permissions::MANAGE_GUILD));
        assert_eq!(guild_permissions(&facts, 5, 1, &[7]), Permissions::all());
        assert_eq!(guild_permissions(&facts, 5, 9, &[]), Permissions::all());
        assert_eq!(
            permission_names(Permissions::MANAGE_GUILD, Lang::En).as_deref(),
            Some("Manage Server")
        );
        assert_eq!(
            permission_names(Permissions::MANAGE_GUILD | Permissions::MANAGE_ROLES, Lang::Tr).as_deref(),
            Some("Sunucuyu Yönet ve Rolleri Yönet")
        );
        assert_eq!(permission_names(Permissions::BAN_MEMBERS, Lang::En), None);
    }
}
