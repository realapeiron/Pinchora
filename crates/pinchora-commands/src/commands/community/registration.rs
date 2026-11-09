//! Domain-owned command registration. Regenerate compatibility tables with `cargo xtask registrations`.

use crate::commands::registration::{commands, compatibility};

commands! {
    typed {
        crate::commands::community::engagement::CommunityAdmin,
        crate::commands::community::tools::HighlightCommand,
        crate::commands::community::tools::ChecklistCommand,
        crate::commands::community::tools::AutopostCommand,
        crate::commands::community::tools::AnniversaryCommand,
        crate::commands::community::tools::ConfessCommand,
        crate::commands::community::engagement::QotdCommand,
        crate::commands::community::engagement::CountingCommand,
        crate::commands::community::engagement::FormCommand,
        crate::commands::community::engagement::ModmailCommand,
        crate::commands::community::periodic::Weekly,
        crate::commands::community::periodic::Monthly,
        crate::commands::community::plaza::PlazaCommand,
        crate::commands::community::partner::PartnerCommand,
        crate::commands::community::remind::RemindCommand,
        crate::commands::community::giveaway::GiveawayCommand,
        crate::commands::community::suggest::Suggest,
        crate::commands::community::suggest::SuggestionCommand,
        crate::commands::community::poll::Poll,
        crate::commands::community::tag::Tag,
        crate::commands::community::starboard::StarboardCommand,
        crate::commands::community::tickets::TicketCommand,
        crate::commands::community::events::EventCommand,
        crate::commands::community::tickets::TicketsCommand,
        crate::commands::community::feeds::FeedsCommand,
        crate::commands::community::invites::InvitesCommand,
        crate::commands::community::invites::InviteAdmin,
        crate::commands::community::birthdays::BirthdayCommand,
        crate::commands::community::birthdays::BirthdaysAdmin,
        crate::commands::community::trackers::GameCommand,
        crate::commands::community::trackers::TrackersAdmin,
    }
    context {
        ("Open ticket", crate::commands::community::tickets_context),
    }
}

