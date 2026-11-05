//! Player characters: a look from `content/looks.json` plus four colours, saved as text.

use pinchora_render::character::{Colors, HAIR_COLORS, OUTFIT_COLORS, SKIN_TONES};

use crate::Content;

/// One thing the character editor changes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Field {
    Look,
    Skin,
    Hair,
    Top,
    Bottom,
}

impl Field {
    pub const ALL: [Field; 5] = [Self::Look, Self::Skin, Self::Hair, Self::Top, Self::Bottom];

    pub fn key(self) -> &'static str {
        match self {
            Self::Look => "look",
            Self::Skin => "skin",
            Self::Hair => "hair",
            Self::Top => "top",
            Self::Bottom => "bottom",
        }
    }

    pub fn from_key(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|f| f.key() == key)
    }
}

/// A designed character. Saved as `"<look>.<skin>.<hair>.<top>.<bottom>"`, e.g. `tidewalker.1.1.0.2`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Character {
    pub look: String,
    pub colors: Colors,
}

/// A digit string below `len`; signs, spaces and empty strings are refused.
fn index(text: &str, len: usize) -> Option<usize> {
    if text.is_empty() || !text.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    text.parse().ok().filter(|&n| n < len)
}

impl Character {
    /// Where the editor starts for a player without a saved character.
    pub fn starter(content: &Content) -> Self {
        Self {
            look: content.looks.first().map_or_else(String::new, |l| l.id.clone()),
            colors: Colors {
                skin: 1,
                hair: 1,
                top: 0,
                bottom: 2,
            },
        }
    }

    pub fn encode(&self) -> String {
        let c = self.colors;
        format!("{}.{}.{}.{}.{}", self.look, c.skin, c.hair, c.top, c.bottom)
    }

    /// Parses a saved code; unknown looks and out-of-range colours give `None`.
    pub fn decode(content: &Content, code: &str) -> Option<Self> {
        let mut parts = code.split('.');
        let look = content.look(parts.next()?)?.id.clone();
        let colors = Colors {
            skin: index(parts.next()?, SKIN_TONES.len())?,
            hair: index(parts.next()?, HAIR_COLORS.len())?,
            top: index(parts.next()?, OUTFIT_COLORS.len())?,
            bottom: index(parts.next()?, OUTFIT_COLORS.len())?,
        };
        parts.next().is_none().then_some(Self { look, colors })
    }

    /// This character with `field` set to `value` (a look id, or a colour index); `None`
    /// when the value is not valid for that field.
    pub fn with(&self, content: &Content, field: Field, value: &str) -> Option<Self> {
        let mut next = self.clone();
        match field {
            Field::Look => next.look = content.look(value)?.id.clone(),
            Field::Skin => next.colors.skin = index(value, SKIN_TONES.len())?,
            Field::Hair => next.colors.hair = index(value, HAIR_COLORS.len())?,
            Field::Top => next.colors.top = index(value, OUTFIT_COLORS.len())?,
            Field::Bottom => next.colors.bottom = index(value, OUTFIT_COLORS.len())?,
        }
        Some(next)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codes_round_trip() {
        let content = Content::get();
        let starter = Character::starter(content);
        assert!(content.look(&starter.look).is_some());
        assert_eq!(Character::decode(content, &starter.encode()), Some(starter.clone()));

        let last = content.looks.last().expect("looks exist").id.clone();
        let far = Character {
            look: last,
            colors: Colors {
                skin: SKIN_TONES.len() - 1,
                hair: HAIR_COLORS.len() - 1,
                top: OUTFIT_COLORS.len() - 1,
                bottom: OUTFIT_COLORS.len() - 1,
            },
        };
        assert_eq!(Character::decode(content, &far.encode()), Some(far));
        assert_eq!(starter.encode().split('.').count(), 5);
    }

    #[test]
    fn bad_codes_are_refused() {
        let content = Content::get();
        let look = &content.looks[0].id;
        for code in [
            "",
            "tidewalker",
            "nobody.0.0.0.0",
            &format!("{look}.8.0.0.0"),
            &format!("{look}.0.12.0.0"),
            &format!("{look}.0.0.12.0"),
            &format!("{look}.0.0.0.12"),
            &format!("{look}.0.0.0"),
            &format!("{look}.0.0.0.0.0"),
            &format!("{look}.-1.0.0.0"),
            &format!("{look}.+1.0.0.0"),
            &format!("{look}.a.0.0.0"),
            &format!("{look}..0.0.0"),
            &format!("{look}.0.0.0.0 "),
        ] {
            assert_eq!(Character::decode(content, code), None, "{code:?}");
        }
    }

    #[test]
    fn editing_one_field_keeps_the_rest() {
        let content = Content::get();
        let base = Character::starter(content);
        let other = &content.looks[1].id;
        let looked = base.with(content, Field::Look, other).expect("known look");
        assert_eq!(looked.look, *other);
        assert_eq!(looked.colors, base.colors);

        let hair = base.with(content, Field::Hair, "7").expect("in range");
        assert_eq!(hair.colors.hair, 7);
        assert_eq!((hair.colors.skin, hair.colors.top), (base.colors.skin, base.colors.top));

        assert!(base.with(content, Field::Look, "nobody").is_none());
        assert!(base.with(content, Field::Skin, "8").is_none());
        assert!(base.with(content, Field::Top, "x").is_none());
        assert_eq!(Field::from_key("bottom"), Some(Field::Bottom));
        assert_eq!(Field::from_key("shoes"), None);
        assert!(Field::ALL.iter().all(|f| Field::from_key(f.key()) == Some(*f)));
    }
}
