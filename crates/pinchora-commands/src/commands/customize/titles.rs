//! `/customize title`: the title under the player's name on the profile and rank cards.

use std::str::Split;

use pinchora_core::t;
use pinchora_game::Content;
use twilight_model::{channel::message::component::ButtonStyle, http::interaction::InteractionResponseData};
use twilight_util::builder::embed::{EmbedBuilder, EmbedFieldBuilder};

use super::{PER_PAGE, Wardrobe, button, expired, menu, option, row, worn_badges};
use crate::{Cx, commands::profile, util::BRAND};

// /Titles

/// `name` as one or more embed fields of at most 1,000 characters: a long list is split between lines.
fn chunked_fields(name: &str, lines: &[String]) -> Vec<(String, String)> {
    let mut fields: Vec<String> = Vec::new();
    for line in lines {
        match fields.last_mut() {
            Some(last) if last.chars().count() + line.chars().count() < 1_000 => {
                last.push('\n');
                last.push_str(line);
            }
            _ => fields.push(line.clone()),
        }
    }
    let total = fields.len();
    fields
        .into_iter()
        .enumerate()
        .map(|(i, lines)| {
            let heading = if total > 1 {
                format!("{name} ({}/{total})", i + 1)
            } else {
                name.to_owned()
            };
            (heading, lines)
        })
        .collect()
}

/// The ephemeral title editor: the profile card with the title under the name, a menu of the
/// titles the player has unlocked, what the rest need, and a button to take the title off.
pub(super) async fn panel(cx: &Cx, page: usize, note: &str) -> anyhow::Result<InteractionResponseData> {
    let lang = cx.lang();
    let content = Content::get();
    let user = cx.author()?;
    let id = user.id.get();
    let player = cx.app.db.user(id).await?;
    let wardrobe = Wardrobe::load_titles(&cx.app, id, &player).await?;
    let equipped = player.title.clone().filter(|t| content.title(t).is_some());
    let unlocked: Vec<_> = content.titles.iter().filter(|t| wardrobe.has_title(t)).collect();
    let locked: Vec<_> = content.titles.iter().filter(|t| !wardrobe.has_title(t)).collect();
    let pages = unlocked.len().div_ceil(PER_PAGE).max(1);
    let page = page.min(pages - 1);

    let options = unlocked
        .iter()
        .skip(page * PER_PAGE)
        .take(PER_PAGE)
        .map(|t| {
            option(
                &content.name(&t.id, lang),
                &t.id,
                content.description(&t.id, lang).unwrap_or_default().into_owned(),
                equipped.as_deref() == Some(t.id.as_str()),
            )
        })
        .collect();

    let worn = worn_badges(&cx.app, id, &player).await?;
    let background = player.background.as_deref().and_then(|e| content.background(e));
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
    match equipped.as_deref().and_then(|t| content.title(t)) {
        Some(t) => text.push_str(&t!(
            lang,
            "customize.title.your-title",
            name = content.name(&t.id, lang)
        )),
        None => text.push_str(&t!(lang, "customize.title.no-title")),
    }
    text.push_str(&t!(
        lang,
        "customize.title.unlocked-count",
        count = unlocked.len(),
        total = content.titles.len()
    ));
    let mut embed = EmbedBuilder::new()
        .color(BRAND)
        .title(t!(lang, "customize.title.title"))
        .description(text);
    let lines = |titles: &[&pinchora_game::Title]| -> Vec<String> {
        titles
            .iter()
            .map(|t| wardrobe.title_line(t, equipped.as_deref(), lang))
            .collect()
    };
    for (name, value) in chunked_fields(&t!(lang, "customize.title.heading-unlocked"), &lines(&unlocked))
        .into_iter()
        .chain(chunked_fields(
            &t!(lang, "customize.title.heading-locked"),
            &lines(&locked),
        ))
    {
        embed = embed.field(EmbedFieldBuilder::new(name, value));
    }

    let mut buttons = Vec::new();
    if pages > 1 {
        buttons.push(button(
            format!("ti:p:{}", page.saturating_sub(1)),
            &t!(lang, "customize.prev-page"),
            ButtonStyle::Secondary,
            page == 0,
        ));
        buttons.push(button(
            format!("ti:p:{}", page + 1),
            &t!(lang, "customize.next-page"),
            ButtonStyle::Secondary,
            page + 1 >= pages,
        ));
    }
    buttons.push(button(
        "ti:r".to_owned(),
        &t!(lang, "customize.remove"),
        ButtonStyle::Danger,
        equipped.is_none(),
    ));
    let components = vec![
        row(vec![menu(
            format!("ti:s:{page}"),
            &t!(lang, "customize.title.placeholder"),
            options,
            1,
            1,
            &t!(lang, "customize.title.nothing-unlocked"),
        )]),
        row(buttons),
    ];
    Ok(Cx::card(embed, "profile.png", png, components, true))
}

