use pinchora_core::t;
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
    commands::{collection::giveable, shop::soulbound_rule, social::ping_only},
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
        let lang = cx.lang();
        if target.bot || target.id == author {
            return cx.reply_ephemeral(t!(lang, "gift.for-friends")).await;
        }
        let content = Content::get();
        let (rows, kept_back) = giveable(cx.app.db.inventory(author.get()).await?);
        let options: Vec<SelectMenuOption> = rows
            .into_iter()
            .filter_map(|row| {
                let item = content.item(&row.item_id)?;
                Some(SelectMenuOption {
                    default: false,
                    description: Some(t!(lang, "gift.you-have", count = row.quantity)),
                    emoji: None,
                    label: item.name_in(lang),
                    value: row.item_id,
                })
            })
            .collect();
        if options.is_empty() {
            return cx
                .reply_ephemeral(if kept_back {
                    t!(lang, "gift.nothing-giveable", rule = soulbound_rule(lang))
                } else {
                    t!(lang, "gift.empty")
                })
                .await;
        }
        cx.reply(InteractionResponseData {
            content: Some(t!(lang, "gift.prompt", user = mention(target.id))),
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
                    placeholder: Some(t!(lang, "gift.placeholder")),
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
    let lang = cx.lang();
    let Some(item) = value.and_then(|v| Content::get().item(v)) else {
        return cx.reply_ephemeral(t!(lang, "gift.pick-something")).await;
    };
    if to == 0 || to == from {
        return cx.reply_ephemeral(t!(lang, "gift.for-friends")).await;
    }
    if item.bound {
        return cx
            .reply_ephemeral(t!(
                lang,
                "gift.cant-give",
                name = item.name_in(lang),
                rule = soulbound_rule(lang)
            ))
            .await;
    }
    if !cx.app.db.gift_item(from, to, &item.id()).await? {
        return cx
            .reply_ephemeral(t!(lang, "gift.none-left", name = item.name_in(lang)))
            .await;
    }
    cx.update(InteractionResponseData {
        content: Some(t!(lang, "gift.sent")),
        components: Some(Vec::new()),
        ..Default::default()
    })
    .await?;
    // The one who gets the gift is told; the sender is not pinged for their own gift.
    cx.reply(InteractionResponseData {
        content: Some(t!(
            lang,
            "gift.announce",
            giver = mention(from),
            friend = mention(to),
            name = item.name_in(lang)
        )),
        allowed_mentions: ping_only(to),
        ..Default::default()
    })
    .await
}
