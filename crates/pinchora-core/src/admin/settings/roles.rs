use super::*;

/// What decides whether the bot can hand a role out, and who may set it up to be handed out.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RoleFacts {
    /// `@everyone`, whose id is the server's.
    pub everyone: bool,
    /// Owned by an integration or a bot.
    pub managed: bool,
    pub position: i64,
    /// The role's permission bits, as Discord sends them.
    pub permissions: u64,
}

/// Permissions that run or police a server, as Discord's bits and the names its settings show. A role
/// holding any of them is only set up to be handed out by the server owner or an administrator.
pub const DANGEROUS_PERMISSIONS: [(u64, &str); 10] = [
    (1 << 3, "Administrator"),
    (1 << 5, "Manage Server"),
    (1 << 28, "Manage Roles"),
    (1 << 4, "Manage Channels"),
    (1 << 29, "Manage Webhooks"),
    (1 << 2, "Ban Members"),
    (1 << 1, "Kick Members"),
    (1 << 40, "Timeout Members"),
    (1 << 17, "Mention @everyone, @here and All Roles"),
    (1 << 13, "Manage Messages"),
];

/// The dangerous permissions among `bits`, by name, in [`DANGEROUS_PERMISSIONS`] order.
pub fn dangerous_permissions(bits: u64) -> Vec<&'static str> {
    DANGEROUS_PERMISSIONS
        .iter()
        .filter(|(bit, _)| bits & bit != 0)
        .map(|(_, name)| *name)
        .collect()
}

/// Catalog keys (`perm.<key>`) of [`DANGEROUS_PERMISSIONS`], in the same order.
const PERMISSION_KEYS: [&str; 10] = [
    "administrator",
    "manage-server",
    "manage-roles",
    "manage-channels",
    "manage-webhooks",
    "ban-members",
    "kick-members",
    "timeout-members",
    "mention-everyone",
    "manage-messages",
];

/// [`dangerous_permissions`] named the way Discord's own settings word them in `lang`.
pub fn dangerous_permissions_in(lang: Lang, bits: u64) -> Vec<String> {
    DANGEROUS_PERMISSIONS
        .iter()
        .zip(PERMISSION_KEYS)
        .filter(|((bit, _), _)| bits & bit != 0)
        .map(|(_, key)| t!(lang, &format!("perm.{key}")))
        .collect()
}

/// Who is setting a role up to be handed out (a level, reputation, store, join or panel role, or a
/// custom action): what decides which roles they may.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Grantor {
    pub owner: bool,
    /// Holds Administrator (the owner does too, as Discord counts it).
    pub administrator: bool,
    /// The position of their highest role (0 with none but `@everyone`); `None` when it could not be
    /// looked up.
    pub top_role: Option<i64>,
}

fn powers(lang: Lang, role: &RoleFacts) -> Option<String> {
    let names = dangerous_permissions_in(lang, role.permissions);
    match names.as_slice() {
        [] => None,
        [one] => Some(one.clone()),
        [first, second] => Some(t!(lang, "refusal.powers-two", first = first, second = second)),
        [first, ..] => Some(t!(lang, "refusal.powers-more", first = first, count = names.len() - 1)),
    }
}

/// Why `grantor` may not set `role` up to be handed out, or `None` when they may. A role with a
/// [dangerous permission](DANGEROUS_PERMISSIONS) needs the owner or an administrator; a role at or
/// above the grantor's own highest role needs the owner, as Discord's own role settings do.
/// `label` names the role in the sentence.
pub fn grant_refusal(role: RoleFacts, grantor: Grantor, label: &str) -> Option<String> {
    grant_refusal_in(Lang::En, role, grantor, label)
}

/// [`grant_refusal`] worded in `lang`.
pub fn grant_refusal_in(lang: Lang, role: RoleFacts, grantor: Grantor, label: &str) -> Option<String> {
    if !(grantor.owner || grantor.administrator)
        && let Some(powers) = powers(lang, &role)
    {
        return Some(t!(lang, "refusal.grant-powers", label = label, powers = powers));
    }
    hierarchy_refusal(lang, role, grantor, label)
}

fn hierarchy_refusal(lang: Lang, role: RoleFacts, grantor: Grantor, label: &str) -> Option<String> {
    if grantor.owner {
        return None;
    }
    match grantor.top_role {
        Some(top) if role.position < top => None,
        Some(_) => Some(t!(lang, "refusal.not-below", label = label)),
        None => Some(t!(lang, "refusal.roles-unchecked")),
    }
}

/// [`grant_refusal`] for a role a custom command or auto-responder hands out. Any member sets those off
/// whenever they like, so a role with a dangerous permission is refused whoever writes it, and the bot
/// checks it again every time it runs ([`run_time_refusal`]).
pub fn action_role_refusal(role: RoleFacts, grantor: Grantor, label: &str) -> Option<String> {
    action_role_refusal_in(Lang::En, role, grantor, label)
}

/// [`action_role_refusal`] worded in `lang`.
pub fn action_role_refusal_in(lang: Lang, role: RoleFacts, grantor: Grantor, label: &str) -> Option<String> {
    run_time_refusal_in(lang, role.permissions, label).or_else(|| hierarchy_refusal(lang, role, grantor, label))
}

/// Why a custom action may not hand out a role with these permissions now, or `None` when it may:
/// permissions can change after the action was written.
pub fn run_time_refusal(permissions: u64, label: &str) -> Option<String> {
    run_time_refusal_in(Lang::En, permissions, label)
}

/// [`run_time_refusal`] worded in `lang`.
pub fn run_time_refusal_in(lang: Lang, permissions: u64, label: &str) -> Option<String> {
    let role = RoleFacts {
        everyone: false,
        managed: false,
        position: 0,
        permissions,
    };
    powers(lang, &role).map(|powers| t!(lang, "refusal.run-time", label = label, powers = powers))
}

/// Why the bot cannot hand out a role, or `None` when it can. `bot_top` is the position of the
/// bot's highest role (`None` when it could not be looked up); `label` names the role in the
/// sentence (a mention in Discord, `@name` on the web).
pub fn role_refusal(role: RoleFacts, bot_top: Option<i64>, label: &str) -> Option<String> {
    role_refusal_in(Lang::En, role, bot_top, label)
}

/// [`role_refusal`] worded in `lang`.
pub fn role_refusal_in(lang: Lang, role: RoleFacts, bot_top: Option<i64>, label: &str) -> Option<String> {
    if role.everyone {
        return Some(t!(lang, "refusal.everyone"));
    }
    if role.managed {
        return Some(t!(lang, "refusal.managed", label = label));
    }
    match bot_top {
        Some(top) if role.position < top => None,
        Some(_) => Some(t!(lang, "refusal.above-bot", label = label)),
        None => Some(t!(lang, "refusal.bot-unchecked")),
    }
}