/// `ti:<what>:...` select and buttons of the title editor.
pub(crate) async fn on_title(cx: &Cx, mut args: Split<'_, char>, values: &[String]) -> anyhow::Result<()> {
    let lang = cx.lang();
    let content = Content::get();
    let id = cx.author()?.id.get();
    match args.next() {
        Some("s") => {
            let page = args.next().and_then(|p| p.parse().ok()).unwrap_or(0);
            let Some(title) = values.first().and_then(|v| content.title(v)) else {
                return expired(cx, "title").await;
            };
            let player = cx.app.db.user(id).await?;
            let wardrobe = Wardrobe::load_titles(&cx.app, id, &player).await?;
            // Hand-made picks of a title that is not theirs yet are refused like any stale choice.
            if !wardrobe.has_title(title) {
                return expired(cx, "title").await;
            }
            cx.app.db.set_title(id, Some(&title.id)).await?;
            let note = t!(lang, "customize.title.saved", name = content.name(&title.id, lang));
            let data = panel(cx, page, &note).await?;
            cx.update(data).await
        }
        Some("p") => {
            let data = panel(cx, args.next().and_then(|p| p.parse().ok()).unwrap_or(0), "").await?;
            cx.update(data).await
        }
        Some("r") => {
            cx.app.db.set_title(id, None).await?;
            let data = panel(cx, 0, &t!(lang, "customize.title.removed")).await?;
            cx.update(data).await
        }
        _ => cx.reply_ephemeral(t!(lang, "customize.title.expired")).await,
    }
}

#[cfg(test)]
mod tests {
    use pinchora_core::stats::PlayerStats;

    use super::*;

    fn wardrobe(stats: PlayerStats, bought: &[&str]) -> Wardrobe {
        Wardrobe {
            stats,
            bought: bought.iter().map(|s| (*s).to_owned()).collect(),
            equipped: Vec::new(),
            chosen: true,
            granted: Vec::new(),
        }
    }

    #[test]
    fn titles_unlock_by_stat_badge_or_grant() {
        let content = Content::get();
        let title = |id: &str| content.title(id).expect("a title");
        let stats = PlayerStats {
            level: 15,
            fish_species: 4,
            ..Default::default()
        };
        let mut w = wardrobe(stats, &["crab_cap"]);
        assert!(w.has_title(title("lighthouse_keeper")), "level 15 reaches 15");
        assert!(!w.has_title(title("harbour_elder")), "but not 40");
        assert!(!w.has_title(title("tide_angler")), "4 species is short of 5");
        assert!(w.has_title(title("capn_crab")), "owning the Crab Cap badge unlocks it");
        assert!(!w.has_title(title("puddle_pilot")), "a badge they do not own does not");
        assert!(
            !w.has_title(title("house_of_the_week")),
            "nothing earns the special title"
        );
        w.granted = vec!["house_of_the_week".to_owned(), "harbour_elder".to_owned()];
        assert!(w.has_title(title("house_of_the_week")), "a grant unlocks it");
        assert!(w.has_title(title("harbour_elder")), "a grant counts for any title");

        let line = |id: &str| w.title_line(title(id), Some("lighthouse_keeper"), pinchora_core::i18n::Lang::En);
        assert_eq!(
            line("lighthouse_keeper"),
            "✅ **Lighthouse Keeper** · Reach level 15. *(worn)*"
        );
        assert_eq!(line("tide_angler"), "🔒 **Tide Angler** · 4/5 fish species");
        assert_eq!(
            line("puddle_pilot"),
            "🔒 **Puddle Pilot** · Own the Rainy Day Duck badge."
        );
    }

    #[test]
    fn long_lists_split_into_fields_that_fit() {
        let content = Content::get();
        let lines: Vec<String> = content
            .titles
            .iter()
            .map(|t| format!("🔒 **{}** · {}", t.name, t.description.repeat(3)))
            .collect();
        let fields = chunked_fields("Still to earn", &lines);
        assert!(fields.len() > 1, "{} fields", fields.len());
        assert!(fields.iter().all(|(_, value)| value.chars().count() <= 1_024));
        assert_eq!(fields[0].0, format!("Still to earn (1/{})", fields.len()));
        let rejoined: Vec<_> = fields.iter().flat_map(|(_, v)| v.lines()).collect();
        assert_eq!(rejoined.len(), lines.len(), "no line is lost");
        assert_eq!(chunked_fields("Unlocked", &lines[..2]).len(), 1);
        assert_eq!(chunked_fields("Unlocked", &lines[..2])[0].0, "Unlocked");
        assert!(chunked_fields("Unlocked", &[]).is_empty());
    }
}
