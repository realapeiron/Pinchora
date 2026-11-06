//! `/voice`: a member runs their own temp room (lock, limit, rename, kick, transfer, claim, hide, invite and the
//! panel), and managers set up the creator channels under `/voice creators`. The panel's buttons, its pickers
//! and its two forms land here too.

use std::str::Split;

use pinchora_core::{
    modules::Module,
    t,
    temp_voice::{DEFAULT_TEMPLATE, MAX_CREATORS, check_limit, check_per_user, check_template},
};
use pinchora_db::{CreatorSave, TempRoom, VoiceCreator};
use twilight_interactions::command::{CommandModel, CreateCommand, ResolvedUser};
use twilight_model::{
    application::interaction::InteractionChannel,
    channel::{
        ChannelType,
        message::{
            Component, MessageFlags,
            component::{ActionRow, Label, SelectMenu, SelectMenuType, TextInput, TextInputStyle},
        },
    },
    http::interaction::InteractionResponseData,
};

use super::{
    control::{self, Act, Who},
    panel, rooms, setups,
};
use crate::{Cx, commands::games::quiet, server_events::tell};

#[derive(CommandModel, CreateCommand)]
#[command(
    name = "voice",
    desc = "Run your temp voice room: lock it, rename it, kick people",
    contexts = "guild"
)]
#[allow(clippy::large_enum_variant)] // parsed once per interaction, never stored
pub enum VoiceCommand {
    #[command(name = "lock")]
    Lock(VoiceLock),
    #[command(name = "unlock")]
    Unlock(VoiceUnlock),
    #[command(name = "limit")]
    Limit(VoiceLimit),
    #[command(name = "rename")]
    Rename(VoiceRename),
    #[command(name = "kick")]
    Kick(VoiceKick),
    #[command(name = "transfer")]
    Transfer(VoiceTransfer),
    #[command(name = "claim")]
    Claim(VoiceClaim),
    #[command(name = "hide")]
    Hide(VoiceHide),
    #[command(name = "show")]
    Show(VoiceShow),
    #[command(name = "invite")]
    Invite(VoiceInvite),
    #[command(name = "panel")]
    Panel(VoicePanel),
    #[command(name = "creators")]
    Creators(VoiceCreators),
}

#[derive(CommandModel, CreateCommand)]
#[command(name = "lock", desc = "Keep new people out of your room")]
pub struct VoiceLock;

#[derive(CommandModel, CreateCommand)]
#[command(name = "unlock", desc = "Let anyone join your room again")]
pub struct VoiceUnlock;

#[derive(CommandModel, CreateCommand)]
#[command(name = "limit", desc = "Set how many people your room takes")]
pub struct VoiceLimit {
    /// How many people at most (0 for no limit)
    #[command(min_value = 0, max_value = 99)]
    people: i64,
}

#[derive(CommandModel, CreateCommand)]
#[command(name = "rename", desc = "Rename your room")]
pub struct VoiceRename {
    /// The new name
    #[command(min_length = 1, max_length = 100)]
    name: String,
}

#[derive(CommandModel, CreateCommand)]
#[command(name = "kick", desc = "Send someone out of your room and keep them out")]
pub struct VoiceKick {
    /// Who to kick
    user: ResolvedUser,
}

#[derive(CommandModel, CreateCommand)]
#[command(name = "transfer", desc = "Give your room to someone who is in it")]
pub struct VoiceTransfer {
    /// The new owner
    user: ResolvedUser,
}

#[derive(CommandModel, CreateCommand)]
#[command(name = "claim", desc = "Take over a room whose owner has left")]
pub struct VoiceClaim;

#[derive(CommandModel, CreateCommand)]
#[command(name = "hide", desc = "Hide your room from people who are not in it")]
pub struct VoiceHide;

#[derive(CommandModel, CreateCommand)]
#[command(name = "show", desc = "Show your hidden room to everyone again")]
pub struct VoiceShow;

#[derive(CommandModel, CreateCommand)]
#[command(
    name = "invite",
    desc = "Let someone into your room, even if it is locked or they were kicked"
)]
pub struct VoiceInvite {
    /// Who to let in
    user: ResolvedUser,
}

#[derive(CommandModel, CreateCommand)]
#[command(name = "panel", desc = "Show the buttons that run your room")]
pub struct VoicePanel;

