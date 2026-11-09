//! Domain-owned command registration. Regenerate compatibility tables with `cargo xtask registrations`.

use crate::commands::registration::{commands, compatibility};

commands! {
    typed {
        crate::commands::games::fish::FishCommand,
        crate::commands::games::duel::DuelCommand,
        crate::commands::games::play::PlayCommand,
        crate::commands::games::collection::dex::Dex,
        crate::commands::games::collection::beachcomb::Beachcomb,
        crate::commands::games::collection::crabpot::CrabPot,
        crate::commands::games::collection::drops::CritterDrop,
        crate::commands::games::party_games::PartyCommand,
    }
    context {
    }
}

compatibility! {
    components {
        Some("fr") => fish::on_reel(cx, parts).await,
        Some("fx") => fish::on_gear(cx, parts, values.first().map(String::as_str)).await,
        Some("fz") => fish::on_bucket(cx, parts).await,
        Some("tt") => duel::on_move(cx, parts).await,
        Some("c4") => connect4::on_button(cx, parts).await,
        Some("bs") => board::fleet::on_component(cx, parts, values).await,
        Some("ck") => board::checkers::on_component(cx, parts, values).await,
        Some("cc") => board::chess::on_component(cx, parts, values).await,
        Some("of") => board::othello::on_component(cx, parts).await,
        Some("pz") => board::picross::on_component(cx, parts, values).await,
        Some("tz") => board::tide2048::on_button(cx, parts).await,
        Some(prefix @ ("mm" | "mi" | "yl" | "ag" | "yt" | "gn")) => {
            quick::on_button(cx, quick::Kind::from_prefix(prefix).expect("quick prefix"), parts).await
        }
        Some("rp") => rps::on_button(cx, parts).await,
        Some("hm") => hangman::on_pick(cx, parts, values.first().map(String::as_str)).await,
        Some("wo") => wordle::on_button(cx, parts).await,
        Some("tv") => trivia::on_answer(cx, parts).await,
        Some("cd") => collection::drops::on_grab(cx, parts).await,
        Some("mb") => collection::drops::on_open(cx, parts).await,
        Some("bc") => collection::dex::on_component(cx, parts, values.first().map(String::as_str)).await,
        Some("kp") => collection::crabpot::on_component(cx, parts).await,
        Some("dy") => derby::on_button(cx, parts).await,
        Some("sh") => share::on_button(cx, parts).await,
        Some("sz") => shells::on_button(cx, parts).await,
        Some("hs") => heist::on_button(cx, parts).await,
        Some(prefix @ ("d9p" | "d9s" | "d9c")) => {
            crate::commands::games::reef_party::on_component(cx, prefix, parts).await
        }
        Some(prefix @ ("hl" | "ac" | "cn" | "wx" | "wy" | "td" | "ni")) => {
            party_games::on_component(cx, prefix, parts).await
        }
    }
    modals {
        Some("cd") => collection::drops::on_word(cx, parts, fields).await,
        Some("pz") if parts.next() == Some("m") => board::picross::on_modal(cx, parts, fields).await,
        Some(prefix @ ("d9p" | "d9s" | "d9c")) => {
            crate::commands::games::reef_party::on_modal(cx, prefix, parts, fields).await
        }
        Some(prefix @ ("hl" | "ac" | "cn" | "wx")) => party_games::on_modal(cx, prefix, parts, fields).await,
        Some("wo") if parts.next() == Some("m") => {
            let text = fields
                .iter()
                .find(|(id, _)| id == wordle::GUESS_FIELD)
                .map_or("", |(_, value)| value.as_str());
            wordle::on_guess(cx, parts, text).await
        }
        Some("of") => board::othello::on_modal(cx, parts, fields).await,
        Some(prefix @ ("mm" | "mi" | "yl" | "ag" | "yt" | "gn")) => {
            quick::on_modal(
                cx,
                quick::Kind::from_prefix(prefix).expect("quick prefix"),
                parts,
                fields,
            )
            .await
        }
    }
    owners {
        "fr" => "fish cast",
        "fx" => "fish gear",
        "fz" => "fish sell",
        "tt" => "duel tictactoe",
        "c4" => "duel connect4",
        "rp" => "duel rps",
        "bs" => "duel seafleet",
        "ck" => "duel checkers",
        "cc" => "duel chess",
        "of" => "duel othello",
        "sh" => "duel share",
        "pz" => "play picross",
        "tz" => "play 2048",
        "hm" => "play hangman",
        "wo" => "play wordle",
        "tv" => "play trivia",
        "mm" => "play memory",
        "mi" => "play minesweeper",
        "yl" => "play higherlower",
        "ag" => "play anagram",
        "yt" => "play typing",
        "gn" => "play guessnumber",
        "dy" => "play derby",
        "sz" => "play shells",
        "hs" => "play heist",
        "hl" => "party hotpearl",
        "ac" => "party acronym",
        "cn" => "party postcard",
        "wx" => "party wordrace",
        "wy" => "party wouldyourrather",
        "td" => "party truthordare",
        "ni" => "party nhie",
        "cd" | "mb" => "critterdrop",
        "bc" => "dex",
        "kp" => "crabpot",
    }
    owner_actions {
        ("fx", "p") => Some("fish shop"),
        ("fz", "b") => Some("fish book"),
    }

    command_gates {
        "fish" => Self::Fishing,
        "dex" => Self::Collection,
        "beachcomb" => Self::Collection,
        "crabpot" => Self::Collection,
        "critterdrop" => Self::Collection,
        "party" => Self::PartyGames,
        "duel" => match sub {
                Some("share") => Self::Party,
                Some("tictactoe" | "connect4" | "rps") => Self::Duels,
                Some("seafleet" | "checkers" | "chess" | "othello") => Self::BoardGames,
                _ => return None,
            },
        "play" => match sub {
                Some(
                    "hangman" | "wordle" | "trivia" | "memory" | "minesweeper" | "higherlower" | "anagram" | "typing"
                    | "guessnumber" | "kelpmaze" | "gemtide",
                ) => Self::Puzzles,
                Some("picross" | "2048" | "reefmind") => Self::BoardGames,
                Some("derby" | "heist" | "shells") => Self::Party,
                Some("stats") => Self::Profile,
                _ => return None,
            },
    }
    component_gates {
        "fr" => Self::Fishing,
        "fx" => Self::Fishing,
        "fz" => Self::Fishing,
        "tt" => Self::Duels,
        "c4" => Self::Duels,
        "rp" => Self::Duels,
        "bs" => Self::BoardGames,
        "ck" => Self::BoardGames,
        "cc" => Self::BoardGames,
        "of" => Self::BoardGames,
        "pz" => Self::BoardGames,
        "tz" => Self::BoardGames,
        "d9c" => Self::BoardGames,
        "d9s" => Self::Puzzles,
        "d9p" => Self::PartyGames,
        "hm" => Self::Puzzles,
        "wo" => Self::Puzzles,
        "tv" => Self::Puzzles,
        "mm" => Self::Puzzles,
        "mi" => Self::Puzzles,
        "yl" => Self::Puzzles,
        "ag" => Self::Puzzles,
        "yt" => Self::Puzzles,
        "gn" => Self::Puzzles,
        "dy" => Self::Party,
        "sh" => Self::Party,
        "sz" => Self::Party,
        "hs" => Self::Party,
        "hl" => Self::PartyGames,
        "ac" => Self::PartyGames,
        "cn" => Self::PartyGames,
        "wx" => Self::PartyGames,
        "wy" => Self::PartyGames,
        "td" => Self::PartyGames,
        "ni" => Self::PartyGames,
        "cd" => Self::Collection,
        "mb" => Self::Collection,
        "bc" => Self::Collection,
        "kp" => Self::Collection,
    }

}
