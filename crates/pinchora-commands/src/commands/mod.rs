pub(crate) mod achievements;
pub(crate) mod anti_nuke;
pub(crate) mod automod;
mod bio;
pub(crate) mod birthdays;
pub(crate) mod board;
pub(crate) mod branding;
pub(crate) mod character;
pub(crate) mod collection;
pub(crate) mod command_control;
pub(crate) mod community_pack;
pub(crate) mod community_v2;
pub(crate) mod config;
pub(crate) mod connect4;
pub(crate) mod contest;
pub(crate) mod customcommand;
pub(crate) mod customize;
mod daily;
pub(crate) mod decorate;
pub(crate) mod derby;
pub(crate) mod duel;
mod economy;
pub(crate) mod economy_v2;
pub use economy::{wallet_card_for, wallet_preview_for};
pub(crate) mod events;
pub(crate) mod feeds;
pub(crate) mod fish;
pub(crate) mod friends;
pub(crate) mod games;
pub(crate) mod gift;
pub(crate) mod giveaway;
pub(crate) mod guestbook;
pub(crate) mod hangman;
pub(crate) mod heist;
pub(crate) mod help;
pub(crate) mod house;
pub(crate) mod info;
pub(crate) mod interact;
pub(crate) mod invites;
pub(crate) mod language;
pub(crate) mod leaderboard;
pub(crate) mod level_roles;
pub(crate) mod moderation;
pub(crate) mod modules;
pub(crate) mod partner;
pub(crate) mod party;
pub(crate) mod party_games;
mod periodic;
pub(crate) mod pet;
mod ping;
pub(crate) mod play;
pub(crate) mod plaza;
mod points;
pub(crate) mod poll;
pub(crate) mod prefix;
pub(crate) mod privacy;
pub(crate) mod profile;
pub(crate) mod quests;
pub(crate) mod quick;
mod rank;
pub(crate) mod remind;
mod rep;
mod rep_roles;
pub(crate) mod roblox;
pub(crate) mod role_panels;
pub(crate) mod roles_pack;
pub(crate) mod rps;
pub(crate) mod share;
pub(crate) mod shells;
pub(crate) mod shop;
mod social;
pub(crate) mod stall;
pub(crate) mod starboard;
pub(crate) mod stats;
pub(crate) mod store;
pub(crate) mod suggest;
pub(crate) mod surge;
pub(crate) mod tag;
pub(crate) mod tempvoice;
pub(crate) mod tickets;
pub(crate) mod trackers;
pub(crate) mod trivia;
use crate::undo;
mod utilities;
pub(crate) mod utility;
pub(crate) mod verification;
pub(crate) mod wordle;
mod wrapped;
mod xp;
pub(crate) mod xp_admin;

use twilight_interactions::command::{CommandModel, CreateCommand};
use twilight_model::application::{command::Command, interaction::application_command::CommandData};

use crate::Cx;

macro_rules! commands {
    ($($ty:ty),* $(,)?) => {
        /// Every slash command, for registration.
        pub(crate) fn legacy_definitions() -> Vec<Command> {
            static COMMANDS: std::sync::LazyLock<Vec<Command>> = std::sync::LazyLock::new(|| {
                let mut commands = vec![$(<$ty as CreateCommand>::create_command().into()),*];
                commands.push(moderation::report_context::definition());
                crate::lang::localize(&mut commands);
                commands
            });
            COMMANDS.clone()
        }

        pub fn definitions() -> Vec<Command> {
            let mut commands = legacy_definitions();
            crate::routing::extend(&mut commands);
            crate::routing::remove_replaced(&mut commands);
            crate::lang::localize(&mut commands);
            commands
        }

        pub(crate) async fn dispatch(cx: &Cx, data: CommandData) -> anyhow::Result<()> {
            if crate::routing::is_admin_path(crate::routing::canonical(&crate::command_control::path(&data))) {
                if cx.guild_id().is_none() {
                    return cx.reply_ephemeral(pinchora_core::t!(cx.lang(), "config.guild-only")).await;
                }
                if !cx.can_manage_guild() {
                    return cx.reply_ephemeral(pinchora_core::t!(cx.lang(), "config.needs-manage-server")).await;
                }
            }
            let data = crate::routing::adapt(data)?;
            if crate::gate::refuses_command(cx, &data).await? {
                return Ok(());
            }
            if data.name == "Report" { return moderation::report_context::run(cx, data).await; }
            let name = data.name.clone();
            $(
                if name == <$ty as CreateCommand>::NAME {
                    return <$ty as CommandModel>::from_interaction(data.into())?.run(cx).await;
                }
            )*
            anyhow::bail!("unknown command `{name}`")
        }
    };
}

