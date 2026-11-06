//! `/customize style`: the accent colour, the panel opacity and the frame of the profile, rank and
//! wallet cards.

use std::str::Split;

use pinchora_core::{i18n::Lang, leveling, t};
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
fn refusal(error: AccentError, lang: Lang) -> String {
    match error {
        AccentError::Malformed => t!(lang, "customize.style.refusal-malformed"),
        AccentError::TooDark => t!(lang, "customize.style.refusal-dark"),
    }
}

/// The name of an accent preset in `lang`.
fn accent_name(name: &str, lang: Lang) -> String {
    t!(lang, &format!("customize.style.accent.{}", name.to_lowercase()))
}

/// The name of a frame in `lang`.
fn frame_name(frame: Frame, lang: Lang) -> String {
    t!(lang, &format!("customize.style.frame.{}", frame.id()))
}

/// The name of a panel opacity in `lang`.
fn opacity_name(opacity: Opacity, lang: Lang) -> String {
    t!(
        lang,
        &format!("customize.style.opacity.{}", opacity.label().to_lowercase())
    )
}

/// Whether what is stored for the player is the style every player starts with. The stored
/// values count, not what the card shows of them: a frame above the player's level is not drawn
/// but is still saved, and Reset has to clear it.
fn is_default_style(row: &UserRow) -> bool {
    row.accent.is_none() && row.panel_opacity == Opacity::Normal.index() && row.frame == Frame::Plain.id()
}

/// How a frame shows in the menu: free, worn, or locked until a level.
fn frame_note(frame: Frame, level: u32, worn: bool, lang: Lang) -> String {
    match (frame.unlock_level(), worn) {
        (_, true) => t!(lang, "customize.style.frame-worn"),
        (0, false) => t!(lang, "customize.style.frame-free"),
        (at, false) if frame.is_unlocked(level) => t!(lang, "customize.style.frame-unlocked", level = at),
        (at, false) => t!(lang, "customize.style.frame-locked", at = at, level = level),
    }
}

