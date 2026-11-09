pub mod admin;
pub mod community;
pub mod economy;
pub mod games;
pub mod house;
pub mod integrations;
pub mod progression;
pub mod roles;
pub mod social;
pub use crate::commands::economy::legacy::wallet_card_for;
pub use crate::commands::economy::legacy::wallet_preview_for;
pub(crate) mod moderation;
use crate::admin::undo;

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
                commands.push(crate::commands::community::tickets_context::definition());
                crate::infra::lang::localize(&mut commands);
                commands
            });
            COMMANDS.clone()
        }

        pub fn definitions() -> Vec<Command> {
            let mut commands = legacy_definitions();
            crate::infra::routing::extend(&mut commands);
            crate::infra::routing::remove_replaced(&mut commands);
            crate::infra::lang::localize(&mut commands);
            commands
        }

        pub(crate) async fn dispatch(cx: &Cx, data: CommandData) -> anyhow::Result<()> {
            if crate::infra::routing::is_admin_path(crate::infra::routing::canonical(&crate::admin::command_control::path(&data))) {
                if cx.guild_id().is_none() {
                    return cx.reply_ephemeral(pinchora_core::t!(cx.lang(), "config.guild-only")).await;
                }
                if !cx.can_manage_guild() {
                    return cx.reply_ephemeral(pinchora_core::t!(cx.lang(), "config.needs-manage-server")).await;
                }
            }
            let data = crate::infra::routing::adapt(data)?;
            if crate::infra::gate::refuses_command(cx, &data).await? {
                return Ok(());
            }
            if data.name == "Report" { return moderation::report_context::run(cx, data).await; }
            if data.name == "Open ticket" { return crate::commands::community::tickets_context::run(cx, data).await; }
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
    crate::commands::admin::workflows::Workflow,
    crate::commands::moderation::policy::Parity,
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
    crate::commands::admin::branding::BrandingCommand,
    crate::commands::progression::achievements::AchievementsCommand,
    crate::commands::progression::achievements::AchievementAdmin,
    crate::commands::admin::ping::Ping,
    crate::commands::admin::help::Help,
    crate::commands::social::profile::Profile,
    crate::commands::progression::rank::Rank,
    crate::commands::economy::leaderboard::Leaderboard,
    crate::commands::economy::daily::Daily,
    crate::commands::community::periodic::Weekly,
    crate::commands::community::periodic::Monthly,
    crate::commands::economy::legacy::Balance,
    crate::commands::economy::player::EconomyCommand,
    crate::commands::economy::legacy::Give,
    crate::commands::progression::rep::Rep,
    crate::commands::social::bio::Bio,
    crate::commands::admin::config::ConfigCommand,
    crate::commands::house::house::HouseCommand,
    crate::commands::economy::shop::ShopCommand,
    crate::commands::economy::shop::Inventory,
    crate::commands::social::pet::PetCommand,
    crate::commands::games::fish::FishCommand,
    crate::commands::economy::gift::Gift,
    crate::commands::games::duel::DuelCommand,
    crate::commands::economy::quests::Quests,
    crate::commands::economy::stall::StallCommand,
    crate::commands::community::plaza::PlazaCommand,
    crate::commands::social::character::CharacterCommand,
    crate::commands::economy::points::PointsCommand,
    crate::commands::economy::store::StoreCommand,
    crate::commands::social::customize::CustomizeCommand,
    crate::commands::roles::role_panels::RolesCommand,
    crate::commands::roles::automation::AutoRolesCommand,
    crate::commands::roles::automation::ColorCommand,
    crate::commands::games::play::PlayCommand,
    crate::commands::community::partner::PartnerCommand,
    crate::commands::social::friends::FriendsCommand,
    crate::commands::social::interact::Hug,
    crate::commands::social::interact::Pat,
    crate::commands::social::interact::Wave,
    crate::commands::social::interact::HighFive,
    crate::commands::community::remind::RemindCommand,
    crate::commands::admin::privacy::PrivacyCommand,
    crate::commands::progression::wrapped::Wrapped,
    crate::commands::community::giveaway::GiveawayCommand,
    crate::commands::community::suggest::Suggest,
    crate::commands::community::suggest::SuggestionCommand,
    crate::commands::community::poll::Poll,
    crate::commands::games::collection::dex::Dex,
    crate::commands::games::collection::beachcomb::Beachcomb,
    crate::commands::games::collection::crabpot::CrabPot,
    crate::commands::games::collection::drops::CritterDrop,
    crate::commands::economy::surge::SurgeCommand,
    crate::commands::games::party_games::PartyCommand,
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
    crate::commands::community::tag::Tag,
    crate::commands::admin::customcommand::CustomCommand,
    crate::commands::admin::language::LanguageCommand,
    crate::commands::admin::stats::StatsCommand,
    crate::commands::community::starboard::StarboardCommand,
    crate::commands::admin::stats::CountersCommand,
    crate::commands::community::tickets::TicketCommand,
    crate::commands::community::events::EventCommand,
    crate::commands::community::tickets::TicketsCommand,
    crate::commands::moderation::automod::AutoMod,
    crate::commands::admin::command_control::CommandControl,
    crate::commands::admin::command_control::BackupCommand,
    crate::commands::moderation::verification::Verify,
    crate::commands::moderation::verification::Verification,
    crate::commands::moderation::verification::Raid,
    crate::commands::moderation::anti_nuke::AntiNuke,
    crate::commands::community::feeds::FeedsCommand,
    crate::commands::admin::tempvoice::VoiceCommand,
    crate::commands::progression::xp_admin::XpAdmin,
    crate::commands::admin::info::Info,
    crate::commands::community::invites::InvitesCommand,
    crate::commands::community::invites::InviteAdmin,
    crate::commands::integrations::roblox::RobloxCommand,
    crate::commands::integrations::roblox::RobloxAdmin,
    crate::commands::community::birthdays::BirthdayCommand,
    crate::commands::community::birthdays::BirthdaysAdmin,
    crate::commands::admin::afk_sticky_schedule::AfkCommand,
    crate::commands::admin::afk_sticky_schedule::StickyCommand,
    crate::commands::admin::afk_sticky_schedule::ScheduleCommand,
    crate::commands::community::trackers::GameCommand,
    crate::commands::community::trackers::TrackersAdmin,
);

#[cfg(test)]
mod quick_command_budget {
    use crate::commands::*;

    #[test]
    fn quick_games_keep_the_slash_command_budget() {
        let definitions = definitions();
        assert_eq!(
            definitions.len(),
            crate::infra::routing::PROJECTED_ROOTS,
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