commands!(
    moderation::parity::Parity,
    community_pack::CommunityAdmin,
    community_v2::HighlightCommand,
    community_v2::ChecklistCommand,
    community_v2::AutopostCommand,
    community_v2::AnniversaryCommand,
    community_v2::ConfessCommand,
    community_pack::QotdCommand,
    community_pack::CountingCommand,
    community_pack::FormCommand,
    community_pack::ModmailCommand,
    branding::BrandingCommand,
    achievements::AchievementsCommand,
    achievements::AchievementAdmin,
    ping::Ping,
    help::Help,
    profile::Profile,
    rank::Rank,
    leaderboard::Leaderboard,
    daily::Daily,
    periodic::Weekly,
    periodic::Monthly,
    economy::Balance,
    economy_v2::EconomyCommand,
    economy::Give,
    rep::Rep,
    bio::Bio,
    config::ConfigCommand,
    house::HouseCommand,
    shop::ShopCommand,
    shop::Inventory,
    pet::PetCommand,
    fish::FishCommand,
    gift::Gift,
    duel::DuelCommand,
    quests::Quests,
    stall::StallCommand,
    plaza::PlazaCommand,
    character::CharacterCommand,
    points::PointsCommand,
    store::StoreCommand,
    customize::CustomizeCommand,
    role_panels::RolesCommand,
    roles_pack::AutoRolesCommand,
    roles_pack::ColorCommand,
    play::PlayCommand,
    partner::PartnerCommand,
    friends::FriendsCommand,
    interact::Hug,
    interact::Pat,
    interact::Wave,
    interact::HighFive,
    remind::RemindCommand,
    privacy::PrivacyCommand,
    wrapped::Wrapped,
    giveaway::GiveawayCommand,
    suggest::Suggest,
    suggest::SuggestionCommand,
    poll::Poll,
    collection::dex::Dex,
    collection::beachcomb::Beachcomb,
    collection::crabpot::CrabPot,
    collection::drops::CritterDrop,
    surge::SurgeCommand,
    party_games::PartyCommand,
    moderation::Ban,
    moderation::Unban,
    moderation::Kick,
    moderation::Timeout,
    moderation::Untimeout,
    moderation::Warn,
    moderation::Warnings,
    moderation::Unwarn,
    moderation::Purge,
    moderation::Slowmode,
    moderation::Lock,
    moderation::Unlock,
    moderation::Nick,
    moderation::RoleCommand,
    moderation::CaseCommand,
    moderation::ReasonCommand,
    tag::Tag,
    customcommand::CustomCommand,
    language::LanguageCommand,
    stats::StatsCommand,
    starboard::StarboardCommand,
    stats::CountersCommand,
    tickets::TicketCommand,
    events::EventCommand,
    tickets::TicketsCommand,
    automod::AutoMod,
    command_control::CommandControl,
    command_control::BackupCommand,
    verification::Verify,
    verification::Verification,
    verification::Raid,
    anti_nuke::AntiNuke,
    feeds::FeedsCommand,
    tempvoice::VoiceCommand,
    xp_admin::XpAdmin,
    info::Info,
    invites::InvitesCommand,
    invites::InviteAdmin,
    roblox::RobloxCommand,
    roblox::RobloxAdmin,
    birthdays::BirthdayCommand,
    birthdays::BirthdaysAdmin,
    utility::AfkCommand,
    utility::StickyCommand,
    utility::ScheduleCommand,
    trackers::GameCommand,
    trackers::TrackersAdmin,
);

#[cfg(test)]
mod quick_command_budget {
    use super::*;

    #[test]
    fn quick_games_keep_the_slash_command_budget() {
        let definitions = definitions();
        assert_eq!(
            definitions.len(),
            crate::routing::PROJECTED_ROOTS,
            "Discord's 100-command budget must not grow"
        );
        for (name, expected) in [
            (
                "play",
                [
                    "memory",
                    "minesweeper",
                    "higherlower",
                    "anagram",
                    "typing",
                    "guessnumber",
                ]
                .as_slice(),
            ),
            ("duel", ["othello"].as_slice()),
            ("party", ["nhie", "fun"].as_slice()),
        ] {
            let command = definitions.iter().find(|c| c.name == name).unwrap();
            assert!(
                command.options.len() <= 25,
                "/{name} has {} subcommands",
                command.options.len()
            );
            for sub in expected {
                assert!(command.options.iter().any(|o| o.name == *sub), "/{name} {sub} missing");
            }
        }
    }
}

pub(crate) mod reef_loop;

pub(crate) mod party_v2;
