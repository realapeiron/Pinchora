mod response;

#[cfg(test)]
mod tests;

use std::{
    sync::{Arc, OnceLock},
    time::Duration,
};

use pinchora_core::infra::i18n::Lang;
use pinchora_core::t;
use tokio::sync::{Mutex, oneshot};
use twilight_model::{
    application::{
        command::CommandOptionChoice,
        interaction::{Interaction, InteractionType},
    },
    channel::message::{AllowedMentions, MessageFlags},
    http::interaction::{InteractionResponse, InteractionResponseData, InteractionResponseType},
    id::{
        Id,
        marker::{ChannelMarker, InteractionMarker, MessageMarker},
    },
};

use crate::{App, Deferral};

/// How long a private answer to a text command stays in the channel before it is deleted.
pub const TEXT_PRIVATE_LINGER: Duration = Duration::from_secs(15);

/// Where a text command answers: as replies to the member's message in its channel.
struct TextTarget {
    channel: Id<ChannelMarker>,
    message: Id<MessageMarker>,
    /// `house edit`, for the notice that sends a member to the slash command.
    path: String,
    /// The language the member is answered in.
    lang: Lang,
    /// The first answer posted, which [`Responder::edit`] edits.
    first: OnceLock<Id<MessageMarker>>,
}

/// Answers one interaction, whichever way it arrived.
///
/// Over the gateway every answer is a REST call. Over the HTTP endpoint the first
/// answer goes back as the HTTP response body; if the handler is slower than the
/// endpoint's budget, the endpoint sends a "thinking…" defer and later answers
/// become edits of that message. Images always take the defer-then-edit path,
/// because files cannot ride in a JSON response body. Over the gateway, [`Responder::defer_if_pending`]
/// does the same for a handler that outlasts Discord's window.
///
/// A text command ([`Responder::text`]) has no interaction to answer: every answer is a reply to the
/// member's message that pings nobody. A private answer is deleted after [`TEXT_PRIVATE_LINGER`]
/// (or when the bot shuts down, whichever is first), and a private panel or a form, which a channel reply cannot stand in for, becomes a notice to use
/// the slash command.
pub struct Responder {
    app: Arc<App>,
    guild: Option<u64>,
    id: Id<InteractionMarker>,
    token: String,
    text: Option<TextTarget>,
    component: bool,
    deferral: Deferral,
    state: Mutex<State>,
    command_policy: OnceLock<pinchora_core::admin::command_control::Rule>,
    deletion_scheduled: OnceLock<()>,
    deferred_private: OnceLock<bool>,
    migration_notice: OnceLock<String>,
}

enum State {
    /// Not answered yet; holds the HTTP response channel in HTTP mode.
    Pending(Option<oneshot::Sender<InteractionResponse>>),
    /// A defer was sent; the next answer edits the original message.
    Deferred,
    /// Answered; further answers become follow-up messages.
    Done,
}

impl Responder {
    pub fn rest(app: Arc<App>, interaction: &Interaction) -> Self {
        Self::new(app, interaction, None)
    }

    pub fn http(app: Arc<App>, interaction: &Interaction, tx: oneshot::Sender<InteractionResponse>) -> Self {
        Self::new(app, interaction, Some(tx))
    }

    /// Answers a text command: `message` in `channel` is the member's, `path` the command it ran.
    pub fn text(
        app: Arc<App>,
        guild: u64,
        channel: Id<ChannelMarker>,
        message: Id<MessageMarker>,
        path: String,
        lang: Lang,
    ) -> Self {
        Self {
            app,
            guild: Some(guild),
            id: message.cast(),
            token: format!("text-{message}"),
            text: Some(TextTarget {
                channel,
                message,
                path,
                lang,
                first: OnceLock::new(),
            }),
            component: false,
            deferral: Deferral::Never,
            state: Mutex::new(State::Pending(None)),
            command_policy: OnceLock::new(),
            deletion_scheduled: OnceLock::new(),
            deferred_private: OnceLock::new(),
            migration_notice: OnceLock::new(),
        }
    }

    pub(crate) fn set_migration_notice(&self, notice: String) {
        let _ = self.migration_notice.set(notice);
    }

