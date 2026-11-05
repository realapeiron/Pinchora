//! House guestbooks: a visitor signs one after `/house visit`, the owner reads the notes,
//! deletes them and blocks signers. Notes are plain text: no mentions, no links, no markdown.

use std::str::Split;

use chrono::Utc;
use pinchora_db::{NoteRow, SignOutcome};
use twilight_interactions::command::{CommandModel, CreateCommand, ResolvedUser};
use twilight_model::{
    channel::message::{
        Component, MessageFlags,
        component::{
            ActionRow, Button, ButtonStyle, Label, SelectMenu, SelectMenuOption, SelectMenuType, TextInput,
            TextInputStyle,
        },
    },
    http::interaction::InteractionResponseData,
};
use twilight_util::builder::embed::{EmbedBuilder, EmbedFieldBuilder, EmbedFooterBuilder};

use super::house::ensure_house;
use crate::{
    Cx,
    util::{BRAND, mention, relative, resolved_display_name},
};

/// Longest note, in characters.
pub(crate) const NOTE_MAX: usize = 140;
/// Notes per page.
const PER_PAGE: i64 = 5;
/// Visitors listed under the notes.
const VISITORS: i64 = 10;
/// The `seen` a public page carries: no note is new, and nobody gets the owner's menus.
const PUBLIC: i64 = i64::MAX;
/// The note's text input in the modal.
const NOTE_FIELD: &str = "note";

#[derive(CommandModel, CreateCommand)]
#[command(name = "guestbook", desc = "Read a house's guestbook and see who visited")]
pub struct HouseGuestbook {
    /// Whose guestbook (default: yours)
    user: Option<ResolvedUser>,
}

pub(crate) async fn run(cx: &Cx, c: HouseGuestbook) -> anyhow::Result<()> {
    let author = cx.author()?.id.get();
    let (host, title) = match &c.user {
        Some(user) if user.resolved.bot => return cx.reply_ephemeral("Bots don't have houses.").await,
        Some(user) if user.resolved.id.get() != author => (
            user.resolved.id.get(),
            format!("📖 {}'s guestbook", resolved_display_name(user)),
        ),
        _ => (author, "📖 Your guestbook".to_owned()),
    };
    ensure_house(&cx.app, host).await?;
    let data = page(cx, host, title, 0, None, "").await?;
    cx.reply(data).await
}

