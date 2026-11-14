//! Reading what a source sent: RSS and Atom (YouTube, Reddit, podcasts, the bridge) through `feed-rs`,
//! Bluesky's author feed as JSON. Texts come out plain (HTML taken out, spaces collapsed) and short.

mod xml;
use xml::*;
mod content;
use content::*;
pub(crate) use content::{cut, is_web_link, plain, web_link};
mod bluesky;
use bluesky::*;
#[cfg(test)]
mod tests;

use chrono::{DateTime, Utc};
use feed_rs::model::{Entry, Feed, Text};
use pinchora_core::community::feeds::Item;
use pinchora_core::community::feeds::Kind;
use serde::Deserialize;

/// The most items kept from one fetch, newest first.
const ITEMS_MAX: usize = 50;
/// Longest kept title, summary and name, in characters.
const TITLE_MAX: usize = 256;
const SUMMARY_MAX: usize = 350;
const NAME_MAX: usize = 100;
const XML_DEPTH_MAX: usize = 64;
const XML_ELEMENTS_MAX: usize = 10_000;

/// What one fetch of a source holds.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Parsed {
    /// What the source calls itself.
    pub title: Option<String>,
    /// An RSS feed's own `ttl`, in minutes.
    pub ttl: Option<u32>,
    /// Newest first.
    pub items: Vec<Item>,
}

/// Why a body could not be read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParseError {
    /// Not RSS, Atom or the JSON that was expected.
    NotAFeed,
    /// Bluesky knows no such account.
    NoAccount,
}

impl ParseError {
    pub fn code(self) -> &'static str {
        match self {
            Self::NotAFeed => "feeds.error.parse",
            Self::NoAccount => "feeds.error.no-account",
        }
    }
}

pub fn parse(kind: Kind, body: &[u8]) -> Result<Parsed, ParseError> {
    match kind {
        Kind::Bluesky => bluesky(body),
        _ => xml(kind, body),
    }
}

fn xml(kind: Kind, body: &[u8]) -> Result<Parsed, ParseError> {
    xml_bounds(body)?;
    let feed: Feed = feed_rs::parser::parse(body).map_err(|_| ParseError::NotAFeed)?;
    let title = feed
        .title
        .as_ref()
        .map(text)
        .filter(|t| !t.is_empty())
        .map(|t| cut(&t, NAME_MAX));
    // feed-rs drops an episode's `itunes:duration` and `itunes:image` unless the item also has MediaRSS:
    // read them from the items in document order, when every item can be matched.
    let extras = podcast_extras(&String::from_utf8_lossy(body));
    let matched = extras.len() == feed.entries.len();
    let mut items: Vec<Item> = feed
        .entries
        .iter()
        .enumerate()
        .filter_map(|(at, e)| {
            let mut item = entry(kind, e)?;
            if let Some((duration, image)) = extras.get(at).filter(|_| matched) {
                item.duration_secs = item.duration_secs.or(*duration);
                item.image = item.image.or_else(|| image.clone());
            }
            Some(item)
        })
        .collect();
    // Most feeds list newest first; some do not. Undated items keep their place at the end.
    items.sort_by_key(|i| std::cmp::Reverse(i.published));
    items.truncate(ITEMS_MAX);
    Ok(Parsed {
        title,
        ttl: feed.ttl,
        items,
    })
}

fn decode_entities(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find('&') {
        out.push_str(&rest[..at]);
        let here = &rest[at..];
        let end = here.find(';').filter(|e| *e <= 10);
        let decoded = end.and_then(|end| {
            let name = &here[1..end];
            let c = match name {
                "amp" => Some('&'),
                "lt" => Some('<'),
                "gt" => Some('>'),
                "quot" => Some('"'),
                "apos" | "#39" => Some('\''),
                "nbsp" => Some(' '),
                _ => name
                    .strip_prefix("#x")
                    .and_then(|hex| u32::from_str_radix(hex, 16).ok())
                    .or_else(|| name.strip_prefix('#').and_then(|n| n.parse().ok()))
                    .and_then(char::from_u32),
            }?;
            Some((c, end))
        });
        match decoded {
            Some((c, end)) => {
                out.push(c);
                rest = &here[end + 1..];
            }
            None => {
                out.push('&');
                rest = &here[1..];
            }
        }
    }
    out.push_str(rest);
    out
}
