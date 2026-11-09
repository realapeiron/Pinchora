pub mod admin;
pub mod community;
mod economy;
#[cfg(test)]
use pinchora_testkit::isolation as fixture_isolation;
mod games;
pub mod house;
pub mod infra;
pub mod integrations;
mod moderation;
mod progression;
mod roles;
mod social;

mod commands;
pub use crate::admin::afk_sticky_sender::bot_permissions as utility_bot_permissions;
pub use crate::admin::afk_sticky_sender::creator_may_ping as utility_creator_may_ping;
pub use crate::admin::afk_sticky_sender::spawn_ticker as spawn_utility_ticker;
pub use crate::admin::afk_sticky_sender::tick as utility_tick;
pub use crate::commands::community::suggest::refresh_suggestion_message;
pub use crate::community::trackers::spawn_ticker as spawn_trackers_ticker;
pub use crate::community::trackers::tick as trackers_tick;
pub use crate::progression::leveling_policy::post_weekly_highlights as leveling_highlights_tick;
pub use crate::progression::leveling_policy::spawn_ticker as spawn_leveling_policy_ticker;
pub use crate::progression::xp_admin::ImportMode;
pub use crate::progression::xp_admin::ImportProgress;
pub use crate::progression::xp_admin::ImportRequest;
pub use crate::progression::xp_admin::import_mee6;
pub use crate::progression::xp_admin::import_mee6_with_fetch;
pub use crate::progression::xp_admin::mee6_resting;
pub use crate::progression::xp_admin::sync_member_roles;