#[derive(CommandModel, CreateCommand)]
#[command(
    name = "creators",
    desc = "Channels that make a room for whoever joins (Manage Server)"
)]
#[allow(clippy::large_enum_variant)] // parsed once per interaction, never stored
pub enum VoiceCreators {
    #[command(name = "add")]
    Add(CreatorAdd),
    #[command(name = "remove")]
    Remove(CreatorRemove),
    #[command(name = "list")]
    List(CreatorList),
}

#[derive(CommandModel, CreateCommand)]
#[command(name = "add", desc = "Make a voice channel a creator, or change one (Manage Server)")]
pub struct CreatorAdd {
    /// The voice channel people join to get a room
    #[command(channel_types = "guild_voice")]
    channel: InteractionChannel,
    /// The category the rooms go in (default: the creator's own)
    #[command(channel_types = "guild_category")]
    category: Option<InteractionChannel>,
    /// How many people a room takes at first (0 for no limit)
    #[command(min_value = 0, max_value = 99)]
    limit: Option<i64>,
    /// What rooms are called: {user} is the owner's name, {n} the room's number
    #[command(min_length = 1, max_length = 80)]
    template: Option<String>,
}

#[derive(CommandModel, CreateCommand)]
#[command(name = "remove", desc = "Stop a channel making rooms (Manage Server)")]
pub struct CreatorRemove {
    /// The creator channel
    #[command(channel_types = "guild_voice")]
    channel: InteractionChannel,
}

#[derive(CommandModel, CreateCommand)]
#[command(name = "list", desc = "The creator channels of this server (Manage Server)")]
pub struct CreatorList;

impl VoiceCommand {
    pub async fn run(self, cx: &Cx) -> anyhow::Result<()> {
        let Some(guild) = cx.guild_id() else {
            return tell(cx, t!(cx.lang(), "error.guild-only")).await;
        };
        let act = match self {
            Self::Creators(creators) => return creators.run(cx, guild).await,
            Self::Panel(_) => return show_panel(cx, guild).await,
            Self::Lock(_) => Act::Lock,
            Self::Unlock(_) => Act::Unlock,
            Self::Hide(_) => Act::Hide,
            Self::Show(_) => Act::Show,
            Self::Claim(_) => Act::Claim,
            Self::Limit(c) => Act::Limit(c.people),
            Self::Rename(c) => Act::Rename(c.name),
            Self::Kick(c) => Act::Kick {
                user: c.user.resolved.id.get(),
                bot: c.user.resolved.bot,
            },
            Self::Transfer(c) if c.user.resolved.bot => {
                return tell(cx, t!(cx.lang(), "voice.transfer-bot")).await;
            }
            Self::Transfer(c) => Act::Transfer(c.user.resolved.id.get()),
            Self::Invite(c) => Act::Invite(c.user.resolved.id.get()),
        };
        run_act(cx, guild, act).await
    }
}

/// The room the member means, then `act` on it, then what to say.
async fn run_act(cx: &Cx, guild: u64, act: Act) -> anyhow::Result<()> {
    let who = who(cx)?;
    let Some(room) = control::resolve(&cx.app, guild, who.user, channel_of(cx)).await? else {
        return tell(cx, t!(cx.lang(), "voice.no-room")).await;
    };
    let text = control::act(&cx.app, &who, room, act).await?;
    tell(cx, text).await
}

fn who(cx: &Cx) -> anyhow::Result<Who> {
    Ok(Who {
        user: cx.author()?.id.get(),
        manager: cx.can_manage_guild(),
        lang: cx.lang(),
    })
}

/// The channel the interaction came from: where a panel in a room's chat (or thread) was pressed.
fn channel_of(cx: &Cx) -> Option<u64> {
    cx.interaction.channel.as_ref().map(|c| c.id.get())
}

/// `/voice panel`: the panel, for the member only.
async fn show_panel(cx: &Cx, guild: u64) -> anyhow::Result<()> {
    let who = who(cx)?;
    let Some(room) = control::resolve(&cx.app, guild, who.user, channel_of(cx)).await? else {
        return tell(cx, t!(cx.lang(), "voice.no-room")).await;
    };
    let (embed, components) = panel::message(cx.lang(), &room);
    cx.reply(InteractionResponseData {
        embeds: Some(vec![embed]),
        components: Some(components),
        flags: Some(MessageFlags::EPHEMERAL),
        allowed_mentions: quiet(),
        ..Default::default()
    })
    .await
}

// /Creators

impl VoiceCreators {
    async fn run(self, cx: &Cx, guild: u64) -> anyhow::Result<()> {
        if !cx.can_manage_guild() {
            return tell(cx, t!(cx.lang(), "error.needs-manage-server")).await;
        }
        match self {
            Self::Add(c) => add_creator(cx, guild, c).await,
            Self::Remove(c) => remove_creator(cx, guild, c).await,
            Self::List(_) => list_creators(cx, guild).await,
        }
    }
}

