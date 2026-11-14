//! Member game lookups and server tracker administration.

mod cards;
pub(crate) use cards::*;
mod riot;
use riot::*;
#[cfg(test)]
mod tests;

use pinchora_core::community::trackers;
use pinchora_core::t;
use pinchora_db::{RiotLink, TrackerSettings};
use pinchora_render::{InfoCard, InfoRow, Theme};
use serde_json::{Value, json};
use twilight_interactions::command::{CommandModel, CommandOption, CreateCommand, CreateOption};
use twilight_model::application::interaction::InteractionChannel;
use twilight_model::channel::message::{
    Component,
    component::{ActionRow, Button, ButtonStyle},
};
use twilight_model::guild::Permissions;
use twilight_util::builder::embed::EmbedBuilder;

use crate::Cx;
use crate::infra::util::BRAND;

fn manage_guild() -> Permissions {
    Permissions::MANAGE_GUILD
}

#[derive(CommandModel, CreateCommand)]
#[command(name = "game", desc = "League, Valorant and Minecraft lookups", contexts = "guild")]
pub enum GameCommand {
    #[command(name = "lol")]
    Lol(Lol),
    #[command(name = "valorant")]
    Valorant(Valorant),
    #[command(name = "mc")]
    Mc(Mc),
}
#[derive(CommandModel, CreateCommand)]
#[command(name = "lol", desc = "League of Legends account")]
pub enum Lol {
    #[command(name = "profile")]
    Profile(LolProfile),
    #[command(name = "link")]
    Link(LolLink),
    #[command(name = "unlink")]
    Unlink(LolUnlink),
}
#[derive(CommandModel, CreateCommand)]
#[command(name = "valorant", desc = "Valorant profile")]
pub enum Valorant {
    #[command(name = "profile")]
    Profile(ValorantProfile),
}
#[derive(CommandModel, CreateCommand)]
#[command(name = "mc", desc = "Minecraft servers and news")]
pub enum Mc {
    #[command(name = "status")]
    Status(McHost),
    #[command(name = "news")]
    News(McNews),
    #[command(name = "trivia")]
    Trivia(McTrivia),
}
#[derive(CommandModel, CreateCommand)]
#[command(name = "profile", desc = "Show a League account's rank and champions")]
pub struct LolProfile {
    /// Riot ID, for example Player#EUW
    #[command(min_length = 3, max_length = 64)]
    name: String,
    /// League platform
    region: LolRegion,
}
#[derive(CommandOption, CreateOption, Clone, Copy)]
pub enum LolRegion {
    #[option(name = "Europe West", value = "euw1")]
    Euw1,
    #[option(name = "North America", value = "na1")]
    Na1,
    #[option(name = "Europe Nordic", value = "eun1")]
    Eun1,
    #[option(name = "Korea", value = "kr")]
    Kr,
    #[option(name = "Brazil", value = "br1")]
    Br1,
    #[option(name = "Turkey", value = "tr1")]
    Tr1,
    #[option(name = "Japan", value = "jp1")]
    Jp1,
}
impl LolRegion {
    fn platform(self) -> &'static str {
        match self {
            Self::Euw1 => "euw1",
            Self::Na1 => "na1",
            Self::Eun1 => "eun1",
            Self::Kr => "kr",
            Self::Br1 => "br1",
            Self::Tr1 => "tr1",
            Self::Jp1 => "jp1",
        }
    }
    fn routing(self) -> &'static str {
        match self {
            Self::Na1 | Self::Br1 => "americas",
            Self::Kr | Self::Jp1 => "asia",
            _ => "europe",
        }
    }
}
#[derive(CommandModel, CreateCommand)]
#[command(name = "unlink", desc = "Remove your linked League account")]
pub struct LolUnlink;
#[derive(CommandModel, CreateCommand)]
#[command(name = "link", desc = "Link your League Riot ID")]
pub struct LolLink {
    /// Riot ID, for example Player#EUW
    #[command(min_length = 3, max_length = 64)]
    name: String,
    /// League platform
    region: LolRegion,
}
#[derive(CommandModel, CreateCommand)]
#[command(name = "profile", desc = "Valorant profile status")]
pub struct ValorantProfile;
#[derive(CommandModel, CreateCommand)]
#[command(name = "news", desc = "Latest Minecraft Java releases")]
pub struct McNews;
#[derive(CommandModel, CreateCommand)]
#[command(name = "trivia", desc = "Start a Minecraft trivia round")]
pub struct McTrivia;
#[derive(CommandModel, CreateCommand)]
#[command(name = "status", desc = "Ping a public Minecraft Java server")]
pub struct McHost {
    /// Public DNS name and optional port
    #[command(min_length = 1, max_length = 253)]
    host: String,
}

