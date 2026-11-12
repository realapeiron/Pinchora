use twilight_model::{
    channel::permission_overwrite::{PermissionOverwrite, PermissionOverwriteType},
    guild::Permissions,
};

use super::RoleInfo;

pub(super) fn missing_post_permissions(
    guild: u64,
    bot: u64,
    roles: &[RoleInfo],
    held: Option<&[u64]>,
    overwrites: &[PermissionOverwrite],
) -> Vec<String> {
    let mut permissions = Permissions::empty();
    if let Some(held) = held {
        for role in roles
            .iter()
            .filter(|role| role.everyone || held.contains(&role.id.parse().unwrap_or(0)))
        {
            permissions |= Permissions::from_bits_truncate(role.permissions.parse().unwrap_or(0));
        }
        if permissions.contains(Permissions::ADMINISTRATOR) {
            return Vec::new();
        }
        let apply = |permissions: Permissions, deny: Permissions, allow: Permissions| (permissions - deny) | allow;
        if let Some(everyone) = overwrites
            .iter()
            .find(|o| o.kind == PermissionOverwriteType::Role && o.id.get() == guild)
        {
            permissions = apply(permissions, everyone.deny, everyone.allow);
        }
        let (mut allow, mut deny) = (Permissions::empty(), Permissions::empty());
        for overwrite in overwrites
            .iter()
            .filter(|o| o.kind == PermissionOverwriteType::Role && held.contains(&o.id.get()))
        {
            allow |= overwrite.allow;
            deny |= overwrite.deny;
        }
        permissions = apply(permissions, deny, allow);
        if let Some(member) = overwrites
            .iter()
            .find(|o| o.kind == PermissionOverwriteType::Member && o.id.get() == bot)
        {
            permissions = apply(permissions, member.deny, member.allow);
        }
    }
    if !permissions.contains(Permissions::VIEW_CHANNEL) {
        return vec!["view_channel".into()];
    }
    [
        (Permissions::SEND_MESSAGES, "send_messages"),
        (Permissions::EMBED_LINKS, "embed_links"),
        (Permissions::ATTACH_FILES, "attach_files"),
    ]
    .into_iter()
    .filter(|(permission, _)| !permissions.contains(*permission))
    .map(|(_, key)| key.to_owned())
    .collect()
}

#[cfg(test)]
mod posting_tests {
    use super::super::{BotInfo, ChannelInfo, GuildInfo};
    use super::{PermissionOverwrite, Permissions, RoleInfo, missing_post_permissions};
    use twilight_model::{channel::permission_overwrite::PermissionOverwriteType, id::Id};

    const GUILD: u64 = 100;
    const BOT: u64 = 200;
    const ROLE: u64 = 300;
    const POST: Permissions = Permissions::VIEW_CHANNEL
        .union(Permissions::SEND_MESSAGES)
        .union(Permissions::EMBED_LINKS)
        .union(Permissions::ATTACH_FILES);

    fn role(id: u64, permissions: Permissions) -> RoleInfo {
        RoleInfo {
            id: id.to_string(),
            name: String::new(),
            color: 0,
            position: 0,
            managed: false,
            everyone: id == GUILD,
            permissions: permissions.bits().to_string(),
            powers: vec![],
            assignable: true,
            why_not: None,
            action_ok: true,
            action_why_not: None,
        }
    }

    fn overwrite(id: u64, kind: PermissionOverwriteType, allow: Permissions, deny: Permissions) -> PermissionOverwrite {
        PermissionOverwrite {
            id: Id::new(id),
            kind,
            allow,
            deny,
        }
    }

    #[test]
    fn discord_posting_precedence_and_hidden_channel() {
        let all = [role(GUILD, POST), role(ROLE, Permissions::empty())];
        let missing = |roles: &[RoleInfo], held: &[u64], overwrites: &[PermissionOverwrite]| {
            missing_post_permissions(GUILD, BOT, roles, Some(held), overwrites)
        };
        assert!(
            missing(
                &[role(GUILD, POST), role(ROLE, Permissions::ADMINISTRATOR)],
                &[ROLE],
                &[overwrite(
                    BOT,
                    PermissionOverwriteType::Member,
                    Permissions::empty(),
                    POST
                )]
            )
            .is_empty()
        );
        let everyone_deny = overwrite(
            GUILD,
            PermissionOverwriteType::Role,
            Permissions::empty(),
            Permissions::SEND_MESSAGES,
        );
        assert_eq!(missing(&all, &[ROLE], &[everyone_deny]), ["send_messages"]);
        let role_allow = overwrite(
            ROLE,
            PermissionOverwriteType::Role,
            Permissions::SEND_MESSAGES,
            Permissions::empty(),
        );
        assert!(missing(&all, &[ROLE], &[everyone_deny, role_allow]).is_empty());
        let member_deny = overwrite(
            BOT,
            PermissionOverwriteType::Member,
            Permissions::empty(),
            Permissions::SEND_MESSAGES,
        );
        assert_eq!(
            missing(
                &all,
                &[ROLE],
                &[
                    everyone_deny,
                    overwrite(
                        ROLE,
                        PermissionOverwriteType::Role,
                        Permissions::SEND_MESSAGES,
                        Permissions::empty()
                    ),
                    member_deny
                ]
            ),
            ["send_messages"]
        );
        let member_allow = overwrite(
            BOT,
            PermissionOverwriteType::Member,
            Permissions::SEND_MESSAGES,
            Permissions::empty(),
        );
        assert!(missing(&all, &[ROLE], &[everyone_deny, member_allow]).is_empty());
        let hidden = overwrite(
            GUILD,
            PermissionOverwriteType::Role,
            Permissions::empty(),
            Permissions::VIEW_CHANNEL,
        );
        assert_eq!(missing(&all, &[ROLE], &[hidden]), ["view_channel"]);
        assert_eq!(
            missing(
                &all,
                &[ROLE],
                &[overwrite(
                    GUILD,
                    PermissionOverwriteType::Role,
                    Permissions::empty(),
                    Permissions::EMBED_LINKS | Permissions::ATTACH_FILES
                )]
            ),
            ["embed_links", "attach_files"]
        );
        assert_eq!(missing(&all, &[], &[]), Vec::<String>::new());
        assert_eq!(missing_post_permissions(GUILD, BOT, &all, None, &[]), ["view_channel"]);
    }

    #[test]
    fn guild_info_serializes_channel_missing_permissions() {
        let info = GuildInfo {
            channels: vec![ChannelInfo {
                id: "1".into(),
                name: "news".into(),
                kind: 5,
                parent_id: None,
                position: 0,
                missing_permissions: vec!["send_messages".into()],
                posting_permissions_known: true,
            }],
            roles: vec![],
            bot: BotInfo {
                top_role_position: None,
                manage_roles: false,
            },
            you: None,
        };
        let json = serde_json::to_value(&info).unwrap();
        assert_eq!(
            json["channels"][0]["missing_permissions"],
            serde_json::json!(["send_messages"])
        );
        assert_eq!(
            serde_json::from_value::<GuildInfo>(json).unwrap().channels[0].missing_permissions,
            ["send_messages"]
        );
    }
}
