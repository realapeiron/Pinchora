//! What temp voice asks of Discord: making, changing and deleting a room's channel, moving members, and
//! the permission overwrites that lock a room, hide it and let members in or keep them out.

use pinchora_core::infra::i18n::Lang;
use pinchora_core::t;
use twilight_http::{Error, error::ErrorType};
use twilight_model::{
    channel::{
        ChannelType,
        permission_overwrite::{PermissionOverwrite, PermissionOverwriteType},
    },
    guild::Permissions,
    http::permission_overwrite::{PermissionOverwrite as SendOverwrite, PermissionOverwriteType as SendKind},
    id::{
        Id,
        marker::{ChannelMarker, GenericMarker},
    },
};

use crate::App;
use crate::roles::roles;

const UNKNOWN_CHANNEL: u64 = 10003;
const UNKNOWN_MEMBER: u64 = 10007;
const MISSING_ACCESS: u64 = 50001;
const MISSING_PERMISSIONS: u64 = 50013;
const NOT_IN_VOICE: u64 = 40032;
const MAX_CHANNELS: u64 = 30013;
const INVALID_FORM: u64 = 50035;

/// What stands in the way of a change to a room.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Fail {
    /// The bot lacks this permission (named as Discord names it).
    Permission(&'static str),
    /// The channel or the member is gone.
    Gone,
    /// The member to move is not in voice.
    NotInVoice,
    /// The category already holds Discord's 50 channels.
    CategoryFull,
    /// The server already has Discord's 500 channels.
    ServerFull,
    /// Discord did not answer, or refused for another reason.
    Other,
}

impl Fail {
    /// What Discord's refusal means for a call that needs `needs`.
    pub(crate) fn of(error: &Error, needs: &'static str) -> Self {
        match roles::error_code(error) {
            Some(MISSING_ACCESS | MISSING_PERMISSIONS) => Self::Permission(needs),
            Some(UNKNOWN_CHANNEL | UNKNOWN_MEMBER) => Self::Gone,
            Some(NOT_IN_VOICE) => Self::NotInVoice,
            _ => {
                if matches!(error.kind(), ErrorType::Response { .. }) {
                    tracing::debug!(%error, "temp voice: Discord refused a call");
                } else {
                    tracing::warn!(%error, "temp voice: Discord could not be reached");
                }
                Self::Other
            }
        }
    }

    /// A sentence for the member (or manager) who asked.
    pub(crate) fn text(self, lang: Lang) -> String {
        match self {
            Self::Permission(permission) => t!(
                lang,
                "voice.fail-permission",
                permission = permission_label(lang, permission)
            ),
            Self::Gone => t!(lang, "voice.fail-gone"),
            Self::NotInVoice => t!(lang, "voice.fail-not-in-voice"),
            Self::CategoryFull => t!(lang, "voice.fail-category-full"),
            Self::ServerFull => t!(lang, "voice.fail-server-full"),
            Self::Other => t!(lang, "voice.fail-other"),
        }
    }
}

pub(crate) const MANAGE_CHANNELS: &str = "Manage Channels";
pub(crate) const MANAGE_ROLES: &str = "Manage Roles";
pub(crate) const MOVE_MEMBERS: &str = "Move Members";
pub(crate) const VIEW_CHANNEL: &str = "View Channel";

/// Discord's permission names in the language of the reply.
pub(crate) fn permission_label(lang: Lang, permission: &str) -> String {
    match permission {
        MANAGE_CHANNELS => t!(lang, "voice.permission.manage-channels"),
        MANAGE_ROLES => t!(lang, "voice.permission.manage-roles"),
        MOVE_MEMBERS => t!(lang, "voice.permission.move-members"),
        VIEW_CHANNEL => t!(lang, "voice.permission.view-channel"),
        _ => permission.to_owned(),
    }
}

// /Overwrites

/// One permission overwrite of a channel, for the pure edits below.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Ow {
    pub id: u64,
    pub member: bool,
    pub allow: Permissions,
    pub deny: Permissions,
}

impl Ow {
    fn from_channel(found: &PermissionOverwrite) -> Option<Self> {
        let member = match found.kind {
            PermissionOverwriteType::Member => true,
            PermissionOverwriteType::Role => false,
            _ => return None,
        };
        Some(Self {
            id: found.id.get(),
            member,
            allow: found.allow,
            deny: found.deny,
        })
    }

