//! `/suggest`, `/suggestion` and `/config suggestions`: members suggest, everyone votes with 👍 and 👎,
//! managers approve or deny and the author is told. Suggestion text is stripped of mentions and every
//! message pings nobody.

use std::time::Duration;

use pinchora_core::{i18n::Lang, t};
use pinchora_db::{Decision, SuggestionRow, SuggestionVote};
use twilight_interactions::command::{CommandModel, CreateCommand};
use twilight_model::{
    application::interaction::InteractionChannel,
    channel::message::{Component, Embed, component::ButtonStyle},
    guild::Permissions,
    http::interaction::InteractionResponseData,
    id::Id,
};
use twilight_util::builder::embed::{EmbedBuilder, EmbedFieldBuilder, EmbedFooterBuilder};

use crate::{
    App, Cx,
    commands::games::{edit_message, quiet},
    leveling::private_channel,
    server_events::{button, dm_lang, message_link, post_refusal, row, tell},
    util::{BRAND, DANGER, SUCCESS, strip_mentions},
};

/// A member can suggest again after this long.
const SUGGEST_EVERY: Duration = Duration::from_secs(30);

fn manage_guild() -> Permissions {
    Permissions::MANAGE_GUILD
}

#[derive(CommandModel, CreateCommand)]
#[command(name = "suggest", desc = "Suggest something for this server", contexts = "guild")]
pub struct Suggest {
    /// Your suggestion
    #[command(min_length = 3, max_length = 1000)]
    text: String,
}

#[derive(CommandModel, CreateCommand)]
#[command(
    name = "suggestion",
    desc = "Decide on the server's suggestions",
    default_permissions = "manage_guild",
    contexts = "guild"
)]
pub enum SuggestionCommand {
    #[command(name = "approve")]
    Approve(SuggestionApprove),
    #[command(name = "deny")]
    Deny(SuggestionDeny),
}

#[derive(CommandModel, CreateCommand)]
#[command(name = "approve", desc = "Approve a suggestion and tell its author")]
pub struct SuggestionApprove {
    /// The suggestion's number
    #[command(min_value = 1, max_value = 2_000_000_000)]
    id: i64,
    /// Why (the author sees it)
    #[command(max_length = 500)]
    reason: Option<String>,
}

#[derive(CommandModel, CreateCommand)]
#[command(name = "deny", desc = "Deny a suggestion and tell its author")]
pub struct SuggestionDeny {
    /// The suggestion's number
    #[command(min_value = 1, max_value = 2_000_000_000)]
    id: i64,
    /// Why (the author sees it)
    #[command(max_length = 500)]
    reason: Option<String>,
}

#[derive(CommandModel, CreateCommand)]
#[command(name = "suggestions", desc = "Where /suggest posts suggestions")]
pub struct ConfigSuggestions {
    /// Channel for suggestions (this turns them on)
    #[command(channel_types = "guild_text guild_announcement")]
    channel: Option<InteractionChannel>,
    /// Turn suggestions off
    off: Option<bool>,
}

/// How `/config show` and `/config suggestions` word the setting.
pub(crate) fn suggestions_summary(lang: Lang, channel: Option<u64>) -> String {
    match channel {
        Some(channel) => t!(lang, "suggest.summary-on", channel = format!("<#{channel}>")),
        None => t!(lang, "suggest.summary-off"),
    }
}

impl ConfigSuggestions {
    pub async fn run(self, cx: &Cx, guild: u64) -> anyhow::Result<()> {
        let db = &cx.app.db;
        let channel = match (self.channel, self.off == Some(true)) {
            (Some(_), true) => return tell(cx, t!(cx.lang(), "suggest.both")).await,
            (Some(channel), false) => {
                db.set_suggestion_channel(guild, Some(channel.id.get())).await?;
                Some(channel.id.get())
            }
            (None, true) => {
                db.set_suggestion_channel(guild, None).await?;
                None
            }
            (None, false) => db.suggestion_channel(guild).await?,
        };
        tell(
            cx,
            t!(
                cx.lang(),
                "suggest.current",
                summary = suggestions_summary(cx.lang(), channel)
            ),
        )
        .await
    }
}

