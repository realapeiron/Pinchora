use super::*;

impl Responder {
    /// A text command's answer, as a reply to the member's message. Private panels become a notice.
    pub(super) async fn post_text(&self, text: &TextTarget, mut data: InteractionResponseData) -> anyhow::Result<()> {
        let flags = data.flags.unwrap_or(MessageFlags::empty());
        let private = flags.contains(MessageFlags::EPHEMERAL);
        if private && data.components.as_ref().is_some_and(|c| !c.is_empty()) {
            data = slash_only_notice(&text.path, text.lang);
        }
        if text.first.get().is_none()
            && let Some(notice) = self.migration_notice.get()
        {
            let content = data.content.get_or_insert_with(String::new);
            if content.chars().count() + notice.chars().count() < 2000 {
                if !content.is_empty() {
                    content.push('\n');
                }
                content.push_str(notice);
            }
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
        if let Some(seconds) = self
            .command_policy
            .get()
            .map(|p| p.delete_reply_secs)
            .filter(|s| *s > 0)
        {
            self.app
                .delete_later(text.channel, posted.id, Duration::from_secs(seconds.into()));
        }
        if private {
            self.app.delete_later(text.channel, posted.id, TEXT_PRIVATE_LINGER);
        }
        Ok(())
    }

    pub(super) async fn edit_text(
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

    pub(super) async fn edit_original(&self, data: &InteractionResponseData) -> anyhow::Result<()> {
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

    pub(super) async fn followup(&self, data: &InteractionResponseData) -> anyhow::Result<()> {
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
        let response = request.await?;
        let seconds = self.command_policy.get().map_or(0, |p| p.delete_reply_secs);
        if seconds > 0 {
            let message = response.model().await?;
            let app = self.app.clone();
            let token = self.token.clone();
            self.app.spawn(async move {
                tokio::time::sleep(Duration::from_secs(seconds.into())).await;
                if let Err(error) = app.interaction().delete_followup(&token, message.id).await {
                    tracing::debug!(%error, "command followup deletion failed");
                }
            });
        }
        Ok(())
    }
}
