mod app;
mod auto;
pub mod branding;
mod card_images;
mod catalog;
mod commands;
pub use commands::suggest::refresh_suggestion_message;
mod community_pack;
mod components;
mod custom;
mod cx;
mod defer;
mod event_rsvp;
mod events;
pub mod feeds;
mod game;
mod gate;
mod hooks;
mod house_view;
mod invalidations;
#[cfg(test)]
mod landing_assets;
mod lang;
mod leveling;
mod log_audit;
mod message_studio;
mod responder;
mod roles;
pub mod routing;
mod server_events;
mod server_log;
mod starboard;
mod stats;
pub mod status;
mod text;
mod tickets;
mod trackers;
pub use trackers::{spawn_ticker as spawn_trackers_ticker, tick as trackers_tick};
mod leveling_parity;
mod util;
mod utility;
mod voice_xp;
pub use leveling_parity::{
    post_weekly_highlights as leveling_highlights_tick, spawn_ticker as spawn_leveling_parity_ticker,
};
mod xp_admin;
pub use utility::{
    bot_permissions as utility_bot_permissions, creator_may_ping as utility_creator_may_ping,
    spawn_ticker as spawn_utility_ticker, tick as utility_tick,
};
pub use xp_admin::{
    ImportMode, ImportProgress, ImportRequest, import_mee6, import_mee6_with_fetch, mee6_resting, sync_member_roles,
};

