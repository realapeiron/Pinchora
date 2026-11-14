use super::*;

// /Scrubbing

/// Text a member typed (or a name), made safe to put into a message: mentions and broadcasts gone, and
/// markdown escaped so it shows as typed.
pub fn scrub_value(text: &str) -> String {
    escape_markdown(&strip_mentions(text))
}

/// Markdown escaped with backslashes, which Discord hides: a name or what a member typed cannot make a
/// masked link (`[Free Nitro](…)`), bold, a spoiler, code, a heading, a list or a quote of its own.
pub fn escape_markdown(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 8);
    for c in text.chars() {
        if ESCAPED.contains(&c) {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

const ESCAPED: [char; 14] = ['\\', '*', '_', '~', '`', '|', '>', '<', '[', ']', '(', ')', '#', '-'];

/// [`escape_markdown`] undone, for an embed's author and footer: Discord shows those as plain text,
/// backslashes and all, and nothing there can become a link or bold.
fn unescape_markdown(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match chars.peek() {
            Some(&next) if c == '\\' && ESCAPED.contains(&next) => {
                out.push(next);
                chars.next();
            }
            _ => out.push(c),
        }
    }
    out
}

/// `@everyone` and `@here` without their `@`, in any case; nothing else changes.
pub(super) fn defang(text: &str) -> String {
    let mut out = text.to_owned();
    for word in ["@everyone", "@here"] {
        // ASCII lowercasing keeps byte offsets, so positions carry over to the original.
        while let Some(at) = out.to_ascii_lowercase().find(word) {
            out.replace_range(at..=at, "");
        }
    }
    out
}

/// A template made safe: with mentions off, everything that could ping is dropped; with them on, still
/// never everyone or here.
pub(super) fn scrub_template(text: &str, allow_mentions: bool) -> String {
    if allow_mentions {
        defang(text)
    } else {
        strip_mentions(text)
    }
}

/// The users and roles a text mentions by `<@id>`, `<@!id>` and `<@&id>`.
pub fn mention_ids(text: &str) -> (Vec<u64>, Vec<u64>) {
    let (mut users, mut roles) = (Vec::new(), Vec::new());
    let mut rest = text;
    while let Some(at) = rest.find("<@") {
        let after = &rest[at + 2..];
        let (role, digits) = match after.strip_prefix('&') {
            Some(d) => (true, d),
            None => (false, after.strip_prefix('!').unwrap_or(after)),
        };
        let end = digits.find(|c: char| !c.is_ascii_digit()).unwrap_or(digits.len());
        if end > 0 && digits[end..].starts_with('>') {
            if let Ok(id) = digits[..end].parse::<u64>() {
                let list = if role { &mut roles } else { &mut users };
                if !list.contains(&id) {
                    list.push(id);
                }
            }
            rest = &digits[end + 1..];
        } else {
            rest = &rest[at + 2..];
        }
    }
    (users, roles)
}

// /Messages

/// What a command or responder says, before it is filled in.
pub struct Spec<'a> {
    pub responses: &'a [String],
    pub embed: Option<&'a EmbedSpec>,
    pub allow_mentions: bool,
}

/// A message after its placeholders were filled and its texts made safe.
#[derive(Debug, Clone, PartialEq)]
pub struct Rendered {
    pub content: String,
    pub embed: Option<EmbedSpec>,
    /// The users and roles the template itself names, which the message may ping when the server allows it.
    pub users: Vec<u64>,
    pub roles: Vec<u64>,
}

/// Every template of a spec, to see which placeholders are in use before they are looked up.
pub fn templates<'a>(spec: &'a Spec<'a>) -> Vec<&'a str> {
    let mut texts: Vec<&str> = spec.responses.iter().map(String::as_str).collect();
    if let Some(embed) = spec.embed {
        texts.extend(
            [&embed.title, &embed.description, &embed.author, &embed.footer]
                .into_iter()
                .flatten()
                .map(String::as_str),
        );
        for field in &embed.fields {
            texts.push(&field.name);
            texts.push(&field.value);
        }
    }
    texts
}

/// The texts of the direct messages among `actions`, which are filled like the reply is.
pub fn action_texts(actions: &[Action]) -> Vec<&str> {
    actions
        .iter()
        .filter_map(|a| match a {
            Action::Dm { text } => Some(text.as_str()),
            _ => None,
        })
        .collect()
}

/// Every template of a spec and of the direct messages among `actions`.
pub(crate) fn all_texts<'a>(spec: &'a Spec<'a>, actions: &'a [Action]) -> Vec<&'a str> {
    let mut texts = templates(spec);
    texts.extend(action_texts(actions));
    texts
}

