//! What the partner, friends and interaction commands share: names, characters and the
//! Accept/Decline buttons of a request.

use std::str::Split;

use std::time::Duration;

use pinchora_game::{Content, character::Character};
use pinchora_render::character::{Channel, Colors, Paint};
use twilight_model::{
    channel::message::{
        AllowedMentions, Component,
        component::{ActionRow, Button, ButtonStyle},
    },
    id::Id,
    user::User,
};

use crate::{App, Cx, util::display_name};

/// How long one player must wait before asking the same player again (a friend or partner request).
const ASK_AGAIN: Duration = Duration::from_secs(60);

/// Allowed mentions that ping `user` and nobody else: what a message that names two players may do.
pub(super) fn ping_only(user: u64) -> Option<AllowedMentions> {
    Some(AllowedMentions {
        users: vec![Id::new(user.max(1))],
        ..Default::default()
    })
}

/// Starts the pair's cooldown for `kind` of request, or tells `cx` to wait and returns true. The request
/// pings the player it is for, so asking again and again would be a way to spam them.
pub(super) async fn asked_too_soon(cx: &Cx, kind: &str, from: u64, to: u64) -> anyhow::Result<bool> {
    let left = cx
        .app
        .cache
        .try_cooldown(&format!("ask:{kind}:{from}:{to}"), ASK_AGAIN)
        .await
        .unwrap_or_else(|error| {
            tracing::warn!(%error, "request cooldown not checked");
            None
        });
    let Some(left) = left else {
        return Ok(false);
    };
    cx.reply_ephemeral(format!(
        "You just asked {}. You can ask again in {} seconds.",
        crate::util::mention(to),
        left.as_secs() + 1
    ))
    .await?;
    Ok(true)
}

/// The name kept for lists and cards: the global name, or the username; never a server nickname,
/// which means nothing in another server.
pub(super) fn plain_name(user: &User) -> String {
    display_name(user, None)
}

/// Remembers the names of the player and of the other one in the command.
pub(super) async fn remember(cx: &Cx, other: &User) -> anyhow::Result<()> {
    let me = cx.author()?;
    cx.app.db.remember_name(me.id.get(), &plain_name(me)).await?;
    cx.app.db.remember_name(other.id.get(), &plain_name(other)).await?;
    Ok(())
}

/// A button with a custom id.
pub(super) fn button(label: &str, custom_id: String, style: ButtonStyle) -> Component {
    Component::Button(Button {
        custom_id: Some(custom_id),
        disabled: false,
        emoji: None,
        id: None,
        label: Some(label.to_owned()),
        sku_id: None,
        style,
        url: None,
    })
}

pub(super) fn row(components: Vec<Component>) -> Component {
    Component::ActionRow(ActionRow { id: None, components })
}

/// A request somebody sent another: who, to whom, and the Unix second it stops being valid.
/// It all rides in the button's custom id, so any pod can answer it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Request {
    pub(super) from: u64,
    pub(super) to: u64,
    pub(super) until: i64,
}

impl Request {
    /// `Accept` and `Decline` for the request: `<prefix>:yes|no:<from>:<to>:<until>`.
    pub(super) fn buttons(self, prefix: &str, accept: &str) -> Component {
        let id = |answer: &str| format!("{prefix}:{answer}:{}:{}:{}", self.from, self.to, self.until);
        row(vec![
            button(accept, id("yes"), ButtonStyle::Success),
            button("Decline", id("no"), ButtonStyle::Secondary),
        ])
    }

    pub(super) fn parse(args: &mut Split<'_, char>) -> Option<Self> {
        Some(Self {
            from: args.next()?.parse().ok()?,
            to: args.next()?.parse().ok()?,
            until: args.next()?.parse().ok()?,
        })
    }

    pub(super) fn expired(self, app: &App) -> bool {
        app.now().timestamp() > self.until
    }
}

/// The character a player designed, or one made from their id so players who never did still
/// stand there as somebody; the flag says whether it is theirs.
pub(super) async fn character_of(app: &App, user: u64) -> anyhow::Result<(Character, bool)> {
    let content = Content::get();
    let saved = app
        .db
        .character(user)
        .await?
        .and_then(|code| Character::decode(content, &code));
    Ok(match saved {
        Some(character) => (character, true),
        None => (stand_in(content, user), false),
    })
}

/// A look and colours picked from the id: the same for the same player, different between players.
fn stand_in(content: &Content, user: u64) -> Character {
    let mut h = user.wrapping_mul(0x9e37_79b9_7f4a_7c15);
    h ^= h >> 29;
    let mut pick = |len: usize| {
        h = h.wrapping_mul(0xbf58_476d_1ce4_e5b9).rotate_left(23) ^ user;
        (h >> 11) as usize % len
    };
    let look = content
        .looks
        .get(pick(content.looks.len().max(1)))
        .map_or_else(String::new, |l| l.id.clone());
    let mut paint = |channel: Channel| Paint::Swatch(pick(channel.swatches().len()) as u8);
    Character {
        look,
        colors: Colors {
            skin: paint(Channel::Skin),
            hair: paint(Channel::Hair),
            eyes: paint(Channel::Eyes),
            top: paint(Channel::Top),
            bottom: paint(Channel::Bottom),
            shoes: paint(Channel::Shoes),
        },
        accessories: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn requests_survive_the_custom_id() {
        let request = Request {
            from: 123_456_789_012_345_678,
            to: 987_654_321_098_765_432,
            until: 1_900_000_000,
        };
        let Component::ActionRow(row) = request.buttons("pt", "Accept") else {
            panic!("a row");
        };
        let Some(Component::Button(accept)) = row.components.first() else {
            panic!("a button");
        };
        let id = accept.custom_id.clone().expect("custom id");
        assert!(id.len() <= 100, "{id}");
        let mut parts = id.split(':');
        assert_eq!((parts.next(), parts.next()), (Some("pt"), Some("yes")));
        assert_eq!(Request::parse(&mut parts), Some(request));
        assert_eq!(Request::parse(&mut "1:2".split(':')), None);
        assert_eq!(Request::parse(&mut "1:x:3".split(':')), None);
    }

    #[test]
    fn players_without_a_character_still_stand_there_as_somebody() {
        let content = Content::get();
        let (a, b) = (stand_in(content, 1001), stand_in(content, 1002));
        assert_eq!(a, stand_in(content, 1001), "the same every time");
        assert_ne!(a, b, "different between players");
        assert!(
            Character::decode(content, &a.encode()).is_some(),
            "a real look and real colours"
        );
    }
}