    pub(crate) fn set_command_policy(&self, policy: pinchora_core::admin::command_control::Rule) {
        if let Some(text) = &self.text
            && policy.delete_invocation_secs > 0
        {
            self.app.delete_later(
                text.channel,
                text.message,
                Duration::from_secs(policy.delete_invocation_secs.into()),
            );
        }
        let _ = self.command_policy.set(policy);
    }

    fn private_default(&self) -> bool {
        !self.component && self.command_policy.get().is_some_and(|p| p.ephemeral)
    }

    fn schedule_original_deletion(&self) {
        let seconds = self.command_policy.get().map_or(0, |p| p.delete_reply_secs);
        if seconds == 0 || self.deletion_scheduled.set(()).is_err() {
            return;
        }
        let app = self.app.clone();
        let token = self.token.clone();
        self.app.spawn(async move {
            tokio::time::sleep(Duration::from_secs(seconds.into())).await;
            if let Err(error) = app.interaction().delete_response(&token).await {
                tracing::debug!(%error, "command reply deletion failed");
            }
        });
    }

    /// Whether this answers a text command rather than an interaction.
    pub fn is_text(&self) -> bool {
        self.text.is_some()
    }

    fn new(app: Arc<App>, interaction: &Interaction, tx: Option<oneshot::Sender<InteractionResponse>>) -> Self {
        Self {
            app,
            guild: interaction.guild_id.map(|id| id.get()),
            id: interaction.id,
            token: interaction.token.clone(),
            text: None,
            // A modal opened from a button answers by updating that button's message.
            component: interaction.kind == InteractionType::MessageComponent
                || (interaction.kind == InteractionType::ModalSubmit && interaction.message.is_some()),
            deferral: Deferral::of(interaction),
            state: Mutex::new(State::Pending(tx)),
            command_policy: OnceLock::new(),
            deletion_scheduled: OnceLock::new(),
            deferred_private: OnceLock::new(),
            migration_notice: OnceLock::new(),
        }
    }

    /// How a slow interaction is deferred, which the simulator checks against what handlers do.
    pub fn deferral(&self) -> Deferral {
        self.deferral
    }

    fn defer_kind(&self) -> InteractionResponseType {
        if self.component {
            InteractionResponseType::DeferredUpdateMessage
        } else {
            InteractionResponseType::DeferredChannelMessageWithSource
        }
    }

    fn defer_response(&self, ephemeral: bool) -> InteractionResponse {
        let _ = self.deferred_private.set(ephemeral);
        InteractionResponse {
            kind: self.defer_kind(),
            data: ephemeral.then(|| InteractionResponseData {
                flags: Some(MessageFlags::EPHEMERAL),
                ..Default::default()
            }),
        }
    }

    /// For the HTTP endpoint when its time budget runs out: claims the response
    /// slot and returns the defer to send, or `None` if the handler already answered.
    pub async fn defer_on_timeout(&self) -> Option<InteractionResponse> {
        let mut state = self.state.lock().await;
        match &mut *state {
            State::Pending(Some(_)) => {
                *state = State::Deferred;
                Some(self.defer_response(self.deferral == Deferral::Private || self.private_default()))
            }
            _ => None,
        }
    }

    /// For the gateway when the handler outlasts its time budget: defers through REST, and returns
    /// whether it did. It does not if the handler has answered meanwhile, or may open a modal, which
    /// only works as the first answer. The state lock is held across the REST call, so an answer
    /// arriving now waits and becomes an edit.
    pub async fn defer_if_pending(&self) -> anyhow::Result<bool> {
        if self.deferral == Deferral::Never || self.text.is_some() {
            return Ok(false);
        }
        let mut state = self.state.lock().await;
        if !matches!(&*state, State::Pending(None)) {
            return Ok(false);
        }
        self.initial(
            None,
            self.defer_response(self.deferral == Deferral::Private || self.private_default()),
        )
        .await?;
        *state = State::Deferred;
        Ok(true)
    }

