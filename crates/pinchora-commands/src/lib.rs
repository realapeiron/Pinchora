mod app;
mod commands;
mod components;
mod cx;
mod events;
mod game;
mod responder;
mod roles;
mod server_log;
mod util;

pub use app::App;
pub use commands::definitions as command_definitions;
pub use cx::Cx;
pub use events::{
    on_ban_add, on_ban_remove, on_member_add, on_member_remove, on_member_update, on_message, on_voice_state_update,
};
pub use responder::Responder;

use std::{sync::Arc, time::Instant};

use twilight_model::application::interaction::{
    Interaction, InteractionData,
    modal::{ModalInteractionComponent, ModalInteractionData},
};

/// Runs one interaction to completion. Every path answers the user, errors included.
pub async fn handle_interaction(app: Arc<App>, mut interaction: Interaction, responder: Arc<Responder>) {
    let started = Instant::now();
    let data = interaction.data.take();
    let cx = Cx {
        app,
        interaction,
        responder,
    };
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
            let texts = modal_texts(&modal);
            (
                format!("modal:{label}"),
                Box::pin(components::dispatch_modal(&cx, &modal.custom_id, &texts)).await,
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
        let _ = cx
            .reply_ephemeral("Something went wrong on our side. Please try again in a moment.")
            .await;
    }
}

/// The text boxes of a submitted modal as `(custom_id, value)`, whether they sit in labels or rows.
fn modal_texts(modal: &ModalInteractionData) -> Vec<(String, String)> {
    fn collect(component: &ModalInteractionComponent, out: &mut Vec<(String, String)>) {
        match component {
            ModalInteractionComponent::TextInput(input) => out.push((input.custom_id.clone(), input.value.clone())),
            ModalInteractionComponent::Label(label) => collect(&label.component, out),
            ModalInteractionComponent::ActionRow(row) => row.components.iter().for_each(|c| collect(c, out)),
            _ => {}
        }
    }
    let mut texts = Vec::new();
    modal.components.iter().for_each(|c| collect(c, &mut texts));
    texts
}
