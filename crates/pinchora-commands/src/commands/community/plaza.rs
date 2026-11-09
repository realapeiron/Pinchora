//! The server plaza: one shared house per server. Members donate furniture to it,
//! managers arrange it with the same editor as houses. Discord ids never collide
//! across entity types, so the plaza is simply a house owned by the guild id.

use pinchora_core::t;
use pinchora_game::Content;
use twilight_interactions::command::{CommandModel, CreateCommand};
use twilight_model::{
    channel::message::{
        Component, MessageFlags,
        component::{ActionRow, SelectMenu, SelectMenuOption, SelectMenuType},
    },
    guild::Permissions,
    http::interaction::InteractionResponseData,
    id::Id,
};

use crate::Cx;
use crate::commands::economy::shop::soulbound_rule;
use crate::commands::games::collection::giveable;
use crate::commands::house::house::CursorMode;
use crate::commands::house::house::EditState;
use crate::commands::house::house::editor;
use crate::commands::house::house::ensure_house;
use crate::commands::house::house::room_card;
use crate::infra::util::mention;

#[derive(CommandModel, CreateCommand)]
#[command(name = "plaza", desc = "Your server's shared plaza", contexts = "guild")]
pub enum PlazaCommand {
    #[command(name = "view")]
    View(PlazaView),
    #[command(name = "donate")]
    Donate(PlazaDonate),
    #[command(name = "edit")]
    Edit(PlazaEdit),
}

#[derive(CommandModel, CreateCommand)]
#[command(name = "view", desc = "Look around the server plaza")]
pub struct PlazaView;

#[derive(CommandModel, CreateCommand)]
#[command(name = "donate", desc = "Give a piece of furniture to the server plaza")]
pub struct PlazaDonate;

#[derive(CommandModel, CreateCommand)]
#[command(name = "edit", desc = "Arrange the plaza's donated furniture (Manage Server)")]
pub struct PlazaEdit;

pub(crate) async fn guild_name(cx: &Cx, guild: u64) -> String {
    let fallback = t!(cx.lang(), "plaza.default-name");
    match cx.app.http.guild(Id::new(guild.max(1))).await {
        Ok(response) => response.model().await.map_or_else(|_| fallback, |g| g.name),
        Err(_) => fallback,
    }
}

/// The guild id when the clicker may arrange this server's plaza.
pub(crate) fn editable_plaza(cx: &Cx) -> Option<u64> {
    let perms = cx.interaction.member.as_ref()?.permissions?;
    (perms.contains(Permissions::MANAGE_GUILD) || perms.contains(Permissions::ADMINISTRATOR))
        .then(|| cx.guild_id())
        .flatten()
}

impl PlazaCommand {
    pub async fn run(self, cx: &Cx) -> anyhow::Result<()> {
        let lang = cx.lang();
        let Some(guild) = cx.guild_id() else {
            return cx.reply_ephemeral(t!(lang, "plaza.guild-only")).await;
        };
        match self {
            Self::View(_) => {
                let name = guild_name(cx, guild).await;
                let data = room_card(cx, guild, &name, 0, None, Some(cx.author()?.id.get())).await?;
                cx.reply(data).await
            }
            Self::Edit(_) => {
                if editable_plaza(cx).is_none() {
                    return cx.reply_ephemeral(t!(lang, "plaza.managers-only")).await;
                }
                ensure_house(&cx.app, guild).await?;
                let state = EditState {
                    prefix: "pe",
                    room: 0,
                    x: 1,
                    y: 0,
                    page: 0,
                    flipped: false,
                    mode: CursorMode::Floor,
                    item: None,
                };
                let data = editor(cx, guild, state, &t!(lang, "plaza.editing"), None).await?;
                cx.reply(data).await
            }
            Self::Donate(_) => {
                let content = Content::get();
                let (rows, kept_back) = giveable(cx.app.db.inventory(cx.author()?.id.get()).await?);
                let options: Vec<SelectMenuOption> = rows
                    .into_iter()
                    .filter_map(|row| {
                        let item = content.item(&row.item_id)?;
                        Some(SelectMenuOption {
                            default: false,
                            description: Some(t!(lang, "plaza.you-have", count = row.quantity)),
                            emoji: None,
                            label: item.name_in(lang),
                            value: row.item_id,
                        })
                    })
                    .collect();
                if options.is_empty() {
                    return cx
                        .reply_ephemeral(if kept_back {
                            t!(lang, "plaza.nothing-donatable", rule = soulbound_rule(lang))
                        } else {
                            t!(lang, "inventory.empty")
                        })
                        .await;
                }
                cx.reply(InteractionResponseData {
                    content: Some(t!(lang, "plaza.donate-prompt")),
                    components: Some(vec![Component::ActionRow(ActionRow {
                        id: None,
                        components: vec![Component::SelectMenu(SelectMenu {
                            channel_types: None,
                            custom_id: "pd".to_owned(),
                            default_values: None,
                            disabled: false,
                            id: None,
                            kind: SelectMenuType::Text,
                            max_values: Some(1),
                            min_values: Some(1),
                            options: Some(options),
                            placeholder: Some(t!(lang, "plaza.donate-placeholder")),
                            required: None,
                        })],
                    })]),
                    flags: Some(MessageFlags::EPHEMERAL),
                    ..Default::default()
                })
                .await
            }
        }
    }
}

/// `pd` select: move one item into the plaza's inventory.
pub(crate) async fn on_donate(cx: &Cx, value: Option<&str>) -> anyhow::Result<()> {
    let lang = cx.lang();
    let Some(guild) = cx.guild_id() else {
        return cx.reply_ephemeral(t!(lang, "plaza.guild-only")).await;
    };
    let Some(item) = value.and_then(|v| Content::get().item(v)) else {
        return cx.reply_ephemeral(t!(lang, "plaza.pick-something")).await;
    };
    let donor = cx.author()?.id.get();
    if item.bound {
        return cx
            .reply_ephemeral(t!(
                lang,
                "plaza.cant-donate",
                name = item.name_in(lang),
                rule = soulbound_rule(lang)
            ))
            .await;
    }
    ensure_house(&cx.app, guild).await?;
    if !cx.app.db.gift_item(donor, guild, &item.id()).await? {
        return cx
            .reply_ephemeral(t!(lang, "plaza.none-left", name = item.name_in(lang)))
            .await;
    }
    cx.update(InteractionResponseData {
        content: Some(t!(lang, "plaza.donated")),
        components: Some(Vec::new()),
        ..Default::default()
    })
    .await?;
    cx.reply_text(t!(
        lang,
        "plaza.donated-news",
        donor = mention(donor),
        name = item.name_in(lang)
    ))
    .await
}
