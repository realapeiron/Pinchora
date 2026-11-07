//! Magic Conch: a tiny, one-shot group toy rendered as a pixel card.
use crate::{
    Cx,
    commands::games::{channel_lang, pay_win, record},
    util::BRAND,
};
use pinchora_core::t;
use pinchora_game::play::Game;
use pinchora_render::{QuickCard, QuickDetails, QuickKind, Theme};
use twilight_model::channel::message::AllowedMentions;
use twilight_util::builder::embed::EmbedBuilder;

const ANSWERS_EN: [&str; 8] = [
    "Yes.",
    "Maybe later.",
    "Ask a friend.",
    "The tide says no.",
    "Give it a try.",
    "A surprise is near.",
    "Take a little break.",
    "Trust your curiosity.",
];
const ANSWERS_TR: [&str; 8] = [
    "Evet.",
    "Belki sonra.",
    "Bir arkadaşına sor.",
    "Gelgit hayır diyor.",
    "Bir dene.",
    "Bir sürpriz yakında.",
    "Biraz mola ver.",
    "Merakına güven.",
];

pub(crate) async fn run(
    cx: &Cx,
    mode: super::FunMode,
    prompt: Option<String>,
    sides: Option<i64>,
) -> anyhow::Result<()> {
    let lang = channel_lang(cx).await;
    let raw = prompt.unwrap_or_else(|| t!(lang, "fun.ask"));
    let strict = super::strict(&cx.app, cx.guild_id().unwrap_or(0)).await;
    let prompt = match super::text::clean_for(&raw, strict, lang) {
        Ok(text) => text,
        Err(reason) => return cx.reply_ephemeral(reason.say(cx.lang())).await,
    };
    let title = Game::MagicConch.name(lang);
    let subtitle = t!(lang, "fun.subtitle");
    let answer = match mode {
        super::FunMode::Conch => {
            let pack = if lang == pinchora_core::i18n::Lang::Tr {
                &ANSWERS_TR
            } else {
                &ANSWERS_EN
            };
            pack[rand::random_range(0..pack.len())].to_owned()
        }
        super::FunMode::Choose => {
            let choices: Vec<_> = prompt
                .split(',')
                .map(str::trim)
                .filter(|v| !v.is_empty())
                .take(8)
                .collect();
            if choices.len() < 2 {
                return cx.reply_ephemeral(t!(cx.lang(), "fun.need-choices")).await;
            }
            choices[rand::random_range(0..choices.len())].chars().take(80).collect()
        }
        super::FunMode::Roll => {
            let n = sides.unwrap_or(6).clamp(2, 100);
            format!("{} / {n}", rand::random_range(1..=n))
        }
    };
    let card_kind = match mode {
        super::FunMode::Conch => QuickKind::Conch,
        super::FunMode::Choose => QuickKind::Choose,
        super::FunMode::Roll => QuickKind::Roll,
    };
    let status = t!(lang, "fun.result");
    let png = cx
        .app
        .render(move |r| {
            r.quick_card(&QuickCard {
                kind: card_kind,
                details: QuickDetails {
                    finished: true,
                    ..Default::default()
                },
                lang,
                title: &title,
                subtitle: &subtitle,
                cells: &[],
                columns: 0,
                prompt: &answer,
                status: &status,
                theme: Theme::reef(),
                distort: false,
            })
        })
        .await?;
    let mut data = Cx::card(
        EmbedBuilder::new()
            .color(BRAND)
            .title(Game::MagicConch.name(lang))
            .description(prompt.chars().take(150).collect::<String>()),
        "fun.png",
        png,
        vec![],
        false,
    );
    data.allowed_mentions = Some(AllowedMentions::default());
    cx.reply(data).await?;
    let me = cx.author()?.id.get();
    record(&cx.app, Game::MagicConch, me, true).await;
    let _ = pay_win(&cx.app, me, Game::MagicConch, 1, "fun_game").await?;
    Ok(())
}
