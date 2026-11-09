use super::*;

impl ConfessCommand {
    pub async fn run(self, cx: &Cx) -> anyhow::Result<()> {
        let Some(guild) = cx.guild_id() else {
            return tell(cx, t!(cx.lang(), "community-v2.guild-only")).await;
        };
        match self {
            Self::Submit(c) => submit_confession(cx, guild, &c.text).await,
            Self::Review(c) => review_confession(cx, guild, c.id, c.approve).await,
            Self::Reveal(c) => reveal_confession(cx, guild, c.id).await,
            Self::Ban(c) => {
                if !cx.can_manage_guild() {
                    return tell(cx, t!(cx.lang(), "community-v2.manage-only")).await;
                }
                let changed = cx
                    .app
                    .db
                    .ban_confession_author(guild, c.id, cx.author()?.id.get(), c.banned)
                    .await?;
                tell(
                    cx,
                    t!(
                        cx.lang(),
                        if changed {
                            "community-v2.confession-ban-saved"
                        } else {
                            "community-v2.confession-missing"
                        }
                    ),
                )
                .await
            }
            Self::Report(c) => {
                let user = cx.author()?.id.get();
                if cx
                    .app
                    .cache
                    .try_cooldown(&format!("confess-report:{guild}:{user}"), Duration::from_secs(60))
                    .await?
                    .is_some()
                {
                    return tell(cx, t!(cx.lang(), "community-v2.confession-slow")).await;
                }
                if cx.app.db.report_confession(guild, c.id, user).await? {
                    let settings: Option<(Option<i64>,)> = sqlx::query_as(
                        "SELECT review_channel_id FROM confession_settings WHERE guild_id=$1 AND enabled",
                    )
                    .bind(guild as i64)
                    .fetch_optional(cx.app.db.pool())
                    .await?;
                    if let Some((Some(channel),)) = settings {
                        let lang = cx.app.guild_lang(guild).await;
                        let _ = cx
                            .app
                            .http
                            .create_message(Id::new(channel as u64))
                            .content(&t!(lang, "community-v2.confession-report-alert", id = c.id))
                            .allowed_mentions(quiet().as_ref())
                            .await;
                    }
                }
                tell(cx, t!(cx.lang(), "community-v2.confession-reported")).await
            }
            Self::Settings(c) => {
                if !cx.can_manage_guild() {
                    return tell(cx, t!(cx.lang(), "community-v2.manage-only")).await;
                }
                if c.review_channel.id == c.publish_channel.id {
                    return tell(cx, t!(cx.lang(), "community-v2.channels-differ")).await;
                }
                if !private_review_channel(&cx.app, guild, c.review_channel.id.get()).await? {
                    return tell(cx, t!(cx.lang(), "community-v2.review-private")).await;
                }
                sqlx::query("INSERT INTO confession_settings(guild_id,review_channel_id,publish_channel_id,enabled) VALUES($1,$2,$3,TRUE) ON CONFLICT(guild_id) DO UPDATE SET review_channel_id=$2,publish_channel_id=$3,enabled=TRUE")
                    .bind(guild as i64).bind(c.review_channel.id.get() as i64).bind(c.publish_channel.id.get() as i64)
                    .execute(cx.app.db.pool()).await?;
                tell(cx, t!(cx.lang(), "community-v2.confession-saved")).await
            }
        }
    }
}