    /// Shows "thinking…" (or, for buttons, acknowledges silently; for a text command, typing).
    pub async fn defer(&self, ephemeral: bool) -> anyhow::Result<()> {
        if let Some(text) = &self.text {
            if let Err(error) = self.app.http.create_typing_trigger(text.channel).await {
                tracing::debug!(%error, "typing not shown");
            }
            return Ok(());
        }
        let mut state = self.state.lock().await;
        if let State::Pending(tx) = &mut *state {
            self.initial(tx.take(), self.defer_response(ephemeral || self.private_default()))
                .await?;
            *state = State::Deferred;
        }
        Ok(())
    }

    /// Sends a new message (commands) or replaces the clicked message (`update`, components). A reply that
    /// sets no `allowed_mentions` pings nobody, so a name or a role in its text (a member's note, a role the
    /// server configured) never pings by accident; the replies that mean to ping say whom.
    pub async fn respond(&self, mut data: InteractionResponseData, update: bool) -> anyhow::Result<()> {
        if let (Some(guild), Some(embeds)) = (self.guild, data.embeds.as_mut()) {
            self.app.brand_embeds(guild, embeds).await;
        }
        if !update && self.private_default() {
            *data.flags.get_or_insert(MessageFlags::empty()) |= MessageFlags::EPHEMERAL;
        }
        data.allowed_mentions.get_or_insert_with(AllowedMentions::default);
        dump_attachments(&data);
        if let Some(text) = &self.text {
            let _order = self.state.lock().await;
            return match text.first.get() {
                Some(&first) if update => self.edit_text(text, first, &data).await,
                _ => self.post_text(text, data).await,
            };
        }
        let mut state = self.state.lock().await;
        // A slow cache miss can outlast the automatic defer. Discord cannot make a public
        // original private by editing flags, so deliver the private result as a follow-up.
        let replace_public = matches!(*state, State::Deferred)
            && !self.component
            && data.flags.is_some_and(|flags| flags.contains(MessageFlags::EPHEMERAL))
            && !self.deferred_private.get().copied().unwrap_or(false);
        let deletes_original = !matches!(*state, State::Done)
            && !(matches!(*state, State::Deferred) && self.component && !update)
            && !replace_public;
        let result = match &mut *state {
            State::Pending(tx) => {
                let has_files = data.attachments.as_ref().is_some_and(|a| !a.is_empty());
                if tx.is_some() && has_files {
                    let defer = InteractionResponse {
                        kind: if update {
                            InteractionResponseType::DeferredUpdateMessage
                        } else {
                            InteractionResponseType::DeferredChannelMessageWithSource
                        },
                        data: data.flags.map(|flags| InteractionResponseData {
                            flags: Some(flags),
                            ..Default::default()
                        }),
                    };
                    self.initial(tx.take(), defer).await?;
                    *state = State::Deferred;
                    self.edit_original(&data).await?;
                    // The edit was the answer: whatever comes next is a follow-up, as over the gateway.
                    *state = State::Done;
                    Ok(())
                } else {
                    let kind = if update {
                        InteractionResponseType::UpdateMessage
                    } else {
                        InteractionResponseType::ChannelMessageWithSource
                    };
                    self.initial(tx.take(), InteractionResponse { kind, data: Some(data) })
                        .await?;
                    *state = State::Done;
                    Ok(())
                }
            }
            State::Deferred => {
                // A button's defer shows nothing, so a new message (a private notice, say) belongs
                // beside the clicked message; editing it would put the notice on it for everyone.
                if replace_public {
                    self.followup(&data).await?;
                    if let Err(error) = self.app.interaction().delete_response(&self.token).await {
                        tracing::debug!(%error, "public command placeholder deletion failed");
                    }
                } else if self.component && !update {
                    self.followup(&data).await?;
                } else {
                    self.edit_original(&data).await?;
                }
                *state = State::Done;
                Ok(())
            }
            State::Done => self.followup(&data).await,
        };
        if result.is_ok() && deletes_original {
            self.schedule_original_deletion();
        }
        result
    }

