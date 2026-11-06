//! `/customize style`: the accent colour, the panel opacity and the frame of the profile, rank and
//! wallet cards.

use std::str::Split;

use pinchora_core::leveling;
use pinchora_db::UserRow;
use pinchora_game::Content;
use pinchora_render::{ACCENT_PRESETS, AccentError, Frame, Opacity, parse_accent};
use twilight_model::{
    channel::message::{
        Component, EmojiReactionType,
        component::{ButtonStyle, Label, TextInput, TextInputStyle},
    },
    http::interaction::InteractionResponseData,
};
use twilight_util::builder::embed::EmbedBuilder;

use super::{button, expired, frame_of, menu, option, profile, row, worn_badges};
use crate::{Cx, util::BRAND};

// /Style

/// The custom id of the colour box in the `cs:hx` modal.
const HEX_FIELD: &str = "hex";

/// The accent colour the player chose, if any, as `#rrggbb`.
fn chosen_accent(row: &UserRow) -> Option<String> {
    row.accent
        .as_deref()
        .and_then(|a| parse_accent(a).ok())
        .map(|rgb| format!("#{rgb:06x}"))
}

/// What a typed colour was refused for, in words the player can act on.
fn refusal(error: AccentError) -> &'static str {
    match error {
        AccentError::Malformed => {
            "That is not a colour code. Type six hex digits such as `#ff6b4a`, or three such as `#f64`."
        }
        AccentError::TooDark => {
            "That colour is too dark to read on the card's navy panels. Try a brighter one, such as `#ff6b4a`."
        }
    }
}

/// Whether what is stored for the player is the style every player starts with. The stored
/// values count, not what the card shows of them: a frame above the player's level is not drawn
/// but is still saved, and Reset has to clear it.
fn is_default_style(row: &UserRow) -> bool {
    row.accent.is_none() && row.panel_opacity == Opacity::Normal.index() && row.frame == Frame::Plain.id()
}

/// How a frame shows in the menu: free, worn, or locked until a level.
fn frame_note(frame: Frame, level: u32, worn: bool) -> String {
    match (frame.unlock_level(), worn) {
        (_, true) => "On your cards now".to_owned(),
        (0, false) => "Free".to_owned(),
        (at, false) if frame.is_unlocked(level) => format!("Unlocked at level {at}"),
        (at, false) => format!("Locked · reach level {at} (you are level {level})"),
    }
}