pub use crate::admin::card_images::CardSubject;
pub use crate::admin::card_images::ServerCard;
pub use crate::admin::card_images::atlas_has;
pub use crate::admin::card_images::card_png;
pub use crate::admin::card_images::classic_card;
pub use crate::admin::card_images::deliver;
pub use crate::admin::card_images::preview_png;
pub use crate::admin::card_images::server_card;
pub use crate::admin::catalog::command_catalog;
pub use crate::admin::catalog::command_catalog_tr;
pub use crate::admin::catalog::site_facts;
pub use crate::admin::custom::Rendered;
pub use crate::admin::custom::Spec;
pub use crate::admin::custom::def_diff;
pub use crate::admin::custom::embed_of;
pub use crate::admin::custom::escape_markdown;
pub use crate::admin::custom::mention_ids;
pub use crate::admin::custom::pick as pick_random;
pub use crate::admin::custom::render_message;
pub use crate::admin::custom::scrub_value;
pub use crate::admin::custom::templates;
pub use crate::admin::stats::cards::server_card_png as stats_card_png;
pub use crate::admin::stats::counter_problem as stats_counter_problem;
pub use crate::admin::stats::counter_value as stats_counter_value;
pub use crate::admin::stats::counters::missing_permission as stats_missing_permission;
pub use crate::admin::stats::flush as stats_flush;
pub use crate::admin::stats::refresh_counters as stats_refresh_counters;
pub use crate::admin::stats::spawn_ticker as spawn_stats_ticker;
pub use crate::admin::stats::sweep as stats_sweep;
pub use crate::commands::admin::customcommand::reserved_names as reserved_command_names;
pub use crate::commands::admin::modules::announce_guild_change;
pub use crate::commands::admin::modules::module_diff;
pub use crate::commands::admin::tempvoice::forget_setup as forget_temp_voice;
pub use crate::commands::admin::tempvoice::missing_permissions as temp_voice_gaps;
pub use crate::commands::admin::tempvoice::room_members as temp_voice_members;
pub use crate::commands::admin::tempvoice::spawn_ticker as spawn_tempvoice_ticker;
pub use crate::commands::admin::tempvoice::tick as tempvoice_tick;
pub use crate::commands::community::invites::grant_invite_reward;
pub use crate::commands::community::invites::reconcile_all_invite_rewards;
pub use crate::commands::community::invites::refresh_created_snapshot as refresh_created_invite_snapshot;
pub use crate::commands::community::invites::refresh_deleted_snapshot as refresh_deleted_invite_snapshot;
pub use crate::commands::community::invites::refresh_snapshot as refresh_invite_snapshot;
pub use crate::commands::community::invites::spawn_reward_ticker as spawn_invite_reward_ticker;
pub use crate::commands::community::partner::partner_name;
pub use crate::commands::community::remind::spawn_ticker as spawn_reminder_ticker;
pub use crate::commands::community::remind::tick as reminder_tick;
pub use crate::commands::community::tools::spawn_ticker as spawn_community_tools_ticker;
pub use crate::commands::community::tools::tick as community_tools_tick;
pub use crate::commands::games::collection::post_bottle;
pub use crate::commands::games::collection::post_critter_drop;
pub use crate::commands::games::collection::spawn_ticker as spawn_collection_ticker;
pub use crate::commands::games::collection::tick as collection_tick;
pub use crate::commands::games::games::spawn_janitor as spawn_game_janitor;
pub use crate::commands::games::games::sweep as sweep_games;
pub use crate::commands::games::party_games::spawn_ticker as spawn_party_ticker;
pub use crate::commands::games::party_games::tick as party_tick;
pub use crate::commands::progression::achievements::evaluate as evaluate_achievements;
pub use crate::commands::progression::achievements::invalidate as invalidate_achievements;
pub use crate::commands::progression::achievements::on_message as achievement_tick;
pub use crate::commands::progression::achievements::source_enabled as achievement_source_enabled;
pub use crate::commands::progression::level_roles::SyncOutcome;
pub use crate::commands::progression::level_roles::SyncReport;
pub use crate::commands::progression::level_roles::sync_level_roles;
pub use crate::commands::progression::level_roles::sync_resting;
pub use crate::commands::roles::automation::ColorRoleReport;
pub use crate::commands::roles::automation::ColorStop;
pub use crate::commands::roles::automation::PaletteColor;
pub use crate::commands::roles::automation::PostRefusal;
pub use crate::commands::roles::automation::PostedPanel;
pub use crate::commands::roles::automation::color_stop_text;
pub use crate::commands::roles::automation::create_palette_roles;
pub use crate::commands::roles::automation::delete_color_role;
pub use crate::commands::roles::automation::give_after_verify;
pub use crate::commands::roles::automation::palette as color_palette;
pub use crate::commands::roles::automation::post_panel as post_role_panel;
pub use crate::commands::roles::automation::spawn_ticker as spawn_roles_ticker;
pub use crate::commands::roles::automation::start_sticky_snapshot;
pub use crate::commands::roles::automation::sticky_changed;
pub use crate::commands::roles::automation::tick as roles_tick;
pub use crate::commands::roles::role_panels::check_emoji;
pub use crate::commands::roles::role_panels::rules_problem as role_rules_problem;
pub use crate::commands::social::pet::current_needs as player_pet_needs;
pub use crate::commands::social::pet::is_asleep as player_pet_asleep;
pub use crate::commands::social::pet::pet_png as player_pet_png;
pub use crate::community::engagement::spawn_ticker as spawn_community_ticker;
pub use crate::community::engagement::tick as community_tick;
pub use crate::community::event_rsvp::check_manage_events as check_event_sync_permission;
pub use crate::community::event_rsvp::on_scheduled_event_update;
pub use crate::community::event_rsvp::publish as publish_event;
pub use crate::community::event_rsvp::refresh_card as refresh_event_card;
pub use crate::community::event_rsvp::spawn_ticker as spawn_event_ticker;
pub use crate::community::event_rsvp::sync_discord as sync_discord_event;
pub use crate::community::event_rsvp::tick as event_tick;
pub use crate::community::events::PermissionChange;
pub use crate::community::events::on_ban_add;
pub use crate::community::events::on_ban_remove;
pub use crate::community::events::on_guild_create;
pub use crate::community::events::on_guild_delete;
pub use crate::community::events::on_member_add;
pub use crate::community::events::on_member_remove;
pub use crate::community::events::on_member_update;
pub use crate::community::events::on_message;
pub use crate::community::events::on_permissions_changed;
pub use crate::community::events::on_reaction;
pub use crate::community::events::on_voice_state_update;
pub use crate::community::events::voice_snapshot;
pub use crate::community::feeds::spawn_ticker as spawn_feeds_ticker;
pub use crate::community::feeds::tick as feeds_tick;
pub use crate::community::server_events::spawn_ticker as spawn_events_ticker;
pub use crate::community::server_events::tick as events_tick;
pub use crate::community::starboard::cleanup_one as cleanup_starboard_copy;
pub use crate::community::starboard::on_message as on_starboard_message;
pub use crate::community::starboard::on_reaction as on_starboard_reaction;
pub use crate::community::starboard::on_update as on_starboard_update;
pub use crate::community::starboard::reconcile_one as reconcile_starboard_post;
pub use crate::community::starboard::spawn_ticker as spawn_starboard_ticker;
pub use crate::community::starboard::tick as starboard_tick;
pub use crate::community::tickets::PanelPost as TicketPanelPost;
pub use crate::community::tickets::close_from_web as close_ticket_from_web;
pub use crate::community::tickets::on_place_deleted as on_ticket_place_deleted;
pub use crate::community::tickets::post_panel as post_ticket_panel;
pub use crate::community::tickets::refresh_status_board as refresh_ticket_status_board;
pub use crate::community::tickets::spawn_ticker as spawn_tickets_ticker;
pub use crate::community::tickets::tick as tickets_tick;
pub use crate::house::house_view::RoomData;
pub use crate::house::house_view::catalog as house_catalog;
pub use crate::house::house_view::character_png;
pub use crate::house::house_view::render_png as render_room_png;
pub use crate::house::house_view::room_name as house_room_name;
pub use crate::house::house_view::seed_at as house_seed_at;
pub use crate::infra::app::App;
pub use crate::infra::app::PreviewError;
pub use crate::infra::cx::Cx;
pub use crate::infra::defer::Deferral;
pub use crate::infra::hooks::ReactionEvent;
pub use crate::infra::invalidations::listen as listen_for_invalidations;
pub use crate::infra::responder::Responder;
pub use crate::infra::text::on_text_command;
pub use crate::infra::util::fill;
pub use crate::moderation::log_audit::on_audit_log_entry;
pub use crate::moderation::server_log::on_message_delete;
pub use crate::moderation::server_log::on_message_delete_bulk;
pub use crate::moderation::server_log::on_message_update;
pub use crate::progression::leveling::open_private_channel;
pub use crate::progression::voice_xp::VoiceSnapshot;
pub use crate::progression::voice_xp::forget as forget_voice_presence;
pub use crate::progression::voice_xp::prune as voice_prune;
pub use crate::progression::voice_xp::rebuild as rebuild_voice;
pub use crate::progression::voice_xp::spawn_ticker as spawn_voice_ticker;
pub use crate::progression::voice_xp::switched_off as voice_switched_off;
pub use crate::progression::voice_xp::switched_on as voice_switched_on;
pub use crate::progression::voice_xp::tick as voice_tick;
pub use crate::social::message_studio::EventMember as StudioEventMember;
pub use crate::social::message_studio::event_text as studio_event_text;
pub use commands::definitions as command_definitions;
pub use commands::moderation::lift_expired_bans;
pub use commands::moderation::refresh_post as refresh_case_post;
pub use commands::moderation::spawn_ticker as spawn_moderation_ticker;

