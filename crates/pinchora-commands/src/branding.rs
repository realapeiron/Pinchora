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

pub fn validate(row: &GuildBranding) -> anyhow::Result<()> {
    if row
        .nickname
        .as_ref()
        .is_some_and(|v| v.is_empty() || v.encode_utf16().count() > 32)
    {
        anyhow::bail!("nickname");
    }
    if row
        .bio
        .as_ref()
        .is_some_and(|v| v.is_empty() || v.chars().count() > 190)
    {
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

fn unsupported(error: &twilight_http::Error) -> bool {
    matches!(error.kind(), ErrorType::Response { status, .. } if matches!(status.get(), 400 | 403))
}

#[derive(Debug)]
pub enum ApplyResult {
    Applied,
    Partial(Vec<&'static str>),
    Cooldown,
}

/// Sends the profile together, then isolates fields if Discord refuses the request.
pub async fn apply(app: &App, guild: u64, row: &GuildBranding) -> anyhow::Result<ApplyResult> {
    validate(row)?;
    let avatar = if let Some(id) = &row.avatar_upload_id {
        Some(data_uri(
            &app.db
                .card_upload(guild, id)
                .await?
                .ok_or_else(|| anyhow::anyhow!("Avatar upload is missing."))?,
        ))
    } else {
        None
    };
    let banner = if let Some(id) = &row.banner_upload_id {
        Some(data_uri(
            &app.db
                .card_upload(guild, id)
                .await?
                .ok_or_else(|| anyhow::anyhow!("Banner upload is missing."))?,
        ))
    } else {
        None
    };
    if !app.db.claim_branding_apply(guild).await? {
        return Ok(ApplyResult::Cooldown);
    }
    app.announce_guild_change(guild).await;
    let id = Id::new(guild);
    let has_profile = row.nickname.is_some() || avatar.is_some() || banner.is_some() || row.bio.is_some();
    if has_profile {
        let mut request = app.http.update_current_member(id);
        if let Some(nick) = row.nickname.as_deref() {
            request = request.nick(Some(nick));
        }
        if let Some(avatar) = avatar.as_deref() {
            request = request.avatar(Some(avatar));
        }
        if let Some(banner) = banner.as_deref() {
            request = request.banner(Some(banner));
        }
        if let Some(bio) = row.bio.as_deref() {
            request = request.bio(Some(bio));
        }
        match request.await {
            Ok(_) => {
                app.db
                    .branding_result(
                        guild,
                        None,
                        [
                            avatar.as_ref().map(|_| true),
                            banner.as_ref().map(|_| true),
                            row.bio.as_ref().map(|_| true),
                        ],
                    )
                    .await?;
                return Ok(ApplyResult::Applied);
            }
            Err(error) if unsupported(&error) => {}
            Err(error) => return Err(error.into()),
        }
    }
    let mut failed = Vec::new();
    let mut features = [None; 3];
    if let Some(nick) = row.nickname.as_deref()
        && let Err(error) = app.http.update_current_member(id).nick(Some(nick)).await
    {
        if !unsupported(&error) {
            return Err(error.into());
        }
        failed.push("nickname");
    }
    if let Some(avatar) = avatar.as_deref() {
        match app.http.update_current_member(id).avatar(Some(avatar)).await {
            Ok(_) => features[0] = Some(true),
            Err(error) if unsupported(&error) => {
                features[0] = Some(false);
                failed.push("avatar");
            }
            Err(error) => return Err(error.into()),
        }
    }
    if let Some(banner) = banner.as_deref() {
        match app.http.update_current_member(id).banner(Some(banner)).await {
            Ok(_) => features[1] = Some(true),
            Err(error) if unsupported(&error) => {
                features[1] = Some(false);
                failed.push("banner");
            }
            Err(error) => return Err(error.into()),
        }
    }
    if let Some(bio) = row.bio.as_deref() {
        match app.http.update_current_member(id).bio(Some(bio)).await {
            Ok(_) => features[2] = Some(true),
            Err(error) if unsupported(&error) => {
                features[2] = Some(false);
                failed.push("bio");
            }
            Err(error) => return Err(error.into()),
        }
    }
    let message = (!failed.is_empty()).then(|| failed.join(","));
    app.db.branding_result(guild, message.as_deref(), features).await?;
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
    fn png_data_uri() {
        assert_eq!(data_uri(b"PNG"), "data:image/png;base64,UE5H");
    }
}