async fn add_creator(cx: &Cx, guild: u64, c: CreatorAdd) -> anyhow::Result<()> {
    let lang = cx.lang();
    if c.channel.kind != ChannelType::GuildVoice {
        return tell(
            cx,
            t!(
                lang,
                "voice.creators.not-voice",
                channel = format!("<#{}>", c.channel.id)
            ),
        )
        .await;
    }
    let channel = c.channel.id.get();
    let before = cx.app.db.voice_creator(guild, channel).await?;
    let template = match &c.template {
        Some(raw) => match check_template(raw) {
            Ok(template) => template,
            Err(_) => {
                return tell(
                    cx,
                    t!(
                        lang,
                        "voice.creators.template-invalid",
                        template = crate::custom::scrub_value(raw)
                    ),
                )
                .await;
            }
        },
        None => before
            .as_ref()
            .map_or_else(|| DEFAULT_TEMPLATE.to_owned(), |b| b.name_template.clone()),
    };
    let limit = match c.limit {
        Some(limit) => match check_limit(limit) {
            Ok(limit) => limit,
            Err(_) => return tell(cx, t!(lang, "voice.limit-nan")).await,
        },
        None => before.as_ref().map_or(0, |b| b.default_limit),
    };
    // Rooms go where the creator sits unless a category was named (or one was set before).
    let category = c
        .category
        .as_ref()
        .map(|cat| cat.id.get() as i64)
        .or_else(|| before.as_ref().and_then(|b| b.category_id))
        .or_else(|| c.channel.parent_id.map(|p| p.get() as i64));
    let creator = VoiceCreator {
        guild_id: guild as i64,
        channel_id: channel as i64,
        category_id: category,
        name_template: template,
        default_limit: limit,
        default_locked: before.as_ref().is_some_and(|b| b.default_locked),
        max_per_user: before.as_ref().map_or(1, |b| b.max_per_user),
        panel: before.as_ref().is_none_or(|b| b.panel),
        panel_channel_id: before.as_ref().and_then(|b| b.panel_channel_id),
        enabled: before.as_ref().is_none_or(|b| b.enabled),
    };
    if check_per_user(i64::from(creator.max_per_user)).is_err() {
        return tell(cx, t!(lang, "voice.creators.per-user-invalid")).await;
    }
    match cx.app.db.save_voice_creator(&creator).await? {
        CreatorSave::Saved(created) => {
            setups::forget(guild).await;
            cx.app.announce_guild_change(guild).await;
            let mut text = t!(
                lang,
                if created {
                    "voice.creators.added"
                } else {
                    "voice.creators.updated"
                },
                channel = format!("<#{channel}>"),
            );
            if let Some(category) = creator.category_id {
                text.push_str(&t!(
                    lang,
                    "voice.creators.category",
                    category = format!("<#{category}>")
                ));
            }
            if !cx.app.module_allowed(guild, None, Module::TempVoice).await {
                text.push_str(&t!(lang, "voice.creators.module-off"));
            }
            match super::missing_permissions(&cx.app, guild).await {
                Ok(missing) if !missing.is_empty() => {
                    let names = missing
                        .iter()
                        .map(|name| rooms::permission_label(lang, name))
                        .collect::<Vec<_>>()
                        .join(", ");
                    text.push_str(&t!(lang, "voice.creators.gaps", permissions = names));
                }
                Ok(_) => {}
                Err(error) => tracing::debug!(%error, guild, "temp voice: the bot's permissions were not read"),
            }
            tell(cx, text).await
        }
        CreatorSave::Full => tell(cx, t!(lang, "voice.creators.full", count = MAX_CREATORS)).await,
        CreatorSave::Taken => tell(cx, t!(lang, "voice.creators.taken", channel = format!("<#{channel}>"))).await,
    }
}

async fn remove_creator(cx: &Cx, guild: u64, c: CreatorRemove) -> anyhow::Result<()> {
    let lang = cx.lang();
    let channel = c.channel.id.get();
    let text = if cx.app.db.remove_voice_creator(guild, channel).await? {
        setups::forget(guild).await;
        t!(lang, "voice.creators.removed", channel = format!("<#{channel}>"))
    } else {
        t!(lang, "voice.creators.unknown", channel = format!("<#{channel}>"))
    };
    tell(cx, text).await
}