    fn to_channel(self) -> PermissionOverwrite {
        PermissionOverwrite {
            allow: self.allow,
            deny: self.deny,
            id: Id::<GenericMarker>::new(self.id),
            kind: if self.member {
                PermissionOverwriteType::Member
            } else {
                PermissionOverwriteType::Role
            },
        }
    }

    fn to_send(self) -> SendOverwrite {
        SendOverwrite {
            allow: Some(self.allow),
            deny: Some(self.deny),
            id: Id::<GenericMarker>::new(self.id),
            kind: if self.member { SendKind::Member } else { SendKind::Role },
        }
    }
}

/// The overwrite for `id` in `list`, made first if there is none, changed by `change`. An overwrite that
/// ends up with nothing in it is left out.
pub(crate) fn edit(list: &mut Vec<Ow>, id: u64, member: bool, change: impl FnOnce(&mut Ow)) -> Ow {
    let at = match list.iter().position(|o| o.id == id && o.member == member) {
        Some(at) => at,
        None => {
            list.push(Ow {
                id,
                member,
                allow: Permissions::empty(),
                deny: Permissions::empty(),
            });
            list.len() - 1
        }
    };
    change(&mut list[at]);
    let done = list[at];
    if done.allow.is_empty() && done.deny.is_empty() {
        list.remove(at);
    }
    done
}

/// `bits` allowed: also taken off the denied ones.
pub(crate) fn allow(o: &mut Ow, bits: Permissions) {
    o.allow.insert(bits);
    o.deny.remove(bits);
}

/// `bits` denied: also taken off the allowed ones.
pub(crate) fn deny(o: &mut Ow, bits: Permissions) {
    o.deny.insert(bits);
    o.allow.remove(bits);
}

/// `bits` neither allowed nor denied.
pub(crate) fn clear(o: &mut Ow, bits: Permissions) {
    o.allow.remove(bits);
    o.deny.remove(bits);
}

/// What a member needs in a room whatever the server's categories say.
pub(crate) fn entry_bits() -> Permissions {
    Permissions::VIEW_CHANNEL | Permissions::CONNECT
}

// /Calls

/// The overwrites a channel has right now.
pub(crate) async fn overwrites_of(app: &App, channel: u64, needs: &'static str) -> Result<Vec<Ow>, Fail> {
    let found = app
        .http
        .channel(Id::new(channel.max(1)))
        .await
        .map_err(|e| Fail::of(&e, needs))?
        .model()
        .await
        .map_err(|_| Fail::Other)?;
    Ok(found
        .permission_overwrites
        .unwrap_or_default()
        .iter()
        .filter_map(Ow::from_channel)
        .collect())
}

/// The category a channel sits under, if any.
pub(crate) async fn parent_of(app: &App, channel: u64) -> Result<Option<u64>, Fail> {
    let found = app
        .http
        .channel(Id::new(channel.max(1)))
        .await
        .map_err(|e| Fail::of(&e, VIEW_CHANNEL))?
        .model()
        .await
        .map_err(|_| Fail::Other)?;
    Ok(found.parent_id.map(|p| p.get()))
}

/// What a category says about one permission bit for a role: denied, allowed, or nothing. A room that is
/// unlocked or shown again goes back to this, not to "nothing", so a private category stays private.
pub(crate) fn default_op(category: &[Ow], role: u64, bit: Permissions) -> Op {
    match category.iter().find(|o| o.id == role && !o.member) {
        Some(o) if o.deny.contains(bit) => Op::Deny,
        Some(o) if o.allow.contains(bit) => Op::Allow,
        _ => Op::Clear,
    }
}

/// Writes one overwrite of a channel.
pub(crate) async fn put_overwrite(app: &App, channel: u64, overwrite: Ow) -> Result<(), Fail> {
    let found = overwrite.to_send();
    app.http
        .update_channel_permission(Id::new(channel.max(1)), &found)
        .await
        .map(|_| ())
        .map_err(|e| Fail::of(&e, MANAGE_ROLES))
}

