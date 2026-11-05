use std::{str::Split, sync::Arc, time::Duration};

use chrono::{Datelike, Timelike, Utc};
use pinchora_game::{
    Content,
    fishing::{self, BAIT_PRICE, CAST_COOLDOWN_SECS, REEL_WINDOW_MS},
};
use twilight_interactions::command::{CommandModel, CreateCommand};
use twilight_model::{
    channel::message::{
        Component,
        component::{ActionRow, Button, ButtonStyle},
    },
    http::interaction::InteractionResponseData,
};
use twilight_util::builder::embed::{EmbedBuilder, EmbedFooterBuilder};

use crate::{
    Cx,
    commands::customize,
    util::{BRAND, SUCCESS, WARN, thousands},
};

#[derive(CommandModel, CreateCommand)]
#[command(
    name = "fish",
    desc = "Cast a line (10 shells of bait). Press Reel the moment something bites!"
)]
pub struct Fish;

fn reel_row(custom_id: String, enabled: bool) -> Vec<Component> {
    vec![Component::ActionRow(ActionRow {
        id: None,
        components: vec![Component::Button(Button {
            custom_id: Some(custom_id),
            disabled: !enabled,
            emoji: None,
            id: None,
            label: Some("Reel!".to_owned()),
            sku_id: None,
            style: ButtonStyle::Success,
            url: None,
        })],
    })]
}

impl Fish {
    pub async fn run(self, cx: &Cx) -> anyhow::Result<()> {
        let user = cx.author()?.id.get();
        if let Some(left) = cx
            .app
            .cache
            .try_cooldown(&format!("fish:{user}"), Duration::from_secs(CAST_COOLDOWN_SECS))
            .await?
        {
            return cx
                .reply_ephemeral(format!(
                    "Your line is still drying. Cast again in {} s.",
                    left.as_secs() + 1
                ))
                .await;
        }
        if cx.app.db.spend_shells(user, BAIT_PRICE, "bait").await?.is_none() {
            return cx
                .reply_ephemeral(format!("Bait costs {BAIT_PRICE} shells. Try `/daily`!"))
                .await;
        }
        let nonce: u32 = rand::random();
        let delay_ms: i64 = rand::random_range(1_500..4_500);
        let bite_at = Utc::now().timestamp_millis() + delay_ms;
        cx.app
            .cache
            .put(
                &format!("cast:{user}:{nonce}"),
                &bite_at.to_string(),
                Duration::from_secs(30),
            )
            .await?;
        let custom_id = format!("fr:{user}:{nonce}");
        let waiting = EmbedBuilder::new()
            .color(BRAND)
            .title("🎣 Line in the water…")
            .description("Wait for the bite, then press **Reel!** Too early scares the fish away.")
            .build();
        cx.reply(InteractionResponseData {
            embeds: Some(vec![waiting]),
            components: Some(reel_row(custom_id.clone(), true)),
            ..Default::default()
        })
        .await?;

        // Reveal the bite after the delay by editing the message.
        let responder = Arc::clone(&cx.responder);
        cx.app.spawn(async move {
            tokio::time::sleep(Duration::from_millis(delay_ms as u64)).await;
            let bite = EmbedBuilder::new()
                .color(WARN)
                .title("❗ Something bit! REEL IT IN!")
                .build();
            let _ = responder
                .edit(&InteractionResponseData {
                    embeds: Some(vec![bite]),
                    components: Some(reel_row(custom_id, true)),
                    ..Default::default()
                })
                .await;
        });
        Ok(())
    }
}

/// `fr:<user>:<nonce>` button.
pub(crate) async fn on_reel(cx: &Cx, mut args: Split<'_, char>) -> anyhow::Result<()> {
    let caster: u64 = args.next().and_then(|v| v.parse().ok()).unwrap_or(0);
    let nonce = args.next().unwrap_or_default();
    let user = cx.author()?.id.get();
    if user != caster {
        return cx
            .reply_ephemeral("That's someone else's line! Cast your own with `/fish`.")
            .await;
    }
    let Some(bite_at) = cx
        .app
        .cache
        .take(&format!("cast:{user}:{nonce}"))
        .await?
        .and_then(|v| v.parse::<i64>().ok())
    else {
        return cx.reply_ephemeral("This line is gone. Cast again with `/fish`.").await;
    };
    let now = Utc::now();
    let late_by = now.timestamp_millis() - bite_at;
    let miss = |title: &str, text: &str| InteractionResponseData {
        embeds: Some(vec![
            EmbedBuilder::new().color(WARN).title(title).description(text).build(),
        ]),
        components: Some(Vec::new()),
        ..Default::default()
    };
    if late_by < 0 {
        return cx
            .update(miss(
                "Too early!",
                "The splash scared the fish away. Cast again with `/fish`.",
            ))
            .await;
    }
    if late_by > REEL_WINDOW_MS {
        return cx
            .update(miss("Too slow!", "It slipped off the hook. Cast again with `/fish`."))
            .await;
    }
    let content = Content::get();
    let pool = fishing::biting(content, now.hour(), now.month());
    let Some((fish, size, value)) = fishing::catch(&pool, rand::random::<f64>(), rand::random::<f64>()) else {
        return cx
            .update(miss("Nothing bites today", "The sea is quiet. Try again later."))
            .await;
    };
    let first = cx.app.db.log_catch(user, &fish.id, size).await?;
    cx.app.quest(user, pinchora_game::quests::QuestKind::Fish).await;
    let balance = cx.app.db.earn_shells(user, value, "fishing").await?;
    let book = cx.app.db.fish_book_count(user).await?;
    let sprite = cx.app.sprites.fish(&fish.id);
    let png = cx
        .app
        .render(move |_| {
            let factor = (144 / sprite.width().max(sprite.height()).max(1)).clamp(1, 12);
            let big = image::imageops::resize(
                sprite.as_ref(),
                sprite.width() * factor,
                sprite.height() * factor,
                image::imageops::FilterType::Nearest,
            );
            let mut buf = std::io::Cursor::new(Vec::new());
            big.write_to(&mut buf, image::ImageFormat::Png)?;
            Ok(buf.into_inner())
        })
        .await?;
    let reaction = format!("{:.2} s", late_by as f64 / 1000.0);
    let embed = EmbedBuilder::new()
        .color(if first { SUCCESS } else { fish.rarity.color() })
        .title(format!(
            "{}You caught a {} {}!",
            if first { "🆕 " } else { "" },
            fish.rarity.label().to_lowercase(),
            fish.name
        ))
        .description(format!(
            "*{}*\n**{size} cm** · sold for **{}** shells · balance {}",
            fish.flavor,
            thousands(value),
            thousands(balance)
        ))
        .footer(EmbedFooterBuilder::new(format!(
            "Reaction {reaction} · Fish book: {book}/{}",
            content.fish.len()
        )));
    let data = Cx::card(embed, "catch.png", png, Vec::new(), false);
    cx.update(data).await?;
    // A new species in the fish book, or the shells it sold for, may have just earned a badge.
    customize::announce(cx).await;
    Ok(())
}
