//! The server profile and one colour resolver for Pinchora embeds.
use std::time::Duration;

use base64::{Engine, engine::general_purpose::STANDARD};
use pinchora_db::GuildBranding;
use twilight_http::error::ErrorType;
use twilight_model::{channel::message::Embed, id::Id};

use crate::{App, util::BRAND};

pub const APPLY_COOLDOWN: Duration = Duration::from_secs(600);

pub fn parse_color(value: &str) -> Option<i32> {
    let hex = value.strip_prefix('#').unwrap_or(value);
    (hex.len() == 6 && hex.bytes().all(|c| c.is_ascii_hexdigit()))
        .then(|| i32::from_str_radix(hex, 16).ok())
        .flatten()
}

// Profile text is plain text: prevent mention spoofing, invite advertising and Markdown links.
fn safe_text(value: &str, max: usize) -> bool {
    let folded: String = value
        .chars()
        .filter(|c| !c.is_whitespace())
        .flat_map(char::to_lowercase)
        .collect();
    !value.trim().is_empty() && value.encode_utf16().count() <= max
        && !value.chars().any(|c| c.is_control() || matches!(c, '@' | '<' | '>' | '*' | '_' | '`' | '~' | '|' | '[' | ']' | '\\' | '\u{200b}'..='\u{200f}' | '\u{202a}'..='\u{202e}' | '\u{2060}'..='\u{206f}'))
        && !["discord.gg/", "discord.com/invite", "discordapp.com/invite"].iter().any(|link| folded.contains(link))
}

pub fn validate(row: &GuildBranding) -> anyhow::Result<()> {
    if row.nickname.as_ref().is_some_and(|v| !safe_text(v, 32)) {
        anyhow::bail!("nickname");
    }
    if row.bio.as_ref().is_some_and(|v| !safe_text(v, 190)) {
        anyhow::bail!("bio");
    }
    if row.embed_color.is_some_and(|v| !(0..=0xffffff).contains(&v)) {
        anyhow::bail!("colour");
    }
    for id in [&row.avatar_upload_id, &row.banner_upload_id].into_iter().flatten() {
        if !pinchora_core::card_design::is_upload_id(id) {
            anyhow::bail!("upload");
        }
    }
    Ok(())
}

fn data_uri(png: &[u8]) -> String {
    format!("data:image/png;base64,{}", STANDARD.encode(png))
}

// Re-encode at the outbound boundary too: backups may contain valid animated PNGs or metadata.
fn profile_png(bytes: &[u8]) -> anyhow::Result<Vec<u8>> {
    use pinchora_core::card_design::{UPLOAD_MAX_SIDE, UPLOAD_STORED_MAX_BYTES};
    anyhow::ensure!(bytes.len() <= UPLOAD_STORED_MAX_BYTES, "upload");
    let mut reader = image::ImageReader::with_format(std::io::Cursor::new(bytes), image::ImageFormat::Png);
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(UPLOAD_MAX_SIDE);
    limits.max_image_height = Some(UPLOAD_MAX_SIDE);
    limits.max_alloc = Some(64 * 1024 * 1024);
    reader.limits(limits);
    let decoded = reader.decode()?;
    let png = pinchora_render::encode_png(&decoded.to_rgba8())?;
    anyhow::ensure!(png.len() <= UPLOAD_STORED_MAX_BYTES, "upload");
    Ok(png)
}

fn unsupported(error: &twilight_http::Error) -> bool {
    matches!(error.kind(), ErrorType::Response { status, .. } if matches!(status.get(), 400 | 403))
}

#[derive(Debug)]
pub enum ApplyResult {
    Applied,
    Partial(Vec<&'static str>),
    Cooldown,
}

/// Apply a complete desired profile. Explicit nulls clear fields removed in the dashboard.
pub async fn apply(app: &App, guild: u64, row: &GuildBranding) -> anyhow::Result<ApplyResult> {
    apply_profile(app, guild, row, None).await
}

pub async fn reset(app: &App, guild: u64) -> anyhow::Result<ApplyResult> {
    let before = app.db.guild_branding(guild).await?;
    apply_profile(app, guild, &GuildBranding::default(), Some(&before)).await
}

async fn apply_profile(
    app: &App,
    guild: u64,
    row: &GuildBranding,
    reset: Option<&GuildBranding>,
) -> anyhow::Result<ApplyResult> {
    validate(row)?;
    let mut images = Vec::new();
    for upload in [&row.avatar_upload_id, &row.banner_upload_id] {
        images.push(if let Some(upload) = upload {
            let png = app
                .db
                .card_upload(guild, upload)
                .await?
                .ok_or_else(|| anyhow::anyhow!("upload"))?;
            Some(data_uri(
                &tokio::task::spawn_blocking(move || profile_png(&png)).await??,
            ))
        } else {
            None
        });
    }
    if !app.db.claim_branding_apply(guild, row.embed_color).await? {
        return Ok(ApplyResult::Cooldown);
    }
    app.announce_guild_change(guild).await;
    let id = Id::new(guild);
    let values = [
        row.nickname.as_deref(),
        images[0].as_deref(),
        images[1].as_deref(),
        row.bio.as_deref(),
    ];
    let result = app
        .http
        .update_current_member(id)
        .nick(values[0])
        .avatar(values[1])
        .banner(values[2])
        .bio(values[3])
        .await;
    let mut accepted = [false; 4];
    let mut features = [None; 3];
    match result {
        Ok(_) => {
            accepted = [true; 4];
            features = [Some(true); 3];
        }
        Err(error) if unsupported(&error) => {
            // Isolate capability refusals; every result is persisted, including transient failures.
            for (index, value) in values.into_iter().enumerate() {
                let request = app.http.update_current_member(id);
                let result = match index {
                    0 => request.nick(value).await,
                    1 => request.avatar(value).await,
                    2 => request.banner(value).await,
                    _ => request.bio(value).await,
                };
                accepted[index] = result.is_ok();
                if index > 0 {
                    features[index - 1] = match &result {
                        Ok(_) => Some(true),
                        Err(error) if unsupported(error) => Some(false),
                        Err(_) => None,
                    };
                }
            }
        }
        Err(error) => tracing::warn!(%error, guild, "branding profile update failed"),
    }
    app.db.branding_result(guild, row, accepted, features, reset).await?;
    let failed: Vec<_> = ["nickname", "avatar", "banner", "bio"]
        .into_iter()
        .zip(accepted)
        .filter_map(|(field, ok)| (!ok).then_some(field))
        .collect();
    Ok(if failed.is_empty() {
        ApplyResult::Applied
    } else {
        ApplyResult::Partial(failed)
    })
}

impl App {
    async fn configured_brand_color(&self, guild: u64) -> anyhow::Result<Option<i32>> {
        self.branding_colors
            .try_get_with(guild, async {
                Ok::<_, anyhow::Error>(self.db.guild_branding(guild).await?.active_embed_color)
            })
            .await
            .map_err(|error| anyhow::anyhow!("{error:#}"))
    }

