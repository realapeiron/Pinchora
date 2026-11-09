//! Domain-owned command registration. Regenerate compatibility tables with `cargo xtask registrations`.

use crate::commands::registration::{commands, compatibility};

commands! {
    typed {
        0 => crate::commands::admin::workflows::Workflow,
        12 => crate::commands::admin::branding::BrandingCommand,
        15 => crate::commands::admin::ping::Ping,
        16 => crate::commands::admin::help::Help,
        28 => crate::commands::admin::config::ConfigCommand,
        54 => crate::commands::admin::privacy::PrivacyCommand,
        83 => crate::commands::admin::customcommand::CustomCommand,
        84 => crate::commands::admin::language::LanguageCommand,
        85 => crate::commands::admin::stats::StatsCommand,
        87 => crate::commands::admin::stats::CountersCommand,
        92 => crate::commands::admin::command_control::CommandControl,
        93 => crate::commands::admin::command_control::BackupCommand,
        99 => crate::commands::admin::tempvoice::VoiceCommand,
        101 => crate::commands::admin::info::Info,
        108 => crate::commands::admin::afk_sticky_schedule::AfkCommand,
        109 => crate::commands::admin::afk_sticky_schedule::StickyCommand,
        110 => crate::commands::admin::afk_sticky_schedule::ScheduleCommand,
    }
    context {
    }
}

