//! Domain-owned command registration. Regenerate compatibility tables with `cargo xtask registrations`.

use crate::commands::registration::{commands, compatibility};

commands! {
    typed {
        crate::commands::roles::role_panels::RolesCommand,
        crate::commands::roles::automation::AutoRolesCommand,
        crate::commands::roles::automation::ColorCommand,
    }
    context {
    }
}

compatibility! {
    components {
        Some("rr") => role_panels::on_toggle(cx, parts).await,
        Some("rb") => role_panels::on_toggle(cx, parts).await,
        Some("rs") => role_panels::on_select(cx, parts, values).await,
        Some("cr") => automation::on_color_component(cx, parts, values).await,
    }
    owners {
        "rr" | "rb" | "rs" => "roles post",
        "cr" => "color pick",
    }

    command_gates {
        "roles" => Self::RolePanels,
        "autoroles" => Self::AutoRoles,
        "color" => Self::ColorRoles,
    }
    component_gates {
        "rr" => Self::RolePanels,
        "rb" => Self::RolePanels,
        "rs" => Self::RolePanels,
        "cr" => Self::ColorRoles,
    }
}