async fn list_creators(cx: &Cx, guild: u64) -> anyhow::Result<()> {
    let lang = cx.lang();
    let creators = cx.app.db.voice_creators(guild).await?;
    if creators.is_empty() {
        return tell(cx, t!(lang, "voice.creators.none")).await;
    }
    let rooms = cx.app.db.temp_rooms(guild).await?;
    let mut lines = vec![t!(
        lang,
        "voice.creators.head",
        count = creators.len(),
        max = MAX_CREATORS
    )];
    for creator in &creators {
        let alive = rooms.iter().filter(|r| r.source_id == creator.channel_id).count();
        let limit = if creator.default_limit > 0 {
            t!(lang, "voice.creators.limit", count = creator.default_limit)
        } else {
            t!(lang, "voice.creators.no-limit")
        };
        lines.push(t!(
            lang,
            "voice.creators.line",
            channel = format!("<#{}>", creator.channel_id),
            template = crate::custom::scrub_value(&creator.name_template),
            limit = limit,
            rooms = t!(lang, "voice.creators.rooms-now", count = alive),
            state = t!(
                lang,
                if creator.enabled {
                    "voice.creators.on"
                } else {
                    "voice.creators.off"
                }
            ),
        ));
    }
    tell(cx, lines.join("\n")).await
}

// /Buttons

/// `vc:<action>`: a panel button.
pub(crate) async fn on_button(cx: &Cx, mut parts: Split<'_, char>) -> anyhow::Result<()> {
    let Some(guild) = cx.guild_id() else {
        return tell(cx, t!(cx.lang(), "error.guild-only")).await;
    };
    let action = parts.next().unwrap_or_default();
    let lang = cx.lang();
    let who = who(cx)?;
    let Some(room) = control::resolve(&cx.app, guild, who.user, channel_of(cx)).await? else {
        return tell(cx, t!(lang, "voice.no-room")).await;
    };
    let act = match action {
        panel::LOCK => Act::Lock,
        panel::UNLOCK => Act::Unlock,
        panel::HIDE if room.hidden => Act::Show,
        panel::HIDE => Act::Hide,
        panel::CLAIM => Act::Claim,
        panel::LIMIT | panel::RENAME | panel::KICK | panel::TRANSFER => {
            return open_second_step(cx, &who, room, action).await;
        }
        _ => return tell(cx, t!(lang, "voice.expired")).await,
    };
    let text = control::act(&cx.app, &who, room, act).await?;
    tell(cx, text).await
}

/// The buttons that need more from the member: a number, a name, a person.
async fn open_second_step(cx: &Cx, who: &Who, room: TempRoom, action: &str) -> anyhow::Result<()> {
    let lang = cx.lang();
    if room.owner_id != Some(who.user as i64) && !who.manager {
        let text = match room.owner_id {
            Some(owner) => t!(lang, "voice.not-owner", owner = format!("<@{owner}>")),
            None => t!(lang, "voice.no-owner"),
        };
        return tell(cx, text).await;
    }
    let channel = room.channel_id;
    match action {
        panel::LIMIT => {
            cx.modal(
                format!("vc:{}:{channel}", panel::LIMIT),
                &t!(lang, "voice.form.limit-title"),
                vec![text_field(
                    "limit",
                    &t!(lang, "voice.form.limit-label"),
                    &t!(lang, "voice.form.limit-hint"),
                    Some(room.user_limit.to_string()),
                    2,
                )],
            )
            .await
        }
        panel::RENAME => {
            // A room renamed lately cannot be again for a while: say when instead of asking for a name.
            if let Some(until) = control::rename_wait(&cx.app, channel as u64).await? {
                return tell(cx, t!(lang, "voice.rename-wait", when = format!("<t:{until}:R>"))).await;
            }
            cx.modal(
                format!("vc:{}:{channel}", panel::RENAME),
                &t!(lang, "voice.form.rename-title"),
                vec![text_field(
                    "name",
                    &t!(lang, "voice.form.rename-label"),
                    &t!(lang, "voice.form.rename-hint"),
                    None,
                    100,
                )],
            )
            .await
        }
        panel::KICK | panel::TRANSFER => {
            let (id, key) = if action == panel::KICK {
                (format!("vo:k:{channel}"), "voice.pick-kick")
            } else {
                (format!("vo:t:{channel}"), "voice.pick-transfer")
            };
            cx.reply(InteractionResponseData {
                content: Some(t!(lang, key)),
                components: Some(vec![Component::ActionRow(ActionRow {
                    id: None,
                    components: vec![Component::SelectMenu(SelectMenu {
                        channel_types: None,
                        custom_id: id,
                        default_values: None,
                        disabled: false,
                        id: None,
                        kind: SelectMenuType::User,
                        max_values: Some(1),
                        min_values: Some(1),
                        options: None,
                        placeholder: Some(t!(lang, "voice.pick-placeholder")),
                        required: None,
                    })],
                })]),
                flags: Some(MessageFlags::EPHEMERAL),
                allowed_mentions: quiet(),
                ..Default::default()
            })
            .await
        }
        _ => tell(cx, t!(lang, "voice.expired")).await,
    }
}

