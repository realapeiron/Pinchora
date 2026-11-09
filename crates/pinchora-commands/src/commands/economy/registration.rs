//! Domain-owned command registration. Regenerate compatibility tables with `cargo xtask registrations`.

use crate::commands::registration::{commands, compatibility};

commands! {
    typed {
        crate::commands::economy::leaderboard::Leaderboard,
        crate::commands::economy::daily::Daily,
        crate::commands::economy::legacy::Balance,
        crate::commands::economy::player::EconomyCommand,
        crate::commands::economy::legacy::Give,
        crate::commands::economy::shop::ShopCommand,
        crate::commands::economy::shop::Inventory,
        crate::commands::economy::gift::Gift,
        crate::commands::economy::quests::Quests,
        crate::commands::economy::stall::StallCommand,
        crate::commands::economy::points::PointsCommand,
        crate::commands::economy::store::StoreCommand,
        crate::commands::economy::surge::SurgeCommand,
    }
    context {
    }
}

compatibility! {
    components {
        Some("lb") => leaderboard::on_button(cx, parts).await,
        Some("mk") => shop::on_market_buy(cx, parts, values.first().map(String::as_str)).await,
        Some("cb") => shop::on_catalog_buy(cx, values.first().map(String::as_str)).await,
        Some("cp") => shop::on_catalog_page(cx, parts).await,
        Some("iv") => shop::on_inventory_page(cx, parts).await,
        Some("sl") => stall::on_list(cx, parts, values.first().map(String::as_str)).await,
        Some("su") => stall::on_unlist(cx, values.first().map(String::as_str)).await,
        Some("sb") => stall::on_buy(cx, values.first().map(String::as_str)).await,
        Some("qc") => quests::on_claim(cx, parts).await,
        Some("gf") => gift::on_gift(cx, parts, values.first().map(String::as_str)).await,
        Some("st") => store::on_buy(cx, values.first().map(String::as_str)).await,
        Some("ss") => surge::on_button(cx, parts).await,
    }
    owners {
        "mk" => "shop market",
        "cb" | "cp" => "shop catalog",
        "iv" => "inventory",
        "sl" => "stall sell",
        "su" => "stall manage",
        "sb" => "stall view",
        "gf" => "gift",
        "qc" => "quests",
        "ss" => "surge",
        "st" => "store",
        "lb" => "leaderboard",
    }
    owner_prelude {
        if custom_id.starts_with("sl:eco:") {
        return Ok(Some(if custom_id.starts_with("sl:eco:step:") {
            "economyv2 jobs"
        } else {
            "economyv2 trade-review"
        }));
    }
    }

    command_gates {
        "leaderboard" => Self::Profile,
        "economyv2" => Self::Economy,
        "daily" => Self::Economy,
        "balance" => Self::Economy,
        "give" => Self::Economy,
        "shop" => Self::Shop,
        "inventory" => Self::Shop,
        "gift" => Self::Shop,
        "stall" => Self::Shop,
        "quests" => Self::Quests,
        "points" => Self::Store,
        "store" => Self::Store,
        "surge" => Self::Party,
    }
    component_gates {
        "lb" => Self::Profile,
        "mk" => Self::Shop,
        "cb" => Self::Shop,
        "cp" => Self::Shop,
        "iv" => Self::Shop,
        "sl" => Self::Shop,
        "su" => Self::Shop,
        "sb" => Self::Shop,
        "gf" => Self::Shop,
        "qc" => Self::Quests,
        "ss" => Self::Party,
        "st" => Self::Store,
    }

    command_gate_overrides {
        (data.name == "economyv2" && subcommand(data) == Some("tools"), None)
    }
    component_gate_overrides {
        (custom_id.starts_with("sl:eco:"), Some(Module::Economy))
    }
}
