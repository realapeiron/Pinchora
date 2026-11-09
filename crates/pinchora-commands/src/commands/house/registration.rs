//! Domain-owned command registration. Regenerate compatibility tables with `cargo xtask registrations`.

use crate::commands::registration::{commands, compatibility};

commands! {
    typed {
        crate::commands::house::house::HouseCommand,
    }
    context {
    }
}

compatibility! {
    components {
        Some("he") => house::on_edit(cx, "he", parts, values.first().map(String::as_str)).await,
        Some(kind @ ("dw" | "df" | "dn")) => {
            decorate::on_component(cx, kind, parts, values.first().map(String::as_str)).await
        }
        Some("hv") => house::on_view_nav(cx, parts).await,
        Some("hu") => house::on_upgrade(cx, parts).await,
        Some("gb") => guestbook::on_component(cx, parts, values.first().map(String::as_str)).await,
        Some("rc") => contest::on_component(cx, parts).await,
    }
    modals {
        Some("gb") => guestbook::on_modal(cx, parts, fields).await,
    }
    owners {
        "he" => "house edit",
        "hv" => "house view",
        "hu" => "house upgrade",
        "dw" | "df" | "dn" => "house decorate",
        "gb" => "house guestbook",
        "rc" => "house contest",
    }

    command_gates {
        "house" => Self::House,
    }
    component_gates {
        "he" => Self::House,
        "hv" => Self::House,
        "hu" => Self::House,
        "dw" => Self::House,
        "df" => Self::House,
        "dn" => Self::House,
        "gb" => Self::House,
        "rc" => Self::House,
    }

}
