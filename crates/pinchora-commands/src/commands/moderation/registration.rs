//! Domain-owned command registration. Regenerate compatibility tables with `cargo xtask registrations`.

use crate::commands::registration::{commands, compatibility};

commands! {
    typed {
        crate::commands::moderation::policy::Parity,
        crate::commands::moderation::Ban,
        crate::commands::moderation::Unban,
        crate::commands::moderation::Kick,
        crate::commands::moderation::Timeout,
        crate::commands::moderation::Untimeout,
        crate::commands::moderation::Warn,
        crate::commands::moderation::Warnings,
        crate::commands::moderation::Unwarn,
        crate::commands::moderation::Purge,
        crate::commands::moderation::Slowmode,
        crate::commands::moderation::Lock,
        crate::commands::moderation::Unlock,
        crate::commands::moderation::Nick,
        crate::commands::moderation::RoleCommand,
        crate::commands::moderation::CaseCommand,
        crate::commands::moderation::ReasonCommand,
        crate::commands::moderation::automod::AutoMod,
        crate::commands::moderation::verification::Verify,
        crate::commands::moderation::verification::Verification,
        crate::commands::moderation::verification::Raid,
        crate::commands::moderation::anti_nuke::AntiNuke,
    }
    context {
        ("Report", crate::commands::moderation::report_context),
    }
}

compatibility! {
    components {
        Some("vf") => crate::moderation::verification::on_component(cx, "vf", parts).await,
        Some("cq") => crate::moderation::verification::on_component(cx, "cq", parts).await,
        Some("rd") => crate::moderation::verification::on_component(cx, "rd", parts).await,
        Some("an") => crate::commands::moderation::anti_nuke::on_component(cx, "an", parts).await,
        Some("vp") => crate::commands::moderation::anti_nuke::on_component(cx, "vp", parts).await,
        Some("am") => crate::commands::moderation::automod::on_component(cx, "am", parts, values).await,
        Some("ax") => crate::commands::moderation::automod::on_component(cx, "ax", parts, values).await,
    }
    modals {
        Some("cq") => crate::moderation::verification::on_modal(cx, parts, fields).await,
    }
    owners {
        "am" | "ax" => "automod",
        "vf" | "cq" => "verify",
        "rd" => "raid",
        "an" | "vp" => "antinuke",
    }
    owner_actions {
        ("ax", "r") => Some("automod exempt role"),
        ("ax", "c") => Some("automod exempt channel"),
    }

    command_gates {
        "modtools" => {
                Self::Moderation
            }
        "Report" => {
                Self::Moderation
            }
        "ban" => {
                Self::Moderation
            }
        "unban" => {
                Self::Moderation
            }
        "kick" => {
                Self::Moderation
            }
        "timeout" => {
                Self::Moderation
            }
        "untimeout" => {
                Self::Moderation
            }
        "warn" => {
                Self::Moderation
            }
        "warnings" => {
                Self::Moderation
            }
        "unwarn" => {
                Self::Moderation
            }
        "purge" => {
                Self::Moderation
            }
        "slowmode" => {
                Self::Moderation
            }
        "lock" => {
                Self::Moderation
            }
        "unlock" => {
                Self::Moderation
            }
        "nick" => {
                Self::Moderation
            }
        "role" => {
                Self::Moderation
            }
        "case" => {
                Self::Moderation
            }
        "reason" => {
                Self::Moderation
            }
        "automod" => Self::AutoMod,
        "antinuke" => Self::AntiNuke,
        "verify" => Self::Verification,
        "verification" => Self::Verification,
        "raid" => Self::AntiRaid,
    }
    component_gates {
        "am" => Self::AutoMod,
        "ax" => Self::AutoMod,
        "vf" => Self::Verification,
        "cq" => Self::Verification,
        "rd" => Self::AntiRaid,
        "an" => Self::AntiNuke,
        "vp" => Self::AntiNuke,
    }

    route_roots {
        (
            "safety",
            "Configure moderation and server protection",
            Some(Permissions::MANAGE_GUILD),
        ),
        ("mod", "Manage moderation actions and records", None),
    }
}