#[derive(CommandModel, CreateCommand)]
#[command(
    name = "trackers",
    desc = "Configure patch feeds and Minecraft watch list",
    default_permissions = "manage_guild",
    contexts = "guild"
)]
pub enum TrackersAdmin {
    #[command(name = "patch-add")]
    PatchAdd(PatchAdd),
    #[command(name = "patch-remove")]
    PatchRemove(PatchRemove),
    #[command(name = "patch-list")]
    PatchList(PatchList),
    #[command(name = "mc-watch")]
    McWatch(McWatch),
    #[command(name = "mc-unwatch")]
    McUnwatch(McUnwatch),
    #[command(name = "mc-news-channel")]
    McNewsChannel(McNewsChannel),
}
#[derive(CommandOption, CreateOption, Clone, Copy)]
pub enum PatchGame {
    #[option(name = "League of Legends", value = "lol")]
    Lol,
    #[option(name = "Valorant", value = "valorant")]
    Valorant,
    #[option(name = "Minecraft", value = "mc")]
    Mc,
}
impl PatchGame {
    fn key(self) -> &'static str {
        match self {
            Self::Lol => "lol",
            Self::Valorant => "valorant",
            Self::Mc => "mc",
        }
    }
}
#[derive(CommandModel, CreateCommand)]
#[command(name = "patch-add", desc = "Post official patch notes to a channel")]
pub struct PatchAdd {
    /// Game to follow
    game: PatchGame,
    /// Channel for patch notes
    #[command(channel_types = "guild_text guild_announcement")]
    channel: InteractionChannel,
}
#[derive(CommandModel, CreateCommand)]
#[command(name = "patch-remove", desc = "Stop patch notes for a game")]
pub struct PatchRemove {
    /// Game to stop following
    game: PatchGame,
}
#[derive(CommandModel, CreateCommand)]
#[command(name = "patch-list", desc = "List game patch channels")]
pub struct PatchList;
#[derive(CommandModel, CreateCommand)]
#[command(name = "mc-watch", desc = "Update a Minecraft server status every five minutes")]
pub struct McWatch {
    /// Public Minecraft host and optional port
    #[command(min_length = 1, max_length = 253)]
    host: String,
    /// Channel for status updates
    #[command(channel_types = "guild_text guild_announcement")]
    channel: InteractionChannel,
}
#[derive(CommandModel, CreateCommand)]
#[command(name = "mc-unwatch", desc = "Stop watching a Minecraft server")]
pub struct McUnwatch {
    /// Minecraft host to remove
    #[command(min_length = 1, max_length = 253)]
    host: String,
}
#[derive(CommandModel, CreateCommand)]
#[command(name = "mc-news-channel", desc = "Set the Minecraft news channel")]
pub struct McNewsChannel {
    /// Channel for Minecraft news
    #[command(channel_types = "guild_text guild_announcement")]
    channel: Option<InteractionChannel>,
    /// Clear the channel
    clear: Option<bool>,
}