compatibility! {
    components {
        0 { Some("wf") => crate::admin::workflows::triggers::on_component(cx, parts).await, }
        1 { Some("ms") => crate::social::message_studio::on_component(cx, parts, values).await, }
        4 { Some("kc") => crate::commands::admin::command_control::on_control_component(cx).await, }
        5 { Some("cf") => crate::commands::admin::command_control::on_component(cx, parts).await, }
        16 { Some("nf") => crate::commands::admin::info::on_component(cx, parts).await, }
        70 { Some("hp") => help::on_component(cx, parts, values.first().map(String::as_str)).await, }
        72 { Some("pr") => privacy::on_component(cx, parts).await, }
        76 { Some("tg") => customcommand::on_page(cx, parts).await, }
        92 { Some("sc") => crate::commands::admin::stats::on_component(cx, parts).await, }
        94 { Some(prefix @ ("sk" | "sm")) => {
            crate::commands::admin::afk_sticky_schedule::on_component(cx, prefix, parts).await
        } }
        102 { Some("vc") => tempvoice::on_button(cx, parts).await, }
        103 { Some("vo") => tempvoice::on_select(cx, parts, values).await, }
    }
    modals {
        5 { Some("cm") => customcommand::on_modal(cx, parts, fields).await, }
        9 { Some("vc") => tempvoice::on_modal(cx, parts, fields).await, }
        16 { Some(prefix @ ("sk" | "sm")) => {
            crate::commands::admin::afk_sticky_schedule::on_modal(cx, prefix, parts, fields).await
        } }
    }
    owners {
        65 { "wf" => "tag", }
        66 { "tg" => "customcommand list", }
        67 { "cm" => "customcommand", }
        68 { "ms" => "customcommand", }
        71 { "sc" => "stats", }
        77 { "vc" | "vo" => "voice", }
        78 { "nf" => "info user", }
        87 { "hp" => "help", }
        88 { "pr" => "privacy", }
        90 { "kc" => "commandcontrol", }
        91 { "cf" => "backup", }
        95 { "af" => "afk set", }
        96 { "sk" => "sticky set", }
        97 { "sm" => "schedule add", }
    }
    owner_actions {
        6 { ("sc", "s") => Some("stats server"), }
        7 { ("sc", "m") => Some("stats me"), }
        8 { ("sc", "c") => Some("stats channel"), }
        11 { ("vc", "lk") => Some("voice lock"), }
        12 { ("vc", "ul") => Some("voice unlock"), }
        13 { ("vc", "lm") => Some("voice limit"), }
        14 { ("vc", "rn") => Some("voice rename"), }
        15 { ("vc", "kk") | ("vo", "k") => Some("voice kick"), }
        16 { ("vc", "tr") | ("vo", "t") => Some("voice transfer"), }
        17 { ("vc", "hd") => Some("voice hide"), }
        18 { ("vc", "cl") => Some("voice claim"), }
        19 { ("cm", "c") => Some("customcommand create"), }
        20 { ("cm", "e") => Some("customcommand edit"), }
    }
    private {
        0 { "workflow", }
        18 { "branding", }
        22 { "config", }
        23 { "counters", }
        25 { "customcommand delete", }
        26 { "customcommand info", }
        27 { "customcommand list", }
        48 { "help", }
        57 { "language", }
        75 { "privacy", }
        98 { "stats", }
        106 { "commandcontrol", }
        107 { "backup", }
        108 { "voice", }
        110 { "info channel", }
        111 { "info invite", }
        116 { "afk", }
        125 { "sticky", }
        126 { "schedule", }
    }
    opens_form {
        0 { "customcommand create", }
        1 { "customcommand edit", }
    }
    opens_modal {
        18 { "vc:lm", }
        19 { "vc:rn", }
        21 { "sk:e", }
        22 { "sm:c", }
    }
    slash_only {
        5 { "setup", }
    }
    aliases {
        3 { ("h", "help"), }
    }
    help_modules {
        0 { "config critters", }
        1 { "config gambling", }
        2 { "config surge", }
        3 { "config partygames", }
        4 { "config modules", }
    }
    help_levels {
        0 { "config level-roles", }
        1 { "config xp", }
        2 { "config rep-roles", }
        3 { "config points", }
        4 { "config store", }
    }
    help_logsetup {
        1 { "config log", }
        2 { "config log-events", }
        3 { "config log-ignore", }
        4 { "config moderation", }
        5 { "config warn-ladder", }
    }
    help_setup {
        0 { "config show", }
        1 { "config prefix", }
        2 { "config welcome", }
        3 { "config welcome-card", }
        4 { "config leveling", }
        5 { "config join-role", }
        6 { "config goodbye", }
        7 { "config suggestions", }
        8 { "config language", }
        9 { "info", }
        10 { "counters", }
    }
    help_moderation {
        1 { "commandcontrol", }
        2 { "backup", }
        3 { "branding", }
    }
    help_community {
        13 { "customcommand", }
        14 { "workflow", }
        15 { "voice", }
        20 { "stats", }
        36 { "afk", }
        37 { "sticky", }
        38 { "schedule", }
    }
    help_profile {
        4 { "privacy", }
    }
    help_start {
        6 { "help", }
        7 { "language", }
        8 { "ping", }
    }
    never_gated {
        0 { "config", }
        1 { "help", }
        2 { "language", }
        3 { "ping", }
        4 { "privacy", }
        6 { "commandcontrol", }
        7 { "backup", }
    }
    never_gated_prefixes {
        0 { "hp", }
        1 { "pr", }
        3 { "kc", }
        4 { "cf", }
    }
    protected {
        0 { "commandcontrol", }
        1 { "backup", }
        2 { "config show", }
        3 { "config modules", }
        4 { "config prefix", }
        5 { "config language", }
        6 { "setup server", }
        7 { "setup commands show", }
        8 { "setup commands set", }
        9 { "setup commands reset", }
        10 { "setup commands alias", }
        11 { "setup backup", }
        12 { "help", }
        13 { "ping", }
        14 { "privacy", }
        15 { "language", }
    }
    ephemeral {
        3 { "info", }
        6 { "stats", }
    }
    routes {
        8 { ("afk set", "afk set", 0) }
        33 { ("backup export", "setup backup export", 11) }
        34 { ("backup import", "setup backup import", 11) }
        35 { ("backup list", "setup backup list", 11) }
        36 { ("backup restore", "setup backup restore", 11) }
        49 { ("branding apply", "setup branding apply", 18) }
        50 { ("branding reset", "setup branding reset", 18) }
        51 { ("branding show", "setup branding show", 18) }
        63 { ("commandcontrol alias", "setup commands alias", 11) }
        64 { ("commandcontrol reset", "setup commands reset", 11) }
        65 { ("commandcontrol set", "setup commands set", 11) }
        66 { ("commandcontrol show", "setup commands show", 11) }
        81 { ("config critters", "progression economy critters", 4) }
        82 { ("config gambling", "progression economy gambling", 4) }
        83 { ("config goodbye", "setup welcome goodbye", 3) }
        84 { ("config join-role", "setup roles join-role", 5) }
        85 { ("config language", "setup server language", 3) }
        86 { ("config level-roles add", "progression leveling roles-add", 4) }
        87 { ("config level-roles list", "progression leveling roles-list", 4) }
        88 { ("config level-roles mode", "progression leveling roles-mode", 4) }
        89 { ("config level-roles remove", "progression leveling roles-remove", 4) }
        90 { ("config level-roles sync", "progression leveling roles-sync", 4) }
        91 { ("config leveling", "progression leveling settings", 4) }
        92 { ("config log", "safety moderation log", 3) }
        93 { ("config log-events", "safety moderation log-events", 3) }
        94 { ("config log-ignore", "safety moderation log-ignore", 3) }
        95 { ("config moderation", "safety moderation settings", 3) }
        96 { ("config modules channels", "setup server modules-channels", 3) }
        97 { ("config modules show", "setup server modules-show", 3) }
        98 { ("config modules toggle", "setup server modules-toggle", 3) }
        99 { ("config partygames", "progression economy partygames", 4) }
        100 { ("config points", "progression economy points-settings", 4) }
        101 { ("config prefix reset", "setup server prefix-reset", 3) }
        102 { ("config prefix set", "setup server prefix-set", 3) }
        103 { ("config prefix show", "setup server prefix-show", 3) }
        104 { ("config rep-roles add", "progression economy rep-roles-add", 4) }
        105 { ("config rep-roles list", "progression economy rep-roles-list", 4) }
        106 { ("config rep-roles remove", "progression economy rep-roles-remove", 4) }
        107 { ("config show", "setup server show", 3) }
        108 { ("config store add", "progression economy store-add", 4) }
        109 { ("config store list", "progression economy store-list", 4) }
        110 { ("config store remove", "progression economy store-remove", 4) }
        111 { ("config suggestions", "community server suggestions", 8) }
        112 { ("config surge", "progression economy surge", 4) }
        113 { ("config warn-ladder", "safety moderation warn-ladder", 3) }
        114 { ("config welcome", "setup welcome message", 3) }
        115 { ("config welcome-card", "setup welcome card", 3) }
        116 { ("config xp clear", "progression leveling xp-clear", 4) }
        117 { ("config xp curve", "progression leveling xp-curve", 20) }
        118 { ("config xp multiplier", "progression leveling xp-multiplier", 4) }
        119 { ("config xp public", "progression leveling xp-public", 20) }
        120 { ("config xp show", "progression leveling xp-show", 4) }
        121 { ("config xp sources", "progression leveling xp-sources", 20) }
        122 { ("config xp tuning", "progression leveling xp-tuning", 20) }
        123 { ("config xp voice", "progression leveling xp-voice", 4) }
        124 { ("config xp voice-role-add", "progression leveling xp-voice-role-add", 20) }
        125 { ("config xp voice-role-list", "progression leveling xp-voice-role-list", 20) }
        126 { ("config xp voice-role-remove", "progression leveling xp-voice-role-remove", 20) }
        127 { ("counters add", "integrations stats counter-add", 7) }
        128 { ("counters list", "integrations stats counter-list", 7) }
        129 { ("counters remove", "integrations stats counter-remove", 7) }
        133 { ("customcommand create", "setup commands custom-create", 11) }
        134 { ("customcommand delete", "setup commands custom-delete", 11) }
        135 { ("customcommand edit", "setup commands custom-edit", 11) }
        136 { ("customcommand info", "setup commands custom-info", 11) }
        137 { ("customcommand list", "setup commands custom-list", 11) }
        204 { ("help", "help", 0) }
        225 { ("info avatar", "info avatar", 0) }
        226 { ("info banner", "info banner", 0) }
        227 { ("info bot", "info bot", 0) }
        228 { ("info channel", "info channel", 0) }
        229 { ("info emoji", "info emoji", 0) }
        230 { ("info invite", "info invite", 0) }
        231 { ("info members", "info members", 0) }
        232 { ("info role", "info role", 0) }
        233 { ("info server", "info server", 0) }
        234 { ("info user", "info user", 0) }
        246 { ("language set", "language set", 0) }
        292 { ("ping", "ping", 0) }
        318 { ("privacy", "privacy", 0) }
        351 { ("schedule add", "community utility schedule-add", 8) }
        352 { ("schedule list", "community utility schedule-list", 8) }
        353 { ("schedule pause", "community utility schedule-pause", 8) }
        354 { ("schedule preview", "community utility schedule-preview", 8) }
        355 { ("schedule remove", "community utility schedule-remove", 8) }
        356 { ("schedule resume", "community utility schedule-resume", 8) }
        357 { ("schedule run-now", "community utility schedule-run-now", 8) }
        372 { ("stats channel", "stats channel", 0) }
        373 { ("stats me", "stats me", 0) }
        374 { ("stats server", "stats server", 0) }
        375 { ("sticky remove", "community utility sticky-remove", 8) }
        376 { ("sticky set", "community utility sticky-set", 8) }
        377 { ("sticky show", "community utility sticky-show", 8) }
        426 { ("voice claim", "voice claim", 0) }
        427 { ("voice creators add", "community utility voice-add", 5) }
        428 { ("voice creators list", "community utility voice-list", 5) }
        429 { ("voice creators remove", "community utility voice-remove", 5) }
        430 { ("voice hide", "voice hide", 0) }
        431 { ("voice invite", "voice invite", 0) }
        432 { ("voice kick", "voice kick", 0) }
        433 { ("voice limit", "voice limit", 0) }
        434 { ("voice lock", "voice lock", 0) }
        435 { ("voice panel", "voice panel", 0) }
        436 { ("voice rename", "voice rename", 0) }
        437 { ("voice show", "voice show", 0) }
        438 { ("voice transfer", "voice transfer", 0) }
        439 { ("voice unlock", "voice unlock", 0) }
        444 { ("workflow list", "setup workflows list", 20) }
        445 { ("workflow runs", "setup workflows runs", 20) }
        446 { ("workflow toggle", "setup workflows toggle", 20) }
    }
    legacy_leaves {
        8 { "afk set" }
        33 { "backup export" }
        34 { "backup import" }
        35 { "backup list" }
        36 { "backup restore" }
        49 { "branding apply" }
        50 { "branding reset" }
        51 { "branding show" }
        63 { "commandcontrol alias" }
        64 { "commandcontrol reset" }
        65 { "commandcontrol set" }
        66 { "commandcontrol show" }
        81 { "config critters" }
        82 { "config gambling" }
        83 { "config goodbye" }
        84 { "config join-role" }
        85 { "config language" }
        86 { "config level-roles add" }
        87 { "config level-roles list" }
        88 { "config level-roles mode" }
        89 { "config level-roles remove" }
        90 { "config level-roles sync" }
        91 { "config leveling" }
        92 { "config log" }
        93 { "config log-events" }
        94 { "config log-ignore" }
        95 { "config moderation" }
        96 { "config modules channels" }
        97 { "config modules show" }
        98 { "config modules toggle" }
        99 { "config partygames" }
        100 { "config points" }
        101 { "config prefix reset" }
        102 { "config prefix set" }
        103 { "config prefix show" }
        104 { "config rep-roles add" }
        105 { "config rep-roles list" }
        106 { "config rep-roles remove" }
        107 { "config show" }
        108 { "config store add" }
        109 { "config store list" }
        110 { "config store remove" }
        111 { "config suggestions" }
        112 { "config surge" }
        113 { "config warn-ladder" }
        114 { "config welcome" }
        115 { "config welcome-card" }
        116 { "config xp clear" }
        117 { "config xp curve" }
        118 { "config xp multiplier" }
        119 { "config xp public" }
        120 { "config xp show" }
        121 { "config xp sources" }
        122 { "config xp tuning" }
        123 { "config xp voice" }
        124 { "config xp voice-role-add" }
        125 { "config xp voice-role-list" }
        126 { "config xp voice-role-remove" }
        127 { "counters add" }
        128 { "counters list" }
        129 { "counters remove" }
        133 { "customcommand create" }
        134 { "customcommand delete" }
        135 { "customcommand edit" }
        136 { "customcommand info" }
        137 { "customcommand list" }
        204 { "help" }
        225 { "info avatar" }
        226 { "info banner" }
        227 { "info bot" }
        228 { "info channel" }
        229 { "info emoji" }
        230 { "info invite" }
        231 { "info members" }
        232 { "info role" }
        233 { "info server" }
        234 { "info user" }
        246 { "language set" }
        292 { "ping" }
        318 { "privacy" }
        351 { "schedule add" }
        352 { "schedule list" }
        353 { "schedule pause" }
        354 { "schedule preview" }
        355 { "schedule remove" }
        356 { "schedule resume" }
        357 { "schedule run-now" }
        372 { "stats channel" }
        373 { "stats me" }
        374 { "stats server" }
        375 { "sticky remove" }
        376 { "sticky set" }
        377 { "sticky show" }
        426 { "voice claim" }
        427 { "voice creators add" }
        428 { "voice creators list" }
        429 { "voice creators remove" }
        430 { "voice hide" }
        431 { "voice invite" }
        432 { "voice kick" }
        433 { "voice limit" }
        434 { "voice lock" }
        435 { "voice panel" }
        436 { "voice rename" }
        437 { "voice show" }
        438 { "voice transfer" }
        439 { "voice unlock" }
        444 { "workflow list" }
        445 { "workflow runs" }
        446 { "workflow toggle" }
    }
    command_gates {
        1501 { "customcommand" => Self::CustomCommands, }
        1600 { "workflow" => Self::Workflows, }
        2100 { "info" => Self::Info, }
        2600 { "stats" => Self::Stats, }
        2601 { "counters" => Self::Stats, }
        3000 { "afk" => Self::Utility, }
        3001 { "sticky" => Self::Utility, }
        3002 { "schedule" => Self::Utility, }
        3700 { "voice" => Self::TempVoice, }
        4900 { "branding" => Self::Branding, }
    }
    component_gates {
        2200 { "wf" => Self::Workflows, }
        2100 { "tg" => Self::CustomCommands, }
        2101 { "cm" => Self::CustomCommands, }
        2102 { "ms" => Self::CustomCommands, }
        2201 { "wk" => Self::Workflows, }
        2500 { "sc" => Self::Stats, }
        3300 { "vc" => Self::TempVoice, }
        3301 { "vo" => Self::TempVoice, }
        3400 { "nf" => Self::Info, }
        3900 { "af" => Self::Utility, }
        3901 { "sk" => Self::Utility, }
        3902 { "sm" => Self::Utility, }
        4600 { "wb" => Self::Branding, }
    }
    help_areas {
        0 { "start", }
        13 { "setup", }
        16 { "modules", }
    }
    route_roots {
        0 { ("setup", "Server settings and tools", Some(Permissions::MANAGE_GUILD)), }
    }
    admin_roots {
        0 { "setup" }
    }
    autocomplete {
        2 { "customcommand" => crate::commands::community::tag::autocomplete(&cx, &command).await, }
        3 { _ => crate::commands::admin::modules::autocomplete(cx.lang(), &command), }
    }
    defer_overrides {
        0 { Some(InteractionData::ApplicationCommand(command)) if command.name == "tag" && command.options.iter().any(|o| o.name == "name" && matches!(&o.value, twilight_model::application::interaction::application_command::CommandOptionValue::String(s) if s.starts_with("wf:"))) => Self::Private, }
        1 { Some(InteractionData::MessageComponent(component)) if component.custom_id.starts_with("wf:") => Self::Private, }
    }
    command_gate_overrides {
        0 { (data.name == "tag" && data.options.iter().any(|o| o.name == "name" && matches!(&o.value, CommandOptionValue::String(s) if s.starts_with("wf:"))), Some(Module::Workflows)) }
    }
}