/// The ephemeral style editor: their real profile card as it looks now, a menu of accent colours,
/// a menu of frames (the locked ones say which level opens them), buttons for the panel opacity
/// and a button that asks for a colour code.
pub(super) async fn panel(cx: &Cx, note: &str) -> anyhow::Result<InteractionResponseData> {
    let lang = cx.lang();
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
        &t!(lang, "customize.style.auto"),
        "auto",
        t!(lang, "customize.style.auto-hint"),
        accent.is_none(),
    )];
    accents.extend(ACCENT_PRESETS.iter().map(|(name, rgb)| {
        let code = format!("#{rgb:06x}");
        option(
            &accent_name(name, lang),
            &code[1..],
            code.clone(),
            accent.as_deref() == Some(code.as_str()),
        )
    }));
    let frames = Frame::ALL
        .iter()
        .map(|f| {
            let mut entry = option(
                &frame_name(*f, lang),
                f.id(),
                frame_note(*f, level, *f == frame, lang),
                *f == frame,
            );
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
                t!(lang, "customize.style.accent-auto-background")
            } else {
                t!(lang, "customize.style.accent-auto")
            }
        },
        |a| format!("`{a}`"),
    );
    text.push_str(&t!(
        lang,
        "customize.style.summary",
        accent = accent_text,
        panels = opacity_name(opacity, lang),
        frame = frame_name(frame, lang)
    ));
    if !show_opacity {
        text.push_str(&t!(lang, "customize.style.opacity-hint"));
    }
    let locked: Vec<String> = Frame::ALL
        .iter()
        .filter(|f| !f.is_unlocked(level))
        .map(|f| {
            t!(
                lang,
                "customize.style.locked-frame",
                name = frame_name(*f, lang),
                level = f.unlock_level()
            )
        })
        .collect();
    if !locked.is_empty() {
        text.push_str(&format!("\n{}", locked.join("  ")));
    }

    let opacity_button = |o: Opacity| {
        button(
            format!("cs:o:{}", o.index()),
            &opacity_name(o, lang),
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
    buttons.push(button(
        "cs:h".to_owned(),
        &t!(lang, "customize.style.hex-button"),
        ButtonStyle::Secondary,
        false,
    ));
    buttons.push(button(
        "cs:r".to_owned(),
        &t!(lang, "customize.style.reset-button"),
        ButtonStyle::Danger,
        is_default,
    ));
    let components = vec![
        row(vec![menu(
            "cs:a".to_owned(),
            &t!(lang, "customize.style.accent-menu"),
            accents,
            1,
            1,
            &t!(lang, "customize.nothing-to-show"),
        )]),
        row(vec![menu(
            "cs:f".to_owned(),
            &t!(lang, "customize.style.frame-menu"),
            frames,
            1,
            1,
            &t!(lang, "customize.nothing-to-show"),
        )]),
        row(buttons),
    ];
    let embed = EmbedBuilder::new()
        .color(BRAND)
        .title(t!(lang, "customize.style.title"))
        .description(text);
    Ok(Cx::card(embed, "profile.png", png, components, true))
}

/// `cs:<what>:...` selects and buttons of the style editor.
pub(crate) async fn on_style(cx: &Cx, mut args: Split<'_, char>, values: &[String]) -> anyhow::Result<()> {
    let lang = cx.lang();
    let id = cx.author()?.id.get();
    let value = values.first().map(String::as_str);
    match args.next() {
        Some("a") => {
            let note = match value {
                Some("auto") => {
                    cx.app.db.set_accent(id, None).await?;
                    t!(lang, "customize.style.saved-auto")
                }
                // A hand-made value is checked like a typed code.
                Some(code) => match parse_accent(&format!("#{code}")) {
                    Ok(rgb) => {
                        let code = format!("#{rgb:06x}");
                        cx.app.db.set_accent(id, Some(&code)).await?;
                        t!(lang, "customize.style.saved-accent", code = code)
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
                t!(lang, "customize.style.saved-frame", frame = frame_name(frame, lang))
            } else {
                // The click is refused, however it was made: a locked frame is never saved.
                t!(
                    lang,
                    "customize.style.frame-refused",
                    frame = frame_name(frame, lang),
                    at = frame.unlock_level(),
                    level = level
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
            let note = t!(
                lang,
                "customize.style.saved-opacity",
                opacity = opacity_name(opacity, lang)
            );
            let data = panel(cx, &note).await?;
            cx.update(data).await
        }
        Some("h") => {
            let player = cx.app.db.user(id).await?;
            cx.modal(
                "cs:hx".to_owned(),
                &t!(lang, "customize.style.modal-title"),
                vec![hex_field(chosen_accent(&player).as_deref(), lang)],
            )
            .await
        }
        Some("r") => {
            cx.app.db.reset_card_style(id).await?;
            let data = panel(cx, &t!(lang, "customize.style.reset-done")).await?;
            cx.update(data).await
        }
        _ => cx.reply_ephemeral(t!(lang, "customize.style.expired")).await,
    }
}

/// The colour box of the accent modal, filled with the current accent when there is one.
#[allow(deprecated)] // TextInput::label: the Label around it carries the label now
fn hex_field(value: Option<&str>, lang: Lang) -> Component {
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
        label: t!(lang, "customize.style.hex-label"),
    })
}

/// `cs:hx`: the colour code typed into the modal.
pub(crate) async fn on_style_modal(
    cx: &Cx,
    mut args: Split<'_, char>,
    fields: &[(String, String)],
) -> anyhow::Result<()> {
    let lang = cx.lang();
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
                    t!(lang, "customize.style.saved-accent", code = code)
                }
                Err(error) => refusal(error, lang),
            };
            let data = panel(cx, &note).await?;
            cx.update(data).await
        }
        _ => cx.reply_ephemeral(t!(lang, "customize.style.form-expired")).await,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn locked_frames_say_their_level() {
        assert_eq!(frame_note(Frame::Plain, 1, false, Lang::En), "Free");
        assert_eq!(frame_note(Frame::Rope, 1, true, Lang::En), "On your cards now");
        assert_eq!(
            frame_note(Frame::Pearl, 12, false, Lang::En),
            "Locked · reach level 20 (you are level 12)"
        );
        assert_eq!(frame_note(Frame::Coral, 12, false, Lang::En), "Unlocked at level 10");
        assert_eq!(
            frame_note(Frame::Pearl, 12, false, Lang::Tr),
            "Kilitli · 20. seviyeye ulaş (sen 12. seviyedesin)"
        );
    }

    #[test]
    fn every_menu_value_and_label_fits_discord() {
        for lang in Lang::ALL {
            for (name, rgb) in ACCENT_PRESETS {
                let code = format!("#{rgb:06x}");
                let entry = option(&accent_name(name, lang), &code[1..], code.clone(), false);
                assert!(entry.label.chars().count() <= 100 && entry.value.len() <= 100);
                assert_ne!(
                    entry.label,
                    format!("customize.style.accent.{}", name.to_lowercase()),
                    "{name} is in the catalog"
                );
            }
            for frame in Frame::ALL {
                assert!(frame_note(frame, 1, false, lang).chars().count() <= 100);
                assert!(
                    !frame_name(frame, lang).starts_with("customize."),
                    "{frame:?} is in the catalog"
                );
            }
            for opacity in Opacity::ALL {
                assert!(!opacity_name(opacity, lang).starts_with("customize."), "{opacity:?}");
            }
        }
        assert!(ACCENT_PRESETS.len() < 25, "Auto and the presets fit one menu");
    }

    #[test]
    fn refusals_explain_what_to_do() {
        assert!(refusal(AccentError::TooDark, Lang::En).contains("too dark"));
        assert!(refusal(AccentError::Malformed, Lang::En).contains("#ff6b4a"));
        assert!(
            refusal(AccentError::TooDark, Lang::Tr).contains("çok koyu")
                || refusal(AccentError::TooDark, Lang::Tr).contains("koyu")
        );
    }
}