/// Picks one of the replies and fills it, and the embed, with `vars`. `pick(n)` chooses an index below
/// `n` for the reply and for every `{random:…}`.
pub fn render_message(spec: &Spec<'_>, vars: &Vars, pick: &mut dyn FnMut(usize) -> usize) -> Rendered {
    let allow = spec.allow_mentions;
    let reply = spec
        .responses
        .get(pick(spec.responses.len().max(1)))
        .map_or("", String::as_str);
    let clean = scrub_template(reply, allow);
    let (users, roles) = if allow {
        mention_ids(&clean)
    } else {
        (Vec::new(), Vec::new())
    };
    let content = truncate(render(&clean, vars, pick).trim(), RESPONSE_MAX);
    let embed = spec.embed.map(|embed| {
        let mut embed = embed.clone();
        fill_embed(&mut embed, &mut |text, max| {
            let clean = scrub_template(text, allow);
            *text = truncate(render(&clean, vars, &mut *pick).trim(), max);
        });
        embed
    });
    Rendered {
        content,
        embed,
        users,
        roles,
    }
}

/// Fills every text of an embed with `fill(text, longest allowed)`, then trims the embed to Discord's
/// total if the filled texts ran over it.
fn fill_embed(embed: &mut EmbedSpec, fill: &mut dyn FnMut(&mut String, usize)) {
    if let Some(text) = &mut embed.title {
        fill(text, EMBED_TITLE_MAX);
    }
    if let Some(text) = &mut embed.description {
        fill(text, EMBED_DESCRIPTION_MAX);
    }
    if let Some(text) = &mut embed.author {
        fill(text, EMBED_AUTHOR_MAX);
        *text = unescape_markdown(text);
    }
    if let Some(text) = &mut embed.footer {
        fill(text, EMBED_FOOTER_MAX);
        *text = unescape_markdown(text);
    }
    for field in &mut embed.fields {
        fill(&mut field.name, EMBED_FIELD_NAME_MAX);
        fill(&mut field.value, EMBED_FIELD_VALUE_MAX);
    }
    // Filling can only grow a text to its own limit, but the limits together are above Discord's total.
    let over = embed.total_chars().saturating_sub(EMBED_TOTAL_MAX);
    if over > 0
        && let Some(text) = &mut embed.description
    {
        let keep = text.chars().count().saturating_sub(over);
        *text = truncate(text, keep.max(1));
    }
}

/// The rendered message as Discord takes it: the text, the embed, and who may be pinged (the member the
/// message is about, and when the server allows it the users and roles the template names; never everyone,
/// and never the server's own `@everyone` role).
pub(crate) fn to_discord(rendered: &Rendered, guild: u64, subject: u64) -> (String, Option<Embed>, AllowedMentions) {
    let mut users: Vec<Id<UserMarker>> = vec![Id::new(subject.max(1))];
    users.extend(
        rendered
            .users
            .iter()
            .filter(|u| **u != subject)
            .map(|u| Id::new((*u).max(1))),
    );
    let roles = rendered
        .roles
        .iter()
        .filter(|r| **r != guild)
        .map(|r| Id::new((*r).max(1)))
        .collect();
    let mentions = AllowedMentions {
        parse: Vec::new(),
        replied_user: false,
        roles,
        users,
    };
    (
        rendered.content.clone(),
        rendered.embed.as_ref().map(embed_of),
        mentions,
    )
}

/// An embed spec as a Discord embed.
pub fn embed_of(spec: &EmbedSpec) -> Embed {
    let mut builder = EmbedBuilder::new().color(spec.color_value().unwrap_or(BRAND));
    if let Some(title) = &spec.title {
        builder = builder.title(title);
    }
    if let Some(description) = &spec.description {
        builder = builder.description(description);
    }
    if let Some(url) = &spec.url {
        builder = builder.url(url);
    }
    if let Some(author) = &spec.author {
        builder = builder.author(EmbedAuthorBuilder::new(author));
    }
    if let Some(footer) = &spec.footer {
        builder = builder.footer(EmbedFooterBuilder::new(footer));
    }
    if let Some(source) = spec.image.as_deref().and_then(|u| ImageSource::url(u).ok()) {
        builder = builder.image(source);
    }
    if let Some(source) = spec.thumbnail.as_deref().and_then(|u| ImageSource::url(u).ok()) {
        builder = builder.thumbnail(source);
    }
    for field in &spec.fields {
        let mut f = EmbedFieldBuilder::new(&field.name, &field.value);
        if field.inline {
            f = f.inline();
        }
        builder = builder.field(f);
    }
    builder.build()
}
