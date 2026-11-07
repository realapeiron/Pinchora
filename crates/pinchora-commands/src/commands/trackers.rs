//! Member game lookups and server tracker administration.
use pinchora_core::{t, trackers};
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

use crate::{Cx, util::BRAND};

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

async fn card(cx: &Cx, title: String, subtitle: String, rows: Vec<InfoRow>) -> anyhow::Result<()> {
    card_with_image(cx, title, subtitle, rows, None).await
}
async fn card_with_image(
    cx: &Cx,
    title: String,
    subtitle: String,
    rows: Vec<InfoRow>,
    image: Option<Vec<u8>>,
) -> anyhow::Result<()> {
    send_card(cx, title, subtitle, rows, image, Vec::new(), false).await
}

async fn send_card(
    cx: &Cx,
    title: String,
    subtitle: String,
    rows: Vec<InfoRow>,
    image: Option<Vec<u8>>,
    components: Vec<Component>,
    update: bool,
) -> anyhow::Result<()> {
    let lang = cx.lang();
    let png = cx
        .app
        .render(move |r| {
            r.svg_to_png(
                &InfoCard {
                    lang,
                    title,
                    subtitle,
                    rows,
                    image,
                    theme: Theme::reef(),
                }
                .to_svg(),
            )
        })
        .await?;
    let data = Cx::card(EmbedBuilder::new().color(BRAND), "game.png", png, components, false);
    if update {
        cx.update(data).await
    } else {
        cx.reply(data).await
    }
}

fn tabs(cx: &Cx, token: &str, selected: &str) -> Vec<Component> {
    let button = |tab: &str, label: String| {
        Component::Button(Button {
            custom_id: Some(format!("gt:{token}:{tab}")),
            disabled: tab == selected,
            emoji: None,
            id: None,
            label: Some(label),
            sku_id: None,
            style: if tab == selected {
                ButtonStyle::Primary
            } else {
                ButtonStyle::Secondary
            },
            url: None,
        })
    };
    vec![Component::ActionRow(ActionRow {
        id: None,
        components: vec![
            button("rank", t!(cx.lang(), "trackers.rank-tab")),
            button("mastery", t!(cx.lang(), "trackers.mastery-tab")),
        ],
    })]
}

pub(crate) async fn on_component<'a>(cx: &Cx, mut parts: impl Iterator<Item = &'a str>) -> anyhow::Result<()> {
    let (Some(token), Some(tab), None) = (parts.next(), parts.next(), parts.next()) else {
        return cx.reply_ephemeral(t!(cx.lang(), "trackers.card-expired")).await;
    };
    if token.len() != 32 || !token.bytes().all(|b| b.is_ascii_hexdigit()) || !matches!(tab, "rank" | "mastery") {
        return cx.reply_ephemeral(t!(cx.lang(), "trackers.card-expired")).await;
    }
    let Some(data) = cx.app.db.tracker_cached(&format!("riot:card:{token}"), 300).await? else {
        return cx.reply_ephemeral(t!(cx.lang(), "trackers.card-expired")).await;
    };
    let Some(title) = data["title"].as_str() else {
        return cx.reply_ephemeral(t!(cx.lang(), "trackers.card-expired")).await;
    };
    let rows = data[tab]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|value| {
            Some(row(
                value["label"].as_str()?.to_owned(),
                value["value"].as_str()?.to_owned(),
            ))
        })
        .collect();
    let image = serde_json::from_value::<Option<Vec<u8>>>(data["image"].clone())
        .ok()
        .flatten();
    send_card(
        cx,
        title.to_owned(),
        t!(cx.lang(), "trackers.league"),
        rows,
        image,
        tabs(cx, token, tab),
        true,
    )
    .await
}

fn champion(data: &Value, id: i64) -> Option<(&str, &str)> {
    data["data"]
        .as_object()?
        .values()
        .find(|v| v["key"].as_str().is_some_and(|key| key.parse::<i64>().ok() == Some(id)))
        .and_then(|v| Some((v["name"].as_str()?, v["image"]["full"].as_str()?)))
}

