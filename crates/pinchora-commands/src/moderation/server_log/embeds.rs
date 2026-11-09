use super::*;

pub(super) fn joined_embed(lang: Lang, user: &User) -> Embed {
    let title = if user.bot {
        t!(lang, "log.entry.bot-joined")
    } else {
        crate::moderation::server_log::title(lang, LogEvent::MemberJoin)
    };
    entry(lang, title, SUCCESS, user.id.get(), Some(user), who(user))
        .field(EmbedFieldBuilder::new(
            t!(lang, "log.entry.account-created"),
            relative(snowflake::timestamp(user.id.get())),
        ))
        .build()
}

pub(super) fn left_embed(lang: Lang, user: &User) -> Embed {
    let title = if user.bot {
        t!(lang, "log.entry.bot-left")
    } else {
        crate::moderation::server_log::title(lang, LogEvent::MemberLeave)
    };
    entry(lang, title, DANGER, user.id.get(), Some(user), who(user)).build()
}

pub(super) fn ban_embed(lang: Lang, user: &User, banned: bool, by: Option<&Attribution>) -> Embed {
    let (event, color) = if banned {
        (LogEvent::Ban, DANGER)
    } else {
        (LogEvent::Unban, SUCCESS)
    };
    let mut embed = entry(lang, title(lang, event), color, user.id.get(), Some(user), who(user));
    if let Some(by) = by {
        embed = embed.field(EmbedFieldBuilder::new(t!(lang, "log.by"), mention(by.actor)).inline());
        if let Some(reason) = &by.reason {
            embed = embed.field(EmbedFieldBuilder::new(t!(lang, "log.reason"), clip(reason, 1_000)));
        }
    }
    embed.build()
}

pub(super) fn names_embed(lang: Lang, user: &User, changes: &[&NameChange]) -> Embed {
    let name = |change: &NameChange| t!(lang, &format!("log.name.{}", change.what));
    let title = match changes {
        [one] => t!(lang, "log.entry.changed", what = name(one)),
        _ => t!(lang, "log.entry.names-changed"),
    };
    let mut embed = entry(lang, title, WARN, user.id.get(), Some(user), who(user));
    for change in changes {
        embed = embed.field(EmbedFieldBuilder::new(name(change), change.text(lang)));
    }
    embed.build()
}

pub(super) fn avatar_embed(lang: Lang, user: &User) -> Embed {
    let title = title(lang, LogEvent::Avatar);
    let mut embed = entry(lang, title, BRAND, user.id.get(), None, who(user));
    if let Ok(image) = ImageSource::url(avatar_url(user)) {
        embed = embed.image(image);
    }
    embed.build()
}

pub(super) fn voice_embed(lang: Lang, user_id: u64, user: Option<&User>, change: &VoiceChange) -> Embed {
    let member = mention(user_id);
    let at = |c: &u64| format!("<#{c}>");
    let (title, color, text) = match change {
        VoiceChange::Joined(channel) => (
            crate::moderation::server_log::title(lang, LogEvent::VoiceJoin),
            SUCCESS,
            t!(lang, "log.entry.joined-voice", member = member, channel = at(channel)),
        ),
        VoiceChange::Left(channel) => (
            crate::moderation::server_log::title(lang, LogEvent::VoiceLeave),
            DANGER,
            t!(lang, "log.entry.left-voice", member = member, channel = at(channel)),
        ),
        VoiceChange::Moved { from, to } => (
            crate::moderation::server_log::title(lang, LogEvent::VoiceMove),
            WARN,
            t!(
                lang,
                "log.entry.moved-voice",
                member = member,
                from = at(from),
                to = at(to)
            ),
        ),
        VoiceChange::Server(channel, what, on) => {
            let key = format!("log.entry.server-{}{what}", if *on { "" } else { "un" });
            (
                t!(lang, "log.entry.server-voice"),
                WARN,
                t!(lang, &key, member = member, channel = at(channel)),
            )
        }
        VoiceChange::Own(channel, what, on) => {
            let key = format!("log.entry.self-{}{what}", if *on { "" } else { "un" });
            (
                t!(lang, "log.entry.own-voice"),
                BRAND,
                t!(lang, &key, member = member, channel = at(channel)),
            )
        }
    };
    entry(lang, title, color, user_id, user, text).build()
}
