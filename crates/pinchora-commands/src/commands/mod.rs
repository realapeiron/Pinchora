mod bio;
mod config;
mod daily;
pub(crate) mod duel;
mod economy;
pub(crate) mod fish;
pub(crate) mod gift;
mod help;
pub(crate) mod house;
pub(crate) mod leaderboard;
pub(crate) mod pet;
mod ping;
mod profile;
pub(crate) mod quests;
mod rank;
mod rep;
pub(crate) mod shop;

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
    economy::Balance,
    economy::Give,
    rep::Rep,
    bio::Bio,
    config::ConfigCommand,
    house::HouseCommand,
    shop::ShopCommand,
    shop::Inventory,
    pet::PetCommand,
    fish::Fish,
    gift::Gift,
    duel::DuelCommand,
    quests::Quests,
);
