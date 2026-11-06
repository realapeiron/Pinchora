//! Who may use a command, and where, as Discord settles it before a slash command is sent: the
//! channel's permission overwrites (Use Application Commands) and the overrides a server sets for
//! the bot under Server Settings, Integrations. Pure: the data is fetched and cached by the [`App`](crate::App).

use std::collections::HashMap;

use twilight_model::{
    application::command::permissions::{CommandPermission, CommandPermissionType, GuildCommandPermissions},
    channel::permission_overwrite::{PermissionOverwrite, PermissionOverwriteType},
    guild::Permissions,
    id::Id,
};

/// What a member may do in a channel: their server permissions with the channel's overwrites on
/// top, in the order Discord applies them (`@everyone`, then the roles together, then the member).
/// Administrators and the owner (who have [`Permissions::ADMINISTRATOR`] here) have everything.
pub(crate) fn channel_permissions(
    base: Permissions,
    guild: u64,
    user: u64,
    held: &[u64],
    overwrites: &[PermissionOverwrite],
) -> Permissions {
    if base.contains(Permissions::ADMINISTRATOR) {
        return Permissions::all();
    }
    let of = |kind, id: u64| overwrites.iter().find(|o| o.kind == kind && o.id.get() == id);
    let mut permissions = base;
    if let Some(everyone) = of(PermissionOverwriteType::Role, guild) {
        permissions = (permissions - everyone.deny) | everyone.allow;
    }
    let (mut allow, mut deny) = (Permissions::empty(), Permissions::empty());
    for role in overwrites
        .iter()
        .filter(|o| o.kind == PermissionOverwriteType::Role && o.id.get() != guild && held.contains(&o.id.get()))
    {
        allow |= role.allow;
        deny |= role.deny;
    }
    permissions = (permissions - deny) | allow;
    if let Some(member) = of(PermissionOverwriteType::Member, user) {
        permissions = (permissions - member.deny) | member.allow;
    }
    permissions
}

/// What a server's overrides say about one member using one command in one channel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Verdict {
    /// Switched off for them here.
    Denied,
    /// Switched on for them by name (their user, a role they hold, or `@everyone`): this beats the
    /// permissions the command asks for by default.
    Allowed,
    /// The overrides have nothing to say: the command's default permissions decide.
    Silent,
}

/// A server's command permission overrides: one entry for every command without its own, and the
/// entries of single commands.
#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct CommandOverrides {
    all: Option<Vec<CommandPermission>>,
    by_name: HashMap<String, Vec<CommandPermission>>,
}

impl CommandOverrides {
    /// Reads what Discord returned for a server. `ids` maps command names to ids, which is how the
    /// entries of single commands are told apart; the entry whose id is the application's holds
    /// the overrides for every command that has no entry of its own.
    pub(crate) fn new(entries: Vec<GuildCommandPermissions>, application: u64, ids: &HashMap<String, u64>) -> Self {
        let mut overrides = Self::default();
        for entry in entries {
            if entry.id.get() == application {
                overrides.all = Some(entry.permissions);
            } else if let Some((name, _)) = ids.iter().find(|(_, id)| **id == entry.id.get()) {
                overrides.by_name.insert(name.clone(), entry.permissions);
            }
        }
        overrides
    }

