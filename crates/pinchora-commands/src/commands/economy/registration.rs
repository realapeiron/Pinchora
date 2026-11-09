//! Domain-owned command registration. Regenerate compatibility tables with `cargo xtask registrations`.

use crate::commands::registration::{commands, compatibility};

commands! {
    typed {
        19 => crate::commands::economy::leaderboard::Leaderboard,
        20 => crate::commands::economy::daily::Daily,
        23 => crate::commands::economy::legacy::Balance,
        24 => crate::commands::economy::player::EconomyCommand,
        25 => crate::commands::economy::legacy::Give,
        30 => crate::commands::economy::shop::ShopCommand,
        31 => crate::commands::economy::shop::Inventory,
        34 => crate::commands::economy::gift::Gift,
        36 => crate::commands::economy::quests::Quests,
        37 => crate::commands::economy::stall::StallCommand,
        40 => crate::commands::economy::points::PointsCommand,
        41 => crate::commands::economy::store::StoreCommand,
        64 => crate::commands::economy::surge::SurgeCommand,
    }
    context {
    }
}

compatibility! {
    components {
        14 { Some("lb") => leaderboard::on_button(cx, parts).await, }
        24 { Some("mk") => shop::on_market_buy(cx, parts, values.first().map(String::as_str)).await, }
        25 { Some("cb") => shop::on_catalog_buy(cx, values.first().map(String::as_str)).await, }
        26 { Some("cp") => shop::on_catalog_page(cx, parts).await, }
        27 { Some("iv") => shop::on_inventory_page(cx, parts).await, }
        36 { Some("sl") => stall::on_list(cx, parts, values.first().map(String::as_str)).await, }
        37 { Some("su") => stall::on_unlist(cx, values.first().map(String::as_str)).await, }
        38 { Some("sb") => stall::on_buy(cx, values.first().map(String::as_str)).await, }
        39 { Some("qc") => quests::on_claim(cx, parts).await, }
        41 { Some("gf") => gift::on_gift(cx, parts, values.first().map(String::as_str)).await, }
        44 { Some("st") => store::on_buy(cx, values.first().map(String::as_str)).await, }
        85 { Some("ss") => surge::on_button(cx, parts).await, }
    }
    owners {
        8 { "mk" => "shop market", }
        9 { "cb" | "cp" => "shop catalog", }
        10 { "iv" => "inventory", }
        11 { "sl" => "stall sell", }
        12 { "su" => "stall manage", }
        13 { "sb" => "stall view", }
        14 { "gf" => "gift", }
        23 { "qc" => "quests", }
        46 { "ss" => "surge", }
        57 { "st" => "store", }
        84 { "lb" => "leaderboard", }
    }
    owner_prelude {
        0 { if custom_id.starts_with("sl:eco:") {
        return Ok(Some(if custom_id.starts_with("sl:eco:step:") {
            "economyv2 jobs"
        } else {
            "economyv2 trade-review"
        }));
    } }
    }
    private {
        41 { "gift", }
        77 { "quests", }
        84 { "economyv2 jobs", }
        85 { "economyv2 provisions", }
        86 { "economyv2 chest", }
        87 { "economyv2 season", }
        88 { "economyv2 prestige", }
        89 { "economyv2 donate", }
        90 { "economyv2 auction", }
        91 { "economyv2 bid", }
        92 { "economyv2 auction-close", }
        93 { "economyv2 trade", }
        94 { "economyv2 trade-review", }
        95 { "economyv2 tools", }
        96 { "stall manage", }
        97 { "stall sell", }
    }
    slash_only {
        16 { "stall manage", }
    }
    aliases {
        0 { ("bal", "balance"), }
        1 { ("lb", "leaderboard"), }
        2 { ("top", "leaderboard"), }
        4 { ("inv", "inventory"), }
    }
    help_community {
        10 { "points", }
        11 { "store", }
    }
    help_crowd {
        3 { "surge", }
    }
    help_shops {
        0 { "shop", }
        1 { "inventory", }
        2 { "gift", }
        3 { "stall", }
    }
    help_rewards {
        0 { "economyv2", }
        3 { "give", }
    }
    help_start {
        2 { "leaderboard", }
        3 { "daily", }
        4 { "balance", }
        5 { "quests", }
    }
    ephemeral {
        0 { "balance", }
        4 { "inventory", }
        5 { "leaderboard", }
    }
    routes {
        37 { ("balance", "balance", 0) }
        143 { ("daily", "daily", 0) }
        153 { ("economyv2 auction", "stall auction", 20) }
        154 { ("economyv2 auction-close", "stall auction-close", 20) }
        155 { ("economyv2 bid", "stall bid", 20) }
        156 { ("economyv2 chest", "rewards chest", 20) }
        157 { ("economyv2 donate", "rewards donate", 20) }
        158 { ("economyv2 jobs", "rewards jobs", 20) }
        159 { ("economyv2 prestige", "rewards prestige", 20) }
        160 { ("economyv2 provisions", "rewards provisions", 20) }
        161 { ("economyv2 season", "rewards season", 20) }
        162 { ("economyv2 tools", "progression economy tools", 20) }
        163 { ("economyv2 trade", "stall trade", 20) }
        164 { ("economyv2 trade-review", "stall trade-review", 20) }
        198 { ("gift", "gift", 0) }
        199 { ("give", "give", 0) }
        235 { ("inventory", "inventory", 0) }
        247 { ("leaderboard", "leaderboard", 0) }
        314 { ("points give", "progression economy points-give", 4) }
        315 { ("points show", "points show", 0) }
        316 { ("points take", "progression economy points-take", 4) }
        322 { ("quests", "quests", 0) }
        358 { ("shop catalog", "shop catalog", 0) }
        359 { ("shop market", "shop market", 0) }
        361 { ("stall manage", "stall manage", 0) }
        362 { ("stall sell", "stall sell", 0) }
        363 { ("stall view", "stall view", 0) }
        378 { ("store", "store", 0) }
        386 { ("surge", "surge", 0) }
    }
    legacy_leaves {
        37 { "balance" }
        143 { "daily" }
        153 { "economyv2 auction" }
        154 { "economyv2 auction-close" }
        155 { "economyv2 bid" }
        156 { "economyv2 chest" }
        157 { "economyv2 donate" }
        158 { "economyv2 jobs" }
        159 { "economyv2 prestige" }
        160 { "economyv2 provisions" }
        161 { "economyv2 season" }
        162 { "economyv2 tools" }
        163 { "economyv2 trade" }
        164 { "economyv2 trade-review" }
        198 { "gift" }
        199 { "give" }
        235 { "inventory" }
        247 { "leaderboard" }
        314 { "points give" }
        315 { "points show" }
        316 { "points take" }
        322 { "quests" }
        358 { "shop catalog" }
        359 { "shop market" }
        361 { "stall manage" }
        362 { "stall sell" }
        363 { "stall view" }
        378 { "store" }
        386 { "surge" }
    }
    command_gates {
        2 { "leaderboard" => Self::Profile, }
        100 { "economyv2" => Self::Economy, }
        101 { "daily" => Self::Economy, }
        104 { "balance" => Self::Economy, }
        105 { "give" => Self::Economy, }
        300 { "shop" => Self::Shop, }
        301 { "inventory" => Self::Shop, }
        302 { "gift" => Self::Shop, }
        303 { "stall" => Self::Shop, }
        600 { "quests" => Self::Quests, }
        800 { "points" => Self::Store, }
        801 { "store" => Self::Store, }
        2400 { "surge" => Self::Party, }
    }
    component_gates {
        0 { "lb" => Self::Profile, }
        300 { "mk" => Self::Shop, }
        301 { "cb" => Self::Shop, }
        302 { "cp" => Self::Shop, }
        303 { "iv" => Self::Shop, }
        304 { "sl" => Self::Shop, }
        305 { "su" => Self::Shop, }
        306 { "sb" => Self::Shop, }
        307 { "gf" => Self::Shop, }
        600 { "qc" => Self::Quests, }
        1303 { "ss" => Self::Party, }
        1600 { "st" => Self::Store, }
    }
    help_areas {
        2 { "rewards", }
        3 { "shops", }
    }
    command_gate_overrides {
        1 { (data.name == "economyv2" && subcommand(data) == Some("tools"), None) }
    }
    component_gate_overrides {
        0 { (custom_id.starts_with("sl:eco:"), Some(Module::Economy)) }
    }
}
