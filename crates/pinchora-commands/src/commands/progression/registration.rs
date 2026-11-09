//! Domain-owned command registration. Regenerate compatibility tables with `cargo xtask registrations`.

use crate::commands::registration::{commands, compatibility};

commands! {
    typed {
        crate::commands::progression::achievements::AchievementsCommand,
        crate::commands::progression::achievements::AchievementAdmin,
        crate::commands::progression::rank::Rank,
        crate::commands::progression::rep::Rep,
        crate::commands::progression::wrapped::Wrapped,
        crate::commands::progression::xp_admin::XpAdmin,
    }
    context {
    }
}

compatibility! {
    components {
        Some("ah") => crate::commands::progression::achievements::on_component(cx, parts).await,
        Some("xi") => crate::commands::progression::xp_admin::on_component(cx, parts).await,
    }
    owners {
        "xi" => "xp",
        "ah" => "achievements",
    }
    owner_actions {
        ("xi", "confirm") => {
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
        }
    }

    command_gates {
        "rank" => Self::Profile,
        "wrapped" => Self::Profile,
        "rep" => Self::Economy,
        "xp" => Self::Leveling,
        "achievements" => Self::Achievements,
        "achievementadmin" => Self::Achievements,
    }
    component_gates {
        "ah" => Self::Achievements,
        "xi" => Self::Leveling,
    }

    route_roots {
        (
            "progression",
            "Configure leveling, economy and achievements",
            Some(Permissions::MANAGE_GUILD),
        ),
    }

}