async fn dragon(cx: &Cx) -> anyhow::Result<(String, Value)> {
    let version = if let Some(value) = cx.app.db.tracker_cached("dragon:version", 86400).await? {
        value.as_str().unwrap_or_default().to_owned()
    } else {
        let versions: Value = cx
            .app
            .web
            .get("https://ddragon.leagueoflegends.com/api/versions.json")
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        let latest = versions[0]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("missing Data Dragon version"))?
            .to_owned();
        cx.app.db.tracker_cache_put("dragon:version", &json!(latest)).await?;
        latest
    };
    let cache_key = format!("dragon:{version}:champions");
    if let Some(data) = cx.app.db.tracker_cached(&cache_key, 86400).await? {
        return Ok((version, data));
    }
    let data: Value = cx
        .app
        .web
        .get(format!(
            "https://ddragon.leagueoflegends.com/cdn/{version}/data/en_US/champion.json"
        ))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    cx.app.db.tracker_cache_put(&cache_key, &data).await?;
    Ok((version, data))
}
fn row(label: String, value: impl Into<String>) -> InfoRow {
    InfoRow {
        label,
        value: value.into(),
    }
}

fn riot_key() -> Option<String> {
    std::env::var("PINCHORA_TRACKERS__RIOT_API_KEY")
        .ok()
        .filter(|s| !s.is_empty())
}
fn riot_prod() -> bool {
    std::env::var("PINCHORA_TRACKERS__RIOT_PRODUCTION").is_ok_and(|s| s == "true" || s == "1")
}

#[derive(Debug)]
struct RiotRateLimited(u64);
impl std::fmt::Display for RiotRateLimited {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Riot rate limited for {} seconds", self.0)
    }
}
impl std::error::Error for RiotRateLimited {}

async fn riot(cx: &Cx, key: &str, region: &str, path: &[&str]) -> anyhow::Result<Value> {
    let cache_key = format!("riot:{region}:{}", path.join(":"));
    if let Some(value) = cx.app.db.tracker_cached(&cache_key, 300).await? {
        return Ok(value);
    }
    anyhow::ensure!(cx.app.cache.reserve_riot(region).await?, "rate_limited");
    let mut url = url::Url::parse(&format!("https://{region}.api.riotgames.com/"))?;
    url.path_segments_mut()
        .map_err(|_| anyhow::anyhow!("invalid URL"))?
        .extend(path);
    let value = match riot_http_get(&cx.app.web, url, key).await {
        Ok(value) => value,
        Err(error) => {
            if let Some(limit) = error.downcast_ref::<RiotRateLimited>() {
                cx.app.cache.block_riot(region, limit.0).await?;
            }
            return Err(error);
        }
    };
    cx.app.db.tracker_cache_put(&cache_key, &value).await?;
    Ok(value)
}

async fn riot_http_get(client: &reqwest::Client, url: url::Url, key: &str) -> anyhow::Result<Value> {
    let response = client.get(url).header("X-Riot-Token", key).send().await?;
    if response.status().as_u16() == 429 {
        let seconds = response
            .headers()
            .get("retry-after")
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(60);
        return Err(RiotRateLimited(seconds.clamp(1, 120)).into());
    }
    anyhow::ensure!(
        response.status().is_success(),
        "riot_status_{}",
        response.status().as_u16()
    );
    Ok(response.json().await?)
}

