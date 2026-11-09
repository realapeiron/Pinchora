//! "Badge unlocked" notices: told once, under the command or chat message that earned the badge.

use pinchora_core::infra::i18n::Lang;
use pinchora_core::t;
use pinchora_game::{Badge, BadgeKind, Content};
use twilight_model::{
    channel::message::{AllowedMentions, Embed, MessageFlags},
    http::{attachment::Attachment, interaction::InteractionResponseData},
    id::{Id, marker::ChannelMarker},
};
use twilight_util::builder::embed::{EmbedBuilder, EmbedFooterBuilder, ImageSource};

use crate::App;
use crate::Cx;
use crate::commands::social::customize::stat_value;
use crate::games::game::scene::Sprites;
use crate::infra::util::mention;

// /Unlocks

/// The achievement badges the player has earned since they were last told about one. Each is
/// reported once: the first time this runs for a badge, and never again. One query reads the
/// stats; another runs only when something is new.
pub(crate) async fn unlocked_badges(app: &App, user: u64) -> anyhow::Result<Vec<&'static Badge>> {
    let (stats, notified) = app.db.badge_progress(user).await?;
    let earned: Vec<&'static Badge> = Content::get()
        .badges
        .iter()
        .filter(|b| matches!(b.kind, BadgeKind::Achievement { stat, at } if stat_value(&stats, stat) >= at))
        .collect();
    if notified
        .as_ref()
        .is_some_and(|known| earned.iter().all(|b| known.contains(&b.id)))
    {
        return Ok(Vec::new());
    }
    let ids: Vec<String> = earned.iter().map(|b| b.id.clone()).collect();
    let fresh = app.db.claim_badge_notices(user, &ids).await?;
    Ok(earned.into_iter().filter(|b| fresh.contains(&b.id)).collect())
}

/// How many "Badge unlocked" embeds one message carries.
const NOTICES_PER_MESSAGE: usize = 3;

/// A "Badge unlocked" embed for each badge (three at most), with the badge art as its thumbnail.
pub(crate) fn unlock_notice(sprites: &Sprites, badges: &[&Badge], lang: Lang) -> (Vec<Embed>, Vec<Attachment>) {
    let content = Content::get();
    let shown = badges.len().min(NOTICES_PER_MESSAGE);
    let mut embeds = Vec::new();
    let mut files = Vec::new();
    for (n, badge) in badges.iter().take(NOTICES_PER_MESSAGE).enumerate() {
        let mut embed = EmbedBuilder::new()
            .color(badge.rarity.color())
            .title(t!(lang, "customize.unlock.title", name = content.name(&badge.id, lang)))
            .description(format!(
                "{}\n*{}*",
                content.description(&badge.id, lang).unwrap_or_default(),
                badge.rarity.label_in(lang)
            ));
        if let Some(art) = sprites.badge_large(&badge.id) {
            let name = format!("badge-{}.png", badge.id);
            if let Ok(thumbnail) = ImageSource::attachment(&name) {
                embed = embed.thumbnail(thumbnail);
            }
            files.push(Attachment::from_bytes(name, art.png.clone(), n as u64 + 1));
        }
        if n + 1 == shown {
            let more = match badges.len() - shown {
                0 => String::new(),
                left => t!(lang, "customize.unlock.more", count = left),
            };
            embed = embed.footer(EmbedFooterBuilder::new(format!(
                "{more}{}",
                t!(lang, "customize.unlock.hint")
            )));
        }
        embeds.push(embed.build());
    }
    (embeds, files)
}