/// The suggestion as its message shows it: the text, who made it, and once decided, who decided and why.
fn suggestion_embed(lang: Lang, s: &SuggestionRow) -> Embed {
    let (color, verdict) = match s.status.as_str() {
        pinchora_db::SUGGESTION_APPROVED => (SUCCESS, Some(t!(lang, "suggest.approved-by"))),
        pinchora_db::SUGGESTION_DENIED => (DANGER, Some(t!(lang, "suggest.denied-by"))),
        _ => (BRAND, None),
    };
    let mut embed = EmbedBuilder::new()
        .color(color)
        .title(t!(lang, "suggest.title", number = s.number))
        .description(&s.body)
        .field(EmbedFieldBuilder::new(t!(lang, "suggest.suggested-by"), format!("<@{}>", s.author_id)).inline());
    if let (Some(verdict), Some(by)) = (verdict, s.decided_by) {
        embed = embed.field(EmbedFieldBuilder::new(verdict, format!("<@{by}>")).inline());
    }
    if let Some(reason) = s.reason.as_deref() {
        embed = embed.field(EmbedFieldBuilder::new(t!(lang, "suggest.reason"), reason));
    }
    let footer = if s.is_open() {
        t!(lang, "suggest.footer-open")
    } else {
        t!(lang, "suggest.footer-closed")
    };
    embed.footer(EmbedFooterBuilder::new(footer)).build()
}

fn vote_id(id: i64, up: bool) -> String {
    format!("sg:v:{id}:{}", if up { 'u' } else { 'd' })
}

/// Reads what follows the `sg:` of a vote button: the suggestion and the side.
fn parse_button<'a>(mut parts: impl Iterator<Item = &'a str>) -> Option<(i64, bool)> {
    if parts.next()? != "v" {
        return None;
    }
    let id = parts.next()?.parse().ok()?;
    let up = match parts.next()? {
        "u" => true,
        "d" => false,
        _ => return None,
    };
    parts.next().is_none().then_some((id, up))
}

/// 👍 and 👎 with their live counts; locked once the suggestion is decided.
fn suggestion_components(s: &SuggestionRow) -> Vec<Component> {
    let locked = !s.is_open();
    vec![row(vec![
        button(
            vote_id(s.id, true),
            &s.up.to_string(),
            Some("👍"),
            ButtonStyle::Secondary,
            locked,
        ),
        button(
            vote_id(s.id, false),
            &s.down.to_string(),
            Some("👎"),
            ButtonStyle::Secondary,
            locked,
        ),
    ])]
}

impl Suggest {
    pub async fn run(self, cx: &Cx) -> anyhow::Result<()> {
        let Some(guild) = cx.guild_id() else {
            return tell(cx, t!(cx.lang(), "suggest.guild-only")).await;
        };
        let Some(channel) = cx.app.db.suggestion_channel(guild).await? else {
            return tell(cx, t!(cx.lang(), "suggest.not-set-up")).await;
        };
        let body = strip_mentions(&self.text);
        if body.is_empty() {
            return tell(cx, t!(cx.lang(), "suggest.needs-text")).await;
        }
        let user = cx.author()?.id.get();
        if let Some(left) = cx
            .app
            .cache
            .try_cooldown(&format!("suggest:{guild}:{user}"), SUGGEST_EVERY)
            .await?
        {
            return tell(cx, t!(cx.lang(), "suggest.slow-down", seconds = left.as_secs() + 1)).await;
        }
        let suggestion = cx.app.db.create_suggestion(guild, user, &body).await?;
        let channel = Id::new(channel.max(1));
        // The suggestion's message answers nobody in particular: it speaks the server's language.
        let server_lang = cx.app.guild_lang(guild).await;
        let posted = cx
            .app
            .http
            .create_message(channel)
            .embeds(&[suggestion_embed(server_lang, &suggestion)])
            .components(&suggestion_components(&suggestion))
            .allowed_mentions(quiet().as_ref())
            .await;
        let message = match posted {
            Ok(response) => response.model().await?,
            Err(error) => {
                tracing::warn!(guild, suggestion = suggestion.id, %error, "could not post a suggestion");
                cx.app.db.abort_suggestion(suggestion.id).await?;
                return tell(cx, post_refusal(cx.lang(), channel, &error)).await;
            }
        };
        cx.app
            .db
            .set_suggestion_message(suggestion.id, channel.get(), message.id.get())
            .await?;
        tell(
            cx,
            t!(
                cx.lang(),
                "suggest.posted",
                number = suggestion.number,
                channel = format!("<#{channel}>"),
                link = message_link(guild, channel.get(), message.id.get())
            ),
        )
        .await
    }
}