async fn lol_profile(cx: &Cx, input: LolProfile, save: bool) -> anyhow::Result<()> {
    let Some(key) = riot_key() else {
        return cx.reply_ephemeral(t!(cx.lang(), "trackers.no-riot-key")).await;
    };
    let Some((name, tag)) = riot_id(&input.name) else {
        return cx.reply_ephemeral(t!(cx.lang(), "trackers.bad-riot-id")).await;
    };
    let region = input.region;
    let result = async {
        let account = riot(
            cx,
            &key,
            region.routing(),
            &["riot", "account", "v1", "accounts", "by-riot-id", name, tag],
        )
        .await?;
        let puuid = account["puuid"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("missing puuid"))?;
        let rank = riot(cx, &key, region.platform(), &league_path(puuid)).await?;
        let mastery = riot(
            cx,
            &key,
            region.platform(),
            &[
                "lol",
                "champion-mastery",
                "v4",
                "champion-masteries",
                "by-puuid",
                puuid,
                "top",
            ],
        )
        .await?;
        Ok::<_, anyhow::Error>((puuid.to_owned(), rank, mastery))
    }
    .await;
    let (puuid, rank, mastery) = match result {
        Ok(result) => result,
        Err(_) => {
            tracing::warn!("Riot lookup failed");
            return cx.reply_ephemeral(t!(cx.lang(), "trackers.riot-unavailable")).await;
        }
    };
    if save {
        cx.app
            .db
            .save_riot_link(
                cx.author()?.id.get(),
                "lol",
                &RiotLink {
                    puuid,
                    game_name: name.into(),
                    tag_line: tag.into(),
                    region: region.platform().into(),
                },
            )
            .await?;
    }
    let solo = rank
        .as_array()
        .and_then(|entries| entries.iter().find(|e| e["queueType"] == "RANKED_SOLO_5x5"));
    let rank_text = solo.map_or_else(
        || t!(cx.lang(), "trackers.unranked"),
        |v| {
            format!(
                "{} {} · {} LP",
                v["tier"].as_str().unwrap_or("?"),
                v["rank"].as_str().unwrap_or("?"),
                v["leaguePoints"].as_i64().unwrap_or(0)
            )
        },
    );
    let rank_rows = vec![
        row(t!(cx.lang(), "trackers.region"), region.platform()),
        row(t!(cx.lang(), "trackers.solo-rank"), rank_text),
    ];
    let mut mastery_rows = Vec::new();
    let dragon = dragon(cx).await.ok();
    let mut icon = None;
    for (i, entry) in mastery.as_array().into_iter().flatten().take(3).enumerate() {
        let id = entry["championId"].as_i64().unwrap_or(0);
        let unknown = t!(cx.lang(), "trackers.unknown");
        let (name, file) = dragon
            .as_ref()
            .and_then(|(_, data)| champion(data, id))
            .unwrap_or((&unknown, ""));
        mastery_rows.push(row(
            format!("{} {}", t!(cx.lang(), "trackers.mastery"), i + 1),
            format!("{name} · {}", entry["championPoints"].as_i64().unwrap_or(0)),
        ));
        if i == 0 && !file.is_empty() {
            let url = format!(
                "https://ddragon.leagueoflegends.com/cdn/{}/img/champion/{file}",
                dragon.as_ref().unwrap().0
            );
            if let Ok(response) = crate::community::trackers::http_client()?.get(url).send().await
                && let Ok(response) = response.error_for_status()
                && let Ok(bytes) = crate::community::trackers::bounded_body(response, 1024 * 1024).await
            {
                icon = cx.app.render(move |_| tracker_icon(&bytes)).await.ok();
            }
        }
    }
    let title = format!("{name}#{tag}");
    let token = format!("{:032x}", rand::random::<u128>());
    let encode_rows = |rows: &[InfoRow]| {
        rows.iter()
            .map(|r| json!({"label":r.label,"value":r.value}))
            .collect::<Vec<_>>()
    };
    cx.app
        .db
        .tracker_user_cache_put(
            cx.author()?.id.get(),
            &format!("riot:card:{token}"),
            &json!({
                "title":title,
                "rank":encode_rows(&rank_rows),
                "mastery":encode_rows(&mastery_rows),
                "image":icon,
            }),
        )
        .await?;
    send_card(
        cx,
        title,
        t!(cx.lang(), "trackers.league"),
        rank_rows,
        icon,
        tabs(cx, &token, "rank"),
        false,
    )
    .await
}

impl GameCommand {
    pub async fn run(self, cx: &Cx) -> anyhow::Result<()> {
        match self {
            Self::Lol(Lol::Profile(input)) => lol_profile(cx, input, false).await,
            Self::Lol(Lol::Link(input)) => {
                lol_profile(
                    cx,
                    LolProfile {
                        name: input.name,
                        region: input.region,
                    },
                    true,
                )
                .await
            }
            Self::Lol(Lol::Unlink(_)) => {
                cx.app.db.delete_riot_link(cx.author()?.id.get(), "lol").await?;
                cx.reply_ephemeral(t!(cx.lang(), "trackers.unlinked")).await
            }
            Self::Valorant(_) => {
                cx.reply_ephemeral(t!(
                    cx.lang(),
                    if riot_prod() {
                        "trackers.valorant-rso-required"
                    } else {
                        "trackers.valorant-waiting"
                    }
                ))
                .await
            }
            Self::Mc(Mc::Status(input)) => match crate::community::trackers::mc_status(&cx.app, &input.host).await {
                Ok(status) => {
                    let image = if let Some(bytes) = status.favicon.as_deref().and_then(trackers::decode_favicon) {
                        cx.app.render(move |_| tracker_icon(&bytes)).await.ok()
                    } else {
                        None
                    };
                    card_with_image(
                        cx,
                        input.host,
                        t!(cx.lang(), "trackers.minecraft"),
                        vec![
                            row(
                                t!(cx.lang(), "trackers.version"),
                                if status.version.is_empty() {
                                    t!(cx.lang(), "trackers.unknown")
                                } else {
                                    status.version
                                },
                            ),
                            row(
                                t!(cx.lang(), "trackers.players"),
                                format!("{} / {}", status.online, status.max),
                            ),
                            row(t!(cx.lang(), "trackers.motd"), status.motd),
                        ],
                        image,
                    )
                    .await
                }
                Err(error) => {
                    tracing::debug!(%error, "Minecraft ping failed");
                    cx.reply_ephemeral(t!(cx.lang(), "trackers.mc-unavailable")).await
                }
            },
            Self::Mc(Mc::News(_)) => match crate::community::trackers::manifest(&cx.app).await {
                Ok(manifest) => {
                    let mut rows: Vec<_> = trackers::release_ids(&manifest)
                        .into_iter()
                        .take(3)
                        .enumerate()
                        .map(|(i, id)| row(format!("#{}", i + 1), id))
                        .collect();
                    if let Ok(news) = crate::community::trackers::launcher_news(&cx.app).await {
                        rows.extend(
                            crate::community::trackers::java_news(&news)
                                .into_iter()
                                .take(2)
                                .map(|(title, link)| row(title, link)),
                        );
                    }
                    card(
                        cx,
                        t!(cx.lang(), "trackers.minecraft"),
                        t!(cx.lang(), "trackers.latest-releases"),
                        rows,
                    )
                    .await
                }
                Err(error) => {
                    tracing::warn!(%error,"Minecraft manifest failed");
                    cx.reply_ephemeral(t!(cx.lang(), "trackers.news-unavailable")).await
                }
            },
            Self::Mc(Mc::Trivia(_)) => {
                crate::commands::games::trivia::start(cx, Some(pinchora_game::play::TriviaCategory::Minecraft)).await
            }
        }
    }
}
fn posting_channel(guild: u64, channel: &twilight_model::channel::Channel) -> bool {
    channel.guild_id.map(|id| id.get()) == Some(guild)
        && matches!(
            channel.kind,
            twilight_model::channel::ChannelType::GuildText | twilight_model::channel::ChannelType::GuildAnnouncement
        )
}

