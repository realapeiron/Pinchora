//! Domain-owned command registration. Regenerate compatibility tables with `cargo xtask registrations`.

use crate::commands::registration::{commands, compatibility};

commands! {
    typed {
        104 => crate::commands::integrations::roblox::RobloxCommand,
        105 => crate::commands::integrations::roblox::RobloxAdmin,
    }
    context {
    }
}

compatibility! {
    components {
        2 { Some("rx") => crate::commands::integrations::roblox::on_component(cx, parts).await, }
    }
    owners {
        102 { "rx" => "roblox verify", }
    }
    owner_actions {
        0 { ("rx", "check") => Some("roblox verify"), }
    }
    private {
        78 { "roblox", }
        79 { "robloxadmin", }
    }
    slash_only {
        8 { "integrations", }
    }
    help_profile {
        5 { "roblox", }
        6 { "robloxadmin", }
    }
    admin_group_moves {
        10 { ("setup feeds", "integrations feeds"), }
        11 { ("setup stats", "integrations stats"), }
        12 { ("setup invites", "integrations invites"), }
    }
    routes {
        336 { ("roblox unverify", "roblox unverify", 18) }
        337 { ("roblox update", "roblox update", 18) }
        338 { ("roblox verify", "roblox verify", 18) }
        339 { ("roblox whois", "roblox whois", 18) }
        340 { ("robloxadmin bind-add", "integrations roblox bind-add", 18) }
        341 { ("robloxadmin bind-list", "integrations roblox bind-list", 18) }
        342 { ("robloxadmin bind-remove", "integrations roblox bind-remove", 18) }
        343 { ("robloxadmin settings", "integrations roblox settings", 18) }
    }
    legacy_leaves {
        336 { "roblox unverify" }
        337 { "roblox update" }
        338 { "roblox verify" }
        339 { "roblox whois" }
        340 { "robloxadmin bind-add" }
        341 { "robloxadmin bind-list" }
        342 { "robloxadmin bind-remove" }
        343 { "robloxadmin settings" }
    }
    command_gates {
        4800 { "roblox" => Self::Roblox, }
        4801 { "robloxadmin" => Self::Roblox, }
    }
    component_gates {
        4500 { "rx" => Self::Roblox, }
        4501 { "rk" => Self::Roblox, }
    }
    route_roots {
        3 { (
            "integrations",
            "Configure feeds, counters and invite tracking",
            Some(Permissions::MANAGE_GUILD),
        ), }
    }
    admin_roots {
        3 { "integrations" }
    }
}