impl SuggestionCommand {
    pub async fn run(self, cx: &Cx) -> anyhow::Result<()> {
        let Some(guild) = cx.guild_id() else {
            return tell(cx, t!(cx.lang(), "suggest.guild-only")).await;
        };
        if !cx.can_manage_guild() {
            return tell(cx, t!(cx.lang(), "suggest.needs-manage")).await;
        }
        match self {
            Self::Approve(c) => decide(cx, guild, c.id, true, c.reason).await,
            Self::Deny(c) => decide(cx, guild, c.id, false, c.reason).await,
        }
    }
}

async fn decide(cx: &Cx, guild: u64, number: i64, approve: bool, reason: Option<String>) -> anyhow::Result<()> {
    let reason = reason.map(|r| strip_mentions(&r)).filter(|r| !r.is_empty());
    let by = cx.author()?.id.get();
    let decision = cx
        .app
        .db
        .decide_suggestion(guild, number as i32, approve, by, reason.as_deref())
        .await?;
    let suggestion = match decision {
        Decision::Done(suggestion) => suggestion,
        Decision::Missing => return tell(cx, t!(cx.lang(), "suggest.none", number = number)).await,
        Decision::Already(status) => {
            return tell(
                cx,
                t!(
                    cx.lang(),
                    "suggest.already",
                    number = number,
                    status = status_word(cx.lang(), &status)
                ),
            )
            .await;
        }
    };
    if let Some((channel, message)) = suggestion.channel_id.zip(suggestion.message_id) {
        let server_lang = cx.app.guild_lang(guild).await;
        edit_message(
            &cx.app,
            channel as u64,
            message as u64,
            None,
            Some(&[suggestion_embed(server_lang, &suggestion)]),
            Some(&suggestion_components(&suggestion)),
        )
        .await;
    }
    let told = if tell_author(&cx.app, guild, &suggestion).await {
        t!(cx.lang(), "suggest.told")
    } else {
        t!(cx.lang(), "suggest.not-told")
    };
    tell(
        cx,
        t!(
            cx.lang(),
            "suggest.decided",
            number = number,
            status = t!(cx.lang(), if approve { "suggest.approved" } else { "suggest.denied" }),
            author = format!("<@{}>", suggestion.author_id),
            told = told
        ),
    )
    .await
}

/// "approved" or "denied" for a suggestion's stored status.
fn status_word(lang: Lang, status: &str) -> String {
    t!(
        lang,
        if status == pinchora_db::SUGGESTION_APPROVED {
            "suggest.approved"
        } else {
            "suggest.denied"
        }
    )
}

/// Sends the author the decision. False when Discord will not deliver it (closed DMs).
async fn tell_author(app: &App, guild: u64, s: &SuggestionRow) -> bool {
    let Some(dm) = private_channel(app, s.author_id as u64).await else {
        return false;
    };
    let lang = dm_lang(app, s.author_id as u64, guild).await;
    let server = async { anyhow::Ok(app.http.guild(Id::new(guild.max(1))).await?.model().await?.name) }
        .await
        .unwrap_or_else(|_| t!(lang, "suggest.the-server"));
    let approved = s.status == pinchora_db::SUGGESTION_APPROVED;
    let quote: String = s.body.chars().take(300).collect();
    let mut embed = EmbedBuilder::new()
        .color(if approved { SUCCESS } else { DANGER })
        .title(t!(
            lang,
            "suggest.dm-title",
            number = s.number,
            status = status_word(lang, &s.status)
        ))
        .description(format!("> {}", quote.replace('\n', "\n> ")))
        .footer(EmbedFooterBuilder::new(server));
    if let Some(reason) = s.reason.as_deref() {
        embed = embed.field(EmbedFieldBuilder::new(t!(lang, "suggest.reason"), reason));
    }
    if let Some((channel, message)) = s.channel_id.zip(s.message_id) {
        embed = embed.field(EmbedFieldBuilder::new(
            t!(lang, "suggest.dm-suggestion"),
            message_link(guild, channel as u64, message as u64),
        ));
    }
    let sent = app
        .http
        .create_message(dm)
        .embeds(&[embed.build()])
        .allowed_mentions(quiet().as_ref())
        .await;
    if let Err(error) = &sent {
        tracing::debug!(user = s.author_id, %error, "suggestion decision not delivered");
    }
    sent.is_ok()
}

