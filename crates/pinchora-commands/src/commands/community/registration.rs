//! Domain-owned command registration. Regenerate compatibility tables with `cargo xtask registrations`.

use crate::commands::registration::{commands, compatibility};

commands! {
    typed {
        2 => crate::commands::community::engagement::CommunityAdmin,
        3 => crate::commands::community::tools::HighlightCommand,
        4 => crate::commands::community::tools::ChecklistCommand,
        5 => crate::commands::community::tools::AutopostCommand,
        6 => crate::commands::community::tools::AnniversaryCommand,
        7 => crate::commands::community::tools::ConfessCommand,
        8 => crate::commands::community::engagement::QotdCommand,
        9 => crate::commands::community::engagement::CountingCommand,
        10 => crate::commands::community::engagement::FormCommand,
        11 => crate::commands::community::engagement::ModmailCommand,
        21 => crate::commands::community::periodic::Weekly,
        22 => crate::commands::community::periodic::Monthly,
        38 => crate::commands::community::plaza::PlazaCommand,
        47 => crate::commands::community::partner::PartnerCommand,
        53 => crate::commands::community::remind::RemindCommand,
        56 => crate::commands::community::giveaway::GiveawayCommand,
        57 => crate::commands::community::suggest::Suggest,
        58 => crate::commands::community::suggest::SuggestionCommand,
        59 => crate::commands::community::poll::Poll,
        82 => crate::commands::community::tag::Tag,
        86 => crate::commands::community::starboard::StarboardCommand,
        88 => crate::commands::community::tickets::TicketCommand,
        89 => crate::commands::community::events::EventCommand,
        90 => crate::commands::community::tickets::TicketsCommand,
        98 => crate::commands::community::feeds::FeedsCommand,
        102 => crate::commands::community::invites::InvitesCommand,
        103 => crate::commands::community::invites::InviteAdmin,
        106 => crate::commands::community::birthdays::BirthdayCommand,
        107 => crate::commands::community::birthdays::BirthdaysAdmin,
        111 => crate::commands::community::trackers::GameCommand,
        112 => crate::commands::community::trackers::TrackersAdmin,
    }
    context {
        114 => ("Open ticket", crate::commands::community::tickets_context),
    }
}