    /// Whether the server lets `user`, holding the roles `held`, use `command` in a channel.
    /// `channels` is the channel followed by its parent when it is a thread: a thread follows the
    /// channel it sits in. The channel rules only ever switch a command off; who it is for is
    /// decided by the member's own entry, then their roles (one that allows beats one that
    /// denies), then `@everyone`. Administrators are never asked.
    pub(crate) fn verdict(&self, command: &str, guild: u64, user: u64, held: &[u64], channels: &[u64]) -> Verdict {
        let Some(entry) = self.by_name.get(command).or(self.all.as_ref()) else {
            return Verdict::Silent;
        };
        let said = |target: CommandPermissionType| entry.iter().find(|p| p.id == target).map(|p| p.permission);
        let channel = |id: u64| said(CommandPermissionType::Channel(Id::new_checked(id)?));
        let here = channels.iter().find_map(|&id| channel(id));
        // Every channel is the server's id less one.
        let everywhere = guild.checked_sub(1).and_then(channel);
        if here.or(everywhere) == Some(false) {
            return Verdict::Denied;
        }
        let verdict = |allowed: bool| if allowed { Verdict::Allowed } else { Verdict::Denied };
        if let Some(allowed) = Id::new_checked(user).and_then(|id| said(CommandPermissionType::User(id))) {
            return verdict(allowed);
        }
        let roles: Vec<bool> = entry
            .iter()
            .filter_map(|p| match p.id {
                CommandPermissionType::Role(role) if role.get() != guild && held.contains(&role.get()) => {
                    Some(p.permission)
                }
                _ => None,
            })
            .collect();
        if !roles.is_empty() {
            return verdict(roles.contains(&true));
        }
        match Id::new_checked(guild).and_then(|id| said(CommandPermissionType::Role(id))) {
            Some(allowed) => verdict(allowed),
            None => Verdict::Silent,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const GUILD: u64 = 100;

    fn overwrite(kind: PermissionOverwriteType, id: u64, allow: Permissions, deny: Permissions) -> PermissionOverwrite {
        PermissionOverwrite {
            allow,
            deny,
            id: Id::new(id),
            kind,
        }
    }

    #[test]
    fn channel_overwrites_apply_everyone_then_roles_then_the_member() {
        use PermissionOverwriteType::{Member, Role};
        let base = Permissions::SEND_MESSAGES | Permissions::USE_SLASH_COMMANDS;
        let apps = Permissions::USE_SLASH_COMMANDS;
        let none = Permissions::empty();
        // No overwrites: the server permissions.
        assert_eq!(channel_permissions(base, GUILD, 1, &[], &[]), base);
        // `@everyone` denied here.
        let muted = [overwrite(Role, GUILD, none, apps)];
        assert!(!channel_permissions(base, GUILD, 1, &[], &muted).contains(apps));
        // A role allows it again; the roles' allow beats another role's deny.
        let staff = [
            overwrite(Role, GUILD, none, apps),
            overwrite(Role, 5, apps, none),
            overwrite(Role, 6, none, apps),
        ];
        assert!(channel_permissions(base, GUILD, 1, &[5, 6], &staff).contains(apps));
        assert!(!channel_permissions(base, GUILD, 1, &[6], &staff).contains(apps));
        assert!(!channel_permissions(base, GUILD, 1, &[], &staff).contains(apps));
        // The member's own overwrite comes last and wins.
        let personal = [overwrite(Role, 6, none, apps), overwrite(Member, 1, apps, none)];
        assert!(channel_permissions(base, GUILD, 1, &[6], &personal).contains(apps));
        assert!(!channel_permissions(base, GUILD, 2, &[6], &personal).contains(apps));
        // Another member's or another role's overwrite is nothing to them.
        let others = [overwrite(Member, 9, none, apps), overwrite(Role, 7, none, apps)];
        assert!(channel_permissions(base, GUILD, 1, &[], &others).contains(apps));
        // Administrators are not stopped by overwrites.
        let admin = Permissions::all();
        assert_eq!(channel_permissions(admin, GUILD, 1, &[], &muted), Permissions::all());
    }

    fn permission(target: CommandPermissionType, allowed: bool) -> CommandPermission {
        CommandPermission {
            id: target,
            permission: allowed,
        }
    }

    fn overrides(app: Vec<CommandPermission>, fish: Vec<CommandPermission>) -> CommandOverrides {
        let entry = |id, permissions| GuildCommandPermissions {
            application_id: Id::new(1),
            guild_id: Id::new(GUILD),
            id: Id::new(id),
            permissions,
        };
        let ids = HashMap::from([("fish".to_owned(), 77), ("pet".to_owned(), 78)]);
        let mut entries = vec![entry(1, app)];
        if !fish.is_empty() {
            entries.push(entry(77, fish));
        }
        CommandOverrides::new(entries, 1, &ids)
    }

    fn role(id: u64) -> CommandPermissionType {
        CommandPermissionType::Role(Id::new(id))
    }

    fn user(id: u64) -> CommandPermissionType {
        CommandPermissionType::User(Id::new(id))
    }

    fn channel(id: u64) -> CommandPermissionType {
        CommandPermissionType::Channel(Id::new(id))
    }

    #[test]
    fn a_command_follows_its_own_entry_else_the_applications() {
        let sets = overrides(
            vec![permission(role(GUILD), false)],
            vec![permission(role(GUILD), true)],
        );
        assert_eq!(sets.verdict("fish", GUILD, 1, &[], &[5]), Verdict::Allowed);
        assert_eq!(
            sets.verdict("pet", GUILD, 1, &[], &[5]),
            Verdict::Denied,
            "the application's"
        );
        assert_eq!(
            sets.verdict("pet", GUILD, 1, &[], &[5]),
            sets.verdict("anything", GUILD, 1, &[], &[5])
        );
        assert_eq!(
            overrides(vec![], vec![]).verdict("fish", GUILD, 1, &[], &[5]),
            Verdict::Silent
        );
        assert_eq!(
            CommandOverrides::default().verdict("fish", GUILD, 1, &[], &[5]),
            Verdict::Silent
        );
    }

    #[test]
    fn who_is_decided_by_the_member_then_roles_then_everyone() {
        let sets = overrides(
            vec![
                permission(role(GUILD), false),
                permission(role(5), true),
                permission(role(6), false),
                permission(user(9), true),
                permission(user(8), false),
            ],
            vec![],
        );
        let check = |user, held: &[u64]| sets.verdict("fish", GUILD, user, held, &[5]);
        assert_eq!(check(1, &[]), Verdict::Denied, "@everyone");
        assert_eq!(check(1, &[5]), Verdict::Allowed, "a role that allows");
        assert_eq!(check(1, &[6]), Verdict::Denied, "a role that denies");
        assert_eq!(check(1, &[5, 6]), Verdict::Allowed, "allow beats deny between roles");
        assert_eq!(check(9, &[6]), Verdict::Allowed, "the member beats their roles");
        assert_eq!(check(8, &[5]), Verdict::Denied, "the member beats their roles");
        let quiet = overrides(vec![permission(role(5), true)], vec![]);
        assert_eq!(
            quiet.verdict("fish", GUILD, 1, &[], &[5]),
            Verdict::Silent,
            "nobody was named"
        );
        // A role the member does not hold is nothing to them.
        assert_eq!(quiet.verdict("fish", GUILD, 1, &[7], &[5]), Verdict::Silent);
    }

    #[test]
    fn channel_rules_only_switch_a_command_off() {
        let sets = overrides(
            vec![
                permission(channel(GUILD - 1), false),
                permission(channel(20), true),
                permission(channel(30), false),
                permission(user(9), true),
            ],
            vec![],
        );
        let check = |user, channels: &[u64]| sets.verdict("fish", GUILD, user, &[], channels);
        assert_eq!(check(1, &[10]), Verdict::Denied, "every channel is off");
        assert_eq!(check(1, &[20]), Verdict::Silent, "a channel that is switched on again");
        assert_eq!(check(9, &[20]), Verdict::Allowed);
        assert_eq!(
            check(9, &[10]),
            Verdict::Denied,
            "a member's allow does not open a closed channel"
        );
        assert_eq!(check(1, &[40, 20]), Verdict::Silent, "a thread follows its channel");
        assert_eq!(check(1, &[40, 30]), Verdict::Denied, "a thread follows its channel");
        let open = overrides(vec![permission(channel(30), false)], vec![]);
        assert_eq!(open.verdict("fish", GUILD, 1, &[], &[10]), Verdict::Silent);
        assert_eq!(open.verdict("fish", GUILD, 1, &[], &[30]), Verdict::Denied);
    }
}