/// One page of the guestbook. The owner sees it privately, with menus to delete notes and block
/// signers, and notes newer than `seen` (the newest read before the book was opened) marked
/// new; anyone else gets a public, read-only page.
async fn page(
    cx: &Cx,
    host: u64,
    title: String,
    page: i64,
    seen: Option<i64>,
    note: &str,
) -> anyhow::Result<InteractionResponseData> {
    let db = &cx.app.db;
    // A public page stays public when the owner turns it, so their menus never show there.
    let own = cx.author()?.id.get() == host && seen != Some(PUBLIC);
    let (mut notes, total) = db.guestbook(host, page * PER_PAGE, PER_PAGE).await?;
    let pages = (total + PER_PAGE - 1).div_euclid(PER_PAGE).max(1);
    let page = page.clamp(0, pages - 1);
    if notes.is_empty() && total > 0 {
        notes = db.guestbook(host, page * PER_PAGE, PER_PAGE).await?.0;
    }
    let visitors = db.recent_visitors(host, VISITORS).await?;
    let (seen, blocked) = if own {
        let seen = match seen {
            Some(seen) => seen,
            None => db.guestbook_seen(host).await?,
        };
        db.mark_guestbook_read(host).await?;
        (seen, db.guestbook_blocks(host).await?)
    } else {
        (PUBLIC, Vec::new())
    };

    let mut lines: Vec<String> = Vec::new();
    if !note.is_empty() {
        lines.push(note.to_owned());
    }
    if notes.is_empty() {
        lines.push(if own {
            "No notes yet. Visitors can sign it after `/house visit`.".to_owned()
        } else {
            "No notes yet. Visit with `/house visit` and press **Sign guestbook**.".to_owned()
        });
    }
    for n in &notes {
        lines.push(format!(
            "**{}** · {}{}\n{}",
            plain(&n.author_name),
            relative(n.created_at),
            if n.id > seen { " · **new**" } else { "" },
            plain(&n.body)
        ));
    }
    let visited = if visitors.is_empty() {
        "Nobody has visited yet.".to_owned()
    } else {
        visitors
            .iter()
            .map(|v| format!("{} · {}", mention(v.guest_id), relative(v.visited_at)))
            .collect::<Vec<_>>()
            .join("\n")
    };
    let footer = match (pages, total) {
        (1, 1) => "1 note".to_owned(),
        (1, n) => format!("{n} notes"),
        (p, n) => format!("Page {}/{p} · {n} notes", page + 1),
    };
    let embed = EmbedBuilder::new()
        .color(BRAND)
        .title(title)
        .description(lines.join("\n\n"))
        .field(EmbedFieldBuilder::new("Recent visitors", visited))
        .footer(EmbedFooterBuilder::new(footer))
        .build();

    let mut components = Vec::new();
    if pages > 1 {
        let nav = |label: &str, target: i64, disabled: bool| {
            Component::Button(Button {
                custom_id: Some(format!("gb:p:{host}:{target}:{seen}")),
                disabled,
                emoji: None,
                id: None,
                label: Some(label.to_owned()),
                sku_id: None,
                style: ButtonStyle::Secondary,
                url: None,
            })
        };
        components.push(row(vec![
            nav("◀ Newer", page - 1, page == 0),
            nav("Older ▶", page + 1, page + 1 >= pages),
        ]));
    }
    if own && !notes.is_empty() {
        let options = notes
            .iter()
            .map(|n| SelectMenuOption {
                default: false,
                description: Some(format!("Signed {}", n.created_at.format("%b %-d, %H:%M UTC"))),
                emoji: None,
                label: clip(&format!("{}: {}", n.author_name, n.body), 100),
                value: n.id.to_string(),
            })
            .collect();
        components.push(row(vec![menu(
            format!("gb:d:{host}:{page}:{seen}"),
            "Delete a note…",
            options,
        )]));
    }
    if own {
        let mut options: Vec<SelectMenuOption> = Vec::new();
        for n in &notes {
            if options.iter().any(|o| o.value == format!("b{}", n.author_id)) {
                continue;
            }
            options.push(SelectMenuOption {
                default: false,
                description: Some("Removes their notes; they can't sign again".to_owned()),
                emoji: None,
                label: clip(&format!("Block {}", n.author_name), 100),
                value: format!("b{}", n.author_id),
            });
        }
        let room = 25usize.saturating_sub(options.len());
        options.extend(blocked.iter().take(room).map(|(id, name)| SelectMenuOption {
            default: false,
            description: Some("Let them sign your guestbook again".to_owned()),
            emoji: None,
            label: clip(&format!("Unblock {name}"), 100),
            value: format!("u{id}"),
        }));
        if !options.is_empty() {
            components.push(row(vec![menu(
                format!("gb:b:{host}:{page}:{seen}"),
                "Block or unblock a signer…",
                options,
            )]));
        }
    }
    Ok(InteractionResponseData {
        embeds: Some(vec![embed]),
        components: Some(components),
        flags: own.then_some(MessageFlags::EPHEMERAL),
        ..Default::default()
    })
}

fn row(components: Vec<Component>) -> Component {
    Component::ActionRow(ActionRow { id: None, components })
}

fn menu(custom_id: String, placeholder: &str, options: Vec<SelectMenuOption>) -> Component {
    Component::SelectMenu(SelectMenu {
        channel_types: None,
        custom_id,
        default_values: None,
        disabled: false,
        id: None,
        kind: SelectMenuType::Text,
        max_values: Some(1),
        min_values: Some(1),
        options: Some(options),
        placeholder: Some(placeholder.to_owned()),
        required: None,
    })
}

