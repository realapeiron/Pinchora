//! Release the delayed join effects once, after the gate transition succeeds.
use crate::App;
use crate::admin::auto;
use crate::admin::card_images::CardSubject;
use crate::admin::card_images::card_png;
use crate::admin::card_images::deliver;
use crate::admin::card_images::server_card;
use crate::community::events::default_message;
use crate::infra::util::display_name;
use crate::infra::util::mention;
use crate::moderation::verification::safety;
use crate::progression::leveling;
use crate::roles::roles;
use pinchora_core::admin::card_design::CardKind;
use pinchora_core::admin::card_design::Delivery;
use pinchora_core::admin::settings::MessageKind;
use std::sync::Arc;
use twilight_model::{
    channel::message::{AllowedMentions, MentionType},
    gateway::payload::incoming::MemberAdd,
    guild::Member,
    id::Id,
};
pub(super) async fn member(app: &Arc<App>, guild: u64, member: Member) -> anyhow::Result<()> {
    let guild_id = Id::new(guild);
    let settings = app.guild_settings(guild).await?;
    crate::commands::roles::automation::on_member_join(
        app,
        &MemberAdd {
            guild_id,
            member: member.clone(),
        },
    )
    .await;
    crate::commands::roles::automation::give_after_verify(app, guild, member.user.id.get()).await;
    if let Some(role) = settings.join_role_id {
        roles::give_join_role(app, guild, member.user.id.get(), role as u64).await;
    }
    auto::member_joined(app, guild, &member).await;
    if !settings.welcome_enabled {
        return Ok(());
    }
    let card = server_card(app, &settings, guild_id.get(), CardKind::Welcome).await?;
    let channel = match card.delivery {
        Delivery::Dm => None,
        _ => match settings.welcome_channel_id {
            Some(channel) => Some(Id::new(channel as u64)),
            None => return Ok(()),
        },
    };
    let guild = safety::request(app, app.http.guild(guild_id).with_counts(true))
        .await?
        .model()
        .await?;
    // The welcome answers nobody in particular: it speaks the server's language.
    let lang = app.guild_lang(guild_id.get()).await;
    let count = guild.approximate_member_count;
    let mut text = settings
        .welcome_message
        .clone()
        .unwrap_or_else(|| default_message(lang, MessageKind::Welcome))
        .replace("{user}", &mention(member.user.id))
        .replace("{server}", &guild.name)
        .replace(
            "{count}",
            &count
                .map(|c| pinchora_core::admin::server_stats::grouped_in(lang, c as i64))
                .unwrap_or_default(),
        );
    let mut mass = false;
    if let Some((chosen, allowed)) = crate::social::message_studio::event_text(
        app,
        guild_id.get(),
        CardKind::Welcome,
        crate::social::message_studio::EventMember {
            user_id: member.user.id.get(),
            user: &mention(member.user.id),
            server: &guild.name,
            level: None,
            booster: member.premium_since.is_some(),
            returning: app.db.card_was_returning(guild_id.get(), member.user.id.get()).await?,
            ordinal: count,
        },
    )
    .await?
    {
        text = chosen;
        mass = allowed;
    }
    let png = if card.delivery.has_image() {
        let subject = CardSubject {
            user: member.user.id,
            avatar: member.user.avatar,
            name: display_name(&member.user, member.nick.as_deref()),
            username: member.user.name.clone(),
            server: guild.name.clone(),
            members: count,
        };
        Some(card_png(app, guild_id.get(), &card.design, &subject).await?)
    } else {
        None
    };
    let mut mentions = AllowedMentions {
        users: vec![member.user.id],
        ..Default::default()
    };
    if mass {
        mentions.parse = vec![MentionType::Everyone, MentionType::Roles];
    }
    let image = png.map(|png| ("welcome.png", png, card.design.alt_text.as_str()));
    match channel {
        Some(channel) => deliver(app, channel, card.delivery, &text, image, &mentions).await,
        None => {
            // Closed direct messages are the member's choice, not a failure.
            let user = member.user.id.get();
            let Some(dm) = leveling::private_channel(app, user).await else {
                return Ok(());
            };
            if let Err(error) = deliver(app, dm, Delivery::WithMessage, &text, image, &mentions).await {
                tracing::debug!(guild = guild_id.get(), user, %error, "welcome direct message not delivered");
            }
            Ok(())
        }
    }
}