async fn submit_confession(cx: &Cx, guild: u64, text: &str) -> anyhow::Result<()> {
    let body = strip_mentions(text).trim().to_owned();
    if body.chars().count() < 3 {
        return tell(cx, t!(cx.lang(), "community-v2.text-required")).await;
    }
    let settings: Option<(Option<i64>,)> =
        sqlx::query_as("SELECT review_channel_id FROM confession_settings WHERE guild_id=$1 AND enabled")
            .bind(guild as i64)
            .fetch_optional(cx.app.db.pool())
            .await?;
    let Some((Some(channel),)) = settings else {
        return tell(cx, t!(cx.lang(), "community-v2.confession-off")).await;
    };
    if !private_review_channel(&cx.app, guild, channel as u64)
        .await
        .unwrap_or(false)
    {
        return tell(cx, t!(cx.lang(), "community-v2.confession-unavailable")).await;
    }
    let user = cx.author()?.id.get();
    if cx
        .app
        .cache
        .try_cooldown(&format!("confess:{guild}:{user}"), Duration::from_secs(3600))
        .await?
        .is_some()
    {
        return tell(cx, t!(cx.lang(), "community-v2.confession-slow")).await;
    }
    let Some(id) = cx.app.db.submit_confession(guild, user, &body).await? else {
        return tell(cx, t!(cx.lang(), "community-v2.confession-slow")).await;
    };
    let lang = cx.app.guild_lang(guild).await;
    let embed = EmbedBuilder::new()
        .title(t!(lang, "community-v2.confession-review-title", id = id))
        .description(&body)
        .build();
    let buttons = vec![row(vec![
        button(
            format!("cv:a:{id}"),
            &t!(lang, "community-v2.approve"),
            None,
            ButtonStyle::Success,
            false,
        ),
        button(
            format!("cv:d:{id}"),
            &t!(lang, "community-v2.deny"),
            None,
            ButtonStyle::Danger,
            false,
        ),
    ])];
    match cx
        .app
        .http
        .create_message(Id::new(channel as u64))
        .embeds(&[cx.app.brand_embed(guild, embed).await])
        .components(&buttons)
        .allowed_mentions(quiet().as_ref())
        .await
    {
        Ok(posted) => {
            let message = posted.model().await?;
            sqlx::query("UPDATE confessions SET review_message_id=$2 WHERE id=$1")
                .bind(id)
                .bind(message.id.get() as i64)
                .execute(cx.app.db.pool())
                .await?;
            tell(cx, t!(cx.lang(), "community-v2.confession-queued", id = id)).await
        }
        Err(error) => {
            sqlx::query("DELETE FROM confessions WHERE id=$1")
                .bind(id)
                .execute(cx.app.db.pool())
                .await?;
            tracing::warn!(guild,id,%error,"confession queue post failed");
            tell(cx, t!(cx.lang(), "community-v2.confession-unavailable")).await
        }
    }
}