/// At most `max` characters, with an ellipsis when cut.
fn clip(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_owned();
    }
    let mut out: String = text.chars().take(max - 1).collect();
    out.push('…');
    out
}

/// `gb:s:<host>` signs, `gb:p:<host>:<page>:<seen>` turns the page, `gb:d:...` (same tail)
/// deletes a note and `gb:b:...` blocks (`b<id>`) or unblocks (`u<id>`) a signer.
pub(crate) async fn on_component(cx: &Cx, mut args: Split<'_, char>, value: Option<&str>) -> anyhow::Result<()> {
    let action = args.next().unwrap_or_default();
    let host: u64 = args.next().and_then(|v| v.parse().ok()).unwrap_or(0);
    let page_no: i64 = args.next().and_then(|v| v.parse().ok()).unwrap_or(0);
    let seen: Option<i64> = args.next().and_then(|v| v.parse().ok());
    let clicker = cx.author()?.id.get();
    if action == "s" {
        return sign_button(cx, host, clicker).await;
    }
    // The page keeps the title it was opened with.
    let title = cx
        .interaction
        .message
        .as_ref()
        .and_then(|m| m.embeds.first())
        .and_then(|e| e.title.clone())
        .unwrap_or_else(|| "📖 Guestbook".to_owned());
    if action != "p" && clicker != host {
        return cx.reply_ephemeral("Only the owner can change their guestbook.").await;
    }
    let note = match (action, value) {
        ("d", Some(id)) => match id.parse() {
            Ok(id) => match cx.app.db.delete_note(host, id).await? {
                Some(n) => format!("Deleted the note from **{}**.", plain(&n.author_name)),
                None => "That note was already gone.".to_owned(),
            },
            Err(_) => String::new(),
        },
        ("b", Some(choice)) => block_choice(cx, host, page_no, choice).await?,
        _ => String::new(),
    };
    let data = page(cx, host, title, page_no, seen, &note).await?;
    cx.update(data).await
}

/// Blocks (`b<id>`) or unblocks (`u<id>`) a signer from the owner's menu.
async fn block_choice(cx: &Cx, host: u64, page_no: i64, choice: &str) -> anyhow::Result<String> {
    let Some(signer) = choice.get(1..).and_then(|id| id.parse::<u64>().ok()) else {
        return Ok(String::new());
    };
    let db = &cx.app.db;
    if choice.starts_with('u') {
        let name = db
            .guestbook_blocks(host)
            .await?
            .into_iter()
            .find(|(id, _)| *id as u64 == signer)
            .map_or_else(|| "them".to_owned(), |(_, name)| name);
        db.unblock_signer(host, signer).await?;
        return Ok(format!("Unblocked **{}**: they can sign again.", plain(&name)));
    }
    let (notes, _): (Vec<NoteRow>, i64) = db.guestbook(host, page_no * PER_PAGE, PER_PAGE).await?;
    let name = notes
        .iter()
        .find(|n| n.author_id as u64 == signer)
        .map_or_else(|| "this visitor".to_owned(), |n| n.author_name.clone());
    let removed = db.block_signer(host, signer, &name).await?;
    Ok(format!(
        "Blocked **{}**: {removed} note{} removed, and they can't sign again.",
        plain(&name),
        if removed == 1 { "" } else { "s" }
    ))
}

fn refusal(outcome: SignOutcome) -> &'static str {
    match outcome {
        SignOutcome::Signed => "Signed!",
        SignOutcome::AlreadyToday => "You already signed this guestbook today. Come back tomorrow!",
        SignOutcome::Locked => "This house is locked, so its guestbook is closed.",
        SignOutcome::Blocked => "The owner of this house has blocked you from signing their guestbook.",
        SignOutcome::NoVisit => "Visit the house first with `/house visit`, then sign its guestbook.",
    }
}

