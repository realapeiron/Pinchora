//! `/privacy`: what other players can see of you, and "delete my data".

use std::str::Split;

use pinchora_db::{Hide, Privacy, TOMBSTONE_DAYS};
use twilight_interactions::command::{CommandModel, CreateCommand};
use twilight_model::{
    channel::message::{
        Component, MessageFlags,
        component::{ActionRow, Button, ButtonStyle},
    },
    http::interaction::InteractionResponseData,
};
use twilight_util::builder::embed::{EmbedBuilder, EmbedFooterBuilder};

use crate::{
    Cx,
    util::{BRAND, DANGER, SUCCESS, thousands},
};

/// How long the "yes, delete" button of the confirm step works, in seconds.
const CONFIRM_SECS: i64 = 120;

#[derive(CommandModel, CreateCommand)]
#[command(name = "privacy", desc = "Choose what others can see of you, or delete your data")]
pub struct PrivacyCommand;

impl PrivacyCommand {
    pub async fn run(self, cx: &Cx) -> anyhow::Result<()> {
        let data = panel(cx, cx.author()?.id.get(), "").await?;
        cx.reply(data).await
    }
}

/// Tells the invoker "This profile is private." and returns true when `target` is somebody else who
/// hides their profile. Used by every command that shows another player's card.
pub(crate) async fn private_profile(cx: &Cx, target: u64) -> anyhow::Result<bool> {
    if target == cx.author()?.id.get() || !cx.app.db.privacy(target).await?.hide_profile {
        return Ok(false);
    }
    cx.reply_ephemeral("This profile is private.").await?;
    Ok(true)
}

/// Tells the invoker "This house is private." and returns true when `owner` is somebody else who
/// hides their house. Used by everything that reads or writes another player's house apart from
/// `/house visit`, which says so by name.
pub(crate) async fn private_house(cx: &Cx, owner: u64) -> anyhow::Result<bool> {
    if owner == cx.author()?.id.get() || !cx.app.db.privacy(owner).await?.hide_house {
        return Ok(false);
    }
    cx.reply_ephemeral("This house is private.").await?;
    Ok(true)
}

