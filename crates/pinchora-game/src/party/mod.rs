//! The party games: pure rules and content for the hermit crab derby, share or grab, the shell game,
//! the treasure heist and the storm surge. Nothing here touches the database or Discord.

pub mod derby;
pub mod heist;
mod rng;
pub mod share;
pub mod shells;
pub mod surge;

pub use rng::Rng;
