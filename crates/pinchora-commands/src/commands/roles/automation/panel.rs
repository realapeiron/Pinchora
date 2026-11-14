//! Role panels: how they read as messages in each style (buttons, a menu, reactions), posting them, and
//! what a click or a pick on one does.

mod message;
pub(crate) use message::{KeyEmoji, emoji_key, request_type, rules_of, style_of};
use message::{panel_message, parse_button};
mod posting;
pub use posting::{PostRefusal, PostedPanel, post_panel};
mod picking;
pub(crate) use picking::{apply, current_roles, on_select, on_toggle};

use std::collections::HashMap;

use pinchora_core::infra::i18n::Lang;
use pinchora_core::roles::automation::Changes;
use pinchora_core::roles::automation::PanelRules;
use pinchora_core::roles::automation::PanelStyle;
use pinchora_core::roles::automation::Refusal;
use pinchora_core::roles::automation::choose;
use pinchora_core::roles::automation::format_span;
use pinchora_core::roles::automation::toggle;
use pinchora_core::t;
use pinchora_db::{PanelRoleRow, PanelRow};
use twilight_http::{Error, request::channel::reaction::RequestReactionType};
use twilight_model::{
    channel::message::{
        Component, Embed, EmojiReactionType,
        component::{ActionRow, Button, ButtonStyle, SelectMenu, SelectMenuOption, SelectMenuType},
    },
    id::{Id, marker::ChannelMarker},
};
use twilight_util::builder::embed::{EmbedBuilder, EmbedFooterBuilder};

use crate::App;
use crate::Cx;
use crate::commands::roles::automation::give;
use crate::commands::roles::role_panels::parse_emoji;
use crate::infra::util::BRAND;
use crate::roles::roles;

const UNKNOWN_CHANNEL: u64 = 10003;
const UNKNOWN_MESSAGE: u64 = 10008;
const MISSING_ACCESS: u64 = 50001;
const MISSING_PERMISSIONS: u64 = 50013;

/// Buttons per action row, which is also Discord's limit.
const ROW_WIDTH: usize = 5;
/// Discord's limit for an embed's description.
const DESCRIPTION_MAX: usize = 4_000;

#[cfg(test)]
mod tests {
    use super::message::{button_id, entry_line};
    use crate::commands::roles::automation::panel::*;

    fn panel(style: &str) -> PanelRow {
        PanelRow {
            id: 7,
            title: "Colours".to_owned(),
            description: Some("Pick one".to_owned()),
            channel_id: None,
            message_id: None,
            style: style.to_owned(),
            exclusive: false,
            required_role: None,
            min_picks: 0,
            max_picks: 0,
        }
    }

    fn entries(n: i64) -> Vec<PanelRoleRow> {
        (1..=n)
            .map(|n| PanelRoleRow {
                role_id: 1_000 + n,
                label: format!("Role {n}"),
                emoji: (n == 1).then(|| "🎮".to_owned()),
                description: (n == 2).then(|| "The second one".to_owned()),
                emoji_id: None,
                temp_secs: None,
            })
            .collect()
    }

    #[test]
    fn button_ids_round_trip() {
        let plain = panel("buttons");
        let id = button_id(&plain, 345_678_901_234_567_890);
        let mut parts = id.split(':');
        assert_eq!(parts.next(), Some("rr"));
        assert_eq!(parse_button(parts), Some((7, 345_678_901_234_567_890)));
        assert!(id.len() <= 100, "Discord's custom id limit");
        // A panel with rules sends the newer prefix; both read the same.
        let strict = PanelRow {
            exclusive: true,
            ..plain
        };
        let id = button_id(&strict, 5);
        let mut parts = id.split(':');
        assert_eq!(parts.next(), Some("rb"));
        assert_eq!(parse_button(parts), Some((7, 5)));
    }

    #[test]
    fn bad_button_ids_are_refused() {
        assert_eq!(parse_button("1".split(':')), None, "no role");
        assert_eq!(parse_button("x:2".split(':')), None, "panel is not a number");
        assert_eq!(parse_button("1:-2".split(':')), None, "role is not an id");
        assert_eq!(parse_button("1:2:3".split(':')), None, "trailing parts");
        assert_eq!(parse_button("".split(':')), None);
    }

