use super::*;

pub(super) fn champion(data: &Value, id: i64) -> Option<(&str, &str)> {
    data["data"]
        .as_object()?
        .values()
        .find(|v| v["key"].as_str().is_some_and(|key| key.parse::<i64>().ok() == Some(id)))
        .and_then(|v| Some((v["name"].as_str()?, v["image"]["full"].as_str()?)))
}

pub(super) async fn dragon(cx: &Cx) -> anyhow::Result<(String, Value)> {
    let version = if let Some(value) = cx.app.db.tracker_cached("dragon:version", 86400).await? {
        value.as_str().unwrap_or_default().to_owned()
    } else {
        let response = crate::community::trackers::http_client()?
            .get("https://ddragon.leagueoflegends.com/api/versions.json")
            .send()
            .await?
            .error_for_status()?;
        let versions: Value =
            serde_json::from_slice(&crate::community::trackers::bounded_body(response, 1024 * 1024).await?)?;
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
    let response = crate::community::trackers::http_client()?
        .get(format!(
            "https://ddragon.leagueoflegends.com/cdn/{version}/data/en_US/champion.json"
        ))
        .send()
        .await?
        .error_for_status()?;
    let data: Value =
        serde_json::from_slice(&crate::community::trackers::bounded_body(response, 2 * 1024 * 1024).await?)?;
    cx.app.db.tracker_cache_put(&cache_key, &data).await?;
    Ok((version, data))
}
/// Bound PNG pixel and metadata allocations before decoding an untrusted icon.
pub(super) fn tracker_icon(bytes: &[u8]) -> anyhow::Result<Vec<u8>> {
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

pub(super) fn row(label: String, value: impl Into<String>) -> InfoRow {
    InfoRow {
        label,
        value: value.into(),
    }
}

pub(super) fn riot_key() -> Option<String> {
    std::env::var("PINCHORA_TRACKERS__RIOT_API_KEY")
        .ok()
        .filter(|s| !s.is_empty())
}
pub(super) fn riot_prod() -> bool {
    std::env::var("PINCHORA_TRACKERS__RIOT_PRODUCTION").is_ok_and(|s| s == "true" || s == "1")
}

#[derive(Debug)]
pub(super) struct RiotRateLimited(pub(super) u64);
impl std::fmt::Display for RiotRateLimited {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Riot rate limited for {} seconds", self.0)
    }
}
impl std::error::Error for RiotRateLimited {}

pub(super) async fn riot(cx: &Cx, key: &str, region: &str, path: &[&str]) -> anyhow::Result<Value> {
    anyhow::ensure!(cx.app.cache.reserve_riot(region).await?, "rate_limited");
    let url = riot_url(region, path)?;
    let value = match riot_http_get(&crate::community::trackers::http_client()?, url, key).await {
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

pub(super) fn riot_url(region: &str, path: &[&str]) -> anyhow::Result<url::Url> {
    let mut url = url::Url::parse(&format!("https://{region}.api.riotgames.com/"))?;
    url.path_segments_mut()
        .map_err(|_| anyhow::anyhow!("invalid URL"))?
        .extend(path);
    Ok(url)
}

pub(super) fn league_path(puuid: &str) -> [&str; 6] {
    ["lol", "league", "v4", "entries", "by-puuid", puuid]
}

pub(super) async fn riot_http_get(client: &reqwest::Client, url: url::Url, key: &str) -> anyhow::Result<Value> {
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
    let bytes = crate::community::trackers::bounded_body(response, 1024 * 1024)
        .await
        .map_err(|_| anyhow::anyhow!("invalid Riot response"))?;
    serde_json::from_slice(&bytes).map_err(|_| anyhow::anyhow!("invalid Riot JSON"))
}

pub(super) fn retry_after(value: &str) -> u64 {
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

pub(super) fn riot_id(value: &str) -> Option<(&str, &str)> {
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