/// Grant a form acceptance role after rechecking the bot hierarchy and role permissions.
pub async fn grant_community_role(app: &App, guild: u64, user: u64, role: u64) -> anyhow::Result<()> {
    crate::commands::roles::automation::give_safe_role(app, guild, user, role, "Form accepted").await
}
pub async fn close_community_mail(
    app: &Arc<App>,
    guild: u64,
    thread: u64,
    actor: u64,
    reason: Option<&str>,
) -> anyhow::Result<bool> {
    crate::community::engagement::close_thread(app, guild, thread, actor, reason).await
}

use std::{sync::Arc, time::Instant};

use twilight_model::application::interaction::{Interaction, InteractionData, InteractionType};

/// [`handle_interaction`] for the gateway, where nothing else answers for a slow handler: one still
/// running after `defer::DEFER_AFTER` is deferred through REST before Discord's three seconds are up.
pub async fn handle_gateway_interaction(app: Arc<App>, interaction: Interaction, responder: Arc<Responder>) {
    let handler = handle_interaction(app, interaction, responder.clone());
    crate::infra::defer::defer_slow(&responder, crate::infra::defer::DEFER_AFTER, handler).await;
}

/// Runs one interaction to completion. Every path answers the user, errors included.
#[tracing::instrument(name = "discord.interaction", skip_all, fields(request_id = %interaction.id, guild_id = ?interaction.guild_id))]
pub async fn handle_interaction(app: Arc<App>, mut interaction: Interaction, responder: Arc<Responder>) {
    let started = Instant::now();
    let data = interaction.data.take();
    let lang = crate::infra::lang::resolve(&app, &interaction).await;
    let cx = Cx {
        app,
        interaction,
        responder,
        lang,
    };
    // Typing into an option that suggests: only suggestions go back, and the command itself neither
    // runs nor passes a module's switch.
    if cx.interaction.kind == InteractionType::ApplicationCommandAutocomplete {
        if let Some(InteractionData::ApplicationCommand(command)) = data {
            let allowed = crate::admin::command_control::autocomplete_allowed(&cx, &command)
                .await
                .unwrap_or(false);
            let command = crate::infra::routing::adapt((*command).clone()).unwrap_or(*command);
            let choices = match command.name.as_str() {
                _ if !allowed => Vec::new(),
                // registrations:autocomplete:start
                "customcommand" => crate::commands::community::tag::autocomplete(&cx, &command).await,
                "birthday" | "birthdays" => crate::commands::community::birthdays::autocomplete(&command),
                "tag" => crate::commands::community::tag::autocomplete(&cx, &command).await,
                _ => crate::commands::admin::modules::autocomplete(cx.lang(), &command), // registrations:autocomplete:end
            };
            if let Err(error) = cx.responder.suggest(choices).await {
                tracing::warn!(%error, name = %command.name, "could not answer an autocomplete");
            }
        }
        return;
    }
    if cx.interaction.kind == InteractionType::ApplicationCommand {
        crate::admin::status::record_interaction(&cx.app.cache).await;
    }
    // Handler futures can hold large command state; on the heap they
    // cannot overflow a worker thread's stack.
    let (label, result) = match data {
        Some(InteractionData::ApplicationCommand(command)) => {
            let invoked = crate::admin::command_control::path(&command);
            let label = crate::infra::routing::canonical(&invoked).to_owned();
            (label, Box::pin(commands::dispatch(&cx, *command)).await)
        }
        Some(InteractionData::MessageComponent(component)) => {
            let label = component.custom_id.split(':').next().unwrap_or_default().to_owned();
            (
                format!("component:{label}"),
                Box::pin(crate::infra::components::dispatch(
                    &cx,
                    &component.custom_id,
                    &component.values,
                ))
                .await,
            )
        }
        Some(InteractionData::ModalSubmit(modal)) => {
            let label = modal.custom_id.split(':').next().unwrap_or_default().to_owned();
            let fields = crate::infra::components::modal_fields(&modal.components, modal.resolved.as_ref());
            (
                format!("modal:{label}"),
                Box::pin(crate::infra::components::dispatch_modal(&cx, &modal.custom_id, &fields)).await,
            )
        }
        _ => return,
    };
    let outcome = if result.is_ok() { "ok" } else { "error" };
    metrics::counter!("pinchora_interactions_total", "name" => label.clone(), "outcome" => outcome).increment(1);
    metrics::histogram!("pinchora_interaction_seconds", "name" => label.clone())
        .record(started.elapsed().as_secs_f64());
    if let Err(error) = result {
        tracing::error!(name = %label, error = ?error, "interaction failed");
        let _ = cx.reply_ephemeral(pinchora_core::t!(cx.lang(), "error.generic")).await;
    }
}