/// Takes one overwrite off a channel.
pub(crate) async fn drop_overwrite(app: &App, channel: u64, id: u64, member: bool) -> Result<(), Fail> {
    let request = app.http.delete_channel_permission(Id::new(channel.max(1)));
    let sent = if member {
        request.member(Id::new(id.max(1))).await
    } else {
        request.role(Id::new(id.max(1))).await
    };
    sent.map(|_| ()).map_err(|e| Fail::of(&e, MANAGE_ROLES))
}

/// What to do to the bits of one overwrite.
#[derive(Debug, Clone, Copy)]
pub(crate) enum Op {
    Allow,
    Deny,
    Clear,
}

/// One change to a channel's overwrite for a role or a member.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Edit {
    pub id: u64,
    pub member: bool,
    pub op: Op,
    pub bits: Permissions,
}

impl Edit {
    pub(crate) fn role(id: u64, op: Op, bits: Permissions) -> Self {
        Self {
            id,
            member: false,
            op,
            bits,
        }
    }

    pub(crate) fn member(id: u64, op: Op, bits: Permissions) -> Self {
        Self {
            id,
            member: true,
            op,
            bits,
        }
    }

    pub(crate) fn apply(self, o: &mut Ow) {
        match self.op {
            Op::Allow => allow(o, self.bits),
            Op::Deny => deny(o, self.bits),
            Op::Clear => clear(o, self.bits),
        }
    }
}

/// Applies `edits` to a channel's overwrites as they are now and writes what changed: one that ends up
/// with nothing in it is deleted.
pub(crate) async fn apply(app: &App, channel: u64, edits: &[Edit]) -> Result<(), Fail> {
    let tx = app.db.lock_temp_room_mutation(channel).await.map_err(|_| Fail::Other)?;
    apply_unlocked(app, channel, edits).await?;
    tx.commit().await.map_err(|_| Fail::Other)?;
    Ok(())
}

pub(crate) async fn apply_state(
    app: &App,
    channel: u64,
    edits: &[Edit],
    locked: Option<bool>,
    hidden: Option<bool>,
) -> Result<(), Fail> {
    let tx = app.db.lock_temp_room_mutation(channel).await.map_err(|_| Fail::Other)?;
    apply_unlocked(app, channel, edits).await?;
    app.db
        .set_room_state(channel, locked, hidden, None)
        .await
        .map_err(|_| Fail::Other)?;
    tx.commit().await.map_err(|_| Fail::Other)?;
    Ok(())
}

async fn apply_unlocked(app: &App, channel: u64, edits: &[Edit]) -> Result<(), Fail> {
    let mut list = overwrites_of(app, channel, MANAGE_ROLES).await?;
    for edit_one in edits {
        let before = list
            .iter()
            .find(|o| o.id == edit_one.id && o.member == edit_one.member)
            .copied();
        let done = edit(&mut list, edit_one.id, edit_one.member, |o| edit_one.apply(o));
        if before == Some(done) || (before.is_none() && done.allow.is_empty() && done.deny.is_empty()) {
            continue;
        }
        if done.allow.is_empty() && done.deny.is_empty() {
            match drop_overwrite(app, channel, edit_one.id, edit_one.member).await {
                Ok(()) | Err(Fail::Gone) => {}
                Err(other) => return Err(other),
            }
        } else {
            put_overwrite(app, channel, done).await?;
        }
    }
    Ok(())
}

/// Makes a voice channel. `overwrites` are written as the channel's own.
pub(crate) async fn make_channel(
    app: &App,
    guild: u64,
    name: &str,
    parent: Option<u64>,
    limit: i32,
    overwrites: &[Ow],
) -> Result<u64, Fail> {
    let sent: Vec<PermissionOverwrite> = overwrites.iter().map(|o| o.to_channel()).collect();
    let mut request = app
        .http
        .create_guild_channel(Id::new(guild.max(1)), name)
        .kind(ChannelType::GuildVoice)
        .permission_overwrites(sent.as_slice());
    if let Some(parent) = parent {
        request = request.parent_id(Id::new(parent.max(1)));
    }
    if limit > 0 {
        request = request.user_limit(limit as u16);
    }
    let made = request
        .await
        .map_err(|e| match roles::error_code(&e) {
            Some(MAX_CHANNELS) => Fail::ServerFull,
            // Everything else about the new channel was checked here, so a form Discord finds invalid with a
            // category in it means the category is full.
            Some(INVALID_FORM) if parent.is_some() => Fail::CategoryFull,
            _ => Fail::of(&e, MANAGE_CHANNELS),
        })?
        .model()
        .await
        .map_err(|_| Fail::Other)?;
    Ok(made.id.get())
}