/// After a command has answered: tells the player about badges that command just earned them,
/// as a follow-up under the answer. Never fails the command; a failed check is only logged.
pub(crate) async fn announce(cx: &Cx) {
    let Ok(user) = cx.author() else { return };
    let badges = match unlocked_badges(&cx.app, user.id.get()).await {
        Ok(badges) if badges.is_empty() => return,
        Ok(badges) => badges,
        Err(error) => {
            tracing::warn!(%error, "badge check failed");
            return;
        }
    };
    let (embeds, files) = unlock_notice(&cx.app.sprites, &badges, cx.lang());
    // Under a private message (an editor) the notice stays private too.
    let private = cx
        .interaction
        .message
        .as_ref()
        .and_then(|m| m.flags)
        .is_some_and(|f| f.contains(MessageFlags::EPHEMERAL));
    let data = InteractionResponseData {
        embeds: Some(embeds),
        attachments: Some(files),
        flags: private.then_some(MessageFlags::EPHEMERAL),
        ..Default::default()
    };
    if let Err(error) = cx.reply(data).await {
        tracing::warn!(%error, "badge notice not sent");
    }
}

/// Like [`announce`] for something that happened outside a command, such as a chat level-up:
/// posts the notice in `channel`, naming the player without pinging them.
pub(crate) async fn announce_in(app: &App, channel: Id<ChannelMarker>, user: u64, lang: Lang) {
    let badges = match unlocked_badges(app, user).await {
        Ok(badges) if badges.is_empty() => return,
        Ok(badges) => badges,
        Err(error) => {
            tracing::warn!(%error, "badge check failed");
            return;
        }
    };
    let (mut embeds, files) = unlock_notice(&app.sprites, &badges, lang);
    app.brand_embeds_in_channel(channel.get(), &mut embeds).await;
    let text = t!(lang, "customize.unlock.announce", user = mention(user));
    let quiet = AllowedMentions::default();
    let sent = app
        .http
        .create_message(channel)
        .content(&text)
        .allowed_mentions(Some(&quiet))
        .embeds(&embeds)
        .attachments(&files)
        .await;
    if let Err(error) = sent {
        tracing::warn!(%error, "badge notice not sent");
    }
}

#[cfg(test)]
mod tests {
    use pinchora_game::Rarity;

    use crate::commands::social::customize::unlocks::*;

    #[test]
    fn the_unlock_notice_carries_the_badge_and_at_most_three_embeds() {
        let content = Content::get();
        let sprites = Sprites::default();
        let few: Vec<&Badge> = ["fin_finder", "little_beacon"]
            .iter()
            .map(|id| content.badge(id).expect("a badge"))
            .collect();
        let (embeds, files) = unlock_notice(&sprites, &few, Lang::En);
        assert_eq!(embeds.len(), 2);
        assert_eq!(embeds[0].title.as_deref(), Some("Badge unlocked: Fin Finder"));
        assert_eq!(embeds[0].color, Some(Rarity::Common.color()));
        assert!(embeds[0].footer.is_none(), "only the last embed has the hint");
        assert!(
            embeds[1]
                .footer
                .as_ref()
                .is_some_and(|f| f.text == "See your collection with /customize badges")
        );
        // Every badge has art in the atlas: a thumbnail per embed, 96 px wide (4x the 24 px sprite).
        assert_eq!(files.len(), 2);
        assert_eq!(
            embeds[0].thumbnail.as_ref().map(|t| t.url.as_str()),
            Some("attachment://badge-fin_finder.png")
        );
        let art = image::load_from_memory(&files[0].file).expect("a png");
        assert_eq!(art.width(), 96);

        let many: Vec<&Badge> = content.badges.iter().take(5).collect();
        let (embeds, _) = unlock_notice(&sprites, &many, Lang::En);
        assert_eq!(embeds.len(), NOTICES_PER_MESSAGE);
        assert!(
            embeds
                .last()
                .and_then(|e| e.footer.as_ref())
                .is_some_and(|f| f.text.starts_with("…and 2 more. "))
        );
        let (turkish, _) = unlock_notice(&sprites, &few, Lang::Tr);
        assert_eq!(
            turkish[1].footer.as_ref().map(|f| f.text.as_str()),
            Some("Koleksiyonunu /customize badges ile gör")
        );
        assert!(
            turkish[0]
                .title
                .as_deref()
                .is_some_and(|t| t.starts_with("Rozet açıldı: "))
        );
    }
}
