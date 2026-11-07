//! Pure Roblox bind and nickname rules shared by the bot and dashboard.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BindKind {
    Verified,
    GroupMember,
    GroupRank,
}

pub fn matches(
    kind: BindKind,
    group_id: Option<u64>,
    min: Option<u8>,
    max: Option<u8>,
    verified: bool,
    ranks: &[(u64, u8)],
) -> bool {
    match kind {
        BindKind::Verified => verified,
        BindKind::GroupMember => {
            verified && group_id.is_some_and(|id| ranks.iter().any(|(group, rank)| *group == id && *rank > 0))
        }
        BindKind::GroupRank => {
            verified
                && group_id.is_some_and(|id| {
                    ranks.iter().any(|(group, rank)| {
                        *group == id && min.is_some_and(|min| *rank >= min) && max.is_some_and(|max| *rank <= max)
                    })
                })
        }
    }
}

pub fn valid_template(template: &str) -> bool {
    if template.is_empty() || template.len() > 128 {
        return false;
    }
    let stripped = template.replace("{roblox-name}", "").replace("{discord-name}", "");
    !stripped.contains('{') && !stripped.contains('}')
}

pub fn nickname(template: &str, discord_name: &str, roblox_name: &str) -> String {
    use unicode_segmentation::UnicodeSegmentation;
    let expanded = template
        .replace("{roblox-name}", roblox_name)
        .replace("{discord-name}", discord_name);
    let mut out = String::new();
    let mut chars = 0;
    for grapheme in expanded.graphemes(true) {
        let width = grapheme.chars().count();
        if chars + width > 32 {
            break;
        }
        out.push_str(grapheme);
        chars += width;
    }
    out
}

pub fn description_has_code(description: &str, code: &str) -> bool {
    description
        .split(|c: char| !c.is_ascii_alphanumeric() && c != '-')
        .any(|word| word == code)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bind_table() {
        let ranks = [(7, 12), (8, 0)];
        assert!(matches(BindKind::Verified, None, None, None, true, &ranks));
        assert!(!matches(BindKind::Verified, None, None, None, false, &ranks));
        assert!(matches(BindKind::GroupMember, Some(7), None, None, true, &ranks));
        assert!(!matches(BindKind::GroupMember, Some(8), None, None, true, &ranks));
        assert!(matches(BindKind::GroupRank, Some(7), Some(10), Some(20), true, &ranks));
        assert!(!matches(BindKind::GroupRank, Some(7), Some(13), Some(20), true, &ranks));
    }
    #[test]
    fn code_is_token() {
        assert!(description_has_code("hello pinchora-crab-ab12!", "pinchora-crab-ab12"));
        assert!(!description_has_code("xxpinchora-crab-ab12yy", "pinchora-crab-ab12"));
    }
    #[test]
    fn nickname_is_bounded() {
        assert!(valid_template("{discord-name} ({roblox-name})"));
        assert!(!valid_template("{unknown}"));
        let name = nickname("{discord-name} ({roblox-name})", &"🦀".repeat(40), "Crab");
        assert_eq!(name.chars().count(), 32);
        let combined = nickname("{roblox-name}", "", &"a\u{301}".repeat(20));
        assert_eq!(combined, "a\u{301}".repeat(16));
        let family = "👩‍👩‍👧‍👦";
        let emoji = nickname("{roblox-name}", "", &family.repeat(5));
        assert_eq!(emoji, family.repeat(4));
    }
}
