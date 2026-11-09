use super::*;

// /Embeds

/// A Discord embed, as far as a server can write one: text, a colour and two pictures.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EmbedSpec {
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    /// Makes the title a link.
    #[serde(default)]
    pub url: Option<String>,
    /// `#rrggbb`.
    #[serde(default)]
    pub color: Option<String>,
    #[serde(default)]
    pub author: Option<String>,
    #[serde(default)]
    pub footer: Option<String>,
    #[serde(default)]
    pub image: Option<String>,
    #[serde(default)]
    pub thumbnail: Option<String>,
    #[serde(default)]
    pub fields: Vec<EmbedFieldSpec>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EmbedFieldSpec {
    pub name: String,
    pub value: String,
    #[serde(default)]
    pub inline: bool,
}

impl EmbedSpec {
    /// Every character the embed shows, counted the way Discord counts its 6,000.
    pub fn total_chars(&self) -> usize {
        let text = |t: &Option<String>| t.as_deref().map_or(0, |t| t.chars().count());
        text(&self.title)
            + text(&self.description)
            + text(&self.author)
            + text(&self.footer)
            + self
                .fields
                .iter()
                .map(|f| f.name.chars().count() + f.value.chars().count())
                .sum::<usize>()
    }

    /// Whether there is anything to show.
    pub fn is_blank(&self) -> bool {
        let blank = |t: &Option<String>| t.as_deref().is_none_or(|t| t.trim().is_empty());
        blank(&self.title)
            && blank(&self.description)
            && blank(&self.image)
            && blank(&self.thumbnail)
            && blank(&self.footer)
            && blank(&self.author)
            && self.fields.is_empty()
    }

    /// The embed's colour as a number.
    pub fn color_value(&self) -> Option<u32> {
        self.color.as_deref().and_then(parse_hex_color)
    }

    /// Every text of the embed, for the message pipeline to fill.
    pub fn texts_mut(&mut self) -> Vec<&mut String> {
        let mut texts: Vec<&mut String> = Vec::new();
        texts.extend(
            [
                &mut self.title,
                &mut self.description,
                &mut self.author,
                &mut self.footer,
            ]
            .into_iter()
            .flatten(),
        );
        for field in &mut self.fields {
            texts.push(&mut field.name);
            texts.push(&mut field.value);
        }
        texts
    }
}

fn trimmed(text: &mut Option<String>) {
    if let Some(t) = text {
        let t2 = t.trim();
        *text = if t2.is_empty() { None } else { Some(t2.to_owned()) };
    }
}

fn check_len(errors: &mut Vec<FieldError>, text: &Option<String>, max: usize, what: &str) {
    if let Some(t) = text
        && t.chars().count() > max
    {
        errors.push(FieldError::new(
            "embed",
            format!("The embed's {what} is at most {max} characters."),
        ));
    }
}

fn check_url(errors: &mut Vec<FieldError>, url: &Option<String>, what: &str) {
    if let Some(u) = url
        && !(u.starts_with("https://")
            && u.len() > "https://".len()
            && u.len() <= URL_MAX
            && !u.contains(char::is_whitespace))
    {
        errors.push(FieldError::new(
            "embed",
            format!("The embed's {what} must be a web address starting with https:// (at most {URL_MAX} characters)."),
        ));
    }
}

/// An embed made tidy and checked; `Ok(None)` when it holds nothing at all.
pub fn clean_embed(embed: Option<EmbedSpec>) -> Result<Option<EmbedSpec>, Vec<FieldError>> {
    let Some(mut e) = embed else { return Ok(None) };
    for text in [
        &mut e.title,
        &mut e.description,
        &mut e.author,
        &mut e.footer,
        &mut e.url,
        &mut e.color,
    ] {
        trimmed(text);
    }
    trimmed(&mut e.image);
    trimmed(&mut e.thumbnail);
    let mut errors = Vec::new();
    check_len(&mut errors, &e.title, EMBED_TITLE_MAX, "title");
    check_len(&mut errors, &e.description, EMBED_DESCRIPTION_MAX, "description");
    check_len(&mut errors, &e.author, EMBED_AUTHOR_MAX, "author");
    check_len(&mut errors, &e.footer, EMBED_FOOTER_MAX, "footer");
    check_url(&mut errors, &e.url, "link");
    check_url(&mut errors, &e.image, "picture");
    check_url(&mut errors, &e.thumbnail, "thumbnail");
    if e.url.is_some() && e.title.is_none() {
        errors.push(FieldError::new("embed", "A link needs a title to sit on."));
    }
    if let Some(color) = &e.color {
        match parse_hex_color(color) {
            Some(value) => e.color = Some(format!("#{value:06x}")),
            None => errors.push(FieldError::new("embed", "The embed's colour looks like `#ff6b4a`.")),
        }
    }
    if e.fields.len() > EMBED_FIELDS_MAX {
        errors.push(FieldError::new(
            "embed",
            format!("An embed has at most {EMBED_FIELDS_MAX} fields."),
        ));
    }
    for (n, field) in e.fields.iter_mut().enumerate() {
        field.name = field.name.trim().to_owned();
        field.value = field.value.trim().to_owned();
        if field.name.is_empty() || field.value.is_empty() {
            errors.push(FieldError::new(
                "embed",
                format!("Field {} needs a name and a value.", n + 1),
            ));
        }
        if field.name.chars().count() > EMBED_FIELD_NAME_MAX || field.value.chars().count() > EMBED_FIELD_VALUE_MAX {
            errors.push(FieldError::new(
                "embed",
                format!(
                    "Field {} is too long: a name is at most {EMBED_FIELD_NAME_MAX} characters and a value {EMBED_FIELD_VALUE_MAX}.",
                    n + 1
                ),
            ));
        }
    }
    if e.total_chars() > EMBED_TOTAL_MAX {
        errors.push(FieldError::new(
            "embed",
            format!(
                "An embed holds at most {EMBED_TOTAL_MAX} characters in all; this one has {}.",
                e.total_chars()
            ),
        ));
    }
    if errors.is_empty() && e.is_blank() {
        errors.push(FieldError::new(
            "embed",
            "An embed needs a title, a description, a field or a picture. Remove it to send plain text.",
        ));
    }
    if errors.is_empty() { Ok(Some(e)) } else { Err(errors) }
}