pub use crate::moderation::automod::check_config_roles as check_automod_roles;
pub use crate::moderation::automod::on_native_execution as on_automod_execution;
pub use crate::moderation::automod::on_native_rule_change as on_automod_rule_change;
pub use crate::moderation::automod::spawn_ticker as spawn_automod_ticker;
pub use crate::moderation::automod::sync_guild as sync_automod;

pub use crate::community::birthdays::spawn_ticker as spawn_birthdays_ticker;
pub use crate::community::birthdays::tick as birthdays_tick;
pub use crate::moderation::anti_nuke::authorize_antinuke;
pub use crate::moderation::anti_nuke::authorize_antinuke_module;
pub use crate::moderation::anti_nuke::on_gateway as on_antinuke_gateway;
pub use crate::moderation::anti_nuke::process as on_antinuke_audit;
pub use crate::moderation::anti_nuke::restore_antinuke;
pub use crate::moderation::anti_nuke::restore_antinuke_actor_roles;
pub use crate::moderation::anti_nuke::save_antinuke_config;
pub use crate::moderation::anti_nuke::spawn_ticker as spawn_antinuke_ticker;
pub use crate::moderation::verification::answer as verification_answer;
pub use crate::moderation::verification::challenge as verification_challenge;
pub use crate::moderation::verification::check_roles as check_verification_roles;
pub use crate::moderation::verification::post as post_verification;
pub use crate::moderation::verification::raid::start as start_raid;
pub use crate::moderation::verification::raid::unlock as unlock_raid;
pub use crate::moderation::verification::request_help as verification_help;
pub use crate::moderation::verification::spawn_ticker as spawn_verification_ticker;
pub use crate::moderation::verification::tick as verification_tick;
pub use crate::moderation::verification::web_identity as verification_web_identity;

