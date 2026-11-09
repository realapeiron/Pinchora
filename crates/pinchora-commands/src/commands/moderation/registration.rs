//! Domain-owned command registration. Regenerate compatibility tables with `cargo xtask registrations`.

use crate::commands::registration::{commands, compatibility};

commands! {
    typed {
        1 => crate::commands::moderation::policy::Parity,
        66 => crate::commands::moderation::Ban,
        67 => crate::commands::moderation::Unban,
        68 => crate::commands::moderation::Kick,
        69 => crate::commands::moderation::Timeout,
        70 => crate::commands::moderation::Untimeout,
        71 => crate::commands::moderation::Warn,
        72 => crate::commands::moderation::Warnings,
        73 => crate::commands::moderation::Unwarn,
        74 => crate::commands::moderation::Purge,
        75 => crate::commands::moderation::Slowmode,
        76 => crate::commands::moderation::Lock,
        77 => crate::commands::moderation::Unlock,
        78 => crate::commands::moderation::Nick,
        79 => crate::commands::moderation::RoleCommand,
        80 => crate::commands::moderation::CaseCommand,
        81 => crate::commands::moderation::ReasonCommand,
        91 => crate::commands::moderation::automod::AutoMod,
        94 => crate::commands::moderation::verification::Verify,
        95 => crate::commands::moderation::verification::Verification,
        96 => crate::commands::moderation::verification::Raid,
        97 => crate::commands::moderation::anti_nuke::AntiNuke,
    }
    context {
        113 => ("Report", crate::commands::moderation::report_context),
    }
}

