//! Domain-owned command registration. Regenerate compatibility tables with `cargo xtask registrations`.

use crate::commands::registration::{commands, compatibility};

commands! {
    typed {
        crate::commands::admin::workflows::Workflow,
        crate::commands::admin::branding::BrandingCommand,
        crate::commands::admin::ping::Ping,
        crate::commands::admin::help::Help,
        crate::commands::admin::config::ConfigCommand,
        crate::commands::admin::privacy::PrivacyCommand,
        crate::commands::admin::customcommand::CustomCommand,
        crate::commands::admin::language::LanguageCommand,
        crate::commands::admin::stats::StatsCommand,
        crate::commands::admin::stats::CountersCommand,
        crate::commands::admin::command_control::CommandControl,
        crate::commands::admin::command_control::BackupCommand,
        crate::commands::admin::tempvoice::VoiceCommand,
        crate::commands::admin::info::Info,
        crate::commands::admin::afk_sticky_schedule::AfkCommand,
        crate::commands::admin::afk_sticky_schedule::StickyCommand,
        crate::commands::admin::afk_sticky_schedule::ScheduleCommand,
    }
    context {
    }
}

compatibility! {
    components {
        Some("wf") => crate::admin::workflows::triggers::on_component(cx, parts).await,
        Some("ms") => crate::social::message_studio::on_component(cx, parts, values).await,
        Some("kc") => crate::commands::admin::command_control::on_control_component(cx).await,
        Some("cf") => crate::commands::admin::command_control::on_component(cx, parts).await,
        Some("nf") => crate::commands::admin::info::on_component(cx, parts).await,
        Some("hp") => help::on_component(cx, parts, values.first().map(String::as_str)).await,
        Some("pr") => privacy::on_component(cx, parts).await,
        Some("tg") => customcommand::on_page(cx, parts).await,
        Some("sc") => crate::commands::admin::stats::on_component(cx, parts).await,
        Some(prefix @ ("sk" | "sm")) => {
            crate::commands::admin::afk_sticky_schedule::on_component(cx, prefix, parts).await
        }
        Some("vc") => tempvoice::on_button(cx, parts).await,
        Some("vo") => tempvoice::on_select(cx, parts, values).await,
    }
    modals {
        Some("cm") => customcommand::on_modal(cx, parts, fields).await,
        Some("vc") => tempvoice::on_modal(cx, parts, fields).await,
        Some(prefix @ ("sk" | "sm")) => {
            crate::commands::admin::afk_sticky_schedule::on_modal(cx, prefix, parts, fields).await
        }
    }
    owners {
        "wf" => "tag",
        "tg" => "customcommand list",
        "cm" => "customcommand",
        "ms" => "customcommand",
        "sc" => "stats",
        "vc" | "vo" => "voice",
        "nf" => "info user",
        "hp" => "help",
        "pr" => "privacy",
        "kc" => "commandcontrol",
        "cf" => "backup",
        "af" => "afk set",
        "sk" => "sticky set",
        "sm" => "schedule add",
    }
    owner_actions {
        ("sc", "s") => Some("stats server"),
        ("sc", "m") => Some("stats me"),
        ("sc", "c") => Some("stats channel"),
        ("vc", "lk") => Some("voice lock"),
        ("vc", "ul") => Some("voice unlock"),
        ("vc", "lm") => Some("voice limit"),
        ("vc", "rn") => Some("voice rename"),
        ("vc", "kk") | ("vo", "k") => Some("voice kick"),
        ("vc", "tr") | ("vo", "t") => Some("voice transfer"),
        ("vc", "hd") => Some("voice hide"),
        ("vc", "cl") => Some("voice claim"),
        ("cm", "c") => Some("customcommand create"),
        ("cm", "e") => Some("customcommand edit"),
    }

    command_gates {
        "customcommand" => Self::CustomCommands,
        "workflow" => Self::Workflows,
        "info" => Self::Info,
        "stats" => Self::Stats,
        "counters" => Self::Stats,
        "afk" => Self::Utility,
        "sticky" => Self::Utility,
        "schedule" => Self::Utility,
        "voice" => Self::TempVoice,
        "branding" => Self::Branding,
    }
    component_gates {
        "wf" => Self::Workflows,
        "tg" => Self::CustomCommands,
        "cm" => Self::CustomCommands,
        "ms" => Self::CustomCommands,
        "wk" => Self::Workflows,
        "sc" => Self::Stats,
        "vc" => Self::TempVoice,
        "vo" => Self::TempVoice,
        "nf" => Self::Info,
        "af" => Self::Utility,
        "sk" => Self::Utility,
        "sm" => Self::Utility,
        "wb" => Self::Branding,
    }

    route_roots {
        ("setup", "Server settings and tools", Some(Permissions::MANAGE_GUILD)),
    }

    autocomplete {
        "customcommand" => crate::commands::community::tag::autocomplete(&cx, &command).await,
        fallback _ => crate::commands::admin::modules::autocomplete(cx.lang(), &command),
    }
    defer_overrides {
        Some(InteractionData::ApplicationCommand(command)) if command.name == "tag" && command.options.iter().any(|o| o.name == "name" && matches!(&o.value, twilight_model::application::interaction::application_command::CommandOptionValue::String(s) if s.starts_with("wf:"))) => Self::Private,
        Some(InteractionData::MessageComponent(component)) if component.custom_id.starts_with("wf:") => Self::Private,
    }
    command_gate_overrides {
        (data.name == "tag" && data.options.iter().any(|o| o.name == "name" && matches!(&o.value, CommandOptionValue::String(s) if s.starts_with("wf:"))), Some(Module::Workflows))
    }
}