/// The ephemeral style editor: their real profile card as it looks now, a menu of accent colours,
/// a menu of frames (the locked ones say which level opens them), buttons for the panel opacity
/// and a button that asks for a colour code.
pub(super) async fn panel(cx: &Cx, note: &str) -> anyhow::Result<InteractionResponseData> {
    let content = Content::get();
    let user = cx.author()?;
    let id = user.id.get();
    let player = cx.app.db.user(id).await?;
    let level = leveling::level_of(player.xp);
    let accent = chosen_accent(&player);
    let frame = frame_of(&player);
    let opacity = Opacity::from_index(player.panel_opacity);
    let background = player.background.as_deref().and_then(|e| content.background(e));

    let mut accents = vec![option(
        "Auto",
        "auto",
        "From your background, else coral".to_owned(),
        accent.is_none(),
    )];
    accents.extend(ACCENT_PRESETS.iter().map(|(name, rgb)| {
        let code = format!("#{rgb:06x}");
        option(name, &code[1..], code.clone(), accent.as_deref() == Some(code.as_str()))
    }));
    let frames = Frame::ALL
        .iter()
        .map(|f| {
            let mut entry = option(f.name(), f.id(), frame_note(*f, level, *f == frame), *f == frame);
            if !f.is_unlocked(level) {
                entry.emoji = Some(EmojiReactionType::Unicode {
                    name: "🔒".to_owned()
                });
            }
            entry
        })
        .collect();

    let worn = worn_badges(&cx.app, id, &player).await?;
    let is_default = is_default_style(&player);
    // Panel opacity only shows over a picture; without one the choice is left out, unless an
    // earlier choice still has to be undone.
    let show_opacity = background.is_some() || opacity != Opacity::Normal;
    let png = Box::pin(profile::card(
        cx,
        user,
        cx.author_display_name(),
        player,
        background,
        &worn,
    ))
    .await?;

    let mut text = String::new();
    if !note.is_empty() {
        text.push_str(note);
        text.push_str("\n\n");
    }
    let accent_text = accent.as_deref().map_or_else(
        || {
            if background.is_some() {
                "Auto (your background's)".to_owned()
            } else {
                "Auto (coral)".to_owned()
            }
        },
        |a| format!("`{a}`"),
    );
    text.push_str(&format!(
        "**Accent** {accent_text} · **Panels** {} · **Frame** {}\nThe style is on your profile, rank and wallet cards.",
        opacity.label(),
        frame.name()
    ));
    if !show_opacity {
        text.push_str("\nPanel opacity shows over a background picture: pick one with `/customize background`.");
    }
    let locked: Vec<String> = Frame::ALL
        .iter()
        .filter(|f| !f.is_unlocked(level))
        .map(|f| format!("🔒 {} · level {}", f.name(), f.unlock_level()))
        .collect();
    if !locked.is_empty() {
        text.push_str(&format!("\n{}", locked.join("  ")));
    }

    let opacity_button = |o: Opacity| {
        button(
            format!("cs:o:{}", o.index()),
            o.label(),
            if o == opacity {
                ButtonStyle::Primary
            } else {
                ButtonStyle::Secondary
            },
            false,
        )
    };
    let mut buttons = Vec::new();
    if show_opacity {
        buttons.extend(Opacity::ALL.map(opacity_button));
    }
    buttons.push(button("cs:h".to_owned(), "Hex code…", ButtonStyle::Secondary, false));
    buttons.push(button(
        "cs:r".to_owned(),
        "Reset style",
        ButtonStyle::Danger,
        is_default,
    ));
    let components = vec![
        row(vec![menu(
            "cs:a".to_owned(),
            "Accent colour…",
            accents,
            1,
            1,
            "Nothing to show",
        )]),
        row(vec![menu("cs:f".to_owned(), "Frame…", frames, 1, 1, "Nothing to show")]),
        row(buttons),
    ];
    let embed = EmbedBuilder::new().color(BRAND).title("Card style").description(text);
    Ok(Cx::card(embed, "profile.png", png, components, true))
}

/// `cs:<what>:...` selects and buttons of the style editor.
pub(crate) async fn on_style(cx: &Cx, mut args: Split<'_, char>, values: &[String]) -> anyhow::Result<()> {
    let id = cx.author()?.id.get();
    let value = values.first().map(String::as_str);
    match args.next() {
        Some("a") => {
            let note = match value {
                Some("auto") => {
                    cx.app.db.set_accent(id, None).await?;
                    "Saved. Your accent follows your background again.".to_owned()
                }
                // A hand-made value is checked like a typed code.
                Some(code) => match parse_accent(&format!("#{code}")) {
                    Ok(rgb) => {
                        let code = format!("#{rgb:06x}");
                        cx.app.db.set_accent(id, Some(&code)).await?;
                        format!("Saved. Your accent is `{code}`.")
                    }
                    Err(_) => return expired(cx, "style").await,
                },
                None => return expired(cx, "style").await,
            };
            let data = panel(cx, &note).await?;
            cx.update(data).await
        }
        Some("f") => {
            let Some(frame) = value.and_then(Frame::from_id) else {
                return expired(cx, "style").await;
            };
            let level = leveling::level_of(cx.app.db.user(id).await?.xp);
            let note = if frame.is_unlocked(level) {
                cx.app.db.set_frame(id, frame.id()).await?;
                format!("Saved. The **{}** frame is on your cards.", frame.name())
            } else {
                // The click is refused, however it was made: a locked frame is never saved.
                format!(
                    "The **{}** frame unlocks at level {}. You are level {level}.",
                    frame.name(),
                    frame.unlock_level()
                )
            };
            let data = panel(cx, &note).await?;
            cx.update(data).await
        }
        Some("o") => {
            let Some(opacity) = args
                .next()
                .and_then(|o| o.parse::<i16>().ok())
                .filter(|o| (0..=2).contains(o))
                .map(Opacity::from_index)
            else {
                return expired(cx, "style").await;
            };
            cx.app.db.set_panel_opacity(id, opacity.index()).await?;
            let data = panel(cx, &format!("Saved. Panels are **{}**.", opacity.label())).await?;
            cx.update(data).await
        }
        Some("h") => {
            let player = cx.app.db.user(id).await?;
            cx.modal(
                "cs:hx".to_owned(),
                "Accent colour",
                vec![hex_field(chosen_accent(&player).as_deref())],
            )
            .await
        }
        Some("r") => {
            cx.app.db.reset_card_style(id).await?;
            let data = panel(cx, "Done. Your cards have the style every player starts with.").await?;
            cx.update(data).await
        }
        _ => cx.reply_ephemeral("This button has expired.").await,
    }
}

