//! Domain-owned command registration. Regenerate compatibility tables with `cargo xtask registrations`.

use crate::commands::registration::{commands, compatibility};

commands! {
    typed {
        crate::commands::integrations::roblox::RobloxCommand,
        crate::commands::integrations::roblox::RobloxAdmin,
    }
    context {
    }
}

compatibility! {
    components {
        Some("rx") => crate::commands::integrations::roblox::on_component(cx, parts).await,
    }
    owners {
        "rx" => "roblox verify",
    }
    owner_actions {
        ("rx", "check") => Some("roblox verify"),
    }

    command_gates {
        "roblox" => Self::Roblox,
        "robloxadmin" => Self::Roblox,
    }
    component_gates {
        "rx" => Self::Roblox,
        "rk" => Self::Roblox,
    }
    route_roots {
        (
            "integrations",
            "Configure feeds, counters and invite tracking",
            Some(Permissions::MANAGE_GUILD),
        ),
    }

}