/// Renames a channel and/or changes how many people it takes.
pub(crate) async fn edit_channel(app: &App, channel: u64, name: Option<&str>, limit: Option<i32>) -> Result<(), Fail> {
    let mut request = app.http.update_channel(Id::new(channel.max(1)));
    if let Some(name) = name {
        request = request.name(name);
    }
    if let Some(limit) = limit {
        request = request.user_limit(limit.clamp(0, 99) as u16);
    }
    request.await.map(|_| ()).map_err(|e| Fail::of(&e, MANAGE_CHANNELS))
}

/// Deletes a channel (or thread). One that is gone already counts as deleted.
pub(crate) async fn delete_channel(app: &App, channel: u64) -> Result<(), Fail> {
    match app.http.delete_channel(Id::<ChannelMarker>::new(channel.max(1))).await {
        Ok(_) => Ok(()),
        Err(error) => match Fail::of(&error, MANAGE_CHANNELS) {
            Fail::Gone => Ok(()),
            other => Err(other),
        },
    }
}

/// Moves a member to a voice channel (`Some`) or disconnects them (`None`).
pub(crate) async fn move_member(app: &App, guild: u64, user: u64, to: Option<u64>) -> Result<(), Fail> {
    app.http
        .update_guild_member(Id::new(guild.max(1)), Id::new(user.max(1)))
        .channel_id(to.map(|c| Id::new(c.max(1))))
        .await
        .map(|_| ())
        .map_err(|e| Fail::of(&e, MOVE_MEMBERS))
}

#[cfg(test)]
mod tests {
    use crate::commands::admin::tempvoice::rooms::*;

    #[test]
    fn locking_changes_only_the_connect_bit_and_keeps_the_rest() {
        let mut list = vec![Ow {
            id: 1,
            member: false,
            allow: Permissions::SEND_MESSAGES,
            deny: Permissions::STREAM,
        }];
        let done = edit(&mut list, 1, false, |o| deny(o, Permissions::CONNECT));
        assert_eq!(done.allow, Permissions::SEND_MESSAGES);
        assert_eq!(done.deny, Permissions::STREAM | Permissions::CONNECT);
        let done = edit(&mut list, 1, false, |o| clear(o, Permissions::CONNECT));
        assert_eq!(done.deny, Permissions::STREAM);
        assert_eq!(list.len(), 1);
    }

    #[test]
    fn a_room_goes_back_to_what_its_category_says() {
        let category = [Ow {
            id: 9,
            member: false,
            allow: Permissions::VIEW_CHANNEL,
            deny: Permissions::CONNECT,
        }];
        assert!(matches!(default_op(&category, 9, Permissions::CONNECT), Op::Deny));
        assert!(matches!(default_op(&category, 9, Permissions::VIEW_CHANNEL), Op::Allow));
        assert!(matches!(default_op(&category, 9, Permissions::SPEAK), Op::Clear));
        assert!(matches!(default_op(&category, 8, Permissions::CONNECT), Op::Clear));
        assert!(matches!(default_op(&[], 9, Permissions::CONNECT), Op::Clear));
    }

    #[test]
    fn an_empty_overwrite_is_dropped_and_allow_beats_an_old_deny() {
        let mut list = Vec::new();
        edit(&mut list, 5, true, |o| deny(o, Permissions::CONNECT));
        assert_eq!(list.len(), 1);
        let done = edit(&mut list, 5, true, |o| allow(o, entry_bits()));
        assert!(done.deny.is_empty() && done.allow.contains(Permissions::CONNECT));
        let done = edit(&mut list, 5, true, |o| clear(o, entry_bits()));
        assert!(done.allow.is_empty() && done.deny.is_empty());
        assert!(list.is_empty(), "nothing left, nothing kept");
        // A role and a member with the same id are two overwrites.
        edit(&mut list, 7, true, |o| allow(o, Permissions::CONNECT));
        edit(&mut list, 7, false, |o| deny(o, Permissions::CONNECT));
        assert_eq!(list.len(), 2);
    }
}