pub use app::{App, PreviewError};
pub use card_images::{CardSubject, ServerCard, atlas_has, card_png, classic_card, deliver, preview_png, server_card};
pub use catalog::{command_catalog, command_catalog_tr, site_facts};
pub use commands::achievements::{
    evaluate as evaluate_achievements, invalidate as invalidate_achievements, on_message as achievement_tick,
    source_enabled as achievement_source_enabled,
};
pub use commands::community_v2::{spawn_ticker as spawn_community_v2_ticker, tick as community_v2_tick};
pub use commands::invites::{
    grant_invite_reward, reconcile_all_invite_rewards, refresh_created_snapshot as refresh_created_invite_snapshot,
    refresh_deleted_snapshot as refresh_deleted_invite_snapshot, refresh_snapshot as refresh_invite_snapshot,
    spawn_reward_ticker as spawn_invite_reward_ticker,
};
pub use commands::pet::{current_needs as player_pet_needs, is_asleep as player_pet_asleep, pet_png as player_pet_png};
pub use commands::role_panels::{check_emoji, rules_problem as role_rules_problem};
pub use commands::roles_pack::{
    ColorRoleReport, ColorStop, PaletteColor, PostRefusal, PostedPanel, color_stop_text, create_palette_roles,
    delete_color_role, give_after_verify, palette as color_palette, post_panel as post_role_panel,
    spawn_ticker as spawn_roles_ticker, start_sticky_snapshot, sticky_changed, tick as roles_tick,
};
pub use commands::{
    collection::{post_bottle, post_critter_drop, spawn_ticker as spawn_collection_ticker, tick as collection_tick},
    customcommand::reserved_names as reserved_command_names,
    definitions as command_definitions,
    games::{spawn_janitor as spawn_game_janitor, sweep as sweep_games},
    level_roles::{SyncOutcome, SyncReport, sync_level_roles, sync_resting},
    moderation::{lift_expired_bans, refresh_post as refresh_case_post, spawn_ticker as spawn_moderation_ticker},
    modules::{announce_guild_change, module_diff},
    partner::partner_name,
    party_games::{spawn_ticker as spawn_party_ticker, tick as party_tick},
    remind::{spawn_ticker as spawn_reminder_ticker, tick as reminder_tick},
    tempvoice::{
        forget_setup as forget_temp_voice, missing_permissions as temp_voice_gaps, room_members as temp_voice_members,
        spawn_ticker as spawn_tempvoice_ticker, tick as tempvoice_tick,
    },
};
pub use community_pack::{spawn_ticker as spawn_community_ticker, tick as community_tick};
pub use custom::{
    Rendered, Spec, def_diff, embed_of, escape_markdown, mention_ids, pick as pick_random, render_message, scrub_value,
    templates,
};
pub use cx::Cx;
pub use defer::Deferral;
pub use event_rsvp::{
    check_manage_events as check_event_sync_permission, on_scheduled_event_update, publish as publish_event,
    refresh_card as refresh_event_card, spawn_ticker as spawn_event_ticker, sync_discord as sync_discord_event,
    tick as event_tick,
};
pub use events::on_reaction;
pub use events::{
    PermissionChange, on_ban_add, on_ban_remove, on_guild_create, on_guild_delete, on_member_add, on_member_remove,
    on_member_update, on_message, on_permissions_changed, on_voice_state_update, voice_snapshot,
};
pub use feeds::{spawn_ticker as spawn_feeds_ticker, tick as feeds_tick};
pub use hooks::ReactionEvent;
pub use house_view::{
    RoomData, catalog as house_catalog, character_png, render_png as render_room_png, room_name as house_room_name,
    seed_at as house_seed_at,
};
pub use invalidations::listen as listen_for_invalidations;
pub use leveling::open_private_channel;
pub use log_audit::on_audit_log_entry;
pub use message_studio::{EventMember as StudioEventMember, event_text as studio_event_text};
pub use responder::Responder;
pub use server_events::{spawn_ticker as spawn_events_ticker, tick as events_tick};
pub use server_log::{on_message_delete, on_message_delete_bulk, on_message_update};
pub use starboard::{
    cleanup_one as cleanup_starboard_copy, on_message as on_starboard_message, on_reaction as on_starboard_reaction,
    on_update as on_starboard_update, reconcile_one as reconcile_starboard_post,
    spawn_ticker as spawn_starboard_ticker, tick as starboard_tick,
};
pub use stats::{
    cards::server_card_png as stats_card_png, counter_problem as stats_counter_problem,
    counter_value as stats_counter_value, counters::missing_permission as stats_missing_permission,
    flush as stats_flush, refresh_counters as stats_refresh_counters, spawn_ticker as spawn_stats_ticker,
    sweep as stats_sweep,
};
pub use text::on_text_command;
pub use tickets::{
    PanelPost as TicketPanelPost, close_from_web as close_ticket_from_web, on_place_deleted as on_ticket_place_deleted,
    post_panel as post_ticket_panel, spawn_ticker as spawn_tickets_ticker, tick as tickets_tick,
};
pub use util::fill;
pub use voice_xp::{
    VoiceSnapshot, forget as forget_voice_presence, prune as voice_prune, rebuild as rebuild_voice,
    spawn_ticker as spawn_voice_ticker, switched_off as voice_switched_off, switched_on as voice_switched_on,
    tick as voice_tick,
};

/// Grant a form acceptance role after rechecking the bot hierarchy and role permissions.
pub async fn grant_community_role(app: &App, guild: u64, user: u64, role: u64) -> anyhow::Result<()> {
    commands::roles_pack::give_safe_role(app, guild, user, role, "Form accepted").await
}
pub async fn close_community_mail(
    app: &Arc<App>,
    guild: u64,
    thread: u64,
    actor: u64,
    reason: Option<&str>,
) -> anyhow::Result<bool> {
    community_pack::close_thread(app, guild, thread, actor, reason).await
}

use std::{sync::Arc, time::Instant};

use twilight_model::application::interaction::{Interaction, InteractionData, InteractionType};

/// [`handle_interaction`] for the gateway, where nothing else answers for a slow handler: one still
/// running after `defer::DEFER_AFTER` is deferred through REST before Discord's three seconds are up.
pub async fn handle_gateway_interaction(app: Arc<App>, interaction: Interaction, responder: Arc<Responder>) {
    let handler = handle_interaction(app, interaction, responder.clone());
    defer::defer_slow(&responder, defer::DEFER_AFTER, handler).await;
}

