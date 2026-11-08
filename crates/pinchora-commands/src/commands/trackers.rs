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
        let response = crate::trackers::http_client()?
            .get("https://ddragon.leagueoflegends.com/api/versions.json")
            .send()
            .await?
            .error_for_status()?;
        let versions: Value = serde_json::from_slice(&crate::trackers::bounded_body(response, 1024 * 1024).await?)?;
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
    let response = crate::trackers::http_client()?
        .get(format!(
            "https://ddragon.leagueoflegends.com/cdn/{version}/data/en_US/champion.json"
        ))
        .send()
        .await?
        .error_for_status()?;
    let data: Value = serde_json::from_slice(&crate::trackers::bounded_body(response, 2 * 1024 * 1024).await?)?;
    cx.app.db.tracker_cache_put(&cache_key, &data).await?;
    Ok((version, data))
}
/// Bound PNG pixel and metadata allocations before decoding an untrusted icon.
fn tracker_icon(bytes: &[u8]) -> anyhow::Result<Vec<u8>> {
    let mut reader = image::ImageReader::with_format(std::io::Cursor::new(bytes), image::ImageFormat::Png);
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(256);
    limits.max_image_height = Some(256);
    limits.max_alloc = Some(1024 * 1024);
    reader.limits(limits);
    let icon = reader
        .decode()?
        .resize_exact(40, 40, image::imageops::FilterType::Triangle)
        .resize_exact(160, 160, image::imageops::FilterType::Nearest);
    let mut output = std::io::Cursor::new(Vec::new());
    icon.write_to(&mut output, image::ImageFormat::Png)?;
    Ok(output.into_inner())
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
    anyhow::ensure!(cx.app.cache.reserve_riot(region).await?, "rate_limited");
    let url = riot_url(region, path)?;
    let value = match riot_http_get(&crate::trackers::http_client()?, url, key).await {
        Ok(value) => value,
        Err(error) => {
            if let Some(limit) = error.downcast_ref::<RiotRateLimited>() {
                cx.app.cache.block_riot(region, limit.0).await?;
            }
            return Err(error);
        }
    };
    Ok(value)
}

fn riot_url(region: &str, path: &[&str]) -> anyhow::Result<url::Url> {
    let mut url = url::Url::parse(&format!("https://{region}.api.riotgames.com/"))?;
    url.path_segments_mut()
        .map_err(|_| anyhow::anyhow!("invalid URL"))?
        .extend(path);
    Ok(url)
}

fn league_path(puuid: &str) -> [&str; 6] {
    ["lol", "league", "v4", "entries", "by-puuid", puuid]
}

async fn riot_http_get(client: &reqwest::Client, url: url::Url, key: &str) -> anyhow::Result<Value> {
    let mut token =
        reqwest::header::HeaderValue::from_str(key).map_err(|_| anyhow::anyhow!("invalid Riot credentials"))?;
    token.set_sensitive(true);
    let response = client
        .get(url)
        .header("X-Riot-Token", token)
        .send()
        .await
        .map_err(|_| anyhow::anyhow!("Riot transport failed"))?;
    if response.status().as_u16() == 429 {
        let seconds = response
            .headers()
            .get("retry-after")
            .and_then(|v| v.to_str().ok())
            .map(retry_after)
            .unwrap_or(60);
        return Err(RiotRateLimited(seconds.max(1)).into());
    }
    anyhow::ensure!(
        response.status().is_success(),
        "riot_status_{}",
        response.status().as_u16()
    );
    let bytes = crate::trackers::bounded_body(response, 1024 * 1024)
        .await
        .map_err(|_| anyhow::anyhow!("invalid Riot response"))?;
    serde_json::from_slice(&bytes).map_err(|_| anyhow::anyhow!("invalid Riot JSON"))
}

fn retry_after(value: &str) -> u64 {
    value
        .trim()
        .parse::<u64>()
        .ok()
        .or_else(|| {
            chrono::DateTime::parse_from_rfc2822(value)
                .ok()
                .map(|until| (until.timestamp() - chrono::Utc::now().timestamp()).max(1) as u64)
        })
        .unwrap_or(60)
        .max(1)
}

