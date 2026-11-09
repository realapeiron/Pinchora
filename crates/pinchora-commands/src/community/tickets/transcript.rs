//! A ticket's messages read back from Discord, as its transcript: the lines (text only with the Message
//! Content intent), who wrote them, and the page members download.

use std::collections::HashMap;

use chrono::{DateTime, Utc};
use pinchora_core::community::tickets::Author;
use pinchora_core::community::tickets::Line;
use pinchora_core::community::tickets::TRANSCRIPT_MAX;
use pinchora_core::community::tickets::TranscriptHead;
use pinchora_core::community::tickets::first_reply;
use pinchora_core::community::tickets::render_html_with_mentions;
use pinchora_core::infra::i18n::Lang;
use pinchora_db::{StoredTranscript, Ticket};
use twilight_model::{
    channel::Message,
    id::{Id, marker::MessageMarker},
};

use crate::App;

/// What one read of a ticket's history gave.
pub(crate) struct Captured {
    pub lines: Vec<Line>,
    /// `(user, name, bot)`, in the order of the lines' author numbers.
    pub authors: Vec<(u64, String, bool)>,
    pub with_content: bool,
    pub first_reply: Option<DateTime<Utc>>,
}

impl Captured {
    pub fn authors(&self) -> Vec<Author> {
        self.authors
            .iter()
            .enumerate()
            .map(|(idx, (_, name, bot))| Author {
                idx: idx as u16,
                name: Some(name.clone()),
                bot: *bot,
            })
            .collect()
    }

    pub fn mention_names(&self) -> HashMap<u64, String> {
        self.authors.iter().map(|(id, name, _)| (*id, name.clone())).collect()
    }
}

/// The name a message shows for its author.
fn author_name(message: &Message) -> String {
    message
        .author
        .global_name
        .clone()
        .unwrap_or_else(|| message.author.name.clone())
}

/// Keep mention ids, never a copied username that would survive that member deleting their data.
fn text_of(message: &Message) -> String {
    message.content.clone()
}

/// Reads up to [`TRANSCRIPT_MAX`] of the newest messages of the ticket's channel or thread, oldest first.
pub(crate) async fn capture(app: &App, ticket: &Ticket) -> anyhow::Result<Captured> {
    let Some(place) = ticket.place() else {
        return Ok(Captured {
            lines: Vec::new(),
            authors: Vec::new(),
            with_content: app.message_content,
            first_reply: None,
        });
    };
    let channel = Id::new(place);
    let mut messages: Vec<Message> = Vec::new();
    let mut before: Option<Id<MessageMarker>> = None;
    while messages.len() < TRANSCRIPT_MAX {
        let request = app.http.channel_messages(channel).limit(100);
        let page: Vec<Message> = match before {
            Some(before) => request.before(before).await?.models().await?,
            None => request.await?.models().await?,
        };
        let full = page.len() == 100;
        before = page.last().map(|m| m.id);
        messages.extend(page);
        if !full {
            break;
        }
    }
    messages.truncate(TRANSCRIPT_MAX);
    messages.reverse();
    let mut authors: Vec<(u64, String, bool)> = Vec::new();
    let mut lines = Vec::with_capacity(messages.len());
    for message in &messages {
        let id = message.author.id.get();
        let a = match authors.iter().position(|(user, _, _)| *user == id) {
            Some(a) => a,
            None => {
                authors.push((id, author_name(message), message.author.bot));
                authors.len() - 1
            }
        };
        let at = DateTime::from_timestamp_micros(message.timestamp.as_micros())
            .unwrap_or_else(|| pinchora_core::infra::snowflake::timestamp(message.id.get()));
        lines.push(Line {
            a: a as u16,
            t: at,
            c: app.message_content.then(|| text_of(message)),
            f: message.attachments.iter().map(|f| f.filename.clone()).collect(),
            e: message.embeds.len().min(u16::MAX as usize) as u16,
        });
        for user in &message.mentions {
            if !authors.iter().any(|(id, _, _)| *id == user.id.get()) {
                authors.push((user.id.get(), user.name.clone(), user.bot));
            }
        }
    }
    let opener = authors
        .iter()
        .position(|(user, _, _)| *user == ticket.opener_id as u64)
        .map(|a| a as u16);
    let listed: Vec<Author> = authors
        .iter()
        .enumerate()
        .map(|(idx, (_, name, bot))| Author {
            idx: idx as u16,
            name: Some(name.clone()),
            bot: *bot,
        })
        .collect();
    let first = first_reply(&lines, &listed, opener).filter(|at| *at >= ticket.opened_at);
    Ok(Captured {
        lines,
        authors,
        with_content: app.message_content,
        first_reply: first,
    })
}

/// The transcript page of `ticket` from what was read or kept, its names as the server shows them.
pub(crate) struct Page<'a> {
    pub server: &'a str,
    pub opener: Option<&'a str>,
    pub closed_by: Option<&'a str>,
}

pub(crate) fn page_html(
    ticket: &Ticket,
    names: &Page<'_>,
    lines: &[Line],
    authors: &[Author],
    mentions: &HashMap<u64, String>,
    with_content: bool,
    lang: Lang,
) -> Vec<u8> {
    let head = TranscriptHead {
        server: names.server,
        number: ticket.number,
        topic: &ticket.topic,
        opener: names.opener,
        opened_at: ticket.opened_at,
        closed_at: ticket.closed_at,
        closed_by: names.closed_by,
        reason: ticket.close_reason.as_deref(),
        with_content,
    };
    render_html_with_mentions(&head, lines, authors, mentions, lang).into_bytes()
}

/// The page of a kept transcript.
pub(crate) fn stored_html(ticket: &Ticket, names: &Page<'_>, stored: &StoredTranscript, lang: Lang) -> Vec<u8> {
    let mentions: HashMap<u64, String> = stored
        .author_ids
        .iter()
        .filter_map(|(idx, id)| {
            stored
                .authors
                .iter()
                .find(|author| author.idx == *idx)?
                .name
                .as_ref()
                .map(|name| (*id, name.clone()))
        })
        .collect();
    page_html(
        ticket,
        names,
        &stored.lines,
        &stored.authors,
        &mentions,
        stored.with_content,
        lang,
    )
}

/// `ticket-0042.html`.
pub(crate) fn file_name(ticket: &Ticket) -> String {
    format!("ticket-{:04}.html", ticket.number)
}
