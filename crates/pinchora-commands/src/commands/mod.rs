mod bio;
pub(crate) mod board;
pub(crate) mod character;
pub(crate) mod collection;
pub(crate) mod config;
pub(crate) mod connect4;
pub(crate) mod contest;
pub(crate) mod customize;
mod daily;
pub(crate) mod decorate;
pub(crate) mod derby;
pub(crate) mod duel;
mod economy;
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
pub(crate) mod interact;
pub(crate) mod leaderboard;
pub(crate) mod level_roles;
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
pub(crate) mod privacy;
pub(crate) mod profile;
pub(crate) mod quests;
mod rank;
pub(crate) mod remind;
mod rep;
mod rep_roles;
pub(crate) mod role_panels;
pub(crate) mod rps;
pub(crate) mod share;
pub(crate) mod shells;
pub(crate) mod shop;
mod social;
pub(crate) mod stall;
pub(crate) mod store;
pub(crate) mod suggest;
pub(crate) mod surge;
pub(crate) mod trivia;
mod undo;
mod utilities;
pub(crate) mod wordle;
mod wrapped;
mod xp;

use twilight_interactions::command::{CommandModel, CreateCommand};
use twilight_model::application::{command::Command, interaction::application_command::CommandData};

use crate::Cx;

macro_rules! commands {
    ($($ty:ty),* $(,)?) => {
        /// Every slash command, for registration.
        pub fn definitions() -> Vec<Command> {
            vec![$(<$ty as CreateCommand>::create_command().into()),*]
        }

        pub(crate) async fn dispatch(cx: &Cx, data: CommandData) -> anyhow::Result<()> {
            if crate::gate::refuses_command(cx, &data).await? {
                return Ok(());
            }
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
    ping::Ping,
    help::Help,
    profile::Profile,
    rank::Rank,
    leaderboard::Leaderboard,
    daily::Daily,
    periodic::Weekly,
    periodic::Monthly,
    economy::Balance,
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
);