async fn lol_profile(cx: &Cx, input: LolProfile, save: bool) -> anyhow::Result<()> {
    let Some(key) = riot_key() else {
        return cx.reply_ephemeral(t!(cx.lang(), "trackers.no-riot-key")).await;
    };
    let Some((name, tag)) = input.name.split_once('#') else {
        return cx.reply_ephemeral(t!(cx.lang(), "trackers.bad-riot-id")).await;
    };
    if name.is_empty() || tag.is_empty() {
        return cx.reply_ephemeral(t!(cx.lang(), "trackers.bad-riot-id")).await;
    }
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
        let summoner = riot(
            cx,
            &key,
            region.platform(),
            &["lol", "summoner", "v4", "summoners", "by-puuid", puuid],
        )
        .await?;
        let id = summoner["id"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("missing summoner id"))?;
        let rank = riot(
            cx,
            &key,
            region.platform(),
            &["lol", "league", "v4", "entries", "by-summoner", id],
        )
        .await?;
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
        Err(error) => {
            tracing::warn!(%error, "Riot lookup failed");
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
        let (name, file) = dragon
            .as_ref()
            .and_then(|(_, data)| champion(data, id))
            .unwrap_or(("Unknown", ""));
        mastery_rows.push(row(
            format!("{} {}", t!(cx.lang(), "trackers.mastery"), i + 1),
            format!("{name} · {}", entry["championPoints"].as_i64().unwrap_or(0)),
        ));
        if i == 0 && !file.is_empty() {
            let url = format!(
                "https://ddragon.leagueoflegends.com/cdn/{}/img/champion/{file}",
                dragon.as_ref().unwrap().0
            );
            if let Ok(response) = cx.app.web.get(url).send().await
                && let Ok(response) = response.error_for_status()
                && let Ok(bytes) = response.bytes().await
                && bytes.len() < 1024 * 1024
            {
                icon = cx
                    .app
                    .render(move |_| pinchora_render::prepare_avatar(&bytes, 160, Some(40)))
                    .await
                    .ok();
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
        .tracker_cache_put(
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
            Self::Mc(Mc::Status(input)) => match trackers::ping(&input.host).await {
                Ok(status) => {
                    let image = if let Some(bytes) = status.favicon.as_deref().and_then(trackers::decode_favicon) {
                        cx.app
                            .render(move |_| pinchora_render::prepare_avatar(&bytes, 160, Some(40)))
                            .await
                            .ok()
                    } else {
                        None
                    };
                    card_with_image(
                        cx,
                        input.host,
                        t!(cx.lang(), "trackers.minecraft"),
                        vec![
                            row(t!(cx.lang(), "trackers.version"), status.version),
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
            Self::Mc(Mc::News(_)) => match crate::trackers::manifest(&cx.app).await {
                Ok(manifest) => {
                    let mut rows: Vec<_> = trackers::release_ids(&manifest)
                        .into_iter()
                        .take(3)
                        .enumerate()
                        .map(|(i, id)| row(format!("#{}", i + 1), id))
                        .collect();
                    if let Ok(news) = crate::trackers::launcher_news(&cx.app).await {
                        rows.extend(
                            crate::trackers::java_news(&news)
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
                crate::commands::trivia::start(cx, Some(pinchora_game::play::TriviaCategory::Minecraft)).await
            }
        }
    }
}
impl TrackersAdmin {
    pub async fn run(self, cx: &Cx) -> anyhow::Result<()> {
        let Some(guild) = cx.guild_id() else {
            return cx.reply_ephemeral(t!(cx.lang(), "error.guild-only")).await;
        };
        if !cx.can_manage_guild() {
            return cx.reply_ephemeral(t!(cx.lang(), "error.needs-manage-server")).await;
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
                let (host, port) = trackers::parse_host(&c.host)?;
                let host = format!("{host}:{port}");
                let watches = settings
                    .mc_watch
                    .as_array_mut()
                    .ok_or_else(|| anyhow::anyhow!("invalid watch list"))?;
                watches.retain(|v| v["host"] != host);
                anyhow::ensure!(watches.len() < 20, "watch limit");
                watches.push(json!({"host": host, "channel_id": c.channel.id.get().to_string()}));
            }
            Self::McUnwatch(c) => {
                let (host, port) = trackers::parse_host(&c.host)?;
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

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn riot_request_uses_header_and_mock_http() {
        use axum::{Router, http::HeaderMap, routing::get};
        let app = Router::new()
            .route(
                "/limit",
                get(|| async { ([("retry-after", "7")], axum::http::StatusCode::TOO_MANY_REQUESTS) }),
            )
            .route(
                "/riot/account/v1/accounts/by-riot-id/A/B",
                get(|headers: HeaderMap| async move {
                    assert_eq!(headers.get("x-riot-token").unwrap(), "test-key");
                    axum::Json(json!({"puuid":"mock-puuid","gameName":"A","tagLine":"B"}))
                }),
            );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        let url = url::Url::parse(&format!("http://{address}/riot/account/v1/accounts/by-riot-id/A/B")).unwrap();
        let value = riot_http_get(&reqwest::Client::new(), url, "test-key").await.unwrap();
        assert_eq!(value["puuid"], "mock-puuid");
        let limited = riot_http_get(
            &reqwest::Client::new(),
            url::Url::parse(&format!("http://{address}/limit")).unwrap(),
            "test-key",
        )
        .await
        .unwrap_err();
        assert_eq!(limited.downcast_ref::<RiotRateLimited>().unwrap().0, 7);
    }
    #[test]
    fn data_dragon_maps_champion_id() {
        let data = json!({"data":{"Ahri":{"key":"103","name":"Ahri","image":{"full":"Ahri.png"}}}});
        assert_eq!(champion(&data, 103), Some(("Ahri", "Ahri.png")));
    }
}
