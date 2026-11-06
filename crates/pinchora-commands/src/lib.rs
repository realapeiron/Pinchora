mod app;
mod auto;
mod card_images;
mod catalog;
mod commands;
mod components;
mod custom;
mod cx;
mod defer;
mod events;
mod game;
mod gate;
mod hooks;
mod house_view;
mod invalidations;
#[cfg(test)]
mod landing_assets;
mod lang;
mod leveling;
mod responder;
mod roles;
mod server_events;
mod server_log;
mod text;
mod util;
mod voice_xp;

pub use app::{App, PreviewError};
pub use card_images::{CardSubject, ServerCard, atlas_has, card_png, classic_card, deliver, preview_png, server_card};
pub use catalog::{command_catalog, site_facts};
pub use commands::{
    collection::{post_bottle, post_critter_drop, spawn_ticker as spawn_collection_ticker, tick as collection_tick},
    customcommand::reserved_names as reserved_command_names,
    definitions as command_definitions,
    games::{spawn_janitor as spawn_game_janitor, sweep as sweep_games},
    level_roles::{SyncOutcome, SyncReport, sync_level_roles, sync_resting},
    modules::{announce_guild_change, module_diff},
    partner::partner_name,
    party_games::{spawn_ticker as spawn_party_ticker, tick as party_tick},
    remind::{spawn_ticker as spawn_reminder_ticker, tick as reminder_tick},
    tempvoice::{
        forget_setup as forget_temp_voice, missing_permissions as temp_voice_gaps, room_members as temp_voice_members,
        spawn_ticker as spawn_tempvoice_ticker, tick as tempvoice_tick,
    },
};
pub use custom::{
    Rendered, Spec, def_diff, escape_markdown, mention_ids, pick as pick_random, render_message, scrub_value, templates,
};
pub use cx::Cx;
pub use defer::Deferral;
pub use events::on_reaction;
pub use events::{
    PermissionChange, on_ban_add, on_ban_remove, on_guild_create, on_guild_delete, on_member_add, on_member_remove,
    on_member_update, on_message, on_permissions_changed, on_voice_state_update, voice_snapshot,
};
pub use hooks::ReactionEvent;
pub use house_view::{
    RoomData, catalog as house_catalog, character_png, render_png as render_room_png, room_name as house_room_name,
    seed_at as house_seed_at,
};
pub use invalidations::listen as listen_for_invalidations;
pub use responder::Responder;
pub use server_events::{spawn_ticker as spawn_events_ticker, tick as events_tick};
pub use text::on_text_command;
pub use util::fill;
pub use voice_xp::{
    VoiceSnapshot, forget as forget_voice_presence, prune as voice_prune, rebuild as rebuild_voice,
    spawn_ticker as spawn_voice_ticker, switched_off as voice_switched_off, switched_on as voice_switched_on,
    tick as voice_tick,
};

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
            let choices = match command.name.as_str() {
                "tag" | "customcommand" => commands::tag::autocomplete(&cx, &command).await,
                _ => commands::modules::autocomplete(&command),
            };
            if let Err(error) = cx.responder.suggest(choices).await {
                tracing::warn!(%error, name = %command.name, "could not answer an autocomplete");
            }
        }
        return;
    }
    // The handler futures are large (every command's state in one enum); on the heap they
    // cannot overflow a worker thread's stack.
    let (label, result) = match data {
        Some(InteractionData::ApplicationCommand(command)) => {
            let label = command.name.clone();
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
