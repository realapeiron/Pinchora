//! Magic Conch: a tiny, one-shot group toy rendered as a pixel card.
use crate::Cx;
use crate::commands::games::games::channel_lang;
use crate::commands::games::games::pay_win;
use crate::commands::games::games::record;
use crate::infra::util::BRAND;
use pinchora_core::t;
use pinchora_game::play::Game;
use pinchora_render::{QuickCard, QuickDetails, QuickKind, Theme};
use twilight_model::channel::message::AllowedMentions;
use twilight_util::builder::embed::EmbedBuilder;

pub(crate) async fn run(
    cx: &Cx,
    mode: crate::commands::games::party_games::FunMode,
    prompt: Option<String>,
    sides: Option<i64>,
) -> anyhow::Result<()> {
    let lang = channel_lang(cx).await;
    let raw = prompt.unwrap_or_else(|| t!(lang, "fun.ask"));
    let strict = crate::commands::games::party_games::strict(&cx.app, cx.guild_id().unwrap_or(0)).await;
    let prompt = match crate::commands::games::party_games::text::clean_for(&raw, strict, lang) {
        Ok(text) => text,
        Err(reason) => return cx.reply_ephemeral(reason.say(cx.lang())).await,
    };
    let title = Game::MagicConch.name(lang);
    let subtitle = t!(lang, "fun.subtitle");
    let answer = match mode {
        crate::commands::games::party_games::FunMode::Conch => {
            t!(lang, &format!("fun.answers.item-{}", rand::random_range(0..8)))
        }
        crate::commands::games::party_games::FunMode::Choose => {
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
        crate::commands::games::party_games::FunMode::Roll => {
            let n = sides.unwrap_or(6).clamp(2, 100);
            format!("{} / {n}", rand::random_range(1..=n))
        }
    };
    let card_kind = match mode {
        crate::commands::games::party_games::FunMode::Conch => QuickKind::Conch,
        crate::commands::games::party_games::FunMode::Choose => QuickKind::Choose,
        crate::commands::games::party_games::FunMode::Roll => QuickKind::Roll,
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