fn button(label: &str, custom_id: String, style: ButtonStyle) -> Component {
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

fn row(components: Vec<Component>) -> Component {
    Component::ActionRow(ActionRow { id: None, components })
}

fn ephemeral(embed: twilight_model::channel::message::Embed, components: Vec<Component>) -> InteractionResponseData {
    InteractionResponseData {
        embeds: Some(vec![embed]),
        components: Some(components),
        flags: Some(MessageFlags::EPHEMERAL),
        ..Default::default()
    }
}

/// The panel: where each choice stands, a button for each, and the delete button.
async fn panel(cx: &Cx, user: u64, note: &str) -> anyhow::Result<InteractionResponseData> {
    let privacy = cx.app.db.privacy(user).await?;
    let state = |hidden: bool, on: &str, off: &str| {
        if hidden {
            format!("🔒 {on}")
        } else {
            format!("🌊 {off}")
        }
    };
    let mut description = String::new();
    if !note.is_empty() {
        description.push_str(note);
        description.push_str("\n\n");
    }
    description.push_str(&format!(
        "**Profile**\n{}\n\n**Global leaderboards**\n{}\n\n**House**\n{}",
        state(
            privacy.hide_profile,
            "Private. Others who look you up with `/profile`, `/rank` or `/balance` see \"This profile is private.\"",
            "Public. Anyone can look at your cards.",
        ),
        state(
            privacy.hide_leaderboard,
            "Hidden. You are left off the global XP and shell boards (server boards still list you).",
            "Listed on the global XP and shell boards.",
        ),
        state(
            privacy.hide_house,
            "Hidden. Not offered in `/house visit` and nobody can visit you, not even with your name (stronger than `/house lock`).",
            "Open. Friends find it in `/house visit`, unless you lock it.",
        ),
    ));
    let toggle = |label: &str, key: &str, on: bool| {
        button(
            label,
            format!("pr:t:{key}"),
            if on {
                ButtonStyle::Primary
            } else {
                ButtonStyle::Secondary
            },
        )
    };
    let embed = EmbedBuilder::new()
        .color(BRAND)
        .title("🔒 Privacy")
        .description(description)
        .footer(EmbedFooterBuilder::new(
            "Press a button to switch it · only you can see this",
        ))
        .build();
    Ok(ephemeral(
        embed,
        vec![
            row(vec![
                toggle(
                    if privacy.hide_profile {
                        "Profile: private"
                    } else {
                        "Profile: public"
                    },
                    "p",
                    privacy.hide_profile,
                ),
                toggle(
                    if privacy.hide_leaderboard {
                        "Boards: hidden"
                    } else {
                        "Boards: listed"
                    },
                    "l",
                    privacy.hide_leaderboard,
                ),
                toggle(
                    if privacy.hide_house {
                        "House: hidden"
                    } else {
                        "House: open"
                    },
                    "h",
                    privacy.hide_house,
                ),
            ]),
            row(vec![button("Delete my data…", "pr:d".to_owned(), ButtonStyle::Danger)]),
        ],
    ))
}

/// The second step: what deleting does, and a button that only works for a couple of minutes.
fn confirm(until: i64) -> InteractionResponseData {
    let embed = EmbedBuilder::new()
        .color(DANGER)
        .title("Delete all your data?")
        .description(format!(
            "This permanently removes everything Pinchora keeps about you: your level, shells and pearls, \
             house and furniture, pets, fish book, badges, titles and backgrounds, friends and partner, \
             guestbook notes you wrote or were left, votes, suggestions, polls, giveaway entries, game stats, \
             reminders and privacy choices.\n\n\
             **It cannot be undone.** You start over as a new player the next time you use Pinchora, with \
             no shells. Shells you had bet in a running duel go back to your opponent, and a giveaway you \
             host is called off.\n\n\
             Deleting does not reset waiting times: for {TOMBSTONE_DAYS} days Pinchora keeps your id and \
             when you last used your daily, weekly and monthly rewards, rep, the daily wordle, paid visits, \
             likes and pet walks, so they are still waiting when you come back."
        ))
        .build();
    ephemeral(
        embed,
        vec![row(vec![
            button("Yes, delete everything", format!("pr:y:{until}"), ButtonStyle::Danger),
            button("Keep my data", "pr:n".to_owned(), ButtonStyle::Secondary),
        ])],
    )
}

/// `pr:t:<p|l|h>` flips a choice, `pr:d` asks for confirmation, `pr:y:<until>` deletes, `pr:n` backs out.
pub(crate) async fn on_component(cx: &Cx, mut args: Split<'_, char>) -> anyhow::Result<()> {
    let user = cx.author()?.id.get();
    match args.next() {
        Some("t") => {
            let which = match args.next() {
                Some("p") => Hide::Profile,
                Some("l") => Hide::Leaderboard,
                Some("h") => Hide::House,
                _ => return cx.reply_ephemeral("This button has expired.").await,
            };
            let before: Privacy = cx.app.db.privacy(user).await?;
            let now_hidden = !match which {
                Hide::Profile => before.hide_profile,
                Hide::Leaderboard => before.hide_leaderboard,
                Hide::House => before.hide_house,
            };
            cx.app.db.set_privacy(user, which, now_hidden).await?;
            let data = panel(cx, user, "Saved.").await?;
            cx.update(data).await
        }
        Some("d") => cx.update(confirm(cx.app.now().timestamp() + CONFIRM_SECS)).await,
        Some("n") => {
            let data = panel(cx, user, "Nothing was deleted.").await?;
            cx.update(data).await
        }
        Some("y") => {
            let until: i64 = args.next().and_then(|v| v.parse().ok()).unwrap_or(0);
            if cx.app.now().timestamp() > until {
                let data = panel(cx, user, "That confirmation timed out. Nothing was deleted.").await?;
                return cx.update(data).await;
            }
            let deleted = cx.app.db.delete_user_data(user, cx.app.now()).await?;
            tracing::info!(
                user,
                rows = deleted.rows,
                tables = deleted.tables,
                "player data deleted"
            );
            let embed = EmbedBuilder::new()
                .color(SUCCESS)
                .title("Your data is deleted")
                .description(format!(
                    "Removed {} rows from {} tables. If you use Pinchora again, you start fresh.",
                    thousands(deleted.rows as i64),
                    deleted.tables
                ))
                .build();
            cx.update(ephemeral(embed, Vec::new())).await
        }
        _ => cx.reply_ephemeral("This button has expired.").await,
    }
}
