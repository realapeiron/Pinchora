//! Hug, pat, wave and high five: a short animated GIF of both players' pixel characters, drawn
//! from their real look and colours, and a count of how many each has sent.

use std::{str::Split, time::Duration};

use pinchora_db::Tally;
use pinchora_render::social::{Interaction, Person, interaction_gif};
use twilight_interactions::command::{CommandModel, CreateCommand, ResolvedUser};
use twilight_model::{channel::message::component::ButtonStyle, http::interaction::InteractionResponseData};
use twilight_util::builder::embed::{EmbedBuilder, EmbedFooterBuilder};

use super::social::{self, button, character_of, row};
use crate::{
    Cx,
    util::{BRAND, mention, thousands},
};

/// Time between two of one player's interactions.
const COOLDOWN: Duration = Duration::from_secs(5);

macro_rules! interaction_command {
    ($name:ident, $command:literal, $desc:literal, $kind:expr) => {
        #[derive(CommandModel, CreateCommand)]
        #[command(name = $command, desc = $desc)]
        pub struct $name {
            /// Who gets it
            user: ResolvedUser,
        }

        impl $name {
            pub async fn run(self, cx: &Cx) -> anyhow::Result<()> {
                send(cx, $kind, &self.user).await
            }
        }
    };
}

interaction_command!(Hug, "hug", "Give someone a hug", Interaction::Hug);
interaction_command!(Pat, "pat", "Give someone a pat on the head", Interaction::Pat);
interaction_command!(Wave, "wave", "Wave at someone", Interaction::Wave);
interaction_command!(HighFive, "highfive", "Give someone a high five", Interaction::HighFive);

async fn send(cx: &Cx, kind: Interaction, target: &ResolvedUser) -> anyhow::Result<()> {
    let from = cx.author()?.id.get();
    let to = &target.resolved;
    if to.bot {
        return cx
            .reply_ephemeral("Pick a person; bots keep their circuits to themselves.")
            .await;
    }
    if to.id.get() == from {
        return cx
            .reply_ephemeral(format!("You can't give yourself a {}. Pick someone else!", kind.noun()))
            .await;
    }
    if throttled(cx, from).await? {
        return Ok(());
    }
    social::remember(cx, to).await?;
    let data = deliver(cx, kind, from, to.id.get()).await?;
    cx.reply(data).await
}

/// Answers a player who is sending too fast; true when they are.
async fn throttled(cx: &Cx, user: u64) -> anyhow::Result<bool> {
    let left = cx
        .app
        .cache
        .try_cooldown(&format!("social:{user}"), COOLDOWN)
        .await
        .unwrap_or_else(|error| {
            tracing::warn!(%error, "social cooldown not checked");
            None
        });
    let Some(left) = left else {
        return Ok(false);
    };
    cx.reply_ephemeral(format!(
        "Easy there! You can send another in {} second{}.",
        left.as_secs() + 1,
        if left.as_secs() == 0 { "" } else { "s" }
    ))
    .await?;
    Ok(true)
}

/// Draws the GIF, counts the interaction and builds the message that carries both.
async fn deliver(cx: &Cx, kind: Interaction, from: u64, to: u64) -> anyhow::Result<InteractionResponseData> {
    let ((a, saved), (b, _)) = (character_of(&cx.app, from).await?, character_of(&cx.app, to).await?);
    let sprites = cx.app.sprites.clone();
    let gif = cx
        .app
        .render(move |_| {
            let (look_a, look_b) = (sprites.look(&a.look), sprites.look(&b.look));
            interaction_gif(
                kind,
                Person {
                    look: &look_a,
                    colors: a.colors,
                },
                Person {
                    look: &look_b,
                    colors: b.colors,
                },
            )
        })
        .await?;
    let tally = cx.app.db.record_interaction(kind.key(), from, to).await?;
    let (name_from, name_to) = (name(cx, from).await, name(cx, to).await);
    let footer = footer(kind, tally, &name_from, &name_to, saved);
    let embed = EmbedBuilder::new()
        .color(BRAND)
        .description(format!(
            "That's {} **#{}** from {} to {}.",
            kind.noun(),
            thousands(i64::from(tally.pair)),
            mention(from),
            mention(to)
        ))
        .footer(EmbedFooterBuilder::new(footer));
    let back = button(
        &format!("{} back", capitalized(kind.noun())),
        format!("hg:{}:{from}:{to}", kind.key()),
        ButtonStyle::Primary,
    );
    let mut data = Cx::card(embed, &format!("{}.gif", kind.key()), gif, vec![row(vec![back])], false);
    data.content = Some(format!(
        "{} {} {} {}",
        kind.emoji(),
        mention(from),
        kind.verb(),
        mention(to)
    ));
    Ok(data)
}

async fn name(cx: &Cx, user: u64) -> String {
    cx.app
        .db
        .remembered_name(user)
        .await
        .ok()
        .flatten()
        .filter(|n| !n.is_empty())
        .unwrap_or_else(|| "Someone".to_owned())
}

fn capitalized(text: &str) -> String {
    let mut chars = text.chars();
    chars
        .next()
        .map(|first| first.to_uppercase().chain(chars).collect())
        .unwrap_or_default()
}

fn plural(kind: Interaction) -> &'static str {
    match kind {
        Interaction::Hug => "hugs",
        Interaction::Pat => "pats",
        Interaction::Wave => "waves",
        Interaction::HighFive => "high fives",
    }
}

/// "Pip: 12 hugs given · Sam: 7 hugs received", and a nudge for a player who never designed a
/// character, whose stand-in is standing there.
fn footer(kind: Interaction, tally: Tally, from: &str, to: &str, saved: bool) -> String {
    let count = |n: i64| format!("{} {}", thousands(n), if n == 1 { kind.noun() } else { plural(kind) });
    let mut text = format!(
        "{from}: {} given · {to}: {} received",
        count(tally.given),
        count(tally.received)
    );
    if !saved {
        text.push_str(" · Design your own with /character");
    }
    text
}

/// `hg:<kind>:<from>:<to>`: the receiver sends one back.
pub(crate) async fn on_component(cx: &Cx, mut args: Split<'_, char>) -> anyhow::Result<()> {
    let kind = args.next().and_then(Interaction::from_key);
    let from: u64 = args.next().and_then(|v| v.parse().ok()).unwrap_or(0);
    let to: u64 = args.next().and_then(|v| v.parse().ok()).unwrap_or(0);
    let Some(kind) = kind.filter(|_| from != 0 && to != 0) else {
        return cx.reply_ephemeral("This button has expired.").await;
    };
    let me = cx.author()?.id.get();
    if me != to {
        return cx.reply_ephemeral(format!("This one is for {}.", mention(to))).await;
    }
    if throttled(cx, me).await? {
        return Ok(());
    }
    // The button has done its job: the message goes on without it, and the answer follows it.
    cx.update(InteractionResponseData {
        components: Some(Vec::new()),
        ..Default::default()
    })
    .await?;
    let data = deliver(cx, kind, me, from).await?;
    cx.reply(data).await
}
