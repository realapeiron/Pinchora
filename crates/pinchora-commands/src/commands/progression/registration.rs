//! Domain-owned command registration. Regenerate compatibility tables with `cargo xtask registrations`.

use crate::commands::registration::{commands, compatibility};

commands! {
    typed {
        13 => crate::commands::progression::achievements::AchievementsCommand,
        14 => crate::commands::progression::achievements::AchievementAdmin,
        18 => crate::commands::progression::rank::Rank,
        26 => crate::commands::progression::rep::Rep,
        55 => crate::commands::progression::wrapped::Wrapped,
        100 => crate::commands::progression::xp_admin::XpAdmin,
    }
    context {
    }
}

compatibility! {
    components {
        3 { Some("ah") => crate::commands::progression::achievements::on_component(cx, parts).await, }
        15 { Some("xi") => crate::commands::progression::xp_admin::on_component(cx, parts).await, }
    }
    owners {
        80 { "xi" => "xp", }
        94 { "ah" => "achievements", }
    }
    owner_actions {
        29 { ("xi", "confirm") => {
            let nonce = parts.next().unwrap_or_default();
            let payload = cx
                .app
                .cache
                .get(&format!(
                    "xp:confirm:{}:{}:{nonce}",
                    cx.guild_id().unwrap_or_default(),
                    cx.author()?.id
                ))
                .await?;
            match payload.as_deref().and_then(|p| p.split(':').nth(1)) {
                Some("reset") => Some("xp reset"),
                Some("all") => Some("xp reset-all"),
                Some("go") => Some("xp import mee6"),
                _ => owner(prefix),
            }
        } }
    }
    private {
        12 { "achievementadmin", }
        109 { "xp", }
    }
    slash_only {
        9 { "progression", }
    }
    help_levels {
        5 { "xp", }
    }
    help_rewards {
        4 { "rep", }
    }
    help_profile {
        3 { "wrapped", }
        8 { "achievements", }
        9 { "achievementadmin", }
    }
    help_start {
        1 { "rank", }
    }
    admin_group_moves {
        13 { ("setup leveling", "progression leveling"), }
        14 { ("setup economy", "progression economy"), }
        15 { ("setup achievements", "progression achievements"), }
    }
    ephemeral {
        1 { "rank", }
    }
    routes {
        2 { ("achievementadmin add", "progression achievements add", 8) }
        3 { ("achievementadmin edit", "progression achievements edit", 8) }
        4 { ("achievementadmin grant", "progression achievements grant", 8) }
        5 { ("achievementadmin list", "progression achievements list", 8) }
        6 { ("achievementadmin remove", "progression achievements remove", 8) }
        7 { ("achievements", "achievements", 0) }
        326 { ("rank", "rank", 0) }
        335 { ("rep", "rep", 0) }
        447 { ("wrapped", "info wrapped", 10) }
        448 { ("xp add", "progression leveling xp-add", 4) }
        449 { ("xp import mee6", "progression leveling xp-import-mee6", 4) }
        450 { ("xp remove", "progression leveling xp-remove", 4) }
        451 { ("xp reset", "progression leveling xp-reset", 4) }
        452 { ("xp reset-all", "progression leveling xp-reset-all", 4) }
        453 { ("xp set", "progression leveling xp-set", 4) }
    }
    legacy_leaves {
        2 { "achievementadmin add" }
        3 { "achievementadmin edit" }
        4 { "achievementadmin grant" }
        5 { "achievementadmin list" }
        6 { "achievementadmin remove" }
        7 { "achievements" }
        326 { "rank" }
        335 { "rep" }
        447 { "wrapped" }
        448 { "xp add" }
        449 { "xp import mee6" }
        450 { "xp remove" }
        451 { "xp reset" }
        452 { "xp reset-all" }
        453 { "xp set" }
    }
    command_gates {
        1 { "rank" => Self::Profile, }
        6 { "wrapped" => Self::Profile, }
        106 { "rep" => Self::Economy, }
        2200 { "xp" => Self::Leveling, }
        4100 { "achievements" => Self::Achievements, }
        4101 { "achievementadmin" => Self::Achievements, }
    }
    component_gates {
        3100 { "ah" => Self::Achievements, }
        3500 { "xi" => Self::Leveling, }
    }
    help_areas {
        15 { "levels", }
    }
    route_roots {
        4 { (
            "progression",
            "Configure leveling, economy and achievements",
            Some(Permissions::MANAGE_GUILD),
        ), }
    }
    admin_roots {
        4 { "progression" }
    }
}
