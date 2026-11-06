//! `/privacy`: what other players can see of you, whether servers may send you direct messages, and
//! "delete my data".

use std::str::Split;

use pinchora_core::{i18n::Lang, t};
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
#[command(
    name = "privacy",
    desc = "Choose what others can see of you, whether servers may DM you, or delete your data"
)]
pub struct PrivacyCommand {
    /// Let servers' custom commands, auto-responders, level-ups, suggestion updates and moderators DM you
    server_dms: Option<bool>,
}

impl PrivacyCommand {
    pub async fn run(self, cx: &Cx) -> anyhow::Result<()> {
        let user = cx.author()?.id.get();
        let note = match self.server_dms {
            Some(on) => {
                cx.app.db.set_dm_opt_out(user, !on).await?;
                t!(cx.lang(), "privacy.saved")
            }
            None => String::new(),
        };
        let data = panel(cx, user, &note).await?;
        cx.reply(data).await
    }
}

/// Tells the invoker "This profile is private." and returns true when `target` is somebody else who
/// hides their profile. Used by every command that shows another player's card.
pub(crate) async fn private_profile(cx: &Cx, target: u64) -> anyhow::Result<bool> {
    if target == cx.author()?.id.get() || !cx.app.db.privacy(target).await?.hide_profile {
        return Ok(false);
    }
    cx.reply_ephemeral(t!(cx.lang(), "privacy.profile-private")).await?;
    Ok(true)
}

/// Tells the invoker "This house is private." and returns true when `owner` is somebody else who
/// hides their house. Used by everything that reads or writes another player's house apart from
/// `/house visit`, which says so by name.
pub(crate) async fn private_house(cx: &Cx, owner: u64) -> anyhow::Result<bool> {
    if owner == cx.author()?.id.get() || !cx.app.db.privacy(owner).await?.hide_house {
        return Ok(false);
    }
    cx.reply_ephemeral(t!(cx.lang(), "privacy.house-private")).await?;
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
    let dms_off = cx.app.db.dm_opt_out(user).await?;
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
    let lang = cx.lang();
    let says = |on: bool, key: &str| t!(lang, &format!("privacy.{key}-{}", if on { "on" } else { "off" }));
    let line = |on: bool, key: &str| state(on, &says(true, key), &says(false, key));
    description.push_str(&t!(
        lang,
        "privacy.panel",
        profile = line(privacy.hide_profile, "profile"),
        boards = line(privacy.hide_leaderboard, "boards"),
        house = line(privacy.hide_house, "house"),
        dms = line(dms_off, "dm-opt-out")
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
        .title(t!(lang, "privacy.title"))
        .description(description)
        .footer(EmbedFooterBuilder::new(t!(lang, "privacy.footer")))
        .build();
    Ok(ephemeral(
        embed,
        vec![
            row(vec![
                toggle(&says(privacy.hide_profile, "button-profile"), "p", privacy.hide_profile),
                toggle(
                    &says(privacy.hide_leaderboard, "button-boards"),
                    "l",
                    privacy.hide_leaderboard,
                ),
                toggle(&says(privacy.hide_house, "button-house"), "h", privacy.hide_house),
                toggle(&says(dms_off, "button-dm-opt-out"), "m", dms_off),
            ]),
            row(vec![button(
                &t!(lang, "privacy.delete"),
                "pr:d".to_owned(),
                ButtonStyle::Danger,
            )]),
        ],
    ))
}

/// The second step: what deleting does, and a button that only works for a couple of minutes.
fn confirm(until: i64, lang: Lang) -> InteractionResponseData {
    let embed = EmbedBuilder::new()
        .color(DANGER)
        .title(t!(lang, "privacy.confirm-title"))
        .description(t!(lang, "privacy.confirm", days = TOMBSTONE_DAYS))
        .build();
    ephemeral(
        embed,
        vec![row(vec![
            button(
                &t!(lang, "privacy.confirm-yes"),
                format!("pr:y:{until}"),
                ButtonStyle::Danger,
            ),
            button(
                &t!(lang, "privacy.confirm-no"),
                "pr:n".to_owned(),
                ButtonStyle::Secondary,
            ),
        ])],
    )
}

/// `pr:t:<p|l|h|m>` flips a choice, `pr:d` asks for confirmation, `pr:y:<until>` deletes, `pr:n` backs out.
pub(crate) async fn on_component(cx: &Cx, mut args: Split<'_, char>) -> anyhow::Result<()> {
    let user = cx.author()?.id.get();
    match args.next() {
        Some("t") => {
            let which = match args.next() {
                Some("p") => Hide::Profile,
                Some("l") => Hide::Leaderboard,
                Some("h") => Hide::House,
                Some("m") => {
                    let off = cx.app.db.dm_opt_out(user).await?;
                    cx.app.db.set_dm_opt_out(user, !off).await?;
                    let data = panel(cx, user, &t!(cx.lang(), "privacy.saved")).await?;
                    return cx.update(data).await;
                }
                _ => return cx.reply_ephemeral(t!(cx.lang(), "privacy.expired")).await,
            };
            let before: Privacy = cx.app.db.privacy(user).await?;
            let now_hidden = !match which {
                Hide::Profile => before.hide_profile,
                Hide::Leaderboard => before.hide_leaderboard,
                Hide::House => before.hide_house,
            };
            cx.app.db.set_privacy(user, which, now_hidden).await?;
            let data = panel(cx, user, &t!(cx.lang(), "privacy.saved")).await?;
            cx.update(data).await
        }
        Some("d") => {
            cx.update(confirm(cx.app.now().timestamp() + CONFIRM_SECS, cx.lang()))
                .await
        }
        Some("n") => {
            let data = panel(cx, user, &t!(cx.lang(), "privacy.kept")).await?;
            cx.update(data).await
        }
        Some("y") => {
            let until: i64 = args.next().and_then(|v| v.parse().ok()).unwrap_or(0);
            if cx.app.now().timestamp() > until {
                let data = panel(cx, user, &t!(cx.lang(), "privacy.timed-out")).await?;
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
                .title(t!(cx.lang(), "privacy.deleted-title"))
                .description(t!(
                    cx.lang(),
                    "privacy.deleted",
                    rows = thousands(deleted.rows as i64),
                    tables = deleted.tables
                ))
                .build();
            cx.update(ephemeral(embed, Vec::new())).await
        }
        _ => cx.reply_ephemeral(t!(cx.lang(), "privacy.expired")).await,
    }
}
