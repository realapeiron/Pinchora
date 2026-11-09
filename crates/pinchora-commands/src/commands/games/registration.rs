//! Domain-owned command registration. Regenerate compatibility tables with `cargo xtask registrations`.

use crate::commands::registration::{commands, compatibility};

commands! {
    typed {
        33 => crate::commands::games::fish::FishCommand,
        35 => crate::commands::games::duel::DuelCommand,
        46 => crate::commands::games::play::PlayCommand,
        60 => crate::commands::games::collection::dex::Dex,
        61 => crate::commands::games::collection::beachcomb::Beachcomb,
        62 => crate::commands::games::collection::crabpot::CrabPot,
        63 => crate::commands::games::collection::drops::CritterDrop,
        65 => crate::commands::games::party_games::PartyCommand,
    }
    context {
    }
}

compatibility! {
    components {
        33 { Some("fr") => fish::on_reel(cx, parts).await, }
        34 { Some("fx") => fish::on_gear(cx, parts, values.first().map(String::as_str)).await, }
        35 { Some("fz") => fish::on_bucket(cx, parts).await, }
        40 { Some("tt") => duel::on_move(cx, parts).await, }
        55 { Some("c4") => connect4::on_button(cx, parts).await, }
        56 { Some("bs") => board::fleet::on_component(cx, parts, values).await, }
        57 { Some("ck") => board::checkers::on_component(cx, parts, values).await, }
        58 { Some("cc") => board::chess::on_component(cx, parts, values).await, }
        59 { Some("of") => board::othello::on_component(cx, parts).await, }
        60 { Some("pz") => board::picross::on_component(cx, parts, values).await, }
        61 { Some("tz") => board::tide2048::on_button(cx, parts).await, }
        62 { Some(prefix @ ("mm" | "mi" | "yl" | "ag" | "yt" | "gn")) => {
            quick::on_button(cx, quick::Kind::from_prefix(prefix).expect("quick prefix"), parts).await
        } }
        63 { Some("rp") => rps::on_button(cx, parts).await, }
        64 { Some("hm") => hangman::on_pick(cx, parts, values.first().map(String::as_str)).await, }
        65 { Some("wo") => wordle::on_button(cx, parts).await, }
        66 { Some("tv") => trivia::on_answer(cx, parts).await, }
        78 { Some("cd") => collection::drops::on_grab(cx, parts).await, }
        79 { Some("mb") => collection::drops::on_open(cx, parts).await, }
        80 { Some("bc") => collection::dex::on_component(cx, parts, values.first().map(String::as_str)).await, }
        81 { Some("kp") => collection::crabpot::on_component(cx, parts).await, }
        82 { Some("dy") => derby::on_button(cx, parts).await, }
        83 { Some("sh") => share::on_button(cx, parts).await, }
        84 { Some("sz") => shells::on_button(cx, parts).await, }
        86 { Some("hs") => heist::on_button(cx, parts).await, }
        88 { Some(prefix @ ("d9p" | "d9s" | "d9c")) => {
            crate::commands::games::reef_party::on_component(cx, prefix, parts).await
        } }
        89 { Some(prefix @ ("hl" | "ac" | "cn" | "wx" | "wy" | "td" | "ni")) => {
            party_games::on_component(cx, prefix, parts).await
        } }
    }
    modals {
        0 { Some("cd") => collection::drops::on_word(cx, parts, fields).await, }
        6 { Some("pz") if parts.next() == Some("m") => board::picross::on_modal(cx, parts, fields).await, }
        7 { Some(prefix @ ("d9p" | "d9s" | "d9c")) => {
            crate::commands::games::reef_party::on_modal(cx, prefix, parts, fields).await
        } }
        8 { Some(prefix @ ("hl" | "ac" | "cn" | "wx")) => party_games::on_modal(cx, prefix, parts, fields).await, }
        10 { Some("wo") if parts.next() == Some("m") => {
            let text = fields
                .iter()
                .find(|(id, _)| id == wordle::GUESS_FIELD)
                .map_or("", |(_, value)| value.as_str());
            wordle::on_guess(cx, parts, text).await
        } }
        11 { Some("of") => board::othello::on_modal(cx, parts, fields).await, }
        12 { Some(prefix @ ("mm" | "mi" | "yl" | "ag" | "yt" | "gn")) => {
            quick::on_modal(
                cx,
                quick::Kind::from_prefix(prefix).expect("quick prefix"),
                parts,
                fields,
            )
            .await
        } }
    }
    owners {
        20 { "fr" => "fish cast", }
        21 { "fx" => "fish gear", }
        22 { "fz" => "fish sell", }
        24 { "tt" => "duel tictactoe", }
        25 { "c4" => "duel connect4", }
        26 { "rp" => "duel rps", }
        27 { "bs" => "duel seafleet", }
        28 { "ck" => "duel checkers", }
        29 { "cc" => "duel chess", }
        30 { "of" => "duel othello", }
        31 { "sh" => "duel share", }
        32 { "pz" => "play picross", }
        33 { "tz" => "play 2048", }
        34 { "hm" => "play hangman", }
        35 { "wo" => "play wordle", }
        36 { "tv" => "play trivia", }
        37 { "mm" => "play memory", }
        38 { "mi" => "play minesweeper", }
        39 { "yl" => "play higherlower", }
        40 { "ag" => "play anagram", }
        41 { "yt" => "play typing", }
        42 { "gn" => "play guessnumber", }
        43 { "dy" => "play derby", }
        44 { "sz" => "play shells", }
        45 { "hs" => "play heist", }
        47 { "hl" => "party hotpearl", }
        48 { "ac" => "party acronym", }
        49 { "cn" => "party postcard", }
        50 { "wx" => "party wordrace", }
        51 { "wy" => "party wouldyourrather", }
        52 { "td" => "party truthordare", }
        53 { "ni" => "party nhie", }
        54 { "cd" | "mb" => "critterdrop", }
        55 { "bc" => "dex", }
        56 { "kp" => "crabpot", }
    }
    owner_actions {
        4 { ("fx", "p") => Some("fish shop"), }
        5 { ("fz", "b") => Some("fish book"), }
    }
    private {
        1 { "fish dock", }
        2 { "fish contest", }
        3 { "fish season", }
        4 { "fish dive", }
        16 { "beachcomb", }
        24 { "crabpot", }
        29 { "dex", }
        37 { "fish gear", }
        38 { "fish sell", }
        39 { "fish shop", }
        59 { "party stop", }
        65 { "play picross", }
        66 { "play wordle", }
        67 { "play 2048", }
        68 { "play memory", }
        69 { "play minesweeper", }
        70 { "play higherlower", }
        71 { "play anagram", }
        72 { "play typing", }
        73 { "play guessnumber", }
    }
    opens_modal {
        0 { "cd:w", }
        1 { "d9p:answer", }
        2 { "d9s:answer", }
        3 { "d9c:answer", }
        7 { "pz:m", }
        8 { "wo:g", }
        10 { "hl:w", }
        11 { "ac:w", }
        12 { "cn:w", }
        13 { "wx:g", }
        25 { "mi:coord", }
        26 { "ag:answer", }
        27 { "yt:answer", }
        28 { "gn:answer", }
        29 { "of:m", }
    }
    slash_only {
        13 { "play 2048", }
        14 { "play picross", }
        15 { "play wordle", }
    }
    help_crowd {
        0 { "play derby", }
        1 { "play heist", }
        2 { "play shells", }
        4 { "beachcomb", }
        5 { "crabpot", }
        6 { "critterdrop", }
        7 { "dex", }
    }
    help_puzzles {
        1 { "play hangman", }
        2 { "play wordle", }
        3 { "play trivia", }
        4 { "play picross", }
        5 { "play 2048", }
        6 { "play memory", }
        7 { "play minesweeper", }
        8 { "play higherlower", }
        9 { "play anagram", }
        10 { "play typing", }
        11 { "play guessnumber", }
        12 { "play reefmind", }
        13 { "play kelpmaze", }
        14 { "play gemtide", }
        15 { "party", }
    }
    help_duels {
        0 { "duel", }
    }
    help_fishing {
        0 { "fish", }
    }
    help_profile {
        7 { "play stats", }
    }
    ephemeral {
        7 { "dex", }
    }
    routes {
        39 { ("beachcomb", "beachcomb", 0) }
        131 { ("crabpot", "crabpot", 0) }
        132 { ("critterdrop", "critterdrop", 0) }
        144 { ("dex", "dex", 0) }
        145 { ("duel checkers", "duel checkers", 0) }
        146 { ("duel chess", "duel chess", 0) }
        147 { ("duel connect4", "duel connect4", 0) }
        148 { ("duel othello", "duel othello", 0) }
        149 { ("duel rps", "duel rps", 0) }
        150 { ("duel seafleet", "duel seafleet", 0) }
        151 { ("duel share", "duel share", 0) }
        152 { ("duel tictactoe", "duel tictactoe", 0) }
        177 { ("fish book", "fish book", 0) }
        178 { ("fish cast", "fish cast", 0) }
        179 { ("fish contest", "fish contest", 0) }
        180 { ("fish dive", "fish dive", 0) }
        181 { ("fish dock", "fish dock", 0) }
        182 { ("fish gear", "fish gear", 0) }
        183 { ("fish season", "fish season", 0) }
        184 { ("fish sell", "fish sell", 0) }
        185 { ("fish shop", "fish shop", 0) }
        268 { ("party acronym", "party acronym", 0) }
        269 { ("party crittercompass", "party crittercompass", 0) }
        270 { ("party driftstory", "party driftstory", 0) }
        271 { ("party fun", "party fun", 0) }
        272 { ("party hotpearl", "party hotpearl", 0) }
        273 { ("party lastreef", "party lastreef", 0) }
        274 { ("party nhie", "party nhie", 0) }
        275 { ("party postcard", "party postcard", 0) }
        276 { ("party reefquiz", "party reefquiz", 0) }
        277 { ("party roomwake", "party roomwake", 0) }
        278 { ("party stop", "party stop", 0) }
        279 { ("party truthordare", "party truthordare", 0) }
        280 { ("party wordrace", "party wordrace", 0) }
        281 { ("party wordwake", "party wordwake", 0) }
        282 { ("party wouldyourrather", "party wouldyourrather", 0) }
        293 { ("play 2048", "play 2048", 0) }
        294 { ("play anagram", "play anagram", 0) }
        295 { ("play derby", "play derby", 0) }
        296 { ("play gemtide", "play gemtide", 0) }
        297 { ("play guessnumber", "play guessnumber", 0) }
        298 { ("play hangman", "play hangman", 0) }
        299 { ("play heist", "play heist", 0) }
        300 { ("play higherlower", "play higherlower", 0) }
        301 { ("play kelpmaze", "play kelpmaze", 0) }
        302 { ("play memory", "play memory", 0) }
        303 { ("play minesweeper", "play minesweeper", 0) }
        304 { ("play picross", "play picross", 0) }
        305 { ("play reefmind", "play reefmind", 0) }
        306 { ("play shells", "play shells", 0) }
        307 { ("play stats", "play stats", 0) }
        308 { ("play trivia", "play trivia", 0) }
        309 { ("play typing", "play typing", 0) }
        310 { ("play wordle", "play wordle", 0) }
    }
    legacy_leaves {
        39 { "beachcomb" }
        131 { "crabpot" }
        132 { "critterdrop" }
        144 { "dex" }
        145 { "duel checkers" }
        146 { "duel chess" }
        147 { "duel connect4" }
        148 { "duel othello" }
        149 { "duel rps" }
        150 { "duel seafleet" }
        151 { "duel share" }
        152 { "duel tictactoe" }
        177 { "fish book" }
        178 { "fish cast" }
        179 { "fish contest" }
        180 { "fish dive" }
        181 { "fish dock" }
        182 { "fish gear" }
        183 { "fish season" }
        184 { "fish sell" }
        185 { "fish shop" }
        268 { "party acronym" }
        269 { "party crittercompass" }
        270 { "party driftstory" }
        271 { "party fun" }
        272 { "party hotpearl" }
        273 { "party lastreef" }
        274 { "party nhie" }
        275 { "party postcard" }
        276 { "party reefquiz" }
        277 { "party roomwake" }
        278 { "party stop" }
        279 { "party truthordare" }
        280 { "party wordrace" }
        281 { "party wordwake" }
        282 { "party wouldyourrather" }
        293 { "play 2048" }
        294 { "play anagram" }
        295 { "play derby" }
        296 { "play gemtide" }
        297 { "play guessnumber" }
        298 { "play hangman" }
        299 { "play heist" }
        300 { "play higherlower" }
        301 { "play kelpmaze" }
        302 { "play memory" }
        303 { "play minesweeper" }
        304 { "play picross" }
        305 { "play reefmind" }
        306 { "play shells" }
        307 { "play stats" }
        308 { "play trivia" }
        309 { "play typing" }
        310 { "play wordle" }
    }
    command_gates {
        500 { "fish" => Self::Fishing, }
        2300 { "dex" => Self::Collection, }
        2301 { "beachcomb" => Self::Collection, }
        2302 { "crabpot" => Self::Collection, }
        2303 { "critterdrop" => Self::Collection, }
        2500 { "party" => Self::PartyGames, }
        3300 { "duel" => match sub {
                Some("share") => Self::Party,
                Some("tictactoe" | "connect4" | "rps") => Self::Duels,
                Some("seafleet" | "checkers" | "chess" | "othello") => Self::BoardGames,
                _ => return None,
            }, }
        3400 { "play" => match sub {
                Some(
                    "hangman" | "wordle" | "trivia" | "memory" | "minesweeper" | "higherlower" | "anagram" | "typing"
                    | "guessnumber" | "kelpmaze" | "gemtide",
                ) => Self::Puzzles,
                Some("picross" | "2048" | "reefmind") => Self::BoardGames,
                Some("derby" | "heist" | "shells") => Self::Party,
                Some("stats") => Self::Profile,
                _ => return None,
            }, }
    }
    component_gates {
        500 { "fr" => Self::Fishing, }
        501 { "fx" => Self::Fishing, }
        502 { "fz" => Self::Fishing, }
        700 { "tt" => Self::Duels, }
        701 { "c4" => Self::Duels, }
        702 { "rp" => Self::Duels, }
        800 { "bs" => Self::BoardGames, }
        801 { "ck" => Self::BoardGames, }
        802 { "cc" => Self::BoardGames, }
        803 { "of" => Self::BoardGames, }
        804 { "pz" => Self::BoardGames, }
        805 { "tz" => Self::BoardGames, }
        900 { "d9c" => Self::BoardGames, }
        1000 { "d9s" => Self::Puzzles, }
        1100 { "d9p" => Self::PartyGames, }
        1200 { "hm" => Self::Puzzles, }
        1201 { "wo" => Self::Puzzles, }
        1202 { "tv" => Self::Puzzles, }
        1203 { "mm" => Self::Puzzles, }
        1204 { "mi" => Self::Puzzles, }
        1205 { "yl" => Self::Puzzles, }
        1206 { "ag" => Self::Puzzles, }
        1207 { "yt" => Self::Puzzles, }
        1208 { "gn" => Self::Puzzles, }
        1300 { "dy" => Self::Party, }
        1301 { "sh" => Self::Party, }
        1302 { "sz" => Self::Party, }
        1304 { "hs" => Self::Party, }
        1400 { "hl" => Self::PartyGames, }
        1401 { "ac" => Self::PartyGames, }
        1402 { "cn" => Self::PartyGames, }
        1403 { "wx" => Self::PartyGames, }
        1404 { "wy" => Self::PartyGames, }
        1405 { "td" => Self::PartyGames, }
        1406 { "ni" => Self::PartyGames, }
        1500 { "cd" => Self::Collection, }
        1501 { "mb" => Self::Collection, }
        1502 { "bc" => Self::Collection, }
        1503 { "kp" => Self::Collection, }
    }
    help_areas {
        6 { "fishing", }
        8 { "duels", }
        9 { "puzzles", }
        10 { "crowd", }
    }
}
