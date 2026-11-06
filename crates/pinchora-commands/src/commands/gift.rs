use pinchora_game::Content;
use twilight_interactions::command::{CommandModel, CreateCommand, ResolvedUser};
use twilight_model::{
    channel::message::{
        Component, MessageFlags,
        component::{ActionRow, SelectMenu, SelectMenuOption, SelectMenuType},
    },
    http::interaction::InteractionResponseData,
};

use crate::{
    Cx,
    commands::{
        collection::{SOULBOUND_RULE, giveable},
        social::ping_only,
    },
    util::mention,
};

#[derive(CommandModel, CreateCommand)]
#[command(name = "gift", desc = "Send a piece of furniture to a friend")]
pub struct Gift {
    /// Who gets the gift
    user: ResolvedUser,
}

impl Gift {
    pub async fn run(self, cx: &Cx) -> anyhow::Result<()> {
        let author = cx.author()?.id;
        let target = &self.user.resolved;
        if target.bot || target.id == author {
            return cx.reply_ephemeral("Gifts are for friends!").await;
        }
        let content = Content::get();
        let (rows, kept_back) = giveable(cx.app.db.inventory(author.get()).await?);
        let options: Vec<SelectMenuOption> = rows
            .into_iter()
            .filter_map(|row| {
                let item = content.item(&row.item_id)?;
                Some(SelectMenuOption {
                    default: false,
                    description: Some(format!("You have {}", row.quantity)),
                    emoji: None,
                    label: item.name(),
                    value: row.item_id,
                })
            })
            .collect();
        if options.is_empty() {
            return cx
                .reply_ephemeral(if kept_back {
                    format!("Nothing in your inventory can be sent as a gift. {SOULBOUND_RULE}")
                } else {
                    "Your inventory is empty. Pick up something in `/house edit` or visit `/shop`.".to_owned()
                })
                .await;
        }
        cx.reply(InteractionResponseData {
            content: Some(format!("What do you want to send to {}?", mention(target.id))),
            components: Some(vec![Component::ActionRow(ActionRow {
                id: None,
                components: vec![Component::SelectMenu(SelectMenu {
                    channel_types: None,
                    custom_id: format!("gf:{}", target.id),
                    default_values: None,
                    disabled: false,
                    id: None,
                    kind: SelectMenuType::Text,
                    max_values: Some(1),
                    min_values: Some(1),
                    options: Some(options),
                    placeholder: Some("Choose a gift…".to_owned()),
                    required: None,
                })],
            })]),
            flags: Some(MessageFlags::EPHEMERAL),
            ..Default::default()
        })
        .await
    }
}

/// `gf:<to>` select.
pub(crate) async fn on_gift(cx: &Cx, mut args: std::str::Split<'_, char>, value: Option<&str>) -> anyhow::Result<()> {
    let to: u64 = args.next().and_then(|v| v.parse().ok()).unwrap_or(0);
    let from = cx.author()?.id.get();
    let Some(item) = value.and_then(|v| Content::get().item(v)) else {
        return cx.reply_ephemeral("Pick something to send.").await;
    };
    if to == 0 || to == from {
        return cx.reply_ephemeral("Gifts are for friends!").await;
    }
    if item.bound {
        return cx
            .reply_ephemeral(format!("**{}** can't be given away. {SOULBOUND_RULE}", item.name()))
            .await;
    }
    if !cx.app.db.gift_item(from, to, &item.id()).await? {
        return cx
            .reply_ephemeral(format!("You don't have a **{}** anymore.", item.name()))
            .await;
    }
    cx.update(InteractionResponseData {
        content: Some("🎁 Sent!".to_owned()),
        components: Some(Vec::new()),
        ..Default::default()
    })
    .await?;
    // The one who gets the gift is told; the sender is not pinged for their own gift.
    cx.reply(InteractionResponseData {
        content: Some(format!(
            "🎁 {} sent {} a **{}**! It's waiting in their `/inventory`.",
            mention(from),
            mention(to),
            item.name()
        )),
        allowed_mentions: ping_only(to),
        ..Default::default()
    })
    .await
}