    /// A server's current embed colour; the default stays coral until configured.
    pub async fn brand_color(&self, guild: u64) -> anyhow::Result<u32> {
        Ok(self.configured_brand_color(guild).await?.map_or(BRAND, |v| v as u32))
    }

    pub async fn brand_embeds(&self, guild: u64, embeds: &mut [Embed]) {
        match self.configured_brand_color(guild).await {
            Ok(Some(color)) => {
                for embed in embeds {
                    embed.color = Some(color as u32);
                }
            }
            Ok(None) => {}
            Err(error) => tracing::warn!(%error, guild, "could not read server branding"),
        }
    }

    pub async fn brand_embed(&self, guild: u64, mut embed: Embed) -> Embed {
        self.brand_embeds(guild, std::slice::from_mut(&mut embed)).await;
        embed
    }

    /// For shared message editors that know only a channel. The lookup is cached for six hours.
    pub async fn brand_embeds_in_channel(&self, channel: u64, embeds: &mut [Embed]) {
        let guild = if let Some(cached) = self.channel_guilds.get(&channel).await {
            cached
        } else {
            let found = match self.http.channel(Id::new(channel)).await {
                Ok(reply) => reply.model().await.ok().and_then(|c| c.guild_id.map(|id| id.get())),
                Err(_) => None,
            };
            if found.is_some() {
                self.channel_guilds.insert(channel, found).await;
            }
            found
        };
        if let Some(guild) = guild {
            self.brand_embeds(guild, embeds).await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn colors_and_limits() {
        assert_eq!(parse_color("#ff6b4a"), Some(0xff6b4a));
        assert_eq!(parse_color("red"), None);
        let row = GuildBranding {
            nickname: Some("a".repeat(33)),
            ..Default::default()
        };
        assert!(validate(&row).is_err());
    }
    #[test]
    fn plain_text_policy_rejects_mentions_invites_markdown_and_unicode_overflow() {
        for value in [
            "@everyone",
            "<@123>",
            "<@&123>",
            "[label](url)",
            "**bold**",
            "`code`",
            "discord.gg/abc",
            "DISCORD.COM/invite/abc",
            "dis\u{200b}cord.gg/abc",
            "\u{202e}spoof",
            " ",
            "line\nfeed",
        ] {
            assert!(!safe_text(value, 190), "{value}");
        }
        assert!(safe_text("Mercan Yardımcısı", 32));
        assert!(safe_text(&"😀".repeat(16), 32));
        assert!(!safe_text(&"😀".repeat(17), 32));
        assert!(!safe_text(&"😀".repeat(96), 190));
    }

    #[test]
    fn outbound_images_are_bounded_decoded_and_reencoded() {
        use pinchora_core::card_design::UPLOAD_STORED_MAX_BYTES;
        assert!(profile_png(b"not a PNG").is_err());
        assert!(profile_png(&vec![0; UPLOAD_STORED_MAX_BYTES + 1]).is_err());
        let input = image::RgbaImage::new(4097, 1);
        let png = pinchora_render::encode_png(&input).unwrap();
        assert!(profile_png(&png).is_err());
        let input = image::RgbaImage::new(2, 2);
        let mut png = pinchora_render::encode_png(&input).unwrap();
        png.extend_from_slice(b"untrusted trailing metadata");
        let output = profile_png(&png).unwrap();
        assert_ne!(output, png);
        assert_eq!(image::load_from_memory(&output).unwrap().to_rgba8(), input);
    }

    #[test]
    fn png_data_uri() {
        assert_eq!(data_uri(b"PNG"), "data:image/png;base64,UE5H");
    }
}