    /// Opens a modal. Discord only takes one as the first answer.
    pub async fn modal(&self, data: InteractionResponseData) -> anyhow::Result<()> {
        if let Some(text) = &self.text {
            let _order = self.state.lock().await;
            return self.post_text(text, slash_only_notice(&text.path, text.lang)).await;
        }
        let mut state = self.state.lock().await;
        let State::Pending(tx) = &mut *state else {
            anyhow::bail!("a modal can only be the first answer");
        };
        let response = InteractionResponse {
            kind: InteractionResponseType::Modal,
            data: Some(data),
        };
        self.initial(tx.take(), response).await?;
        *state = State::Done;
        Ok(())
    }

    /// Answers an autocomplete with its suggestions (Discord takes at most 25). It takes no defer and
    /// no message: this is the first and only answer.
    pub async fn suggest(&self, choices: Vec<CommandOptionChoice>) -> anyhow::Result<()> {
        let mut state = self.state.lock().await;
        let State::Pending(tx) = &mut *state else {
            anyhow::bail!("suggestions can only be the first answer");
        };
        let response = InteractionResponse {
            kind: InteractionResponseType::ApplicationCommandAutocompleteResult,
            data: Some(InteractionResponseData {
                choices: Some(choices),
                ..Default::default()
            }),
        };
        self.initial(tx.take(), response).await?;
        *state = State::Done;
        Ok(())
    }

    async fn initial(
        &self,
        tx: Option<oneshot::Sender<InteractionResponse>>,
        response: InteractionResponse,
    ) -> anyhow::Result<()> {
        match tx {
            Some(tx) => tx
                .send(response)
                .map_err(|_| anyhow::anyhow!("HTTP interaction request already closed")),
            None => {
                self.app
                    .interaction()
                    .create_response(self.id, &self.token, &response)
                    .await?;
                Ok(())
            }
        }
    }

    /// Edits the first answer, e.g. to reveal something after a delay. Like [`Responder::respond`], it pings
    /// nobody unless `data` says whom.
    pub async fn edit(&self, data: &InteractionResponseData) -> anyhow::Result<()> {
        let mut branded = data.clone();
        if let (Some(guild), Some(embeds)) = (self.guild, branded.embeds.as_mut()) {
            self.app.brand_embeds(guild, embeds).await;
        }
        let data = &branded;
        let quiet;
        let data = if data.allowed_mentions.is_some() {
            data
        } else {
            quiet = InteractionResponseData {
                allowed_mentions: Some(AllowedMentions::default()),
                ..data.clone()
            };
            &quiet
        };
        if let Some(text) = &self.text {
            let first = text
                .first
                .get()
                .ok_or_else(|| anyhow::anyhow!("nothing posted to edit"))?;
            return self.edit_text(text, *first, data).await;
        }
        self.edit_original(data).await
    }

    /// The channel and id of the message the first answer became, for handlers that edit it later.
    pub async fn original(&self) -> anyhow::Result<(u64, u64)> {
        if let Some(text) = &self.text {
            let first = text.first.get().ok_or_else(|| anyhow::anyhow!("nothing posted yet"))?;
            return Ok((text.channel.get(), first.get()));
        }
        let message = self.app.interaction().response(&self.token).await?.model().await?;
        Ok((message.channel_id.get(), message.id.get()))
    }
}

/// What a text command answers instead of a private panel or a form: the slash command to use.
pub(crate) fn slash_only_notice(path: &str, lang: Lang) -> InteractionResponseData {
    InteractionResponseData {
        content: Some(t!(lang, "text.slash-only", path = path)),
        flags: Some(MessageFlags::EPHEMERAL),
        ..Default::default()
    }
}

/// Development aid: with `PINCHORA_DEBUG_DUMP_DIR` set, every outgoing image is also
/// written there, so cards and rooms can be checked without a Discord client.
fn dump_attachments(data: &InteractionResponseData) {
    static DIR: std::sync::LazyLock<Option<std::path::PathBuf>> =
        std::sync::LazyLock::new(|| std::env::var_os("PINCHORA_DEBUG_DUMP_DIR").map(Into::into));
    let (Some(dir), Some(files)) = (DIR.as_ref(), data.attachments.as_ref()) else {
        return;
    };
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis());
    for file in files {
        let _ = std::fs::write(dir.join(format!("{stamp}-{}", file.filename)), &file.file);
    }
}