async fn review_confession(cx: &Cx, guild: u64, id: i64, approve: bool) -> anyhow::Result<()> {
    if !cx.can_manage_guild() {
        return tell(cx, t!(cx.lang(), "community-v2.manage-only")).await;
    }
    let row: Option<(String,Option<i64>,i64)> = sqlx::query_as(
        "UPDATE confessions SET status=$3,reviewer_id=$4,reviewed_at=now() WHERE id=$1 AND guild_id=$2 AND status='pending' RETURNING body,review_message_id,author_id")
        .bind(id).bind(guild as i64).bind(if approve { "approved" } else { "denied" })
        .bind(cx.author()?.id.get() as i64).fetch_optional(cx.app.db.pool()).await?;
    let Some((body, review_message, author)) = row else {
        return tell(cx, t!(cx.lang(), "community-v2.confession-missing")).await;
    };
    let settings: Option<(Option<i64>, Option<i64>)> =
        sqlx::query_as("SELECT review_channel_id,publish_channel_id FROM confession_settings WHERE guild_id=$1")
            .bind(guild as i64)
            .fetch_optional(cx.app.db.pool())
            .await?;
    if approve && settings.as_ref().and_then(|s| s.1).is_none() {
        sqlx::query("UPDATE confessions SET status='pending',reviewer_id=NULL,reviewed_at=NULL WHERE id=$1")
            .bind(id)
            .execute(cx.app.db.pool())
            .await?;
        return tell(cx, t!(cx.lang(), "community-v2.confession-unavailable")).await;
    }
    if approve && let Some((_, Some(channel))) = settings.as_ref() {
        let lang = cx.app.guild_lang(guild).await;
        let embed = EmbedBuilder::new()
            .title(t!(lang, "community-v2.confession-public-title", id = id))
            .description(&body)
            .build();
        match cx
            .app
            .http
            .create_message(Id::new(*channel as u64))
            .embeds(&[cx.app.brand_embed(guild, embed).await])
            .allowed_mentions(quiet().as_ref())
            .await
        {
            Ok(posted) => {
                let message = posted.model().await?;
                sqlx::query("UPDATE confessions SET publish_message_id=$2 WHERE id=$1")
                    .bind(id)
                    .bind(message.id.get() as i64)
                    .execute(cx.app.db.pool())
                    .await?;
            }
            Err(error) => {
                sqlx::query("UPDATE confessions SET status='pending',reviewer_id=NULL,reviewed_at=NULL WHERE id=$1")
                    .bind(id)
                    .execute(cx.app.db.pool())
                    .await?;
                tracing::warn!(guild,id,%error,"approved confession not published");
                return tell(cx, t!(cx.lang(), "community-v2.confession-unavailable")).await;
            }
        }
    }
    if let Some((Some(review_channel), _)) = settings.as_ref()
        && let Some(message) = review_message
    {
        let empty: Vec<Component> = Vec::new();
        let _ = cx
            .app
            .http
            .update_message(Id::new(*review_channel as u64), Id::new(message as u64))
            .components(Some(&empty))
            .await;
    }
    if let Some(dm) = private_channel(&cx.app, author as u64).await {
        let lang = cx.app.user_lang_choice(author as u64).await.unwrap_or_default();
        let _ = cx
            .app
            .http
            .create_message(dm)
            .content(&t!(
                lang,
                if approve {
                    "community-v2.confession-approved"
                } else {
                    "community-v2.confession-denied"
                },
                id = id
            ))
            .allowed_mentions(quiet().as_ref())
            .await;
    }
    tell(cx, t!(cx.lang(), "community-v2.confession-reviewed")).await
}

async fn reveal_confession(cx: &Cx, guild: u64, id: i64) -> anyhow::Result<()> {
    let owner = cx.app.http.guild(Id::new(guild)).await?.model().await?.owner_id.get();
    if owner != cx.author()?.id.get() {
        return tell(cx, t!(cx.lang(), "community-v2.owner-only")).await;
    }
    let row: Option<(i64,)> = sqlx::query_as("SELECT author_id FROM confessions WHERE id=$1 AND guild_id=$2")
        .bind(id)
        .bind(guild as i64)
        .fetch_optional(cx.app.db.pool())
        .await?;
    let Some((author,)) = row else {
        return tell(cx, t!(cx.lang(), "community-v2.confession-missing")).await;
    };
    sqlx::query("INSERT INTO confession_reveals(guild_id,confession_id,owner_id) VALUES($1,$2,$3)")
        .bind(guild as i64)
        .bind(id)
        .bind(owner as i64)
        .execute(cx.app.db.pool())
        .await?;
    tell(
        cx,
        t!(
            cx.lang(),
            "community-v2.confession-revealed",
            author = format!("<@{author}>")
        ),
    )
    .await
}

pub(crate) async fn on_confession_button<'a>(cx: &Cx, mut parts: impl Iterator<Item = &'a str>) -> anyhow::Result<()> {
    let approve = match parts.next() {
        Some("a") => true,
        Some("d") => false,
        _ => return tell(cx, t!(cx.lang(), "community-v2.confession-missing")).await,
    };
    let Some(id) = parts.next().and_then(|s| s.parse::<i64>().ok()) else {
        return tell(cx, t!(cx.lang(), "community-v2.confession-missing")).await;
    };
    if parts.next().is_some() {
        return tell(cx, t!(cx.lang(), "community-v2.confession-missing")).await;
    }
    let Some(guild) = cx.guild_id() else {
        return tell(cx, t!(cx.lang(), "community-v2.guild-only")).await;
    };
    review_confession(cx, guild, id, approve).await
}