#[allow(deprecated)] // TextInput::label: the Label around it carries the label now
fn text_field(id: &str, label: &str, hint: &str, value: Option<String>, max: u16) -> Component {
    Component::Label(Label {
        component: Box::new(Component::TextInput(TextInput {
            custom_id: id.to_owned(),
            id: None,
            label: None,
            max_length: Some(max),
            min_length: Some(1),
            placeholder: None,
            required: Some(true),
            style: TextInputStyle::Short,
            value,
        })),
        description: Some(hint.to_owned()),
        id: None,
        label: label.to_owned(),
    })
}

// /Pickers

/// `vo:<k|t>:<room>`: the person picked for a kick or a transfer.
pub(crate) async fn on_select(cx: &Cx, mut parts: Split<'_, char>, values: &[String]) -> anyhow::Result<()> {
    let lang = cx.lang();
    let (Some(guild), Some(kind)) = (cx.guild_id(), parts.next()) else {
        return tell(cx, t!(lang, "voice.expired")).await;
    };
    let Some(channel) = parts.next().and_then(|c| c.parse::<u64>().ok()) else {
        return tell(cx, t!(lang, "voice.expired")).await;
    };
    let Some(picked) = values.first().and_then(|v| v.parse::<u64>().ok()) else {
        return tell(cx, t!(lang, "voice.expired")).await;
    };
    // The room's id came from the client: it counts only when it is a room of this server.
    let Some(room) = cx
        .app
        .db
        .temp_room(channel)
        .await?
        .filter(|r| r.guild_id as u64 == guild)
    else {
        return tell(cx, t!(lang, "voice.gone")).await;
    };
    let act = match kind {
        "k" => Act::Kick {
            user: picked,
            bot: false,
        },
        "t" => {
            // Interaction data is taken before component dispatch, so check the chosen user with Discord.
            // If Discord cannot identify them, fail closed instead of letting a bot own the room.
            let Some(user) = cx.app.user(picked).await else {
                return tell(cx, t!(lang, "voice.fail-other")).await;
            };
            if user.bot {
                return tell(cx, t!(lang, "voice.transfer-bot")).await;
            }
            Act::Transfer(picked)
        }
        _ => return tell(cx, t!(lang, "voice.expired")).await,
    };
    let text = control::act(&cx.app, &who(cx)?, room, act).await?;
    cx.update(InteractionResponseData {
        content: Some(text),
        components: Some(Vec::new()),
        allowed_mentions: quiet(),
        ..Default::default()
    })
    .await
}

// /Forms

/// `vc:<lm|rn>:<room>`: a submitted form.
pub(crate) async fn on_modal(cx: &Cx, mut parts: Split<'_, char>, fields: &[(String, String)]) -> anyhow::Result<()> {
    let lang = cx.lang();
    let (Some(guild), Some(action)) = (cx.guild_id(), parts.next()) else {
        return tell(cx, t!(lang, "voice.expired")).await;
    };
    let Some(channel) = parts.next().and_then(|c| c.parse::<u64>().ok()) else {
        return tell(cx, t!(lang, "voice.expired")).await;
    };
    let field = |name: &str| fields.iter().find(|(id, _)| id == name).map_or("", |(_, v)| v.as_str());
    let act = match action {
        panel::LIMIT => match field("limit").trim().parse::<i64>() {
            Ok(limit) => Act::Limit(limit),
            Err(_) => return tell(cx, t!(lang, "voice.limit-nan")).await,
        },
        panel::RENAME => Act::Rename(field("name").to_owned()),
        _ => return tell(cx, t!(lang, "voice.expired")).await,
    };
    let Some(room) = cx
        .app
        .db
        .temp_room(channel)
        .await?
        .filter(|r| r.guild_id as u64 == guild)
    else {
        return tell(cx, t!(lang, "voice.gone")).await;
    };
    let text = control::act(&cx.app, &who(cx)?, room, act).await?;
    tell(cx, text).await
}
