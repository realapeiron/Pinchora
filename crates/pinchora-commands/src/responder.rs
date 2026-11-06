use std::{
    sync::{Arc, OnceLock},
    time::Duration,
};

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
/// member's message that pings nobody. A private answer is deleted after [`TEXT_PRIVATE_LINGER`],
/// and a private panel or a form, which a channel reply cannot stand in for, becomes a notice to use
/// the slash command.
pub struct Responder {
    app: Arc<App>,
    id: Id<InteractionMarker>,
    token: String,
    text: Option<TextTarget>,
    component: bool,
    deferral: Deferral,
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

    /// Answers a text command: `message` in `channel` is the member's, `path` the command it ran.
    pub fn text(app: Arc<App>, channel: Id<ChannelMarker>, message: Id<MessageMarker>, path: String) -> Self {
        Self {
            app,
            id: message.cast(),
            token: format!("text-{message}"),
            text: Some(TextTarget {
                channel,
                message,
                path,
                first: OnceLock::new(),
            }),
            component: false,
            deferral: Deferral::Never,
            state: Mutex::new(State::Pending(None)),
        }
    }

    /// Whether this answers a text command rather than an interaction.
    pub fn is_text(&self) -> bool {
        self.text.is_some()
    }

    fn new(app: Arc<App>, interaction: &Interaction, tx: Option<oneshot::Sender<InteractionResponse>>) -> Self {
        Self {
            app,
            id: interaction.id,
            token: interaction.token.clone(),
            text: None,
            // A modal opened from a button answers by updating that button's message.
            component: interaction.kind == InteractionType::MessageComponent
                || (interaction.kind == InteractionType::ModalSubmit && interaction.message.is_some()),
            deferral: Deferral::of(interaction),
            state: Mutex::new(State::Pending(tx)),
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
                Some(self.defer_response(self.deferral == Deferral::Private))
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
        self.initial(None, self.defer_response(self.deferral == Deferral::Private))
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
            self.initial(tx.take(), self.defer_response(ephemeral)).await?;
            *state = State::Deferred;
        }
        Ok(())
    }