fn riot_id(value: &str) -> Option<(&str, &str)> {
    if value.chars().any(char::is_control) {
        return None;
    }
    let (name, tag) = value.trim().split_once('#')?;
    let name = name.trim();
    let tag = tag.trim();
    (!name.is_empty()
        && !tag.is_empty()
        && name.chars().count() <= 64
        && tag.chars().count() <= 16
        && !tag.contains('#')
        && !name
            .chars()
            .chain(tag.chars())
            .any(|c| c.is_control() || matches!(c, '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}')))
    .then_some((name, tag))
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
            if let Ok(response) = crate::trackers::http_client()?.get(url).send().await
                && let Ok(response) = response.error_for_status()
                && let Ok(bytes) = crate::trackers::bounded_body(response, 1024 * 1024).await
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
            Self::Mc(Mc::Status(input)) => match crate::trackers::mc_status(&cx.app, &input.host).await {
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

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn tracker_icons_bound_pixels_and_compressed_metadata() {
        let mut png = std::io::Cursor::new(Vec::new());
        image::RgbaImage::new(64, 64)
            .write_to(&mut png, image::ImageFormat::Png)
            .unwrap();
        assert!(tracker_icon(png.get_ref()).is_ok());
        let mut oversized = std::io::Cursor::new(Vec::new());
        image::RgbaImage::new(512, 512)
            .write_to(&mut oversized, image::ImageFormat::Png)
            .unwrap();
        assert!(tracker_icon(oversized.get_ref()).is_err());
        // Text metadata is never expanded by the pixel decoder and is discarded on
        // re-encoding; the resulting card attachment cannot carry the compressed bomb.
        let safe = tracker_icon(include_bytes!("../../tests/fixtures/tracker-icon-metadata-bomb.png")).unwrap();
        assert!(safe.len() < 16_384);
        assert!(
            !safe
                .windows(4)
                .any(|chunk| matches!(chunk, b"zTXt" | b"iTXt" | b"iCCP"))
        );
    }

    #[test]
    fn tracker_destinations_must_be_text_channels_in_the_invoking_guild() {
        for (guild, kind, expected) in [
            (123, 0, true),
            (123, 5, true),
            (456, 0, false),
            (123, 1, false),
            (123, 11, false),
        ] {
            let channel =
                serde_json::from_value(json!({"id":"789", "guild_id":guild.to_string(), "type":kind})).unwrap();
            assert_eq!(posting_channel(123, &channel), expected);
        }
    }

    #[tokio::test]
    async fn riot_request_uses_header_and_mock_http() {
        use axum::{Router, http::HeaderMap, routing::get};
        let app = Router::new()
            .route(
                "/limit",
                get(|| async { ([("retry-after", "7")], axum::http::StatusCode::TOO_MANY_REQUESTS) }),
            )
            .route(
                "/lol/league/v4/entries/by-puuid/mock-puuid",
                get(|| async {
                    axum::Json(json!([{"queueType":"RANKED_SOLO_5x5","tier":"GOLD","rank":"II","leaguePoints":45}]))
                }),
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
        let value = riot_http_get(&crate::trackers::http_client().unwrap(), url, "test-key")
            .await
            .unwrap();
        assert_eq!(value["puuid"], "mock-puuid");
        let mut url = riot_url("euw1", &league_path("mock-puuid")).unwrap();
        url.set_scheme("http").unwrap();
        url.set_host(Some("127.0.0.1")).unwrap();
        url.set_port(Some(address.port())).unwrap();
        let rank = riot_http_get(&crate::trackers::http_client().unwrap(), url, "test-key")
            .await
            .unwrap();
        assert_eq!(rank[0]["tier"], "GOLD");
        let limited = riot_http_get(
            &crate::trackers::http_client().unwrap(),
            url::Url::parse(&format!("http://{address}/limit")).unwrap(),
            "test-key",
        )
        .await
        .unwrap_err();
        assert_eq!(limited.downcast_ref::<RiotRateLimited>().unwrap().0, 7);
    }
    #[test]
    fn riot_ids_retry_after_and_regions() {
        assert_eq!(riot_id("  Çağrı 猫 # TR1  "), Some(("Çağrı 猫", "TR1")));
        for id in ["a#b#c", "a#", "#b", "a\n#b", "a#b\u{202e}"] {
            assert!(riot_id(id).is_none(), "{id}");
        }
        assert_eq!(retry_after("600"), 600);
        assert_eq!(retry_after("0"), 1);
        assert_eq!(retry_after("invalid"), 60);
        let later = (chrono::Utc::now() + chrono::Duration::seconds(600)).to_rfc2822();
        assert!((599..=600).contains(&retry_after(&later)));
        let url = riot_url(
            "europe",
            &["riot", "account", "v1", "accounts", "by-riot-id", "Çağrı/猫", "TR#1"],
        )
        .unwrap();
        assert!(url.path().ends_with("%C3%87a%C4%9Fr%C4%B1%2F%E7%8C%AB/TR%231"));
        assert!(url.query().is_none() && url.fragment().is_none());
        assert_eq!(LolRegion::Tr1.routing(), "europe");
        assert_eq!(LolRegion::Br1.routing(), "americas");
        assert_eq!(LolRegion::Jp1.routing(), "asia");
    }

    #[tokio::test]
    async fn riot_errors_never_include_credentials_or_follow_redirects() {
        use axum::{Router, routing::get};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let app = Router::new()
            .route(
                "/redirect",
                get(|| async { (axum::http::StatusCode::FOUND, [("location", "/leak")]) }),
            )
            .route(
                "/leak",
                get(|| async {
                    panic!("token redirect followed");
                    #[allow(unreachable_code)]
                    ""
                }),
            )
            .route(
                "/error",
                get(|| async { (axum::http::StatusCode::BAD_REQUEST, "secret-key") }),
            );
        let server = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        for path in ["redirect", "error"] {
            let error = riot_http_get(
                &crate::trackers::http_client().unwrap(),
                url::Url::parse(&format!("http://{addr}/{path}")).unwrap(),
                "secret-key",
            )
            .await
            .unwrap_err();
            assert!(!format!("{error:?}").contains("secret-key"));
        }
        server.abort();
    }

    #[test]
    fn data_dragon_maps_champion_id() {
        let data = json!({"data":{"Ahri":{"key":"103","name":"Ahri","image":{"full":"Ahri.png"}}}});
        assert_eq!(champion(&data, 103), Some(("Ahri", "Ahri.png")));
    }
}
