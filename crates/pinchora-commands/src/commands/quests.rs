use std::{str::Split, time::Duration};

use chrono::Utc;
use pinchora_core::t;
use pinchora_game::quests::{self, ALL_CLEAR_PEARLS};
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
    util::{BRAND, SUCCESS},
};

#[derive(CommandModel, CreateCommand)]
#[command(name = "quests", desc = "Today's three quests and their rewards")]
pub struct Quests;

impl Quests {
    pub async fn run(self, cx: &Cx) -> anyhow::Result<()> {
        let data = board(cx, None).await?;
        cx.reply(data).await
    }
}

fn today() -> i64 {
    Utc::now().timestamp().div_euclid(86_400)
}

fn bar(progress: i64, target: i64) -> String {
    let filled = (progress.min(target) * 10 / target.max(1)) as usize;
    format!("{}{}", "▰".repeat(filled), "▱".repeat(10 - filled))
}

async fn board(cx: &Cx, note: Option<String>) -> anyhow::Result<InteractionResponseData> {
    let user = cx.author()?.id.get();
    let day = today();
    let lang = cx.lang();
    let mut lines = Vec::new();
    let mut claimable = false;
    for q in quests::for_day(user, day) {
        let progress = cx
            .app
            .cache
            .counter(&format!("quest:{day}:{user}:{}", q.kind.key()))
            .await?;
        let claimed = cx
            .app
            .cache
            .counter(&format!("quest-claimed:{day}:{user}:{}", q.kind.key()))
            .await?
            > 0;
        let done = progress >= q.target;
        claimable |= done && !claimed;
        let mark = if claimed {
            "✅"
        } else if done {
            "🎁"
        } else {
            "▫️"
        };
        lines.push(t!(
            lang,
            "quests.line",
            mark = mark,
            text = q.text_in(lang),
            bar = bar(progress, q.target),
            progress = progress.min(q.target),
            target = q.target,
            reward = q.reward
        ));
    }
    let seconds_left = 86_400 - Utc::now().timestamp().rem_euclid(86_400);
    let embed = EmbedBuilder::new()
        .color(if claimable { SUCCESS } else { BRAND })
        .title(t!(lang, "quests.title"))
        .description(format!(
            "{}\n\n{}{}",
            lines.join("\n"),
            t!(lang, "quests.all-clear", pearls = ALL_CLEAR_PEARLS),
            note.map(|n| format!("\n\n{n}")).unwrap_or_default()
        ))
        .footer(EmbedFooterBuilder::new(t!(
            lang,
            "quests.footer",
            hours = seconds_left / 3600,
            minutes = seconds_left % 3600 / 60
        )));
    Ok(InteractionResponseData {
        embeds: Some(vec![embed.build()]),
        components: Some(vec![Component::ActionRow(ActionRow {
            id: None,
            components: vec![Component::Button(Button {
                custom_id: Some(format!("qc:{day}")),
                disabled: !claimable,
                emoji: None,
                id: None,
                label: Some(t!(lang, "quests.claim")),
                sku_id: None,
                style: ButtonStyle::Success,
                url: None,
            })],
        })]),
        flags: Some(MessageFlags::EPHEMERAL),
        ..Default::default()
    })
}

/// `qc:<day>`: pays every finished, unclaimed quest of that day once; all three pay pearls. A board from an
/// earlier day has nothing to claim any more, and says so instead of paying today's quests.
pub(crate) async fn on_claim(cx: &Cx, mut args: Split<'_, char>) -> anyhow::Result<()> {
    let user = cx.author()?.id.get();
    let day = today();
    let lang = cx.lang();
    if args.next().and_then(|v| v.parse::<i64>().ok()) != Some(day) {
        let data = board(cx, Some(t!(lang, "quests.expired"))).await?;
        return cx.update(data).await;
    }
    let ttl = Duration::from_secs(2 * 86_400);
    let (mut shells, mut finished) = (0, 0);
    for q in quests::for_day(user, day) {
        let progress = cx
            .app
            .cache
            .counter(&format!("quest:{day}:{user}:{}", q.kind.key()))
            .await?;
        if progress < q.target {
            continue;
        }
        finished += 1;
        let key = format!("quest-claimed:{day}:{user}:{}", q.kind.key());
        if cx.app.cache.once(&format!("{key}:lock"), ttl).await? {
            cx.app.cache.incr(&key, ttl).await?;
            shells += q.reward;
        }
    }
    let mut parts = Vec::new();
    if shells > 0 {
        cx.app.db.earn_shells(user, shells, "quests").await?;
        parts.push(t!(lang, "quests.shells", shells = shells));
    }
    if finished == 3 && cx.app.cache.once(&format!("quest-allclear:{day}:{user}"), ttl).await? {
        cx.app.db.earn_pearls(user, ALL_CLEAR_PEARLS).await?;
        parts.push(t!(lang, "quests.pearls", pearls = ALL_CLEAR_PEARLS));
    }
    let note = if parts.is_empty() {
        t!(lang, "quests.nothing")
    } else {
        format!("🎉 {}", parts.join(" · "))
    };
    let data = board(cx, Some(note)).await?;
    cx.update(data).await
}