pub use crate::admin::command_control::command_reserved_aliases;
pub use crate::admin::command_control::control_catalog;
pub use crate::admin::command_control::control_validation_catalog;

pub use crate::admin::config_backup::backup_apply;
pub use crate::admin::config_backup::backup_names;
pub use crate::admin::config_backup::backup_preview;
pub use crate::admin::config_backup::backup_rate_limit;
pub use crate::admin::config_backup::validate_backup;

pub use crate::commands::moderation::policy::check_config as check_moderation_policy_config;
pub use crate::moderation::policy::spawn_ticker as spawn_moderation_policy_ticker;
pub use crate::moderation::policy::sweep as sweep_moderation_policy;
pub use crate::moderation::policy::voice_event as on_moderation_policy_voice;

pub use crate::commands::moderation::policy::review as review_moderation_policy;

pub use crate::integrations::roblox::spawn_ticker as spawn_roblox_ticker;
pub use crate::moderation::policy::seed_voice as seed_moderation_policy_voice;

pub use crate::commands::social::character::randomise as randomise_character;
pub use crate::commands::social::profile::ProfileContext;
pub use crate::commands::social::profile::profile_card_for;

pub use crate::commands::house::house::Who;
pub use commands::{wallet_card_for, wallet_preview_for};

pub use crate::commands::games::reef_party::tick as reef_party_tick;

pub use crate::economy::player::spawn_ticker as spawn_player_economy_ticker;

pub use crate::admin::workflows::spawn_ticker as spawn_workflows_ticker;