compatibility! {
    components {
        Some("bd") => crate::commands::community::birthdays::on_component(cx, parts).await,
        Some("gt") => crate::commands::community::trackers::on_component(cx, parts).await,
        Some("pe") => house::on_edit(cx, "pe", parts, values.first().map(String::as_str)).await,
        Some("pd") => plaza::on_donate(cx, values.first().map(String::as_str)).await,
        Some("pt") => partner::on_component(cx, parts, values.first().map(String::as_str)).await,
        Some("rm") => remind::on_component(cx, parts, values.first().map(String::as_str)).await,
        Some("gw") => giveaway::on_component(cx, parts).await,
        Some("cv") => crate::commands::community::tools::on_confession_button(cx, parts).await,
        Some("sg") => suggest::on_vote(cx, parts).await,
        Some("pl") => poll::on_component(cx, parts).await,
        Some("sn") => crate::commands::community::feeds::on_component(cx, parts).await,
        Some("in") => crate::commands::community::invites::on_component(cx, parts).await,
        Some("tl") => crate::commands::community::invites::on_temp_component(cx, parts).await,
        Some("sr") => crate::commands::community::starboard::on_component(cx, parts).await,
        Some("ev") => crate::commands::community::events::on_component(cx, parts).await,
        Some("fm") => crate::commands::community::engagement::on_component(cx, parts).await,
        Some("qd") => crate::commands::community::engagement::on_qotd_decision(cx, parts).await,
        Some("mml") if parts.next() == Some("g") => {
            crate::commands::community::engagement::on_mail_select(cx, values).await
        }
        Some("tk") => crate::community::tickets::on_open(cx, parts, values).await,
        Some("tp") => crate::community::tickets::on_control(cx, parts).await,
        Some("tr") => crate::community::tickets::on_rating(cx, parts).await,
    }
    modals {
        Some("ev") => crate::commands::community::events::on_modal(cx, parts, fields).await,
        Some("fm") => crate::commands::community::engagement::on_modal(cx, parts, fields).await,
        Some("tf") => crate::community::tickets::on_form(cx, parts, fields).await,
    }
    owners {
        "pe" => "plaza edit",
        "pd" => "plaza donate",
        "pt" => "partner",
        "gw" => "giveaway",
        "cv" => "confess",
        "sg" => "suggest",
        "pl" => "poll",
        "tk" | "tp" | "tf" | "tr" => "ticket",
        "sn" => "feeds list",
        "gt" => "game lol profile",
        "sr" => "starboard top",
        "in" => "invites top",
        "tl" => "invites temp",
        "rm" => "remind",
        "bd" => "birthday set",
        "ev" => "event",
        "qd" => "qotd queue",
        "fm" => "form open",
        "mml" => "modmail close",
    }
    owner_actions {
        ("ev", "c") => Some("event create"),
        ("ev", "p") => Some("event attendees"),
        ("fm", "a" | "d") => Some("form post"),
        ("pt", "leave" | "stay") => Some("partner leave"),
        ("tp", "cl" | "cy") => Some("ticket close"),
        ("tp", "cm") => Some("ticket claim"),
        ("tp", "tr") => Some("ticket transcript"),
        ("tp", "ro") => Some("ticket reopen"),
    }

    autocomplete {
        "birthday" | "birthdays" => crate::commands::community::birthdays::autocomplete(&command),
        "tag" => crate::commands::community::tag::autocomplete(&cx, &command).await,
    }

    command_gates {
        "weekly" => Self::Economy,
        "monthly" => Self::Economy,
        "plaza" => Self::Plaza,
        "partner" => Self::Social,
        "giveaway" => Self::Giveaways,
        "tag" => Self::CustomCommands,
        "suggest" => Self::Suggestions,
        "suggestion" => Self::Suggestions,
        "poll" => Self::Polls,
        "feeds" => Self::Feeds,
        "game" => Self::GameTrackers,
        "trackers" => Self::GameTrackers,
        "ticket" => Self::Tickets,
        "tickets" => Self::Tickets,
        "Open ticket" => Self::Tickets,
        "starboard" => Self::Starboard,
        "invites" => Self::Invites,
        "inviteadmin" => Self::Invites,
        "highlight" => Self::Utility,
        "checklist" => Self::Utility,
        "autopost" => Self::Utility,
        "anniversary" => Self::Birthdays,
        "confess" => Self::Modmail,
        "birthday" => Self::Birthdays,
        "birthdays" => Self::Birthdays,
        "event" => Self::Events,
        "community" => match sub {
                Some("qotd") => Self::Qotd,
                Some("counting") => Self::Counting,
                Some("forms") => Self::Forms,
                Some("modmail") => Self::Modmail,
                _ => return None,
            },
        "qotd" => Self::Qotd,
        "counting" => Self::Counting,
        "form" => Self::Forms,
        "modmail" => Self::Modmail,
    }
    component_gates {
        "pe" => Self::Plaza,
        "pd" => Self::Plaza,
        "pt" => Self::Social,
        "gw" => Self::Giveaways,
        "sg" => Self::Suggestions,
        "pl" => Self::Polls,
        "tk" => Self::Tickets,
        "tp" => Self::Tickets,
        "tf" => Self::Tickets,
        "tr" => Self::Tickets,
        "sn" => Self::Feeds,
        "sr" => Self::Starboard,
        "in" => Self::Invites,
        "tl" => Self::Invites,
        "bd" => Self::Birthdays,
        "ev" => Self::Events,
        "qd" => Self::Qotd,
        "ct" => Self::Counting,
        "fm" => Self::Forms,
        "mml" => Self::Modmail,
        "cv" => Self::Modmail,
        "gt" => Self::GameTrackers,
        "mc" => Self::GameTrackers,
    }

    route_roots {
        (
            "community",
            "Configure community tools",
            Some(Permissions::MANAGE_GUILD),
        ),
        ("rewards", "Claim your weekly and monthly rewards", None),
    }
}
