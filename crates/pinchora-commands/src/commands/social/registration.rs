//! Domain-owned command registration. Regenerate compatibility tables with `cargo xtask registrations`.

use crate::commands::registration::{commands, compatibility};

commands! {
    typed {
        17 => crate::commands::social::profile::Profile,
        27 => crate::commands::social::bio::Bio,
        32 => crate::commands::social::pet::PetCommand,
        39 => crate::commands::social::character::CharacterCommand,
        42 => crate::commands::social::customize::CustomizeCommand,
        48 => crate::commands::social::friends::FriendsCommand,
        49 => crate::commands::social::interact::Hug,
        50 => crate::commands::social::interact::Pat,
        51 => crate::commands::social::interact::Wave,
        52 => crate::commands::social::interact::HighFive,
    }
    context {
    }
}

compatibility! {
    components {
        28 { Some("pc") => pet::on_care(cx, parts).await, }
        29 { Some("pa") => pet::on_adopt(cx, values.first().map(String::as_str)).await, }
        30 { Some("ps") => pet::on_switch(cx, values.first().map(String::as_str)).await, }
        31 { Some("pn") => pet::on_nursery(cx, parts, values.first().map(String::as_str)).await, }
        32 { Some("py") => pet::on_trip(cx, parts, values.first().map(String::as_str)).await, }
        42 { Some("ch") => character::on_old(cx).await, }
        43 { Some("ca") => character::on_component(cx, parts, values).await, }
        45 { Some("cz") => customize::on_component(cx, parts, values).await, }
        46 { Some("ti") => customize::on_title(cx, parts, values).await, }
        47 { Some("cs") => customize::on_style(cx, parts, values).await, }
        48 { Some("sw") => customize::on_showcase(cx, parts, values).await, }
        68 { Some("fd") => friends::on_component(cx, parts, values.first().map(String::as_str)).await, }
        69 { Some("hg") => interact::on_component(cx, parts).await, }
    }
    modals {
        3 { Some("cs") => customize::on_style_modal(cx, parts, fields).await, }
        4 { Some("ca") => character::on_modal(cx, parts, fields).await, }
    }
    owners {
        15 { "pc" => "pet view", }
        16 { "pa" => "pet adopt", }
        17 { "ps" => "pet list", }
        18 { "pn" => "pet list", }
        19 { "py" => "pet trip", }
        61 { "fd" => "friends list", }
        62 { "hg" => "hug", }
        85 { "ch" | "ca" => "character", }
        86 { "cz" | "ti" | "cs" | "sw" => "customize", }
    }
    owner_actions {
        21 { ("hg", "pat") => Some("pat"), }
        22 { ("hg", "wave") => Some("wave"), }
        23 { ("hg", "highfive") => Some("highfive"), }
    }
    private {
        8 { "pet streak", }
        17 { "bio", }
        20 { "character", }
        28 { "customize", }
        40 { "friends remove", }
        60 { "pet adopt", }
        61 { "pet list", }
        62 { "pet rename", }
    }
    opens_modal {
        4 { "ca:hex", }
        5 { "ca:name", }
        9 { "cs:h", }
    }
    slash_only {
        4 { "character", }
        10 { "customize", }
    }
    help_friends {
        0 { "friends", }
        2 { "hug", }
        3 { "pat", }
        4 { "wave", }
        5 { "highfive", }
    }
    help_pets {
        0 { "pet", }
    }
    help_profile {
        0 { "bio", }
        1 { "character", }
        2 { "customize", }
    }
    help_start {
        0 { "profile", }
    }
    ephemeral {
        2 { "profile", }
    }
    routes {
        40 { ("bio", "customize bio", 10) }
        53 { ("character", "character", 0) }
        138 { ("customize background", "customize background", 0) }
        139 { ("customize badges", "customize badges", 0) }
        140 { ("customize showcase", "customize showcase", 0) }
        141 { ("customize style", "customize style", 0) }
        142 { ("customize title", "customize title", 0) }
        188 { ("friends add", "friends add", 0) }
        189 { ("friends list", "friends list", 0) }
        190 { ("friends remove", "friends remove", 0) }
        205 { ("highfive", "highfive", 0) }
        224 { ("hug", "hug", 0) }
        283 { ("pat", "pat", 0) }
        284 { ("pet adopt", "pet adopt", 0) }
        285 { ("pet capsule", "pet capsule", 0) }
        286 { ("pet list", "pet list", 0) }
        287 { ("pet rename", "pet rename", 0) }
        288 { ("pet streak", "pet streak", 0) }
        289 { ("pet trip claim", "pet trip claim", 0) }
        290 { ("pet trip go", "pet trip go", 0) }
        291 { ("pet view", "pet view", 0) }
        319 { ("profile", "profile", 0) }
        442 { ("wave", "wave", 0) }
    }
    legacy_leaves {
        40 { "bio" }
        53 { "character" }
        138 { "customize background" }
        139 { "customize badges" }
        140 { "customize showcase" }
        141 { "customize style" }
        142 { "customize title" }
        188 { "friends add" }
        189 { "friends list" }
        190 { "friends remove" }
        205 { "highfive" }
        224 { "hug" }
        283 { "pat" }
        284 { "pet adopt" }
        285 { "pet capsule" }
        286 { "pet list" }
        287 { "pet rename" }
        288 { "pet streak" }
        289 { "pet trip claim" }
        290 { "pet trip go" }
        291 { "pet view" }
        319 { "profile" }
        442 { "wave" }
    }
    command_gates {
        0 { "profile" => Self::Profile, }
        3 { "bio" => Self::Profile, }
        4 { "customize" => Self::Profile, }
        5 { "character" => Self::Profile, }
        400 { "pet" => Self::Pets, }
        1301 { "friends" => Self::Social, }
        1302 { "hug" => Self::Social, }
        1303 { "pat" => Self::Social, }
        1304 { "wave" => Self::Social, }
        1305 { "highfive" => Self::Social, }
    }
    component_gates {
        1 { "ch" => Self::Profile, }
        2 { "ca" => Self::Profile, }
        3 { "cz" => Self::Profile, }
        4 { "ti" => Self::Profile, }
        5 { "cs" => Self::Profile, }
        6 { "sw" => Self::Profile, }
        400 { "pc" => Self::Pets, }
        401 { "pa" => Self::Pets, }
        402 { "ps" => Self::Pets, }
        403 { "pn" => Self::Pets, }
        404 { "py" => Self::Pets, }
        1901 { "fd" => Self::Social, }
        1902 { "hg" => Self::Social, }
    }
    help_areas {
        1 { "profile", }
        5 { "pets", }
        7 { "friends", }
    }
    text_allowed {
        0 { "customize bio", }
    }
}