/// `sg:v:<id>:<u|d>`: a vote, a switch, or taking the vote back; the buttons show the new counts.
pub(crate) async fn on_vote<'a>(cx: &Cx, parts: impl Iterator<Item = &'a str>) -> anyhow::Result<()> {
    let Some((id, up)) = parse_button(parts) else {
        return tell(cx, t!(cx.lang(), "social.button-expired")).await;
    };
    let guild = cx.guild_id();
    let found = cx
        .app
        .db
        .suggestion(id)
        .await?
        .filter(|s| Some(s.guild_id as u64) == guild);
    if found.is_none() {
        return tell(cx, t!(cx.lang(), "suggest.gone")).await;
    }
    let user = cx.author()?.id.get();
    if cx.app.db.vote_suggestion(id, user, up).await? == SuggestionVote::Closed {
        return tell(cx, t!(cx.lang(), "suggest.closed")).await;
    }
    let Some(now) = cx.app.db.suggestion(id).await? else {
        return tell(cx, t!(cx.lang(), "suggest.gone")).await;
    };
    cx.update(InteractionResponseData {
        components: Some(suggestion_components(&now)),
        allowed_mentions: quiet(),
        ..Default::default()
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    fn suggestion(status: &str) -> SuggestionRow {
        SuggestionRow {
            id: 5,
            guild_id: 1,
            number: 3,
            author_id: 4,
            body: "More plants".to_owned(),
            channel_id: Some(2),
            message_id: Some(9),
            status: status.to_owned(),
            reason: (status != "open").then(|| "Good idea".to_owned()),
            decided_by: (status != "open").then_some(6),
            decided_at: None,
            up: 7,
            down: 2,
        }
    }

    #[test]
    fn vote_buttons_round_trip() {
        for up in [true, false] {
            let id = vote_id(12, up);
            let mut parts = id.split(':');
            assert_eq!(parts.next(), Some("sg"));
            assert_eq!(parse_button(parts), Some((12, up)));
        }
        for bad in ["", "v", "v:1", "v:x:u", "v:1:x", "v:1:u:2", "z:1:u"] {
            assert_eq!(parse_button(bad.split(':')), None, "{bad:?}");
        }
    }

    #[test]
    fn an_open_suggestion_shows_live_counts_on_open_buttons() {
        let Component::ActionRow(buttons) = &suggestion_components(&suggestion("open"))[0] else {
            panic!("no row");
        };
        let labels: Vec<_> = buttons
            .components
            .iter()
            .map(|c| match c {
                Component::Button(b) => (b.label.clone().unwrap_or_default(), b.disabled),
                _ => panic!("not a button"),
            })
            .collect();
        assert_eq!(labels, [("7".to_owned(), false), ("2".to_owned(), false)]);
    }

    #[test]
    fn a_decided_suggestion_names_who_decided_and_locks_the_buttons() {
        let embed = suggestion_embed(Lang::En, &suggestion("approved"));
        assert_eq!(embed.color, Some(SUCCESS));
        let fields: Vec<_> = embed
            .fields
            .iter()
            .map(|f| (f.name.as_str(), f.value.as_str()))
            .collect();
        assert_eq!(
            fields,
            [
                ("Suggested by", "<@4>"),
                ("Approved by", "<@6>"),
                ("Reason", "Good idea")
            ]
        );
        let Component::ActionRow(buttons) = &suggestion_components(&suggestion("denied"))[0] else {
            panic!("no row");
        };
        assert!(
            buttons
                .components
                .iter()
                .all(|c| matches!(c, Component::Button(b) if b.disabled))
        );
        assert_eq!(suggestion_embed(Lang::En, &suggestion("denied")).color, Some(DANGER));
    }
}
