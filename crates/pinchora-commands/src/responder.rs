use std::sync::Arc;

use tokio::sync::{Mutex, oneshot};
use twilight_model::{
    application::interaction::{Interaction, InteractionType},
    channel::message::MessageFlags,
    http::interaction::{InteractionResponse, InteractionResponseData, InteractionResponseType},
    id::{Id, marker::InteractionMarker},
};

use crate::App;

/// Answers one interaction, whichever way it arrived.
///
/// Over the gateway every answer is a REST call. Over the HTTP endpoint the first
/// answer goes back as the HTTP response body; if the handler is slower than the
/// endpoint's budget, the endpoint sends a "thinking…" defer and later answers
/// become edits of that message. Images always take the defer-then-edit path,
/// because files cannot ride in a JSON response body.
pub struct Responder {
    app: Arc<App>,
    id: Id<InteractionMarker>,
    token: String,
    component: bool,
    state: Mutex<State>,
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

    fn new(app: Arc<App>, interaction: &Interaction, tx: Option<oneshot::Sender<InteractionResponse>>) -> Self {
        Self {
            app,
            id: interaction.id,
            token: interaction.token.clone(),
            component: interaction.kind == InteractionType::MessageComponent,
            state: Mutex::new(State::Pending(tx)),
        }
    }

    fn defer_kind(&self) -> InteractionResponseType {
        if self.component {
            InteractionResponseType::DeferredUpdateMessage
        } else {
            InteractionResponseType::DeferredChannelMessageWithSource
        }
    }

    /// For the HTTP endpoint when its time budget runs out: claims the response
    /// slot and returns the defer to send, or `None` if the handler already answered.
    pub async fn defer_on_timeout(&self) -> Option<InteractionResponse> {
        let mut state = self.state.lock().await;
        match &mut *state {
            State::Pending(Some(_)) => {
                *state = State::Deferred;
                Some(InteractionResponse {
                    kind: self.defer_kind(),
                    data: None,
                })
            }
            _ => None,
        }
    }

    /// Shows "thinking…" (or, for buttons, acknowledges silently).
    pub async fn defer(&self, ephemeral: bool) -> anyhow::Result<()> {
        let mut state = self.state.lock().await;
        if let State::Pending(tx) = &mut *state {
            let data = ephemeral.then(|| InteractionResponseData {
                flags: Some(MessageFlags::EPHEMERAL),
                ..Default::default()
            });
            let response = InteractionResponse {
                kind: self.defer_kind(),
                data,
            };
            self.initial(tx.take(), response).await?;
            *state = State::Deferred;
        }
        Ok(())
    }

    /// Sends a new message (commands) or replaces the clicked message (`update`, components).
    pub async fn respond(&self, data: InteractionResponseData, update: bool) -> anyhow::Result<()> {
        dump_attachments(&data);
        let mut state = self.state.lock().await;
        match &mut *state {
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
                    self.edit_original(&data).await
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
                self.edit_original(&data).await?;
                *state = State::Done;
                Ok(())
            }
            State::Done => self.followup(&data).await,
        }
    }

    /// Opens a modal. Discord only takes one as the first answer.
    pub async fn modal(&self, data: InteractionResponseData) -> anyhow::Result<()> {
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

    /// Edits the first answer, e.g. to reveal something after a delay.
    pub async fn edit(&self, data: &InteractionResponseData) -> anyhow::Result<()> {
        self.edit_original(data).await
    }

    async fn edit_original(&self, data: &InteractionResponseData) -> anyhow::Result<()> {
        let client = self.app.interaction();
        let mut request = client.update_response(&self.token);
        if let Some(content) = &data.content {
            request = request.content(Some(content));
        }
        if let Some(embeds) = &data.embeds {
            request = request.embeds(Some(embeds));
        }
        if let Some(components) = &data.components {
            request = request.components(Some(components));
        }
        if let Some(attachments) = &data.attachments {
            request = request.attachments(attachments);
        }
        request.await?;
        Ok(())
    }

    async fn followup(&self, data: &InteractionResponseData) -> anyhow::Result<()> {
        let client = self.app.interaction();
        let mut request = client.create_followup(&self.token);
        if let Some(content) = &data.content {
            request = request.content(content);
        }
        if let Some(embeds) = &data.embeds {
            request = request.embeds(embeds);
        }
        if let Some(components) = &data.components {
            request = request.components(components);
        }
        if let Some(attachments) = &data.attachments {
            request = request.attachments(attachments);
        }
        if let Some(flags) = data.flags {
            request = request.flags(flags);
        }
        request.await?;
        Ok(())
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