/// Runs one interaction to completion. Every path answers the user, errors included.
pub async fn handle_interaction(app: Arc<App>, mut interaction: Interaction, responder: Arc<Responder>) {
    let started = Instant::now();
    let data = interaction.data.take();
    let lang = lang::resolve(&app, &interaction).await;
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
            let allowed = command_control::autocomplete_allowed(&cx, &command)
                .await
                .unwrap_or(false);
            let command = routing::adapt((*command).clone()).unwrap_or(*command);
            let choices = match command.name.as_str() {
                _ if !allowed => Vec::new(),
                "birthday" | "birthdays" => commands::birthdays::autocomplete(&command),
                "tag" | "customcommand" => commands::tag::autocomplete(&cx, &command).await,
                _ => commands::modules::autocomplete(cx.lang(), &command),
            };
            if let Err(error) = cx.responder.suggest(choices).await {
                tracing::warn!(%error, name = %command.name, "could not answer an autocomplete");
            }
        }
        return;
    }
    if cx.interaction.kind == InteractionType::ApplicationCommand {
        status::record_interaction(&cx.app.cache).await;
    }
    // The handler futures are large (every command's state in one enum); on the heap they
    // cannot overflow a worker thread's stack.
    let (label, result) = match data {
        Some(InteractionData::ApplicationCommand(command)) => {
            let invoked = command_control::path(&command);
            let label = routing::canonical(&invoked).to_owned();
            (label, Box::pin(commands::dispatch(&cx, *command)).await)
        }
        Some(InteractionData::MessageComponent(component)) => {
            let label = component.custom_id.split(':').next().unwrap_or_default().to_owned();
            (
                format!("component:{label}"),
                Box::pin(components::dispatch(&cx, &component.custom_id, &component.values)).await,
            )
        }
        Some(InteractionData::ModalSubmit(modal)) => {
            let label = modal.custom_id.split(':').next().unwrap_or_default().to_owned();
            let fields = components::modal_fields(&modal.components);
            (
                format!("modal:{label}"),
                Box::pin(components::dispatch_modal(&cx, &modal.custom_id, &fields)).await,
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

mod automod;
mod birthdays;
pub use automod::{
    check_config_roles as check_automod_roles, on_native_execution as on_automod_execution,
    on_native_rule_change as on_automod_rule_change, spawn_ticker as spawn_automod_ticker, sync_guild as sync_automod,
};

mod verification;
pub use verification::raid::{start as start_raid, unlock as unlock_raid};
pub use verification::{
    answer as verification_answer, challenge as verification_challenge, check_roles as check_verification_roles,
    post as post_verification, request_help as verification_help, spawn_ticker as spawn_verification_ticker,
    tick as verification_tick, web_identity as verification_web_identity,
};
mod anti_nuke;
pub use anti_nuke::{
    authorize_antinuke, authorize_antinuke_module, on_gateway as on_antinuke_gateway, process as on_antinuke_audit,
    restore_antinuke, restore_antinuke_actor_roles, save_antinuke_config, spawn_ticker as spawn_antinuke_ticker,
};
pub use birthdays::{spawn_ticker as spawn_birthdays_ticker, tick as birthdays_tick};

mod command_control;
pub use command_control::{command_reserved_aliases, control_catalog, control_validation_catalog};

mod config_backup;
pub use config_backup::{backup_apply, backup_names, backup_preview, backup_rate_limit, validate_backup};

mod mod_parity;
pub use commands::moderation::parity::check_config as check_mod_parity_config;
pub use mod_parity::{
    spawn_ticker as spawn_mod_parity_ticker, sweep as sweep_mod_parity, voice_event as on_mod_parity_voice,
};

pub use commands::moderation::parity::review as review_mod_parity;

pub use mod_parity::seed_voice as seed_mod_parity_voice;
pub mod roblox;
pub use roblox::spawn_ticker as spawn_roblox_ticker;

pub use commands::character::randomise as randomise_character;
pub use commands::profile::{ProfileContext, profile_card_for};

pub use commands::{wallet_card_for, wallet_preview_for};
pub mod house_edit;
pub mod undo;
pub use commands::house::Who;

pub use commands::party_v2::tick as party_v2_tick;
