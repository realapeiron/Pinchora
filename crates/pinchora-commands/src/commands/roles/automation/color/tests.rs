    use crate::commands::roles::automation::color::*;

    #[test]
    fn palette_names_are_localized() {
        assert_eq!(localized_palette_name(Lang::Tr, "Coral"), "Mercan");
        assert_eq!(localized_palette_name(Lang::Tr, "Sky"), "Gökyüzü");
    }

    #[test]
    fn role_mentions_and_bare_ids_are_read() {
        let text = "<@&123456789012345678>, 223456789012345678 <@&123456789012345678> 12 <@&0> x";
        assert_eq!(role_ids(text, 10), [123_456_789_012_345_678, 223_456_789_012_345_678]);
        assert_eq!(role_ids("", 10), Vec::<u64>::new());
        let many: String = (1..=12u64)
            .map(|n| format!("<@&{}> ", 100_000_000_000_000_000 + n))
            .collect();
        assert_eq!(role_ids(&many, 10).len(), 10);
    }

    #[test]
    fn the_menu_offers_the_palette_then_the_servers_own_colours() {
        let own = ColorRoleRow {
            role_id: 5,
            hex: "#123456".to_owned(),
            name: "Midnight".to_owned(),
        };
        let in_palette = ColorRoleRow {
            role_id: 6,
            hex: palette()[0].hex.clone(),
            name: "Renamed".to_owned(),
        };
        let offered = choices(&[own, in_palette]);
        assert_eq!(
            offered.len(),
            palette().len() + 1,
            "a palette colour is not listed twice"
        );
        assert_eq!(offered[0].name, palette()[0].name);
        assert_eq!(offered.last().map(|c| c.name.as_str()), Some("Midnight"));
        assert!(offered.len() <= MENU * MENUS);
    }

    #[test]
    fn controls_split_into_menus_of_25_and_mark_the_current_colour() {
        let offered: Vec<Choice> = (0..30u32)
            .map(|i| Choice {
                name: format!("C{i}"),
                hex: hex_of(0x10_0000 + i),
                rgb: 0x10_0000 + i,
            })
            .collect();
        let rows = controls(&offered, Some(27), Lang::En);
        assert_eq!(rows.len(), 3, "two menus and the button");
        let menu = |row: &Component| match row {
            Component::ActionRow(r) => match &r.components[0] {
                Component::SelectMenu(m) => Some(m.clone()),
                _ => None,
            },
            _ => None,
        };
        let (first, second) = (menu(&rows[0]).unwrap(), menu(&rows[1]).unwrap());
        assert_eq!(first.custom_id, "cr:s:0");
        assert_eq!(second.custom_id, "cr:s:1");
        assert_eq!(first.options.as_ref().unwrap().len(), 25);
        assert_eq!(second.options.as_ref().unwrap().len(), 5);
        assert!(
            second.options.as_ref().unwrap()[2].default,
            "the member's colour is marked"
        );
        assert!(first.options.as_ref().unwrap().iter().all(|o| !o.default));
        assert!(first.options.as_ref().unwrap()[0].value.len() == 6);
    }