impl TrackersAdmin {
    pub async fn run(self, cx: &Cx) -> anyhow::Result<()> {
        let Some(guild) = cx.guild_id() else {
            return cx.reply_ephemeral(t!(cx.lang(), "error.guild-only")).await;
        };
        if !cx.can_manage_guild() {
            return cx.reply_ephemeral(t!(cx.lang(), "error.needs-manage-server")).await;
        }
        let target = match &self {
            Self::PatchAdd(c) => Some(c.channel.id),
            Self::McWatch(c) => Some(c.channel.id),
            Self::McNewsChannel(c) if !c.clear.unwrap_or(false) => c.channel.as_ref().map(|c| c.id),
            _ => None,
        };
        if let Some(target) = target {
            let channel = cx.app.http.channel(target).await?.model().await?;
            if !posting_channel(guild, &channel) {
                return cx.reply_ephemeral(t!(cx.lang(), "trackers.invalid-channel")).await;
            }
        }
        let mut settings: TrackerSettings = cx.app.db.tracker_settings(guild).await?;
        match self {
            Self::PatchAdd(c) => {
                settings.patch_channels[c.game.key()] = json!(c.channel.id.get().to_string());
            }
            Self::PatchRemove(c) => {
                settings.patch_channels.as_object_mut().map(|m| m.remove(c.game.key()));
            }
            Self::PatchList(_) => {
                return cx
                    .reply_ephemeral(format!(
                        "{}\n{}",
                        t!(cx.lang(), "trackers.patch-channels"),
                        settings.patch_channels
                    ))
                    .await;
            }
            Self::McWatch(c) => {
                let Ok((host, port)) = trackers::parse_host(&c.host) else {
                    return cx.reply_ephemeral(t!(cx.lang(), "trackers.invalid-host")).await;
                };
                let host = format!("{host}:{port}");
                let watches = settings
                    .mc_watch
                    .as_array_mut()
                    .ok_or_else(|| anyhow::anyhow!("invalid watch list"))?;
                watches.retain(|v| v["host"] != host);
                if watches.len() >= 20 {
                    return cx.reply_ephemeral(t!(cx.lang(), "trackers.invalid-settings")).await;
                }
                watches.push(json!({"host": host, "channel_id": c.channel.id.get().to_string()}));
            }
            Self::McUnwatch(c) => {
                let Ok((host, port)) = trackers::parse_host(&c.host) else {
                    return cx.reply_ephemeral(t!(cx.lang(), "trackers.invalid-host")).await;
                };
                let host = format!("{host}:{port}");
                settings
                    .mc_watch
                    .as_array_mut()
                    .ok_or_else(|| anyhow::anyhow!("invalid watch list"))?
                    .retain(|v| v["host"] != host);
            }
            Self::McNewsChannel(c) => {
                settings.mc_news_channel_id = if c.clear.unwrap_or(false) {
                    None
                } else {
                    c.channel.map(|c| c.id.get() as i64)
                };
            }
        }
        cx.app.db.save_tracker_settings(guild, &settings).await?;
        cx.reply_ephemeral(t!(cx.lang(), "trackers.saved")).await
    }
}
