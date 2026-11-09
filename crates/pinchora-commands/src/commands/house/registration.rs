//! Domain-owned command registration. Regenerate compatibility tables with `cargo xtask registrations`.

use crate::commands::registration::{commands, compatibility};

commands! {
    typed {
        29 => crate::commands::house::house::HouseCommand,
    }
    context {
    }
}

compatibility! {
    components {
        18 { Some("he") => house::on_edit(cx, "he", parts, values.first().map(String::as_str)).await, }
        20 { Some(kind @ ("dw" | "df" | "dn")) => {
            decorate::on_component(cx, kind, parts, values.first().map(String::as_str)).await
        } }
        22 { Some("hv") => house::on_view_nav(cx, parts).await, }
        23 { Some("hu") => house::on_upgrade(cx, parts).await, }
        53 { Some("gb") => guestbook::on_component(cx, parts, values.first().map(String::as_str)).await, }
        54 { Some("rc") => contest::on_component(cx, parts).await, }
    }
    modals {
        2 { Some("gb") => guestbook::on_modal(cx, parts, fields).await, }
    }
    owners {
        0 { "he" => "house edit", }
        1 { "hv" => "house view", }
        2 { "hu" => "house upgrade", }
        3 { "dw" | "df" | "dn" => "house decorate", }
        4 { "gb" => "house guestbook", }
        5 { "rc" => "house contest", }
    }
    private {
        5 { "house aquarium", }
        6 { "house garden", }
        7 { "house craft", }
        49 { "house contest vote", }
        52 { "house decorate", }
        53 { "house edit", }
        54 { "house lock", }
        55 { "house upgrade", }
    }
    opens_modal {
        6 { "gb:s", }
    }
    slash_only {
        11 { "house decorate", }
        12 { "house edit", }
    }
    help_house {
        0 { "house", }
    }
    routes {
        209 { ("house aquarium", "house aquarium", 0) }
        210 { ("house contest enter", "house contest enter", 0) }
        211 { ("house contest view", "house contest view", 0) }
        212 { ("house contest vote", "house contest vote", 0) }
        213 { ("house contest winners", "house contest winners", 0) }
        214 { ("house craft", "house craft", 0) }
        215 { ("house decorate", "house decorate", 0) }
        216 { ("house edit", "house edit", 0) }
        217 { ("house garden", "house garden", 0) }
        218 { ("house guestbook", "house guestbook", 0) }
        219 { ("house like", "house like", 0) }
        220 { ("house lock", "house lock", 0) }
        221 { ("house upgrade", "house upgrade", 0) }
        222 { ("house view", "house view", 0) }
        223 { ("house visit", "house visit", 0) }
    }
    legacy_leaves {
        209 { "house aquarium" }
        210 { "house contest enter" }
        211 { "house contest view" }
        212 { "house contest vote" }
        213 { "house contest winners" }
        214 { "house craft" }
        215 { "house decorate" }
        216 { "house edit" }
        217 { "house garden" }
        218 { "house guestbook" }
        219 { "house like" }
        220 { "house lock" }
        221 { "house upgrade" }
        222 { "house view" }
        223 { "house visit" }
    }
    command_gates {
        200 { "house" => Self::House, }
    }
    component_gates {
        100 { "he" => Self::House, }
        101 { "hv" => Self::House, }
        102 { "hu" => Self::House, }
        103 { "dw" => Self::House, }
        104 { "df" => Self::House, }
        105 { "dn" => Self::House, }
        106 { "gb" => Self::House, }
        107 { "rc" => Self::House, }
    }
    help_areas {
        4 { "house", }
    }
}