compatibility! {
    components {
        11 { Some("bd") => crate::commands::community::birthdays::on_component(cx, parts).await, }
        17 { Some("gt") => crate::commands::community::trackers::on_component(cx, parts).await, }
        19 { Some("pe") => house::on_edit(cx, "pe", parts, values.first().map(String::as_str)).await, }
        21 { Some("pd") => plaza::on_donate(cx, values.first().map(String::as_str)).await, }
        67 { Some("pt") => partner::on_component(cx, parts, values.first().map(String::as_str)).await, }
        71 { Some("rm") => remind::on_component(cx, parts, values.first().map(String::as_str)).await, }
        73 { Some("gw") => giveaway::on_component(cx, parts).await, }
        74 { Some("cv") => crate::commands::community::tools::on_confession_button(cx, parts).await, }
        75 { Some("sg") => suggest::on_vote(cx, parts).await, }
        77 { Some("pl") => poll::on_component(cx, parts).await, }
        87 { Some("sn") => crate::commands::community::feeds::on_component(cx, parts).await, }
        90 { Some("in") => crate::commands::community::invites::on_component(cx, parts).await, }
        91 { Some("tl") => crate::commands::community::invites::on_temp_component(cx, parts).await, }
        93 { Some("sr") => crate::commands::community::starboard::on_component(cx, parts).await, }
        95 { Some("ev") => crate::commands::community::events::on_component(cx, parts).await, }
        96 { Some("fm") => crate::commands::community::engagement::on_component(cx, parts).await, }
        97 { Some("qd") => crate::commands::community::engagement::on_qotd_decision(cx, parts).await, }
        98 { Some("mml") if parts.next() == Some("g") => {
            crate::commands::community::engagement::on_mail_select(cx, values).await
        } }
        99 { Some("tk") => crate::community::tickets::on_open(cx, parts, values).await, }
        100 { Some("tp") => crate::community::tickets::on_control(cx, parts).await, }
        101 { Some("tr") => crate::community::tickets::on_rating(cx, parts).await, }
    }
    modals {
        13 { Some("ev") => crate::commands::community::events::on_modal(cx, parts, fields).await, }
        14 { Some("fm") => crate::commands::community::engagement::on_modal(cx, parts, fields).await, }
        15 { Some("tf") => crate::community::tickets::on_form(cx, parts, fields).await, }
    }
    owners {
        6 { "pe" => "plaza edit", }
        7 { "pd" => "plaza donate", }
        60 { "pt" => "partner", }
        63 { "gw" => "giveaway", }
        64 { "cv" => "confess", }
        69 { "sg" => "suggest", }
        70 { "pl" => "poll", }
        72 { "tk" | "tp" | "tf" | "tr" => "ticket", }
        76 { "sn" => "feeds list", }
        79 { "gt" => "game lol profile", }
        81 { "sr" => "starboard top", }
        82 { "in" => "invites top", }
        83 { "tl" => "invites temp", }
        89 { "rm" => "remind", }
        93 { "bd" => "birthday set", }
        98 { "ev" => "event", }
        99 { "qd" => "qotd queue", }
        100 { "fm" => "form open", }
        101 { "mml" => "modmail close", }
    }
    owner_actions {
        1 { ("ev", "c") => Some("event create"), }
        2 { ("ev", "p") => Some("event attendees"), }
        3 { ("fm", "a" | "d") => Some("form post"), }
        24 { ("pt", "leave" | "stay") => Some("partner leave"), }
        25 { ("tp", "cl" | "cy") => Some("ticket close"), }
        26 { ("tp", "cm") => Some("ticket claim"), }
        27 { ("tp", "tr") => Some("ticket transcript"), }
        28 { ("tp", "ro") => Some("ticket reopen"), }
    }
    private {
        9 { "partner streak", }
        14 { "birthday", }
        15 { "birthdays", }
        30 { "feeds", }
        31 { "event list", }
        32 { "event attendees", }
        33 { "event remind", }
        34 { "event template", }
        35 { "event edit", }
        36 { "event cancel", }
        42 { "giveaway", }
        43 { "highlight", }
        44 { "checklist", }
        45 { "autopost", }
        46 { "anniversary", }
        47 { "confess", }
        50 { "invites temp", }
        51 { "inviteadmin", }
        58 { "partner leave", }
        63 { "plaza donate", }
        64 { "plaza edit", }
        74 { "poll", }
        81 { "remind", }
        99 { "suggest", }
        100 { "suggestion", }
        102 { "ticket", }
        103 { "tickets", }
        112 { "starboard", }
        117 { "qotd", }
        118 { "counting", }
        119 { "form list", }
        120 { "modmail", }
        121 { "community qotd", }
        122 { "community counting", }
        123 { "community forms", }
        124 { "community modmail", }
        127 { "game lol unlink", }
        128 { "trackers", }
    }
    opens_form {
        2 { "event create", }
        3 { "form open", }
    }
    opens_modal {
        14 { "tk:o", }
        15 { "tk:s", }
        16 { "tk:c", }
        17 { "tk:f", }
        23 { "ev:c", }
        24 { "fm:o", }
    }
    slash_only {
        0 { "confess", }
        1 { "checklist", }
        2 { "highlight", }
        3 { "remind", }
        7 { "community", }
    }
    help_modules {
        5 { "birthdays", }
    }
    help_community {
        0 { "poll", }
        1 { "suggest", }
        2 { "suggestion", }
        3 { "giveaway", }
        4 { "highlight", }
        5 { "checklist", }
        6 { "autopost", }
        7 { "anniversary", }
        8 { "confess", }
        12 { "tag", }
        16 { "feeds", }
        17 { "trackers", }
        21 { "event", }
        22 { "qotd", }
        23 { "counting", }
        24 { "form", }
        25 { "modmail", }
        26 { "community qotd", }
        27 { "community counting", }
        28 { "community forms", }
        29 { "community modmail", }
        30 { "ticket", }
        31 { "tickets", }
        32 { "Open ticket", }
        33 { "starboard", }
        34 { "invites", }
        35 { "inviteadmin", }
    }
    help_puzzles {
        0 { "game", }
    }
    help_friends {
        1 { "partner", }
    }
    help_house {
        1 { "plaza", }
    }
    help_rewards {
        1 { "weekly", }
        2 { "monthly", }
        5 { "remind", }
        6 { "birthday", }
    }
    community_admin_paths {
        0 { "qotd settings", }
        1 { "qotd queue", }
        2 { "counting setup", }
        3 { "counting reset", }
        4 { "form create", }
        5 { "form post", }
        6 { "form close", }
        7 { "modmail settings", }
    }
    admin_group_moves {
        4 { ("setup tickets", "community tickets"), }
        5 { ("setup starboard", "community starboard"), }
        6 { ("setup events", "community events"), }
        7 { ("setup birthdays", "community birthdays"), }
        8 { ("setup utility", "community utility"), }
        9 { ("setup community", "community server"), }
    }
    never_gated {
        5 { "remind", }
    }
    never_gated_prefixes {
        2 { "rm", }
    }
    protected {
        16 { "remind", }
    }
    autocomplete {
        0 { "birthday" | "birthdays" => crate::commands::community::birthdays::autocomplete(&command), }
        1 { "tag" => crate::commands::community::tag::autocomplete(&cx, &command).await, }
    }
    routes {
        0 { ("Open ticket", "Open ticket", 0) }
        9 { ("anniversary off", "anniversary off", 20) }
        10 { ("anniversary set", "anniversary set", 20) }
        26 { ("autopost list", "autopost list", 20) }
        27 { ("autopost remove", "autopost remove", 20) }
        28 { ("autopost set", "autopost set", 20) }
        41 { ("birthday list", "birthday list", 0) }
        42 { ("birthday me", "birthday me", 0) }
        43 { ("birthday next", "birthday next", 0) }
        44 { ("birthday remove", "birthday remove", 0) }
        45 { ("birthday set", "birthday set", 0) }
        46 { ("birthday view", "birthday view", 0) }
        47 { ("birthdays preview", "community birthdays preview", 8) }
        48 { ("birthdays settings", "community birthdays settings", 8) }
        54 { ("checklist add", "checklist add", 20) }
        55 { ("checklist done", "checklist done", 20) }
        56 { ("checklist list", "checklist list", 20) }
        57 { ("checklist remove", "checklist remove", 20) }
        67 { ("community counting reset", "counting reset", 18) }
        68 { ("community counting setup", "counting setup", 18) }
        69 { ("community forms close", "form close", 18) }
        70 { ("community forms create", "form create", 18) }
        71 { ("community forms post", "form post", 18) }
        72 { ("community modmail settings", "modmail settings", 18) }
        73 { ("community qotd queue", "qotd queue", 18) }
        74 { ("community qotd settings", "qotd settings", 18) }
        75 { ("confess ban", "confess ban", 0) }
        76 { ("confess report", "confess report", 0) }
        77 { ("confess reveal", "confess reveal", 20) }
        78 { ("confess review", "confess review", 20) }
        79 { ("confess settings", "confess settings", 20) }
        80 { ("confess submit", "confess submit", 20) }
        130 { ("counting stats", "counting stats", 0) }
        165 { ("event attendees", "event attendees", 0) }
        166 { ("event cancel", "event cancel", 0) }
        167 { ("event create", "event create", 0) }
        168 { ("event edit", "event edit", 0) }
        169 { ("event list", "event list", 0) }
        170 { ("event remind", "event remind", 0) }
        171 { ("event template", "community events template", 8) }
        172 { ("feeds add", "integrations feeds add", 7) }
        173 { ("feeds edit", "integrations feeds edit", 7) }
        174 { ("feeds list", "integrations feeds list", 7) }
        175 { ("feeds remove", "integrations feeds remove", 7) }
        176 { ("feeds test", "integrations feeds test", 7) }
        186 { ("form list", "form list", 0) }
        187 { ("form open", "form open", 0) }
        191 { ("game lol link", "game lol link", 0) }
        192 { ("game lol profile", "game lol profile", 0) }
        193 { ("game lol unlink", "game lol unlink", 0) }
        194 { ("game mc news", "game mc news", 0) }
        195 { ("game mc status", "game mc status", 0) }
        196 { ("game mc trivia", "game mc trivia", 0) }
        197 { ("game valorant profile", "game valorant profile", 0) }
        200 { ("giveaway end", "giveaway end", 0) }
        201 { ("giveaway list", "giveaway list", 0) }
        202 { ("giveaway reroll", "giveaway reroll", 0) }
        203 { ("giveaway start", "giveaway start", 0) }
        206 { ("highlight add", "highlight add", 20) }
        207 { ("highlight list", "highlight list", 20) }
        208 { ("highlight remove", "highlight remove", 20) }
        236 { ("inviteadmin bonus-add", "integrations invites bonus-add", 7) }
        237 { ("inviteadmin bonus-remove", "integrations invites bonus-remove", 7) }
        238 { ("inviteadmin post", "integrations invites post", 7) }
        239 { ("inviteadmin reward-add", "integrations invites reward-add", 7) }
        240 { ("inviteadmin reward-remove", "integrations invites reward-remove", 7) }
        241 { ("inviteadmin settings", "integrations invites settings", 7) }
        242 { ("invites temp", "invites temp", 0) }
        243 { ("invites top", "invites top", 0) }
        244 { ("invites user", "invites user", 0) }
        249 { ("modmail block", "modmail block", 0) }
        250 { ("modmail close", "modmail close", 0) }
        251 { ("modmail snippet-add", "modmail snippet-add", 0) }
        252 { ("modmail snippet-list", "modmail snippet-list", 0) }
        253 { ("modmail snippet-use", "modmail snippet-use", 0) }
        262 { ("monthly", "rewards monthly", 10) }
        264 { ("partner leave", "partner leave", 0) }
        265 { ("partner propose", "partner propose", 0) }
        266 { ("partner streak", "partner streak", 0) }
        267 { ("partner view", "partner view", 0) }
        311 { ("plaza donate", "plaza donate", 0) }
        312 { ("plaza edit", "community server plaza-edit", 8) }
        313 { ("plaza view", "plaza view", 0) }
        317 { ("poll", "poll", 0) }
        321 { ("qotd suggest", "qotd suggest", 0) }
        328 { ("remind cancel", "remind cancel", 0) }
        329 { ("remind list", "remind list", 0) }
        330 { ("remind me in", "remind me in", 0) }
        331 { ("remind me when", "remind me when", 0) }
        332 { ("remind repeat", "remind repeat", 20) }
        333 { ("remind repeat-cancel", "remind repeat-cancel", 20) }
        334 { ("remind repeat-list", "remind repeat-list", 20) }
        364 { ("starboard edit", "community starboard edit", 7) }
        365 { ("starboard list", "community starboard list", 7) }
        366 { ("starboard remove", "community starboard remove", 7) }
        367 { ("starboard setup", "community starboard setup", 7) }
        368 { ("starboard top", "community starboard top", 7) }
        369 { ("starboard votes add", "community starboard votes-add", 7) }
        370 { ("starboard votes list", "community starboard votes-list", 7) }
        371 { ("starboard votes remove", "community starboard votes-remove", 7) }
        379 { ("suggest", "suggest", 0) }
        380 { ("suggestion approve", "mod suggestion approve", 9) }
        381 { ("suggestion consider", "suggestion consider", 20) }
        382 { ("suggestion deny", "mod suggestion deny", 9) }
        383 { ("suggestion implement", "suggestion implement", 20) }
        384 { ("suggestion note", "suggestion note", 20) }
        385 { ("suggestion thread", "suggestion thread", 20) }
        387 { ("tag", "tag", 0) }
        388 { ("ticket add", "ticket add", 0) }
        389 { ("ticket claim", "ticket claim", 0) }
        390 { ("ticket close", "ticket close", 0) }
        391 { ("ticket escalate", "ticket escalate", 0) }
        392 { ("ticket force-open", "ticket force-open", 0) }
        393 { ("ticket move", "ticket move", 0) }
        394 { ("ticket note", "ticket note", 0) }
        395 { ("ticket priority", "ticket priority", 0) }
        396 { ("ticket remove", "ticket remove", 0) }
        397 { ("ticket rename", "ticket rename", 0) }
        398 { ("ticket reopen", "ticket reopen", 0) }
        399 { ("ticket request-close", "ticket request-close", 0) }
        400 { ("ticket snippet", "ticket snippet", 0) }
        401 { ("ticket transcript", "ticket transcript", 0) }
        402 { ("ticket transfer", "ticket transfer", 0) }
        403 { ("ticket unclaim", "ticket unclaim", 0) }
        404 { ("tickets panel create", "community tickets panel-create", 5) }
        405 { ("tickets panel delete", "community tickets panel-delete", 5) }
        406 { ("tickets panel edit", "community tickets panel-edit", 5) }
        407 { ("tickets panel list", "community tickets panel-list", 5) }
        408 { ("tickets panel post", "community tickets panel-post", 5) }
        409 { ("tickets settings", "community tickets settings", 5) }
        411 { ("trackers mc-news-channel", "integrations trackers mc-news-channel", 19) }
        412 { ("trackers mc-unwatch", "integrations trackers mc-unwatch", 19) }
        413 { ("trackers mc-watch", "integrations trackers mc-watch", 19) }
        414 { ("trackers patch-add", "integrations trackers patch-add", 19) }
        415 { ("trackers patch-list", "integrations trackers patch-list", 19) }
        416 { ("trackers patch-remove", "integrations trackers patch-remove", 19) }
        443 { ("weekly", "rewards weekly", 10) }
    }
    legacy_leaves {
        0 { "Open ticket" }
        9 { "anniversary off" }
        10 { "anniversary set" }
        26 { "autopost list" }
        27 { "autopost remove" }
        28 { "autopost set" }
        41 { "birthday list" }
        42 { "birthday me" }
        43 { "birthday next" }
        44 { "birthday remove" }
        45 { "birthday set" }
        46 { "birthday view" }
        47 { "birthdays preview" }
        48 { "birthdays settings" }
        54 { "checklist add" }
        55 { "checklist done" }
        56 { "checklist list" }
        57 { "checklist remove" }
        67 { "community counting reset" }
        68 { "community counting setup" }
        69 { "community forms close" }
        70 { "community forms create" }
        71 { "community forms post" }
        72 { "community modmail settings" }
        73 { "community qotd queue" }
        74 { "community qotd settings" }
        75 { "confess ban" }
        76 { "confess report" }
        77 { "confess reveal" }
        78 { "confess review" }
        79 { "confess settings" }
        80 { "confess submit" }
        130 { "counting stats" }
        165 { "event attendees" }
        166 { "event cancel" }
        167 { "event create" }
        168 { "event edit" }
        169 { "event list" }
        170 { "event remind" }
        171 { "event template" }
        172 { "feeds add" }
        173 { "feeds edit" }
        174 { "feeds list" }
        175 { "feeds remove" }
        176 { "feeds test" }
        186 { "form list" }
        187 { "form open" }
        191 { "game lol link" }
        192 { "game lol profile" }
        193 { "game lol unlink" }
        194 { "game mc news" }
        195 { "game mc status" }
        196 { "game mc trivia" }
        197 { "game valorant profile" }
        200 { "giveaway end" }
        201 { "giveaway list" }
        202 { "giveaway reroll" }
        203 { "giveaway start" }
        206 { "highlight add" }
        207 { "highlight list" }
        208 { "highlight remove" }
        236 { "inviteadmin bonus-add" }
        237 { "inviteadmin bonus-remove" }
        238 { "inviteadmin post" }
        239 { "inviteadmin reward-add" }
        240 { "inviteadmin reward-remove" }
        241 { "inviteadmin settings" }
        242 { "invites temp" }
        243 { "invites top" }
        244 { "invites user" }
        249 { "modmail block" }
        250 { "modmail close" }
        251 { "modmail snippet-add" }
        252 { "modmail snippet-list" }
        253 { "modmail snippet-use" }
        262 { "monthly" }
        264 { "partner leave" }
        265 { "partner propose" }
        266 { "partner streak" }
        267 { "partner view" }
        311 { "plaza donate" }
        312 { "plaza edit" }
        313 { "plaza view" }
        317 { "poll" }
        321 { "qotd suggest" }
        328 { "remind cancel" }
        329 { "remind list" }
        330 { "remind me in" }
        331 { "remind me when" }
        332 { "remind repeat" }
        333 { "remind repeat-cancel" }
        334 { "remind repeat-list" }
        364 { "starboard edit" }
        365 { "starboard list" }
        366 { "starboard remove" }
        367 { "starboard setup" }
        368 { "starboard top" }
        369 { "starboard votes add" }
        370 { "starboard votes list" }
        371 { "starboard votes remove" }
        379 { "suggest" }
        380 { "suggestion approve" }
        381 { "suggestion consider" }
        382 { "suggestion deny" }
        383 { "suggestion implement" }
        384 { "suggestion note" }
        385 { "suggestion thread" }
        387 { "tag" }
        388 { "ticket add" }
        389 { "ticket claim" }
        390 { "ticket close" }
        391 { "ticket escalate" }
        392 { "ticket force-open" }
        393 { "ticket move" }
        394 { "ticket note" }
        395 { "ticket priority" }
        396 { "ticket remove" }
        397 { "ticket rename" }
        398 { "ticket reopen" }
        399 { "ticket request-close" }
        400 { "ticket snippet" }
        401 { "ticket transcript" }
        402 { "ticket transfer" }
        403 { "ticket unclaim" }
        404 { "tickets panel create" }
        405 { "tickets panel delete" }
        406 { "tickets panel edit" }
        407 { "tickets panel list" }
        408 { "tickets panel post" }
        409 { "tickets settings" }
        411 { "trackers mc-news-channel" }
        412 { "trackers mc-unwatch" }
        413 { "trackers mc-watch" }
        414 { "trackers patch-add" }
        415 { "trackers patch-list" }
        416 { "trackers patch-remove" }
        443 { "weekly" }
    }
    command_gates {
        102 { "weekly" => Self::Economy, }
        103 { "monthly" => Self::Economy, }
        700 { "plaza" => Self::Plaza, }
        1300 { "partner" => Self::Social, }
        1400 { "giveaway" => Self::Giveaways, }
        1500 { "tag" => Self::CustomCommands, }
        1700 { "suggest" => Self::Suggestions, }
        1701 { "suggestion" => Self::Suggestions, }
        1800 { "poll" => Self::Polls, }
        1900 { "feeds" => Self::Feeds, }
        2000 { "game" => Self::GameTrackers, }
        2001 { "trackers" => Self::GameTrackers, }
        2700 { "ticket" => Self::Tickets, }
        2701 { "tickets" => Self::Tickets, }
        2702 { "Open ticket" => Self::Tickets, }
        2800 { "starboard" => Self::Starboard, }
        2900 { "invites" => Self::Invites, }
        2901 { "inviteadmin" => Self::Invites, }
        3003 { "highlight" => Self::Utility, }
        3004 { "checklist" => Self::Utility, }
        3005 { "autopost" => Self::Utility, }
        3100 { "anniversary" => Self::Birthdays, }
        3200 { "confess" => Self::Modmail, }
        4000 { "birthday" => Self::Birthdays, }
        4001 { "birthdays" => Self::Birthdays, }
        4200 { "event" => Self::Events, }
        4300 { "community" => match sub {
                Some("qotd") => Self::Qotd,
                Some("counting") => Self::Counting,
                Some("forms") => Self::Forms,
                Some("modmail") => Self::Modmail,
                _ => return None,
            }, }
        4400 { "qotd" => Self::Qotd, }
        4500 { "counting" => Self::Counting, }
        4600 { "form" => Self::Forms, }
        4700 { "modmail" => Self::Modmail, }
    }
    component_gates {
        200 { "pe" => Self::Plaza, }
        201 { "pd" => Self::Plaza, }
        1900 { "pt" => Self::Social, }
        2000 { "gw" => Self::Giveaways, }
        2300 { "sg" => Self::Suggestions, }
        2400 { "pl" => Self::Polls, }
        2600 { "tk" => Self::Tickets, }
        2601 { "tp" => Self::Tickets, }
        2602 { "tf" => Self::Tickets, }
        2603 { "tr" => Self::Tickets, }
        3200 { "sn" => Self::Feeds, }
        3600 { "sr" => Self::Starboard, }
        3700 { "in" => Self::Invites, }
        3701 { "tl" => Self::Invites, }
        3800 { "bd" => Self::Birthdays, }
        4000 { "ev" => Self::Events, }
        4100 { "qd" => Self::Qotd, }
        4200 { "ct" => Self::Counting, }
        4300 { "fm" => Self::Forms, }
        4400 { "mml" => Self::Modmail, }
        4401 { "cv" => Self::Modmail, }
        4700 { "gt" => Self::GameTrackers, }
        4701 { "mc" => Self::GameTrackers, }
    }
    help_areas {
        11 { "community", }
    }
    admin_roots {
        2 { "community" }
    }
    route_roots {
        2 { (
            "community",
            "Configure community tools",
            Some(Permissions::MANAGE_GUILD),
        ), }
        6 { ("rewards", "Claim your weekly and monthly rewards", None), }
    }
}