    /// Sends a new message (commands) or replaces the clicked message (`update`, components).
    pub async fn respond(&self, data: InteractionResponseData, update: bool) -> anyhow::Result<()> {
        dump_attachments(&data);
        if let Some(text) = &self.text {
            let _order = self.state.lock().await;
            return match text.first.get() {
                Some(&first) if update => self.edit_text(text, first, &data).await,
                _ => self.post_text(text, data).await,
            };
        }
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
                if self.component && !update {
                    self.followup(&data).await?;
                } else {
                    self.edit_original(&data).await?;
                }
                *state = State::Done;
                Ok(())
            }
            State::Done => self.followup(&data).await,
        }
    }

    /// Opens a modal. Discord only takes one as the first answer.
    pub async fn modal(&self, data: InteractionResponseData) -> anyhow::Result<()> {
        if let Some(text) = &self.text {
            let _order = self.state.lock().await;
            return self.post_text(text, slash_only_notice(&text.path)).await;
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

    /// Edits the first answer, e.g. to reveal something after a delay.
    pub async fn edit(&self, data: &InteractionResponseData) -> anyhow::Result<()> {
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

    /// A text command's answer, as a reply to the member's message. Private panels become a notice.
    async fn post_text(&self, text: &TextTarget, mut data: InteractionResponseData) -> anyhow::Result<()> {
        let flags = data.flags.unwrap_or(MessageFlags::empty());
        let private = flags.contains(MessageFlags::EPHEMERAL);
        if private && data.components.as_ref().is_some_and(|c| !c.is_empty()) {
            data = slash_only_notice(&text.path);
        }
        let none = AllowedMentions::default();
        let mentions = data.allowed_mentions.as_ref().unwrap_or(&none);
        let mut request = self
            .app
            .http
            .create_message(text.channel)
            .reply(text.message)
            .fail_if_not_exists(false)
            .allowed_mentions(Some(mentions));
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
        let rest = flags - MessageFlags::EPHEMERAL;
        if !rest.is_empty() {
            request = request.flags(rest);
        }
        let posted = request.await?.model().await?;
        let _ = text.first.set(posted.id);
        if private {
            let (app, channel, id) = (self.app.clone(), text.channel, posted.id);
            tokio::spawn(async move {
                tokio::time::sleep(TEXT_PRIVATE_LINGER).await;
                if let Err(error) = app.http.delete_message(channel, id).await {
                    tracing::debug!(%error, "private text answer not deleted");
                }
            });
        }
        Ok(())
    }

    async fn edit_text(
        &self,
        text: &TextTarget,
        message: Id<MessageMarker>,
        data: &InteractionResponseData,
    ) -> anyhow::Result<()> {
        let mut request = self.app.http.update_message(text.channel, message);
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
        if let Some(mentions) = &data.allowed_mentions {
            request = request.allowed_mentions(Some(mentions));
        }
        request.await?;
        Ok(())
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
        if let Some(mentions) = &data.allowed_mentions {
            request = request.allowed_mentions(Some(mentions));
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
        if let Some(mentions) = &data.allowed_mentions {
            request = request.allowed_mentions(Some(mentions));
        }
        request.await?;
        Ok(())
    }
}

/// What a text command answers instead of a private panel or a form: the slash command to use.
pub(crate) fn slash_only_notice(path: &str) -> InteractionResponseData {
    InteractionResponseData {
        content: Some(format!("This one only works as a slash command: use `/{path}`.")),
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

#[cfg(test)]
mod tests {
    use std::{
        sync::{
            Arc, Mutex as StdMutex,
            atomic::{AtomicU64, Ordering},
        },
        time::Duration,
    };

    use axum::{
        Router,
        body::Bytes,
        extract::State,
        http::{Method, Uri},
    };
    use pinchora_cache::Cache;
    use pinchora_db::Db;
    use serde_json::{Value, json};
    use tokio::{net::TcpListener, sync::oneshot, time::sleep};
    use twilight_http::Client;

    use super::*;
    use crate::defer::defer_slow;

    /// One REST call the mock Discord saw.
    struct Call {
        method: Method,
        path: String,
        body: Value,
    }

    type Calls = Arc<StdMutex<Vec<Call>>>;

    async fn record(State(calls): State<Calls>, method: Method, uri: Uri, body: Bytes) -> &'static str {
        calls.lock().unwrap().push(Call {
            method,
            path: uri.path().to_owned(),
            body: serde_json::from_slice(&body).unwrap_or(Value::Null),
        });
        "{}"
    }

    /// The real [`App`] on the test databases with Discord replaced by a mock that writes down every call.
    /// `None` without the databases, like the simulator.
    struct Harness {
        app: Arc<App>,
        calls: Calls,
    }

    impl Harness {
        async fn new() -> Option<Self> {
            let (Ok(database), Ok(redis)) = (
                std::env::var("PINCHORA_TEST_DATABASE_URL"),
                std::env::var("PINCHORA_TEST_REDIS_URL"),
            ) else {
                eprintln!("skipped, set PINCHORA_TEST_DATABASE_URL and PINCHORA_TEST_REDIS_URL");
                return None;
            };
            let calls = Calls::default();
            let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind the mock");
            let mock = listener.local_addr().expect("mock address");
            let router = Router::new().fallback(record).with_state(calls.clone());
            tokio::spawn(axum::serve(listener, router).into_future());
            let rest = Arc::new(
                Client::builder()
                    .token("Bot test".into())
                    .proxy(mock.to_string(), true)
                    .ratelimiter(None)
                    .build(),
            );
            let db = Db::connect_waiting(&database, 1, std::time::Duration::from_secs(60))
                .await
                .expect("connect to the database");
            let cache = Cache::connect(&redis).await.expect("connect to redis");
            let app = App::new(rest.clone(), rest, Id::new(1), db, cache, "test".into());
            Some(Self {
                app: Arc::new(app),
                calls,
            })
        }

        /// A gateway responder for `interaction`.
        fn responder(&self, interaction: &Interaction) -> Arc<Responder> {
            Arc::new(Responder::rest(self.app.clone(), interaction))
        }

        /// What Discord was asked to do so far: `callback <type>`, `edit` or `followup`, each
        /// followed by `ephemeral` when it was private.
        fn seen(&self) -> Vec<String> {
            self.calls
                .lock()
                .unwrap()
                .iter()
                .map(|call| {
                    let flags = call.body["data"]["flags"]
                        .as_u64()
                        .or(call.body["flags"].as_u64())
                        .unwrap_or(0);
                    let private = if flags & 64 != 0 { " ephemeral" } else { "" };
                    if call.path.ends_with("/callback") {
                        format!("callback {}{private}", call.body["type"])
                    } else if call.method == Method::PATCH {
                        format!("edit{private}")
                    } else {
                        format!("followup{private}")
                    }
                })
                .collect()
        }

        /// Waits until Discord has been asked `n` times.
        async fn wait_for(&self, n: usize) {
            for _ in 0..2_000 {
                if self.calls.lock().unwrap().len() >= n {
                    return;
                }
                sleep(Duration::from_millis(5)).await;
            }
            panic!("only {:?} after waiting for {n} calls", self.seen());
        }
    }

    static NEXT: AtomicU64 = AtomicU64::new(1_000);

    /// An interaction as Discord sends it; `message` is the one a component sits on.
    fn interaction(kind: u8, data: Value, message: bool) -> Interaction {
        let id = NEXT.fetch_add(1, Ordering::Relaxed);
        let mut interaction = json!({
            "id": id.to_string(),
            "application_id": "1",
            "type": kind,
            "token": format!("test-{id}"),
            "version": 1,
            "guild_id": "5",
            "channel": { "id": "2", "type": 0, "guild_id": "5" },
            "locale": "en-US",
            "guild_locale": "en-US",
            "context": 0,
            "attachment_size_limit": 26_214_400,
            "authorizing_integration_owners": {},
            "entitlements": [],
            "member": {
                "user": { "id": "7", "username": "tester", "discriminator": "0", "global_name": "Tester", "avatar": null },
                "roles": [],
                "joined_at": "2024-01-01T00:00:00.000000+00:00",
                "deaf": false,
                "mute": false,
                "flags": 0,
                "permissions": "0",
            },
            "data": data,
        });
        if message {
            interaction["message"] = json!({
                "id": "3",
                "channel_id": "2",
                "type": 0,
                "author": { "id": "1", "username": "Pinchora", "discriminator": "0", "avatar": null, "bot": true },
                "content": "",
                "timestamp": "2024-01-01T00:00:00.000000+00:00",
                "edited_timestamp": null,
                "tts": false,
                "mention_everyone": false,
                "mentions": [],
                "mention_roles": [],
                "pinned": false,
                "attachments": [],
                "embeds": [],
                "components": [],
                "flags": 0,
            });
        }
        serde_json::from_value(interaction).expect("interaction JSON")
    }

    /// `/name` or, with `sub`, `/name sub`.
    fn command(name: &str, sub: Option<&str>) -> Interaction {
        let options = sub.map_or_else(|| json!([]), |sub| json!([{ "name": sub, "type": 1, "options": [] }]));
        interaction(
            2,
            json!({ "id": "9", "name": name, "type": 1, "options": options }),
            false,
        )
    }

    /// A click on a button of a message.
    fn click(custom_id: &str) -> Interaction {
        interaction(
            3,
            json!({ "custom_id": custom_id, "component_type": 2, "values": [] }),
            true,
        )
    }

    fn say(text: &str) -> InteractionResponseData {
        InteractionResponseData {
            content: Some(text.to_owned()),
            ..Default::default()
        }
    }

    fn say_privately(text: &str) -> InteractionResponseData {
        InteractionResponseData {
            flags: Some(MessageFlags::EPHEMERAL),
            ..say(text)
        }
    }

    /// Runs a handler that answers once `gate` opens, under a 20 ms timer, and gives back the handle.
    /// The test sees Discord's side while the handler is still waiting.
    fn slow(
        responder: &Arc<Responder>,
        answer: InteractionResponseData,
        update: bool,
    ) -> (oneshot::Sender<()>, tokio::task::JoinHandle<()>) {
        let (open, gate) = oneshot::channel::<()>();
        let responder = responder.clone();
        let task = tokio::spawn(async move {
            let handler = async {
                let _ = gate.await;
                responder.respond(answer, update).await.expect("answer");
            };
            defer_slow(&responder, Duration::from_millis(20), handler).await;
        });
        (open, task)
    }

    #[tokio::test]
    async fn a_fast_handler_is_answered_without_a_defer() {
        let Some(harness) = Harness::new().await else { return };
        let responder = harness.responder(&command("profile", None));
        let handler = async { responder.respond(say("quick"), false).await.expect("answer") };
        defer_slow(&responder, Duration::from_secs(5), handler).await;
        assert_eq!(harness.seen(), ["callback 4"]);
    }

    #[tokio::test]
    async fn a_slow_handler_gets_one_defer_and_then_an_edit() {
        let Some(harness) = Harness::new().await else { return };
        let responder = harness.responder(&command("profile", None));
        assert_eq!(responder.deferral(), Deferral::Public);
        let (open, task) = slow(&responder, say("late"), false);
        harness.wait_for(1).await;
        assert_eq!(harness.seen(), ["callback 5"], "a public thinking message");
        open.send(()).unwrap();
        task.await.unwrap();
        assert_eq!(harness.seen(), ["callback 5", "edit"]);
    }

    #[tokio::test]
    async fn a_slow_private_command_is_deferred_privately() {
        let Some(harness) = Harness::new().await else { return };
        let responder = harness.responder(&command("house", Some("edit")));
        assert_eq!(responder.deferral(), Deferral::Private);
        let (open, task) = slow(&responder, say_privately("late"), false);
        harness.wait_for(1).await;
        assert_eq!(harness.seen(), ["callback 5 ephemeral"]);
        open.send(()).unwrap();
        task.await.unwrap();
        assert_eq!(harness.seen(), ["callback 5 ephemeral", "edit"]);
    }

    #[tokio::test]
    async fn a_slow_click_keeps_the_clicked_message() {
        let Some(harness) = Harness::new().await else { return };
        let responder = harness.responder(&click("qc:1"));
        let (open, task) = slow(&responder, say("updated"), true);
        harness.wait_for(1).await;
        assert_eq!(harness.seen(), ["callback 6"], "deferred update, nothing shown");
        open.send(()).unwrap();
        task.await.unwrap();
        assert_eq!(harness.seen(), ["callback 6", "edit"]);
    }

    #[tokio::test]
    async fn a_slow_click_answers_a_private_notice_beside_the_message() {
        let Some(harness) = Harness::new().await else { return };
        let responder = harness.responder(&click("qc:1"));
        let (open, task) = slow(&responder, say_privately("Not yours."), false);
        harness.wait_for(1).await;
        open.send(()).unwrap();
        task.await.unwrap();
        assert_eq!(
            harness.seen(),
            ["callback 6", "followup ephemeral"],
            "an edit would put the notice on the clicked message for everyone"
        );
    }

    #[tokio::test]
    async fn a_click_that_may_open_a_modal_is_never_deferred() {
        let Some(harness) = Harness::new().await else { return };
        let responder = harness.responder(&click("wo:g:daily:5"));
        assert_eq!(responder.deferral(), Deferral::Never);
        let handler = async {
            sleep(Duration::from_millis(80)).await;
            let form = InteractionResponseData {
                custom_id: Some("wo:m:daily:5".to_owned()),
                title: Some("Wordle guess".to_owned()),
                ..Default::default()
            };
            responder.modal(form).await.expect("modal");
        };
        defer_slow(&responder, Duration::from_millis(20), handler).await;
        assert_eq!(harness.seen(), ["callback 9"]);
    }

    /// The handler answers about when the timer fires, over and over: whichever gets the response
    /// slot first, Discord must see exactly one first answer and exactly one message from it.
    #[tokio::test]
    async fn the_timer_and_the_answer_never_both_take_the_first_slot() {
        let Some(harness) = Harness::new().await else { return };
        let mut deferred = 0;
        for round in 0..40u64 {
            harness.calls.lock().unwrap().clear();
            let responder = harness.responder(&command("profile", None));
            let handler = async {
                sleep(Duration::from_millis(round / 2)).await;
                responder.respond(say("answer"), false).await.expect("answer");
            };
            defer_slow(&responder, Duration::from_millis(10), handler).await;
            let seen = harness.seen();
            match seen.iter().map(String::as_str).collect::<Vec<_>>()[..] {
                ["callback 4"] => {}
                ["callback 5", "edit"] => deferred += 1,
                _ => panic!("round {round}: {seen:?}"),
            }
        }
        eprintln!("the timer won {deferred} of 40 rounds");
    }
}
