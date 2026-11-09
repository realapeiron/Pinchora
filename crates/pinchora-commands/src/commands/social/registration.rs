//! Domain-owned command registration. Regenerate compatibility tables with `cargo xtask registrations`.

use crate::commands::registration::{commands, compatibility};

commands! {
    typed {
        crate::commands::social::profile::Profile,
        crate::commands::social::bio::Bio,
        crate::commands::social::pet::PetCommand,
        crate::commands::social::character::CharacterCommand,
        crate::commands::social::customize::CustomizeCommand,
        crate::commands::social::friends::FriendsCommand,
        crate::commands::social::interact::Hug,
        crate::commands::social::interact::Pat,
        crate::commands::social::interact::Wave,
        crate::commands::social::interact::HighFive,
    }
    context {
    }
}

compatibility! {
    components {
        Some("pc") => pet::on_care(cx, parts).await,
        Some("pa") => pet::on_adopt(cx, values.first().map(String::as_str)).await,
        Some("ps") => pet::on_switch(cx, values.first().map(String::as_str)).await,
        Some("pn") => pet::on_nursery(cx, parts, values.first().map(String::as_str)).await,
        Some("py") => pet::on_trip(cx, parts, values.first().map(String::as_str)).await,
        Some("ch") => character::on_old(cx).await,
        Some("ca") => character::on_component(cx, parts, values).await,
        Some("cz") => customize::on_component(cx, parts, values).await,
        Some("ti") => customize::on_title(cx, parts, values).await,
        Some("cs") => customize::on_style(cx, parts, values).await,
        Some("sw") => customize::on_showcase(cx, parts, values).await,
        Some("fd") => friends::on_component(cx, parts, values.first().map(String::as_str)).await,
        Some("hg") => interact::on_component(cx, parts).await,
    }
    modals {
        Some("cs") => customize::on_style_modal(cx, parts, fields).await,
        Some("ca") => character::on_modal(cx, parts, fields).await,
    }
    owners {
        "pc" => "pet view",
        "pa" => "pet adopt",
        "ps" => "pet list",
        "pn" => "pet list",
        "py" => "pet trip",
        "fd" => "friends list",
        "hg" => "hug",
        "ch" | "ca" => "character",
        "cz" | "ti" | "cs" | "sw" => "customize",
    }
    owner_actions {
        ("hg", "pat") => Some("pat"),
        ("hg", "wave") => Some("wave"),
        ("hg", "highfive") => Some("highfive"),
    }

    command_gates {
        "profile" => Self::Profile,
        "bio" => Self::Profile,
        "customize" => Self::Profile,
        "character" => Self::Profile,
        "pet" => Self::Pets,
        "friends" => Self::Social,
        "hug" => Self::Social,
        "pat" => Self::Social,
        "wave" => Self::Social,
        "highfive" => Self::Social,
    }
    component_gates {
        "ch" => Self::Profile,
        "ca" => Self::Profile,
        "cz" => Self::Profile,
        "ti" => Self::Profile,
        "cs" => Self::Profile,
        "sw" => Self::Profile,
        "pc" => Self::Pets,
        "pa" => Self::Pets,
        "ps" => Self::Pets,
        "pn" => Self::Pets,
        "py" => Self::Pets,
        "fd" => Self::Social,
        "hg" => Self::Social,
    }

}
