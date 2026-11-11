//! Database integration tests. Global workers take an exclusive PostgreSQL fixture lock.
//! Migration regressions use private schemas in rollback transactions, so they can
//! share this binary without replacing or truncating the application schema.

mod achievements;
mod anti_nuke;
mod birthdays;
mod board_games;
mod card_designs;
mod collection;
mod command_control;
mod community;
mod db;
mod economy_player;
mod events_review;
mod feeds_live;
mod games;
mod invites_review;
mod language;
mod leveling_policy;
mod locks;
mod message_studio;
mod migrations;
mod moderation;
mod moderation_policy;
mod modules;
mod out_of_order_migrations;
mod party;
mod party_games;
mod privacy;
mod reef_games;
mod reef_loop;
mod reef_migration;
mod reef_party;
mod roblox;
mod roles_automation;
mod server_events;
mod starboard;
mod stats;
mod support;
mod temp_voice;
mod ticket_ops;
mod tickets;
mod transaction_cancellation;
mod utility;
mod workflows;