/// "Sign guestbook" on a visit: opens the note form, unless the rules say no.
async fn sign_button(cx: &Cx, host: u64, clicker: u64) -> anyhow::Result<()> {
    if clicker == host {
        return cx
            .reply_ephemeral("This is your own guestbook. Read it with `/house guestbook`.")
            .await;
    }
    let today = Utc::now().date_naive();
    if let Some(why) = cx.app.db.guestbook_refusal(host, clicker, today).await? {
        return cx.reply_ephemeral(refusal(why)).await;
    }
    cx.modal(format!("gb:n:{host}"), "Sign the guestbook", vec![note_field()])
        .await
}

#[allow(deprecated)] // TextInput::label: the Label around it carries the label now
fn note_field() -> Component {
    Component::Label(Label {
        component: Box::new(Component::TextInput(TextInput {
            custom_id: NOTE_FIELD.to_owned(),
            id: None,
            label: None,
            max_length: Some(NOTE_MAX as u16),
            min_length: Some(1),
            placeholder: Some("Lovely room! The aquarium is my favourite.".to_owned()),
            required: Some(true),
            style: TextInputStyle::Paragraph,
            value: None,
        })),
        description: Some(format!(
            "Up to {NOTE_MAX} characters, plain text. Links and mentions are left out."
        )),
        id: None,
        label: "Your note".to_owned(),
    })
}

/// The note form, submitted: `gb:n:<host>`.
pub(crate) async fn on_modal(cx: &Cx, mut args: Split<'_, char>, fields: &[(String, String)]) -> anyhow::Result<()> {
    if args.next() != Some("n") {
        return cx.reply_ephemeral("This form has expired.").await;
    }
    let host: u64 = args.next().and_then(|v| v.parse().ok()).unwrap_or(0);
    let author = cx.author()?.id.get();
    if author == host {
        return cx.reply_ephemeral("You can't sign your own guestbook.").await;
    }
    let raw = fields
        .iter()
        .find(|(id, _)| id == NOTE_FIELD)
        .map_or("", |(_, v)| v.as_str());
    let body = clean_note(raw);
    if body.is_empty() {
        return cx
            .reply_ephemeral("Your note was empty once links and mentions were left out. Try a few words!")
            .await;
    }
    let today = Utc::now().date_naive();
    let name = cx.author_display_name();
    match cx.app.db.sign_guestbook(host, author, &name, &body, today).await? {
        SignOutcome::Signed => {
            cx.reply_ephemeral(format!(
                "✍️ Signed! Your note is in their guestbook:\n> {}",
                plain(&body)
            ))
            .await
        }
        other => cx.reply_ephemeral(refusal(other)).await,
    }
}

/// A note as it is stored: one line without mentions or links, at most [`NOTE_MAX`] characters.
/// It is shown through [`plain`], so markdown shows as typed.
pub(crate) fn clean_note(raw: &str) -> String {
    let mut text = raw.to_owned();
    // Taking one thing out can make another (`@every<@1>one`), so repeat until nothing changes.
    loop {
        let next = strip_everyone(&strip_mentions(&text));
        if next == text {
            break;
        }
        text = next;
    }
    let words: Vec<&str> = text.split_whitespace().filter(|w| !is_link(w)).collect();
    let text: String = words.join(" ").chars().take(NOTE_MAX).collect();
    text.trim().to_owned()
}

