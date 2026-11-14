use super::*;

// /English

/// What the English content says about an id.
pub(super) struct English<'a> {
    pub(super) name: Cow<'a, str>,
    pub(super) description: Option<&'a str>,
}

fn named<'a>(name: &'a str, description: Option<&'a str>) -> Option<English<'a>> {
    Some(English {
        name: Cow::Borrowed(name),
        description,
    })
}

/// The English name and description of any content id (a colour variant and `~bound` included).
pub(super) fn english<'a>(content: &'a Content, id: &str) -> Option<English<'a>> {
    if let Some(item) = content.item(id) {
        return Some(English {
            name: Cow::Owned(item.name()),
            description: Some(item.base.description.as_str()),
        });
    }
    let id = plain(id);
    if let Some(f) = content.furniture(id) {
        return named(&f.name, Some(&f.description));
    }
    if let Some(s) = content.sets.iter().find(|s| s.id == id) {
        return named(&s.name, Some(&s.description));
    }
    if let Some(s) = content.wallpaper(id).or_else(|| content.floor(id)) {
        return named(&s.name, None);
    }
    if let Some(r) = content.room_type(id) {
        return named(&r.name, Some(&r.description));
    }
    if let Some(p) = content.pet(id) {
        return named(&p.name, Some(&p.personality));
    }
    if let Some(f) = content.fish(id) {
        return named(&f.name, Some(&f.flavor));
    }
    if let Some(l) = content.look(id) {
        return named(&l.name, None);
    }
    if let Some(b) = content.background(id) {
        return named(&b.name, None);
    }
    if let Some(b) = content.badge(id) {
        return named(&b.name, Some(&b.description));
    }
    if let Some(t) = content.title(id) {
        return named(&t.name, Some(&t.description));
    }
    if let Some(s) = content.fishing.spot(id) {
        return named(&s.name, Some(&s.blurb));
    }
    if let Some(r) = content.fishing.rod(id) {
        return named(&r.name, Some(&r.blurb));
    }
    if let Some(b) = content.fishing.bait(id) {
        return named(&b.name, Some(&b.blurb));
    }
    if let Some(a) = character::accessory(id) {
        return named(&a.name, None);
    }
    if let Some(p) = character::preset(id) {
        return named(&p.name, None);
    }
    if let Some(e) = Collection::get().entry(id) {
        return named(&e.name, Some(&e.flavor));
    }
    if let Some(t) = crate::trips::get(id) {
        return named(&t.name, Some(&t.blurb));
    }
    if let Some(t) = contest::themes().iter().find(|t| t.id == id) {
        return named(&t.name, Some(&t.description));
    }
    None
}