    #[test]
    fn a_panel_becomes_rows_of_five_buttons() {
        let (embed, rows) = panel_message(&panel("buttons"), &entries(12), Lang::En);
        assert_eq!(embed.title.as_deref(), Some("Colours"));
        let description = embed.description.unwrap();
        assert!(
            description.starts_with("Pick one\n\n🎮 **Role 1** · <@&1001>"),
            "{description}"
        );
        assert!(
            description.contains("**Role 2** · <@&1002> — The second one"),
            "{description}"
        );
        assert_eq!(
            embed.footer.map(|f| f.text).as_deref(),
            Some("Click a button to get a role; click it again to drop it")
        );
        let widths: Vec<usize> = rows
            .iter()
            .map(|row| match row {
                Component::ActionRow(r) => r.components.len(),
                _ => 0,
            })
            .collect();
        assert_eq!(widths, [5, 5, 2]);
        let Component::ActionRow(first) = &rows[0] else {
            panic!("not a row");
        };
        let Component::Button(button) = &first.components[0] else {
            panic!("not a button");
        };
        assert_eq!(button.custom_id.as_deref(), Some("rr:7:1001"));
        assert!(button.emoji.is_some());
    }

    #[test]
    fn a_menu_panel_is_one_select_with_the_panels_limits() {
        let strict = PanelRow {
            min_picks: 1,
            max_picks: 3,
            required_role: Some(55),
            ..panel("select")
        };
        let (embed, rows) = panel_message(&strict, &entries(6), Lang::En);
        let [Component::ActionRow(row)] = &rows[..] else {
            panic!("one row, got {}", rows.len());
        };
        let [Component::SelectMenu(menu)] = &row.components[..] else {
            panic!("one menu");
        };
        assert_eq!(menu.custom_id, "rs:7");
        assert_eq!((menu.min_values, menu.max_values), (Some(1), Some(3)));
        let options = menu.options.as_ref().unwrap();
        assert_eq!(options.len(), 6);
        assert_eq!(options[0].value, "1001");
        assert!(options[0].emoji.is_some());
        assert_eq!(options[1].description.as_deref(), Some("The second one"));
        let description = embed.description.unwrap();
        assert!(
            !description.contains("The second one"),
            "a menu shows the line itself: {description}"
        );
        assert!(description.ends_with("Needs <@&55>"), "{description}");
        // Exclusive means one at a time.
        let exclusive = PanelRow {
            exclusive: true,
            ..panel("select")
        };
        let (_, rows) = panel_message(&exclusive, &entries(4), Lang::En);
        let Component::ActionRow(row) = &rows[0] else {
            panic!("a row")
        };
        let Component::SelectMenu(menu) = &row.components[0] else {
            panic!("a menu")
        };
        assert_eq!((menu.min_values, menu.max_values), (Some(0), Some(1)));
    }

    #[test]
    fn a_reactions_panel_has_no_components_and_says_what_each_emoji_gives() {
        let (embed, rows) = panel_message(&panel("reactions"), &entries(3), Lang::En);
        assert!(rows.is_empty());
        assert!(embed.footer.unwrap().text.starts_with("React with an emoji"));
        let timed = PanelRoleRow {
            temp_secs: Some(3_600),
            ..entries(1).remove(0)
        };
        assert!(entry_line(&timed, PanelStyle::Reactions).ends_with("· ⏳ 1h"));
    }

    #[test]
    fn a_huge_panel_still_fits_an_embed() {
        let long = PanelRoleRow {
            role_id: 1,
            label: "L".repeat(80),
            emoji: None,
            description: Some("d".repeat(100)),
            emoji_id: None,
            temp_secs: None,
        };
        let many = vec![long; 25];
        let (embed, _) = panel_message(&panel("reactions"), &many, Lang::En);
        assert!(embed.description.unwrap().chars().count() <= 4_096);
    }

    #[test]
    fn emojis_are_known_by_a_key() {
        let heart = EmojiReactionType::Unicode {
            name: "❤️".to_owned()
        };
        let bare = EmojiReactionType::Unicode { name: "❤".to_owned() };
        assert_eq!(emoji_key(&heart), emoji_key(&bare));
        let custom = EmojiReactionType::Custom {
            animated: false,
            id: Id::new(42),
            name: Some("blob".to_owned()),
        };
        assert_eq!(emoji_key(&custom), "c:42");
        assert!(matches!(KeyEmoji::parse("c:42"), Some(KeyEmoji::Custom(42))));
        assert!(
            KeyEmoji::parse("c:x").is_none() && KeyEmoji::parse("zz").is_none() && KeyEmoji::parse("c:0").is_none()
        );
        let game = KeyEmoji::parse("u:🎮").expect("a unicode key");
        assert!(matches!(game.request(), RequestReactionType::Unicode { name } if name == "🎮"));
    }
}
