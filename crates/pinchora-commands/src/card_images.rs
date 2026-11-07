//! Welcome and goodbye images: the design a server saved (or the classic card from its settings), drawn
//! for one member, and sent the way the server chose. The member events, `/config welcome-card` and the
//! dashboard's preview and test message all go through here, so they draw the same picture.

use pinchora_core::{
    card_design::{CardDesign, CardKind, Delivery, parse_design},
    i18n::Lang,
};
use pinchora_db::GuildSettings;
use pinchora_render::{CardFill, DesignedCard, Renderer, character::atlas, templates};
use twilight_model::{
    channel::message::AllowedMentions,
    http::attachment::Attachment,
    id::{
        Id,
        marker::{ChannelMarker, UserMarker},
    },
    util::ImageHash,
};

use crate::{App, PreviewError};

/// The avatar is fetched at least this big, and at most.
const AVATAR_MIN: u32 = 32;
const AVATAR_MAX: u32 = 512;
/// The pixel grid of a pixelated avatar, as on the classic card.
const AVATAR_GRID: u32 = 44;

/// The image a server sends for one kind of event, and how.
#[derive(Debug, Clone, PartialEq)]
pub struct ServerCard {
    pub design: CardDesign,
    pub delivery: Delivery,
    /// Whether the design is the server's own rather than the classic card.
    pub custom: bool,
}

/// Whether the atlas has a sprite, for the design validator.
pub fn atlas_has(id: &str) -> bool {
    atlas().get(id).is_some()
}

/// The classic card in the server's welcome theme, headline and accent; goodbyes share the theme and
/// accent with their own headline. Its words are in `lang`, the server's language.
pub fn classic_card(settings: &GuildSettings, kind: CardKind, lang: Lang) -> CardDesign {
    let theme = settings
        .welcome_theme
        .as_deref()
        .unwrap_or(pinchora_core::settings::DEFAULT_THEME);
    let title = match kind {
        CardKind::Welcome => settings.welcome_title.as_deref(),
        CardKind::Goodbye => None,
    };
    templates::classic(kind, theme, title, settings.welcome_accent.map(|c| c as u32), lang)
}

/// What the server sends for `kind`: its saved design, or the classic card when it has none (or what is
/// stored no longer reads, which is logged).
pub async fn server_card(
    app: &App,
    settings: &GuildSettings,
    guild: u64,
    kind: CardKind,
) -> anyhow::Result<ServerCard> {
    let row = app.db.card_design(guild, kind).await?;
    let delivery = row.as_ref().map_or(kind.default_delivery(), |r| r.delivery(kind));
    let saved = row.and_then(|r| r.design).and_then(|json| {
        let parsed = serde_json::from_str(&json)
            .map_err(|error| vec![pinchora_core::settings::FieldError::new("design", error.to_string())])
            .and_then(|value| parse_design(&value, &atlas_has));
        parsed
            .map_err(|errors| tracing::warn!(guild, kind = kind.key(), ?errors, "saved card design unreadable"))
            .ok()
    });
    Ok(match saved {
        Some(design) => ServerCard {
            design,
            delivery,
            custom: true,
        },
        None => ServerCard {
            design: classic_card(settings, kind, app.guild_lang(guild).await),
            delivery,
            custom: false,
        },
    })
}

/// Who the card is for, and the server it is about.
#[derive(Debug, Clone)]
pub struct CardSubject {
    pub user: Id<UserMarker>,
    pub avatar: Option<ImageHash>,
    /// The name the server shows.
    pub name: String,
    pub username: String,
    pub server: String,
    pub members: Option<u64>,
}

/// Everything a drawing of `design` for `subject` needs, fetched, as the job the renderer runs.
async fn card_job(
    app: &App,
    guild: u64,
    design: &CardDesign,
    subject: &CardSubject,
) -> anyhow::Result<impl FnOnce(&Renderer) -> anyhow::Result<Vec<u8>> + Send + 'static> {
    let size = design.avatar_size().unwrap_or(AVATAR_MIN).clamp(AVATAR_MIN, AVATAR_MAX);
    let grid = design.pixelated_avatar().then_some(AVATAR_GRID);
    let avatar = app.avatar_of(subject.user, subject.avatar, size, grid).await;
    let upload = match design.upload() {
        Some(id) => app.db.card_upload(guild, id).await?,
        None => None,
    };
    let lang = app.guild_lang(guild).await;
    let (design, subject) = (design.clone(), subject.clone());
    Ok(move |r: &Renderer| {
        let fill = CardFill {
            lang,
            user: &subject.name,
            username: &subject.username,
            server: &subject.server,
            members: subject.members,
        };
        r.designed_card(&DesignedCard {
            design: &design,
            avatar_png: &avatar,
            upload_png: upload.as_deref(),
            fill: &fill,
        })
    })
}

/// Draws `design` for `subject`. The uploaded background, if the design has one, is the guild's.
pub async fn card_png(app: &App, guild: u64, design: &CardDesign, subject: &CardSubject) -> anyhow::Result<Vec<u8>> {
    app.render(card_job(app, guild, design, subject).await?).await
}

/// [`card_png`] for the dashboard (previews and test messages), drawn in the preview lane: it waits its turn
/// among the other previews and gives up, with an error to show, rather than hold the editor for long.
pub async fn preview_png(
    app: &App,
    guild: u64,
    design: &CardDesign,
    subject: &CardSubject,
) -> Result<Vec<u8>, PreviewError> {
    app.render_preview(card_job(app, guild, design, subject).await?).await
}

/// Sends the text and the image to `channel` as `delivery` says: together, the image first, one of them
/// alone. A direct message is sent like [`Delivery::WithMessage`]; opening it is the caller's job.
pub async fn deliver(
    app: &App,
    channel: Id<ChannelMarker>,
    delivery: Delivery,
    text: &str,
    image: Option<(&str, Vec<u8>)>,
    mentions: &AllowedMentions,
) -> anyhow::Result<()> {
    let text = Some(text).filter(|t| !t.trim().is_empty() && delivery.has_text());
    let image = image.filter(|_| delivery.has_image());
    let attach = |(name, png): (&str, Vec<u8>)| [Attachment::from_bytes(name.to_owned(), png, 1)];
    match (delivery, text, image) {
        (_, None, None) => {}
        (Delivery::BeforeMessage, Some(text), Some(image)) => {
            app.http.create_message(channel).attachments(&attach(image)).await?;
            app.http
                .create_message(channel)
                .content(text)
                .allowed_mentions(Some(mentions))
                .await?;
        }
        (_, text, image) => {
            let files = image.map(attach);
            let mut message = app.http.create_message(channel).allowed_mentions(Some(mentions));
            if let Some(text) = text {
                message = message.content(text);
            }
            if let Some(files) = &files {
                message = message.attachments(files);
            }
            message.await?;
        }
    }
    Ok(())
}