/// Text shown as typed: Discord markdown characters escaped.
pub(crate) fn plain(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for (i, c) in text.chars().enumerate() {
        if matches!(c, '\\' | '*' | '_' | '~' | '`' | '|' | '[' | ']' | '<' | '>' | '#') || (i == 0 && c == '-') {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

/// Removes user, role, channel and command mentions (`<@1>`, `<@!1>`, `<@&1>`, `<#1>`, `</a b:1>`).
fn strip_mentions(text: &str) -> String {
    let digits = |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit());
    let is_mention = |inner: &str| {
        if let Some(id) = ["@&", "@!", "@", "#"].iter().find_map(|p| inner.strip_prefix(p)) {
            return digits(id);
        }
        inner
            .strip_prefix('/')
            .and_then(|cmd| cmd.rsplit_once(':'))
            .is_some_and(|(_, id)| digits(id))
    };
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(open) = rest.find('<') {
        out.push_str(&rest[..open]);
        let after = &rest[open + 1..];
        match after.find('>') {
            Some(close) if is_mention(&after[..close]) => rest = &after[close + 1..],
            _ => {
                out.push('<');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

/// Removes `@everyone` and `@here`, in any case.
fn strip_everyone(text: &str) -> String {
    let mut out = text.to_owned();
    for word in ["@everyone", "@here"] {
        // ASCII lowercasing keeps byte offsets, so positions carry over to the original.
        while let Some(at) = out.to_ascii_lowercase().find(word) {
            out.replace_range(at..at + word.len(), "");
        }
    }
    out
}

/// Whether a word is or contains a web address or an invite.
fn is_link(word: &str) -> bool {
    const TLDS: [&str; 34] = [
        "com", "net", "org", "io", "gg", "co", "me", "ly", "xyz", "app", "dev", "tv", "info", "biz", "us", "uk", "de",
        "ru", "fr", "link", "site", "online", "shop", "store", "live", "club", "fun", "top", "gl", "tk", "ml", "ga",
        "cf", "to",
    ];
    let lower = word.to_ascii_lowercase();
    if lower.contains("://") || lower.starts_with("www.") || lower.contains("discord.gg") {
        return true;
    }
    let host = lower.split('/').next().unwrap_or_default();
    let host = host.trim_end_matches(|c: char| !c.is_ascii_alphanumeric());
    match host.rsplit_once('.') {
        Some((name, tld)) => {
            !name.is_empty()
                && !name.ends_with('.')
                && TLDS.contains(&tld)
                && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-')
        }
        None => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn notes_lose_mentions_and_pings() {
        assert_eq!(clean_note("hi <@123> and <@!45> in <#678>"), "hi and in");
        assert_eq!(clean_note("<@&99> team"), "team");
        assert_eq!(clean_note("try </house visit:123456>"), "try");
        assert_eq!(clean_note("@everyone look"), "look");
        assert_eq!(clean_note("@HeRe!"), "!");
        assert_eq!(clean_note("@every<@1>one sneaky"), "sneaky");
        assert_eq!(clean_note("a <3 b"), "a <3 b", "a heart is not a mention");
    }

    #[test]
    fn notes_lose_links() {
        assert_eq!(clean_note("see https://spam.example/x now"), "see now");
        assert_eq!(clean_note("join discord.gg/abc or www.site.org"), "join or");
        assert_eq!(clean_note("cool bit.ly/xyz and example.com."), "cool and");
        assert_eq!(
            clean_note("e.g. a cozy room.Love it"),
            "e.g. a cozy room.Love it",
            "not every dot is a link"
        );
    }

    #[test]
    fn notes_show_markdown_as_typed() {
        assert_eq!(plain("**bold** _it_ `code`"), "\\*\\*bold\\*\\* \\_it\\_ \\`code\\`");
        assert_eq!(plain("- dash first, then - not"), "\\- dash first, then - not");
        assert_eq!(plain("[click](here) a <3"), "\\[click\\](here) a \\<3");
        assert_eq!(plain(&clean_note("# big\n> quote\n- list")), "\\# big \\> quote - list");
    }

    #[test]
    fn notes_are_one_short_line() {
        assert_eq!(clean_note("  lots   of\n\nspace "), "lots of space");
        assert_eq!(clean_note("   "), "");
        let long = "a".repeat(400);
        assert_eq!(clean_note(&long).chars().count(), NOTE_MAX);
        assert_eq!(clean_note("ünïcödé 🐚"), "ünïcödé 🐚");
    }

    #[test]
    fn clips_menu_labels() {
        assert_eq!(clip("short", 10), "short");
        assert_eq!(clip("abcdefghijk", 5), "abcd…");
        assert_eq!(clip(&"é".repeat(120), 100).chars().count(), 100);
    }
}
