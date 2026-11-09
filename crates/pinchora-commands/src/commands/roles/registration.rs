//! Domain-owned command registration. Regenerate compatibility tables with `cargo xtask registrations`.

use crate::commands::registration::{commands, compatibility};

commands! {
    typed {
        43 => crate::commands::roles::role_panels::RolesCommand,
        44 => crate::commands::roles::automation::AutoRolesCommand,
        45 => crate::commands::roles::automation::ColorCommand,
    }
    context {
    }
}

compatibility! {
    components {
        49 { Some("rr") => role_panels::on_toggle(cx, parts).await, }
        50 { Some("rb") => role_panels::on_toggle(cx, parts).await, }
        51 { Some("rs") => role_panels::on_select(cx, parts, values).await, }
        52 { Some("cr") => automation::on_color_component(cx, parts, values).await, }
    }
    owners {
        58 { "rr" | "rb" | "rs" => "roles post", }
        59 { "cr" => "color pick", }
    }
    private {
        13 { "autoroles", }
        21 { "color", }
        83 { "roles", }
    }
    help_community {
        9 { "roles", }
        18 { "autoroles", }
        19 { "color", }
    }
    routes {
        29 { ("autoroles add", "setup roles auto-add", 5) }
        30 { ("autoroles list", "setup roles auto-list", 5) }
        31 { ("autoroles remove", "setup roles auto-remove", 5) }
        32 { ("autoroles sticky", "setup roles auto-sticky", 5) }
        58 { ("color list", "color list", 0) }
        59 { ("color pick", "color pick", 0) }
        60 { ("color remove", "color remove", 0) }
        61 { ("color set", "color set", 0) }
        62 { ("color setup", "setup roles color-setup", 5) }
        346 { ("roles add", "setup roles panel-add", 5) }
        347 { ("roles panel", "setup roles panel-create", 5) }
        348 { ("roles post", "setup roles panel-post", 5) }
        349 { ("roles remove", "setup roles panel-remove", 5) }
        350 { ("roles temp", "setup roles panel-temp", 5) }
    }
    legacy_leaves {
        29 { "autoroles add" }
        30 { "autoroles list" }
        31 { "autoroles remove" }
        32 { "autoroles sticky" }
        58 { "color list" }
        59 { "color pick" }
        60 { "color remove" }
        61 { "color set" }
        62 { "color setup" }
        346 { "roles add" }
        347 { "roles panel" }
        348 { "roles post" }
        349 { "roles remove" }
        350 { "roles temp" }
    }
    command_gates {
        900 { "roles" => Self::RolePanels, }
        1100 { "autoroles" => Self::AutoRoles, }
        1200 { "color" => Self::ColorRoles, }
    }
    component_gates {
        1700 { "rr" => Self::RolePanels, }
        1701 { "rb" => Self::RolePanels, }
        1702 { "rs" => Self::RolePanels, }
        1800 { "cr" => Self::ColorRoles, }
    }
}