compatibility! {
    components {
        6 { Some("vf") => crate::moderation::verification::on_component(cx, "vf", parts).await, }
        7 { Some("cq") => crate::moderation::verification::on_component(cx, "cq", parts).await, }
        8 { Some("rd") => crate::moderation::verification::on_component(cx, "rd", parts).await, }
        9 { Some("an") => crate::commands::moderation::anti_nuke::on_component(cx, "an", parts).await, }
        10 { Some("vp") => crate::commands::moderation::anti_nuke::on_component(cx, "vp", parts).await, }
        12 { Some("am") => crate::commands::moderation::automod::on_component(cx, "am", parts, values).await, }
        13 { Some("ax") => crate::commands::moderation::automod::on_component(cx, "ax", parts, values).await, }
    }
    modals {
        1 { Some("cq") => crate::moderation::verification::on_modal(cx, parts, fields).await, }
    }
    owners {
        73 { "am" | "ax" => "automod", }
        74 { "vf" | "cq" => "verify", }
        75 { "rd" => "raid", }
        92 { "an" | "vp" => "antinuke", }
    }
    owner_actions {
        9 { ("ax", "r") => Some("automod exempt role"), }
        10 { ("ax", "c") => Some("automod exempt channel"), }
    }
    private {
        10 { "modtools", }
        11 { "Report", }
        19 { "case", }
        56 { "nick", }
        76 { "purge", }
        80 { "reason", }
        82 { "role", }
        101 { "warnings", }
        104 { "automod", }
        105 { "antinuke", }
        113 { "verify", }
        114 { "verification", }
        115 { "raid", }
    }
    opens_modal {
        20 { "cq:a", }
    }
    slash_only {
        6 { "safety", }
    }
    help_logsetup {
        0 { "antinuke", }
    }
    help_moderation {
        0 { "automod", }
        4 { "verify", }
        5 { "verification", }
        6 { "raid", }
        7 { "antinuke status", }
        8 { "antinuke events", }
        9 { "modtools", }
        10 { "Report", }
        11 { "ban", }
        12 { "unban", }
        13 { "kick", }
        14 { "timeout", }
        15 { "untimeout", }
        16 { "warn", }
        17 { "warnings", }
        18 { "unwarn", }
        19 { "purge", }
        20 { "slowmode", }
        21 { "lock", }
        22 { "unlock", }
        23 { "nick", }
        24 { "role", }
        25 { "case", }
        26 { "reason", }
    }
    admin_group_moves {
        0 { ("setup moderation", "safety moderation"), }
        1 { ("setup automod", "safety automod"), }
        2 { ("setup verification", "safety verification"), }
        3 { ("setup antinuke", "safety antinuke"), }
    }
    previous_actions {
        0 { ("mod ban", "ban"), }
        1 { ("mod kick", "kick"), }
        2 { ("mod warn", "warn"), }
        3 { ("mod timeout", "timeout"), }
        4 { ("mod purge", "purge"), }
    }
    routes {
        1 { ("Report", "Report", 0) }
        11 { ("antinuke disable", "safety antinuke disable", 6) }
        12 { ("antinuke enable", "safety antinuke enable", 6) }
        13 { ("antinuke events", "safety antinuke events", 6) }
        14 { ("antinuke limits", "safety antinuke limits", 6) }
        15 { ("antinuke status", "safety antinuke status", 6) }
        16 { ("antinuke trust add", "safety antinuke trust-add", 6) }
        17 { ("antinuke trust remove", "safety antinuke trust-remove", 6) }
        18 { ("antinuke vip add", "safety antinuke vip-add", 6) }
        19 { ("antinuke vip remove", "safety antinuke vip-remove", 6) }
        20 { ("automod exempt channel", "safety automod exempt-channel", 6) }
        21 { ("automod exempt role", "safety automod exempt-role", 6) }
        22 { ("automod profile", "safety automod profile", 6) }
        23 { ("automod show", "safety automod show", 6) }
        24 { ("automod test", "safety automod test", 6) }
        25 { ("automod toggle", "safety automod toggle", 6) }
        38 { ("ban", "ban", 18) }
        52 { ("case", "mod case", 9) }
        245 { ("kick", "kick", 18) }
        248 { ("lock", "mod lock", 9) }
        254 { ("modtools mass", "mod mass", 18) }
        255 { ("modtools names", "mod names", 18) }
        256 { ("modtools notes", "mod notes", 18) }
        257 { ("modtools policy", "safety moderation policy", 18) }
        258 { ("modtools report", "mod report", 18) }
        259 { ("modtools restrict", "mod restrict", 18) }
        260 { ("modtools roles", "mod roles", 18) }
        261 { ("modtools voice", "mod voice", 18) }
        263 { ("nick", "mod nick", 9) }
        320 { ("purge", "purge", 18) }
        323 { ("raid lockdown", "mod raid lockdown", 9) }
        324 { ("raid status", "mod raid status", 9) }
        325 { ("raid unlock", "mod raid unlock", 9) }
        327 { ("reason", "mod reason", 9) }
        344 { ("role add", "mod role add", 9) }
        345 { ("role remove", "mod role remove", 9) }
        360 { ("slowmode", "mod slowmode", 9) }
        410 { ("timeout", "timeout", 18) }
        417 { ("unban", "mod unban", 9) }
        418 { ("unlock", "mod unlock", 9) }
        419 { ("untimeout", "mod untimeout", 9) }
        420 { ("unwarn", "mod unwarn", 9) }
        421 { ("verification message", "safety verification message", 6) }
        422 { ("verification mode", "safety verification mode", 6) }
        423 { ("verification post", "safety verification post", 6) }
        424 { ("verification setup", "safety verification setup", 6) }
        425 { ("verify", "verify", 0) }
        440 { ("warn", "warn", 18) }
        441 { ("warnings", "mod warnings", 9) }
    }
    legacy_leaves {
        1 { "Report" }
        11 { "antinuke disable" }
        12 { "antinuke enable" }
        13 { "antinuke events" }
        14 { "antinuke limits" }
        15 { "antinuke status" }
        16 { "antinuke trust add" }
        17 { "antinuke trust remove" }
        18 { "antinuke vip add" }
        19 { "antinuke vip remove" }
        20 { "automod exempt channel" }
        21 { "automod exempt role" }
        22 { "automod profile" }
        23 { "automod show" }
        24 { "automod test" }
        25 { "automod toggle" }
        38 { "ban" }
        52 { "case" }
        245 { "kick" }
        248 { "lock" }
        254 { "modtools mass" }
        255 { "modtools names" }
        256 { "modtools notes" }
        257 { "modtools policy" }
        258 { "modtools report" }
        259 { "modtools restrict" }
        260 { "modtools roles" }
        261 { "modtools voice" }
        263 { "nick" }
        320 { "purge" }
        323 { "raid lockdown" }
        324 { "raid status" }
        325 { "raid unlock" }
        327 { "reason" }
        344 { "role add" }
        345 { "role remove" }
        360 { "slowmode" }
        410 { "timeout" }
        417 { "unban" }
        418 { "unlock" }
        419 { "untimeout" }
        420 { "unwarn" }
        421 { "verification message" }
        422 { "verification mode" }
        423 { "verification post" }
        424 { "verification setup" }
        425 { "verify" }
        440 { "warn" }
        441 { "warnings" }
    }
    command_gates {
        1000 { "modtools" => {
                Self::Moderation
            } }
        1001 { "Report" => {
                Self::Moderation
            } }
        1002 { "ban" => {
                Self::Moderation
            } }
        1003 { "unban" => {
                Self::Moderation
            } }
        1004 { "kick" => {
                Self::Moderation
            } }
        1005 { "timeout" => {
                Self::Moderation
            } }
        1006 { "untimeout" => {
                Self::Moderation
            } }
        1007 { "warn" => {
                Self::Moderation
            } }
        1008 { "warnings" => {
                Self::Moderation
            } }
        1009 { "unwarn" => {
                Self::Moderation
            } }
        1010 { "purge" => {
                Self::Moderation
            } }
        1011 { "slowmode" => {
                Self::Moderation
            } }
        1012 { "lock" => {
                Self::Moderation
            } }
        1013 { "unlock" => {
                Self::Moderation
            } }
        1014 { "nick" => {
                Self::Moderation
            } }
        1015 { "role" => {
                Self::Moderation
            } }
        1016 { "case" => {
                Self::Moderation
            } }
        1017 { "reason" => {
                Self::Moderation
            } }
        3500 { "automod" => Self::AutoMod, }
        3600 { "antinuke" => Self::AntiNuke, }
        3800 { "verify" => Self::Verification, }
        3801 { "verification" => Self::Verification, }
        3900 { "raid" => Self::AntiRaid, }
    }
    component_gates {
        2700 { "am" => Self::AutoMod, }
        2701 { "ax" => Self::AutoMod, }
        2800 { "vf" => Self::Verification, }
        2801 { "cq" => Self::Verification, }
        2900 { "rd" => Self::AntiRaid, }
        3000 { "an" => Self::AntiNuke, }
        3001 { "vp" => Self::AntiNuke, }
    }
    help_areas {
        12 { "moderation", }
        14 { "logsetup", }
    }
    admin_roots {
        1 { "safety" }
    }
    route_roots {
        1 { (
            "safety",
            "Configure moderation and server protection",
            Some(Permissions::MANAGE_GUILD),
        ), }
        5 { ("mod", "Manage moderation actions and records", None), }
    }
}