/// The colour box of the accent modal, filled with the current accent when there is one.
#[allow(deprecated)] // TextInput::label: the Label around it carries the label now
fn hex_field(value: Option<&str>) -> Component {
    Component::Label(Label {
        component: Box::new(Component::TextInput(TextInput {
            custom_id: HEX_FIELD.to_owned(),
            id: None,
            label: None,
            max_length: Some(7),
            min_length: Some(1),
            placeholder: Some("#ff6b4a".to_owned()),
            required: Some(true),
            style: TextInputStyle::Short,
            value: value.map(str::to_owned),
        })),
        description: None,
        id: None,
        label: "Hex colour code".to_owned(),
    })
}

/// `cs:hx`: the colour code typed into the modal.
pub(crate) async fn on_style_modal(
    cx: &Cx,
    mut args: Split<'_, char>,
    fields: &[(String, String)],
) -> anyhow::Result<()> {
    match args.next() {
        Some("hx") => {
            let typed = fields
                .iter()
                .find(|(id, _)| id == HEX_FIELD)
                .map_or("", |(_, v)| v.as_str());
            let note = match parse_accent(typed) {
                Ok(rgb) => {
                    let code = format!("#{rgb:06x}");
                    cx.app.db.set_accent(cx.author()?.id.get(), Some(&code)).await?;
                    format!("Saved. Your accent is `{code}`.")
                }
                Err(error) => refusal(error).to_owned(),
            };
            let data = panel(cx, &note).await?;
            cx.update(data).await
        }
        _ => cx.reply_ephemeral("This form has expired.").await,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn locked_frames_say_their_level() {
        assert_eq!(frame_note(Frame::Plain, 1, false), "Free");
        assert_eq!(frame_note(Frame::Rope, 1, true), "On your cards now");
        assert_eq!(
            frame_note(Frame::Pearl, 12, false),
            "Locked · reach level 20 (you are level 12)"
        );
        assert_eq!(frame_note(Frame::Coral, 12, false), "Unlocked at level 10");
    }

    #[test]
    fn every_menu_value_and_label_fits_discord() {
        for (name, rgb) in ACCENT_PRESETS {
            let code = format!("#{rgb:06x}");
            let entry = option(name, &code[1..], code.clone(), false);
            assert!(entry.label.chars().count() <= 100 && entry.value.len() <= 100);
        }
        assert!(ACCENT_PRESETS.len() < 25, "Auto and the presets fit one menu");
        for frame in Frame::ALL {
            assert!(frame_note(frame, 1, false).chars().count() <= 100);
        }
    }

    #[test]
    fn refusals_explain_what_to_do() {
        assert!(refusal(AccentError::TooDark).contains("too dark"));
        assert!(refusal(AccentError::Malformed).contains("#ff6b4a"));
    }
}
